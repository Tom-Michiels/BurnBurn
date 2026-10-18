//! W1-L2a vertical slice without presentation: pointer input → editor commit → document command
//! → undo. Presentation only has to translate `EditCommit::Ops` into `GeometryCommand::Ops`.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers and let-else in tests

use std::time::Duration;

use burnburn_document::{
    CommandPayload, DocumentCommand, DocumentSession, EditLabel, GeometryCommand, OpsCommand, UndoLabel,
};
use burnburn_editor::{
    CanvasInput, EditCommit, Editor, NodeCommand, PointerEvent, PointerPhase, Selection, SelectionChange, SnapSettings,
    ToolContext, ToolKind, ToolStep,
};
use burnburn_geometry::{Path, Point, Segment, Shape};
use burnburn_types::{DocumentId, GeometryId, Length, MonotonicTime, RequestId, StableId};

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

struct World {
    session: DocumentSession,
    selection: Selection,
    editor: Editor,
    snap: SnapSettings,
}

impl World {
    fn new(shapes: Vec<Shape>) -> (World, Vec<GeometryId>) {
        let mut session = DocumentSession::new(DocumentId::from_raw(1));
        let mut ids = Vec::new();
        for shape in shapes {
            let cmd = DocumentCommand {
                document: session.id(),
                expected: session.revision(),
                request: RequestId::from_raw(1),
                payload: CommandPayload::Edit(GeometryCommand::CreateShape { shape, name: "s".into() }),
            };
            ids.push(session.apply(cmd).unwrap().changes.created[0]);
        }
        let snap = SnapSettings { objects: false, ..SnapSettings::default() };
        (World { session, selection: Selection::default(), editor: Editor::new(), snap }, ids)
    }

    fn select(&mut self, ids: &[GeometryId]) {
        self.selection.apply(&SelectionChange::Replace(ids.to_vec()));
    }

    fn ctx<'a>(&'a self, snapshot: &'a burnburn_document::DocumentSnapshot) -> ToolContext<'a> {
        ToolContext { document: snapshot, selection: &self.selection, mm_per_point: 0.5, snap: &self.snap }
    }

    fn pointer(&mut self, phase: PointerPhase, x: f64, y: f64) -> ToolStep {
        self.pointer_with(phase, x, y, false)
    }

    fn pointer_with(&mut self, phase: PointerPhase, x: f64, y: f64, shift: bool) -> ToolStep {
        let mut ev = PointerEvent::mouse(phase, p(x, y), MonotonicTime::from_offset(Duration::from_millis(1)));
        ev.modifiers.shift = shift;
        let snapshot = self.session.snapshot();
        let ctx = ToolContext { document: &snapshot, selection: &self.selection, mm_per_point: 0.5, snap: &self.snap };
        self.editor.input(&CanvasInput::Pointer(ev), &ctx)
    }

    fn click(&mut self, x: f64, y: f64) -> ToolStep {
        let down = self.pointer(PointerPhase::Down, x, y);
        self.pointer(PointerPhase::Up, x, y);
        down
    }

    fn shift_click(&mut self, x: f64, y: f64) {
        self.pointer_with(PointerPhase::Down, x, y, true);
        self.pointer_with(PointerPhase::Up, x, y, true);
    }

    fn command(&mut self, cmd: NodeCommand) -> ToolStep {
        let snapshot = self.session.snapshot();
        let ctx = ToolContext { document: &snapshot, selection: &self.selection, mm_per_point: 0.5, snap: &self.snap };
        self.editor.node_command(cmd, &ctx)
    }

    /// What presentation does with an Ops commit: one document command.
    fn commit(&mut self, step: ToolStep) -> burnburn_document::CommandOutcome {
        let ToolStep::Commit(EditCommit::Ops(ops)) = step else { panic!("expected an Ops commit, got {step:?}") };
        let cmd = DocumentCommand {
            document: self.session.id(),
            expected: self.session.revision(),
            request: RequestId::from_raw(2),
            payload: CommandPayload::Edit(GeometryCommand::Ops(ops)),
        };
        self.session.apply(cmd).unwrap()
    }

    fn path(&self, id: GeometryId) -> Path {
        self.session.snapshot().object(id).unwrap().path().as_ref().clone()
    }
}

fn zigzag() -> Shape {
    Shape::Polyline {
        points: vec![p(0.0, 0.0), p(10.0, 10.0), p(20.0, 0.0), p(30.0, 10.0), p(40.0, 0.0)],
        closed: false,
    }
}

#[test]
fn node_edit_mode_selects_nodes_and_breaks_the_path_as_one_undoable_step() {
    let (mut w, ids) = World::new(vec![zigzag()]);
    w.select(&ids);
    w.editor.select_tool(ToolKind::Node);
    w.click(20.0, 0.0);
    assert_eq!(w.editor.node_selection().len(), 1);
    let before = w.path(ids[0]);
    let step = w.command(NodeCommand::BreakAtNodes);
    let out = w.commit(step);
    assert_eq!(out.changes.modified, vec![ids[0]]);
    assert_eq!(w.path(ids[0]).subpaths().len(), 2);
    assert_eq!(w.session.undo_label(), Some(UndoLabel::Edit(EditLabel::EditNodes)));
    w.session.undo(w.session.revision()).unwrap();
    assert_eq!(w.path(ids[0]), before);
}

#[test]
fn the_start_point_tool_rotates_a_closed_contour_through_the_document() {
    let (mut w, ids) = World::new(vec![Shape::Rectangle(
        burnburn_geometry::Rect::new(p(0.0, 0.0), Length::from_mm(10.0).unwrap(), Length::from_mm(10.0).unwrap())
            .unwrap(),
    )]);
    w.select(&ids);
    w.editor.select_tool(ToolKind::StartPoint);
    let step = w.click(10.0, 10.0);
    w.commit(step);
    assert_eq!(w.path(ids[0]).subpaths()[0].start(), p(10.0, 10.0));
}

#[test]
fn the_round_corner_tool_uses_the_editor_radius_that_survives_tool_switches() {
    let (mut w, ids) = World::new(vec![Shape::Rectangle(
        burnburn_geometry::Rect::new(p(0.0, 0.0), Length::from_mm(10.0).unwrap(), Length::from_mm(10.0).unwrap())
            .unwrap(),
    )]);
    w.select(&ids);
    w.editor.set_round_radius(Length::from_mm(3.0).unwrap());
    w.editor.select_tool(ToolKind::Node);
    w.editor.select_tool(ToolKind::RoundCorner);
    let step = w.click(0.0, 0.0);
    w.commit(step);
    let arcs: Vec<f64> = w.path(ids[0]).subpaths()[0]
        .segments()
        .iter()
        .filter_map(|s| match s {
            Segment::Arc(a) => Some(a.semi_diameters().0.length()),
            _ => None,
        })
        .collect();
    assert_eq!(arcs.len(), 1);
    assert!((arcs[0] - 3.0).abs() < 1e-9);
    // Changing the radius while the tool is active applies to the next click.
    w.editor.set_round_radius(Length::from_mm(1.0).unwrap());
    let step = w.click(10.0, 0.0);
    w.commit(step);
    let radii: Vec<f64> = w.path(ids[0]).subpaths()[0]
        .segments()
        .iter()
        .filter_map(|s| match s {
            Segment::Arc(a) => Some(a.semi_diameters().0.length()),
            _ => None,
        })
        .collect();
    assert_eq!(radii.len(), 2);
    assert!(radii.iter().any(|r| (r - 1.0).abs() < 1e-9));
}

#[test]
fn node_commands_outside_node_edit_mode_do_nothing() {
    let (mut w, ids) = World::new(vec![zigzag()]);
    w.select(&ids);
    assert!(w.editor.node_selection().is_empty());
    assert_eq!(w.command(NodeCommand::DeleteNodes), ToolStep::Nothing);
    let snapshot = w.session.snapshot();
    assert!(!w.editor.node_command_enabled(NodeCommand::DeleteNodes, &w.ctx(&snapshot)));
}

#[test]
fn ops_commits_are_plain_document_commands() {
    // The commit carries exactly the command the document accepts; presentation adds nothing.
    let (mut w, ids) = World::new(vec![zigzag()]);
    w.select(&ids);
    w.editor.select_tool(ToolKind::Node);
    w.click(10.0, 10.0);
    w.shift_click(30.0, 10.0);
    assert_eq!(w.editor.node_selection().len(), 2);
    let step = w.command(NodeCommand::DeleteNodes);
    let ToolStep::Commit(EditCommit::Ops(OpsCommand::PathOp { targets, .. })) = &step else { panic!("{step:?}") };
    assert_eq!(targets, &ids);
    w.commit(step);
    assert_eq!(w.path(ids[0]).subpaths()[0].segments().len(), 2);
}
