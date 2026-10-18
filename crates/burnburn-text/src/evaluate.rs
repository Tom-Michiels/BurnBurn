//! Text as a document evaluator (contract §11, DR-22): the text source in the snapshot →
//! outlines per line as a `DocumentPatch`.

use burnburn_document::{
    DerivedKey, DerivedOutput, DerivedOwner, DerivedOwnerKind, DocumentPatch, DocumentSnapshot, EvalDiagnosis,
    EvalStatus, Evaluator, ObjectKind, OwnerSolution,
};
use burnburn_types::{CanonicalEncode, CanonicalWriter, ContentHash, HashDomain};

use crate::font::{FontError, FontSource};
use crate::model::{TextMode, TextObject};
use crate::shape::{TextError, shape};

/// Bumped whenever the same inputs would give different outlines.
pub const EVALUATOR_VERSION: u64 = 1;

fn source_writer(obj: &TextObject) -> CanonicalWriter {
    let mut w = CanonicalWriter::new();
    w.tag("burnburn-text-inputs");
    w.u64(EVALUATOR_VERSION);
    obj.encode(&mut w);
    w
}

/// Canonical inputs hash: the text source (document canonical form), the asset hash of the font
/// bytes every chain entry resolves to, and [`EVALUATOR_VERSION`]. `None` when a font of the
/// chain cannot be loaded.
pub fn inputs_hash(obj: &TextObject, fonts: &dyn FontSource) -> Option<ContentHash> {
    let mut w = source_writer(obj);
    if obj.mode == TextMode::Outline {
        for r in std::iter::once(&obj.font).chain(obj.fallback.iter()) {
            // The bytes actually used, also for a reference by family name only.
            let data = fonts.load(r).ok()?;
            w.bytes(data.hash().as_bytes());
        }
    }
    let payload = w.finish().ok()?;
    Some(ContentHash::of(HashDomain::EvalText, &payload))
}

/// Inputs hash when a font is missing: the source alone (the record must still say which
/// definition failed).
fn source_hash(obj: &TextObject) -> ContentHash {
    let mut w = source_writer(obj);
    w.tag("fonts-unresolved");
    ContentHash::of(HashDomain::EvalText, &w.finish().unwrap_or_default())
}

/// The `burnburn_document::Evaluator` for `DerivedOwner::Text`, over a [`FontSource`] (project
/// assets first; presentation/runtime supply the catalog). One output per non-empty line, keyed
/// by the line index.
pub struct TextEvaluator<F: FontSource> {
    fonts: F,
}

impl<F: FontSource> TextEvaluator<F> {
    pub fn new(fonts: F) -> Self {
        TextEvaluator { fonts }
    }

    pub fn fonts(&self) -> &F {
        &self.fonts
    }
}

impl<F: FontSource> Evaluator for TextEvaluator<F> {
    fn owner_kind(&self) -> DerivedOwnerKind {
        DerivedOwnerKind::Text
    }

    fn evaluate(&self, snapshot: &DocumentSnapshot, owner: DerivedOwner) -> DocumentPatch {
        let failed = |inputs, diagnosis| DocumentPatch {
            owner,
            inputs,
            outputs: Vec::new(),
            status: EvalStatus::Failed { diagnosis },
            solution: OwnerSolution::Keep,
        };
        let Some(obj) = (match owner {
            DerivedOwner::Text(id) => snapshot.text(id),
            _ => None,
        }) else {
            return failed(ContentHash::of(HashDomain::EvalText, b"unknown-owner"), EvalDiagnosis::InvalidDefinition);
        };
        let shaped = match shape(obj, &self.fonts) {
            Ok(s) => s,
            Err(TextError::Font(FontError::NotFound(font))) => {
                return failed(source_hash(obj), EvalDiagnosis::FontMissing { font });
            }
            Err(_) => return failed(source_hash(obj), EvalDiagnosis::InvalidDefinition),
        };
        let Some(inputs) = inputs_hash(obj, &self.fonts) else {
            return failed(source_hash(obj), EvalDiagnosis::InvalidDefinition);
        };
        let lines = obj.content.split('\n');
        let outputs = shaped
            .line_outlines
            .into_iter()
            .zip(lines)
            .enumerate()
            .filter(|(_, (p, _))| !p.is_empty())
            .map(|(i, (path, line))| DerivedOutput {
                key: DerivedKey { owner, local: i as u64 },
                kind: ObjectKind::Path,
                path,
                name: Some(line.to_owned()),
            })
            .collect();
        DocumentPatch {
            owner,
            inputs,
            outputs,
            status: EvalStatus::Valid,
            solution: OwnerSolution::Text { missing: shaped.missing },
        }
    }
}
