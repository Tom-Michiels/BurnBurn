//! Public contract: corner cuts, curve trim/extend/break and the sketch construction flag
//! (SolidCraft-look modify tools). Each command is one transaction and one history entry; a
//! refused command changes nothing.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use std::f64::consts::{FRAC_PI_2, PI};

use burnburn_document::sketch::{Sketch, SketchCommand, SketchError, SketchLabel};
use burnburn_document::{
    CommandError, CommandPayload, DocumentCommand, DocumentSession, EditError, EditLabel, GeometryCommand, UndoLabel,
};
use burnburn_geometry::{
    BreakError, CornerCut, CornerError, EllipticalArc, EndpointRef, Path, PathBuilder, Point, Rect, Segment, Shape,
};
use burnburn_types::{Angle, DocumentId, GeometryId, Length, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn session() -> DocumentSession {
    DocumentSession::new(DocumentId::from_raw(1))
}

fn run(s: &mut DocumentSession, payload: CommandPayload) -> Result<burnburn_document::CommandOutcome, CommandError> {
    s.apply(DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(7), payload })
}

fn edit(s: &mut DocumentSession, c: GeometryCommand) -> Result<burnburn_document::CommandOutcome, CommandError> {
    run(s, CommandPayload::Edit(c))
}

fn shape(s: &mut DocumentSession, shape: Shape) -> GeometryId {
    edit(s, GeometryCommand::CreateShape { shape, name: String::new() }).unwrap().changes.created[0]
}

fn square(s: &mut DocumentSession) -> GeometryId {
    shape(s, Shape::Rectangle(Rect::new(Point::ORIGIN, mm(10.0), mm(10.0)).unwrap()))
}

fn path_of(s: &DocumentSession, id: GeometryId) -> Path {
    s.snapshot().object(id).unwrap().path().as_ref().clone()
}

#[test]
fn cutting_a_corner_is_one_undoable_edit_with_the_analytic_area() {
    let mut s = session();
    let sq = square(&mut s);
    let before = s.revision();
    let out = edit(
        &mut s,
        GeometryCommand::CutCorner {
            target: sq,
            pick: Point::new(0.3, 0.3),
            reach: mm(2.0),
            cut: CornerCut::Fillet { radius: 4.0 },
        },
    )
    .unwrap();
    assert_eq!(out.changes.modified, vec![sq]);
    assert_eq!(s.undo_label(), Some(UndoLabel::Edit(EditLabel::CutCorner)));
    let area = path_of(&s, sq).signed_area();
    assert!((area - (100.0 - (4.0 - PI) * 16.0 / 4.0)).abs() < 1e-9, "{area}");
    assert!(s.revision() > before);
    s.undo(s.revision()).unwrap();
    assert!((path_of(&s, sq).signed_area() - 100.0).abs() < 1e-12);
}

#[test]
fn a_refused_corner_cut_changes_nothing() {
    let mut s = session();
    let sq = square(&mut s);
    let before = (s.revision(), path_of(&s, sq));
    let r = edit(
        &mut s,
        GeometryCommand::CutCorner {
            target: sq,
            pick: Point::new(0.0, 0.0),
            reach: mm(1.0),
            cut: CornerCut::Fillet { radius: 40.0 },
        },
    );
    assert!(matches!(r, Err(CommandError::Edit(EditError::Corner(CornerError::TooLarge)))), "{r:?}");
    let r = edit(
        &mut s,
        GeometryCommand::CutCorner {
            target: sq,
            pick: Point::new(5.0, 5.0),
            reach: mm(1.0),
            cut: CornerCut::ChamferEqual { distance: 1.0 },
        },
    );
    assert!(matches!(r, Err(CommandError::Edit(EditError::Corner(CornerError::NoCorner)))), "{r:?}");
    assert_eq!((s.revision(), path_of(&s, sq)), before);
}

#[test]
fn trimming_a_circle_and_a_line_with_curve_aware_trim() {
    let mut s = session();
    let circle = shape(&mut s, Shape::Circle { center: Point::ORIGIN, radius: mm(10.0) });
    let wall = shape(&mut s, Shape::Line { from: Point::new(0.0, -20.0), to: Point::new(0.0, 20.0) });
    let out =
        edit(&mut s, GeometryCommand::TrimAt { target: circle, pick: Point::new(10.0, 0.0), cutters: vec![wall] })
            .unwrap();
    assert_eq!(out.changes.modified, vec![circle]);
    assert_eq!(s.undo_label(), Some(UndoLabel::Edit(EditLabel::Trim)));
    let p = path_of(&s, circle);
    assert!(!p.subpaths()[0].is_closed());
    let b = p.bounds().unwrap();
    assert!((b.min().x + 10.0).abs() < 1e-9 && b.max().x.abs() < 1e-9, "{b:?}");
    // Nothing crosses: typed error, document unchanged.
    let far = shape(&mut s, Shape::Line { from: Point::new(50.0, 0.0), to: Point::new(60.0, 0.0) });
    let before = s.revision();
    let r = edit(&mut s, GeometryCommand::TrimAt { target: circle, pick: Point::new(-10.0, 0.0), cutters: vec![far] });
    assert!(matches!(r, Err(CommandError::Edit(EditError::Trim(_)))), "{r:?}");
    assert_eq!(s.revision(), before);
}

#[test]
fn extending_an_arc_moves_its_end_to_the_boundary() {
    let mut s = session();
    let arc = EllipticalArc::circle(Point::ORIGIN, 10.0, Angle::ZERO, Angle::from_radians(FRAC_PI_2).unwrap()).unwrap();
    let path = PathBuilder::new().move_to(arc.start_point()).arc(arc).build().unwrap();
    let id = edit(&mut s, GeometryCommand::CreatePath { path, name: String::new() }).unwrap().changes.created[0];
    let wall = shape(&mut s, Shape::Line { from: Point::new(-6.0, -20.0), to: Point::new(-6.0, 20.0) });
    edit(&mut s, GeometryCommand::ExtendEnd { target: id, end: EndpointRef::End(0), boundaries: vec![wall] }).unwrap();
    let end = path_of(&s, id).subpaths()[0].end();
    assert!(end.distance(Point::new(-6.0, 8.0)) < 1e-9, "{end:?}");
    assert_eq!(s.undo_label(), Some(UndoLabel::Edit(EditLabel::Extend)));
}

#[test]
fn breaking_splits_a_line_and_refuses_an_open_end() {
    let mut s = session();
    let line = shape(&mut s, Shape::Line { from: Point::ORIGIN, to: Point::new(10.0, 0.0) });
    edit(&mut s, GeometryCommand::BreakAt { target: line, pick: Point::new(4.0, 0.2) }).unwrap();
    let p = path_of(&s, line);
    assert_eq!(p.subpaths().len(), 2);
    assert_eq!(p.subpaths()[0].end(), Point::new(4.0, 0.0));
    assert_eq!(s.undo_label(), Some(UndoLabel::Edit(EditLabel::Break)));
    let other = shape(&mut s, Shape::Line { from: Point::ORIGIN, to: Point::new(10.0, 0.0) });
    let r = edit(&mut s, GeometryCommand::BreakAt { target: other, pick: Point::ORIGIN });
    assert!(matches!(r, Err(CommandError::Edit(EditError::Break(BreakError::AtEnd)))), "{r:?}");
    assert!(matches!(path_of(&s, other).subpaths()[0].segments()[0], Segment::Line { .. }));
}

#[test]
fn the_construction_flag_of_a_sketch_entity_toggles_in_one_undo_step() {
    let mut s = session();
    let mut sk = Sketch::new("S");
    let a = sk.point(0.0, 0.0).unwrap();
    let b = sk.point(10.0, 0.0).unwrap();
    let line = sk.line(a, b).unwrap();
    run(&mut s, CommandPayload::Sketch(SketchCommand::Create { sketch: Box::new(sk) })).unwrap();
    let id = s.snapshot().sketch_layer().sketches().next().unwrap().0;
    run(
        &mut s,
        CommandPayload::Sketch(SketchCommand::SetConstruction { sketch: id, entity: line, construction: true }),
    )
    .unwrap();
    assert!(s.snapshot().sketch(id).unwrap().entity(line).unwrap().construction);
    assert_eq!(s.undo_label(), Some(UndoLabel::Sketch(SketchLabel::SetConstruction)));
    s.undo(s.revision()).unwrap();
    assert!(!s.snapshot().sketch(id).unwrap().entity(line).unwrap().construction);
    // A point cannot be refused for being a point, but an unknown entity is typed.
    let r = run(
        &mut s,
        CommandPayload::Sketch(SketchCommand::SetConstruction {
            sketch: id,
            entity: burnburn_document::sketch::EntityId::from_raw(99),
            construction: true,
        }),
    );
    assert!(matches!(r, Err(CommandError::Sketch(SketchError::UnknownEntity(_)))), "{r:?}");
}
