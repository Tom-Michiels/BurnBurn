//! C09 (G5-cam): the preflight panel (Laser ▸ Preflight…) lists the findings of the plan on
//! screen, errors first, and "Show source" takes the user to the cause: the geometry is selected
//! and the view centres on it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_test_support::host::rig as common;

use burnburn_presentation::{
    CamDialog, CommandKey, DialogAction, DialogActionKind, DialogField, DialogNote, DialogValue, DialogViewModel,
    FindingKind, HeadlessEditorHarness, SelectMode, UiIntent,
};
use burnburn_types::{DocumentId, GeometryId};

use common::{RUIDA, profile};

const PREFLIGHT: CamDialog = CamDialog::Preflight;

/// Three squares: one inside the bed on a layer, one beyond the bed edge on the same layer, one on
/// no layer at all.
fn rig() -> (HeadlessEditorHarness, DocumentId, Vec<GeometryId>) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    for (from, to) in [((10.0, 10.0), (30.0, 30.0)), ((1490.0, 10.0), (1520.0, 30.0)), ((100.0, 100.0), (120.0, 120.0))]
    {
        h.invoke(doc, CommandKey::ToolRectangle);
        h.drag(doc, from, to);
    }
    h.invoke(doc, CommandKey::ToolSelect);
    let ids: Vec<GeometryId> = h.document(doc).unwrap().objects.iter().map(|o| o.id).collect();
    h.send(UiIntent::Select { document: doc, ids: ids[..2].to_vec(), mode: SelectMode::Replace });
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    (h, doc, ids)
}

fn open(h: &mut HeadlessEditorHarness, doc: DocumentId) -> DialogViewModel {
    h.send(UiIntent::OpenCamDialog { document: doc, dialog: PREFLIGHT });
    h.controller().cam_dialog(doc, PREFLIGHT).expect("the dialog is open")
}

fn pick(h: &mut HeadlessEditorHarness, doc: DocumentId, index: usize) {
    h.send(UiIntent::SetCamField {
        document: doc,
        dialog: PREFLIGHT,
        field: DialogField::PreflightPick,
        value: DialogValue::Choice(index),
    });
}

fn show(h: &mut HeadlessEditorHarness, doc: DocumentId) {
    h.send(UiIntent::CamDialogAction { document: doc, dialog: PREFLIGHT, action: DialogAction::ShowSource });
}

fn kinds(vm: &DialogViewModel) -> Vec<Option<DialogNote>> {
    vm.row(DialogField::PreflightPick).unwrap().labels.iter().map(|l| l.note).collect()
}

#[test]
fn without_a_plan_the_panel_says_so() {
    let (mut h, doc, _) = rig();
    let shown = open(&mut h, doc);
    assert!(shown.notes.contains(&DialogNote::PreflightNoPlan));
    assert!(shown.row(DialogField::PreflightPick).unwrap().labels.is_empty());
    assert!(!shown.action(DialogActionKind::ShowSource).unwrap().enabled);
}

#[test]
fn findings_are_listed_errors_first_with_the_object_they_are_about() {
    let (mut h, doc, ids) = rig();
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    let shown = open(&mut h, doc);
    assert!(shown.notes.contains(&DialogNote::PreflightCounts { errors: 1, warnings: 1 }));
    assert_eq!(
        kinds(&shown),
        vec![
            Some(DialogNote::Finding(FindingKind::OutsideBed)),
            Some(DialogNote::Finding(FindingKind::UnassignedGeometry))
        ],
        "the error before the warning"
    );
    let names: Vec<String> = h
        .document(doc)
        .unwrap()
        .objects
        .iter()
        .filter(|o| o.id == ids[1] || o.id == ids[2])
        .map(|o| o.label.clone())
        .collect();
    let labels: Vec<String> =
        shown.row(DialogField::PreflightPick).unwrap().labels.iter().map(|l| l.text.clone()).collect();
    assert_eq!(labels, names, "each finding names its object");
}

#[test]
fn show_source_selects_the_cause_and_centres_the_view_on_it() {
    let (mut h, doc, ids) = rig();
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    open(&mut h, doc);
    pick(&mut h, doc, 0);
    assert!(h.controller().cam_dialog(doc, PREFLIGHT).unwrap().action(DialogActionKind::ShowSource).unwrap().enabled);
    show(&mut h, doc);
    let view = h.document(doc).unwrap();
    assert_eq!(view.selection, vec![ids[1]], "the object that leaves the bed");
    let centre = view.camera.center_mm();
    assert!((centre.x - 1505.0).abs() < 1e-6 && (centre.y - 20.0).abs() < 1e-6, "{centre:?}");

    pick(&mut h, doc, 1);
    show(&mut h, doc);
    let view = h.document(doc).unwrap();
    assert_eq!(view.selection, vec![ids[2]], "the object on no layer");
    assert!(h.controller().cam_dialog(doc, PREFLIGHT).is_some(), "the panel stays open beside the canvas");
}

#[test]
fn an_edit_makes_the_list_stale_and_a_finding_without_geometry_cannot_be_shown() {
    let (mut h, doc, _) = rig();
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    open(&mut h, doc);
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (300.0, 300.0), (320.0, 320.0));
    let shown = h.controller().cam_dialog(doc, PREFLIGHT).unwrap();
    assert!(shown.notes.contains(&DialogNote::PreflightStale));
    assert!(kinds(&shown).contains(&Some(DialogNote::Finding(FindingKind::StalePlan))));
    let stale_index =
        kinds(&shown).iter().position(|k| *k == Some(DialogNote::Finding(FindingKind::StalePlan))).unwrap();
    pick(&mut h, doc, stale_index);
    assert!(
        !h.controller().cam_dialog(doc, PREFLIGHT).unwrap().action(DialogActionKind::ShowSource).unwrap().enabled,
        "a stale plan has no place on the canvas"
    );
}

#[test]
fn a_clean_plan_says_so() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (30.0, 30.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let id = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    let shown = open(&mut h, doc);
    assert!(shown.notes.contains(&DialogNote::PreflightClean));
}
