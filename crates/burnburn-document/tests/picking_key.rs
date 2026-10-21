//! X04: `DocumentSnapshot::picking_key` identifies the content that canvas picking (hit test,
//! snapping) depends on, so an editor-side index can be reused exactly as long as it is valid.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_document::{ArrangeCommand, CommandOutcome, CommandPayload, DocumentCommand, DocumentSession};
use burnburn_geometry::{Affine, Point, Rect, Vector};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId, StableId};

fn run(s: &mut DocumentSession, payload: CommandPayload) -> CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap()
}

fn session_with(n: usize) -> (DocumentSession, Vec<GeometryId>) {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let mut ids = Vec::new();
    for i in 0..n {
        let side = Length::from_mm(5.0).unwrap();
        let rect = Rect::new(Point::new(10.0 * i as f64, 0.0), side, side).unwrap();
        ids.push(run(&mut s, CommandPayload::CreateRectangle { rect, name: format!("r{i}") }).changes.created[0]);
    }
    (s, ids)
}

#[test]
fn the_key_is_stable_for_clones_and_changes_with_every_edit_that_affects_picking() {
    let (mut s, ids) = session_with(3);
    let before = s.snapshot().picking_key();
    assert_eq!(before, s.snapshot().picking_key(), "same snapshot");
    assert_eq!(before, (*s.snapshot()).clone().picking_key(), "a clone shares the content");

    let m = Affine::translate(Vector::new(1.0, 0.0)).unwrap();
    run(&mut s, CommandPayload::Transform { targets: vec![ids[0]], transform: m });
    let moved = s.snapshot().picking_key();
    assert_ne!(before, moved, "geometry changed");

    run(&mut s, CommandPayload::Arrange(ArrangeCommand::SetHidden { targets: vec![ids[1]], hidden: true }));
    let hidden = s.snapshot().picking_key();
    assert_ne!(moved, hidden, "hidden/locked/draw order are part of picking");
}

#[test]
fn an_edit_never_makes_a_held_key_equal_to_the_new_state() {
    let (mut s, ids) = session_with(2);
    let key = s.snapshot().picking_key();
    let m = Affine::translate(Vector::new(0.0, 7.0)).unwrap();
    run(&mut s, CommandPayload::Transform { targets: ids, transform: m });
    assert_ne!(key, s.snapshot().picking_key(), "the moved state is not the held content");
}
