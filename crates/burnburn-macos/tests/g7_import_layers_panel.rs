//! The import sheet offers the layers a plotter or PDF file proposes (G7-trace, P04): a check
//! "create operations for these layers" that is shown only when the file proposes layers, sends one
//! intent and comes back checked; committing then makes the operations. Headless; the checkbox
//! itself is drawn by the hosts from this model.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path as FsPath, PathBuf};

use burnburn_io::fs::FileSystem;
use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::import_panel;
use burnburn_macos::panels_files::{import_layers_check_text, import_layers_intent};
use burnburn_presentation::{CommandKey, FilesIntent, HeadlessEditorHarness, UiIntent};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_types::DocumentId;

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

#[test]
fn the_check_is_there_for_a_plotter_file_and_sends_the_acceptance_which_comes_back_checked() {
    let bytes = read_fixture("import", "square-circle.plt").unwrap();
    let (mut h, doc) = preview("square-circle.plt", &bytes);
    let panel = import_panel(&h.view(), doc, Locale::En).unwrap();
    assert_eq!(panel.layers, Some(false), "shown, not checked");
    let intent = import_layers_intent(doc, &h.view(), true).expect("an intent");
    assert_eq!(intent, UiIntent::Files(FilesIntent::SetImportLayers { document: doc, create: true }));
    h.send(intent);
    assert_eq!(import_panel(&h.view(), doc, Locale::En).unwrap().layers, Some(true));
    h.send(import_layers_intent(doc, &h.view(), false).unwrap());
    assert_eq!(import_panel(&h.view(), doc, Locale::En).unwrap().layers, Some(false));

    // Accepted, committing makes one operation per pen and one undo step undoes it all.
    h.send(import_layers_intent(doc, &h.view(), true).unwrap());
    h.invoke(doc, CommandKey::CommitImport);
    let rows = h.document(doc).unwrap().operations;
    assert_eq!(rows.len(), 2, "Pen 1 and Pen 2");
    h.invoke(doc, CommandKey::Undo);
    assert!(h.document(doc).unwrap().operations.is_empty() && h.document(doc).unwrap().objects.is_empty());
}

#[test]
fn a_file_without_a_proposal_has_no_check_and_no_intent() {
    let bytes = read_fixture("import", "mm-plate.dxf").unwrap();
    let (h, doc) = preview("mm-plate.dxf", &bytes);
    assert_eq!(import_panel(&h.view(), doc, Locale::En).unwrap().layers, None);
    assert!(import_layers_intent(doc, &h.view(), true).is_none());
    // No import open at all.
    let mut empty = HeadlessEditorHarness::new();
    let doc = empty.new_document().unwrap();
    assert!(import_layers_intent(doc, &empty.view(), true).is_none());
}

#[test]
fn the_check_has_its_words_in_both_languages() {
    assert_eq!(import_layers_check_text(Locale::En), "Create an operation for each of these layers");
    assert_eq!(import_layers_check_text(Locale::Nl), "Maak voor elke van deze lagen een bewerking aan");
}
