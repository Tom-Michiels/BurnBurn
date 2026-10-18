//! Story (W1-L2b): draw a pentagon with the editor, commit it to the document, change its sides
//! and corner radius afterwards, then convert it to a plain path. Presentation will translate the
//! commit the same way (`EditCommit::CreateParametric` → `ShapeCommand::Create`).

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use std::time::Duration;

use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession, DocumentSnapshot, ShapeCommand};
use burnburn_editor::{
    CanvasInput, EditCommit, Editor, Modifiers, PointerEvent, PointerPhase, Selection, SnapSettings, ToolContext,
    ToolKind, ToolStep,
};
use burnburn_geometry::{Point, ShapeParams};
use burnburn_types::{DocumentId, MonotonicTime, RequestId, StableId};

fn pointer(phase: PointerPhase, x: f64, y: f64) -> CanvasInput {
    CanvasInput::Pointer(PointerEvent {
        modifiers: Modifiers::default(),
        ..PointerEvent::mouse(phase, Point::new(x, y), MonotonicTime::from_offset(Duration::from_millis(1)))
    })
}

fn apply(s: &mut DocumentSession, payload: CommandPayload) -> burnburn_document::CommandOutcome {
    s.apply(DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload })
        .unwrap()
}

#[test]
fn drawn_shape_is_created_edited_and_converted() {
    let mut session = DocumentSession::new(DocumentId::from_raw(1));
    let snapshot: std::sync::Arc<DocumentSnapshot> = session.snapshot();
    let (selection, snap) = (Selection::default(), SnapSettings { objects: false, ..SnapSettings::default() });
    let ctx = ToolContext { document: &snapshot, selection: &selection, mm_per_point: 0.5, snap: &snap };

    let mut editor = Editor::new();
    editor.select_tool(ToolKind::Pentagon);
    editor.input(&pointer(PointerPhase::Down, 10.0, 10.0), &ctx);
    editor.input(&pointer(PointerPhase::Move, 50.0, 45.0), &ctx);
    let ToolStep::Commit(EditCommit::CreateParametric { shape }) =
        editor.input(&pointer(PointerPhase::Up, 50.0, 45.0), &ctx)
    else {
        panic!("a drag must commit one parametric shape")
    };
    let id = apply(&mut session, CommandPayload::Shape(ShapeCommand::Create { shape, name: "Vijfhoek".into() }))
        .changes
        .created[0];
    let before = session.snapshot().object(id).unwrap().clone();
    let b = before.bounds().unwrap();
    assert!((b.min().x - 10.0).abs() < 1e-9 && (b.max().y - 45.0).abs() < 1e-9, "{b:?}");

    // Sides afterwards: five → seven, in place.
    apply(
        &mut session,
        CommandPayload::Shape(ShapeCommand::SetParams { target: id, params: ShapeParams::Polygon { sides: 7 } }),
    );
    let snap_now = session.snapshot();
    let o = snap_now.object(id).unwrap();
    assert_eq!(o.path().subpaths()[0].segments().len() + 1, 7);
    // The placement (circle the corners sit on) stays, so the top corner stays where it was.
    assert_eq!(o.parametric().unwrap().frame(), before.parametric().unwrap().frame());
    assert!(o.path().subpaths()[0].start().distance(before.path().subpaths()[0].start()) < 1e-9);

    // Convert: same outline, no more parameters.
    apply(&mut session, CommandPayload::Shape(ShapeCommand::ConvertToPath { targets: vec![id] }));
    let after = session.snapshot();
    assert!(after.object(id).unwrap().parametric().is_none());
    assert_eq!(after.object(id).unwrap().path(), o.path());
}
