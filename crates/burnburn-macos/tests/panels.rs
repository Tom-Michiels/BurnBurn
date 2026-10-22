//! Screens on the existing presentation view models, headless: toolbar tools, the X/Y/B/H
//! inspector and the JobPlan preview panel (plan id + clearly labelled time estimate).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_document::process::{
    Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessSettings, Speed,
};
use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
use burnburn_macos::adapter::{CommitCause, NativeEvent, Reaction, translate};
use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::{INSPECTOR_FIELDS, estimate_text, plan_panel};
use burnburn_presentation::{CommandKey, FieldKey, HeadlessEditorHarness, SelectMode, UiIntent};
use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};

#[test]
fn a_tool_button_selects_that_catalog_tool_for_that_window() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    assert!(h.view().toolbar.button("select").unwrap().selected, "select tool is the default");

    let Reaction::Intent(intent) =
        translate(NativeEvent::ToolButton { document: doc, id: "sketch.line".into() }, &h.view())
    else {
        panic!("a tool button is a tool choice")
    };
    assert_eq!(intent, UiIntent::SelectTool { document: Some(doc), id: "sketch.line".into() });
    h.send(intent);
    let vm = h.view();
    assert!(vm.toolbar.button("sketch.line").unwrap().selected);
    assert!(!vm.toolbar.button("select").unwrap().selected);
}

#[test]
fn a_variant_entry_is_a_variant_choice_of_its_group() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let event = NativeEvent::VariantChosen { document: doc, group: "sketch.line".into(), id: "sketch.polyline".into() };
    let Reaction::Intent(intent) = translate(event, &h.view()) else { panic!("a variant is a choice") };
    assert_eq!(
        intent,
        UiIntent::SelectVariant { document: Some(doc), group: "sketch.line".into(), id: "sketch.polyline".into() }
    );
}

#[test]
fn a_tool_button_for_a_closed_window_and_a_restyle_are_no_intent() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let gone = <burnburn_types::DocumentId as burnburn_types::StableId>::from_raw(9_999);
    assert_eq!(
        translate(NativeEvent::ToolButton { document: gone, id: "select".into() }, &h.view()),
        Reaction::Nothing
    );
    assert_eq!(translate(NativeEvent::ToolStripRestyle { document: doc }, &h.view()), Reaction::Nothing);
}

#[test]
fn inspector_has_x_y_width_height_and_each_commits_raw_text() {
    assert_eq!(INSPECTOR_FIELDS[..4], [FieldKey::X, FieldKey::Y, FieldKey::Width, FieldKey::Height]);
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 20.0), (130.0, 100.0));
    let vm = h.view();
    let inspector = vm.document(doc).unwrap().inspector.clone().expect("selection has an inspector");
    for key in INSPECTOR_FIELDS {
        assert!(inspector.field(key).is_some(), "presentation offers {key:?}");
    }
    let Reaction::Intent(intent) = translate(
        NativeEvent::FieldCommitted {
            document: doc,
            field: FieldKey::X,
            text: "15,5".into(),
            cause: CommitCause::Return,
        },
        &vm,
    ) else {
        panic!("X commit")
    };
    h.send(intent);
    let x = h.document(doc).unwrap().inspector.unwrap().field(FieldKey::X).unwrap().text.clone();
    assert_eq!(x, "15.5");
}

#[test]
fn estimate_is_labelled_as_estimate_and_uses_the_locale_decimal_sign() {
    assert_eq!(estimate_text(4.04, Locale::Nl), "≈ 4,0 s (schatting)");
    assert_eq!(estimate_text(4.04, Locale::En), "≈ 4.0 s (estimate)");
    assert_eq!(estimate_text(125.0, Locale::Nl), "≈ 2 min 05 s (schatting)");
    assert_eq!(estimate_text(3725.0, Locale::En), "≈ 1 h 02 min (estimate)");
    assert_eq!(estimate_text(f64::NAN, Locale::Nl), "onbekend");
    assert_eq!(estimate_text(-1.0, Locale::En), "unknown");
}

fn profile() -> MachineProfile {
    MachineProfile::new(
        "research-1500x900",
        1,
        MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap(),
        1000.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
}

fn cut() -> OperationDraft {
    let p = PowerPercent::new(20.0).unwrap();
    OperationDraft {
        name: "Snijden".into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: ProcessSettings {
            speed: Speed::mm_per_s(100.0).unwrap(),
            power: PowerRange::new(p, p).unwrap(),
            passes: Passes::new(1).unwrap(),
            air_assist: true,
        },
        output_enabled: true,
    }
}

/// The preview panel names exactly the compiled plan (the same id RD export and upload use) and
/// says so when the design changed afterwards; it never hides that the time is an estimate.
#[test]
fn plan_panel_names_the_compiled_plan_and_marks_it_stale_after_an_edit() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let empty = plan_panel(&h.view(), doc, Locale::Nl);
    assert_eq!((empty.title.as_str(), empty.plan_id.as_str()), ("Geen plan", ""));

    h.send(UiIntent::SetMachineProfile {
        profile: Box::new(profile()),
        ruida: RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged },
    });
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (130.0, 90.0));
    let rect = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![rect], mode: SelectMode::Replace });
    h.send(UiIntent::AddOperation { document: doc, draft: cut() });
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    let plan = h.document(doc).unwrap().plan.expect("compiled");

    let panel = plan_panel(&h.view(), doc, Locale::Nl);
    assert_eq!(panel.title, "Voorbeeld van het plan");
    assert_eq!(panel.plan_id, plan.id.to_string());
    // The hash is for the tooltip and "Copy id", never the visible text (LF3 Q4).
    assert_eq!(panel.id_tooltip, format!("Plan-id: {}", plan.id));
    assert_eq!(panel.copy_id_title, "Kopieer id");
    assert_eq!(plan_panel(&h.view(), doc, Locale::En).copy_id_title, "Copy id");
    assert_eq!(panel.estimate, estimate_text(plan.preview.estimated_seconds, Locale::Nl));
    assert!(panel.estimate.ends_with("(schatting)"));
    assert_eq!(panel.state, "Actueel");
    assert_eq!(panel.findings, format!("{} fouten, {} waarschuwingen", plan.errors, plan.warnings));

    h.send(UiIntent::CommitField { document: doc, field: FieldKey::Width, text: "150".into() });
    assert!(h.document(doc).unwrap().plan.unwrap().stale);
    let stale = plan_panel(&h.view(), doc, Locale::En);
    assert_eq!(stale.plan_id, plan.id.to_string(), "the old plan keeps its identity");
    assert_eq!(stale.state, "Out of date: the design or the machine changed since compiling");
}
