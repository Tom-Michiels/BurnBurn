//! G6-machine, W01: the designer's release ("approved") of a prepared job is kept by the history,
//! so it survives a restart; a changed design is another job and must be released again; the
//! operator finds a released job after a restart without opening the project.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_test_support::host::rig as common;

use std::sync::Arc;
use std::sync::atomic::Ordering;

use burnburn_presentation::{
    CommandKey, HeadlessEditorHarness, HostEffect, MachineStatusKey, MessageKey, Role, UiIntent,
};
use burnburn_test_support::FakeClock;

use common::*;

fn approve(h: &mut HeadlessEditorHarness, doc: burnburn_types::DocumentId) -> burnburn_presentation::JobCardViewModel {
    h.invoke(doc, CommandKey::ApproveJob);
    h.document(doc).unwrap().job.unwrap()
}

/// A new application run over the same history: a fresh controller and window.
fn restarted(history: &Arc<RecordingHistory>) -> (HeadlessEditorHarness, burnburn_types::DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    h.controller().attach_history(history.clone()).unwrap();
    let doc = h.new_document().unwrap();
    design(&mut h, doc);
    (h, doc)
}

#[test]
fn approving_records_the_job_in_the_history_before_it_shows_as_approved() {
    let r = rig();
    let history = history(&r.peer);
    let (mut h, doc) = restarted(&history);
    let job = prepare(&mut h, doc);
    assert!(!job.approved && history.approvals.lock().unwrap().is_empty());
    let job = approve(&mut h, doc);
    assert!(job.approved);
    let kept = history.approvals.lock().unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!((kept[0].plan, kept[0].rd_hash), (job.plan, job.rd_hash), "exactly the job that was reviewed");
}

#[test]
fn a_history_that_cannot_keep_the_approval_leaves_the_job_unapproved_and_says_so() {
    let r = rig();
    let history = history(&r.peer);
    let (mut h, doc) = restarted(&history);
    prepare(&mut h, doc);
    history.fail.store(true, Ordering::SeqCst);
    let update = h.invoke(doc, CommandKey::ApproveJob);
    assert!(
        update
            .effects
            .iter()
            .any(|e| matches!(e, HostEffect::ShowError { message: MessageKey::IntentNotRecorded, .. })),
        "{:?}",
        update.effects
    );
    assert!(!h.document(doc).unwrap().job.unwrap().approved, "an approval that is not durable is not given");
    assert!(history.approvals.lock().unwrap().is_empty());
}

#[test]
fn after_a_restart_the_same_job_prepared_again_is_still_approved() {
    let r = rig();
    let history = history(&r.peer);
    let (mut first, doc) = restarted(&history);
    prepare(&mut first, doc);
    let approved = approve(&mut first, doc);
    drop(first);

    let (mut second, doc2) = restarted(&history);
    let job = prepare(&mut second, doc2);
    assert_eq!(job.plan, approved.plan, "the same design compiles to the same plan");
    assert!(job.approved, "the release was durable, nobody has to approve the same job twice");
}

#[test]
fn a_changed_design_has_to_be_approved_again() {
    let r = rig();
    let history = history(&r.peer);
    let (mut h, doc) = restarted(&history);
    prepare(&mut h, doc);
    let approved = approve(&mut h, doc);

    // The designer moves on: another shape, a new compile, a new prepared job.
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (200.0, 200.0), (230.0, 230.0));
    let next = prepare(&mut h, doc);
    assert_ne!(next.plan, approved.plan);
    assert!(!next.approved, "an edited plan does not inherit the release of the earlier one");
}

#[test]
fn an_operator_finds_the_released_job_after_a_restart_without_a_window_and_can_upload_it() {
    let mut first = rig();
    let kept = history(&first.peer);
    // The designer's run: prepare and release over the shared history.
    first.h.controller().attach_history(kept.clone()).unwrap();
    let job = prepare(&mut first.h, first.doc);
    let released = approve(&mut first.h, first.doc);
    assert_eq!(job.plan, released.plan);
    drop(first);

    // The operator's run: a machine, the history, no project open.
    let clock = FakeClock::new();
    let peer = Peer::new(clock.clone());
    let service = service(&clock, &peer, kept.clone());
    let mut h = HeadlessEditorHarness::new();
    h.controller().attach_machine(service.clone());
    h.controller().attach_history(kept).unwrap();
    h.controller().set_role(Role::Operator);

    let view = h.operator_view();
    let row = view.jobs.iter().find(|j| j.plan == released.plan).expect("the released job is listed");
    assert_eq!(row.document, None, "it comes from the history, not from a window");
    assert_eq!(row.rd_hash, released.rd_hash);

    // Upload sends the frozen RD bytes.
    let chunks = released.bytes.div_ceil(CHUNK);
    script_identity(&peer, MODEL);
    for i in 0..chunks {
        peer.script(
            burnburn_test_support::Step::expect_matching(
                format!("chunk {i}"),
                move |q| matches!(q, burnburn_machine::Request::UploadChunk { index, .. } if *index == i),
            )
            .reply(burnburn_machine::Reply::Ack),
        );
    }
    h.press(None, row.upload.as_ref().expect("upload control"));
    for _ in 0..64 {
        service.poll().unwrap();
    }
    let vm = h.operator_view().machine.unwrap();
    assert_ne!(vm.status, MachineStatusKey::Failed, "{:?}", vm.message);
    assert_eq!(uploaded_bytes(&peer).len(), released.bytes, "the released job's RD bytes went out");
}

#[test]
fn an_operator_cannot_release_a_job() {
    let mut r = rig();
    prepare(&mut r.h, r.doc);
    r.h.controller().set_role(Role::Operator);
    r.h.send(UiIntent::Invoke { document: Some(r.doc), command: CommandKey::ApproveJob });
    assert!(!r.h.document(r.doc).unwrap().job.unwrap().approved);
}
