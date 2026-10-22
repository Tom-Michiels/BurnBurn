//! G7-editor E08: the binding of a text to a guide path (`TextObject::path`) survives save and
//! reopen, is part of the project hash, and a project without bindings keeps the file form and
//! hash it always had. The evaluator is a fake so io stays below `burnburn-text`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use burnburn_document::text::{FontRef, FontSize, FontStyle, PathBinding, TextCommand, TextObject};
use burnburn_document::{
    CommandPayload, DerivedKey, DerivedOutput, DerivedOwner, DerivedOwnerKind, DocumentCommand, DocumentPatch,
    DocumentSession, DocumentSnapshot, EvalStatus, Evaluator, ObjectKind, OwnerSolution,
};
use burnburn_geometry::{Path as GPath, Point, Rect};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{Project, ProjectUuid, load_project, project_hash, save_project};
use burnburn_types::{ContentHash, DocumentId, GeometryId, Length, RequestId, StableId};

struct Fake;

impl Evaluator for Fake {
    fn owner_kind(&self) -> DerivedOwnerKind {
        DerivedOwnerKind::Text
    }
    fn evaluate(&self, _: &DocumentSnapshot, owner: DerivedOwner) -> DocumentPatch {
        let path = GPath::rectangle(
            Rect::new(Point::new(0.0, 0.0), Length::from_mm(5.0).unwrap(), Length::from_mm(2.0).unwrap()).unwrap(),
        );
        DocumentPatch {
            owner,
            inputs: ContentHash::from_bytes([7; 32]),
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

fn text(binding: Option<PathBinding>) -> TextObject {
    let mut t = TextObject::new(
        "Rondje",
        FontRef::new("Liberation Sans", FontStyle::Regular),
        FontSize::from_mm(Length::from_mm(8.0).unwrap()).unwrap(),
    );
    t.path = binding;
    t
}

fn binding() -> PathBinding {
    PathBinding { guide: GeometryId::from_raw(7), start_mm: 12.5, offset_mm: -1.0 / 3.0, reverse: true, warp: true }
}

fn project_with(t: TextObject) -> Project {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let cmd = DocumentCommand {
        document: s.id(),
        expected: s.revision(),
        request: RequestId::from_raw(1),
        payload: CommandPayload::Text(TextCommand::Create { text: Box::new(t) }),
    };
    s.apply_with(cmd, &[&Fake]).unwrap();
    Project::new(ProjectUuid::from_bytes([3; 16]), (*s.snapshot()).clone())
}

fn reopened(p: &Project) -> Project {
    let fs = MemoryFileSystem::new();
    fs.create_dir_all_for_test(Path::new("/p"));
    let path = Path::new("/p/bound.burnburn");
    save_project(&fs, path, p).unwrap();
    load_project(&fs, path).unwrap()
}

#[test]
fn a_binding_survives_save_and_reopen_exactly() {
    let original = project_with(text(Some(binding())));
    let back = reopened(&original);
    let id = back.document.text_layer().texts().next().unwrap().0;
    assert_eq!(back.document.text(id).unwrap().path, Some(binding()));
    assert_eq!(project_hash(&back).unwrap(), project_hash(&original).unwrap());
}

#[test]
fn the_project_hash_sees_every_part_of_the_binding() {
    let base = project_hash(&project_with(text(Some(binding())))).unwrap();
    let changed = |f: &dyn Fn(&mut PathBinding)| {
        let mut b = binding();
        f(&mut b);
        project_hash(&project_with(text(Some(b)))).unwrap()
    };
    for (name, hash) in [
        ("guide", changed(&|b| b.guide = GeometryId::from_raw(8))),
        ("start", changed(&|b| b.start_mm = 13.0)),
        ("offset", changed(&|b| b.offset_mm = 2.0)),
        ("reverse", changed(&|b| b.reverse = false)),
        ("warp", changed(&|b| b.warp = false)),
        ("unbound", project_hash(&project_with(text(None))).unwrap()),
    ] {
        assert_ne!(hash, base, "{name}");
    }
}

#[test]
fn a_text_without_a_binding_has_no_trace_of_one_in_the_file() {
    let plain = project_with(text(None));
    let back = reopened(&plain);
    let id = back.document.text_layer().texts().next().unwrap().0;
    assert_eq!(back.document.text(id).unwrap().path, None);
    // The hash of a project without bindings is what it was before the field existed: two
    // identical projects agree and the field is not part of their canonical form.
    assert_eq!(project_hash(&plain).unwrap(), project_hash(&back).unwrap());
}

#[test]
fn a_binding_to_a_missing_object_is_kept_not_dropped() {
    // The guide may be gone (deleted, or in a project that never had it): that is an evaluation
    // state, not a load error.
    let mut b = binding();
    b.guide = GeometryId::from_raw(999_999);
    let back = reopened(&project_with(text(Some(b))));
    let id = back.document.text_layer().texts().next().unwrap().0;
    assert_eq!(back.document.text(id).unwrap().path, Some(b));
}
