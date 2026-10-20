//! W3-UI-schikken: the result of a nest as one document command. Every object gets its own
//! placement (rotation and translation); one transaction, one history entry; a failing move
//! changes nothing.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use burnburn_document::{
    CommandError, CommandPayload, DocumentCommand, DocumentSession, EditError, EditLabel, GeometryCommand,
    PlaceCommand, UndoLabel,
};
use burnburn_geometry::{Affine, Bounds, Point, Rect, Shape, Vector};
use burnburn_types::{Angle, DocumentId, GeometryId, Length, RequestId, StableId};

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

fn rect(s: &mut DocumentSession, x: f64, y: f64, w: f64, h: f64) -> GeometryId {
    let shape = Shape::Rectangle(Rect::new(Point::new(x, y), mm(w), mm(h)).unwrap());
    edit(s, GeometryCommand::CreateShape { shape, name: "r".into() }).unwrap().changes.created[0]
}

fn bounds(s: &DocumentSession, id: GeometryId) -> Bounds {
    s.snapshot().object(id).unwrap().bounds().unwrap()
}

fn nest(moves: Vec<(GeometryId, Affine)>) -> GeometryCommand {
    GeometryCommand::Place(PlaceCommand::Nest { moves })
}

fn shift(dx: f64, dy: f64) -> Affine {
    Affine::translate(Vector::new(dx, dy)).unwrap()
}

fn quarter_turn_then(dx: f64, dy: f64) -> Affine {
    Affine::rotate(Angle::from_degrees(90.0).unwrap()).then(&shift(dx, dy))
}

#[test]
fn every_object_gets_its_own_placement_in_one_undoable_entry() {
    let mut s = session();
    let a = rect(&mut s, 0.0, 0.0, 10.0, 4.0);
    let b = rect(&mut s, 50.0, 50.0, 10.0, 4.0);
    let before = s.revision();
    edit(&mut s, nest(vec![(a, shift(100.0, 0.0)), (b, quarter_turn_then(30.0, 5.0))])).unwrap();
    assert_ne!(s.revision(), before);
    assert_eq!(s.undo_label(), Some(UndoLabel::Edit(EditLabel::Nest)));
    let (ba, bb) = (bounds(&s, a), bounds(&s, b));
    assert_eq!((ba.min(), ba.max()), (Point::new(100.0, 0.0), Point::new(110.0, 4.0)));
    // Rotated by 90° counter-clockwise about the origin: (x, y) → (−y, x); the 10 × 4 box becomes 4 × 10.
    assert!((bb.width() - 4.0).abs() < 1e-9 && (bb.height() - 10.0).abs() < 1e-9, "{bb:?}");
    assert!((bb.min().x - (30.0 - 54.0)).abs() < 1e-9 && (bb.min().y - (5.0 + 50.0)).abs() < 1e-9, "{bb:?}");
    s.undo(s.revision()).unwrap();
    assert_eq!(bounds(&s, a).min(), Point::new(0.0, 0.0));
    assert_eq!(bounds(&s, b).min(), Point::new(50.0, 50.0));
}

#[test]
fn a_move_that_cannot_be_made_changes_nothing() {
    let mut s = session();
    let a = rect(&mut s, 0.0, 0.0, 10.0, 4.0);
    let ghost = GeometryId::from_raw(99);
    let before = s.revision();
    let result = edit(&mut s, nest(vec![(a, shift(5.0, 0.0)), (ghost, shift(1.0, 1.0))]));
    assert!(matches!(result, Err(CommandError::Edit(EditError::UnknownGeometry(g))) if g == ghost), "{result:?}");
    assert_eq!(s.revision(), before);
    assert_eq!(bounds(&s, a).min(), Point::new(0.0, 0.0), "the valid move was rolled back with it");
}

#[test]
fn nothing_to_place_and_the_same_object_twice_are_refused() {
    let mut s = session();
    let a = rect(&mut s, 0.0, 0.0, 10.0, 4.0);
    assert!(matches!(edit(&mut s, nest(vec![])), Err(CommandError::Edit(EditError::NotEnoughTargets))));
    let twice = edit(&mut s, nest(vec![(a, shift(1.0, 0.0)), (a, shift(2.0, 0.0))]));
    assert!(matches!(twice, Err(CommandError::Edit(EditError::DuplicateTarget(g))) if g == a), "{twice:?}");
    assert_eq!(bounds(&s, a).min(), Point::new(0.0, 0.0));
}
