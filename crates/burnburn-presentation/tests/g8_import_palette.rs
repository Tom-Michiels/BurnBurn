//! G8-trace, P04: the layers an import proposes (a colour, a pen) are tied to the nearest colour of
//! the layer palette, deterministically. Distance is the squared distance in sRGB; ties go to the
//! lower slot; every layer gets its own slot (the nearest one nobody has yet), so two reds stay two
//! layers. Without a palette (a headless host) the layers take the lowest free slots as before.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path as FsPath, PathBuf};

use burnburn_io::fs::FileSystem;
use burnburn_presentation::{CommandKey, FilesIntent, HeadlessEditorHarness, UiIntent};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_types::DocumentId;

/// slot 0 black, 1 blue, 2 red, 3 green, 4 yellow, 5 grey; the rest white.
fn palette() -> Vec<[u8; 3]> {
    let mut p = vec![[0, 0, 0], [0, 0, 255], [255, 0, 0], [0, 160, 0], [255, 255, 0], [128, 128, 128]];
    p.resize(30, [255, 255, 255]);
    p
}

fn with_file(name: &str, bytes: &[u8], palette: Option<Vec<[u8; 3]>>) -> (HeadlessEditorHarness, DocumentId, PathBuf) {
    let mut h = HeadlessEditorHarness::new();
    if let Some(p) = palette {
        h.controller().set_layer_palette(p);
    }
    h.fs().create_dir_all_for_test(FsPath::new("/in"));
    let path = PathBuf::from(format!("/in/{name}"));
    let temp = h.fs().create_temp_in(FsPath::new("/in"), ".fixture").unwrap();
    h.fs().write_all(&temp, bytes).unwrap();
    h.fs().replace(&temp, &path).unwrap();
    let doc = h.new_document().expect("window");
    (h, doc, path)
}

fn import_with_layers(h: &mut HeadlessEditorHarness, doc: DocumentId, path: PathBuf) {
    h.send(UiIntent::Import { document: doc, path });
    h.run_tasks();
    h.send(UiIntent::Files(FilesIntent::SetImportLayers { document: doc, create: true }));
    h.invoke(doc, CommandKey::CommitImport);
}

/// (name, slot) of every operation, in list order.
fn slots(h: &HeadlessEditorHarness, doc: DocumentId) -> Vec<(String, u8)> {
    h.document(doc).unwrap().operations.iter().map(|o| (o.name.clone(), o.slot)).collect()
}

fn stream(data: &str) -> Vec<u8> {
    format!("<< /Length {} >>\nstream\n{data}\nendstream", data.len()).into_bytes()
}

/// One 100 × 100 pt page with the given content stream.
fn pdf(content: &str) -> Vec<u8> {
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

/// A red stroke, a blue stroke and a black fill: exactly three palette colours.
const EXACT: &str = "1 0 0 RG 0 0 m 50 0 l S 0 0 1 RG 0 60 m 50 60 l S 0 g 60 60 20 20 re f";

#[test]
fn a_layer_whose_colour_is_in_the_palette_takes_that_slot() {
    let (mut h, doc, path) = with_file("exact.pdf", &pdf(EXACT), Some(palette()));
    import_with_layers(&mut h, doc, path);
    assert_eq!(
        slots(&h, doc),
        [("stroke #ff0000".to_owned(), 2), ("stroke #0000ff".to_owned(), 1), ("fill #000000".to_owned(), 0)]
    );
}

#[test]
fn a_colour_between_palette_colours_takes_the_nearest() {
    // (240, 20, 20) is near red; (20, 20, 200) near blue; (200, 200, 10) near yellow, not grey.
    let content = "0.941 0.078 0.078 RG 0 0 m 50 0 l S 0.078 0.078 0.784 RG 0 60 m 50 60 l S 0.784 0.784 0.04 rg 60 60 20 20 re f";
    let (mut h, doc, path) = with_file("near.pdf", &pdf(content), Some(palette()));
    import_with_layers(&mut h, doc, path);
    let got: Vec<u8> = slots(&h, doc).into_iter().map(|(_, s)| s).collect();
    assert_eq!(got, [2, 1, 4], "red, blue, yellow");
}

#[test]
fn two_layers_with_the_same_nearest_colour_get_two_slots_the_second_nearest_still_free() {
    // Both strokes are nearest to red (slot 2). The second, (230, 30, 30), then takes the nearest
    // slot that is still free; by hand its squared distance is 29 612 to the grey of slot 5 and at
    // least 52 150 to every other colour.
    let content = "1 0 0 RG 0 0 m 50 0 l S 0.902 0.118 0.118 RG 0 60 m 50 60 l S";
    let (mut h, doc, path) = with_file("twins.pdf", &pdf(content), Some(palette()));
    import_with_layers(&mut h, doc, path);
    let got: Vec<u8> = slots(&h, doc).into_iter().map(|(_, s)| s).collect();
    assert_eq!(got, [2, 5]);
}

#[test]
fn a_slot_that_a_layer_of_the_document_already_has_is_not_taken_again() {
    let (mut h, doc, path) = with_file("exact.pdf", &pdf(EXACT), Some(palette()));
    import_with_layers(&mut h, doc, path.clone());
    import_with_layers(&mut h, doc, path);
    let got: Vec<u8> = slots(&h, doc).into_iter().map(|(_, s)| s).collect();
    assert_eq!(got.len(), 6);
    let mut distinct = got.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(distinct.len(), 6, "six layers, six slots: {got:?}");
    assert_eq!(got[..3], [2, 1, 0], "the first import is unchanged");
    // The red of the second import: black is taken, so the nearest free colour to red is the grey
    // (by hand: 48 897 to slot 5, 65 025 to yellow, 90 625 to green, 130 050 to blue).
    assert_eq!(got[3], 5);
}

#[test]
fn a_tie_goes_to_the_lower_slot() {
    // 0.2 is gray level 51: exactly between black (0) and (102, 0, 0).
    let content = "0.2 0 0 RG 0 0 m 50 0 l S";
    let two = vec![[0, 0, 0], [102, 0, 0]];
    let (mut h, doc, path) = with_file("tie.pdf", &pdf(content), Some(two));
    import_with_layers(&mut h, doc, path);
    assert_eq!(slots(&h, doc), [("stroke #330000".to_owned(), 0)]);
}

#[test]
fn the_pens_of_a_plotter_file_take_the_slots_of_their_number() {
    let bytes = read_fixture("import", "square-circle.plt").expect("verified fixture");
    let (mut h, doc, path) = with_file("square-circle.plt", &bytes, Some(palette()));
    import_with_layers(&mut h, doc, path);
    assert_eq!(slots(&h, doc), [("Pen 1".to_owned(), 0), ("Pen 2".to_owned(), 1)]);
}

#[test]
fn without_a_palette_the_layers_take_the_lowest_free_slots_in_order() {
    let (mut h, doc, path) = with_file("exact.pdf", &pdf(EXACT), None);
    import_with_layers(&mut h, doc, path);
    assert_eq!(
        slots(&h, doc),
        [("stroke #ff0000".to_owned(), 0), ("stroke #0000ff".to_owned(), 1), ("fill #000000".to_owned(), 2)]
    );
}

#[test]
fn the_slots_survive_undo_and_redo_and_the_same_file_gives_the_same_slots_every_time() {
    let run = || {
        let (mut h, doc, path) = with_file("exact.pdf", &pdf(EXACT), Some(palette()));
        import_with_layers(&mut h, doc, path);
        let first = slots(&h, doc);
        h.invoke(doc, CommandKey::Undo);
        assert!(slots(&h, doc).is_empty());
        h.invoke(doc, CommandKey::Redo);
        assert_eq!(slots(&h, doc), first, "redo brings the same colours back");
        first
    };
    assert_eq!(run(), run());
}
