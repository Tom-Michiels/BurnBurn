//! LF17: running tasks (compile, trace, nest, ...) are listed per document with their progress and
//! whether they can be cancelled, so the hosts can show a progress indicator with a cancel button.
//! A result of an outdated revision stays refused.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_document::process::{
    Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessSettings, Speed,
};
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, SelectMode, TaskBadge, TaskKind, UiIntent};
use burnburn_test_support::host::rig as common;
use burnburn_types::DocumentId;
use common::{RUIDA, profile};

fn ready(h: &mut HeadlessEditorHarness) -> DocumentId {
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    h.run_tasks();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (30.0, 20.0));
    let rect = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![rect], mode: SelectMode::Replace });
    let p = PowerPercent::new(20.0).unwrap();
    h.send(UiIntent::AddOperation {
        document: doc,
        draft: OperationDraft {
            name: "Snijden".into(),
            mode: OperationMode::Line { kerf: Kerf::NONE },
            settings: ProcessSettings {
                speed: Speed::mm_per_s(100.0).unwrap(),
                power: PowerRange::new(p, p).unwrap(),
                passes: Passes::new(1).unwrap(),
                air_assist: true,
            },
            output_enabled: true,
        },
    });
    doc
}

fn compile_badge(h: &HeadlessEditorHarness, doc: DocumentId) -> Option<TaskBadge> {
    h.document(doc).unwrap().tasks.iter().copied().find(|t| t.kind == TaskKind::Compile)
}

#[test]
fn a_started_compile_is_a_running_cancellable_task_until_it_finishes() {
    let mut h = HeadlessEditorHarness::new();
    let doc = ready(&mut h);
    assert!(compile_badge(&h, doc).is_none(), "nothing runs before the compile");
    h.invoke(doc, CommandKey::Compile);
    let badge = compile_badge(&h, doc).expect("the compile is listed");
    assert!(badge.cancellable);
    assert_eq!(badge.progress_permille, None, "the compiler reports no progress: indeterminate");
    h.run_tasks();
    assert!(compile_badge(&h, doc).is_none(), "gone after it finished");
    assert!(h.document(doc).unwrap().plan.is_some());
}

#[test]
fn cancelling_a_compile_stops_the_worker_and_leaves_no_plan() {
    let mut h = HeadlessEditorHarness::new();
    let doc = ready(&mut h);
    h.invoke(doc, CommandKey::Compile);
    let badge = compile_badge(&h, doc).unwrap();
    let requests = h.take_task_requests();
    assert!(requests.iter().all(|r| !r.cancel.is_cancelled()));
    h.send(UiIntent::CancelTask { document: doc, task: badge.id });
    let ours = requests.iter().find(|r| r.id == badge.id).expect("the compile request");
    assert!(ours.cancel.is_cancelled(), "the cancel intent reaches the worker's token");
    for r in requests {
        h.complete(r);
    }
    assert!(compile_badge(&h, doc).is_none());
    assert!(h.document(doc).unwrap().plan.is_none(), "a cancelled compile delivers nothing");
}

#[test]
fn a_cancel_from_another_document_changes_nothing() {
    let mut h = HeadlessEditorHarness::new();
    let doc = ready(&mut h);
    h.invoke(doc, CommandKey::Compile);
    let badge = compile_badge(&h, doc).unwrap();
    let other = h.new_document().unwrap();
    h.send(UiIntent::CancelTask { document: other, task: badge.id });
    let requests = h.take_task_requests();
    assert!(requests.iter().all(|r| !r.cancel.is_cancelled()), "another document cannot cancel it");
    for r in requests {
        h.complete(r);
    }
    assert!(h.document(doc).unwrap().plan.is_some());
}

#[test]
fn a_result_for_an_outdated_revision_is_still_refused_and_the_task_disappears() {
    let mut h = HeadlessEditorHarness::new();
    let doc = ready(&mut h);
    h.invoke(doc, CommandKey::Compile);
    let stale = h.take_task_requests();
    h.invoke(doc, CommandKey::Undo); // an edit while the worker runs
    for r in stale {
        h.complete_despite_cancel(r);
    }
    assert!(compile_badge(&h, doc).is_none());
    assert!(h.document(doc).unwrap().plan.is_none(), "the old result is not shown");
}

#[test]
fn reported_progress_is_shown_monotonic_and_clamped_and_dropped_with_the_task() {
    let mut h = HeadlessEditorHarness::new();
    let doc = ready(&mut h);
    h.invoke(doc, CommandKey::Compile);
    let badge = compile_badge(&h, doc).unwrap();
    h.controller().report_task_progress(doc, badge.id, 250);
    assert_eq!(compile_badge(&h, doc).unwrap().progress_permille, Some(250));
    h.controller().report_task_progress(doc, badge.id, 100);
    assert_eq!(compile_badge(&h, doc).unwrap().progress_permille, Some(250), "never backwards");
    h.controller().report_task_progress(doc, badge.id, 5000);
    assert_eq!(compile_badge(&h, doc).unwrap().progress_permille, Some(1000), "clamped");
    h.run_tasks();
    // A late report for a finished task is ignored.
    let update = h.controller().report_task_progress(doc, badge.id, 500);
    assert!(!update.view_changed);
}

#[test]
fn only_tasks_that_can_stop_halfway_offer_a_cancel() {
    for kind in [TaskKind::Compile, TaskKind::Trace, TaskKind::Centerline, TaskKind::TraceStudy, TaskKind::Nest] {
        assert!(kind.is_cancellable(), "{kind:?}");
    }
    for kind in [TaskKind::Save, TaskKind::Autosave, TaskKind::ExportRd, TaskKind::ExportGcode, TaskKind::SaveBitmap] {
        assert!(!kind.is_cancellable(), "{kind:?} finishes what it started");
    }
}
