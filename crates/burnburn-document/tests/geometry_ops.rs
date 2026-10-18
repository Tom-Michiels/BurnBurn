//! W1-L2a: selection-wide geometry operations as document commands. Each is one transaction and
//! one history entry; a failed command changes nothing; undo restores content and identity.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use burnburn_document::{
    CommandError, CommandPayload, DocumentCommand, DocumentSession, EditError, EditLabel, GeometryCommand, OpsCommand,
    UndoLabel,
};
use burnburn_geometry::{
    BooleanOptions, CutError, DiagnosticTolerance, FillRule, NodeRef, NodeSetOp, Path, PathOp, PathOpError, Point,
    ProductionTolerance, Rect, Segment, Shape, Winding,
};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn session() -> DocumentSession {
    DocumentSession::new(DocumentId::from_raw(1))
}

fn edit(s: &mut DocumentSession, c: GeometryCommand) -> Result<burnburn_document::CommandOutcome, CommandError> {
    let cmd = DocumentCommand {
        document: s.id(),
        expected: s.revision(),
        request: RequestId::from_raw(7),
        payload: CommandPayload::Edit(c),
    };
    s.apply(cmd)
}

fn ops(s: &mut DocumentSession, c: OpsCommand) -> Result<burnburn_document::CommandOutcome, CommandError> {
    edit(s, GeometryCommand::Ops(c))
}

fn shape(s: &mut DocumentSession, shape: Shape, name: &str) -> GeometryId {
    edit(s, GeometryCommand::CreateShape { shape, name: name.into() }).unwrap().changes.created[0]
}

fn rect(s: &mut DocumentSession, x: f64, y: f64, w: f64, h: f64) -> GeometryId {
    shape(s, Shape::Rectangle(Rect::new(Point::new(x, y), mm(w), mm(h)).unwrap()), "r")
}

fn line(s: &mut DocumentSession, a: (f64, f64), b: (f64, f64)) -> GeometryId {
    let points = vec![Point::new(a.0, a.1), Point::new(b.0, b.1)];
    shape(s, Shape::Polyline { points, closed: false }, "l")
}

fn path_of(s: &DocumentSession, id: GeometryId) -> Path {
    s.snapshot().object(id).unwrap().path().as_ref().clone()
}

fn opts() -> BooleanOptions {
    BooleanOptions { fill: FillRule::EvenOdd, tolerance: ProductionTolerance::new(mm(0.001)).unwrap() }
}

#[test]
fn round_corners_is_one_undoable_step_that_keeps_identity() {
    let mut s = session();
    let r = rect(&mut s, 0.0, 0.0, 10.0, 10.0);
    let before = path_of(&s, r);
    let rev = s.revision();
    let out =
        ops(&mut s, OpsCommand::PathOp { targets: vec![r], op: PathOp::RoundCorners { radius: 2.0, nodes: None } })
            .unwrap();
    assert_eq!(out.changes.modified, vec![r]);
    assert!(out.changes.created.is_empty() && out.changes.deleted.is_empty());
    let after = path_of(&s, r);
    assert!(after.subpaths()[0].segments().iter().filter(|g| matches!(g, Segment::Arc(_))).count() == 4);
    assert_eq!(s.undo_label(), Some(UndoLabel::Edit(EditLabel::RoundCorners)));
    assert_ne!(s.revision(), rev);
    s.undo(s.revision()).unwrap();
    assert_eq!(path_of(&s, r), before, "undo restores the exact outline");
}

#[test]
fn a_path_op_on_several_targets_is_one_entry_and_skips_those_it_would_not_change() {
    let mut s = session();
    let a = rect(&mut s, 0.0, 0.0, 10.0, 10.0);
    let b = rect(&mut s, 20.0, 0.0, 10.0, 10.0);
    let circle = shape(&mut s, Shape::Circle { center: Point::ORIGIN, radius: mm(5.0) }, "c");
    // The circle has no straight corner: it is skipped, the rectangles are rounded, in one entry.
    let out = ops(
        &mut s,
        OpsCommand::PathOp { targets: vec![a, b, circle], op: PathOp::RoundCorners { radius: 1.0, nodes: None } },
    )
    .unwrap();
    assert_eq!(out.changes.modified, vec![a, b]);
    s.undo(s.revision()).unwrap();
    assert_eq!(s.undo_label(), Some(UndoLabel::Edit(EditLabel::CreateShape)), "one undo removed the whole operation");
    // Alone, the circle reports why nothing happened.
    let alone = ops(
        &mut s,
        OpsCommand::PathOp { targets: vec![circle], op: PathOp::RoundCorners { radius: 1.0, nodes: None } },
    );
    assert_eq!(
        alone,
        Err(CommandError::Edit(EditError::PathOp(PathOpError::Round(burnburn_geometry::RoundError::NothingToRound))))
    );
    // Direction: the second pass has nothing left to change for the already clockwise shape.
    let cw =
        ops(&mut s, OpsCommand::PathOp { targets: vec![a], op: PathOp::SetWinding { winding: Winding::Clockwise } })
            .unwrap();
    assert_eq!(cw.changes.modified, vec![a]);
    let noop =
        ops(&mut s, OpsCommand::PathOp { targets: vec![a], op: PathOp::SetWinding { winding: Winding::Clockwise } });
    assert_eq!(noop, Err(CommandError::Edit(EditError::NothingChanged)));
    let mixed =
        ops(&mut s, OpsCommand::PathOp { targets: vec![a, b], op: PathOp::SetWinding { winding: Winding::Clockwise } })
            .unwrap();
    assert_eq!(mixed.changes.modified, vec![b], "a was already clockwise and is skipped");
    assert_eq!(s.undo_label(), Some(UndoLabel::Edit(EditLabel::Direction)));
}

#[test]
fn a_failing_target_leaves_every_other_target_untouched() {
    let mut s = session();
    let a = rect(&mut s, 0.0, 0.0, 10.0, 10.0);
    let before = path_of(&s, a);
    let rev = s.revision();
    let nodes = vec![NodeRef { subpath: 0, node: 1 }, NodeRef { subpath: 0, node: 9 }];
    let bad =
        ops(&mut s, OpsCommand::PathOp { targets: vec![a], op: PathOp::NodeSet { op: NodeSetOp::Delete, nodes } });
    assert!(matches!(bad, Err(CommandError::Edit(EditError::PathOp(_)))));
    assert_eq!((s.revision(), path_of(&s, a)), (rev, before));
    let ghost = GeometryId::from_raw(999);
    let unknown = ops(
        &mut s,
        OpsCommand::PathOp { targets: vec![a, ghost], op: PathOp::RoundCorners { radius: 1.0, nodes: None } },
    );
    assert_eq!(unknown, Err(CommandError::Edit(EditError::UnknownGeometry(ghost))));
    assert_eq!(s.revision(), rev);
}

#[test]
fn deleting_a_node_set_through_the_document_is_one_entry() {
    let mut s = session();
    let r = rect(&mut s, 0.0, 0.0, 10.0, 10.0);
    let nodes = vec![NodeRef { subpath: 0, node: 0 }, NodeRef { subpath: 0, node: 2 }];
    ops(&mut s, OpsCommand::PathOp { targets: vec![r], op: PathOp::NodeSet { op: NodeSetOp::Delete, nodes } }).unwrap();
    assert_eq!(path_of(&s, r).subpaths()[0].segments().len(), 1, "two nodes of a square left: a closed digon");
    assert_eq!(s.undo_label(), Some(UndoLabel::Edit(EditLabel::EditNodes)));
}

#[test]
fn optimize_shapes_keeps_identity_and_reports_nothing_to_do() {
    let mut s = session();
    let pts: Vec<Point> = (0..=10).map(|i| Point::new(f64::from(i), 0.0)).collect();
    let l = shape(&mut s, Shape::Polyline { points: pts, closed: false }, "l");
    let tol = ProductionTolerance::new(mm(0.01)).unwrap();
    ops(&mut s, OpsCommand::PathOp { targets: vec![l], op: PathOp::Optimize { tolerance: tol } }).unwrap();
    assert_eq!(path_of(&s, l).subpaths()[0].segments().len(), 1);
    assert_eq!(s.undo_label(), Some(UndoLabel::Edit(EditLabel::OptimizeShapes)));
    let again = ops(&mut s, OpsCommand::PathOp { targets: vec![l], op: PathOp::Optimize { tolerance: tol } });
    assert_eq!(again, Err(CommandError::Edit(EditError::NothingChanged)));
}

#[test]
fn cut_shapes_replaces_the_subject_by_its_pieces_and_undo_restores_both_objects() {
    let mut s = session();
    let subject = rect(&mut s, 0.0, 0.0, 10.0, 10.0);
    let cutter = rect(&mut s, 5.0, -5.0, 10.0, 20.0);
    let (subject_path, cutter_path) = (path_of(&s, subject), path_of(&s, cutter));
    let out =
        ops(&mut s, OpsCommand::CutShapes { subjects: vec![subject], cutter, keep_cutter: false, options: opts() })
            .unwrap();
    assert_eq!(out.changes.created.len(), 2);
    assert_eq!(out.changes.deleted.len(), 2, "subject and cutter are gone");
    let areas: Vec<f64> = out.changes.created.iter().map(|id| path_of(&s, *id).signed_area()).collect();
    assert!((areas.iter().sum::<f64>() - 100.0).abs() < 1e-6, "{areas:?}");
    assert_eq!(s.undo_label(), Some(UndoLabel::Edit(EditLabel::CutShapes)));
    s.undo(s.revision()).unwrap();
    assert_eq!((path_of(&s, subject), path_of(&s, cutter)), (subject_path, cutter_path));
    // Keeping the cutter leaves it alone.
    let kept =
        ops(&mut s, OpsCommand::CutShapes { subjects: vec![subject], cutter, keep_cutter: true, options: opts() })
            .unwrap();
    assert_eq!(kept.changes.deleted, vec![subject]);
}

#[test]
fn cut_shapes_without_a_crossing_or_with_bad_operands_changes_nothing() {
    let mut s = session();
    let far = rect(&mut s, 100.0, 100.0, 5.0, 5.0);
    let cutter = rect(&mut s, 0.0, 0.0, 10.0, 10.0);
    let rev = s.revision();
    assert_eq!(
        ops(&mut s, OpsCommand::CutShapes { subjects: vec![far], cutter, keep_cutter: false, options: opts() }),
        Err(CommandError::Edit(EditError::NothingChanged))
    );
    // The cutter is not its own subject.
    assert_eq!(
        ops(&mut s, OpsCommand::CutShapes { subjects: vec![cutter], cutter, keep_cutter: false, options: opts() }),
        Err(CommandError::Edit(EditError::NotEnoughTargets))
    );
    let open = line(&mut s, (0.0, 0.0), (10.0, 10.0));
    let rev2 = s.revision();
    assert_eq!(
        ops(&mut s, OpsCommand::CutShapes { subjects: vec![far], cutter: open, keep_cutter: false, options: opts() }),
        Err(CommandError::Edit(EditError::Cut(CutError::OpenCutter)))
    );
    assert_eq!(s.revision(), rev2);
    assert_ne!(rev, rev2);
}

#[test]
fn cutting_an_open_line_creates_open_pieces() {
    let mut s = session();
    let l = line(&mut s, (-5.0, 5.0), (15.0, 5.0));
    let cutter = rect(&mut s, 0.0, 0.0, 10.0, 10.0);
    let out =
        ops(&mut s, OpsCommand::CutShapes { subjects: vec![l], cutter, keep_cutter: true, options: opts() }).unwrap();
    assert_eq!(out.changes.created.len(), 3);
    assert!(out.changes.created.iter().all(|id| !path_of(&s, *id).subpaths()[0].is_closed()));
}

#[test]
fn joining_several_lines_replaces_them_by_one_object_and_undo_restores_them() {
    let mut s = session();
    let a = line(&mut s, (0.0, 0.0), (10.0, 0.0));
    let b = line(&mut s, (10.02, 0.0), (10.0, 10.0));
    let tol = DiagnosticTolerance::new(mm(0.1)).unwrap();
    let out = ops(&mut s, OpsCommand::JoinShapes { targets: vec![a, b], tolerance: tol }).unwrap();
    assert_eq!((out.changes.created.len(), out.changes.deleted.len()), (1, 2));
    let joined = path_of(&s, out.changes.created[0]);
    assert_eq!(joined.subpaths().len(), 1);
    assert_eq!(s.undo_label(), Some(UndoLabel::Edit(EditLabel::JoinShapes)));
    s.undo(s.revision()).unwrap();
    assert_eq!(path_of(&s, a).subpaths().len(), 1);
    assert_eq!(path_of(&s, b).subpaths()[0].start(), Point::new(10.02, 0.0));
}

#[test]
fn joining_one_shape_closes_it_in_place_and_far_pieces_are_refused() {
    let mut s = session();
    let open = shape(
        &mut s,
        Shape::Polyline {
            points: vec![Point::new(0.0, 0.0), Point::new(4.0, 0.0), Point::new(4.0, 4.0), Point::new(0.0, 0.03)],
            closed: false,
        },
        "",
    );
    let tol = DiagnosticTolerance::new(mm(0.05)).unwrap();
    let out = ops(&mut s, OpsCommand::JoinShapes { targets: vec![open], tolerance: tol }).unwrap();
    assert_eq!(out.changes.modified, vec![open], "identity kept for a single target");
    assert!(path_of(&s, open).subpaths()[0].is_closed());
    let a = line(&mut s, (0.0, 0.0), (1.0, 0.0));
    let b = line(&mut s, (50.0, 0.0), (51.0, 0.0));
    assert_eq!(
        ops(&mut s, OpsCommand::JoinShapes { targets: vec![a, b], tolerance: tol }),
        Err(CommandError::Edit(EditError::NothingChanged))
    );
}
