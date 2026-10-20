//! Text objects (DR-22) survive save/reopen: source (content, font chain with embedded asset
//! hash, size, layout, placement, mode), evaluation record (also `FontMissing`) and the derived
//! outlines with their keys. Contributed by Generators/tekst; the evaluator is a fake so io stays
//! below `burnburn-text`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use burnburn_document::text::{
    Align, FontRef, FontSize, FontStyle, TextCommand, TextMode, TextObject, VAlign, VariableText,
};
use burnburn_document::{
    CommandPayload, DerivedKey, DerivedOutput, DerivedOwner, DerivedOwnerKind, DocumentCommand, DocumentPatch,
    DocumentSession, DocumentSnapshot, EvalDiagnosis, EvalStatus, Evaluator, ObjectKind, OwnerSolution,
};
use burnburn_geometry::{Affine, Path as GPath, Point, Rect, Vector};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{Project, ProjectUuid, load_project, project_hash, save_project};
use burnburn_types::{ContentHash, DocumentId, HashDomain, Length, RequestId, StableId};

/// Valid while the content does not start with '?'; one rectangle per character as "outline".
struct Fake;

impl Evaluator for Fake {
    fn owner_kind(&self) -> DerivedOwnerKind {
        DerivedOwnerKind::Text
    }
    fn evaluate(&self, snap: &DocumentSnapshot, owner: DerivedOwner) -> DocumentPatch {
        let DerivedOwner::Text(id) = owner else { panic!("only text owners") };
        let t = snap.text(id).unwrap();
        let inputs = ContentHash::from_bytes([9; 32]);
        if t.content.starts_with('?') {
            return DocumentPatch {
                owner,
                inputs,
                outputs: vec![],
                status: EvalStatus::Failed { diagnosis: EvalDiagnosis::FontMissing { font: t.font.clone() } },
                solution: OwnerSolution::Keep,
            };
        }
        let w = t.content.chars().count() as f64 * 1.0 / 3.0;
        let path = GPath::rectangle(
            Rect::new(Point::new(0.1, 0.2), Length::from_mm(w).unwrap(), Length::from_mm(7.0).unwrap()).unwrap(),
        );
        DocumentPatch {
            owner,
            inputs,
            outputs: vec![DerivedOutput {
                key: DerivedKey { owner, local: 0 },
                kind: ObjectKind::Path,
                path,
                name: None,
            }],
            status: EvalStatus::Valid,
            solution: OwnerSolution::Text { missing: vec![] },
        }
    }
}

fn cmd(s: &DocumentSession, c: TextCommand) -> DocumentCommand {
    DocumentCommand {
        document: s.id(),
        expected: s.revision(),
        request: RequestId::from_raw(1),
        payload: CommandPayload::Text(c),
    }
}

fn every_field_text() -> TextObject {
    let font = FontRef::new("Liberation Sans", FontStyle::Bold).with_asset(ContentHash::of(HashDomain::Asset, b"font"));
    let mut t = TextObject::new("Naam: Ålesund\nregel 2", font, FontSize::from_points(13.0).unwrap());
    t.fallback = vec![FontRef::new("BIZ UDPGothic", FontStyle::Regular), FontRef::new("X", FontStyle::BoldItalic)];
    t.layout.align = Align::Right;
    t.layout.line_spacing = 1.0 / 3.0 + 1.0;
    t.layout.letter_spacing_mm = -0.1;
    t.layout.kerning = false;
    t.layout.ligatures = false;
    t.layout.uppercase = true;
    t.layout.valign = VAlign::Bottom;
    t.layout.v_space_mm = 1.0 / 3.0;
    t.layout.welded = true;
    t.variable = Some(VariableText { offset: -3 });
    t.placement = Affine::translate(Vector::new(10.0 / 3.0, -2.5)).unwrap();
    t
}

fn saved_and_reopened(doc: DocumentSnapshot) -> (Project, Project) {
    let fs = MemoryFileSystem::new();
    fs.create_dir_all_for_test(Path::new("/p"));
    let path = Path::new("/p/text.burnburn");
    let original = Project::new(ProjectUuid::from_bytes([4; 16]), doc);
    save_project(&fs, path, &original).unwrap();
    let back = load_project(&fs, path).unwrap();
    (original, back)
}

#[test]
fn text_object_survives_save_and_reopen_with_record_and_outlines() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    s.apply_with(cmd(&s, TextCommand::Create { text: Box::new(every_field_text()) }), &[&Fake]).unwrap();
    let mut single = TextObject::new(
        "AB-12",
        FontRef::new("-", FontStyle::Regular),
        FontSize::from_mm(Length::from_mm(5.0).unwrap()).unwrap(),
    );
    single.mode = TextMode::SingleLine;
    let out = s.apply_with(cmd(&s, TextCommand::Create { text: Box::new(single.clone()) }), &[&Fake]).unwrap();
    // Its font goes missing: last valid outline + FontMissing record must survive the file.
    let failed = out.changes.texts[0];
    single.content = "?weg".into();
    s.apply_with(cmd(&s, TextCommand::Set { text: failed, source: Box::new(single) }), &[&Fake]).unwrap();

    let snap = (*s.snapshot()).clone();
    let (original, back) = saved_and_reopened(snap.clone());
    let doc = &back.document;
    assert_eq!(doc.text_layer(), snap.text_layer(), "sources and records");
    let a: Vec<_> = snap.objects().map(|(_, o)| o.clone()).collect();
    let b: Vec<_> = doc.objects().map(|(_, o)| o.clone()).collect();
    assert_eq!(b, a, "outlines keep id, key and bit-identical geometry");
    assert!(doc.objects().any(|(id, _)| doc.production_blocked(id)), "the failed text stays blocked");
    assert_eq!(project_hash(&back).unwrap(), project_hash(&original).unwrap());
}

#[test]
fn project_hash_covers_text_sources() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let empty = project_hash(&Project::new(ProjectUuid::from_bytes([4; 16]), (*s.snapshot()).clone())).unwrap();
    let out = s.apply(cmd(&s, TextCommand::Create { text: Box::new(every_field_text()) })).unwrap();
    let id = out.changes.texts[0];
    let before = project_hash(&Project::new(ProjectUuid::from_bytes([4; 16]), (*s.snapshot()).clone())).unwrap();
    assert_ne!(before, empty);
    let mut t = every_field_text();
    t.content = "Andere naam".into();
    s.apply(cmd(&s, TextCommand::Set { text: id, source: Box::new(t) })).unwrap();
    let after = project_hash(&Project::new(ProjectUuid::from_bytes([4; 16]), (*s.snapshot()).clone())).unwrap();
    assert_ne!(before, after);
}
