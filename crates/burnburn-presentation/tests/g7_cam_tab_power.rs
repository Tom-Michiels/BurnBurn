//! Tab Cut Power (C04, G7-cam) in the Cut Settings sheet: a power field in the tab group
//! (`0` = skip the tabs, as before), greyed with its reason when the profile has no proof for a
//! power per movement, one `SetAdvanced` when applied, and the preview shows the tab segments
//! with their power.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use burnburn_document::UndoLabel;
use burnburn_document::process::ProcessLabel;
use burnburn_job::{Feature, MachineCapabilities, MoveKind, Support};
use burnburn_presentation::{
    AppIntent, CommandKey, CutExtras, CutField, CutSettingsViewModel, FieldState, HeadlessEditorHarness, HostEffect, MessageKey,
    OperationKind, PreviewIntent, RdRefusal, SelectMode, UiIntent,
};
use burnburn_types::{DocumentId, GeometryId, OperationId};

use common::{RUIDA, profile};

fn layer(h: &mut HeadlessEditorHarness) -> (DocumentId, OperationId) {
    let doc = h.new_document().expect("window");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let id: GeometryId = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    (doc, h.document(doc).unwrap().operations[0].id)
}

fn machine(h: &mut HeadlessEditorHarness, tab_power: Support) {
    let caps = MachineCapabilities::conservative().with(Feature::TabCutPower, tab_power);
    h.send(UiIntent::SetMachineProfile {
        profile: Box::new(profile(1).with_capabilities(caps).unwrap()),
        ruida: RUIDA,
    });
}

fn open(h: &mut HeadlessEditorHarness, doc: DocumentId, op: OperationId) -> CutSettingsViewModel {
    h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
    h.document(doc).unwrap().cut_settings.expect("sheet open")
}

fn apply(h: &mut HeadlessEditorHarness, doc: DocumentId, op: OperationId, extras: CutExtras) -> Option<MessageKey> {
    let mut input = open(h, doc, op).input();
    input.extras = extras;
    h.send(UiIntent::ApplyCutSettings { document: doc, operation: op, settings: input });
    let still_open = h.document(doc).unwrap().cut_settings;
    if still_open.is_some() {
        let error = still_open.and_then(|s| s.error);
        h.send(UiIntent::CancelCutSettings { document: doc });
        return error;
    }
    None
}

/// Two tabs of 2 mm on the 40 x 20 mm rectangle (perimeter 120), cut at `power` percent.
fn tabs(power: &str) -> CutExtras {
    CutExtras::new()
        .with_flag(CutField::TabsEnabled, true)
        .with_text(CutField::TabsValue, "2")
        .with_text(CutField::TabsWidth, "2")
        .with_text(CutField::TabsPower, power)
}

#[test]
fn the_power_is_a_field_of_the_tab_group_that_is_off_at_zero() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = layer(&mut h);
    machine(&mut h, Support::Supported);
    let sheet = open(&mut h, doc, op);
    assert_eq!(sheet.extras.text(CutField::TabsPower), "0", "tabs are skipped unless a power is typed");
    assert_eq!(
        sheet.extras.state(CutField::TabsPower, OperationKind::Line, &sheet.gates),
        FieldState::Disabled(None),
        "greyed with the other tab fields until tabs are switched on"
    );
    h.send(UiIntent::CancelCutSettings { document: doc });

    let before = h.document(doc).unwrap().revision;
    assert_eq!(apply(&mut h, doc, op, tabs("25")), None);
    assert!(h.document(doc).unwrap().revision > before);
    assert_eq!(h.document(doc).unwrap().undo, Some(UndoLabel::Process(ProcessLabel::SetAdvanced)));
    assert_eq!(open(&mut h, doc, op).extras.text(CutField::TabsPower), "25");
    h.send(UiIntent::CancelCutSettings { document: doc });

    // Back to zero: the tabs stay, but are skipped again.
    assert_eq!(apply(&mut h, doc, op, tabs("0")), None);
    let sheet = open(&mut h, doc, op);
    assert_eq!((sheet.extras.text(CutField::TabsPower), sheet.extras.flag(CutField::TabsEnabled)), ("0", true));
}

#[test]
fn a_power_outside_the_open_range_or_not_a_number_is_refused_and_changes_nothing() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = layer(&mut h);
    machine(&mut h, Support::Supported);
    let before = h.document(doc).unwrap().revision;
    for bad in ["100", "150", "-5", "abc", "25 watt"] {
        assert_eq!(apply(&mut h, doc, op, tabs(bad)), Some(MessageKey::InvalidCutField(CutField::TabsPower)), "{bad}");
    }
    assert_eq!(h.document(doc).unwrap().revision, before);
    // A decimal comma and a percent sign are fine.
    assert_eq!(apply(&mut h, doc, op, tabs("12,5 %")), None);
    assert_eq!(open(&mut h, doc, op).extras.text(CutField::TabsPower), "12.5");
}

#[test]
fn without_proof_for_a_power_per_movement_the_field_is_greyed_and_a_value_is_refused() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = layer(&mut h);
    // The default matrix leaves the power per movement unverified (RD has no proven form).
    machine(&mut h, Support::Unverified { reason: "not proven".into() });
    let sheet = open(&mut h, doc, op);
    assert_eq!(
        sheet.extras.state(CutField::TabsPower, OperationKind::Line, &sheet.gates),
        FieldState::Disabled(Some(MessageKey::MissingCapability(Feature::TabCutPower)))
    );
    // The other tab fields stay editable: skipped tabs need nothing new.
    assert!(matches!(
        sheet.extras.state(CutField::TabsWidth, OperationKind::Line, &sheet.gates),
        FieldState::Editable | FieldState::Disabled(None)
    ));
    h.send(UiIntent::CancelCutSettings { document: doc });
    let before = h.document(doc).unwrap().revision;
    assert_eq!(apply(&mut h, doc, op, tabs("25")), Some(MessageKey::MissingCapability(Feature::TabCutPower)));
    assert_eq!(h.document(doc).unwrap().revision, before);
    // Tabs without a power go through as they always did.
    assert_eq!(apply(&mut h, doc, op, tabs("0")), None);
}

#[test]
fn the_plan_cuts_the_tabs_at_the_power_and_the_preview_shows_it() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = layer(&mut h);
    machine(&mut h, Support::Supported);
    assert_eq!(apply(&mut h, doc, op, tabs("25")), None);
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    let plan = h.document(doc).unwrap().plan.expect("a plan");
    assert_eq!(plan.errors, 0);
    let cuts: Vec<_> = plan.preview.moves.iter().filter(|m| m.kind == MoveKind::Cut).collect();
    let tab_moves: Vec<_> = cuts.iter().filter(|m| m.power.is_some()).collect();
    assert_eq!(tab_moves.len(), 2, "two tabs, each one stretch");
    for m in &tab_moves {
        assert!((m.from.distance(m.to) - 2.0).abs() < 1e-9, "a tab is 2 mm wide");
    }
    // The tab power is a quarter of the power of the operation (its row says it in percent).
    let op_power: f64 = {
        let view = h.document(doc).unwrap();
        view.operations[0].power.trim_end_matches('%').trim().replace(',', ".").parse::<f64>().unwrap() / 100.0
    };
    for m in &tab_moves {
        assert!((m.power.unwrap().get() - op_power * 0.25).abs() < 1e-12, "{:?} vs {op_power}", m.power);
    }
    assert!(cuts.iter().any(|m| m.power.is_none()), "the rest of the contour keeps the full power");
    // Picking a tab in the preview says which power it is cut with.
    let m = tab_moves[0];
    let at_mm = [(m.from.x_mm() + m.to.x_mm()) / 2.0, (m.from.y_mm() + m.to.y_mm()) / 2.0];
    h.send(UiIntent::App(AppIntent::Preview(PreviewIntent::Pick { document: doc, at_mm, tolerance_mm: 0.2 })));
    let picked = h.controller().preview_view(doc).and_then(|v| v.selected).expect("a move is picked");
    assert_eq!(picked.power, m.power);
}

#[test]
fn rd_output_of_a_plan_with_tab_power_is_refused_in_words() {
    // A profile that (wrongly) claims the feature still gets the named refusal from the RD
    // backend: nothing is written.
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = layer(&mut h);
    machine(&mut h, Support::Supported);
    assert_eq!(apply(&mut h, doc, op, tabs("25")), None);
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    let path = std::path::PathBuf::from("/out/job.rd");
    h.send(UiIntent::ExportRd { document: doc, path: path.clone() });
    let updates = h.run_tasks();
    let refusal = updates.iter().flat_map(|u| u.effects.iter()).find_map(|e| match e {
        HostEffect::ShowError { message, .. } => Some(*message),
        _ => None,
    });
    assert_eq!(refusal, Some(MessageKey::RdRefused(RdRefusal::TabCutPower)));
    assert!(burnburn_io::fs::FileSystem::read(h.fs(), &path, 1 << 20).is_err(), "no file was written");
}
