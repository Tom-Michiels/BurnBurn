//! G6-machine, C06: a shape can keep its start point and its direction. The lock is document data
//! (undoable in one step for any number of shapes, canonical only when set, gone with the shape),
//! and the planner reads it (see the cam tests).

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use burnburn_document::process::{PathLock, ProcessCommand, ProcessError, ProcessLabel};
use burnburn_document::{CommandError, CommandOutcome, CommandPayload, DocumentCommand, DocumentSession, UndoLabel};
use burnburn_geometry::{Point, Rect};
use burnburn_types::{CanonicalEncode, CanonicalWriter, DocumentId, GeometryId, Length, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn session() -> DocumentSession {
    DocumentSession::new(DocumentId::from_raw(1))
}

fn run(s: &mut DocumentSession, payload: CommandPayload) -> Result<CommandOutcome, CommandError> {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd)
}

fn rectangle(s: &mut DocumentSession, x: f64) -> GeometryId {
    let rect = Rect::new(Point::new(x, 0.0), mm(10.0), mm(10.0)).unwrap();
    run(s, CommandPayload::CreateRectangle { rect, name: String::new() }).unwrap().changes.created[0]
}

fn process_bytes(s: &DocumentSession) -> Vec<u8> {
    let mut w = CanonicalWriter::new();
    s.snapshot().process().encode(&mut w);
    w.finish().unwrap()
}

fn lock(s: &mut DocumentSession, geometries: Vec<GeometryId>, lock: PathLock) -> Result<CommandOutcome, CommandError> {
    run(s, CommandPayload::Process(ProcessCommand::SetPathLock { geometries, lock }))
}

const BOTH: PathLock = PathLock { start: true, direction: true };
const START: PathLock = PathLock { start: true, direction: false };

#[test]
fn a_shape_is_unlocked_until_it_is_locked() {
    let mut s = session();
    let g = rectangle(&mut s, 0.0);
    assert_eq!(s.snapshot().process().path_lock(g), PathLock::default());
    lock(&mut s, vec![g], START).unwrap();
    assert_eq!(s.snapshot().process().path_lock(g), START);
    assert_eq!(s.snapshot().process().path_locks().collect::<Vec<_>>(), vec![(g, START)]);
}

#[test]
fn one_command_locks_any_number_of_shapes_and_undo_reverses_all_of_them_at_once() {
    let mut s = session();
    let (a, b) = (rectangle(&mut s, 0.0), rectangle(&mut s, 20.0));
    lock(&mut s, vec![a, b], BOTH).unwrap();
    assert_eq!(s.undo_label(), Some(UndoLabel::Process(ProcessLabel::SetPathLock)));
    assert_eq!(s.snapshot().process().path_locks().count(), 2);
    s.undo(s.revision()).unwrap();
    assert_eq!(s.snapshot().process().path_locks().count(), 0, "one step for the whole selection");
    s.redo(s.revision()).unwrap();
    assert_eq!(s.snapshot().process().path_lock(b), BOTH);
}

#[test]
fn the_default_lock_removes_the_entry_and_the_process_layer_is_empty_again() {
    let mut s = session();
    let g = rectangle(&mut s, 0.0);
    lock(&mut s, vec![g], BOTH).unwrap();
    assert!(!s.snapshot().process().is_empty());
    lock(&mut s, vec![g], PathLock::default()).unwrap();
    assert!(s.snapshot().process().is_empty(), "nothing locked is nothing stored");
}

#[test]
fn locks_change_the_canonical_process_layer_only_when_there_are_any() {
    let mut s = session();
    let g = rectangle(&mut s, 0.0);
    let before = process_bytes(&s);
    lock(&mut s, vec![g], PathLock::default()).unwrap();
    assert_eq!(process_bytes(&s), before, "a project without locks keeps its hash");
    lock(&mut s, vec![g], START).unwrap();
    let start = process_bytes(&s);
    assert_ne!(start, before);
    lock(&mut s, vec![g], BOTH).unwrap();
    assert_ne!(process_bytes(&s), start, "start and direction are separate facts");
}

#[test]
fn an_unknown_shape_is_a_typed_error_and_changes_nothing() {
    let mut s = session();
    let g = rectangle(&mut s, 0.0);
    let revision = s.revision();
    let err = lock(&mut s, vec![g, GeometryId::from_raw(99)], BOTH).unwrap_err();
    assert_eq!(err, CommandError::Process(ProcessError::UnknownGeometry(GeometryId::from_raw(99))));
    assert_eq!(s.revision(), revision);
    assert_eq!(s.snapshot().process().path_lock(g), PathLock::default(), "the known shape was not locked either");
}

#[test]
fn deleting_a_shape_drops_its_lock_and_undo_brings_it_back() {
    let mut s = session();
    let g = rectangle(&mut s, 0.0);
    lock(&mut s, vec![g], BOTH).unwrap();
    run(&mut s, CommandPayload::Delete { targets: vec![g] }).unwrap();
    assert_eq!(s.snapshot().process().path_locks().count(), 0);
    s.undo(s.revision()).unwrap();
    assert_eq!(s.snapshot().process().path_lock(g), BOTH);
}
