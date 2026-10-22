//! W4-L8b family 3, headless: the Windows file sheets (open/recent, import report with page choice,
//! export with the loss report and its confirmation, templates, Material Library, Art Library and
//! bundles) show what the presentation says and send its intents. The view models come from the
//! real controller over an in-memory file system: no dialog, no disk of the user.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::HashSet;
use std::path::{Path as FsPath, PathBuf};

use burnburn_io::fs::FileSystem;
use burnburn_presentation::{
    BundleKind, CommandKey, ExportFormat, FilesIntent, HeadlessEditorHarness, HostEffect, ImportPages, OperationField,
    SelectMode, TemplateScope, UiIntent,
};
use burnburn_types::{DocumentId, StableId};
use burnburn_windows::host_panels::files::{
    FilesInput, Sheet, effect_dialog, files_spec, id, on_files_event, on_files_file_chosen, sheet_open,
};
use burnburn_windows::host_panels::{
    ControlKind, DialogRequest, FilePurpose, PanelEvent, PanelReaction, PanelSpec, PanelValues, notify,
};
use burnburn_windows::l10n::Locale;
use common::{click, event, texts};

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

fn rect_layer(h: &mut HeadlessEditorHarness) -> DocumentId {
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let object = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![object], mode: SelectMode::Replace });
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    let op = h.document(doc).unwrap().operations[0].id;
    h.send(UiIntent::SetOperationField {
        document: doc,
        operation: op,
        field: OperationField::Speed,
        text: "12".into(),
    });
    doc
}

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

struct Ctx {
    recent: Vec<PathBuf>,
    templates: Vec<PathBuf>,
}

fn input<'a>(
    h: &'a HeadlessEditorHarness,
    doc: DocumentId,
    ctx: &'a Ctx,
    vm: &'a burnburn_presentation::AppViewModel,
) -> FilesInput<'a> {
    let _ = h;
    FilesInput {
        document: doc,
        vm,
        locale: Locale::En,
        recent_projects: &ctx.recent,
        recent_templates: &ctx.templates,
        template_scope: 0,
        recent_row: None,
        template_row: None,
    }
}

fn ctx() -> Ctx {
    Ctx { recent: Vec::new(), templates: Vec::new() }
}

fn spec(h: &HeadlessEditorHarness, doc: DocumentId, sheet: Sheet) -> PanelSpec {
    let vm = h.view();
    files_spec(sheet, &input(h, doc, &ctx(), &vm))
}

fn react(h: &HeadlessEditorHarness, doc: DocumentId, sheet: Sheet, ev: PanelEvent) -> PanelReaction {
    let vm = h.view();
    on_files_event(sheet, &input(h, doc, &ctx(), &vm), &ev)
}

/// Sends what a reaction sends, then lets the workers finish.
fn run(h: &mut HeadlessEditorHarness, reaction: PanelReaction) {
    for i in reaction.intents() {
        h.send(i.clone());
    }
    h.run_tasks();
}

fn text(spec: &PanelSpec, cid: u16) -> String {
    spec.control(cid).unwrap_or_else(|| panic!("control {cid} missing")).text.clone()
}

fn combo(spec: &PanelSpec, cid: u16) -> (Vec<String>, Option<usize>) {
    match &spec.control(cid).unwrap().kind {
        ControlKind::Combo { items, selected } => (items.clone(), *selected),
        ControlKind::List { items, selected, .. } => (items.clone(), *selected),
        other => panic!("{other:?}"),
    }
}

fn checked(spec: &PanelSpec, cid: u16) -> bool {
    matches!(spec.control(cid).unwrap().kind, ControlKind::Check { checked: true })
}

fn sel(cid: u16, row: usize) -> PanelValues {
    let mut v = PanelValues::default();
    v.selection.insert(cid, Some(row));
    v
}

#[test]
fn ids_of_every_sheet_are_unique_and_inside_the_files_block() {
    let mut h = harness();
    let doc = rect_layer(&mut h);
    for intent in [
        UiIntent::Files(FilesIntent::OpenMaterialLibrary { document: doc }),
        UiIntent::Files(FilesIntent::OpenArtLibrary { document: doc }),
        UiIntent::Files(FilesIntent::OpenBundleExport { document: doc }),
        UiIntent::Files(FilesIntent::OpenExport { document: doc, format: ExportFormat::Svg }),
    ] {
        h.send(intent);
    }
    h.run_tasks();
    let mut all = HashSet::new();
    for sheet in Sheet::ALL {
        for c in spec(&h, doc, sheet).controls() {
            assert!((7400..7600).contains(&c.id), "{sheet:?} {}", c.id);
            assert!(all.insert(c.id), "duplicate id {} in {sheet:?}", c.id);
        }
    }
    assert!(Sheet::ALL.iter().all(|s| Sheet::from_code(s.code()) == Some(*s)));
}

#[test]
fn a_sheet_is_open_exactly_while_presentation_has_it_open() {
    let mut h = harness();
    let doc = rect_layer(&mut h);
    let vm = h.view();
    assert!(Sheet::ALL.iter().all(|s| !sheet_open(*s, &vm, doc)), "nothing open yet (Start is the user's choice)");
    h.send(UiIntent::Files(FilesIntent::OpenMaterialLibrary { document: doc }));
    h.run_tasks();
    assert!(sheet_open(Sheet::Material, &h.view(), doc));
    assert!(!sheet_open(Sheet::Art, &h.view(), doc));
    h.send(UiIntent::Files(FilesIntent::CloseMaterialLibrary { document: doc }));
    assert!(!sheet_open(Sheet::Material, &h.view(), doc));
}

// ---- import ------------------------------------------------------------------------------------

#[test]
fn a_pdf_import_offers_its_pages_and_the_choice_reaches_presentation() {
    let mut h = harness();
    let doc = h.new_document().unwrap();
    let path = put(&h, "/in/twee.pdf", &two_page_pdf());
    h.send(UiIntent::Import { document: doc, path });
    h.run_tasks();
    assert!(sheet_open(Sheet::Import, &h.view(), doc));
    let s = spec(&h, doc, Sheet::Import);
    let (pages, selected) = combo(&s, id::import::PAGES);
    assert_eq!(pages, ["Page 1 — 35.3 × 17.6 mm", "Page 2 — 70.6 × 35.3 mm", "All pages side by side"]);
    assert_eq!(selected, Some(0));
    assert!(s.control(id::import::PAGES).unwrap().visible);
    assert!(!s.control(id::import::UNIT).unwrap().visible, "a PDF has its own unit");
    assert!(s.control(id::import::COMMIT).unwrap().enabled);

    let reaction = react(&h, doc, Sheet::Import, event(id::import::PAGES, notify::CHANGED, sel(id::import::PAGES, 1)));
    assert!(matches!(
        reaction.intents().as_slice(),
        [UiIntent::Files(FilesIntent::SetImportPages { pages: ImportPages::One(1), .. })]
    ));
    run(&mut h, reaction);
    assert_eq!(combo(&spec(&h, doc, Sheet::Import), id::import::PAGES).1, Some(1));
    // Importeer commits, cancel closes the preview.
    assert_eq!(
        react(&h, doc, Sheet::Import, click(id::import::COMMIT, PanelValues::default())),
        PanelReaction::Intent(UiIntent::Invoke { document: Some(doc), command: CommandKey::CommitImport })
    );
    assert_eq!(
        react(&h, doc, Sheet::Import, event(id::import::COMMIT, notify::CLOSED, PanelValues::default())),
        PanelReaction::Intent(UiIntent::Invoke { document: Some(doc), command: CommandKey::CancelImport }),
        "closing the window cancels the preview"
    );
}

#[test]
fn a_plotter_file_reports_its_pens_and_an_svg_asks_for_its_unit() {
    let mut h = harness();
    let doc = h.new_document().unwrap();
    let path = put(&h, "/in/plaat.plt", b"IN;SP1;PU0,0;PD400,0,400,400,0,400,0,0;PU200,200;SP2;CI100;PU;SP0;");
    h.send(UiIntent::Import { document: doc, path });
    h.run_tasks();
    let s = spec(&h, doc, Sheet::Import);
    assert!(text(&s, id::import::REPORT).contains("Proposed layers:\n  Pen 1: 1 object\n  Pen 2: 1 object"));
    assert_eq!(text(&s, id::import::UNIT_PROMPT), "Plotter units (0.025 mm per step)");

    let svg = put(
        &h,
        "/in/v.svg",
        br#"<svg xmlns="http://www.w3.org/2000/svg" width="10mm" height="10mm" viewBox="0 0 10 10"><rect width="10" height="10" fill="none" stroke="black"/></svg>"#,
    );
    h.send(UiIntent::Import { document: doc, path: svg });
    h.run_tasks();
    let s = spec(&h, doc, Sheet::Import);
    assert!(s.control(id::import::UNIT).unwrap().visible, "SVG and DXF can be given a unit");
    let reaction = react(&h, doc, Sheet::Import, event(id::import::UNIT, notify::CHANGED, sel(id::import::UNIT, 2)));
    assert!(matches!(
        reaction.intents().as_slice(),
        [UiIntent::SetImportUnit { unit: burnburn_presentation::ImportUnit::Inch, .. }]
    ));
}

// ---- export ------------------------------------------------------------------------------------

#[test]
fn the_export_dialog_waits_for_the_acknowledgement_sends_the_whole_form_and_writes_via_a_save_dialog() {
    let mut h = harness();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (10.0, 10.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let source = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![source], mode: SelectMode::Replace });
    h.send(UiIntent::SelectTool { document: Some(doc), id: "sketch.pattern.rectangular".into() });
    h.send(UiIntent::SetToolOption { document: doc, key: burnburn_presentation::ParamKey::Columns, text: "2".into() });
    h.send(UiIntent::SetToolOption { document: doc, key: burnburn_presentation::ParamKey::Rows, text: "2".into() });
    h.drag(doc, (0.0, 0.0), (30.0, 40.0));
    h.invoke(doc, CommandKey::ToolSelect);
    h.send(UiIntent::Select { document: doc, ids: Vec::new(), mode: SelectMode::Replace });
    h.send(UiIntent::Files(FilesIntent::OpenExport { document: doc, format: ExportFormat::Svg }));
    h.run_tasks();

    let s = spec(&h, doc, Sheet::Export);
    assert_eq!(text(&s, id::export::TITLE), "Export as SVG");
    assert!(text(&s, id::export::LOSSES).contains("1 modifiers are written as plain copies (confirmation needed)"));
    assert!(s.control(id::export::ACKNOWLEDGE).unwrap().visible);
    assert!(!s.control(id::export::EXPORT).unwrap().enabled, "the button waits for the acknowledgement");
    assert!(s.control(id::export::STROKE).unwrap().visible && !s.control(id::export::CURVES).unwrap().visible);
    assert_eq!(combo(&s, id::export::UNIT).0, ["millimetres", "inches", "pixels (96 per inch)"]);
    // Export without the acknowledgement asks nothing.
    assert_eq!(
        react(&h, doc, Sheet::Export, click(id::export::EXPORT, PanelValues::default())),
        PanelReaction::Nothing
    );

    let mut ack = PanelValues::default();
    ack.checks.insert(id::export::ACKNOWLEDGE, true);
    let r = react(&h, doc, Sheet::Export, click(id::export::ACKNOWLEDGE, ack));
    assert_eq!(
        r,
        PanelReaction::Intent(UiIntent::Files(FilesIntent::SetExportAcknowledged {
            document: doc,
            acknowledged: true
        }))
    );
    run(&mut h, r);
    let s = spec(&h, doc, Sheet::Export);
    assert!(checked(&s, id::export::ACKNOWLEDGE) && s.control(id::export::EXPORT).unwrap().enabled);

    // An option change sends the whole form as typed; the same form again sends nothing.
    let mut form = PanelValues::default();
    form.selection.insert(id::export::UNIT, Some(1));
    form.selection.insert(id::export::LAYERS, Some(1));
    form.selection.insert(id::export::CURVES, Some(0));
    form.checks.insert(id::export::USE_COLORS, true);
    for (cid, t) in [(id::export::STROKE, "0.1"), (id::export::DECIMALS, "2"), (id::export::TOLERANCE, "0.01")] {
        form.texts.insert(cid, t.to_owned());
    }
    let r = react(&h, doc, Sheet::Export, event(id::export::UNIT, notify::CHANGED, form.clone()));
    assert!(
        matches!(r.intents().as_slice(), [UiIntent::Files(FilesIntent::SetExportOptions { options, .. })] if options.decimals == "2")
    );
    run(&mut h, r);
    assert_eq!(
        react(&h, doc, Sheet::Export, event(id::export::DECIMALS, notify::EDIT_DONE, form)),
        PanelReaction::Nothing
    );

    match react(&h, doc, Sheet::Export, click(id::export::EXPORT, PanelValues::default())) {
        PanelReaction::Dialog(DialogRequest::Save { purpose, suggested_name, filters, .. }) => {
            assert_eq!(purpose, FilePurpose::ExportDrawing { document: doc });
            assert_eq!(
                suggested_name,
                format!("{}.svg", h.view().document(doc).unwrap().title.trim_end_matches(".burnburn"))
            );
            assert_eq!(filters[0].extensions, vec!["svg"]);
            let vm = h.view();
            let r = on_files_file_chosen(&vm, &purpose, Some(PathBuf::from("/out/plaat.svg")));
            run(&mut h, r);
            assert!(h.fs().exists(FsPath::new("/out/plaat.svg")).unwrap(), "the file was written by presentation");
            assert_eq!(on_files_file_chosen(&vm, &purpose, None), PanelReaction::Nothing);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        react(&h, doc, Sheet::Export, event(id::export::CLOSE, notify::CLOSED, PanelValues::default())),
        PanelReaction::Intent(UiIntent::Files(FilesIntent::CloseExport { document: doc }))
    );
}

// ---- Material Library --------------------------------------------------------------------------

#[test]
fn the_material_sheet_stores_a_layer_filters_and_applies_with_its_buttons_gated() {
    let mut h = harness();
    let doc = rect_layer(&mut h);
    h.send(UiIntent::Files(FilesIntent::OpenMaterialLibrary { document: doc }));
    let loading = spec(&h, doc, Sheet::Material);
    assert_eq!(text(&loading, id::material::STATUS), "Reading the library…");
    assert!(!loading.control(id::material::SAVE).unwrap().enabled, "nothing acts while loading");
    h.run_tasks();

    // Store layer one as an entry.
    let mut values = texts(&[
        (id::material::NAME, "Snijden"),
        (id::material::NEW_MATERIAL, "Berken"),
        (id::material::STORE_THICKNESS, "3"),
    ]);
    values.selection.insert(id::material::LAYER, Some(0));
    let r = react(&h, doc, Sheet::Material, click(id::material::SAVE, values.clone()));
    assert!(
        matches!(r.intents().as_slice(), [UiIntent::Files(FilesIntent::SaveMaterial { name, material, thickness, .. })] if name == "Snijden" && material == "Berken" && thickness == "3")
    );
    run(&mut h, r);
    let s = spec(&h, doc, Sheet::Material);
    assert_eq!(combo(&s, id::material::ENTRIES).0, ["Snijden — Berken 3 mm (r1)"]);
    assert_eq!(combo(&s, id::material::MATERIAL).0, ["All materials", "Berken"]);
    assert_eq!(text(&s, id::material::DETAIL), "Line · 12 mm/s · 20 % · 1 pass(es)");
    assert!(s.control(id::material::APPLY).unwrap().enabled && s.control(id::material::REMOVE).unwrap().enabled);
    assert!(!s.control(id::material::UPDATE).unwrap().enabled, "a hand-made layer has nothing to update");

    // The search maps all three controls to one filter intent.
    let mut filter = texts(&[(id::material::THICKNESS, "6")]);
    filter.selection.insert(id::material::MATERIAL, Some(1));
    filter.selection.insert(id::material::MACHINE, Some(0));
    let r = react(&h, doc, Sheet::Material, event(id::material::THICKNESS, notify::EDIT_DONE, filter));
    assert!(
        matches!(r.intents().as_slice(), [UiIntent::Files(FilesIntent::SetMaterialFilter { material: Some(m), thickness, .. })] if m == "Berken" && thickness == "6")
    );

    // Apply needs the layer of the pop-up; remove asks nothing more but needs the entry.
    let mut apply = PanelValues::default();
    apply.selection.insert(id::material::LAYER, Some(0));
    let r = react(&h, doc, Sheet::Material, click(id::material::APPLY, apply));
    assert!(matches!(r.intents().as_slice(), [UiIntent::Files(FilesIntent::ApplyMaterial { .. })]));
    let r = react(&h, doc, Sheet::Material, click(id::material::REMOVE, PanelValues::default()));
    assert!(matches!(r.intents().as_slice(), [UiIntent::Files(FilesIntent::RemoveMaterial { .. })]));
    assert_eq!(
        react(&h, doc, Sheet::Material, click(id::material::CLOSE, PanelValues::default())),
        PanelReaction::Intent(UiIntent::Invoke { document: Some(doc), command: CommandKey::ShowMaterialLibrary })
    );
}

// ---- Art Library -------------------------------------------------------------------------------

const SQUARE_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10mm" height="10mm" viewBox="0 0 10 10"><rect x="0" y="0" width="10" height="10" fill="none" stroke="black"/></svg>"#;

#[test]
fn the_art_sheet_adds_a_file_via_a_dialog_shows_a_thumbnail_and_places_it() {
    let mut h = harness();
    let doc = h.new_document().unwrap();
    h.send(UiIntent::Files(FilesIntent::OpenArtLibrary { document: doc }));
    h.run_tasks();
    let file = put(&h, "/in/vierkant.svg", SQUARE_SVG.as_bytes());

    let mut v = texts(&[(id::art::NAME, "")]);
    v.selection.insert(id::art::UNIT, Some(0));
    let purpose = match react(&h, doc, Sheet::Art, click(id::art::ADD_FILE, v)) {
        PanelReaction::Dialog(DialogRequest::Open { purpose, filters, .. }) => {
            assert!(filters[0].extensions.contains(&"svg") && filters[0].extensions.contains(&"dxf"));
            purpose
        }
        other => panic!("{other:?}"),
    };
    let vm = h.view();
    let r = on_files_file_chosen(&vm, &purpose, Some(file));
    assert!(matches!(r, PanelReaction::Intents(ref v) if v.len() == 2), "the first file also creates the group: {r:?}");
    run(&mut h, r);
    let s = spec(&h, doc, Sheet::Art);
    assert_eq!(combo(&s, id::art::GROUP).0, ["All groups", "General (1)"]);
    assert_eq!(combo(&s, id::art::ITEMS).0, ["vierkant (General)"]);
    assert_eq!(text(&s, id::art::DETAIL), "Format SVG");
    match &s.control(id::art::THUMBNAIL).unwrap().kind {
        ControlKind::Bitmap { image: Some(img), .. } => assert_eq!((img.width, img.height), (64, 64)),
        other => panic!("{other:?}"),
    }
    assert!(s.control(id::art::PLACE).unwrap().enabled);
    assert!(!s.control(id::art::RENAME_GROUP).unwrap().enabled, "no group chosen");

    let r = react(&h, doc, Sheet::Art, click(id::art::PLACE, PanelValues::default()));
    run(&mut h, r);
    assert_eq!(h.document(doc).unwrap().objects.len(), 1);
    // The search sends once; the same text again nothing.
    let r = react(&h, doc, Sheet::Art, event(id::art::SEARCH, notify::ENTER, texts(&[(id::art::SEARCH, "nope")])));
    run(&mut h, r);
    assert_eq!(combo(&spec(&h, doc, Sheet::Art), id::art::ITEMS).0, ["No items"]);
    assert_eq!(
        react(&h, doc, Sheet::Art, event(id::art::SEARCH, notify::EDIT_DONE, texts(&[(id::art::SEARCH, "nope")]))),
        PanelReaction::Nothing
    );
}

// ---- bundles -----------------------------------------------------------------------------------

#[test]
fn the_bundle_export_gates_on_a_choice_toggles_kinds_and_writes_through_a_save_dialog() {
    let mut h = harness();
    let doc = rect_layer(&mut h);
    h.send(UiIntent::Files(FilesIntent::OpenMaterialLibrary { document: doc }));
    h.run_tasks();
    for name in ["Snijden", "Graveren"] {
        let mut v = texts(&[
            (id::material::NAME, name),
            (id::material::NEW_MATERIAL, "Berken"),
            (id::material::STORE_THICKNESS, "3"),
        ]);
        v.selection.insert(id::material::LAYER, Some(0));
        let r = react(&h, doc, Sheet::Material, click(id::material::SAVE, v));
        run(&mut h, r);
    }
    h.send(UiIntent::Files(FilesIntent::OpenBundleExport { document: doc }));
    h.run_tasks();
    let s = spec(&h, doc, Sheet::Bundle);
    assert_eq!(text(&s, id::bundle::TITLE), "Export bundle");
    assert_eq!(text(&s, id::bundle::KIND), "Material settings (0 of 2)");
    assert!(!s.control(id::bundle::PRIMARY).unwrap().enabled, "nothing chosen yet");
    assert!(!s.control(id::bundle::KIND + 1).unwrap().enabled, "no art to choose from");

    let r = react(&h, doc, Sheet::Bundle, click(id::bundle::KIND, PanelValues::default()));
    assert!(matches!(
        r.intents().as_slice(),
        [UiIntent::Files(FilesIntent::SetBundleKind { kind: BundleKind::Materials, selected: true, .. })]
    ));
    run(&mut h, r);
    let s = spec(&h, doc, Sheet::Bundle);
    assert!(checked(&s, id::bundle::KIND) && s.control(id::bundle::PRIMARY).unwrap().enabled);

    match react(&h, doc, Sheet::Bundle, click(id::bundle::PRIMARY, PanelValues::default())) {
        PanelReaction::Dialog(DialogRequest::Save { purpose, filters, .. }) => {
            assert_eq!(purpose, FilePurpose::BundleWrite { document: doc });
            assert_eq!(filters[0].extensions, vec!["burnburnbundle"]);
            let vm = h.view();
            let r = on_files_file_chosen(&vm, &purpose, Some(PathBuf::from("/out/a.burnburnbundle")));
            run(&mut h, r);
            assert!(h.fs().exists(FsPath::new("/out/a.burnburnbundle")).unwrap());
        }
        other => panic!("{other:?}"),
    }
}

// ---- start sheet, templates and the effects ----------------------------------------------------

#[test]
fn recent_projects_open_by_their_row_and_a_stale_row_does_nothing() {
    let mut h = harness();
    let doc = h.new_document().unwrap();
    let c = Ctx { recent: vec![PathBuf::from("C:/a.burnburn"), PathBuf::from("C:/b.burnburn")], templates: Vec::new() };
    let vm = h.view();
    let inp = input(&h, doc, &c, &vm);
    let s = files_spec(Sheet::Start, &inp);
    assert_eq!(combo(&s, id::start::RECENT).0, ["a.burnburn", "b.burnburn"]);
    let open = |row| on_files_event(Sheet::Start, &inp, &click(id::start::OPEN_RECENT, sel(id::start::RECENT, row)));
    assert_eq!(open(1), PanelReaction::Intent(UiIntent::Open { path: PathBuf::from("C:/b.burnburn") }));
    assert_eq!(open(5), PanelReaction::Nothing);
    assert_eq!(
        on_files_event(Sheet::Start, &inp, &click(id::start::CLEAR_RECENT, PanelValues::default())),
        PanelReaction::Intent(UiIntent::ClearRecentProjects)
    );
    match on_files_event(Sheet::Start, &inp, &click(id::start::OPEN, PanelValues::default())) {
        PanelReaction::Dialog(DialogRequest::Open { purpose, .. }) => {
            assert_eq!(purpose, FilePurpose::OpenProject);
            assert_eq!(
                on_files_file_chosen(&vm, &purpose, Some(PathBuf::from("C:/x.burnburn"))),
                PanelReaction::Intent(UiIntent::Open { path: PathBuf::from("C:/x.burnburn") })
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_start_sheet_buttons_invoke_the_file_commands_or_ask_for_a_file() {
    let mut h = harness();
    let doc = h.new_document().unwrap();
    let c = ctx();
    let vm = h.view();
    let inp = input(&h, doc, &c, &vm);
    for (button, command) in [
        (id::start::EXPORT_SVG, CommandKey::ExportSvg),
        (id::start::EXPORT_DXF, CommandKey::ExportDxf),
        (id::start::SAVE_TEMPLATE, CommandKey::SaveAsTemplate),
        (id::start::NEW_TEMPLATE, CommandKey::NewFromTemplate),
        (id::start::MATERIAL, CommandKey::ShowMaterialLibrary),
        (id::start::ART, CommandKey::ShowArtLibrary),
        (id::start::BUNDLE_EXPORT, CommandKey::ExportBundle),
        (id::start::BUNDLE_IMPORT, CommandKey::ImportBundle),
    ] {
        let enabled = vm.active == Some(doc) && vm.commands.get(&command).is_some_and(|s| s.enabled);
        let r = on_files_event(Sheet::Start, &inp, &click(button, PanelValues::default()));
        let want = enabled.then_some(UiIntent::Invoke { document: Some(doc), command });
        assert_eq!(r.intents().first().cloned().cloned(), want, "{command:?}");
    }
    match on_files_event(Sheet::Start, &inp, &click(id::start::IMPORT, PanelValues::default())) {
        PanelReaction::Dialog(DialogRequest::Open { purpose, filters, .. }) => {
            assert_eq!(purpose, FilePurpose::ImportDrawing { document: doc });
            assert_eq!(filters[0].extensions[0], "dxf", "drawings first for Import");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn template_effects_become_dialogs_and_the_answers_become_template_intents() {
    let mut h = harness();
    let doc = h.new_document().unwrap();
    let c = ctx();
    let vm = h.view();
    let mut inp = input(&h, doc, &c, &vm);
    inp.template_scope = 1;

    match effect_dialog(
        &HostEffect::ShowTemplateSavePanel { document: doc, suggested_name: "Vorm.burnburntemplate".into() },
        &inp,
    ) {
        Some(DialogRequest::Save { purpose, suggested_name, filters, .. }) => {
            assert_eq!(suggested_name, "Vorm.burnburntemplate");
            assert_eq!(filters[0].extensions, vec!["burnburntemplate"]);
            assert_eq!(purpose, FilePurpose::TemplateSave { document: doc, scope: TemplateScope::SettingsOnly });
            let path = PathBuf::from("C:/t/vorm.burnburntemplate");
            assert_eq!(
                on_files_file_chosen(&vm, &purpose, Some(path.clone())),
                PanelReaction::Intent(UiIntent::Files(FilesIntent::SaveAsTemplate {
                    document: doc,
                    path,
                    scope: TemplateScope::SettingsOnly
                }))
            );
        }
        other => panic!("{other:?}"),
    }
    match effect_dialog(&HostEffect::ShowTemplateOpenPanel, &inp) {
        Some(DialogRequest::Open { purpose, .. }) => {
            let path = PathBuf::from("C:/t/a.burnburntemplate");
            assert_eq!(
                on_files_file_chosen(&vm, &purpose, Some(path.clone())),
                PanelReaction::Intent(UiIntent::Files(FilesIntent::NewFromTemplate { path }))
            );
            assert_eq!(on_files_file_chosen(&vm, &purpose, None), PanelReaction::Nothing);
        }
        other => panic!("{other:?}"),
    }
    match effect_dialog(&HostEffect::ShowBundleOpenPanel { document: doc }, &inp) {
        Some(DialogRequest::Open { purpose, filters, .. }) => {
            assert_eq!(filters[0].extensions, vec!["burnburnbundle"]);
            let path = PathBuf::from("C:/b.burnburnbundle");
            assert_eq!(
                on_files_file_chosen(&vm, &purpose, Some(path.clone())),
                PanelReaction::Intent(UiIntent::Files(FilesIntent::OpenBundleImport { document: doc, path }))
            );
        }
        other => panic!("{other:?}"),
    }
    match effect_dialog(&HostEffect::ShowExportPanel { document: doc, suggested_name: "Plaat.rd".into() }, &inp) {
        Some(DialogRequest::Save { purpose, suggested_name, .. }) => {
            assert_eq!((suggested_name.as_str(), &purpose), ("Plaat.rd", &FilePurpose::ExportRd { document: doc }));
            let path = PathBuf::from("C:/o/plaat.rd");
            assert_eq!(
                on_files_file_chosen(&vm, &purpose, Some(path.clone())),
                PanelReaction::Intent(UiIntent::ExportRd { document: doc, path })
            );
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(effect_dialog(&HostEffect::Redraw { document: doc }, &inp), None);
    let _ = (DocumentId::from_raw(1), PanelEvent::default());
}
