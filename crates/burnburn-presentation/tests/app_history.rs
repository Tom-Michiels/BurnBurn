//! X02 (W3-UI-app): the undo history panel. A click on a step is a series of undo/redo commands:
//! the document ends exactly where repeated Cmd+Z / Cmd+Shift+Z would have put it.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_document::{ArrangeLabel, UndoLabel};
use burnburn_presentation::{
    AppIntent, CommandKey, FieldKey, HeadlessEditorHarness, HistoryViewModel, NudgeDirection, NudgeStep, UiIntent,
};
use burnburn_types::{DocumentId, StableId};

/// A rectangle, then two nudges: three steps (draw, move, move).
fn three_steps() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (30.0, 30.0));
    h.invoke(doc, CommandKey::ToolSelect);
    for _ in 0..2 {
        h.send(UiIntent::App(AppIntent::Nudge {
            document: doc,
            direction: NudgeDirection::Right,
            step: NudgeStep::Normal,
        }));
    }
    (h, doc)
}

fn history(h: &mut HeadlessEditorHarness, doc: DocumentId) -> HistoryViewModel {
    h.controller().history_view(doc).expect("history")
}

fn x_of(h: &HeadlessEditorHarness, doc: DocumentId) -> Option<String> {
    h.document(doc)?.inspector?.field(FieldKey::X).map(|f| f.text.clone())
}

fn jump(h: &mut HeadlessEditorHarness, doc: DocumentId, position: usize) {
    h.send(UiIntent::App(AppIntent::HistoryJump { document: doc, position }));
}

#[test]
fn the_list_has_every_step_oldest_first_with_its_label() {
    let (mut h, doc) = three_steps();
    let view = history(&mut h, doc);
    assert_eq!(view.position, 3);
    assert_eq!(view.rows.len(), 3);
    assert_eq!(view.rows[0].label, UndoLabel::CreateRectangle);
    assert_eq!(view.rows[1].label, UndoLabel::Transform);
    assert!(view.rows.iter().all(|r| r.applied));
}

#[test]
fn a_click_on_an_earlier_step_goes_back_and_the_later_steps_stay_listed_as_undone() {
    let (mut h, doc) = three_steps();
    jump(&mut h, doc, 2);
    let view = history(&mut h, doc);
    assert_eq!(view.position, 2);
    assert_eq!(view.rows.iter().map(|r| r.applied).collect::<Vec<_>>(), vec![true, true, false]);
    assert_eq!(x_of(&h, doc).as_deref(), Some("11"), "one move undone");
    jump(&mut h, doc, 1);
    assert_eq!(x_of(&h, doc).as_deref(), Some("10"), "both moves undone");
    assert_eq!(history(&mut h, doc).rows.len(), 3, "nothing is forgotten by going back");
}

#[test]
fn position_zero_is_the_opened_state_and_a_click_forward_redoes_step_by_step() {
    let (mut h, doc) = three_steps();
    jump(&mut h, doc, 0);
    assert_eq!(history(&mut h, doc).position, 0);
    assert!(h.document(doc).unwrap().objects.is_empty(), "the rectangle is gone");
    jump(&mut h, doc, 3);
    assert_eq!(history(&mut h, doc).position, 3);
    assert_eq!(
        x_of(&h, doc).as_deref(),
        Some("12"),
        "both moves are back; redo selects what it restores, as Cmd+Shift+Z"
    );
    assert_eq!(h.document(doc).unwrap().objects.len(), 1);
}

#[test]
fn a_jump_ends_where_the_same_number_of_undo_commands_would() {
    let (mut by_click, doc_a) = three_steps();
    let (mut by_keys, doc_b) = three_steps();
    jump(&mut by_click, doc_a, 1);
    by_keys.invoke(doc_b, CommandKey::Undo);
    by_keys.invoke(doc_b, CommandKey::Undo);
    let (a, b) = (by_click.document(doc_a).unwrap(), by_keys.document(doc_b).unwrap());
    assert_eq!(a.objects, b.objects);
    assert_eq!((a.undo, a.redo), (b.undo, b.redo));
    assert_eq!(a.dirty, b.dirty);
}

#[test]
fn a_new_edit_after_going_back_replaces_the_undone_steps() {
    let (mut h, doc) = three_steps();
    jump(&mut h, doc, 1);
    h.invoke(doc, CommandKey::SelectAll);
    h.invoke(doc, CommandKey::LockSelected);
    let view = history(&mut h, doc);
    assert_eq!(view.position, 2);
    assert_eq!(view.rows.len(), 2, "redo steps are dropped by a new edit, as with Cmd+Z");
    assert_eq!(view.rows[1].label, UndoLabel::Arrange(ArrangeLabel::Lock));
}

#[test]
fn clicking_the_current_position_or_beyond_the_list_changes_nothing() {
    let (mut h, doc) = three_steps();
    let revision = h.document(doc).unwrap().revision;
    jump(&mut h, doc, 3);
    jump(&mut h, doc, 99);
    assert_eq!(h.document(doc).unwrap().revision, revision, "no history step, no new revision");
    assert_eq!(history(&mut h, doc).position, 3);
}

#[test]
fn a_gesture_in_progress_is_cancelled_by_a_jump_and_never_reaches_history() {
    let (mut h, doc) = three_steps();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.begin_drag(doc, (50.0, 50.0), (70.0, 70.0));
    jump(&mut h, doc, 1);
    assert_eq!(history(&mut h, doc).position, 1);
    assert_eq!(h.document(doc).unwrap().objects.len(), 1, "the half-drawn rectangle did not become an object");
}

#[test]
fn an_unknown_document_has_no_history() {
    let mut h = HeadlessEditorHarness::new();
    assert_eq!(h.controller().history_view(DocumentId::from_raw(99)), None);
}
