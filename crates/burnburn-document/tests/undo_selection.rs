//! G5-UI-gaten P02: the undo record keeps the selection of the moment, so undo and redo hand it
//! back (document contract; the presentation no longer guesses it from `changed`).

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_document::{
    CommandOutcome, CommandPayload, DocumentCommand, DocumentSession, GeometryCommand, ImportItem,
};
use burnburn_geometry::{Path, Point};
use burnburn_types::{ContentHash, DocumentId, GeometryId, RequestId, StableId};

fn run(s: &mut DocumentSession, payload: CommandPayload) -> CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap()
}

fn two_objects() -> (DocumentSession, Vec<GeometryId>) {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let items = (0..2)
        .map(|i| {
            let x = f64::from(i) * 20.0;
            let pts = [Point::new(x, 0.0), Point::new(x + 10.0, 0.0), Point::new(x + 10.0, 10.0), Point::new(x, 10.0)];
            ImportItem { name: format!("o{i}"), entity: None, path: Path::polyline(&pts, true).unwrap() }
        })
        .collect();
    let asset = ContentHash::from_bytes([1; 32]);
    let ids = run(&mut s, CommandPayload::Edit(GeometryCommand::Import { asset, items })).changes.created;
    (s, ids)
}

#[test]
fn undo_returns_the_selection_from_before_and_redo_the_one_after() {
    let (mut s, ids) = two_objects();
    s.set_selection(&[ids[0]]);
    let out = run(&mut s, CommandPayload::Delete { targets: vec![ids[0]] });
    assert!(out.selection.is_none(), "a plain commit reports no restored selection");
    s.set_selection(&[ids[1]]); // the user picked the other object afterwards
    let undone = s.undo(s.revision()).unwrap();
    assert_eq!(undone.selection, Some(vec![ids[0]]));
    let redone = s.redo(s.revision()).unwrap();
    assert_eq!(redone.selection, Some(vec![ids[1]]));
}

#[test]
fn a_selection_set_after_an_undo_does_not_rewrite_the_older_record() {
    let (mut s, ids) = two_objects();
    s.set_selection(&[ids[0]]);
    run(&mut s, CommandPayload::Delete { targets: vec![ids[0]] });
    s.set_selection(&[]);
    s.undo(s.revision()).unwrap();
    s.set_selection(&[ids[1]]); // after the undo: belongs to no record
    let redone = s.redo(s.revision()).unwrap();
    assert_eq!(redone.selection, Some(vec![]), "redo restores what was selected after the delete");
}
