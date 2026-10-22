//! G6-machine, W01: the designer's approval of a frozen job is a journal record, so it survives a
//! restart. A job is its plan, its RD bytes and its machine setup: a changed plan is another job
//! and is not approved.
#![allow(clippy::unwrap_used)]

mod common;

use burnburn_history::{CompiledJobId, HistoryError, Role};
use common::*;

#[test]
fn an_approved_job_is_still_approved_after_a_restart() {
    let fs = Fs::new();
    let (mut h, _) = open(&fs);
    let profile = profile(1);
    let job = record(&mut h, &fs, &compile_session(&plate(), &profile), &profile, t(0));
    assert!(!h.is_approved(job), "freezing a job approves nothing");
    h.approve_job(&fs, job, Role::Designer, t(1)).unwrap();
    assert!(h.is_approved(job));

    drop(h);
    let (h, report) = open(&fs);
    assert_eq!(report.corrupt, None);
    assert!(h.is_approved(job), "the approval was durable before approve_job returned");
    assert_eq!(h.approved_jobs().collect::<Vec<_>>(), vec![job]);
}

#[test]
fn approving_twice_writes_one_record() {
    let fs = Fs::new();
    let (mut h, _) = open(&fs);
    let profile = profile(1);
    let job = record(&mut h, &fs, &compile_session(&plate(), &profile), &profile, t(0));
    h.approve_job(&fs, job, Role::Designer, t(1)).unwrap();
    let records = h.records().len();
    h.approve_job(&fs, job, Role::Designer, t(2)).unwrap();
    assert_eq!(h.records().len(), records, "the same job approved again is not a new fact");
}

#[test]
fn a_job_that_was_never_frozen_cannot_be_approved() {
    let fs = Fs::new();
    let (mut h, _) = open(&fs);
    let nobody = burnburn_types::ContentHash::of(burnburn_types::HashDomain::HistoryRecord, b"nobody");
    let unknown: CompiledJobId = burnburn_history::compiled_job_id(&nobody, &nobody, profile(1).revision());
    let refused = h.approve_job(&fs, unknown, Role::Designer, t(0));
    assert!(matches!(refused, Err(HistoryError::UnknownJob(j)) if j == unknown), "{refused:?}");
    assert_eq!(h.approved_jobs().count(), 0);
}

#[test]
fn a_changed_plan_is_another_job_and_does_not_inherit_the_approval() {
    let fs = Fs::new();
    let (mut h, _) = open(&fs);
    let profile = profile(1);
    let mut doc = plate();
    let first = record(&mut h, &fs, &compile_session(&doc, &profile), &profile, t(0));
    h.approve_job(&fs, first, Role::Designer, t(1)).unwrap();

    rectangle(&mut doc, 200.0, 200.0, 30.0, 30.0);
    let second = record(&mut h, &fs, &compile_session(&doc, &profile), &profile, t(2));
    assert_ne!(first, second);
    assert!(!h.is_approved(second), "the edited design must be approved again");
    assert!(h.is_approved(first), "the earlier frozen job keeps its own approval");
}

#[test]
fn the_same_plan_for_another_machine_setup_is_not_approved() {
    let fs = Fs::new();
    let (mut h, _) = open(&fs);
    let doc = plate();
    let (p1, p2) = (profile(1), profile(2));
    let first = record(&mut h, &fs, &compile_session(&doc, &p1), &p1, t(0));
    h.approve_job(&fs, first, Role::Designer, t(1)).unwrap();
    let second = record(&mut h, &fs, &compile_session(&doc, &p2), &p2, t(2));
    assert!(!h.is_approved(second), "approval belongs to the job for its machine setup revision");
}

#[test]
fn an_approval_is_remembered_with_who_gave_it() {
    let fs = Fs::new();
    let (mut h, _) = open(&fs);
    let profile = profile(1);
    let job = record(&mut h, &fs, &compile_session(&plate(), &profile), &profile, t(0));
    h.approve_job(&fs, job, Role::Designer, t(1)).unwrap();
    assert_eq!(h.approval_role(job), Some(Role::Designer));
    // An operator may not approve: the release is a designer's decision (IM31).
    let refused = h.approve_job(&fs, job, Role::Operator, t(2));
    assert!(matches!(refused, Err(HistoryError::NotADesigner)), "{refused:?}");
}
