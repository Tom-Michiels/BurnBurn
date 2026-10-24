//! LF17: the empty state inside the forms. An empty line becomes an `EmptyState` row; the remnants,
//! recognition and hub panels show it while their list is empty; the library and bundle skeletons
//! carry a hidden row the host shows when the model says the list is empty.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_host_models::form::{Control, FormRow, FormSpec};
use burnburn_host_models::form_lines::{FormLine, lines_form};
use burnburn_host_models::hub_form::{ID_DEVICE, ID_HISTORY, ID_PROOFS, hub_form};
use burnburn_host_models::hub_panel::hub_panel;
use burnburn_host_models::l10n::Locale;
use burnburn_host_models::panels::files_form::{art, art_form, bundle, bundle_form, material, material_form};
use burnburn_host_models::panels::{G5Line, G5Panel, g5_form, g5_panel};
use burnburn_presentation::{AppViewModel, HeadlessEditorHarness, RemnantRow, SavedTemplateView};
use burnburn_types::DocumentId;

fn rig() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    (h, doc)
}

fn all_rows(spec: &FormSpec) -> Vec<&FormRow> {
    spec.sections.iter().flat_map(|s| &s.rows).collect()
}

fn empty_rows(spec: &FormSpec) -> Vec<&FormRow> {
    all_rows(spec).into_iter().filter(|r| matches!(r.control, Control::EmptyState { .. })).collect()
}

#[test]
fn an_empty_line_becomes_an_empty_state_row_without_a_panel_control() {
    #[derive(Clone, Copy)]
    struct C;
    let lines: Vec<FormLine<C>> = vec![
        FormLine::Heading("Restplaten".into()),
        FormLine::Empty { symbol: "rectangle.dashed".into(), text: "Nog niets.".into() },
    ];
    let form = lines_form("T", &lines, |_| None);
    let rows = empty_rows(&form.spec);
    assert_eq!(rows.len(), 1);
    assert!(
        matches!(&rows[0].control, Control::EmptyState { symbol, text } if symbol == "rectangle.dashed" && text == "Nog niets.")
    );
    assert!(form.controls.is_empty(), "nothing to press");
    assert_eq!(form.spec.problems(), Vec::new());
}

fn g5_has_empty_state(vm: &AppViewModel, doc: DocumentId, panel: G5Panel) -> bool {
    let model = g5_panel(vm, doc, panel, Locale::Nl, &[]).expect("panel");
    model.lines.iter().any(|l| matches!(l, G5Line::EmptyState { .. }))
}

#[test]
fn the_remnants_panel_lines_carry_the_empty_state_only_while_the_library_is_empty() {
    let (h, doc) = rig();
    let mut vm = (*h.view()).clone();
    assert!(g5_has_empty_state(&vm, doc, G5Panel::Remnants));
    let model = g5_panel(&vm, doc, G5Panel::Remnants, Locale::Nl, &[]).unwrap();
    let form = g5_form(&model.title, &model.lines);
    assert_eq!(empty_rows(&form.spec).len(), 1, "the form shows it");
    vm.documents[0].remnants.rows.push(RemnantRow {
        id: 1,
        name: "Rest A".into(),
        material: "Multiplex".into(),
        thickness_mm: 3.0,
        width_mm: 200.0,
        height_mm: 100.0,
        usable_cm2: 150.0,
        zones: 0,
        note: String::new(),
        date: "2026-10-01".into(),
    });
    assert!(!g5_has_empty_state(&vm, doc, G5Panel::Remnants));
}

#[test]
fn the_recognition_panel_lines_carry_the_empty_state_only_without_any_template() {
    let (h, doc) = rig();
    let mut vm = (*h.view()).clone();
    assert!(g5_has_empty_state(&vm, doc, G5Panel::Recognition));
    vm.documents[0].recognition.saved.push(SavedTemplateView {
        id: 1,
        name: "Deur".into(),
        width_px: 64,
        height_px: 64,
        device: burnburn_presentation::CameraDeviceId("cam".into()),
        in_use: false,
    });
    assert!(!g5_has_empty_state(&vm, doc, G5Panel::Recognition));
}

#[test]
fn the_hub_form_shows_an_empty_state_in_place_of_each_empty_list_and_a_list_when_it_has_rows() {
    let (mut h, _) = rig();
    let mut vm = h.controller().machine_hub_view();
    vm.devices.clear();
    vm.queue.rows.clear();
    vm.history.clear();
    vm.calibration.rows.clear();
    let spec = hub_form(&hub_panel(&vm, false, Locale::Nl), Locale::Nl);
    assert_eq!(empty_rows(&spec).len(), 4, "device, queue, history and proofs");
    let ids: Vec<u32> = all_rows(&spec).iter().map(|r| r.id.0).collect();
    for gone in [ID_DEVICE, ID_HISTORY, ID_PROOFS] {
        assert!(!ids.contains(&gone), "the disabled pop-up {gone} makes room for the empty state");
    }
    assert_eq!(spec.problems(), Vec::new());
    vm.devices.push(burnburn_presentation::DeviceRow {
        id: "ruida".into(),
        revision: 1,
        width_mm: 600.0,
        height_mm: 400.0,
        active: true,
        model: None,
        firmware: None,
    });
    let spec = hub_form(&hub_panel(&vm, false, Locale::Nl), Locale::Nl);
    assert_eq!(empty_rows(&spec).len(), 3);
    assert!(all_rows(&spec).iter().any(|r| r.id.0 == ID_DEVICE), "the device pop-up is back");
}

#[test]
fn the_library_and_bundle_forms_have_an_empty_state_row_the_host_shows_when_the_model_says_so() {
    for (spec, id) in [
        (material_form(Locale::Nl), material::EMPTY),
        (art_form(Locale::Nl), art::EMPTY),
        (bundle_form(Locale::Nl), bundle::EMPTY),
    ] {
        let rows = empty_rows(&spec);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id.0, id);
        assert_eq!(spec.problems(), Vec::new());
    }
}
