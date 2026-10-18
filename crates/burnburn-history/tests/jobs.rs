//! Frozen job revisions (IM17, W01): the CompiledJob is independent of the live document.
#![allow(clippy::unwrap_used)]

mod common;

use common::*;

#[test]
fn editing_project_does_not_modify_prepared_job_revision() {
    let fs = Fs::new();
    let (mut h, _) = open(&fs);
    let mut doc = plate();
    let profile = profile(1);
    let first = compile_session(&doc, &profile);
    let job = record(&mut h, &fs, &first, &profile, t(0));

    // The designer keeps editing: another shape, then an undo, then another edit.
    rectangle(&mut doc, 200.0, 200.0, 30.0, 30.0);
    let after_edit = compile_session(&doc, &profile);
    assert_ne!(after_edit.plan.id(), first.plan.id(), "the edit changes what a new compile produces");
    let rev = doc.revision();
    doc.undo(rev).unwrap();
    rectangle(&mut doc, 400.0, 100.0, 50.0, 10.0);

    // Reopen the history as after an application restart.
    drop(h);
    let (h, report) = open(&fs);
    assert_eq!(report.corrupt, None);
    let stored = h.job(job).expect("frozen job survives a restart");
    assert_eq!(stored.plan, first.plan.id().hash());
    assert_eq!(stored.rd_hash, burnburn_machine::rd_hash(&first.rd));
    assert_eq!(&stored.profile, profile.revision());
    assert_eq!(stored.project_hash, burnburn_io::project_hash(&first.project).unwrap());

    let plan = h.load_plan(&fs, job).unwrap();
    assert_eq!(plan, first.plan, "the frozen plan is bit-identical to the reviewed one");
    assert_eq!(h.load_rd(&fs, job).unwrap(), first.rd, "the frozen RD bytes are the exported ones");
    let artifact = h.load_artifact(&fs, job, 1470).unwrap();
    assert_eq!((artifact.plan(), artifact.rd_hash()), (first.plan.id(), stored.rd_hash));
}
