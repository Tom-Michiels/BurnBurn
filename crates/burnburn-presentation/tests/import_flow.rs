//! B6 / P03: choose a file → import report → required unit decision → commit (one entry);
//! cancel at any moment before the commit changes nothing. Fixtures from `test-data/import`.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path as FsPath, PathBuf};

use burnburn_document::{EditLabel, UndoLabel};
use burnburn_io::fs::FileSystem;
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, ImportUnit, MessageKey, StatusMessage, UiIntent};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_types::DocumentId;

fn with_file(name: &str, bytes: &[u8]) -> (HeadlessEditorHarness, DocumentId, PathBuf) {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(FsPath::new("/in"));
    let path = PathBuf::from(format!("/in/{name}"));
    let temp = h.fs().create_temp_in(FsPath::new("/in"), ".fixture").unwrap();
    h.fs().write_all(&temp, bytes).unwrap();
    h.fs().replace(&temp, &path).unwrap();
    let doc = h.new_document().expect("window");
    (h, doc, path)
}

fn fixture(name: &str) -> (HeadlessEditorHarness, DocumentId, PathBuf) {
    with_file(name, &read_fixture("import", name).expect("verified fixture"))
}

fn close(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
}

/// 4 × 2 drawing units without $INSUNITS: no import before an explicit unit; the preview size
/// follows the choice (inch → 101,6 × 50,8 mm, mm → 4 × 2 mm).
#[test]
fn unitless_dxf_needs_explicit_scale_before_import_commit() {
    let (mut h, doc, path) = fixture("unitless-lines.dxf");
    h.send(UiIntent::Import { document: doc, path });
    h.run_tasks();
    let import = h.document(doc).unwrap().import.expect("import preview");
    assert!(import.needs_unit);
    assert_eq!((import.unit, import.size_mm), (None, None));
    assert!(!h.view().commands[&CommandKey::CommitImport].enabled, "Importeer disabled until a choice");
    h.invoke(doc, CommandKey::CommitImport);
    assert!(h.document(doc).unwrap().objects.is_empty());

    h.send(UiIntent::SetImportUnit { document: doc, unit: ImportUnit::Inch });
    let import = h.document(doc).unwrap().import.unwrap();
    assert!(close(import.size_mm.unwrap(), (101.6, 50.8)), "{:?}", import.size_mm);
    h.send(UiIntent::SetImportUnit { document: doc, unit: ImportUnit::Millimeter });
    assert!(close(h.document(doc).unwrap().import.unwrap().size_mm.unwrap(), (4.0, 2.0)));
    assert!(h.view().commands[&CommandKey::CommitImport].enabled);

    h.invoke(doc, CommandKey::CommitImport);
    let vm = h.document(doc).unwrap();
    assert_eq!(vm.objects.len(), 2);
    assert_eq!(vm.undo, Some(UndoLabel::Edit(EditLabel::Import)));
    assert!(vm.import.is_none());
    h.invoke(doc, CommandKey::Undo);
    assert!(h.document(doc).unwrap().objects.is_empty(), "one undo removes the whole import");
}

/// The report lists what was not imported, with its source reference; cancelling changes nothing.
#[test]
fn cancelled_import_changes_nothing() {
    let (mut h, doc, path) = fixture("mm-plate.dxf");
    let before = h.document(doc).unwrap();
    h.send(UiIntent::Import { document: doc, path });
    h.run_tasks();
    let import = h.document(doc).unwrap().import.expect("preview");
    assert!(!import.needs_unit, "declared mm");
    assert!(import.skipped.iter().any(|s| s.entity.contains("2E")), "TEXT is reported: {:?}", import.skipped);
    assert_eq!(import.open_contours, 1, "the open arc is reported");
    h.invoke(doc, CommandKey::CancelImport);
    let after = h.document(doc).unwrap();
    assert!(after.import.is_none());
    assert_eq!((after.revision, after.dirty, after.objects.len()), (before.revision, before.dirty, 0));
}

/// SVG with physical size: no unit question; the commit is one entry.
#[test]
fn svg_import_reports_skipped_elements_and_commits_in_one_entry() {
    let (mut h, doc, path) = fixture("mm-viewbox.svg");
    h.send(UiIntent::Import { document: doc, path });
    h.run_tasks();
    let import = h.document(doc).unwrap().import.expect("preview");
    assert!(!import.needs_unit);
    assert!(close(import.size_mm.unwrap(), (100.0, 50.0)), "{:?}", import.size_mm);
    assert_eq!(import.skipped.len(), 2, "text and hidden ellipse: {:?}", import.skipped);
    h.invoke(doc, CommandKey::CommitImport);
    let vm = h.document(doc).unwrap();
    assert_eq!(vm.objects.len(), import.items);
    assert_eq!(vm.undo, Some(UndoLabel::Edit(EditLabel::Import)));
}

/// A damaged file: message, nothing imported, no preview.
#[test]
fn damaged_import_reports_error_and_imports_nothing() {
    let (mut h, doc, path) = with_file("kapot.dxf", b"0\nSECTION\n2\nENTITIES\n0\nLINE\n10\nnan");
    h.send(UiIntent::Import { document: doc, path });
    h.run_tasks();
    let vm = h.document(doc).unwrap();
    assert!(vm.import.is_none());
    assert!(vm.objects.is_empty());
    assert!(vm.messages.contains(&StatusMessage { key: MessageKey::ImportFailed }));
}
