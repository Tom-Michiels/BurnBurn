//! Laser-window actions in the production history: durable before the driver sees them,
//! idempotent per action id, outcomes that may stay unknown, and a diagnostic replay that needs
//! no machine (W1-L4a, M05/M07).
#![allow(clippy::unwrap_used, clippy::panic)]

mod common;

use burnburn_history::{
    ActionId, DiagEvent, DiagnosticOptions, ExecutionStateDto, HistoryError, IntentKind, LiveIntentDto, LiveOutcomeDto,
    LiveUnknownDto, diagnostic_replay,
};
use burnburn_machine::{
    ControllerFileName, ExecutionId, HomeTarget, JogKey, LiveOutcome, LiveRequest, LiveUnknown, MachineId,
};
use common::*;

fn machine() -> MachineId {
    MachineId::new("ruida-bench-1")
}

#[test]
fn a_live_intent_is_durable_and_a_repeated_action_id_is_not_fresh() {
    let fs = Fs::new();
    let (mut h, _) = open(&fs);
    let action = ActionId::from_raw(0x10);
    let first = h.record_live_intent(&fs, machine(), action, &LiveRequest::Home(HomeTarget::Xy), t(1)).unwrap();
    assert!(first.fresh);
    let records = h.records().len();
    let again = h.record_live_intent(&fs, machine(), action, &LiveRequest::Home(HomeTarget::Xy), t(2)).unwrap();
    assert!(!again.fresh, "the same click must not reach the driver twice");
    assert_eq!(again.seq, first.seq);
    assert_eq!(h.records().len(), records, "nothing new is written");

    let (h, report) = open(&fs);
    assert_eq!(report.corrupt, None);
    let v = h.live_action(action).unwrap();
    assert_eq!(v.machine, machine());
    assert_eq!(v.intent, LiveIntentDto::Home { target: "xy".into() });
}

#[test]
fn an_intent_without_an_outcome_is_unknown_after_a_restart_never_acked() {
    let fs = Fs::new();
    let (mut h, _) = open(&fs);
    let action = ActionId::from_raw(0x11);
    h.record_live_intent(&fs, machine(), action, &LiveRequest::JogStart(JogKey::XLeft), t(1)).unwrap();
    assert_eq!(h.live_action(action).unwrap().outcome, None, "pending while the app runs");

    let (h, _) = open(&fs);
    let v = h.live_action(action).unwrap();
    assert_eq!(v.outcome, Some(LiveOutcomeDto::Unknown { why: LiveUnknownDto::InterruptedByRestart }));
    assert!(v.interrupted_by_restart);
}

#[test]
fn outcomes_are_journaled_once_and_survive_a_restart() {
    let fs = Fs::new();
    let (mut h, _) = open(&fs);
    let (a, b, c, d) = (ActionId::from_raw(1), ActionId::from_raw(2), ActionId::from_raw(3), ActionId::from_raw(4));
    for (id, req) in [
        (a, LiveRequest::Pause),
        (b, LiveRequest::MoveAbsolute { x_um: 1_000, y_um: 2_000 }),
        (c, LiveRequest::RunFile(ControllerFileName::new("PLAAT01").unwrap())),
        (d, LiveRequest::ReadPosition),
    ] {
        h.record_live_intent(&fs, machine(), id, &req, t(1)).unwrap();
    }
    h.record_live_outcome(&fs, a, &LiveOutcome::Acked, t(2)).unwrap();
    h.record_live_outcome(&fs, b, &LiveOutcome::Unknown(LiveUnknown::ReplyLost), t(2)).unwrap();
    h.record_live_outcome(&fs, c, &LiveOutcome::NotSent, t(2)).unwrap();
    h.record_live_outcome(&fs, d, &LiveOutcome::Answered, t(2)).unwrap();
    assert_eq!(
        h.record_live_outcome(&fs, a, &LiveOutcome::Refused, t(3)),
        Err(HistoryError::LiveOutcomeAlreadyRecorded(a)),
        "a settled action is not rewritten"
    );

    let (h, _) = open(&fs);
    assert_eq!(h.live_action(a).unwrap().outcome, Some(LiveOutcomeDto::Acked));
    assert_eq!(h.live_action(b).unwrap().outcome, Some(LiveOutcomeDto::Unknown { why: LiveUnknownDto::ReplyLost }));
    assert_eq!(h.live_action(c).unwrap().outcome, Some(LiveOutcomeDto::NotSent));
    assert_eq!(h.live_action(d).unwrap().outcome, Some(LiveOutcomeDto::Answered));
    assert!(!h.live_action(a).unwrap().interrupted_by_restart);
}

#[test]
fn pending_is_not_a_recordable_outcome_and_unknown_actions_are_rejected() {
    let fs = Fs::new();
    let (mut h, _) = open(&fs);
    let a = ActionId::from_raw(7);
    assert_eq!(h.record_live_outcome(&fs, a, &LiveOutcome::Acked, t(1)), Err(HistoryError::UnknownLiveAction(a)));
    h.record_live_intent(&fs, machine(), a, &LiveRequest::Resume, t(1)).unwrap();
    assert!(matches!(h.record_live_outcome(&fs, a, &LiveOutcome::Pending, t(2)), Err(HistoryError::Encode(_))));
}

#[test]
fn live_actions_create_no_execution_and_do_not_touch_the_queue() {
    let fs = Fs::new();
    let (mut h, _) = open(&fs);
    let before = h.next_execution_id();
    h.record_live_intent(&fs, machine(), ActionId::from_raw(9), &LiveRequest::Home(HomeTarget::Xy), t(1)).unwrap();
    assert_eq!(h.next_execution_id(), before);
    assert_eq!(h.executions().count(), 0);
    assert_eq!(h.queue().count(), 0);
}

// ---- M07: diagnostic replay ----

fn history_with_a_job_and_live_traffic() -> (Fs, ActionId) {
    let fs = Fs::new();
    let (mut h, _) = open(&fs);
    let c = compile_session(&plate(), &profile(1));
    let job = record(&mut h, &fs, &c, &profile(1), t(0));
    let id = h.create_execution(&fs, job, machine(), None, profile(1).revision(), t(1)).unwrap();
    h.record_intent(&fs, id, ActionId::from_raw(100), IntentKind::Upload, t(2)).unwrap();
    let live = ActionId::from_raw(101);
    h.record_live_intent(&fs, machine(), live, &LiveRequest::Pause, t(3)).unwrap();
    h.record_live_outcome(&fs, live, &LiveOutcome::Acked, t(4)).unwrap();
    (fs, live)
}

#[test]
fn diagnostic_replay_needs_no_machine_contact() {
    let (fs, live) = history_with_a_job_and_live_traffic();
    // Only a reopened history goes in: there is no port, actor or driver in the signature.
    let (h, _) = open(&fs);
    let report = diagnostic_replay(&h, &DiagnosticOptions::default());

    assert_eq!(report.jobs.len(), 1);
    assert_eq!(report.jobs[0].profile_revision, 1);
    assert_eq!(report.executions.len(), 1);
    assert_eq!(
        report.executions[0].state,
        ExecutionStateDto::Unknown { why: burnburn_history::UnknownReasonDto::InterruptedByRestart }
    );
    let kinds: Vec<_> = report.executions[0].timeline.iter().map(|e| matches!(e, DiagEvent::Intent { .. })).collect();
    assert!(kinds.contains(&true), "the upload intent is in the timeline");
    assert_eq!(report.live_actions.len(), 1);
    assert_eq!(report.live_actions[0].action, live);
    assert_eq!(report.live_actions[0].outcome, Some(LiveOutcomeDto::Acked));
    assert_eq!(report.records, h.records().len() as u64);
    // Replaying twice gives the same report: it reads, it does not act.
    assert_eq!(report, diagnostic_replay(&h, &DiagnosticOptions::default()));
}

#[test]
fn diagnostic_replay_omits_private_data_unless_it_is_ticked() {
    let (fs, _) = history_with_a_job_and_live_traffic();
    let (h, _) = open(&fs);
    let plain = diagnostic_replay(&h, &DiagnosticOptions::default());
    assert_eq!(plain.jobs[0].project_path, None, "project paths are private by default");
    assert_eq!(plain.executions[0].machine, None, "connection data is opt-in");
    assert!(!plain.render_text().contains("ruida-bench-1"));
    assert!(!plain.render_text().contains("/work/plaat.burnburn"));

    let full = diagnostic_replay(&h, &DiagnosticOptions { include_project_paths: true, include_machine_ids: true });
    assert_eq!(full.jobs[0].project_path.as_deref(), Some(std::path::Path::new("/work/plaat.burnburn")));
    assert_eq!(full.executions[0].machine.as_deref(), Some("ruida-bench-1"));
    assert!(full.render_text().contains("ruida-bench-1"));
    let _ = ExecutionId::from_raw(0);
}
