//! C12 (G7-cam): common-line cutting in the Cut Settings sheet of a Line layer: a switch with a
//! tolerance and a minimal overlap, greyed with its reason while the machine has no proof for
//! it, one `SetAdvanced` when applied, and the plan then cuts a shared edge once.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_test_support::host::rig as common;
mod edit_rig;

use burnburn_document::process::ProcessLabel;
use burnburn_document::{ObjectKind, UndoLabel};
use burnburn_geometry::{Path, Point};
use burnburn_job::{Feature, MachineCapabilities, MoveKind, Support};
use burnburn_presentation::{
    CommandKey, CutExtras, CutField, CutSettingsViewModel, FieldState, HeadlessEditorHarness, MessageKey,
    OperationKind, SelectMode, UiIntent,
};
use burnburn_types::{DocumentId, OperationId};
use edit_rig::opened_with_kinds;

use common::{RUIDA, profile};

fn rectangle(x: f64, y: f64, w: f64, h: f64) -> Path {
    Path::polyline(&[Point::new(x, y), Point::new(x + w, y), Point::new(x + w, y + h), Point::new(x, y + h)], true)
        .unwrap()
}

/// Two 20 x 10 rectangles 0.03 mm apart (exact numbers, opened from a project).
fn layer(h: &mut HeadlessEditorHarness) -> (DocumentId, OperationId) {
    let (opened, doc, ids) = opened_with_kinds(vec![
        (rectangle(0.0, 0.0, 20.0, 10.0), ObjectKind::Path),
        (rectangle(20.03, 0.0, 20.0, 10.0), ObjectKind::Path),
    ]);
    *h = opened;
    h.send(UiIntent::Select { document: doc, ids, mode: SelectMode::Replace });
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    (doc, h.document(doc).unwrap().operations[0].id)
}

fn machine(h: &mut HeadlessEditorHarness, support: Support) {
    let caps = MachineCapabilities::conservative().with(Feature::CommonLine, support);
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

fn on(tolerance: &str, overlap: &str) -> CutExtras {
    CutExtras::new()
        .with_flag(CutField::CommonLineEnabled, true)
        .with_text(CutField::CommonLineTolerance, tolerance)
        .with_text(CutField::CommonLineMinOverlap, overlap)
}

#[test]
fn the_switch_has_its_values_and_one_apply_is_one_set_advanced() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = layer(&mut h);
    machine(&mut h, Support::Supported);
    let sheet = open(&mut h, doc, op);
    assert!(!sheet.extras.flag(CutField::CommonLineEnabled));
    assert_eq!(sheet.extras.text(CutField::CommonLineTolerance), "0.05");
    assert_eq!(sheet.extras.text(CutField::CommonLineMinOverlap), "1");
    assert_eq!(
        sheet.extras.state(CutField::CommonLineTolerance, OperationKind::Line, &sheet.gates),
        FieldState::Disabled(None),
        "greyed until the switch is on"
    );
    h.send(UiIntent::CancelCutSettings { document: doc });

    assert_eq!(apply(&mut h, doc, op, on("0,1", "2")), None);
    assert_eq!(h.document(doc).unwrap().undo, Some(UndoLabel::Process(ProcessLabel::SetAdvanced)));
    let sheet = open(&mut h, doc, op);
    assert!(sheet.extras.flag(CutField::CommonLineEnabled));
    assert_eq!(
        (sheet.extras.text(CutField::CommonLineTolerance), sheet.extras.text(CutField::CommonLineMinOverlap)),
        ("0.1", "2")
    );
    h.send(UiIntent::CancelCutSettings { document: doc });
    // Switched off again: gone.
    assert_eq!(apply(&mut h, doc, op, CutExtras::new().with_flag(CutField::CommonLineEnabled, false)), None);
    assert!(!open(&mut h, doc, op).extras.flag(CutField::CommonLineEnabled));
}

#[test]
fn values_out_of_range_are_refused_by_field() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = layer(&mut h);
    machine(&mut h, Support::Supported);
    let before = h.document(doc).unwrap().revision;
    for (tolerance, overlap, field) in [
        ("0", "1", CutField::CommonLineTolerance),
        ("6", "1", CutField::CommonLineTolerance),
        ("abc", "1", CutField::CommonLineTolerance),
        ("0,05", "-1", CutField::CommonLineMinOverlap),
        ("0,05", "abc", CutField::CommonLineMinOverlap),
    ] {
        assert_eq!(
            apply(&mut h, doc, op, on(tolerance, overlap)),
            Some(MessageKey::InvalidCutField(field)),
            "{tolerance} {overlap}"
        );
    }
    assert_eq!(h.document(doc).unwrap().revision, before);
}

#[test]
fn without_proof_the_switch_is_greyed_with_its_reason_and_a_value_is_refused() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = layer(&mut h);
    machine(&mut h, Support::Unverified { reason: "not proven".into() });
    let sheet = open(&mut h, doc, op);
    let reason = FieldState::Disabled(Some(MessageKey::MissingCapability(Feature::CommonLine)));
    for f in [CutField::CommonLineEnabled, CutField::CommonLineTolerance, CutField::CommonLineMinOverlap] {
        assert_eq!(sheet.extras.state(f, OperationKind::Line, &sheet.gates), reason, "{f:?}");
    }
    h.send(UiIntent::CancelCutSettings { document: doc });
    assert_eq!(apply(&mut h, doc, op, on("0,05", "1")), Some(MessageKey::MissingCapability(Feature::CommonLine)));
}

#[test]
fn the_plan_cuts_the_edge_the_rectangles_nearly_share_once() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = layer(&mut h);
    machine(&mut h, Support::Supported);
    let cut_length = |h: &mut HeadlessEditorHarness| {
        h.invoke(doc, CommandKey::Compile);
        h.run_tasks();
        let plan = h.document(doc).unwrap().plan.expect("a plan");
        assert_eq!(plan.errors, 0);
        plan.preview.moves.iter().filter(|m| m.kind == MoveKind::Cut).map(|m| m.from.distance(m.to)).sum::<f64>()
    };
    let plain = cut_length(&mut h);
    assert!((plain - 120.0).abs() < 1e-6, "{plain}");
    assert_eq!(apply(&mut h, doc, op, on("0,05", "1")), None);
    let shared = cut_length(&mut h);
    assert!((shared - 110.0).abs() < 1e-6, "{shared}");
}

#[test]
fn common_line_and_dot_mode_exclude_each_other_on_the_sheet() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, op) = layer(&mut h);
    // Dot mode is unverified on the default matrix; allow it for this check.
    let caps = MachineCapabilities::conservative()
        .with(Feature::CommonLine, Support::Supported)
        .with(Feature::DotMode, Support::Supported);
    h.send(UiIntent::SetMachineProfile {
        profile: Box::new(profile(1).with_capabilities(caps).unwrap()),
        ruida: RUIDA,
    });
    let both = on("0,05", "1").with_flag(CutField::DotEnabled, true);
    assert_eq!(apply(&mut h, doc, op, both), Some(MessageKey::CutOptionsConflict));
}
