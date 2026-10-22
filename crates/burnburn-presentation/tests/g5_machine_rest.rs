//! G5-machine-rest: the machine activity an update/quit gate reads, and the memory notice of the
//! undo history. Through the public controller route; fake controller and clock, no real laser.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_test_support::host::rig as common;

use burnburn_document::HistoryBudget;
use burnburn_machine::ExecutionState;
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, MessageKey};

use common::*;

#[test]
fn an_idle_machine_has_no_activity_and_a_stored_job_is_reported() {
    let mut r = rig();
    assert_eq!(r.h.controller().machine_activity().executions, vec![], "nothing happened yet");
    assert!(!r.h.controller().machine_activity().live_control);

    let job = prepare(&mut r.h, r.doc);
    upload_and_store(&mut r, &job);
    let activity = r.h.controller().machine_activity();
    assert_eq!(activity.executions, vec![ExecutionState::Stored], "the stored job is in the controller");
}

#[test]
fn without_a_machine_there_is_no_activity() {
    let mut h = HeadlessEditorHarness::new();
    h.new_document().unwrap();
    assert_eq!(h.controller().machine_activity(), burnburn_presentation::MachineActivity::default());
}

fn draw(h: &mut HeadlessEditorHarness, doc: burnburn_types::DocumentId, at: f64) {
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (at, at), (at + 5.0, at + 5.0));
    h.invoke(doc, CommandKey::ToolSelect);
}

fn trimmed(h: &HeadlessEditorHarness, doc: burnburn_types::DocumentId) -> Option<usize> {
    h.document(doc).unwrap().messages.iter().find_map(|m| match m.key {
        MessageKey::HistoryTrimmed(n) => Some(n),
        _ => None,
    })
}

/// X04 in words: when the oldest steps are forgotten for memory the status line says how many.
#[test]
fn forgotten_undo_steps_are_announced_with_their_number() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    draw(&mut h, doc, 0.0);
    assert_eq!(trimmed(&h, doc), None, "with the default budget nothing is forgotten");

    h.controller().set_history_budget(doc, HistoryBudget { max_bytes: 1 });
    draw(&mut h, doc, 20.0);
    let n = trimmed(&h, doc).expect("a trim notice");
    assert!(n >= 1, "{n}");

    // The next ordinary change that forgets nothing clears the notice again.
    h.controller().set_history_budget(doc, HistoryBudget::DEFAULT);
    draw(&mut h, doc, 40.0);
    assert_eq!(trimmed(&h, doc), None);
}
