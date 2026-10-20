//! W3-UI-bestanden, import (P03): HP-GL/PLT, PDF/AI with a page choice and LightBurn projects in the
//! existing import flow. Fixtures: `test-data/import` and `research/lightburn/fixtures`; every
//! expectation is derived by hand from the file's text.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod common;

use std::path::{Path as FsPath, PathBuf};

use burnburn_document::{EditLabel, UndoLabel};
use burnburn_io::fs::FileSystem;
use burnburn_presentation::{
    CommandKey, FilesIntent, HeadlessEditorHarness, ImportKind, ImportPages, MessageKey, StatusMessage, UiIntent,
};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_types::DocumentId;

const MM: f64 = 25.4 / 72.0;

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

fn preview(h: &mut HeadlessEditorHarness, doc: DocumentId, path: PathBuf) {
    h.send(UiIntent::Import { document: doc, path });
    h.run_tasks();
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// A classic two-page PDF: page 1 is 100 × 50 pt with a 40 × 20 pt box at the corner, page 2 is
/// 200 × 100 pt with one line of 100 pt; built by hand, independent of the importer.
fn two_page_pdf() -> Vec<u8> {
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

fn stream(data: &str) -> Vec<u8> {
    format!("<< /Length {} >>\nstream\n{data}\nendstream", data.len()).into_bytes()
}

/// HP-GL: millimetre sizes come from the plotter units (0,025 mm); the report names what was
/// skipped; the pen is the item's name; one undo entry removes the whole import.
#[test]
fn hpgl_import_previews_pens_and_the_report_and_commits_in_one_entry() {
    let bytes = read_fixture("import", "square-circle.plt").expect("verified fixture");
    let (mut h, doc, path) = with_file("square-circle.plt", &bytes);
    preview(&mut h, doc, path);
    let import = h.document(doc).unwrap().import.expect("preview");
    assert_eq!(import.kind, ImportKind::Hpgl);
    assert!(!import.needs_unit, "plotter units are known");
    assert_eq!(import.items, 3, "square, circle and the arc run");
    assert_eq!(import.skipped.len(), 1);
    assert_eq!(import.skipped[0].kind, "LB", "the label is reported, not drawn");
    let (w, hgt) = import.size_mm.expect("size");
    assert!(w >= 20.0 && hgt >= 10.0, "square of 10 mm and an arc run to x = 20 mm: {w} x {hgt}");
    let names: Vec<_> = import.layers.iter().map(|l| (l.name.as_str(), l.items)).collect();
    assert_eq!(names, [("Pen 1", 1), ("Pen 2", 2)], "SP1 draws the square, SP2 the circle and the arc run");
    assert!(import.pages.is_empty() && import.lbrn2.is_none());

    h.invoke(doc, CommandKey::CommitImport);
    let vm = h.document(doc).unwrap();
    assert_eq!(vm.objects.len(), 3);
    assert_eq!(vm.undo, Some(UndoLabel::Edit(EditLabel::Import)));
    assert!(vm.import.is_none());
    h.invoke(doc, CommandKey::Undo);
    assert!(h.document(doc).unwrap().objects.is_empty());
}

/// PDF: every page is listed with its physical size; the choice decides what the preview and the
/// commit contain; "all pages" places them side by side.
#[test]
fn pdf_import_lists_the_pages_and_the_choice_decides_what_is_committed() {
    let (mut h, doc, path) = with_file("two.pdf", &two_page_pdf());
    preview(&mut h, doc, path);
    let import = h.document(doc).unwrap().import.expect("preview");
    assert_eq!(import.kind, ImportKind::Pdf);
    assert_eq!(import.pages.len(), 2);
    assert!(close(import.pages[0].width_mm, 100.0 * MM) && close(import.pages[0].height_mm, 50.0 * MM));
    assert!(close(import.pages[1].width_mm, 200.0 * MM) && close(import.pages[1].height_mm, 100.0 * MM));
    assert_eq!(import.page_choice, Some(ImportPages::One(0)), "the first page is the default");
    assert!(!import.needs_unit && !import.busy);
    let (w, _) = import.size_mm.unwrap();
    assert!(close(w, 40.0 * MM), "page 1 holds the 40 pt box: {w}");

    h.send(UiIntent::Files(FilesIntent::SetImportPages { document: doc, pages: ImportPages::One(1) }));
    assert!(h.document(doc).unwrap().import.unwrap().busy, "the page is read on a worker");
    h.run_tasks();
    let import = h.document(doc).unwrap().import.unwrap();
    assert!(!import.busy);
    assert_eq!(import.page_choice, Some(ImportPages::One(1)));
    assert!(close(import.size_mm.unwrap().0, 100.0 * MM), "page 2 holds the 100 pt line");

    h.send(UiIntent::Files(FilesIntent::SetImportPages { document: doc, pages: ImportPages::All }));
    h.run_tasks();
    let import = h.document(doc).unwrap().import.unwrap();
    assert_eq!(import.items, 2, "box and line");
    // Page 1 is 100 pt wide, a 10 mm gap, then the line of page 2 starts at 100 pt + 10 mm.
    assert!(close(import.size_mm.unwrap().0, 100.0 * MM + 10.0 + 100.0 * MM), "{:?}", import.size_mm);

    h.invoke(doc, CommandKey::CommitImport);
    let vm = h.document(doc).unwrap();
    assert_eq!(vm.objects.len(), 2);
    assert_eq!(vm.undo, Some(UndoLabel::Edit(EditLabel::Import)));
}

/// A page that does not exist (a stale host list) changes nothing; the commit waits for a result
/// that belongs to the newest choice.
#[test]
fn an_unknown_page_is_ignored_and_a_late_result_of_an_older_choice_is_dropped() {
    let (mut h, doc, path) = with_file("two.pdf", &two_page_pdf());
    preview(&mut h, doc, path);
    h.send(UiIntent::Files(FilesIntent::SetImportPages { document: doc, pages: ImportPages::One(7) }));
    let import = h.document(doc).unwrap().import.unwrap();
    assert_eq!((import.page_choice, import.busy), (Some(ImportPages::One(0)), false));

    h.send(UiIntent::Files(FilesIntent::SetImportPages { document: doc, pages: ImportPages::One(1) }));
    let older = h.take_task_requests();
    h.send(UiIntent::Files(FilesIntent::SetImportPages { document: doc, pages: ImportPages::All }));
    let newer = h.take_task_requests();
    for request in older.into_iter().chain(newer) {
        h.complete_despite_cancel(request);
    }
    let import = h.document(doc).unwrap().import.unwrap();
    assert_eq!(import.page_choice, Some(ImportPages::All));
    assert_eq!(import.items, 2, "the result of the older choice did not overwrite the newer one");
}

/// A legacy PostScript Illustrator file is refused with its own reason, nothing is imported.
#[test]
fn a_legacy_illustrator_file_is_refused_with_the_reason() {
    let (mut h, doc, path) = with_file("old.ai", b"%!PS-Adobe-3.0\n%%Creator: Adobe Illustrator(TM) 3.0\n");
    preview(&mut h, doc, path);
    let vm = h.document(doc).unwrap();
    assert!(vm.import.is_none() && vm.objects.is_empty());
    assert!(
        vm.messages.contains(&StatusMessage {
            key: MessageKey::Files(burnburn_presentation::FilesMessage::ImportLegacyIllustrator)
        }),
        "{:?}",
        vm.messages
    );
}

fn research(name: &str) -> Vec<u8> {
    std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../research/lightburn/fixtures").join(name))
        .unwrap()
}

/// LightBurn project: the layer report names what the process model did not take over (with the
/// value it had), and the project-level warnings come before anything is imported.
#[test]
fn lbrn2_preview_reports_each_layer_with_its_unmapped_values_and_the_project_warnings() {
    let (mut h, doc, path) = with_file("rectangle-fill.lbrn2", &research("rectangle-fill.lbrn2"));
    preview(&mut h, doc, path);
    let import = h.document(doc).unwrap().import.expect("preview");
    assert_eq!(import.kind, ImportKind::Lbrn2);
    assert!(!import.needs_unit && import.units.is_none(), "LightBurn files are in millimetres");
    let report = import.lbrn2.expect("the project report");
    assert_eq!((report.shapes, import.items), (1, 1));
    assert_eq!(report.layers.len(), 1);
    let layer = &report.layers[0];
    assert_eq!((layer.index, layer.name.as_str(), layer.shapes, layer.taken), (0, "C00", 1, true));
    assert!(layer.unmapped.iter().any(|u| u.name == "maxPower2" && u.value.as_deref() == Some("20")));
    assert!(layer.unmapped.iter().any(|u| u.name == "priority" && u.value.as_deref() == Some("0")));
    assert!(!layer.unmapped.iter().any(|u| u.name == "speed"), "a mapped setting is not repeated");
    use burnburn_presentation::Lbrn2Warning;
    assert!(
        report.warnings.contains(&Lbrn2Warning::OtherDevice { file: "Ruida".into(), active: None }),
        "{:?}",
        report.warnings
    );
    assert!(report.warnings.contains(&Lbrn2Warning::MirroredOrigin { x: true, y: true }));
}

/// A profile whose id names the device is the same device: no device warning.
#[test]
fn lbrn2_device_warning_goes_away_when_the_active_profile_is_that_device() {
    let (mut h, doc, path) = with_file("rectangle-fill.lbrn2", &research("rectangle-fill.lbrn2"));
    let profile = burnburn_job::MachineProfile::new(
        "ruida-6445",
        1,
        burnburn_job::MachineFrame::new(600.0, 400.0, burnburn_job::OriginCorner::BottomLeft).unwrap(),
        1000.0,
        400.0,
        burnburn_job::PowerRatio::new(0.0).unwrap(),
        burnburn_job::PowerRatio::new(1.0).unwrap(),
    )
    .unwrap();
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile), ruida: common::RUIDA });
    preview(&mut h, doc, path);
    let report = h.document(doc).unwrap().import.unwrap().lbrn2.unwrap();
    assert!(!report.warnings.iter().any(|w| matches!(w, burnburn_presentation::Lbrn2Warning::OtherDevice { .. })));
}

/// The commit creates the shapes and one operation per layer; the operation carries the layer's
/// settings, and its output is switched off until the user has looked at it.
#[test]
fn lbrn2_commit_creates_the_shapes_and_the_layer_operations() {
    let (mut h, doc, path) = with_file("rectangle-fill.lbrn2", &research("rectangle-fill.lbrn2"));
    preview(&mut h, doc, path);
    h.invoke(doc, CommandKey::CommitImport);
    let vm = h.document(doc).unwrap();
    assert!(vm.import.is_none());
    assert_eq!(vm.objects.len(), 1);
    assert_eq!(vm.operations.len(), 1);
    let op = &vm.operations[0];
    assert_eq!((op.name.as_str(), op.targets), ("C00", 1));
    assert_eq!(op.speed, "100", "speed 100 mm/s from the file");
    assert!(!op.output_enabled, "an imported operation starts disabled");
}

/// An embedded bitmap becomes an image object at its frame, with the image operation of its layer.
#[test]
fn lbrn2_embedded_bitmap_becomes_an_image_object_with_its_image_operation() {
    let (mut h, doc, path) = with_file("gray-ramp-grayscale.lbrn2", &research("gray-ramp-grayscale.lbrn2"));
    preview(&mut h, doc, path);
    let report = h.document(doc).unwrap().import.unwrap().lbrn2.unwrap();
    assert_eq!(report.images.len(), 1);
    assert_eq!(report.images[0].state, burnburn_presentation::Lbrn2ImageState::Embedded);
    assert_eq!(report.images[0].cut_index, Some(0));
    h.invoke(doc, CommandKey::CommitImport);
    let vm = h.document(doc).unwrap();
    assert_eq!(vm.objects.len(), 1, "the frame of the bitmap");
    assert_eq!(vm.operations.len(), 1);
    assert_eq!(vm.operations[0].kind, burnburn_presentation::OperationKind::Image);
    assert_eq!(vm.operations[0].targets, 1);
    assert!(!vm.operations[0].output_enabled);
}

/// A damaged bitmap is reported as unreadable and is not imported; the rest still is.
#[test]
fn lbrn2_unreadable_bitmap_is_reported_and_skipped() {
    let xml = br#"<LightBurnProject><CutSetting type="Cut"><index Value="0"/><name Value="C00"/><maxPower Value="50"/><speed Value="10"/></CutSetting><Shape Type="Rect" CutIndex="0" W="10" H="5"><XForm>1 0 0 1 0 0</XForm></Shape><Shape Type="Bitmap" CutIndex="0" W="5" H="2" File="/x/y.png" Data="AAAA"><XForm>1 0 0 1 0 0</XForm></Shape></LightBurnProject>"#;
    let (mut h, doc, path) = with_file("mixed.lbrn2", xml);
    preview(&mut h, doc, path);
    let report = h.document(doc).unwrap().import.unwrap().lbrn2.unwrap();
    assert_eq!(report.images[0].state, burnburn_presentation::Lbrn2ImageState::Unreadable);
    h.invoke(doc, CommandKey::CommitImport);
    assert_eq!(h.document(doc).unwrap().objects.len(), 1, "only the rectangle");
}
