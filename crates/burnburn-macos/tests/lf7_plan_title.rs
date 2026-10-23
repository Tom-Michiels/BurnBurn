//! LF7 (LF-A rest point): the plan title says "Plan van <date time> · <duration>" from the plan's
//! own compile moment (injected job clock), in both languages; without a clock it stays the plain
//! title instead of showing a wrong time.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use burnburn_document::process::{
    Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessSettings, Speed,
};
use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::plan_panel;
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, JobClock, SelectMode, UiIntent};
use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};

fn compiled(clock: Option<JobClock>) -> (HeadlessEditorHarness, burnburn_types::DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    h.controller().attach_clock(Arc::new(move || clock));
    let doc = h.new_document().unwrap();
    h.send(UiIntent::SetMachineProfile {
        profile: Box::new(
            MachineProfile::new(
                "research-1500x900",
                1,
                MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap(),
                1000.0,
                400.0,
                PowerRatio::new(0.0).unwrap(),
                PowerRatio::new(1.0).unwrap(),
            )
            .unwrap(),
        ),
        ruida: RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged },
    });
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (130.0, 90.0));
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
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    (h, doc)
}

#[test]
fn the_plan_title_names_the_compile_moment_and_the_duration_in_both_languages() {
    // 2026-06-17 21:04:05 UTC at +08:00 (independent reference: `date -u -r 1781730245`) is
    // 2026-06-18 05:04 local.
    let clock = JobClock::new(1_781_730_245, 480).unwrap();
    let (h, doc) = compiled(Some(clock));
    let nl = plan_panel(&h.view(), doc, Locale::Nl).title;
    assert!(nl.starts_with("Plan van 18-06-2026 05:04 \u{b7} "), "{nl}");
    assert!(nl.ends_with(" s") || nl.contains("min"), "a duration follows the moment: {nl}");
    let en = plan_panel(&h.view(), doc, Locale::En).title;
    assert!(en.starts_with("Plan of 2026-06-18 05:04 \u{b7} "), "{en}");
}

#[test]
fn a_plan_without_a_usable_clock_keeps_the_plain_title() {
    let (h, doc) = compiled(None);
    assert_eq!(plan_panel(&h.view(), doc, Locale::Nl).title, "Voorbeeld van het plan");
}
