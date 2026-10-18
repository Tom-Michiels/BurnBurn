//! Public contract: document commands, transactions, revisions and undo/redo.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use burnburn_document::{
    Anchor, Axis, CommandError, CommandPayload, DocumentCommand, DocumentSession, GeometrySource, ObjectKind, UndoLabel,
};
use burnburn_geometry::{Affine, Point, Rect, Vector};
use burnburn_types::{DocumentId, Length, RequestId, RevisionId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn rect(w: f64, h: f64) -> Rect {
    Rect::new(Point::new(10.0, 20.0), mm(w), mm(h)).unwrap()
}

fn cmd(session: &DocumentSession, payload: CommandPayload) -> DocumentCommand {
    DocumentCommand { document: session.id(), expected: session.revision(), request: RequestId::from_raw(1), payload }
}

fn session() -> DocumentSession {
    DocumentSession::new(DocumentId::from_raw(1))
}

#[test]
fn create_rectangle_assigns_stable_geometry_id() {
    let mut s = session();
    let out =
        s.apply(cmd(&s, CommandPayload::CreateRectangle { rect: rect(120.0, 80.0), name: "Plaat".into() })).unwrap();

    assert_eq!(out.previous, RevisionId::INITIAL);
    assert!(out.revision > out.previous);
    assert_eq!(out.changes.created.len(), 1);
    let id = out.changes.created[0];

    let snap = s.snapshot();
    assert_eq!(snap.revision(), out.revision);
    assert_eq!(snap.len(), 1);
    let obj = snap.object(id).unwrap();
    assert_eq!((obj.name(), obj.kind(), obj.source()), ("Plaat", ObjectKind::Rectangle, &GeometrySource::Drawn));
    let b = obj.bounds().unwrap();
    assert_eq!((b.width(), b.height()), (120.0, 80.0));
    assert_eq!(s.undo_label(), Some(UndoLabel::CreateRectangle));

    // Deterministic: a second session issues the same id for the same first command.
    let mut t = session();
    let again =
        t.apply(cmd(&t, CommandPayload::CreateRectangle { rect: rect(1.0, 1.0), name: String::new() })).unwrap();
    assert_eq!(again.changes.created, vec![id]);
}

fn create(s: &mut DocumentSession, w: f64, h: f64) -> burnburn_types::GeometryId {
    let c = cmd(s, CommandPayload::CreateRectangle { rect: rect(w, h), name: String::new() });
    s.apply(c).unwrap().changes.created[0]
}

#[test]
fn resize_then_undo_restores_geometry_and_identity() {
    let mut s = session();
    let id = create(&mut s, 120.0, 80.0);
    let before = s.snapshot();

    let resize =
        cmd(&s, CommandPayload::SetDimension { target: id, axis: Axis::Width, value: mm(150.0), anchor: Anchor::Min });
    let out = s.apply(resize).unwrap();
    assert_eq!(out.changes.modified, vec![id]);
    let b = s.snapshot().object(id).unwrap().bounds().unwrap();
    assert!((b.width() - 150.0).abs() < 1e-9 && (b.height() - 80.0).abs() < 1e-9, "{b:?}");
    assert!((b.min().x - 10.0).abs() < 1e-9, "min anchor stays fixed: {b:?}");
    assert_eq!(s.undo_label(), Some(UndoLabel::SetDimension));

    let undone = s.undo(s.revision()).unwrap();
    assert!(undone.revision > out.revision, "undo yields a new, newer revision");
    assert_eq!(undone.changes.modified, vec![id]);
    let after = s.snapshot();
    assert!(after.same_content(&before), "geometry restored");
    let obj = after.object(id).unwrap();
    assert_eq!(obj.id(), id, "same identity after undo");
    assert_eq!(obj.bounds().unwrap().width(), 120.0);
    assert_eq!(s.redo_label(), Some(UndoLabel::SetDimension));

    let redone = s.redo(s.revision()).unwrap();
    assert!(redone.revision > undone.revision);
    assert_eq!(s.snapshot().object(id).unwrap().bounds().unwrap().width(), b.width());
}

#[test]
fn failed_command_changes_nothing() {
    let mut s = session();
    let a = create(&mut s, 10.0, 10.0);
    let before = s.snapshot();
    let label = s.undo_label();
    let missing = burnburn_types::GeometryId::from_raw(999);

    // The first target is valid; the second is not. Nothing may be applied.
    let err = s.apply(cmd(&s, CommandPayload::Delete { targets: vec![a, missing] })).unwrap_err();
    assert_eq!(err, CommandError::UnknownGeometry(missing));
    let move_both = Affine::translate(Vector::new(5.0, 0.0)).unwrap();
    let err =
        s.apply(cmd(&s, CommandPayload::Transform { targets: vec![a, missing], transform: move_both })).unwrap_err();
    assert_eq!(err, CommandError::UnknownGeometry(missing));
    let zero =
        cmd(&s, CommandPayload::SetDimension { target: a, axis: Axis::Height, value: mm(0.0), anchor: Anchor::Min });
    assert!(matches!(s.apply(zero), Err(CommandError::InvalidValue { .. })));

    assert_eq!(s.revision(), before.revision());
    assert!(s.snapshot().same_content(&before));
    assert_eq!(s.undo_label(), label, "no history entry for a failed command");
    // The id counter did not advance either: the next object gets the next id after `a`.
    let b = create(&mut s, 1.0, 1.0);
    assert_eq!(b.raw(), a.raw() + 1);
}

#[test]
fn stale_expected_revision_is_rejected() {
    let mut s = session();
    let id = create(&mut s, 10.0, 10.0);
    let stale = RevisionId::INITIAL;
    let current = s.revision();
    let mut c = cmd(&s, CommandPayload::Rename { target: id, name: "late".into() });
    c.expected = stale;
    assert_eq!(s.apply(c), Err(CommandError::StaleRevision { expected: stale, actual: current }));
    assert_eq!(s.undo(stale), Err(CommandError::StaleRevision { expected: stale, actual: current }));
    assert_eq!(s.revision(), current);
    assert_eq!(s.snapshot().object(id).unwrap().name(), "");
}

#[test]
fn dirty_follows_saved_content_across_undo() {
    let mut s = session();
    assert!(!s.is_dirty(), "a new empty document is clean");
    let id = create(&mut s, 10.0, 10.0);
    assert!(s.is_dirty());
    s.mark_saved(s.revision());
    assert!(!s.is_dirty());

    s.apply(cmd(&s, CommandPayload::Rename { target: id, name: "x".into() })).unwrap();
    assert!(s.is_dirty());
    s.undo(s.revision()).unwrap();
    assert!(!s.is_dirty(), "undo back to the saved content is clean, despite a new revision");

    // Marking an older revision than the current one cannot be verified: stay dirty.
    let old = s.revision();
    s.redo(s.revision()).unwrap();
    s.mark_saved(old);
    assert!(s.is_dirty());
}

#[test]
fn apply_after_undo_clears_redo_and_never_reuses_ids() {
    let mut s = session();
    let first = create(&mut s, 10.0, 10.0);
    s.undo(s.revision()).unwrap();
    assert!(s.snapshot().is_empty());
    let second = create(&mut s, 10.0, 10.0);
    assert_ne!(first, second, "an undone create's id is not handed out again");
    assert!(!s.can_redo());
    assert_eq!(s.redo(s.revision()), Err(CommandError::NothingToRedo));
}
