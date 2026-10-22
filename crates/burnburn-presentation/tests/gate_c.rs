//! Gate C headless (docs/user-flows/gate-c.md): preview → compile → prepare → upload → start →
//! status, through the real AppController with `MachineIntent`s to a machine service over the
//! real execution actor, a scripted fake controller and a fake clock.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_test_support::host::rig as common;

use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use burnburn_machine::{ExecutionState, Reply, Request};
use burnburn_presentation::{
    CommandKey, FieldKey, MachineAction, MachineStatusKey, MessageKey, StatusMessage, UiIntent,
};
use burnburn_test_support::Step;
use burnburn_types::{ContentHash, HashDomain};

use common::*;

fn press(r: &mut Rig, kind: MachineAction) {
    let button = r.h.machine_view().expect("machine panel").button(kind).cloned();
    let button = button.unwrap_or_else(|| panic!("no {kind:?} control"));
    assert!(button.enabled, "{kind:?} control is disabled");
    r.h.press(None, &button);
    pump(r);
}

fn status(r: &Rig) -> MachineStatusKey {
    r.h.machine_view().expect("machine panel").status
}

/// The whole day-to-day route on a cooperating controller.
#[test]
fn headless_gate_c_prepare_upload_start_completes() {
    let mut r = rig();
    let job = prepare(&mut r.h, r.doc);
    assert!(chunk_count(&job) > 1, "fixture uploads several chunks");
    upload_and_store(&mut r, &job);
    assert_eq!(status(&r), MachineStatusKey::Stored);
    assert!(r.h.machine_view().unwrap().offers(MachineAction::Start));

    r.peer.script(Step::expect(Request::Start).reply(Reply::Ack));
    press(&mut r, MachineAction::Start);
    let vm = r.h.machine_view().unwrap();
    assert_eq!(vm.status, MachineStatusKey::StartRequested, "a start ACK is not proof that the job runs");
    assert_eq!(vm.message, Some(MessageKey::StartRequestedAwaitingStatus));
    assert!(!vm.offers(MachineAction::Start), "start is offered once per execution");

    r.peer.script(Step::expect(Request::QueryStatus).reply(Reply::Status(burnburn_machine::ControllerStatus::Running)));
    press(&mut r, MachineAction::QueryStatus);
    assert_eq!(status(&r), MachineStatusKey::Running);
    r.peer
        .script(Step::expect(Request::QueryStatus).reply(Reply::Status(burnburn_machine::ControllerStatus::Finished)));
    press(&mut r, MachineAction::QueryStatus);
    let vm = r.h.machine_view().unwrap();
    assert_eq!(vm.status, MachineStatusKey::Completed);
    assert_eq!(vm.job, Some(job.plan), "the machine runs the prepared plan");
    assert_eq!(starts_sent(&r.peer), 1);
    r.peer.verify_complete().unwrap();
}

/// M04: every chunk ACKed is "geüpload, nog niet opgeslagen bevestigd", never "Stored".
#[test]
fn upload_ack_is_not_shown_as_stored() {
    let mut r = rig();
    let job = prepare(&mut r.h, r.doc);
    script_upload_acks(&r.peer, &job);
    r.h.press(Some(r.doc), job.upload.as_ref().expect("upload control"));
    pump(&r);
    let vm = r.h.machine_view().expect("machine panel");
    assert_eq!(vm.status, MachineStatusKey::UploadedNotConfirmed);
    assert_eq!(vm.message, Some(MessageKey::UploadedNotStoredConfirmed));
    assert_eq!(vm.progress, Some((chunk_count(&job), chunk_count(&job))));
    assert!(!vm.offers(MachineAction::Start), "no start before stored evidence");
    assert!(vm.offers(MachineAction::ConfirmStored));

    // A forged start (stale host, script) is refused and never reaches the controller.
    let start = burnburn_presentation::ActionId {
        kind: MachineAction::Start,
        subject: burnburn_presentation::ActionSubject::Execution(vm.execution.expect("execution")),
        generation: 0,
    };
    let update = r.h.send(UiIntent::Machine { document: None, action: start, seen: Some(ExecutionState::Stored) });
    assert!(!update.effects.is_empty(), "refusal is reported");
    assert_eq!(starts_sent(&r.peer), 0);
    assert_eq!(status(&r), MachineStatusKey::UploadedNotConfirmed);
}

/// M06/C4: a lost start reply stays Unknown — "status onbekend — controleer machine" — and no
/// control offers a second start.
#[test]
fn headless_gate_c_lost_start_reply_stays_unknown() {
    let mut r = rig();
    let job = prepare(&mut r.h, r.doc);
    upload_and_store(&mut r, &job);
    r.peer.script(Step::expect(Request::Start).lose_reply(Reply::Ack));
    press(&mut r, MachineAction::Start);
    r.clock.advance(TIMEOUT * 2);
    pump(&r);

    let vm = r.h.machine_view().unwrap();
    assert_eq!(vm.status, MachineStatusKey::Unknown);
    assert_eq!(vm.message, Some(MessageKey::StatusUnknownCheckMachine));
    assert!(!vm.offers(MachineAction::Start), "no retry control that starts again");
    assert!(vm.offers(MachineAction::QueryStatus), "reading status is the way forward");

    // Clicking the old start control again (double click, stale window) is a no-op.
    let start = vm.button(MachineAction::Start).cloned().expect("start control shown disabled");
    r.h.press(None, &start);
    pump(&r);
    assert_eq!(starts_sent(&r.peer), 1);
}

/// M06: after a lost start reply and a reconnect, nothing is resent; only a status query follows.
#[test]
fn lost_start_reply_does_not_cause_second_start_after_reconnect() {
    let mut r = rig();
    let job = prepare(&mut r.h, r.doc);
    upload_and_store(&mut r, &job);
    r.peer.script(Step::expect(Request::Start).reply(Reply::Ack).then_disconnect());
    press(&mut r, MachineAction::Start);
    r.clock.advance(TIMEOUT * 2);
    pump(&r);
    let before = r.peer.sent().len();

    press(&mut r, MachineAction::Reconnect);
    assert_eq!(r.peer.sent().len(), before, "a reconnect sends nothing");
    assert_eq!(status(&r), MachineStatusKey::Unknown);
    assert!(!r.h.machine_view().unwrap().offers(MachineAction::Start));

    r.peer.script(Step::expect(Request::QueryStatus).reply(Reply::Status(burnburn_machine::ControllerStatus::Running)));
    press(&mut r, MachineAction::QueryStatus);
    assert_eq!(r.peer.sent()[before..], [Request::QueryStatus]);
    assert_eq!(starts_sent(&r.peer), 1);
    assert_eq!(status(&r), MachineStatusKey::Unknown, "status bits carry no job identity: stays Unknown");
}

/// C4: Cancel after a start sends one software stop; the outcome stays Unknown with an explanation.
#[test]
fn cancel_after_start_is_unknown_with_explanation() {
    let mut r = rig();
    let job = prepare(&mut r.h, r.doc);
    upload_and_store(&mut r, &job);
    r.peer.script(Step::expect(Request::Start).reply(Reply::Ack));
    press(&mut r, MachineAction::Start);
    let cancel = r.h.machine_view().unwrap().button(MachineAction::Cancel).cloned().expect("stop control");
    assert_eq!(cancel.label, MessageKey::MachineAction(MachineAction::Cancel));
    r.peer.script(Step::expect(Request::Stop).reply(Reply::Ack));
    press(&mut r, MachineAction::Cancel);

    let vm = r.h.machine_view().unwrap();
    assert_eq!(vm.status, MachineStatusKey::Unknown);
    assert_eq!(vm.message, Some(MessageKey::StopSentOutcomeUnknown));
    assert!(!vm.offers(MachineAction::Start));
    assert_eq!(r.peer.sent().iter().filter(|q| **q == Request::Stop).count(), 1);
    r.peer.verify_complete().unwrap();
}

/// Preview, RD export and upload all name the same immutable JobPlanId and the same RD bytes.
#[test]
fn preview_export_and_upload_name_the_same_job_plan() {
    let mut r = rig();
    r.h.fs().create_dir_all_for_test(FsPath::new("/work"));
    let job = prepare(&mut r.h, r.doc);
    let plan = r.h.document(r.doc).unwrap().plan.expect("plan");
    assert_eq!((plan.preview.plan, job.plan), (plan.id, plan.id));

    r.h.send(UiIntent::ExportRd { document: r.doc, path: PathBuf::from("/work/plaat.rd") });
    r.h.run_tasks();
    let export = r.h.document(r.doc).unwrap().export.expect("export");
    assert_eq!((export.plan, export.rd_hash), (job.plan, job.rd_hash));

    upload_and_store(&mut r, &job);
    let uploaded = uploaded_bytes(&r.peer);
    assert_eq!(ContentHash::of(HashDomain::Rd, &uploaded), job.rd_hash, "upload sends the exported bytes");
    let file = burnburn_io::fs::FileSystem::read(r.h.fs(), FsPath::new("/work/plaat.rd"), 1 << 24).unwrap();
    assert_eq!(uploaded, file);
    assert_eq!(r.h.machine_view().unwrap().job, Some(plan.id));
}

/// W01: editing after preparing leaves the prepared job exactly as it was; the banner says so,
/// and an upload still sends the frozen bytes.
#[test]
fn editing_project_does_not_modify_prepared_job_revision() {
    let mut r = rig();
    let job = prepare(&mut r.h, r.doc);
    let doc = r.doc;
    r.h.send(UiIntent::CommitField { document: doc, field: FieldKey::X, text: "40".into() });
    let card = r.h.document(doc).unwrap().job.expect("job kept");
    assert_eq!((card.plan, card.rd_hash, card.bytes), (job.plan, job.rd_hash, job.bytes));
    assert!(card.design_changed, "banner: ontwerp gewijzigd sinds job");
    assert!(r.h.document(doc).unwrap().messages.contains(&StatusMessage { key: MessageKey::DesignChangedSinceJob }));

    // A new compile shows a new plan; the job stays the old one until it is prepared again.
    r.h.invoke(doc, CommandKey::Compile);
    r.h.run_tasks();
    let vm = r.h.document(doc).unwrap();
    assert_ne!(vm.plan.as_ref().map(|p| p.id), Some(job.plan));
    assert_eq!(vm.job.as_ref().map(|j| j.plan), Some(job.plan));
    r.h.invoke(doc, CommandKey::Undo);
    assert_eq!(r.h.document(doc).unwrap().job.map(|j| j.rd_hash), Some(job.rd_hash), "undo never touches the job");

    upload_and_store(&mut r, &job);
    assert_eq!(ContentHash::of(HashDomain::Rd, &uploaded_bytes(&r.peer)), job.rd_hash);
}

/// P02: document undo/redo never emits a machine action.
#[test]
fn undo_redo_never_emits_machine_intent() {
    let mut r = rig();
    let job = prepare(&mut r.h, r.doc);
    upload_and_store(&mut r, &job);
    let before = r.peer.sent().len();
    for _ in 0..3 {
        r.h.invoke(r.doc, CommandKey::Undo);
    }
    for _ in 0..3 {
        r.h.invoke(r.doc, CommandKey::Redo);
    }
    pump(&r);
    assert_eq!(r.peer.sent().len(), before);
    assert_eq!(status(&r), MachineStatusKey::Stored);
}

/// ADR 0008: the start intent is recorded before the controller receives it; when it cannot be
/// recorded nothing is sent.
#[test]
fn start_intent_is_recorded_before_the_controller_receives_it() {
    let mut r = rig();
    let job = prepare(&mut r.h, r.doc);
    upload_and_store(&mut r, &job);

    r.history.fail.store(true, Ordering::SeqCst);
    let start = r.h.machine_view().unwrap().button(MachineAction::Start).cloned().expect("start control");
    let update = r.h.press(None, &start);
    assert!(update.effects.iter().any(|e| matches!(
        e,
        burnburn_presentation::HostEffect::ShowError { message: MessageKey::IntentNotRecorded, .. }
    )));
    assert_eq!(starts_sent(&r.peer), 0);
    assert_eq!(status(&r), MachineStatusKey::Stored);

    r.history.fail.store(false, Ordering::SeqCst);
    r.peer.script(Step::expect(Request::Start).reply(Reply::Ack));
    let start = r.h.machine_view().unwrap().button(MachineAction::Start).cloned().expect("start control");
    r.h.press(None, &start);
    pump(&r);
    assert_eq!(starts_sent(&r.peer), 1);
    let records = r.history.records.lock().unwrap();
    let (record, sent_before) = records.iter().find(|(rec, _)| rec.action.kind == MachineAction::Start).unwrap();
    assert_eq!(record.plan, job.plan);
    let start_index = r.peer.sent().iter().position(|q| *q == Request::Start).unwrap();
    assert!(*sent_before <= start_index, "recorded while the start had not been sent");
}

/// ADR 0008 §5: after a restart every unfinished execution is Unknown and only a status query
/// may follow. Needs the durable journal (`burnburn-history`) to rebuild the service state.
#[test]
fn restart_marks_unfinished_execution_unknown_without_resending_start() {
    let mut r = rig();
    let job = prepare(&mut r.h, r.doc);
    upload_and_store(&mut r, &job);
    r.peer.script(Step::expect(Request::Start).reply(Reply::Ack));
    press(&mut r, MachineAction::Start);

    // App restart: a new service over the same history and controller.
    let restarted = service(&r.clock, &r.peer, Arc::clone(&r.history));
    let mut h = burnburn_presentation::HeadlessEditorHarness::new();
    h.controller().attach_machine(restarted);
    let vm = h.machine_view().expect("machine panel");
    assert_eq!(vm.status, MachineStatusKey::Unknown);
    assert!(!vm.offers(MachineAction::Start));
    assert_eq!(vm.job, Some(job.plan));
    assert_eq!(starts_sent(&r.peer), 1);
}

/// Machine identity before any upload: another model is a failure with a reason, no job byte sent.
#[test]
fn identity_mismatch_uploads_nothing() {
    let mut r = rig();
    let job = prepare(&mut r.h, r.doc);
    script_identity(&r.peer, "RDC6442S");
    r.h.press(Some(r.doc), job.upload.as_ref().expect("upload control"));
    pump(&r);
    let vm = r.h.machine_view().unwrap();
    assert_eq!((vm.status, vm.message), (MachineStatusKey::Failed, Some(MessageKey::IdentityMismatch)));
    assert!(uploaded_bytes(&r.peer).is_empty());
    assert!(!vm.offers(MachineAction::Start));
}

/// After a restart the unknown execution is settled only by an explicit operator statement,
/// and only after a status query showed the controller is not running. Nothing is resent.
#[test]
fn restarted_unknown_execution_is_reconciled_by_the_operator_after_a_status_query() {
    let mut r = rig();
    let job = prepare(&mut r.h, r.doc);
    upload_and_store(&mut r, &job);
    r.peer.script(Step::expect(Request::Start).reply(Reply::Ack));
    press(&mut r, MachineAction::Start);
    let before = r.peer.sent().len();

    let restarted = service(&r.clock, &r.peer, Arc::clone(&r.history));
    let mut h = burnburn_presentation::HeadlessEditorHarness::new();
    h.controller().attach_machine(restarted.clone());
    h.controller().set_role(burnburn_presentation::Role::Operator);
    let done = MachineAction::RecordOutcome(burnburn_presentation::Outcome::RanToCompletion);
    let vm = h.machine_view().unwrap();
    assert_eq!(vm.status, MachineStatusKey::Unknown);
    assert!(!vm.offers(done), "no statement before the controller was asked");

    // The controller still runs: no statement yet.
    r.peer.script(Step::expect(Request::QueryStatus).reply(Reply::Status(burnburn_machine::ControllerStatus::Running)));
    let query = h.machine_view().unwrap().button(MachineAction::QueryStatus).cloned().expect("status control");
    h.press(None, &query);
    restarted.poll().unwrap();
    assert_eq!(h.machine_view().unwrap().status, MachineStatusKey::Unknown, "status bits carry no job identity");
    assert!(!h.machine_view().unwrap().offers(done));

    r.peer.script(Step::expect(Request::QueryStatus).reply(Reply::Status(burnburn_machine::ControllerStatus::Idle)));
    let query = h.machine_view().unwrap().button(MachineAction::QueryStatus).cloned().expect("status control");
    h.press(None, &query);
    restarted.poll().unwrap();
    let vm = h.machine_view().unwrap();
    assert!(vm.offers(done), "after an idle status the operator may state the outcome");
    assert!(!vm.offers(MachineAction::Start));
    let statement = vm.button(done).cloned().unwrap();
    h.press(None, &statement);
    assert_eq!(h.machine_view().unwrap().status, MachineStatusKey::Completed);
    let outcomes = r.history.outcomes.lock().unwrap().clone();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(outcomes[0].role, burnburn_presentation::Role::Operator);
    assert_eq!(r.peer.sent()[before..], [Request::QueryStatus, Request::QueryStatus], "only status queries");
    assert_eq!(starts_sent(&r.peer), 1);

    // The same statement again (double click, second window) records nothing new.
    h.press(None, &statement);
    assert_eq!(r.history.outcomes.lock().unwrap().len(), 1);
}
