//! G7-trace, P04: a PDF or plotter file proposes one layer per stroke colour, fill colour or pen. When
//! the user accepts the proposal the import creates one operation per layer, targeting that layer's
//! objects, in the same single undo step as the objects themselves. Without the acceptance nothing
//! changes: the import is the objects alone, as before.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod common;

use std::path::{Path as FsPath, PathBuf};

use burnburn_document::process::OperationMode;
use burnburn_document::{CompoundLabel, EditLabel, UndoLabel};
use burnburn_io::fs::FileSystem;
use burnburn_io::import::ImportLayer;
use burnburn_io::{Project, load_project};
use burnburn_presentation::{CommandKey, FilesIntent, HeadlessEditorHarness, ImportKind, UiIntent};
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

fn preview(h: &mut HeadlessEditorHarness, doc: DocumentId, path: PathBuf) {
    h.send(UiIntent::Import { document: doc, path });
    h.run_tasks();
}

fn accept_layers(h: &mut HeadlessEditorHarness, doc: DocumentId, create: bool) {
    h.send(UiIntent::Files(FilesIntent::SetImportLayers { document: doc, create }));
}

fn saved(h: &mut HeadlessEditorHarness, doc: DocumentId) -> Project {
    let path = PathBuf::from("/out/p.burnburn");
    h.fs().create_dir_all_for_test(FsPath::new("/out"));
    h.send(UiIntent::Save { document: doc, path: Some(path.clone()) });
    h.run_tasks();
    load_project(h.fs(), &path).expect("saved project")
}

/// (name, kind is a line, number of targets) per operation, in order.
fn operations(p: &Project) -> Vec<(String, bool, usize)> {
    p.document
        .process()
        .operations()
        .map(|op| (op.name().to_owned(), matches!(op.mode(), OperationMode::Line { .. }), op.targets().len()))
        .collect()
}

fn stream(data: &str) -> Vec<u8> {
    format!("<< /Length {} >>\nstream\n{data}\nendstream", data.len()).into_bytes()
}

/// One page of 100 × 100 pt: a red line and a red box, a blue line, and a black filled box.
fn colour_pdf() -> Vec<u8> {
    let content = "1 0 0 RG 0 0 m 50 0 l S 1 0 0 RG 0 10 20 20 re S 0 0 1 RG 0 60 m 50 60 l S 0 g 60 60 20 20 re f";
    let objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R >>".to_vec(),
        stream(content),
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

#[test]
fn the_preview_names_each_layer_with_its_colour_or_pen_and_nothing_is_created_until_it_is_accepted() {
    let (mut h, doc, path) = with_file("colours.pdf", &colour_pdf());
    preview(&mut h, doc, path);
    let import = h.document(doc).unwrap().import.expect("preview");
    assert_eq!(import.kind, ImportKind::Pdf);
    let rows: Vec<_> = import.layers.iter().map(|l| (l.layer, l.items)).collect();
    assert_eq!(
        rows,
        [
            (Some(ImportLayer::Stroke([255, 0, 0])), 2),
            (Some(ImportLayer::Stroke([0, 0, 255])), 1),
            (Some(ImportLayer::Fill([0, 0, 0])), 1)
        ]
    );
    assert!(!import.layers_accepted, "the proposal is only a proposal");

    // Not accepted: the objects alone, as before; no operations, the plain import is one undo step.
    h.invoke(doc, CommandKey::CommitImport);
    let vm = h.document(doc).unwrap();
    assert_eq!(vm.objects.len(), 4);
    assert_eq!(vm.undo, Some(UndoLabel::Edit(EditLabel::Import)));
    assert!(operations(&saved(&mut h, doc)).is_empty());
}

#[test]
fn an_accepted_pdf_proposal_creates_one_operation_per_colour_in_one_undo_step() {
    let (mut h, doc, path) = with_file("colours.pdf", &colour_pdf());
    preview(&mut h, doc, path);
    accept_layers(&mut h, doc, true);
    assert!(h.document(doc).unwrap().import.unwrap().layers_accepted);
    h.invoke(doc, CommandKey::CommitImport);

    let vm = h.document(doc).unwrap();
    assert_eq!(vm.objects.len(), 4);
    assert!(vm.import.is_none());
    assert_eq!(vm.undo, Some(UndoLabel::Compound(CompoundLabel::ImportLayers)), "objects and layers are one step");
    // Strokes become cut lines, the fill a fill; each operation holds the objects of its colour.
    let ops = operations(&saved(&mut h, doc));
    assert_eq!(
        ops,
        [
            ("stroke #ff0000".to_owned(), true, 2),
            ("stroke #0000ff".to_owned(), true, 1),
            ("fill #000000".to_owned(), false, 1)
        ]
    );

    // One undo takes the objects and the operations away together; redo brings both back.
    h.invoke(doc, CommandKey::Undo);
    assert!(h.document(doc).unwrap().objects.is_empty());
    assert!(operations(&saved(&mut h, doc)).is_empty());
    h.invoke(doc, CommandKey::Redo);
    assert_eq!(h.document(doc).unwrap().objects.len(), 4);
    assert_eq!(operations(&saved(&mut h, doc)).len(), 3);
}

#[test]
fn a_plotter_file_proposes_its_pens_and_accepting_makes_a_line_operation_for_each() {
    let bytes = read_fixture("import", "square-circle.plt").expect("verified fixture");
    let (mut h, doc, path) = with_file("square-circle.plt", &bytes);
    preview(&mut h, doc, path);
    let import = h.document(doc).unwrap().import.expect("preview");
    let rows: Vec<_> = import.layers.iter().map(|l| (l.layer, l.items)).collect();
    assert_eq!(rows, [(Some(ImportLayer::Pen(1)), 1), (Some(ImportLayer::Pen(2)), 2)]);
    accept_layers(&mut h, doc, true);
    h.invoke(doc, CommandKey::CommitImport);
    let ops = operations(&saved(&mut h, doc));
    assert_eq!(ops, [("Pen 1".to_owned(), true, 1), ("Pen 2".to_owned(), true, 2)]);
}

#[test]
fn the_acceptance_can_be_taken_back_and_formats_without_a_proposal_ignore_it() {
    let (mut h, doc, path) = with_file("colours.pdf", &colour_pdf());
    preview(&mut h, doc, path);
    accept_layers(&mut h, doc, true);
    accept_layers(&mut h, doc, false);
    assert!(!h.document(doc).unwrap().import.unwrap().layers_accepted);
    h.invoke(doc, CommandKey::CommitImport);
    assert!(operations(&saved(&mut h, doc)).is_empty());

    // A DXF has layers of its own and no proposal; accepting does nothing and commits as usual.
    let bytes = read_fixture("import", "mm-plate.dxf").expect("verified fixture");
    let (mut h, doc, path) = with_file("mm-plate.dxf", &bytes);
    preview(&mut h, doc, path);
    let import = h.document(doc).unwrap().import.expect("preview");
    assert!(import.layers.is_empty());
    accept_layers(&mut h, doc, true);
    assert!(!h.document(doc).unwrap().import.unwrap().layers_accepted);
    h.invoke(doc, CommandKey::CommitImport);
    assert!(operations(&saved(&mut h, doc)).is_empty());
}

#[test]
fn importing_the_same_file_twice_with_layers_adds_the_operations_again_without_trouble() {
    let (mut h, doc, path) = with_file("colours.pdf", &colour_pdf());
    for _ in 0..2 {
        preview(&mut h, doc, path.clone());
        accept_layers(&mut h, doc, true);
        h.invoke(doc, CommandKey::CommitImport);
    }
    assert_eq!(h.document(doc).unwrap().objects.len(), 8);
    assert_eq!(operations(&saved(&mut h, doc)).len(), 6);
}
