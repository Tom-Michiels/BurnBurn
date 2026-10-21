//! Acceptance plan BC01: a bar code made in the barcode dialog is read back by an independent
//! decoder (`rxing`, a ZXing port), at the minimum module, with a Unicode payload and with a long
//! payload; and the output geometry in the RD file has the symbol's size.
//!
//! The symbol is taken from the app's scene (what the user sees and cuts), rasterised here with
//! our own little rasteriser and handed to the decoder; nothing of the encoder is reused.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test code

mod common;

use burnburn_geometry::{Path, Segment};
use burnburn_presentation::{
    BarcodeEdit, BarcodeField, BarcodeIssue, CommandKey, FieldKey, HeadlessEditorHarness, SelectMode, SymbolInputMode,
    Symbology, UiIntent,
};
use burnburn_types::DocumentId;
use common::*;
use rxing::{BarcodeFormat, DecodeHints};

fn dialog() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::BarcodeNew);
    (h, doc)
}

fn edit(h: &mut HeadlessEditorHarness, doc: DocumentId, e: BarcodeEdit) {
    h.send(UiIntent::Barcode { document: doc, edit: e });
}

/// Fills the dialog (type first: it resets the type-dependent fields) without creating.
fn fill(h: &mut HeadlessEditorHarness, doc: DocumentId, sym: Symbology, payload: &str, module: &str) {
    edit(h, doc, BarcodeEdit::Symbology(sym));
    edit(h, doc, BarcodeEdit::Payload(payload.into()));
    edit(h, doc, BarcodeEdit::Module(module.into()));
}

/// Fills the dialog and creates the symbol; returns its path as the scene shows it.
fn create(h: &mut HeadlessEditorHarness, doc: DocumentId, sym: Symbology, payload: &str, module: &str) -> Path {
    fill(h, doc, sym, payload, module);
    finish(h, doc)
}

fn finish(h: &mut HeadlessEditorHarness, doc: DocumentId) -> Path {
    let d = h.document(doc).unwrap().barcode_dialog.unwrap();
    assert!(d.can_create, "{:?}", d.problems);
    edit(h, doc, BarcodeEdit::Create);
    let vm = h.document(doc).unwrap();
    let id = *vm.selection.last().expect("the new symbol is selected");
    let scene = h.scene(doc).unwrap();
    (*scene.items.iter().find(|i| i.source == id).expect("in the scene").path).clone()
}

fn rectangles(path: &Path) -> Vec<(f64, f64, f64, f64)> {
    path.subpaths()
        .iter()
        .map(|s| {
            let mut pts = vec![s.start()];
            for seg in s.segments() {
                match seg {
                    Segment::Line { to } => pts.push(*to),
                    other => panic!("not a straight edge: {other:?}"),
                }
            }
            let (x0, x1) = pts.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.x), b.max(p.x)));
            let (y0, y1) = pts.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.y), b.max(p.y)));
            (x0, y0, x1, y1)
        })
        .collect()
}

/// Luma raster of the symbol: `px` pixels per module (module = `module_mm`), white margin 24 px.
fn raster(path: &Path, module_mm: f64, px: f64) -> (Vec<u8>, u32, u32) {
    let b = path.bounds().unwrap();
    let scale = px / module_mm;
    let (w, h) = ((b.width() * scale).round() as usize, (b.height() * scale).round() as usize);
    let m = (10.0 * px) as usize; // 10 modules of white around the bars (>= any quiet zone)
    let (fw, fh) = (w + 2 * m, h + 2 * m);
    let mut img = vec![255u8; fw * fh];
    for (x0, y0, x1, y1) in rectangles(path) {
        let (c0, c1) = (((x0 - b.min().x) * scale).round() as usize, ((x1 - b.min().x) * scale).round() as usize);
        let (r0, r1) = (h - ((y1 - b.min().y) * scale).round() as usize, h - ((y0 - b.min().y) * scale).round() as usize);
        for r in r0..r1 {
            for c in c0..c1 {
                img[(r + m) * fw + c + m] = 0;
            }
        }
    }
    (img, fw as u32, fh as u32)
}

fn read(path: &Path, module_mm: f64, format: BarcodeFormat) -> Result<String, String> {
    let (img, w, h) = raster(path, module_mm, 8.0);
    let mut hints = DecodeHints::default();
    rxing::helpers::detect_in_luma_with_hints(img, w, h, Some(format), &mut hints)
        .map(|r| r.getText().to_string())
        .map_err(|e| format!("{e:?}"))
}

#[test]
fn bc01_a_unicode_payload_is_read_back_at_every_error_correction_level() {
    // Accents, a Euro sign, CJK and an emoji: all of it is UTF-8 bytes in a byte-mode QR code.
    let payload = "Pla\u{e5}t \u{20ac}12,50 \u{65e5}\u{672c}\u{8a9e} \u{1f525}";
    for step in 0..4_u8 {
        let (mut h, doc) = dialog();
        fill(&mut h, doc, Symbology::QrCode, payload, "0,4");
        edit(&mut h, doc, BarcodeEdit::InputMode(SymbolInputMode::Unicode));
        edit(&mut h, doc, BarcodeEdit::Ecc(step));
        assert_eq!(h.document(doc).unwrap().barcode_dialog.unwrap().ecc.unwrap().selected, step);
        let path = finish(&mut h, doc);
        let got = read(&path, 0.4, BarcodeFormat::QR_CODE).unwrap_or_else(|e| panic!("step {step}: {e}"));
        assert_eq!(got, payload, "ECC step {step}");
    }
}

#[test]
fn bc01_the_longest_payload_of_a_level_decodes_and_one_more_byte_is_refused() {
    // QR version 10, level L, byte mode holds 271 bytes (ISO 18004 capacity table).
    let long: String = (0..271).map(|i| char::from(b'a' + (i % 26) as u8)).collect();
    let (mut h, doc) = dialog();
    fill(&mut h, doc, Symbology::QrCode, &long, "0,4");
    edit(&mut h, doc, BarcodeEdit::Ecc(0));
    let path = finish(&mut h, doc);
    assert_eq!(read(&path, 0.4, BarcodeFormat::QR_CODE).unwrap(), long);

    let (mut h, doc) = dialog();
    fill(&mut h, doc, Symbology::QrCode, &format!("{long}x"), "0,4");
    edit(&mut h, doc, BarcodeEdit::Ecc(0));
    let d = h.document(doc).unwrap().barcode_dialog.unwrap();
    assert!(!d.can_create, "272 bytes do not fit");
    assert!(d.problems.iter().any(|p| p.field == BarcodeField::Payload), "{:?}", d.problems);
    let rev = h.document(doc).unwrap().revision;
    edit(&mut h, doc, BarcodeEdit::Create);
    assert_eq!(h.document(doc).unwrap().revision, rev, "nothing is created from an over-long payload");
}

#[test]
fn bc01_the_minimum_module_is_the_edge_between_creatable_and_refused() {
    let (mut h, doc) = dialog();
    edit(&mut h, doc, BarcodeEdit::Payload("MIN".into()));
    edit(&mut h, doc, BarcodeEdit::MinModule("0,2".into()));
    // Just below: refused, with the limit named.
    edit(&mut h, doc, BarcodeEdit::Module("0,199".into()));
    let d = h.document(doc).unwrap().barcode_dialog.unwrap();
    assert!(!d.can_create);
    assert!(
        d.problems
            .iter()
            .any(|p| p.field == BarcodeField::Module && matches!(p.issue, BarcodeIssue::ModuleTooSmall { .. })),
        "{:?}",
        d.problems
    );
    // Exactly at the minimum: created, one module is exactly 0.2 mm in the scene, and it decodes.
    let path = create(&mut h, doc, Symbology::QrCode, "MIN", "0,2");
    let smallest = rectangles(&path).iter().map(|r| (r.2 - r.0).min(r.3 - r.1)).fold(f64::MAX, f64::min);
    assert!((smallest - 0.2).abs() < 1e-9, "smallest feature {smallest}");
    assert_eq!(read(&path, 0.2, BarcodeFormat::QR_CODE).unwrap(), "MIN");
}

#[test]
fn bc01_linear_symbols_decode_with_their_check_digits() {
    for (sym, payload, expect, format) in [
        (Symbology::Ean, "400638133393", "4006381333931", BarcodeFormat::EAN_13),
        (Symbology::Code128, "BurnBurn-2026", "BurnBurn-2026", BarcodeFormat::CODE_128),
        (Symbology::Code93, "LASER 42", "LASER 42", BarcodeFormat::CODE_93),
    ] {
        let (mut h, doc) = dialog();
        let path = create(&mut h, doc, sym, payload, "0,4");
        assert_eq!(read(&path, 0.4, format).unwrap_or_else(|e| panic!("{sym:?}: {e}")), expect);
    }
}

#[test]
fn bc01_the_output_cuts_the_symbol_at_its_real_size_in_machine_coordinates() {
    let (mut h, doc) = dialog();
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile()), ruida: RUIDA });
    create(&mut h, doc, Symbology::QrCode, "https://burnburn.example/p/42", "0,5");
    let vm = h.document(doc).unwrap();
    let id = *vm.selection.last().unwrap();
    // The symbol is made in the middle of the view (the origin of a new window): put it on the bed.
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::X, text: "100".into() });
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::Y, text: "200".into() });
    let b = h.scene(doc).unwrap().items.iter().find(|i| i.source == id).unwrap().path.bounds().unwrap();
    assert!((b.min().x - 100.0).abs() < 1e-9 && (b.min().y - 200.0).abs() < 1e-9);
    h.send(UiIntent::AddOperation { document: doc, draft: line_cut("Snijden") });
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    h.fs().create_dir_all_for_test(std::path::Path::new("/work"));
    let out = std::path::PathBuf::from("/work/qr.rd");
    h.send(UiIntent::ExportRd { document: doc, path: out.clone() });
    h.run_tasks();
    let bytes = burnburn_io::fs::FileSystem::read(h.fs(), &out, u64::MAX).expect("exported RD");
    let rd = decode(&bytes);
    let (lo, hi) = cut_extent(&cuts(&rd));
    // The symbol's own extent (what the dialog made and the scene shows) is the extent of the
    // laser-on motion in the RD, to the micrometre.
    assert!((lo.x - b.min().x).abs() < 0.002 && (lo.y - b.min().y).abs() < 0.002, "{lo:?} vs {:?}", b.min());
    assert!((hi.x - b.max().x).abs() < 0.002 && (hi.y - b.max().y).abs() < 0.002, "{hi:?} vs {:?}", b.max());
}

#[test]
fn bc01_data_mode_names_the_first_character_it_cannot_carry_instead_of_changing_it() {
    // Input Mode "Data" is one byte per character; a Euro sign is not one. It is refused with the
    // character and its index, and the same text is fine in Unicode mode.
    let (mut h, doc) = dialog();
    fill(&mut h, doc, Symbology::QrCode, "Pla\u{e5}t \u{20ac}", "0,4");
    let d = h.document(doc).unwrap().barcode_dialog.unwrap();
    assert!(!d.can_create);
    assert!(d.problems.iter().any(|p| p.issue == BarcodeIssue::InvalidChar { ch: '\u{20ac}', index: 6 }), "{:?}", d.problems);
    edit(&mut h, doc, BarcodeEdit::InputMode(SymbolInputMode::Unicode));
    assert!(h.document(doc).unwrap().barcode_dialog.unwrap().can_create);
}
