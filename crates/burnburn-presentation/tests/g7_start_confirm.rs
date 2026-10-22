//! G7-werk, M06: a start is confirmed first. The confirmation names the job, its revision, the
//! machine (and what the user declared about its controller), the material and the place on the bed,
//! and says honestly that the start has no hardware proof. Nothing is sent before the user confirms
//! exactly what is on screen; a start that was sent once is never offered again, also not after a
//! lost reply. Real controller, machine service over the real execution actor, scripted fake peer.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod common;

use burnburn_machine::{ExecutionState, JobAnchor, LiveRequest, Reply, Request, StartFrom};
use burnburn_presentation::{
    CamDialog, DialogAction, DialogField, DialogValue, LaserSetup, MachineAction, MachineStatusKey, StartEvidence,
    UiIntent,
};
use burnburn_test_support::Step;
use common::laser::{all_supported, lab};
use common::*;

fn starts_sent(peer: &Peer) -> usize {
    peer.sent().iter().filter(|r| **r == Request::Start).count()
}

/// A stored job, not started: the state in which the Start control is offered.
fn stored() -> (Rig, burnburn_presentation::JobCardViewModel) {
    let mut r = rig();
    let job = prepare(&mut r.h, r.doc);
    upload_and_store(&mut r, &job);
    assert!(r.h.machine_view().unwrap().offers(MachineAction::Start));
    (r, job)
}

fn confirm_of(r: &Rig) -> Option<burnburn_presentation::StartConfirmViewModel> {
    r.h.document(r.doc).unwrap().start_confirm
}

fn request(r: &mut Rig) {
    r.h.send(UiIntent::RequestStart { document: Some(r.doc) });
}

fn confirm(r: &mut Rig) {
    let c = confirm_of(r).expect("a confirmation on screen");
    r.h.send(UiIntent::ConfirmStart { document: Some(r.doc), action: c.action, seen: c.seen });
    pump(r);
}

#[test]
fn asking_to_start_shows_the_confirmation_and_sends_nothing() {
    let (mut r, job) = stored();
    assert!(confirm_of(&r).is_none(), "no confirmation before it is asked for");
    request(&mut r);
    let c = confirm_of(&r).expect("the confirmation");
    assert_eq!(c.plan, job.plan);
    assert_eq!(c.revision, job.plan_revision, "the revision the frozen job was compiled from");
    assert_eq!(c.machine, MACHINE);
    assert_eq!(c.profile_id, "research-1500x900");
    assert_eq!(c.profile_revision, 1);
    assert_eq!(c.material, None, "no plate with a material in this drawing");
    assert_eq!((c.declared_model.as_deref(), c.declared_firmware.as_deref()), (None, None));
    let b = c.bounds.expect("the size of the job");
    assert!((b.width() - 120.0).abs() < 1e-6 && (b.height() - 80.0).abs() < 1e-6, "{b:?}");
    assert_eq!(c.placed_from_mm, None, "not placed on the head");
    assert!(matches!(c.evidence, StartEvidence::NotProven { .. }), "the start has no hardware proof");
    assert_eq!(starts_sent(&r.peer), 0, "asking sends nothing");
    assert_eq!(r.h.machine_view().unwrap().status, MachineStatusKey::Stored);
}

#[test]
fn confirming_sends_exactly_one_start_and_closes_the_confirmation() {
    let (mut r, _) = stored();
    r.peer.script(Step::expect(Request::Start).reply(Reply::Ack));
    request(&mut r);
    confirm(&mut r);
    assert_eq!(starts_sent(&r.peer), 1);
    assert!(confirm_of(&r).is_none());
    // A second confirmation (double click, stale window) is not another start.
    r.h.send(UiIntent::ConfirmStart {
        document: Some(r.doc),
        action: burnburn_presentation::ActionId {
            kind: MachineAction::Start,
            subject: burnburn_presentation::ActionSubject::Execution(r.h.machine_view().unwrap().execution.unwrap()),
            generation: 0,
        },
        seen: Some(ExecutionState::Stored),
    });
    pump(&r);
    assert_eq!(starts_sent(&r.peer), 1);
}

#[test]
fn cancelling_closes_the_confirmation_and_the_start_is_still_offered() {
    let (mut r, _) = stored();
    request(&mut r);
    assert!(confirm_of(&r).is_some());
    r.h.send(UiIntent::CancelStart);
    assert!(confirm_of(&r).is_none());
    assert_eq!(starts_sent(&r.peer), 0);
    assert!(r.h.machine_view().unwrap().offers(MachineAction::Start));
}

#[test]
fn a_confirmation_is_only_offered_for_a_stored_job_that_was_not_started() {
    // Nothing uploaded.
    let mut r = rig();
    prepare(&mut r.h, r.doc);
    request(&mut r);
    assert!(confirm_of(&r).is_none());

    // Started once (reply lost): never offered again.
    let (mut r, _) = stored();
    r.peer.script(Step::expect(Request::Start).lose_reply(Reply::Ack));
    request(&mut r);
    confirm(&mut r);
    r.clock.advance(TIMEOUT * 2);
    pump(&r);
    assert_eq!(r.h.machine_view().unwrap().status, MachineStatusKey::Unknown);
    request(&mut r);
    assert!(confirm_of(&r).is_none(), "a lost reply does not lead to a second start");
    assert_eq!(starts_sent(&r.peer), 1);
}

#[test]
fn a_confirmation_that_is_no_longer_current_sends_nothing() {
    let (mut r, _) = stored();
    request(&mut r);
    let shown = confirm_of(&r).expect("confirmation");
    // The execution is cancelled while the confirmation is open.
    let cancel = r.h.machine_view().unwrap().button(MachineAction::Cancel).cloned().unwrap();
    r.h.press(None, &cancel);
    pump(&r);
    assert!(confirm_of(&r).is_none(), "the confirmation went with the execution it was about");
    r.h.send(UiIntent::ConfirmStart { document: Some(r.doc), action: shown.action, seen: shown.seen });
    pump(&r);
    assert_eq!(starts_sent(&r.peer), 0);
}

#[test]
fn the_confirmation_names_the_declared_model_and_says_when_the_firmware_is_not_declared() {
    let mut r = rig();
    const DEV: CamDialog = CamDialog::Device;
    r.h.send(UiIntent::OpenCamDialog { document: r.doc, dialog: DEV });
    r.h.send(UiIntent::SetCamField {
        document: r.doc,
        dialog: DEV,
        field: DialogField::Model,
        value: DialogValue::Text(MODEL.into()),
    });
    r.h.send(UiIntent::CamDialogAction { document: r.doc, dialog: DEV, action: DialogAction::Apply });
    let job = prepare(&mut r.h, r.doc);
    upload_and_store(&mut r, &job);
    request(&mut r);
    let c = confirm_of(&r).expect("confirmation");
    assert_eq!(c.declared_model.as_deref(), Some(MODEL));
    assert_eq!(c.declared_firmware, None, "what was not declared is not invented");
}

#[test]
fn a_job_placed_on_the_head_says_where() {
    let lab = lab();
    lab.service.set_capabilities(all_supported());
    let mut h = burnburn_presentation::HeadlessEditorHarness::new();
    h.controller().attach_machine(lab.service.clone());
    let doc = h.new_document().unwrap();
    design(&mut h, doc);
    lab.peer.script(
        Step::expect(Request::Live(LiveRequest::ReadPosition))
            .reply(Reply::Ack)
            .reply(Reply::Position { x_um: Some(500_000), y_um: Some(400_000) }),
    );
    let vm = h.controller().laser_view().unwrap();
    let control = vm.control(&burnburn_presentation::LaserControlKind::ShowPosition).unwrap();
    h.send(UiIntent::Laser {
        document: Some(doc),
        action: control.action,
        seen: vm.seen.clone(),
        command: burnburn_presentation::LaserCommand::Action(burnburn_presentation::LaserAction::ShowPosition),
    });
    for _ in 0..8 {
        lab.service.poll().unwrap();
    }
    h.send(UiIntent::LaserSetup { setup: LaserSetup::StartFrom(StartFrom::CurrentPosition) });
    h.send(UiIntent::LaserSetup { setup: LaserSetup::Anchor(JobAnchor::TopLeft) });
    let job = prepare(&mut h, doc);
    let mut r = Rig {
        h,
        doc,
        clock: lab.clock.clone(),
        peer: lab.peer.clone(),
        service: lab.service.clone(),
        history: lab.history.inner.clone(),
    };
    upload_and_store(&mut r, &job);
    request(&mut r);
    let c = confirm_of(&r).expect("confirmation");
    assert_eq!(c.placed_from_mm, Some((500.0, 400.0)));
}
