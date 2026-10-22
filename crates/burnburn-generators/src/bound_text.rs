//! Text on a path as a live binding (G7-editor, E08): the evaluator of `DerivedOwner::Text` that
//! lets a text with a [`burnburn_document::text::PathBinding`] follow its guide object.

use burnburn_document::text::{PathBinding, TextMode, TextObject};
use burnburn_document::{
    DerivedKey, DerivedOutput, DerivedOwner, DerivedOwnerKind, DocumentPatch, DocumentSnapshot, EvalDiagnosis,
    EvalStatus, Evaluator, ObjectKind, OwnerSolution,
};
use burnburn_geometry::ProductionTolerance;
use burnburn_text::{FontError, FontSource, TextError, TextEvaluator};
use burnburn_types::{CanonicalEncode, CanonicalWriter, ContentHash, HashDomain, Length};

use crate::error::GenError;
use crate::path_text::{Distort, PathTextParams, text_on_path};

/// Bumped whenever the same inputs would give different bound outlines.
pub const BOUND_TEXT_VERSION: u64 = 1;

/// Chord tolerance of the bent outlines (mm), like the text-on-path dialog.
const TOLERANCE_MM: f64 = 0.01;

/// The text evaluator of the application: plain text is shaped as ever (`TextEvaluator`); a text
/// bound to a guide is bent along the guide's first outline (`text_on_path`). Both give the
/// outlines as a `DocumentPatch`; a bound text whose guide is missing, empty or too short for it
/// fails with the last valid outlines kept and production blocked.
pub struct BoundTextEvaluator<F: FontSource> {
    inner: TextEvaluator<F>,
}

impl<F: FontSource> BoundTextEvaluator<F> {
    pub fn new(fonts: F) -> Self {
        BoundTextEvaluator { inner: TextEvaluator::new(fonts) }
    }

    fn bound(
        &self,
        snapshot: &DocumentSnapshot,
        owner: DerivedOwner,
        obj: &TextObject,
        binding: PathBinding,
    ) -> DocumentPatch {
        let fonts = self.inner.fonts();
        let guide = snapshot.object(binding.guide);
        let inputs = bound_inputs(obj, guide.map(|g| g.path().as_ref()), fonts);
        let failed = |diagnosis| DocumentPatch {
            owner,
            inputs,
            outputs: Vec::new(),
            status: EvalStatus::Failed { diagnosis },
            solution: OwnerSolution::Keep,
        };
        let Some(guide) = guide else {
            return failed(EvalDiagnosis::MissingSources { objects: vec![binding.guide] });
        };
        let Ok(tolerance) = Length::from_mm(TOLERANCE_MM).map_err(|_| ()).and_then(|l| ProductionTolerance::new(l).map_err(|_| ()))
        else {
            return failed(EvalDiagnosis::InvalidDefinition);
        };
        let params = PathTextParams {
            distort: if binding.warp { Distort::Warp } else { Distort::Rigid },
            start: binding.start_mm,
            offset: binding.offset_mm,
            reverse: binding.reverse,
            tolerance,
        };
        match text_on_path(obj, fonts, guide.path(), &params) {
            Ok(placed) => DocumentPatch {
                owner,
                inputs,
                outputs: vec![DerivedOutput {
                    key: DerivedKey { owner, local: 0 },
                    kind: ObjectKind::Path,
                    path: placed.path,
                    name: Some(obj.content.lines().next().unwrap_or_default().to_owned()),
                }],
                status: EvalStatus::Valid,
                solution: OwnerSolution::Text { missing: Vec::new() },
            },
            Err(GenError::Text(TextError::Font(FontError::NotFound(font)))) => {
                failed(EvalDiagnosis::FontMissing { font })
            }
            Err(_) => failed(EvalDiagnosis::InvalidDefinition),
        }
    }
}

/// Canonical inputs hash of a bound text: the source (binding included), the guide's geometry and
/// the bytes of the fonts it resolves to.
fn bound_inputs(obj: &TextObject, guide: Option<&burnburn_geometry::Path>, fonts: &dyn FontSource) -> ContentHash {
    let mut w = CanonicalWriter::new();
    w.tag("burnburn-bound-text-inputs");
    w.u64(BOUND_TEXT_VERSION);
    obj.encode(&mut w);
    match guide {
        Some(path) => {
            w.tag("guide");
            path.encode(&mut w);
        }
        None => w.tag("no-guide"),
    }
    if obj.mode == TextMode::Outline {
        for r in std::iter::once(&obj.font).chain(obj.fallback.iter()) {
            match fonts.load(r) {
                Ok(data) => w.bytes(data.hash().as_bytes()),
                Err(_) => w.tag("font-unresolved"),
            }
        }
    }
    ContentHash::of(HashDomain::EvalText, &w.finish().unwrap_or_else(|_| b"non-finite".to_vec()))
}

impl<F: FontSource> Evaluator for BoundTextEvaluator<F> {
    fn owner_kind(&self) -> DerivedOwnerKind {
        DerivedOwnerKind::Text
    }

    fn evaluate(&self, snapshot: &DocumentSnapshot, owner: DerivedOwner) -> DocumentPatch {
        if let DerivedOwner::Text(id) = owner
            && let Some(obj) = snapshot.text(id)
            && let Some(binding) = obj.path
        {
            return self.bound(snapshot, owner, obj, binding);
        }
        self.inner.evaluate(snapshot, owner)
    }
}
