//! LF7 (look-and-feel, LF-A rest point): the plan says when it was compiled, from the injected job
//! clock (never the real time), and keeps the moment of its own compile.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::sync::{Arc, Mutex};

use burnburn_document::process::{
    Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessSettings, Speed,
};
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, JobClock, SelectMode, UiIntent};
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

#[test]
fn a_compiled_plan_carries_the_moment_of_its_compile_from_the_injected_clock() {
    let now = Arc::new(Mutex::new(JobClock::new(1_781_751_845, 120).unwrap()));
    let mut h = HeadlessEditorHarness::new();
    let source = Arc::clone(&now);
    h.controller().attach_clock(Arc::new(move || Some(*source.lock().unwrap())));
    let doc = ready(&mut h);
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    let first = h.document(doc).unwrap().plan.expect("a plan");
    assert_eq!(first.compiled_at, Some(JobClock::new(1_781_751_845, 120).unwrap()));

    // The clock moves on; the plan keeps the moment it was compiled, and a new compile gets the new one.
    *now.lock().unwrap() = JobClock::new(1_781_751_999, 120).unwrap();
    assert_eq!(h.document(doc).unwrap().plan.unwrap().compiled_at, first.compiled_at);
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    assert_eq!(h.document(doc).unwrap().plan.unwrap().compiled_at, Some(JobClock::new(1_781_751_999, 120).unwrap()));
}

#[test]
fn without_a_usable_clock_the_plan_has_no_time_instead_of_a_wrong_one() {
    let mut h = HeadlessEditorHarness::new();
    h.controller().attach_clock(Arc::new(|| None));
    let doc = ready(&mut h);
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    assert_eq!(h.document(doc).unwrap().plan.expect("a plan").compiled_at, None);
}
