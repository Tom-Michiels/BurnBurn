//! The file panels, headless (W3-UI-bestanden): what the import report, the Material and Art
//! Library panels, the bundle sheet and the export dialog show, and the intents their controls
//! send. The view models come from the real controller (in-memory file system, no AppKit); the
//! native panels need a window (tests/native/macos/file_panels.rs).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path as FsPath, PathBuf};

use burnburn_io::fs::FileSystem;
use burnburn_macos::l10n::Locale;
use burnburn_macos::l10n_files::{FilesText, files_text};
use burnburn_macos::panels::import_panel;
use burnburn_macos::panels_files::*;
use burnburn_presentation::{
    AppViewModel, BundleConflict, BundleKind, CommandKey, ExportFormat, FilesIntent, HeadlessEditorHarness,
    ImportPages, OperationField, SelectMode, UiIntent,
};
use burnburn_types::DocumentId;

fn harness() -> HeadlessEditorHarness {
    let mut h = HeadlessEditorHarness::new();
    for dir in ["/lib", "/in", "/out"] {
        h.fs().create_dir_all_for_test(FsPath::new(dir));
    }
    h.controller().set_library_dir(Some(PathBuf::from("/lib")));
    h
}

fn put(h: &HeadlessEditorHarness, path: &str, bytes: &[u8]) -> PathBuf {
    h.fs().insert(FsPath::new(path), bytes.to_vec());
    PathBuf::from(path)
}

fn rect_layer(h: &mut HeadlessEditorHarness) -> (DocumentId, burnburn_types::OperationId) {
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let id = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    let op = h.document(doc).unwrap().operations[0].id;
    h.send(UiIntent::SetOperationField {
        document: doc,
        operation: op,
        field: OperationField::Speed,
        text: "12".into(),
    });
    (doc, op)
}

fn dispatch(h: &mut HeadlessEditorHarness, intent: impl FnOnce(&AppViewModel) -> Option<UiIntent>) {
    let vm = h.view();
    h.send(intent(&vm).expect("the control sends an intent"));
    h.run_tasks();
}

#[test]
fn every_file_text_has_nl_and_en() {
    for text in FilesText::ALL {
        for locale in [Locale::Nl, Locale::En] {
            assert!(!files_text(*text, locale).trim().is_empty(), "{text:?} has no {locale:?} text");
        }
    }
    // The placeholders of both languages match, so a translation cannot lose a value.
    for text in FilesText::ALL {
        let holes = |s: &str| (0..8).filter(|i| s.contains(&format!("{{{i}}}"))).count();
        assert_eq!(
            holes(files_text(*text, Locale::Nl)),
            holes(files_text(*text, Locale::En)),
            "{text:?}: placeholders differ between NL and EN"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Import report.
// ---------------------------------------------------------------------------------------------

fn two_page_pdf() -> Vec<u8> {
    let stream = |data: &str| format!("<< /Length {} >>\nstream\n{data}\nendstream", data.len()).into_bytes();
    let objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 50] /Contents 4 0 R >>".to_vec(),
        stream("0 0 40 20 re S"),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] /Contents 6 0 R >>".to_vec(),
        stream("0 0 m 100 0 l S"),
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for o in &offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes(),
    );
    out
}

fn preview(h: &mut HeadlessEditorHarness, doc: DocumentId, path: PathBuf) {
    h.send(UiIntent::Import { document: doc, path });
    h.run_tasks();
}

#[test]
fn a_pdf_import_offers_its_pages_and_all_pages_together() {
    let mut h = harness();
    let doc = h.new_document().unwrap();
    let path = put(&h, "/in/twee.pdf", &two_page_pdf());
    preview(&mut h, doc, path);
    let vm = h.view();
    let panel = import_panel(&vm, doc, Locale::En).unwrap();
    assert!(!panel.show_units, "a PDF has its own unit");
    assert_eq!(panel.unit_prompt, "Size from the file (1 point = 0.3528 mm)");
    assert_eq!(
        panel.pages.items,
        ["Page 1 — 35.3 × 17.6 mm", "Page 2 — 70.6 × 35.3 mm", "All pages side by side"],
        "100 × 50 pt and 200 × 100 pt"
    );
    assert_eq!(panel.pages.selected, Some(0));
    assert!(panel.commit_enabled);

    h.send(import_pages_intent(doc, &vm, 2).unwrap());
    assert!(import_panel(&h.view(), doc, Locale::En).unwrap().report.contains("Reading the page"));
    assert!(!import_panel(&h.view(), doc, Locale::En).unwrap().commit_enabled, "Importeer waits for the pages");
    h.run_tasks();
    let panel = import_panel(&h.view(), doc, Locale::En).unwrap();
    assert_eq!(panel.pages.selected, Some(2));
    assert!(panel.commit_enabled);
    assert!(import_pages_intent(doc, &h.view(), 3).is_none(), "no such item");
    // The page choice itself is the controller's: the same values come back as a choice.
    let Some(UiIntent::Files(FilesIntent::SetImportPages { pages, .. })) = import_pages_intent(doc, &h.view(), 1)
    else {
        panic!("page intent")
    };
    assert_eq!(pages, ImportPages::One(1));
}

#[test]
fn a_plotter_file_shows_its_pens_as_proposed_layers() {
    let mut h = harness();
    let doc = h.new_document().unwrap();
    let path = put(&h, "/in/plaat.plt", b"IN;SP1;PU0,0;PD400,0,400,400,0,400,0,0;PU200,200;SP2;CI100;PU;SP0;");
    preview(&mut h, doc, path);
    let panel = import_panel(&h.view(), doc, Locale::En).unwrap();
    assert!(!panel.show_units && panel.pages.items.is_empty());
    assert_eq!(panel.unit_prompt, "Plotter units (0.025 mm per step)");
    assert!(panel.report.contains("Proposed layers:\n  Pen 1: 1 object\n  Pen 2: 1 object"), "{}", panel.report);
    let nl = import_panel(&h.view(), doc, Locale::Nl).unwrap();
    assert!(nl.report.contains("Voorgestelde lagen:\n  Pen 1: 1 object"), "{}", nl.report);
}

#[test]
fn a_lightburn_project_reports_layers_values_warnings_and_what_was_left_out() {
    let mut h = harness();
    let doc = h.new_document().unwrap();
    let fixture = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../research/lightburn/fixtures/rectangle-fill.lbrn2"),
    )
    .unwrap();
    let path = put(&h, "/in/plaat.lbrn2", &fixture);
    preview(&mut h, doc, path);
    let panel = import_panel(&h.view(), doc, Locale::En).unwrap();
    assert_eq!(panel.unit_prompt, "LightBurn projects are in millimetres");
    assert!(!panel.show_units);
    let report = &panel.report;
    assert!(report.contains("Layers of the project:\n  C00 (fill): 1 shapes → a layer is created"), "{report}");
    assert!(report.contains("not taken over:") && report.contains("maxPower2=20"), "{report}");
    assert!(report.contains("priority=0"), "{report}");
    assert!(report.contains("Note: made for device \"Ruida\"; active profile: none."), "{report}");
    assert!(report.contains("mirrors the origin (horizontally + vertically)"), "{report}");
    assert!(panel.commit_enabled);
}

// ---------------------------------------------------------------------------------------------
// Material Library panel.
// ---------------------------------------------------------------------------------------------

fn open_material(h: &mut HeadlessEditorHarness, doc: DocumentId) {
    h.send(UiIntent::Files(FilesIntent::OpenMaterialLibrary { document: doc }));
    h.run_tasks();
}

#[test]
fn the_material_panel_shows_copied_and_linked_layers_and_gates_its_buttons() {
    let mut h = harness();
    let (doc, a) = rect_layer(&mut h);
    // A second layer.
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (60.0, 0.0), (80.0, 10.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let second = h.document(doc).unwrap().objects[1].id;
    h.send(UiIntent::Select { document: doc, ids: vec![second], mode: SelectMode::Replace });
    h.send(UiIntent::AssignLayer { document: doc, slot: 2, add: false });
    let b = h.document(doc).unwrap().operations[1].id;
    h.send(UiIntent::Files(FilesIntent::OpenMaterialLibrary { document: doc }));
    let loading = material_panel(&h.view(), doc, Locale::En).unwrap();
    assert_eq!(loading.status, "Reading the library…");
    assert_eq!(material_actions(&h.view(), doc, 0), MaterialActions::default(), "nothing can act while loading");
    h.run_tasks();

    let panel = material_panel(&h.view(), doc, Locale::En).unwrap();
    assert_eq!(panel.status, "");
    assert_eq!(panel.entries.items, ["No settings found"]);
    assert_eq!(panel.operations.items.len(), 2);
    assert!(panel.operations.items[0].ends_with("set by hand"), "{:?}", panel.operations.items);
    // Store layer one, apply it to layer two.
    dispatch(&mut h, |vm| material_save_intent(doc, vm, 0, "Snijden", "Berken", "3"));
    let panel = material_panel(&h.view(), doc, Locale::En).unwrap();
    assert_eq!(panel.entries.items, ["Snijden — Berken 3 mm (r1)"]);
    assert_eq!(panel.materials.items, ["All materials", "Berken"]);
    assert_eq!(panel.detail, "Line · 12 mm/s · 20 % · 1 pass(es)");
    let actions = material_actions(&h.view(), doc, 1);
    assert!(actions.apply && actions.overwrite && actions.save && actions.remove && !actions.update);
    dispatch(&mut h, |vm| material_apply_intent(doc, vm, 1));
    let panel = material_panel(&h.view(), doc, Locale::En).unwrap();
    assert!(panel.operations.items[0].ends_with("set by hand"));
    assert!(
        panel.operations.items[1].contains("copy of recipe Snijden (r1) from library entry 1: up to date"),
        "{:?}",
        panel.operations.items
    );
    assert!(material_actions(&h.view(), doc, 1).update, "a linked layer can update its recipe");

    // The library moves on: the layer is told, and is not changed.
    h.send(UiIntent::SetOperationField {
        document: doc,
        operation: a,
        field: OperationField::Speed,
        text: "30".into(),
    });
    dispatch(&mut h, |vm| material_overwrite_intent(doc, vm, 0));
    let panel = material_panel(&h.view(), doc, Locale::En).unwrap();
    assert!(panel.operations.items[1].ends_with("update available (r1 → r2)"), "{:?}", panel.operations.items);
    let layer_b = h.document(doc).unwrap().operations.into_iter().find(|o| o.id == b).unwrap();
    assert_eq!(layer_b.speed, "12");
    dispatch(&mut h, |vm| material_update_intent(doc, vm, 1, true));
    let layer_b = h.document(doc).unwrap().operations.into_iter().find(|o| o.id == b).unwrap();
    assert_eq!(layer_b.speed, "30", "only the explicit update changes the layer");
    assert!(material_update_intent(doc, &h.view(), 0, false).is_none(), "a hand-made layer has nothing to update");
}

#[test]
fn the_material_filter_controls_map_to_the_search() {
    let mut h = harness();
    let (doc, _) = rect_layer(&mut h);
    open_material(&mut h, doc);
    dispatch(&mut h, |vm| material_save_intent(doc, vm, 0, "Snijden", "Berken", "3"));
    dispatch(&mut h, |vm| material_save_intent(doc, vm, 0, "Graveren", "Acryl", "6"));
    let panel = material_panel(&h.view(), doc, Locale::En).unwrap();
    assert_eq!(panel.materials.items, ["All materials", "Acryl", "Berken"]);
    dispatch(&mut h, |vm| material_filter_intent(doc, vm, 2, "", 0));
    let panel = material_panel(&h.view(), doc, Locale::En).unwrap();
    assert_eq!((panel.materials.selected, panel.entries.items.len()), (Some(2), 1));
    dispatch(&mut h, |vm| material_filter_intent(doc, vm, 0, "6", 0));
    assert_eq!(material_panel(&h.view(), doc, Locale::En).unwrap().entries.items, ["Graveren — Acryl 6 mm (r1)"]);
    assert!(material_filter_intent(doc, &h.view(), 9, "", 0).is_none(), "no such material item");
    dispatch(&mut h, |vm| material_select_intent(doc, vm, 0));
    dispatch(&mut h, |vm| material_remove_intent(doc, vm));
    assert_eq!(material_panel(&h.view(), doc, Locale::En).unwrap().entries.items, ["No settings found"]);
}

// ---------------------------------------------------------------------------------------------
// Art Library panel.
// ---------------------------------------------------------------------------------------------

const SQUARE_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10mm" height="10mm" viewBox="0 0 10 10"><rect x="0" y="0" width="10" height="10" fill="none" stroke="black"/></svg>"#;

#[test]
fn the_art_panel_creates_a_group_when_the_first_file_comes_and_shows_a_thumbnail() {
    let mut h = harness();
    let doc = h.new_document().unwrap();
    h.send(UiIntent::Files(FilesIntent::OpenArtLibrary { document: doc }));
    h.run_tasks();
    let file = put(&h, "/in/vierkant.svg", SQUARE_SVG.as_bytes());
    for intent in art_add_file_intents(doc, &h.view(), &file, "", 0, Locale::En) {
        h.send(intent);
    }
    h.run_tasks();
    let panel = art_panel(&h.view(), doc, Locale::En).unwrap();
    assert_eq!(panel.groups.items, ["All groups", "General (1)"]);
    assert_eq!(panel.items.items, ["vierkant (General)"]);
    assert_eq!(panel.detail, "Format SVG");
    let bitmap = panel.thumbnail.expect("a thumbnail");
    assert_eq!((bitmap.width, bitmap.height, bitmap.rgba.len()), (64, 64, 64 * 64 * 4));
    let dark = bitmap.rgba.chunks(4).filter(|p| p[0] < 128).count();
    assert!(dark > 100 && dark < 64 * 64 / 2, "an outline, not a filled box: {dark} dark pixels");
    assert!(panel.can_place && panel.can_edit_item && !panel.can_edit_group);

    dispatch(&mut h, |vm| art_place_intent(doc, vm));
    assert_eq!(h.document(doc).unwrap().objects.len(), 1);
    dispatch(&mut h, |vm| art_group_intent(doc, vm, 1));
    assert!(art_panel(&h.view(), doc, Locale::En).unwrap().can_edit_group);
    dispatch(&mut h, |vm| art_rename_group_intent(doc, vm, "Vormen"));
    dispatch(&mut h, |vm| art_rename_item_intent(doc, vm, "Blok"));
    let panel = art_panel(&h.view(), doc, Locale::En).unwrap();
    assert_eq!(panel.items.items, ["Blok (Vormen)"]);
    dispatch(&mut h, |vm| art_search_intent(doc, vm, "nope"));
    assert_eq!(art_panel(&h.view(), doc, Locale::En).unwrap().items.items, ["No items"]);
    assert!(art_search_intent(doc, &h.view(), "nope").is_none(), "an unchanged search sends nothing");
}

#[test]
fn a_dxf_without_a_unit_shows_the_prompt_and_sends_the_chosen_unit() {
    let mut h = harness();
    let doc = h.new_document().unwrap();
    h.send(UiIntent::Files(FilesIntent::OpenArtLibrary { document: doc }));
    h.run_tasks();
    let dxf =
        std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-data/import/unitless-lines.dxf"))
            .unwrap();
    let file = put(&h, "/in/plan.dxf", &dxf);
    for intent in art_add_file_intents(doc, &h.view(), &file, "Plan", 0, Locale::En) {
        h.send(intent);
    }
    h.run_tasks();
    let panel = art_panel(&h.view(), doc, Locale::En).unwrap();
    assert_eq!(panel.scale_prompt.as_deref(), Some("plan.dxf names no unit. Choose one and add it again:"));
    assert!(panel.items.items == ["No items"]);
    // Inch (item 2 of the unit pop-up): the second try carries the unit.
    for intent in art_add_file_intents(doc, &h.view(), &file, "Plan", 2, Locale::En) {
        h.send(intent);
    }
    h.run_tasks();
    let panel = art_panel(&h.view(), doc, Locale::En).unwrap();
    assert!(panel.scale_prompt.is_none());
    assert_eq!(panel.items.items.len(), 1);
}

// ---------------------------------------------------------------------------------------------
// Bundle sheet and export dialog.
// ---------------------------------------------------------------------------------------------

#[test]
fn the_bundle_sheet_gates_export_on_a_choice_and_shows_tri_state_kinds() {
    let mut h = harness();
    let (doc, _) = rect_layer(&mut h);
    open_material(&mut h, doc);
    dispatch(&mut h, |vm| material_save_intent(doc, vm, 0, "Snijden", "Berken", "3"));
    dispatch(&mut h, |vm| material_save_intent(doc, vm, 0, "Graveren", "Berken", "3"));
    h.send(UiIntent::Files(FilesIntent::OpenBundleExport { document: doc }));
    h.run_tasks();
    let panel = bundle_panel(&h.view(), doc, Locale::En).unwrap();
    assert_eq!(panel.title, "Export bundle");
    assert_eq!(panel.kinds[0].title, "Material settings (0 of 2)");
    assert!(!panel.can_export, "nothing chosen yet");
    assert!(!panel.kinds[1].enabled, "no art to choose from");
    dispatch(&mut h, |vm| bundle_kind_intent(doc, vm, 0, true));
    let panel = bundle_panel(&h.view(), doc, Locale::En).unwrap();
    assert_eq!((panel.kinds[0].state, panel.can_export), (CheckState::On, true));
    h.send(UiIntent::Files(FilesIntent::ToggleBundleItem {
        document: doc,
        item: burnburn_presentation::BundleItem::Material(1),
        selected: false,
    }));
    assert_eq!(bundle_panel(&h.view(), doc, Locale::En).unwrap().kinds[0].state, CheckState::Mixed);
    assert!(bundle_kind_intent(doc, &h.view(), 7, true).is_none());
    // A click on the box: a mixed or empty kind becomes "all of it", a full kind becomes "none".
    let click = |h: &mut HeadlessEditorHarness| {
        let intent = bundle_kind_toggle_intent(doc, &h.view(), 0).expect("the materials box can be clicked");
        h.send(intent);
    };
    click(&mut h);
    assert_eq!(bundle_panel(&h.view(), doc, Locale::En).unwrap().kinds[0].state, CheckState::On, "mixed → all");
    click(&mut h);
    assert_eq!(bundle_panel(&h.view(), doc, Locale::En).unwrap().kinds[0].state, CheckState::Off, "all → none");
    assert!(bundle_kind_toggle_intent(doc, &h.view(), 1).is_none(), "an empty kind has nothing to click");
    dispatch(&mut h, |vm| bundle_kind_intent(doc, vm, 0, true));
    let path = PathBuf::from("/out/a.burnburnbundle");
    h.send(bundle_write_intent(doc, path.clone()));
    h.run_tasks();
    assert!(bundle_panel(&h.view(), doc, Locale::En).unwrap().status.starts_with("Bundle written: /out/"));
    assert!(h.fs().exists(&path).unwrap());
}

#[test]
fn the_bundle_import_sheet_shows_the_profile_diff_and_only_a_differing_profile_can_be_replaced() {
    use burnburn_io::bundle::{BundleContents, encode_bundle};
    use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
    let profile = |revision: u32, speed: f64| {
        MachineProfile::new(
            "ruida-1500",
            revision,
            MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap(),
            speed,
            400.0,
            PowerRatio::new(0.0).unwrap(),
            PowerRatio::new(1.0).unwrap(),
        )
        .unwrap()
    };
    let mut h = harness();
    h.send(UiIntent::SetMachineProfile {
        profile: Box::new(profile(1, 1000.0)),
        ruida: burnburn_ruida::RuidaProfile {
            magic: burnburn_ruida::Magic::FIXTURE_0X88,
            overscan: burnburn_ruida::OverscanPolicy::ControllerManaged,
        },
    });
    let doc = h.new_document().unwrap();
    let bundle =
        encode_bundle(&BundleContents { profiles: &[profile(2, 600.0)], ..BundleContents::default() }).unwrap();
    let file = put(&h, "/in/b.burnburnbundle", &bundle);
    h.send(bundle_open_intent(doc, file));
    assert_eq!(bundle_panel(&h.view(), doc, Locale::En).unwrap().status, "Reading the bundle…");
    h.run_tasks();
    let panel = bundle_panel(&h.view(), doc, Locale::En).unwrap();
    assert!(panel.import && panel.can_apply);
    assert!(
        panel.preview.contains("ruida-1500 (r2, here r1) — differs in: max_speed_mm_s"),
        "the field names come before anything is replaced: {}",
        panel.preview
    );
    assert!(panel.preview.contains("A differing profile stays as it is unless you tick it above."));
    assert_eq!(panel.profiles.len(), 1);
    assert!(panel.profiles[0].enabled && !panel.profiles[0].replace);
    assert_eq!(panel.conflict.selected, Some(0), "skip is the default");

    dispatch(&mut h, |vm| bundle_profile_intent(doc, vm, 0, true));
    dispatch(&mut h, |_| bundle_conflict_intent(doc, 2));

    let panel = bundle_panel(&h.view(), doc, Locale::En).unwrap();
    assert!(panel.profiles[0].replace);
    assert_eq!(panel.conflict.selected, Some(2));
    assert_eq!(
        bundle_conflict_intent(doc, 1),
        Some(UiIntent::Files(FilesIntent::SetBundleConflict { document: doc, choice: BundleConflict::Replace }))
    );
    assert!(bundle_conflict_intent(doc, 3).is_none());
    h.send(bundle_apply_intent(doc));
    h.run_tasks();
    let panel = bundle_panel(&h.view(), doc, Locale::En).unwrap();
    assert!(panel.report.starts_with("Done: 0 settings added"), "{}", panel.report);
    assert!(panel.report.contains("0 profiles added, 1 replaced"), "{}", panel.report);
    assert!(!panel.can_apply, "an import is applied once");
    assert!(!panel.profiles[0].enabled);
    let _ = BundleKind::Art;
}

#[test]
fn the_export_dialog_gates_the_button_on_the_acknowledgement_and_lists_what_is_lost() {
    let mut h = harness();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (10.0, 10.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let source = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![source], mode: SelectMode::Replace });
    // A rectangular pattern is a modifier: its copies are written as plain objects.
    h.send(UiIntent::SelectTool { document: Some(doc), id: "sketch.pattern.rectangular".into() });
    h.send(UiIntent::SetToolOption { document: doc, key: burnburn_presentation::ParamKey::Columns, text: "2".into() });
    h.send(UiIntent::SetToolOption { document: doc, key: burnburn_presentation::ParamKey::Rows, text: "2".into() });
    h.drag(doc, (0.0, 0.0), (30.0, 40.0));
    h.invoke(doc, CommandKey::ToolSelect);
    h.send(UiIntent::Select { document: doc, ids: Vec::new(), mode: SelectMode::Replace });

    h.send(UiIntent::Files(FilesIntent::OpenExport { document: doc, format: ExportFormat::Svg }));
    assert!(export_panel(&h.view(), doc, Locale::En).unwrap().losses.contains("Working out what is lost"));
    h.run_tasks();
    let panel = export_panel(&h.view(), doc, Locale::En).unwrap();
    assert_eq!(panel.title, "Export as SVG");
    assert!(panel.losses.starts_with("The file cannot hold this:"), "{}", panel.losses);
    assert!(panel.losses.contains("1 modifiers are written as plain copies (confirmation needed)"), "{}", panel.losses);
    assert!(panel.show_acknowledge && !panel.acknowledged);
    assert!(!panel.can_export, "the button waits for the acknowledgement");
    assert!(panel.show_stroke && panel.show_decimals && !panel.show_curves);
    assert_eq!(panel.units.items, ["millimetres", "inches", "pixels (96 per inch)"]);
    assert_eq!(panel.layers.items, ["No groups", "One group per project layer"]);
    h.send(export_acknowledge_intent(doc, true));
    assert!(export_panel(&h.view(), doc, Locale::En).unwrap().can_export);

    // The controls map to options; an unchanged form sends nothing.
    let form = ExportForm {
        selection_only: false,
        unit: 1,
        layers: 1,
        use_colors: true,
        stroke: "0.1".into(),
        decimals: "2".into(),
        curves: 0,
        tolerance: "0.01".into(),
    };
    dispatch(&mut h, |vm| export_options_intent(doc, vm, &form));
    let panel = export_panel(&h.view(), doc, Locale::En).unwrap();
    assert_eq!((panel.units.selected, panel.layers.selected, panel.decimals.as_str()), (Some(1), Some(1), "2"));
    assert!(export_options_intent(doc, &h.view(), &form).is_none());
    let bad = ExportForm { stroke: "dik".into(), ..form.clone() };
    dispatch(&mut h, |vm| export_options_intent(doc, vm, &bad));
    let panel = export_panel(&h.view(), doc, Locale::En).unwrap();
    assert_eq!(panel.field_error, Some(burnburn_presentation::ExportField::Stroke));
    assert_eq!(panel.status, "An option holds an unusable value; the last good options stay in use.");
    assert!(export_options_intent(doc, &h.view(), &ExportForm { unit: 5, ..form }).is_none(), "no such unit item");

    h.send(export_write_intent(doc, PathBuf::from("/out/plaat.svg")));
    h.run_tasks();
    assert!(export_panel(&h.view(), doc, Locale::En).unwrap().status.starts_with("File written: /out/plaat.svg"));
    assert_eq!(export_file_name("Plaat.burnburn", ExportFormat::Dxf), "Plaat.dxf");
}

#[test]
fn templates_send_their_scope_and_the_recent_list_picks_by_position() {
    use burnburn_presentation::TemplateScope;
    use burnburn_types::StableId;
    let doc = <DocumentId as StableId>::from_raw(1);
    let path = PathBuf::from("/t/a.burnburntemplate");
    assert_eq!(
        template_save_intent(doc, path.clone(), 1),
        Some(UiIntent::Files(FilesIntent::SaveAsTemplate {
            document: doc,
            path: path.clone(),
            scope: TemplateScope::SettingsOnly
        }))
    );
    assert!(template_save_intent(doc, path.clone(), -1).is_none(), "no pop-up item, no template");
    assert_eq!(template_scope_titles(Locale::En).len(), TEMPLATE_SCOPES.len());
    let recent = [PathBuf::from("/t/a.burnburntemplate"), PathBuf::from("/t/b.burnburntemplate")];
    assert_eq!(
        recent_template_intent(1, &recent),
        Some(UiIntent::Files(FilesIntent::NewFromTemplate { path: recent[1].clone() }))
    );
    assert!(recent_template_intent(2, &recent).is_none(), "a stale menu entry does nothing");
}
