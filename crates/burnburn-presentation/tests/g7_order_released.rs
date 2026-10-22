//! G7-werk, W01/W02: an order also finds a job the designer released and the history kept, with no
//! window open for it. One matching job is the job of the order, several need the operator's
//! explicit choice (never automatic), the choice is kept, and the upload of such a job counts for
//! the order only through a recorded execution.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_test_support::host::rig as common;

use std::sync::Arc;

use burnburn_presentation::{
    CommandKey, FieldKey, HeadlessEditorHarness, JobCardViewModel, OrderDraft, Role, UiIntent,
};
use burnburn_test_support::FakeClock;
use common::*;

/// The designer's run: prepare and release a job over the shared history; returns the job and the
/// name it carries.
fn designer_releases(history: &Arc<RecordingHistory>, width: Option<&str>) -> (JobCardViewModel, String) {
    let mut r = rig();
    r.h.controller().attach_history(history.clone()).unwrap();
    if let Some(width) = width {
        r.h.send(UiIntent::CommitField { document: r.doc, field: FieldKey::Width, text: width.into() });
    }
    prepare(&mut r.h, r.doc);
    r.h.invoke(r.doc, CommandKey::ApproveJob);
    let job = r.h.document(r.doc).unwrap().job.unwrap();
    assert!(job.approved);
    let name = r.h.document(r.doc).unwrap().title;
    (job, name)
}

/// The operator's run: a machine, the history, no project open.
struct Operator {
    h: HeadlessEditorHarness,
    peer: Peer,
    service: Arc<burnburn_presentation::MachineService<FakePort>>,
}

fn operator(history: &Arc<RecordingHistory>) -> Operator {
    let clock = FakeClock::new();
    let peer = Peer::new(clock.clone());
    let service = service(&clock, &peer, history.clone());
    let mut h = HeadlessEditorHarness::new();
    h.controller().attach_machine(service.clone());
    h.controller().attach_history(history.clone()).unwrap();
    h.controller().set_role(Role::Operator);
    Operator { h, peer, service }
}

fn order_for(part: &str) -> UiIntent {
    UiIntent::AddOrder { order: OrderDraft { name: "Klant 31".into(), part: part.into(), quantity: 2, remnant: None } }
}

#[test]
fn an_order_takes_the_one_released_job_that_no_window_holds_and_can_upload_it() {
    let shared = history(&Peer::new(FakeClock::new()));
    let (job, name) = designer_releases(&shared, None);

    let mut op = operator(&shared);
    assert!(op.h.operator_view().jobs.iter().all(|j| j.document.is_none()), "no window holds the job");
    op.h.send(order_for(&name));
    let order = op.h.operator_view().orders[0].clone();
    assert_eq!(order.job, Some(job.plan), "the one released job is the job of the order");
    assert!(!order.needs_choice);
    let upload = order.upload.clone().expect("an upload for the order");
    assert!(upload.enabled);

    script_upload_acks(&op.peer, &job);
    op.h.send(UiIntent::UploadForOrder { order: order.id, action: upload.action, seen: upload.seen.clone() });
    for _ in 0..64 {
        op.service.poll().unwrap();
    }
    assert_eq!(uploaded_bytes(&op.peer).len(), job.bytes, "the frozen RD bytes of the released job went out");
    // An upload counts nothing: only a recorded completion does.
    assert_eq!(op.h.operator_view().orders[0].completed, 0);
}

#[test]
fn several_released_jobs_for_one_order_wait_for_the_operators_choice_which_is_kept() {
    let shared = history(&Peer::new(FakeClock::new()));
    let (first, name) = designer_releases(&shared, None);
    let (second, _) = designer_releases(&shared, Some("100"));
    assert_ne!(first.plan, second.plan);

    let mut op = operator(&shared);
    op.h.send(order_for(&name));
    let order = op.h.operator_view().orders[0].clone();
    assert!(order.needs_choice, "two jobs match: nothing is picked automatically");
    assert_eq!(order.job, None);
    assert_eq!(order.candidates.len(), 2);
    assert!(order.upload.is_none(), "no upload before the choice");

    op.h.send(UiIntent::ChooseOrderJob { order: order.id, plan: second.plan });
    let order = op.h.operator_view().orders[0].clone();
    assert_eq!((order.job, order.needs_choice), (Some(second.plan), false));
    assert!(order.upload.is_some_and(|u| u.enabled));

    // A plan that is not one of the candidates is refused.
    let mut other = op.h.operator_view().jobs[0].plan;
    for j in op.h.operator_view().jobs {
        if j.plan != first.plan && j.plan != second.plan {
            other = j.plan;
        }
    }
    if other != first.plan && other != second.plan {
        op.h.send(UiIntent::ChooseOrderJob { order: order.id, plan: other });
        assert_eq!(op.h.operator_view().orders[0].job, Some(second.plan), "the choice stands");
    }

    // The next run of the app finds the same choice.
    drop(op);
    let again = operator(&shared);
    let order = again.h.operator_view().orders[0].clone();
    assert_eq!(order.job, Some(second.plan), "the choice was kept with the order");
}

#[test]
fn a_job_a_window_holds_is_one_candidate_not_two() {
    let mut r = rig();
    r.h.controller().attach_history(r.history.clone()).unwrap();
    prepare(&mut r.h, r.doc);
    r.h.invoke(r.doc, CommandKey::ApproveJob);
    let job = r.h.document(r.doc).unwrap().job.unwrap();
    let name = r.h.document(r.doc).unwrap().title;
    r.h.controller().set_role(Role::Operator);
    r.h.send(order_for(&name));
    let order = r.h.operator_view().orders[0].clone();
    assert_eq!(order.candidates, vec![job.plan], "the window's job and the history's copy are one job");
    assert_eq!(order.job, Some(job.plan));
}
