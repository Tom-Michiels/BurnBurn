//! The import sheet on Windows offers the layers a plotter or PDF file proposes (G7-trace, P04),
//! headless: a check that is there only when the file proposes layers, sends one intent, comes back
//! checked and makes the operations when the import is committed. Same model as the macOS sheet.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::{Path as FsPath, PathBuf};

use burnburn_io::fs::FileSystem;
use burnburn_presentation::{CommandKey, FilesIntent, HeadlessEditorHarness, UiIntent};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_types::DocumentId;
use burnburn_windows::host_panels::files::{FilesInput, Sheet, files_spec, id, on_files_event};
use burnburn_windows::host_panels::{ControlKind, PanelReaction, PanelSpec, PanelValues, notify};
use burnburn_windows::l10n::Locale;
use common::event;

fn preview(file: &str, bytes: &[u8]) -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(FsPath::new("/in"));
    let path = PathBuf::from(format!("/in/{file}"));
    h.fs().write_all(&path, bytes).unwrap();
    let doc = h.new_document().unwrap();
    h.send(UiIntent::Import { document: doc, path });
    h.run_tasks();
    (h, doc)
}

fn input<'a>(doc: DocumentId, vm: &'a burnburn_presentation::AppViewModel) -> FilesInput<'a> {
    FilesInput {
        document: doc,
        vm,
        locale: Locale::En,
        recent_projects: &[],
        recent_templates: &[],
        template_scope: 0,
        recent_row: None,
        template_row: None,
    }
}

fn spec(h: &HeadlessEditorHarness, doc: DocumentId) -> PanelSpec {
    let vm = h.view();
    files_spec(Sheet::Import, &input(doc, &vm))
}

#[test]
fn the_check_is_there_for_a_plotter_file_sends_the_acceptance_and_comes_back_checked() {
    let bytes = read_fixture("import", "square-circle.plt").unwrap();
    let (mut h, doc) = preview("square-circle.plt", &bytes);
    let s = spec(&h, doc);
    let check = s.control(id::import::LAYERS).expect("the check");
    assert!(check.visible);
    assert_eq!(check.text, "Create an operation for each of these layers");
    assert!(matches!(check.kind, ControlKind::Check { checked: false }));

    let mut values = PanelValues::default();
    values.checks.insert(id::import::LAYERS, true);
    let vm = h.view();
    let reaction = on_files_event(Sheet::Import, &input(doc, &vm), &event(id::import::LAYERS, notify::CLICKED, values));
    let PanelReaction::Intent(intent) = reaction else { panic!("an intent: {reaction:?}") };
    assert_eq!(intent, UiIntent::Files(FilesIntent::SetImportLayers { document: doc, create: true }));
    h.send(intent);
    assert!(matches!(spec(&h, doc).control(id::import::LAYERS).unwrap().kind, ControlKind::Check { checked: true }));

    h.invoke(doc, CommandKey::CommitImport);
    assert_eq!(h.document(doc).unwrap().operations.len(), 2, "one operation per pen");
}

#[test]
fn a_file_without_a_proposal_hides_the_check() {
    let bytes = read_fixture("import", "mm-plate.dxf").unwrap();
    let (h, doc) = preview("mm-plate.dxf", &bytes);
    assert!(!spec(&h, doc).control(id::import::LAYERS).expect("the control exists").visible);
}
