//! W4-UI-gaten, schetsfamilie (S01–S08): gaten uit de storydekking die in de UI bedienbaar zijn.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path as FsPath, PathBuf};

use burnburn_document::sketch::{Sketch, SketchCommand};
use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
use burnburn_editor::Modifiers;
use burnburn_io::{Project, ProjectUuid, save_project};
use burnburn_presentation::{
    CommandKey, DocumentViewModel, HeadlessEditorHarness, HostEffect, MessageKey, SelectMode, SketchConstraintKind,
    ToolRejection, UiIntent,
};
use burnburn_sketch::SketchEvaluator;
use burnburn_sketch::solver::SketchSolverAdapter;
use burnburn_types::{DocumentId, RequestId, StableId};

type H = HeadlessEditorHarness;

fn vm(h: &H, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).expect("document view")
}

fn open_window(h: &mut H) -> DocumentId {
    h.run_tasks()
        .iter()
        .flat_map(|u| u.effects.clone())
        .find_map(|e| match e {
            HostEffect::OpenWindow { document } => Some(document),
            _ => None,
        })
        .expect("opened")
}

/// Opens `sketch` in sketch mode.
fn in_sketch(sketch: Sketch) -> (H, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let mut session = DocumentSession::new(DocumentId::from_raw(77));
    let cmd = DocumentCommand {
        document: session.id(),
        expected: session.revision(),
        request: RequestId::from_raw(1),
        payload: CommandPayload::Sketch(SketchCommand::Create { sketch: Box::new(sketch) }),
    };
    session.apply_with(cmd, &[&SketchEvaluator::new(SketchSolverAdapter)]).expect("solves");
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    let file = PathBuf::from("/work/schets.burnburn");
    save_project(h.fs(), &file, &Project::new(ProjectUuid::from_bytes([5; 16]), (*session.snapshot()).clone()))
        .unwrap();
    h.send(UiIntent::Open { path: file });
    let doc = open_window(&mut h);
    h.send(UiIntent::Select { document: doc, ids: vec![vm(&h, doc).objects[0].id], mode: SelectMode::Replace });
    h.invoke(doc, CommandKey::SketchEdit);
    assert!(vm(&h, doc).sketch_mode.is_some(), "sketch mode is on");
    (h, doc)
}

fn kinds(h: &H, doc: DocumentId) -> Vec<SketchConstraintKind> {
    vm(h, doc).sketch_mode.expect("sketch mode").rows.iter().map(|r| r.kind).collect()
}

// ---------------------------------------------------------------------------------------------
// S08
// ---------------------------------------------------------------------------------------------

/// A line 2 degrees off horizontal is neither clearly horizontal nor clearly sloped: auto-constrain
/// applies nothing at all (not even the clear relation of the other line) and says why.
#[test]
fn ambiguous_autoconstrain_is_not_applied() {
    let mut s = Sketch::new("S");
    let a = s.point(0.0, 0.0).unwrap();
    let b = s.point(40.0, 1.4).unwrap(); // atan(1.4 / 40) = 2.0 degrees
    let c = s.point(0.0, 20.0).unwrap();
    let d = s.point(40.0, 20.0).unwrap(); // clearly horizontal
    s.line(a, b).unwrap();
    s.line(c, d).unwrap();
    let (mut h, doc) = in_sketch(s);
    let revision = vm(&h, doc).revision;
    h.send(UiIntent::SelectTool { document: Some(doc), id: "sketch.auto_constrain".to_owned() });
    h.click(doc, (20.0, 20.0), Modifiers::default());
    let after = vm(&h, doc);
    assert_eq!(after.revision, revision, "the document is unchanged");
    assert!(kinds(&h, doc).is_empty(), "no relation was applied");
    assert!(after.messages.iter().any(|m| m.key == MessageKey::ToolRejected(ToolRejection::AmbiguousRelations)));
}
