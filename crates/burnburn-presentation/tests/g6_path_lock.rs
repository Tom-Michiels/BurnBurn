//! G6-machine, C06: Shapes ▸ Lock Start Point / Lock Direction act on the selection. They are
//! toggles: checked when every selected shape has the lock, one undo step per press.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_document::process::{PathLock, ProcessLabel};
use burnburn_document::UndoLabel;
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, MessageKey, SelectMode, UiIntent};
use burnburn_types::{DocumentId, GeometryId};

fn window() -> (HeadlessEditorHarness, DocumentId, Vec<GeometryId>) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    for x in [0.0, 30.0] {
        h.invoke(doc, CommandKey::ToolRectangle);
        h.drag(doc, (x, 0.0), (x + 20.0, 10.0));
    }
    h.invoke(doc, CommandKey::ToolSelect);
    let ids = h.document(doc).unwrap().objects.iter().map(|o| o.id).collect();
    (h, doc, ids)
}

fn select(h: &mut HeadlessEditorHarness, doc: DocumentId, ids: &[GeometryId]) {
    h.send(UiIntent::Select { document: doc, ids: ids.to_vec(), mode: SelectMode::Replace });
}

fn state(h: &HeadlessEditorHarness, key: CommandKey) -> (bool, bool) {
    let c = &h.view().commands[&key];
    (c.enabled, c.checked)
}

fn lock_of(h: &mut HeadlessEditorHarness, doc: DocumentId, id: GeometryId) -> PathLock {
    h.controller().path_lock(doc, id).unwrap()
}

#[test]
fn the_commands_need_a_selection() {
    let (mut h, doc, ids) = window();
    select(&mut h, doc, &[]);
    assert_eq!(state(&h, CommandKey::LockPathStart), (false, false));
    assert_eq!(state(&h, CommandKey::LockPathDirection), (false, false));
    select(&mut h, doc, &ids[..1]);
    assert_eq!(state(&h, CommandKey::LockPathStart), (true, false));
    assert_eq!(state(&h, CommandKey::LockPathDirection), (true, false));
}

#[test]
fn pressing_locks_every_selected_shape_in_one_undo_step_and_pressing_again_frees_them() {
    let (mut h, doc, ids) = window();
    select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::LockPathStart);
    for id in &ids {
        assert_eq!(lock_of(&mut h, doc, *id), PathLock { start: true, direction: false });
    }
    assert_eq!(state(&h, CommandKey::LockPathStart), (true, true));
    assert_eq!(state(&h, CommandKey::LockPathDirection), (true, false), "the other lock is a separate switch");
    assert_eq!(
        h.view().commands[&CommandKey::Undo].label,
        MessageKey::UndoAction(UndoLabel::Process(ProcessLabel::SetPathLock)),
        "one press is one undo step"
    );

    h.invoke(doc, CommandKey::LockPathDirection);
    assert_eq!(lock_of(&mut h, doc, ids[0]), PathLock { start: true, direction: true });

    h.invoke(doc, CommandKey::LockPathStart);
    assert_eq!(lock_of(&mut h, doc, ids[0]), PathLock { start: false, direction: true });
    assert_eq!(state(&h, CommandKey::LockPathStart), (true, false));

    // One press, one step: undoing the last press brings the start lock back on both shapes.
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(lock_of(&mut h, doc, ids[0]), PathLock { start: true, direction: true });
    assert_eq!(lock_of(&mut h, doc, ids[1]), PathLock { start: true, direction: true });
}

#[test]
fn a_mixed_selection_is_unchecked_and_pressing_locks_all_of_it() {
    let (mut h, doc, ids) = window();
    select(&mut h, doc, &ids[..1]);
    h.invoke(doc, CommandKey::LockPathStart);
    select(&mut h, doc, &ids);
    assert_eq!(state(&h, CommandKey::LockPathStart), (true, false), "checked means every selected shape has it");
    h.invoke(doc, CommandKey::LockPathStart);
    assert_eq!(lock_of(&mut h, doc, ids[1]), PathLock { start: true, direction: false });
    assert_eq!(state(&h, CommandKey::LockPathStart), (true, true));
}

#[test]
fn shapes_keep_their_other_lock_when_one_is_toggled() {
    let (mut h, doc, ids) = window();
    select(&mut h, doc, &ids[..1]);
    h.invoke(doc, CommandKey::LockPathDirection);
    select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::LockPathStart);
    assert_eq!(lock_of(&mut h, doc, ids[0]), PathLock { start: true, direction: true }, "kept its direction lock");
    assert_eq!(lock_of(&mut h, doc, ids[1]), PathLock { start: true, direction: false });
}
