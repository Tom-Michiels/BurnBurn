//! G6-werk, W08: the compact operator view gathers what the operator needs to run the right job:
//! machine, revision, size on the bed and counts, in one place, on every window's view model. It has
//! no editing commands, and read and control rights stay apart (the role decides what is offered).

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod common;

use burnburn_presentation::{CommandKey, Role};
use common::*;

fn approved_rig() -> (Rig, burnburn_presentation::JobCardViewModel) {
    let mut r = rig();
    prepare(&mut r.h, r.doc);
    r.h.invoke(r.doc, CommandKey::ApproveJob);
    let job = r.h.document(r.doc).unwrap().job.unwrap();
    assert!(job.approved);
    (r, job)
}

#[test]
fn an_approved_job_in_the_operator_view_names_its_revision_and_its_size_on_the_bed() {
    let mut r = rig();
    let revision = r.h.document(r.doc).unwrap().revision;
    prepare(&mut r.h, r.doc);
    r.h.invoke(r.doc, CommandKey::ApproveJob);
    let view = r.h.operator_view();
    assert_eq!(view.jobs.len(), 1);
    let row = &view.jobs[0];
    assert_eq!(row.revision, revision, "the document revision the job was compiled from");
    let b = row.bounds.expect("the plan has bounds");
    // The 120 x 80 rectangle of the rig, whatever corner the bed counts from.
    assert!((b.width() - 120.0).abs() < 1e-6 && (b.height() - 80.0).abs() < 1e-6, "{b:?}");
    assert_eq!(row.material, None, "no plate with a material in this drawing");
}

#[test]
fn a_changed_design_does_not_change_what_the_operator_view_says_about_the_frozen_job() {
    let (mut r, _) = approved_rig();
    let before = r.h.operator_view().jobs[0].clone();
    assert!(before.bounds.is_some(), "there is something to keep");
    r.h.invoke(r.doc, CommandKey::SelectAll);
    r.h.invoke(r.doc, CommandKey::Duplicate);
    let after = r.h.operator_view().jobs[0].clone();
    assert_eq!((after.revision, after.bounds), (before.revision, before.bounds), "frozen with the job");
}

#[test]
fn every_window_carries_the_operator_view_and_it_equals_the_controllers() {
    let (mut r, _) = approved_rig();
    r.h.controller().set_role(Role::Operator);
    let direct = r.h.operator_view();
    let on_window = r.h.document(r.doc).unwrap().operator.expect("an operator view on the window");
    assert_eq!(on_window, direct);
    assert_eq!(on_window.jobs.len(), 1);
    assert!(on_window.machine.is_some());
}

#[test]
fn without_a_machine_service_orders_or_approved_jobs_there_is_no_operator_view() {
    let mut h = burnburn_presentation::HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    assert!(h.document(doc).unwrap().operator.is_none());
}
