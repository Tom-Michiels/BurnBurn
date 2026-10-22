//! IM31: compact operator view, roles, one machine service per machine, orders with counts and
//! remnant link, and explicit choice between ambiguous candidate jobs. Fake controller and clock.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_test_support::host::rig as common;

use std::path::{Path as FsPath, PathBuf};

use burnburn_machine::{ControllerStatus, Reply, Request};
use burnburn_presentation::{
    CommandKey, Dispatched, FieldKey, HeadlessEditorHarness, HostEffect, MachineAction, MachinePort, MachineStatusKey,
    MessageKey, OrderDraft, Role, UiIntent,
};
use burnburn_test_support::Step;

use common::*;

fn press(r: &mut Rig, kind: MachineAction) {
    let button = r.h.operator_view().machine.expect("machine").button(kind).cloned().expect("control");
    assert!(button.enabled, "{kind:?} disabled");
    r.h.press(None, &button);
    pump(r);
}

/// Prepared and approved by the designer; then the controller is used by an operator.
fn approved_rig() -> (Rig, burnburn_presentation::JobCardViewModel) {
    let mut r = rig();
    prepare(&mut r.h, r.doc);
    r.h.invoke(r.doc, CommandKey::ApproveJob);
    let job = r.h.document(r.doc).unwrap().job.unwrap();
    assert!(job.approved);
    r.h.controller().set_role(Role::Operator);
    (r, job)
}

fn operator_upload_and_store(r: &mut Rig, job: &burnburn_presentation::JobCardViewModel) {
    script_upload_acks(&r.peer, job);
    let row = r.h.operator_view().jobs.into_iter().find(|j| j.plan == job.plan).expect("job row");
    r.h.press(None, row.upload.as_ref().expect("upload control"));
    pump(r);
    r.peer.script(Step::expect(Request::QueryStored).reply(Reply::Stored(true)));
    press(r, MachineAction::ConfirmStored);
}

/// First IM31 test: after a reconnect the operator view shows the uncertain start, offers no
/// start, and a resend of the last click is a no-op.
#[test]
fn reconnected_operator_view_does_not_repeat_last_machine_action() {
    let (mut r, job) = approved_rig();
    operator_upload_and_store(&mut r, &job);
    let start = r.h.operator_view().machine.unwrap().button(MachineAction::Start).cloned().unwrap();
    r.peer.script(Step::expect(Request::Start).reply(Reply::Ack).then_disconnect());
    r.h.press(None, &start);
    r.clock.advance(TIMEOUT * 2);
    pump(&r);
    press(&mut r, MachineAction::Reconnect);

    let view = r.h.operator_view();
    let machine = view.machine.expect("machine");
    assert_eq!(machine.status, MachineStatusKey::Unknown);
    assert_eq!(machine.message, Some(MessageKey::StatusUnknownCheckMachine));
    assert!(!machine.offers(MachineAction::Start));
    assert_eq!(machine.job, Some(job.plan));

    // The host replays its last click after the reconnect (or the operator clicks the stale button).
    r.h.press(None, &start);
    pump(&r);
    let direct = r.service.dispatch(burnburn_presentation::MachineIntent {
        action: start.action,
        machine: burnburn_machine::MachineId::new(MACHINE),
        job: None,
        required_state: start.seen.clone(),
        order: None,
        role: Role::Operator,
    });
    assert!(matches!(direct, Ok(Dispatched::AlreadyHandled(_))));
    assert_eq!(starts_sent(&r.peer), 1);
}

/// Operator: approved jobs only, big status, no editor functions; editing intents are refused.
#[test]
fn operator_sees_only_approved_jobs_and_cannot_edit() {
    let mut r = rig();
    let approved = prepare(&mut r.h, r.doc);
    r.h.invoke(r.doc, CommandKey::ApproveJob);
    let other = r.h.new_document().unwrap();
    design(&mut r.h, other);
    r.h.send(UiIntent::CommitField { document: other, field: FieldKey::Width, text: "60".into() });
    let unapproved = prepare(&mut r.h, other);
    assert_ne!(approved.plan, unapproved.plan);

    r.h.controller().set_role(Role::Operator);
    let view = r.h.operator_view();
    assert_eq!(view.jobs.iter().map(|j| j.plan).collect::<Vec<_>>(), vec![approved.plan]);
    assert_eq!(view.machine.as_ref().map(|m| m.status), Some(MachineStatusKey::Idle));

    let revision = r.h.document(r.doc).unwrap().revision;
    let refused = r.h.send(UiIntent::CommitField { document: r.doc, field: FieldKey::X, text: "50".into() });
    assert!(
        refused
            .effects
            .contains(&HostEffect::ShowError { document: Some(r.doc), message: MessageKey::NotAllowedForRole })
    );
    r.h.drag(r.doc, (10.0, 10.0), (60.0, 60.0));
    r.h.invoke(r.doc, CommandKey::SelectAll);
    r.h.invoke(r.doc, CommandKey::Delete);
    r.h.invoke(r.doc, CommandKey::ApproveJob);
    assert_eq!(r.h.document(r.doc).unwrap().revision, revision, "operator changes nothing");
    assert!(!r.h.view().commands[&CommandKey::Delete].enabled);
    assert!(!r.h.view().commands[&CommandKey::PrepareJob].enabled);

    // Uploading the unapproved job (stale button from the designer window) is refused.
    let upload = unapproved.upload.clone().unwrap();
    let refused = r.h.press(Some(other), &upload);
    assert!(
        refused
            .effects
            .iter()
            .any(|e| matches!(e, HostEffect::ShowError { message: MessageKey::NotAllowedForRole, .. }))
    );
    assert!(r.peer.sent().is_empty());
}

/// One machine service per machine: two windows (controllers) share one connection owner, see the
/// same status and the same control ids; a click in both sends one start.
#[test]
fn two_windows_share_one_machine_service() {
    let mut r = rig();
    let job = prepare(&mut r.h, r.doc);
    let mut second = HeadlessEditorHarness::new();
    second.controller().attach_machine(r.service.clone());
    upload_and_store(&mut r, &job);

    let first_view = r.h.machine_view().unwrap();
    let second_view = second.machine_view().unwrap();
    assert_eq!(first_view, second_view, "one machine, one status");

    r.peer.script(Step::expect(Request::Start).reply(Reply::Ack));
    let start = first_view.button(MachineAction::Start).cloned().unwrap();
    r.h.press(None, &start);
    second.press(None, second_view.button(MachineAction::Start).unwrap());
    pump(&r);
    assert_eq!(starts_sent(&r.peer), 1);
    assert_eq!(second.machine_view().unwrap().status, MachineStatusKey::StartRequested);
}

/// W07: the order count rises only with Completed evidence of an execution for that order;
/// Stored and Unknown do not count. The remnant link is shown with the order.
#[test]
fn order_count_changes_only_by_recorded_execution() {
    let (mut r, job) = approved_rig();
    let name = r.h.document(r.doc).unwrap().title;
    r.h.send(UiIntent::AddOrder {
        order: OrderDraft {
            name: "Klant 17".into(),
            part: name,
            quantity: 2,
            remnant: Some("Rest MDF 3 mm 600×400".into()),
        },
    });
    let order = r.h.operator_view().orders[0].clone();
    assert_eq!((order.quantity, order.completed, order.job), (2, 0, Some(job.plan)));
    assert_eq!(order.remnant.as_deref(), Some("Rest MDF 3 mm 600×400"));

    script_upload_acks(&r.peer, &job);
    r.h.send(UiIntent::UploadForOrder { order: order.id, action: order.upload.clone().unwrap().action, seen: None });
    pump(&r);
    r.peer.script(Step::expect(Request::QueryStored).reply(Reply::Stored(true)));
    press(&mut r, MachineAction::ConfirmStored);
    assert_eq!(r.h.operator_view().orders[0].completed, 0, "stored_upload_does_not_count_completed_piece");

    r.peer.script(Step::expect(Request::Start).reply(Reply::Ack));
    press(&mut r, MachineAction::Start);
    r.peer.script(Step::expect(Request::QueryStatus).reply(Reply::Status(ControllerStatus::Running)));
    press(&mut r, MachineAction::QueryStatus);
    assert_eq!(r.h.operator_view().orders[0].completed, 0, "running is not completed");
    r.peer.script(Step::expect(Request::QueryStatus).reply(Reply::Status(ControllerStatus::Finished)));
    press(&mut r, MachineAction::QueryStatus);
    assert_eq!(r.h.operator_view().orders[0].completed, 1);

    // Second piece: the start reply is lost → Unknown, which does not count.
    let order = r.h.operator_view().orders[0].clone();
    script_upload_acks(&r.peer, &job);
    r.h.send(UiIntent::UploadForOrder {
        order: order.id,
        action: order.upload.clone().unwrap().action,
        seen: order.upload.unwrap().seen,
    });
    pump(&r);
    r.peer.script(Step::expect(Request::QueryStored).reply(Reply::Stored(true)));
    press(&mut r, MachineAction::ConfirmStored);
    r.peer.script(Step::expect(Request::Start).lose_reply(Reply::Ack));
    press(&mut r, MachineAction::Start);
    r.clock.advance(TIMEOUT * 2);
    pump(&r);
    assert_eq!(r.h.operator_view().machine.unwrap().status, MachineStatusKey::Unknown);
    assert_eq!(r.h.operator_view().orders[0].completed, 1, "Unknown is never counted");
}

/// Two approved jobs match one order: no automatic pick; upload waits for an explicit choice.
#[test]
fn two_candidate_jobs_for_an_order_need_an_explicit_choice() {
    let mut r = rig();
    r.h.fs().create_dir_all_for_test(FsPath::new("/a"));
    r.h.fs().create_dir_all_for_test(FsPath::new("/b"));
    let first = r.doc;
    r.h.send(UiIntent::Save { document: first, path: Some(PathBuf::from("/a/plaat.burnburn")) });
    let second = r.h.new_document().unwrap();
    design(&mut r.h, second);
    r.h.send(UiIntent::CommitField { document: second, field: FieldKey::Width, text: "100".into() });
    r.h.send(UiIntent::Save { document: second, path: Some(PathBuf::from("/b/plaat.burnburn")) });
    r.h.run_tasks();
    let jobs = [prepare(&mut r.h, first), prepare(&mut r.h, second)];
    for d in [first, second] {
        r.h.invoke(d, CommandKey::ApproveJob);
    }
    r.h.controller().set_role(Role::Operator);
    r.h.send(UiIntent::AddOrder {
        order: OrderDraft { name: "Klant 18".into(), part: "plaat.burnburn".into(), quantity: 1, remnant: None },
    });
    let order = r.h.operator_view().orders[0].clone();
    assert!(order.needs_choice);
    assert_eq!(order.job, None);
    assert_eq!(order.candidates.len(), 2);
    assert!(order.upload.is_none(), "no upload before the choice");

    r.h.send(UiIntent::ChooseOrderJob { order: order.id, plan: jobs[1].plan });
    let order = r.h.operator_view().orders[0].clone();
    assert_eq!((order.job, order.needs_choice), (Some(jobs[1].plan), false));
    assert!(order.upload.is_some_and(|u| u.enabled));
}

/// IM31 durable: orders and which execution was made for which order live in the history; a
/// restarted controller shows the same order with the same count of completed pieces.
#[test]
fn orders_and_their_counts_survive_a_restart() {
    let (mut r, job) = approved_rig();
    r.h.controller().attach_history(r.history.clone()).unwrap();
    let part = r.h.document(r.doc).unwrap().title;
    let draft = OrderDraft { name: "Klant A".into(), part, quantity: 2, remnant: None };
    r.h.send(UiIntent::AddOrder { order: draft.clone() });
    assert_eq!(r.history.orders.lock().unwrap().orders.len(), 1, "the order is in the history");

    script_upload_acks(&r.peer, &job);
    let order = r.h.operator_view().orders[0].clone();
    let upload = order.upload.clone().expect("upload for order");
    r.h.send(UiIntent::UploadForOrder { order: order.id, action: upload.action, seen: upload.seen.clone() });
    pump(&r);
    r.peer.script(Step::expect(Request::QueryStored).reply(Reply::Stored(true)));
    press(&mut r, MachineAction::ConfirmStored);
    r.peer.script(Step::expect(Request::Start).reply(Reply::Ack));
    press(&mut r, MachineAction::Start);
    r.peer.script(Step::expect(Request::QueryStatus).reply(Reply::Status(ControllerStatus::Running)));
    press(&mut r, MachineAction::QueryStatus);
    r.peer.script(Step::expect(Request::QueryStatus).reply(Reply::Status(ControllerStatus::Finished)));
    press(&mut r, MachineAction::QueryStatus);
    assert_eq!(r.h.operator_view().orders[0].completed, 1);

    let restarted = service(&r.clock, &r.peer, r.history.clone());
    let mut h = HeadlessEditorHarness::new();
    h.controller().set_role(Role::Operator);
    h.controller().attach_history(r.history.clone()).unwrap();
    h.controller().attach_machine(restarted);
    let orders = h.operator_view().orders;
    assert_eq!(orders.len(), 1);
    assert_eq!((orders[0].name.as_str(), orders[0].quantity, orders[0].completed), ("Klant A", 2, 1));
}
