//! Public contract: IM09 geometry commands. Each is one transaction and one history entry;
//! a failed command changes nothing; undo restores content and identity.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use burnburn_document::{
    AlignTarget, CommandError, CommandPayload, DocumentCommand, DocumentSession, EditError, EditLabel, GeometryCommand,
    GeometrySource, ImportItem, ObjectKind, UndoLabel,
};
use burnburn_geometry::{
    AlignEdge, BooleanOp, BooleanOptions, DiagnosticTolerance, Direction, FillRule, HandleMode, HandleRef, HandleSide,
    NodeEdit, NodeEditError, NodeRef, OffsetJoin, Path, PathBuilder, Point, ProductionTolerance, Rect, Segment, Shape,
    Spacing,
};
use burnburn_types::{Angle, ContentHash, DocumentId, GeometryId, HashDomain, Length, RequestId, StableId};

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

fn shape(s: &mut DocumentSession, shape: Shape, name: &str) -> GeometryId {
    edit(s, GeometryCommand::CreateShape { shape, name: name.into() }).unwrap().changes.created[0]
}

fn rect(s: &mut DocumentSession, x: f64, y: f64, w: f64, h: f64) -> GeometryId {
    shape(s, Shape::Rectangle(Rect::new(Point::new(x, y), mm(w), mm(h)).unwrap()), "r")
}

fn opts() -> BooleanOptions {
    BooleanOptions { fill: FillRule::EvenOdd, tolerance: ProductionTolerance::new(mm(0.001)).unwrap() }
}

#[test]
fn shapes_become_named_objects_of_their_kind() {
    let mut s = session();
    let c = shape(&mut s, Shape::Circle { center: Point::ORIGIN, radius: mm(5.0) }, "Gat");
    let e =
        shape(&mut s, Shape::Ellipse { center: Point::ORIGIN, rx: mm(5.0), ry: mm(2.0), rotation: Angle::ZERO }, "");
    let p =
        shape(&mut s, Shape::Polygon { center: Point::ORIGIN, radius: mm(5.0), sides: 6, rotation: Angle::ZERO }, "");
    let st = shape(
        &mut s,
        Shape::Star { center: Point::ORIGIN, outer: mm(5.0), inner: mm(2.0), points: 5, rotation: Angle::ZERO },
        "",
    );
    let l = shape(&mut s, Shape::Polyline { points: vec![Point::ORIGIN, Point::new(1.0, 1.0)], closed: false }, "");
    let snap = s.snapshot();
    let kinds: Vec<ObjectKind> = [c, e, p, st, l].iter().map(|id| snap.object(*id).unwrap().kind()).collect();
    assert_eq!(
        kinds,
        vec![ObjectKind::Circle, ObjectKind::Ellipse, ObjectKind::Polygon, ObjectKind::Star, ObjectKind::Polyline]
    );
    assert_eq!(snap.object(c).unwrap().name(), "Gat");
    assert_eq!(s.undo_label(), Some(UndoLabel::Edit(EditLabel::CreateShape)));
    // An invalid shape is rejected and nothing changes.
    let before = s.revision();
    let bad = edit(
        &mut s,
        GeometryCommand::CreateShape {
            shape: Shape::Polygon { center: Point::ORIGIN, radius: mm(5.0), sides: 2, rotation: Angle::ZERO },
            name: String::new(),
        },
    );
    assert!(matches!(bad, Err(CommandError::Edit(EditError::Shape(_)))));
    assert_eq!(s.revision(), before);
}

#[test]
fn node_edit_is_one_undoable_command_that_keeps_identity() {
    let mut s = session();
    let path = PathBuilder::new()
        .move_to(Point::new(0.0, 0.0))
        .cubic_to(Point::new(2.0, 5.0), Point::new(8.0, 5.0), Point::new(10.0, 0.0))
        .cubic_to(Point::new(12.0, 5.0), Point::new(18.0, 5.0), Point::new(20.0, 0.0))
        .build()
        .unwrap();
    let id =
        edit(&mut s, GeometryCommand::CreatePath { path: path.clone(), name: "Logo".into() }).unwrap().changes.created
            [0];
    let out = edit(
        &mut s,
        GeometryCommand::EditNodes {
            target: id,
            edit: NodeEdit::MoveHandle {
                handle: HandleRef { node: NodeRef { subpath: 0, node: 1 }, side: HandleSide::Out },
                to: Point::new(13.0, 9.0),
                mode: HandleMode::Corner,
            },
        },
    )
    .unwrap();
    assert_eq!(out.changes.modified, vec![id]);
    let edited = s.snapshot().object(id).unwrap().path().subpaths()[0].segments().to_vec();
    assert_eq!(edited[0], path.subpaths()[0].segments()[0]);
    assert_eq!(s.undo_label(), Some(UndoLabel::Edit(EditLabel::EditNodes)));
    s.undo(s.revision()).unwrap();
    assert_eq!(s.snapshot().object(id).unwrap().path().as_ref(), &path);
    // A rejected node edit is a typed error.
    let err = edit(
        &mut s,
        GeometryCommand::EditNodes { target: id, edit: NodeEdit::InsertNode { subpath: 0, segment: 0, t: 2.0 } },
    );
    assert_eq!(err, Err(CommandError::Edit(EditError::Node(NodeEditError::InvalidParameter))));
}

#[test]
fn boolean_replaces_sources_by_one_result_and_undo_restores_them() {
    let mut s = session();
    let a = rect(&mut s, 0.0, 0.0, 10.0, 10.0);
    let b = rect(&mut s, 5.0, 5.0, 10.0, 10.0);
    let out = edit(
        &mut s,
        GeometryCommand::Boolean { op: BooleanOp::Union, targets: vec![a, b], options: opts(), keep_sources: false },
    )
    .unwrap();
    assert_eq!(out.changes.deleted, vec![a, b]);
    let r = out.changes.created[0];
    let snap = s.snapshot();
    assert!((snap.object(r).unwrap().path().signed_area() - 175.0).abs() < 1e-6);
    assert_eq!(snap.len(), 1);
    s.undo(s.revision()).unwrap();
    let back = s.snapshot();
    assert!(back.object(a).is_some() && back.object(b).is_some() && back.object(r).is_none());
    // One operand is not a boolean.
    let one = edit(
        &mut s,
        GeometryCommand::Boolean { op: BooleanOp::Union, targets: vec![a], options: opts(), keep_sources: false },
    );
    assert_eq!(one, Err(CommandError::Edit(EditError::NotEnoughTargets)));
}

#[test]
fn collapsing_offset_fails_without_touching_the_document() {
    let mut s = session();
    let a = rect(&mut s, 0.0, 0.0, 10.0, 10.0);
    let rev = s.revision();
    let r = edit(
        &mut s,
        GeometryCommand::Offset {
            targets: vec![a],
            distance: mm(-6.0),
            join: OffsetJoin::Miter,
            tolerance: opts().tolerance,
            keep_sources: true,
        },
    );
    assert!(matches!(r, Err(CommandError::Edit(EditError::Offset(_)))), "{r:?}");
    assert_eq!(s.revision(), rev);
    let ok = edit(
        &mut s,
        GeometryCommand::Offset {
            targets: vec![a],
            distance: mm(1.0),
            join: OffsetJoin::Miter,
            tolerance: opts().tolerance,
            keep_sources: true,
        },
    )
    .unwrap();
    assert!(s.snapshot().object(a).is_some());
    assert!((s.snapshot().object(ok.changes.created[0]).unwrap().path().signed_area() - 144.0).abs() < 1e-6);
}

#[test]
fn trim_extend_and_gap_repair_modify_only_the_target() {
    let mut s = session();
    let line = shape(&mut s, Shape::Line { from: Point::new(0.0, 0.0), to: Point::new(10.0, 0.0) }, "");
    let cut = shape(&mut s, Shape::Line { from: Point::new(5.0, -1.0), to: Point::new(5.0, 1.0) }, "");
    let out = edit(&mut s, GeometryCommand::TrimLine { target: line, pick: Point::new(8.0, 0.0), cutters: vec![cut] })
        .unwrap();
    assert_eq!(out.changes.modified, vec![line]);
    assert_eq!(s.snapshot().object(line).unwrap().path().subpaths()[0].end(), Point::new(5.0, 0.0));
    let wall = shape(&mut s, Shape::Line { from: Point::new(9.0, -1.0), to: Point::new(9.0, 1.0) }, "");
    edit(
        &mut s,
        GeometryCommand::ExtendLine {
            target: line,
            end: burnburn_geometry::EndpointRef::End(0),
            boundaries: vec![wall],
        },
    )
    .unwrap();
    let segs = s.snapshot().object(line).unwrap().path().subpaths()[0].segments().to_vec();
    assert_eq!(segs, vec![Segment::Line { to: Point::new(9.0, 0.0) }]);

    let open = shape(
        &mut s,
        Shape::Polyline {
            points: vec![Point::new(0.0, 0.0), Point::new(4.0, 0.0), Point::new(4.0, 4.0), Point::new(0.0, 0.03)],
            closed: false,
        },
        "",
    );
    let tol = DiagnosticTolerance::new(mm(0.05)).unwrap();
    edit(&mut s, GeometryCommand::CloseGaps { targets: vec![open], tolerance: tol }).unwrap();
    assert!(s.snapshot().object(open).unwrap().path().subpaths()[0].is_closed());
    // Nothing to repair is a typed error, not an empty history entry.
    let none = edit(&mut s, GeometryCommand::CloseGaps { targets: vec![open], tolerance: tol });
    assert_eq!(none, Err(CommandError::Edit(EditError::NothingChanged)));
}

#[test]
fn align_and_distribute_are_one_entry_and_keep_sizes() {
    let mut s = session();
    let a = rect(&mut s, 0.0, 0.0, 10.0, 5.0);
    let b = rect(&mut s, 20.0, 3.0, 4.0, 6.0);
    let c = rect(&mut s, 50.0, -2.0, 10.0, 4.0);
    edit(&mut s, GeometryCommand::Align { targets: vec![a, b, c], edge: AlignEdge::Top, to: AlignTarget::Object(b) })
        .unwrap();
    let snap = s.snapshot();
    for id in [a, b, c] {
        assert_eq!(snap.object(id).unwrap().bounds().unwrap().max().y, 9.0);
    }
    assert_eq!(snap.object(c).unwrap().bounds().unwrap().width(), 10.0);
    edit(
        &mut s,
        GeometryCommand::Distribute {
            targets: vec![a, b, c],
            direction: Direction::Horizontal,
            spacing: Spacing::Centers,
        },
    )
    .unwrap();
    assert_eq!(s.snapshot().object(b).unwrap().bounds().unwrap().center().x, 30.0);
    assert_eq!(s.undo_label(), Some(UndoLabel::Edit(EditLabel::Distribute)));
    let bad =
        edit(&mut s, GeometryCommand::Align { targets: vec![a, b], edge: AlignEdge::Left, to: AlignTarget::Object(c) });
    assert_eq!(bad, Err(CommandError::Edit(EditError::ReferenceNotInTargets(c))));
}

#[test]
fn nested_groups_and_ungroup_restore_membership() {
    let mut s = session();
    let a = rect(&mut s, 0.0, 0.0, 1.0, 1.0);
    let b = rect(&mut s, 2.0, 0.0, 1.0, 1.0);
    let c = rect(&mut s, 4.0, 0.0, 1.0, 1.0);
    let inner = edit(&mut s, GeometryCommand::Group { targets: vec![a, b] }).unwrap();
    let g1 = inner.changes.groups[0];
    let outer = edit(&mut s, GeometryCommand::Group { targets: vec![a, b, c] }).unwrap();
    let g2 = outer.changes.groups[0];
    let snap = s.snapshot();
    assert_eq!(snap.object(a).unwrap().groups(), &[g2, g1]);
    assert_eq!(snap.object(c).unwrap().groups(), &[g2]);
    let mut members = snap.group_members(g2);
    members.sort();
    assert_eq!(members, vec![a, b, c]);
    edit(&mut s, GeometryCommand::Ungroup { group: g2 }).unwrap();
    assert_eq!(s.snapshot().object(a).unwrap().groups(), &[g1]);
    assert!(s.snapshot().object(c).unwrap().groups().is_empty());
    let missing = edit(&mut s, GeometryCommand::Ungroup { group: g2 });
    assert_eq!(missing, Err(CommandError::Edit(EditError::NoSuchGroup(g2))));
    // Group ids come from the document's id space: never reused after undo.
    s.undo(s.revision()).unwrap();
    s.undo(s.revision()).unwrap();
    let again = edit(&mut s, GeometryCommand::Group { targets: vec![a, c] }).unwrap();
    assert_ne!(again.changes.groups[0], g2);
}

#[test]
fn import_creates_objects_that_remember_their_asset_and_entity() {
    let mut s = session();
    let asset = ContentHash::of(HashDomain::Asset, b"<svg/>");
    let items = vec![
        ImportItem {
            name: "path1".into(),
            entity: Some("path1".into()),
            path: Path::circle(Point::ORIGIN, 3.0).unwrap(),
        },
        ImportItem {
            name: "rect2".into(),
            entity: Some("rect2".into()),
            path: Path::line(Point::ORIGIN, Point::new(1.0, 0.0)).unwrap(),
        },
    ];
    let out = edit(&mut s, GeometryCommand::Import { asset, items }).unwrap();
    assert_eq!(out.changes.created.len(), 2);
    let o = s.snapshot();
    let first = o.object(out.changes.created[0]).unwrap();
    assert_eq!(first.source(), &GeometrySource::Imported { asset, entity: Some("path1".into()) });
    assert_eq!(first.kind(), ObjectKind::Path);
    assert_eq!(s.undo_label(), Some(UndoLabel::Edit(EditLabel::Import)));
    assert_eq!(
        edit(&mut s, GeometryCommand::Import { asset, items: vec![] }),
        Err(CommandError::Edit(EditError::NothingChanged))
    );
}
