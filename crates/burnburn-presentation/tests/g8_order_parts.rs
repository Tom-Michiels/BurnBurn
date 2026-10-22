//! G8-cam-werk, W01/W02: an order finds a job by the name of a part in it, not only by the file
//! name of the project. The designer's release keeps the names of the parts with the frozen job, so
//! an operator without a window finds the same job; one matching job is the job of the order, several
//! still need the operator's explicit choice, and nothing is picked by guessing.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_test_support::host::rig as common;

use std::sync::Arc;

use burnburn_presentation::{CommandKey, HeadlessEditorHarness, JobCardViewModel, OrderDraft, Role, UiIntent};
use burnburn_test_support::FakeClock;
use common::*;

/// The designer's run over the shared history: the job, its file name and the name of the part drawn in it.
fn designer_releases(history: &Arc<RecordingHistory>) -> (JobCardViewModel, String, String) {
    let mut r = rig();
    r.h.controller().attach_history(history.clone()).unwrap();
    prepare(&mut r.h, r.doc);
    r.h.invoke(r.doc, CommandKey::ApproveJob);
    let job = r.h.document(r.doc).unwrap().job.unwrap();
    assert!(job.approved);
    let title = r.h.document(r.doc).unwrap().title;
    let part = r.h.document(r.doc).unwrap().objects[0].label.clone();
    (job, title, part)
}

fn operator(history: &Arc<RecordingHistory>) -> HeadlessEditorHarness {
    let clock = FakeClock::new();
    let peer = Peer::new(clock.clone());
    let service = service(&clock, &peer, history.clone());
    let mut h = HeadlessEditorHarness::new();
    h.controller().attach_machine(service);
    h.controller().attach_history(history.clone()).unwrap();
    h.controller().set_role(Role::Operator);
    h
}

fn order_for(part: &str) -> UiIntent {
    UiIntent::AddOrder { order: OrderDraft { name: "Klant 8".into(), part: part.into(), quantity: 3, remnant: None } }
}

#[test]
fn an_order_for_the_name_of_a_part_finds_the_released_job_that_holds_it() {
    let shared = history(&Peer::new(FakeClock::new()));
    let (job, title, part) = designer_releases(&shared);
    assert_ne!(part, title, "the part name is not the file name: this is a new way to find the job");

    let mut op = operator(&shared);
    op.send(order_for(&part));
    let order = op.operator_view().orders[0].clone();
    assert_eq!(order.job, Some(job.plan), "the one job with that part is the job of the order");
    assert!(!order.needs_choice);
}

#[test]
fn the_name_is_compared_without_case_and_surrounding_spaces() {
    let shared = history(&Peer::new(FakeClock::new()));
    let (job, _, part) = designer_releases(&shared);
    let mut op = operator(&shared);
    op.send(order_for(&format!("  {}  ", part.to_uppercase())));
    assert_eq!(op.operator_view().orders[0].job, Some(job.plan));
}

#[test]
fn the_file_name_still_finds_the_job_and_a_name_nobody_has_finds_nothing() {
    let shared = history(&Peer::new(FakeClock::new()));
    let (job, title, _) = designer_releases(&shared);
    let mut op = operator(&shared);
    op.send(order_for(&title));
    assert_eq!(op.operator_view().orders[0].job, Some(job.plan));
    op.send(order_for("Een onderdeel dat nergens in zit"));
    let orders = op.operator_view().orders;
    assert_eq!(orders[1].job, None);
    assert!(orders[1].candidates.is_empty() && !orders[1].needs_choice);
}

#[test]
fn a_part_in_two_released_jobs_waits_for_the_operators_choice() {
    let shared = history(&Peer::new(FakeClock::new()));
    let (first, _, part) = designer_releases(&shared);
    // A second job with the same part, a different plan (a different width).
    let mut r = rig();
    r.h.controller().attach_history(shared.clone()).unwrap();
    r.h.send(UiIntent::CommitField {
        document: r.doc,
        field: burnburn_presentation::FieldKey::Width,
        text: "100".into(),
    });
    prepare(&mut r.h, r.doc);
    r.h.invoke(r.doc, CommandKey::ApproveJob);
    let second = r.h.document(r.doc).unwrap().job.unwrap();
    assert_ne!(first.plan, second.plan);

    let mut op = operator(&shared);
    op.send(order_for(&part));
    let order = op.operator_view().orders[0].clone();
    assert!(order.needs_choice, "two jobs hold the part: nothing is picked automatically");
    assert_eq!((order.job, order.candidates.len()), (None, 2));
    op.send(UiIntent::ChooseOrderJob { order: order.id, plan: second.plan });
    assert_eq!(op.operator_view().orders[0].job, Some(second.plan));
}

#[test]
fn a_job_a_window_holds_is_found_by_the_name_of_its_part_too() {
    let mut r = rig();
    prepare(&mut r.h, r.doc);
    r.h.invoke(r.doc, CommandKey::ApproveJob);
    let job = r.h.document(r.doc).unwrap().job.unwrap();
    let part = r.h.document(r.doc).unwrap().objects[0].label.clone();
    r.h.controller().set_role(Role::Operator);
    r.h.send(order_for(&part));
    assert_eq!(r.h.operator_view().orders[0].job, Some(job.plan));
}
