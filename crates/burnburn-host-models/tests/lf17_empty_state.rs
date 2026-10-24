//! LF17: every panel model that can be empty says so with one `EmptyState` (symbol, one sentence,
//! at most one button) and returns nothing as soon as there is something to show. The running
//! tasks of a document become progress rows, with a cancel button where the task can be cancelled.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_host_models::empty_state::{
    EmptyState, art_empty, bundle_empty, history_empty, hub_empty, material_empty, object_list_empty, operations_empty,
    progress_rows, recognition_empty, remnants_empty,
};
use burnburn_host_models::l10n::Locale;
use burnburn_presentation::{
    ArtItemRow, ArtPanelViewModel, BundleChoice, BundleDialogViewModel, BundleExportView, BundleMode, CommandKey,
    HeadlessEditorHarness, MaterialPanelViewModel, ProofKind, ProofRow, RemnantRow, RemnantsViewModel, TaskKind,
    UiIntent,
};
use burnburn_types::{DocumentId, StableId};

fn rig() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    (h, doc)
}

fn assert_empty(state: Option<EmptyState>, what: &str) -> EmptyState {
    let state = state.unwrap_or_else(|| panic!("{what}: an empty panel gives an empty state"));
    assert!(!state.symbol.is_empty(), "{what}: a symbol");
    assert!(state.text.ends_with('.'), "{what}: one plain sentence, got {:?}", state.text);
    assert!(!state.text.contains("Geen machine"), "{what}: no red error text");
    state
}

#[test]
fn the_object_list_is_empty_until_there_is_an_object_and_offers_to_draw() {
    let (mut h, doc) = rig();
    for locale in [Locale::Nl, Locale::En] {
        let state = assert_empty(object_list_empty(&h.view(), doc, locale), "objects");
        let action = state.action.expect("a button");
        assert_eq!(action.intent, UiIntent::Invoke { document: Some(doc), command: CommandKey::ToolRectangle });
        assert!(!action.label.is_empty());
    }
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (30.0, 20.0));
    assert_eq!(object_list_empty(&h.view(), doc, Locale::Nl), None, "an object fills the list");
    assert_eq!(
        object_list_empty(&h.view(), <DocumentId as StableId>::from_raw(9999), Locale::Nl),
        None,
        "no such window: nothing"
    );
}

#[test]
fn the_operations_panel_is_empty_until_an_operation_exists() {
    let (mut h, doc) = rig();
    assert_empty(operations_empty(&h.view(), doc, Locale::Nl), "operations");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (30.0, 20.0));
    h.send(UiIntent::AddOperationText {
        document: doc,
        name: "Snijden".into(),
        kind: burnburn_presentation::OperationKind::Line,
        speed: "100".into(),
        power: "20".into(),
    });
    assert!(!h.document(doc).unwrap().operations.is_empty(), "the fixture has an operation");
    assert_eq!(operations_empty(&h.view(), doc, Locale::Nl), None);
}

#[test]
fn the_history_is_empty_until_the_first_step() {
    let (mut h, doc) = rig();
    let history = h.controller().history_view(doc).unwrap();
    assert_empty(history_empty(&history, Locale::En), "history");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (30.0, 20.0));
    let history = h.controller().history_view(doc).unwrap();
    assert!(!history.rows.is_empty());
    assert_eq!(history_empty(&history, Locale::En), None);
}

fn material(entries: usize, loading: bool) -> MaterialPanelViewModel {
    let row = |id| burnburn_presentation::MaterialRow {
        id,
        name: "Snijden".into(),
        material: "Multiplex".into(),
        thickness_mm: 3.0,
        machine: "ruida".into(),
        revision: 1,
        mode: burnburn_presentation::OperationKind::Line,
        speed: "10".into(),
        power: "50".into(),
        passes: "1".into(),
        notes: String::new(),
    };
    MaterialPanelViewModel {
        loading,
        persisted: true,
        issue: None,
        materials: Vec::new(),
        thicknesses_mm: Vec::new(),
        machines: Vec::new(),
        filter: burnburn_presentation::MaterialFilter::default(),
        entries: (0..entries as u64).map(row).collect(),
        selected: None,
        operations: Vec::new(),
        active_machine: None,
    }
}

fn art(items: Vec<ArtItemRow>, loading: bool, busy: bool) -> ArtPanelViewModel {
    ArtPanelViewModel {
        loading,
        persisted: true,
        busy,
        issue: None,
        groups: Vec::new(),
        group: None,
        search: String::new(),
        items,
        selected: None,
        needs_scale: None,
    }
}

#[test]
fn the_material_library_is_empty_only_when_it_has_loaded_and_holds_nothing() {
    assert_empty(material_empty(&material(0, false), Locale::Nl), "materials");
    assert_eq!(material_empty(&material(0, true), Locale::Nl), None, "still loading: not empty yet");
    assert_eq!(material_empty(&material(1, false), Locale::Nl), None);
}

#[test]
fn the_art_library_is_empty_only_when_it_has_loaded_and_holds_no_item() {
    assert_empty(art_empty(&art(Vec::new(), false, false), Locale::En), "art");
    assert_eq!(art_empty(&art(Vec::new(), true, false), Locale::En), None, "loading");
    assert_eq!(art_empty(&art(Vec::new(), false, true), Locale::En), None, "an item is being added");
    let item = ArtItemRow {
        id: 1,
        name: "Vorm".into(),
        group: "Vormen".into(),
        tags: Vec::new(),
        format: burnburn_io::art_library::ArtFormat::Svg,
        thumbnail: None,
    };
    assert_eq!(art_empty(&art(vec![item], false, false), Locale::En), None);
}

#[test]
fn the_remnants_panel_is_empty_until_a_remnant_is_kept() {
    let mut r = RemnantsViewModel::default();
    assert_empty(remnants_empty(&r, Locale::Nl), "remnants");
    r.rows.push(RemnantRow {
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
    assert_eq!(remnants_empty(&r, Locale::Nl), None);
}

#[test]
fn the_recognition_panel_is_empty_without_a_template_and_a_saved_one() {
    let (h, doc) = rig();
    let mut r = h.document(doc).unwrap().recognition;
    assert_empty(recognition_empty(&r, Locale::En), "recognition");
    r.saved.push(burnburn_presentation::SavedTemplateView {
        id: 1,
        name: "Deur".into(),
        width_px: 64,
        height_px: 64,
        device: burnburn_presentation::CameraDeviceId("cam".into()),
        in_use: false,
    });
    assert_eq!(recognition_empty(&r, Locale::En), None, "a kept template is something to show");
}

fn bundle(mode: BundleMode, loading: bool) -> BundleDialogViewModel {
    BundleDialogViewModel { loading, busy: false, issue: None, mode }
}

#[test]
fn the_bundle_sheet_is_empty_when_there_is_nothing_to_put_in_or_take_out() {
    assert_empty(bundle_empty(&bundle(BundleMode::Export(BundleExportView::default()), false), Locale::Nl), "bundle");
    assert_eq!(
        bundle_empty(&bundle(BundleMode::Export(BundleExportView::default()), true), Locale::Nl),
        None,
        "still reading the libraries"
    );
    let full = BundleExportView {
        templates: vec![BundleChoice { id: "t".to_owned(), label: "Sjabloon".into(), selected: true }],
        ..BundleExportView::default()
    };
    assert_eq!(bundle_empty(&bundle(BundleMode::Export(full), false), Locale::Nl), None);
}

#[test]
fn each_tab_of_the_machine_hub_is_empty_on_its_own() {
    let (mut h, _) = rig();
    let mut hub = h.controller().machine_hub_view();
    hub.devices.clear();
    hub.queue.rows.clear();
    hub.history.clear();
    hub.calibration.rows.clear();
    let empty = hub_empty(&hub, Locale::Nl);
    for (state, what) in
        [(empty.devices, "devices"), (empty.queue, "queue"), (empty.history, "history"), (empty.proofs, "proofs")]
    {
        assert_empty(state, what);
    }

    hub.calibration.rows.push(ProofRow {
        id: 1,
        kind: ProofKind::Kerf,
        machine: "m".into(),
        material: "Multiplex".into(),
        speed_mm_s: Some(10.0),
        value_mm: 0.1,
        value_y_mm: None,
        method: "schuifmaat".into(),
        date: "2026-10-01".into(),
    });
    hub.devices.push(burnburn_presentation::DeviceRow {
        id: "ruida".into(),
        revision: 1,
        width_mm: 600.0,
        height_mm: 400.0,
        active: true,
        model: None,
        firmware: None,
    });
    let filled = hub_empty(&hub, Locale::Nl);
    assert_eq!((filled.devices, filled.proofs), (None, None));
    assert!(filled.queue.is_some() && filled.history.is_some(), "the other tabs stay empty");
}

#[test]
fn a_running_compile_becomes_a_cancellable_progress_row_that_follows_the_task() {
    let (h, doc) = rig();
    assert_eq!(progress_rows(&h.document(doc).unwrap(), Locale::Nl), Vec::new(), "idle: no rows");
    let (mut h2, doc2) = compile_rig();
    h2.invoke(doc2, CommandKey::Compile);
    let rows = progress_rows(&h2.document(doc2).unwrap(), Locale::En);
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.fraction, None, "no progress reported: indeterminate");
    let cancel = row.cancel.as_ref().expect("a compile can be cancelled");
    assert_eq!(cancel.intent, UiIntent::CancelTask { document: doc2, task: row.task });
    let badge = h2.document(doc2).unwrap().tasks.iter().find(|t| t.kind == TaskKind::Compile).copied().unwrap();
    h2.controller().report_task_progress(doc2, badge.id, 400);
    let rows = progress_rows(&h2.document(doc2).unwrap(), Locale::En);
    assert_eq!(rows[0].fraction, Some(0.4));
    h2.run_tasks();
    assert_eq!(progress_rows(&h2.document(doc2).unwrap(), Locale::En), Vec::new(), "gone when finished");
    drop(h);
}

#[test]
fn short_reads_show_no_progress_row() {
    let (mut h, doc) = compile_rig();
    h.invoke(doc, CommandKey::Measure);
    assert_eq!(progress_rows(&h.document(doc).unwrap(), Locale::Nl), Vec::new(), "a measure is too short to show");
}

fn compile_rig() -> (HeadlessEditorHarness, DocumentId) {
    use burnburn_document::process::{
        Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessSettings, Speed,
    };
    use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
    use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};
    let profile = MachineProfile::new(
        "bed",
        1,
        MachineFrame::new(600.0, 400.0, OriginCorner::BottomLeft).unwrap(),
        1000.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap();
    let ruida = RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged };
    let mut h = HeadlessEditorHarness::new();
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile), ruida });
    h.run_tasks();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (30.0, 20.0));
    let rect = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![rect], mode: burnburn_presentation::SelectMode::Replace });
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
    (h, doc)
}
