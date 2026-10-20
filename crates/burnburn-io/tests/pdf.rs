//! Public contract (W2-L6, P03): PDF and Illustrator (.ai, PDF-compatible) vector import at
//! physical scale. The PDF files are built here by an independent writer (classic xref table,
//! xref streams, object streams, Flate via the `zip` crate's deflate); every expectation is derived
//! by hand from the content text (1 pt = 25.4/72 mm), never from a previous run.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers and let-else in tests

use std::io::{Cursor, Write};

use burnburn_io::import::{
    Analysis, AnalyzeError, ImportFormat, ParseError, ParseLimits, PdfPages, SkipReason, UnitDecision, analyze,
    parse_pdf, parse_pdf_with,
};

const MM: f64 = 25.4 / 72.0;

fn close(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() <= eps
}

/// `<< /Length n extra >> stream … endstream` for raw `data`.
fn stream(extra: &str, data: &[u8]) -> Vec<u8> {
    let mut v = format!("<< /Length {} {extra} >>\nstream\n", data.len()).into_bytes();
    v.extend_from_slice(data);
    v.extend_from_slice(b"\nendstream");
    v
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for x in data {
        a = (a + u32::from(*x)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

/// zlib stream whose deflate body comes from the zip crate (independent of the importer's inflate).
fn zlib(data: &[u8]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("x", opts).unwrap();
    zip.write_all(data).unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(zip.finish().unwrap().into_inner())).unwrap();
    let mut raw = Vec::new();
    std::io::copy(&mut archive.by_index_raw(0).unwrap(), &mut raw).unwrap();
    let mut out = vec![0x78, 0x9c];
    out.extend_from_slice(&raw);
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

/// A classic PDF 1.4 file: `objects[i]` is object `i + 1`; object 1 must be the catalog.
fn pdf(objects: &[Vec<u8>]) -> Vec<u8> {
    let mut out = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
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

/// One-page PDF: catalog, page tree, page (`page_extra` adds entries) and its content stream.
fn one_page(page_extra: &str, content: &str) -> Vec<u8> {
    pdf(&[
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R {page_extra} >>").into_bytes(),
        stream("", content.as_bytes()),
    ])
}

fn import(bytes: &[u8]) -> burnburn_io::import::ImportDraft {
    parse_pdf(bytes).unwrap().to_draft(PdfPages::One(0)).unwrap()
}

#[test]
fn rectangle_imports_at_physical_size_with_y_up_origin_at_the_page_corner() {
    // 1 inch margin, 2 × 1 inch box.
    let doc = parse_pdf(&one_page("", "72 72 144 72 re S")).unwrap();
    assert_eq!(doc.pages().len(), 1);
    let page = &doc.pages()[0];
    assert!(close(page.width_mm, 612.0 * MM, 1e-9) && close(page.height_mm, 792.0 * MM, 1e-9), "{page:?}");
    let draft = doc.to_draft(PdfPages::One(0)).unwrap();
    assert_eq!(draft.items.len(), 1);
    let path = &draft.items[0].path;
    assert!(path.subpaths()[0].is_closed());
    let b = path.bounds().unwrap();
    assert!(close(b.min().x, 25.4, 1e-9) && close(b.min().y, 25.4, 1e-9), "{b:?}");
    assert!(close(b.max().x, 76.2, 1e-9) && close(b.max().y, 50.8, 1e-9), "{b:?}");
    assert!(close(path.signed_area().abs(), 50.8 * 25.4, 1e-9));
    assert_eq!(draft.report.units, UnitDecision::Pdf { mm_per_unit: MM, pages: 1 });
}

#[test]
fn curves_lines_close_and_nested_transforms_follow_the_pdf_graphics_state() {
    // Inside q…Q the scale-by-2 + translate applies; after Q the original matrix is back.
    let content = "q 2 0 0 2 36 36 cm 0 0 m 10 0 l 10 10 20 10 20 0 c S Q 0 0 m 36 0 l S";
    let draft = import(&one_page("", content));
    assert_eq!(draft.items.len(), 2);
    let first = &draft.items[0].path.subpaths()[0];
    // Start (0,0) → (36, 36) pt; line to (20+36, 36) = (56, 36); cubic end (2·20+36, 36) = (76, 36).
    assert!(close(first.start().x, 36.0 * MM, 1e-9) && close(first.start().y, 36.0 * MM, 1e-9));
    assert!(close(first.end().x, 76.0 * MM, 1e-9) && close(first.end().y, 36.0 * MM, 1e-9), "{:?}", first.end());
    let burnburn_geometry::Segment::Cubic { c1, c2, .. } = first.segments()[1] else { panic!("expected a cubic") };
    assert!(close(c1.x, 56.0 * MM, 1e-9) && close(c1.y, 56.0 * MM, 1e-9), "{c1:?}");
    assert!(close(c2.x, 76.0 * MM, 1e-9) && close(c2.y, 56.0 * MM, 1e-9), "{c2:?}");
    let second = draft.items[1].path.bounds().unwrap();
    assert!(close(second.min().x, 0.0, 1e-12) && close(second.max().x, 36.0 * MM, 1e-9));
}

#[test]
fn v_and_y_curve_operators_and_h_close_the_subpath() {
    let draft = import(&one_page("", "0 0 m 10 0 20 10 v 20 30 20 40 y h S"));
    let sp = &draft.items[0].path.subpaths()[0];
    assert!(sp.is_closed());
    let segs = sp.segments();
    let burnburn_geometry::Segment::Cubic { c1, .. } = segs[0] else { panic!() };
    assert!(close(c1.x, 0.0, 1e-12) && close(c1.y, 0.0, 1e-12), "v repeats the current point as first control");
    let burnburn_geometry::Segment::Cubic { c2, to, .. } = segs[1] else { panic!() };
    assert!(close(c2.x, 20.0 * MM, 1e-9) && close(c2.y, 40.0 * MM, 1e-9), "y repeats the end as second control");
    assert!(close(to.x, 20.0 * MM, 1e-9) && close(to.y, 40.0 * MM, 1e-9));
}

#[test]
fn page_rotation_userunit_and_cropbox_define_the_page_frame() {
    // CropBox 100 × 50 pt at (10, 20); UserUnit 2 doubles every length; Rotate 90 swaps width and height.
    let bytes = one_page("/CropBox [10 20 110 70] /UserUnit 2 /Rotate 90", "10 20 m 110 20 l S");
    let doc = parse_pdf(&bytes).unwrap();
    let page = &doc.pages()[0];
    assert_eq!(page.rotation_deg, 90);
    assert!(close(page.width_mm, 50.0 * 2.0 * MM, 1e-9) && close(page.height_mm, 100.0 * 2.0 * MM, 1e-9), "{page:?}");
    // The bottom edge of the crop box, rotated clockwise, becomes the page's left edge: x = 0, y from 200 pt down to 0.
    let b = doc.to_draft(PdfPages::One(0)).unwrap().items[0].path.bounds().unwrap();
    assert!(close(b.min().x, 0.0, 1e-9) && close(b.max().x, 0.0, 1e-9), "{b:?}");
    assert!(close(b.min().y, 0.0, 1e-9) && close(b.max().y, 200.0 * MM, 1e-9), "{b:?}");
}

#[test]
fn several_pages_are_listed_chosen_and_laid_out_side_by_side() {
    let page = |content: u32| {
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents {content} 0 R >>").into_bytes()
    };
    let bytes = pdf(&[
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_vec(),
        page(5),
        page(6),
        stream("", b"10 10 m 20 10 l S"),
        stream("", b"30 30 m 40 30 l S"),
    ]);
    let doc = parse_pdf(&bytes).unwrap();
    assert_eq!(doc.pages().len(), 2);
    let second = doc.to_draft(PdfPages::One(1)).unwrap();
    assert_eq!(second.items.len(), 1);
    assert!(close(second.items[0].path.bounds().unwrap().min().x, 30.0 * MM, 1e-9));
    assert!(second.items[0].entity.starts_with("p2#"), "{}", second.items[0].entity);
    // All pages: the second page is shifted by the first page's width plus the 10 mm gap.
    let all = doc.to_draft(PdfPages::All).unwrap();
    assert_eq!(all.items.len(), 2);
    let shift = 100.0 * MM + 10.0;
    assert!(close(all.items[1].path.bounds().unwrap().min().x, 30.0 * MM + shift, 1e-9));
    assert!(doc.to_draft(PdfPages::One(2)).is_err(), "a page that does not exist is an error");
}

#[test]
fn pages_inherit_mediabox_and_resources_from_the_page_tree() {
    let bytes = pdf(&[
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 100] /Rotate 0 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>".to_vec(),
        stream("", b"0 0 m 200 100 l S"),
    ]);
    let doc = parse_pdf(&bytes).unwrap();
    assert!(close(doc.pages()[0].width_mm, 200.0 * MM, 1e-9));
}

#[test]
fn flate_content_streams_and_content_arrays_are_decoded() {
    let content = zlib(b"0 0 m 72 0 l 72 72 l 0 72 l h S");
    let bytes = pdf(&[
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents [4 0 R 5 0 R] >>".to_vec(),
        stream("/Filter /FlateDecode", &content),
        // The second stream continues in the middle of an operator pair: streams are joined with whitespace.
        stream("", b"100 100 m 172 100 l S"),
    ]);
    let draft = import(&bytes);
    assert_eq!(draft.items.len(), 2);
    assert!(close(draft.items[0].path.signed_area().abs(), 25.4 * 25.4, 1e-9));
}

#[test]
fn hidden_optional_content_layers_are_not_imported() {
    let bytes = pdf(&[
        b"<< /Type /Catalog /Pages 2 0 R /OCProperties << /OCGs [5 0 R 6 0 R] /D << /OFF [6 0 R] >> >> >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R \
          /Resources << /Properties << /L1 5 0 R /L2 6 0 R >> >> >>"
            .to_vec(),
        stream(
            "",
            b"/OC /L1 BDC 0 0 m 10 0 l S EMC /OC /L2 BDC 0 20 m 10 20 l S /Span BMC 0 30 m 10 30 l S EMC EMC 0 40 m 10 40 l S",
        ),
        b"<< /Type /OCG /Name (Zichtbaar) >>".to_vec(),
        b"<< /Type /OCG /Name (Verborgen) >>".to_vec(),
    ]);
    let draft = import(&bytes);
    let ys: Vec<f64> = draft.items.iter().map(|i| i.path.bounds().unwrap().min().y).collect();
    assert_eq!(ys.len(), 2, "{ys:?}");
    assert!(
        close(ys[0], 0.0, 1e-12) && close(ys[1], 40.0 * MM, 1e-9),
        "only the visible layer and plain content: {ys:?}"
    );
    assert!(draft.report.skipped.iter().any(|s| s.reason == SkipReason::Hidden), "hidden layer is reported");
}

#[test]
fn text_images_shading_and_clipping_are_reported_not_dropped() {
    let content = "BT /F1 12 Tf 10 10 Td (Hallo) Tj ET q 20 0 0 20 0 0 cm /Im1 Do Q /Sh1 sh \
                   0 0 50 50 re W n 0 0 m 30 0 l S BI /W 1 /H 1 /BPC 8 /CS /G ID \x00 EI";
    let draft = import(&one_page("/Resources << /XObject << /Im1 5 0 R >> >>", content));
    assert_eq!(draft.items.len(), 1, "the plain line after the clip still imports");
    let kinds: Vec<&str> = draft.report.skipped.iter().map(|s| s.kind.as_str()).collect();
    for expected in ["text", "image", "shading", "clip"] {
        assert!(kinds.contains(&expected), "{expected} not reported in {kinds:?}");
    }
}

#[test]
fn colours_name_the_items_by_stroke_or_fill() {
    let draft = import(&one_page("", "1 0 0 RG 0 0 m 10 0 l S 0 0 1 rg 0 5 10 10 re f 0.5 g 0 20 m 10 20 l S"));
    let names: Vec<&str> = draft.items.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, ["stroke #ff0000", "fill #0000ff", "stroke #ff0000"], "gray fill does not rename a stroke");
}

#[test]
fn form_xobjects_apply_their_matrix_and_self_reference_is_bounded() {
    let bytes = pdf(&[
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R \
          /Resources << /XObject << /F1 5 0 R /Loop 6 0 R >> >> >>"
            .to_vec(),
        stream("", b"q 1 0 0 1 10 10 cm /F1 Do Q /Loop Do"),
        stream("/Type /XObject /Subtype /Form /BBox [0 0 100 100] /Matrix [2 0 0 2 0 0]", b"0 0 m 5 0 l S"),
        stream(
            "/Type /XObject /Subtype /Form /BBox [0 0 100 100] /Resources << /XObject << /Loop 6 0 R >> >>",
            b"/Loop Do 1 1 m 2 2 l S",
        ),
    ]);
    let draft = import(&bytes);
    let b = draft.items[0].path.bounds().unwrap();
    // Form matrix [2 0 0 2 0 0] first, then the page's translate(10, 10): x from 10 to 20.
    assert!(close(b.min().x, 10.0 * MM, 1e-9) && close(b.max().x, 20.0 * MM, 1e-9), "{b:?}");
    assert!(draft.report.skipped.iter().any(|s| matches!(s.reason, SkipReason::Invalid(r) if r.contains("recursion"))));
}

#[test]
fn xref_streams_object_streams_and_png_predictors_are_read() {
    // PDF 1.5 layout: objects 1-3 live in an object stream (object 5); the xref stream (object 6) uses
    // /W [1 2 1] with the PNG "Up" predictor (12).
    let objstm_objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R >>",
    ];
    let mut header = String::new();
    let mut body = String::new();
    for (i, o) in objstm_objects.iter().enumerate() {
        header.push_str(&format!("{} {} ", i + 1, body.len()));
        body.push_str(o);
        body.push('\n');
    }
    let objstm_data = format!("{header}{body}");
    let first = header.len();
    let mut out = b"%PDF-1.5\n".to_vec();
    let off4 = out.len();
    out.extend_from_slice(b"4 0 obj\n");
    out.extend_from_slice(&stream("", b"10 10 m 60 10 l S"));
    out.extend_from_slice(b"\nendobj\n");
    let off5 = out.len();
    out.extend_from_slice(b"5 0 obj\n");
    out.extend_from_slice(&stream(
        &format!("/Type /ObjStm /N 3 /First {first} /Filter /FlateDecode"),
        &zlib(objstm_data.as_bytes()),
    ));
    out.extend_from_slice(b"\nendobj\n");
    let off6 = out.len();
    // Entries: 0 free; 1..3 compressed in object 5 (index 0..2); 4 at off4; 5 at off5; 6 at off6.
    let rows: Vec<[u8; 4]> = vec![
        [0, 0, 0, 255],
        [2, 0, 5, 0],
        [2, 0, 5, 1],
        [2, 0, 5, 2],
        [1, (off4 >> 8) as u8, off4 as u8, 0],
        [1, (off5 >> 8) as u8, off5 as u8, 0],
        [1, (off6 >> 8) as u8, off6 as u8, 0],
    ];
    let mut filtered = Vec::new();
    let mut prev = [0u8; 4];
    for r in &rows {
        filtered.push(2); // "Up": each byte minus the byte above.
        for i in 0..4 {
            filtered.push(r[i].wrapping_sub(prev[i]));
        }
        prev = *r;
    }
    out.extend_from_slice(b"6 0 obj\n");
    out.extend_from_slice(&stream(
        "/Type /XRef /Size 7 /W [1 2 1] /Root 1 0 R /Filter /FlateDecode /DecodeParms << /Predictor 12 /Columns 4 >>",
        &zlib(&filtered),
    ));
    out.extend_from_slice(format!("\nendobj\nstartxref\n{off6}\n%%EOF\n").as_bytes());
    let draft = import(&out);
    assert_eq!(draft.items.len(), 1);
    assert!(close(draft.items[0].path.bounds().unwrap().max().x, 60.0 * MM, 1e-9));
}

#[test]
fn a_damaged_xref_is_rebuilt_by_scanning_for_objects() {
    let mut bytes = one_page("", "0 0 m 72 0 l S");
    let at = bytes.windows(9).rposition(|w| w == b"startxref").unwrap();
    bytes.truncate(at);
    bytes.extend_from_slice(b"startxref\n999999\n%%EOF\n");
    let draft = import(&bytes);
    assert_eq!(draft.items.len(), 1);
    assert!(close(draft.items[0].path.bounds().unwrap().max().x, 25.4, 1e-9));
}

#[test]
fn encrypted_and_non_pdf_files_are_typed_errors() {
    let mut bytes = one_page("", "0 0 m 1 1 l S");
    let at = bytes.windows(7).rposition(|w| w == b"/Root 1").unwrap();
    bytes.splice(at..at, b"/Encrypt << /Filter /Standard >> ".iter().copied());
    assert_eq!(parse_pdf(&bytes).unwrap_err(), ParseError::Unsupported("encrypted PDF"));
    assert!(matches!(parse_pdf(b"hello world"), Err(ParseError::Malformed { .. })));
    assert!(matches!(parse_pdf(b""), Err(ParseError::Malformed { .. })));
}

#[test]
fn illustrator_files_are_pdf_compatible_or_a_named_unsupported_legacy_format() {
    assert_eq!(ImportFormat::from_extension("AI"), Some(ImportFormat::Ai));
    // Modern .ai = PDF, possibly behind a preamble; offsets stay relative to the %PDF header.
    let mut ai = b"\x00\x00 junk before the header\n".to_vec();
    ai.extend_from_slice(&one_page("", "0 0 m 72 0 l S"));
    let Analysis::Pdf(doc) = analyze(ImportFormat::Ai, &ai).unwrap() else { panic!("expected a PDF analysis") };
    assert_eq!(doc.to_draft(PdfPages::One(0)).unwrap().items.len(), 1);
    // Legacy PostScript-based Illustrator.
    let legacy = b"%!PS-Adobe-3.0 EPSF-3.0\n%%Creator: Adobe Illustrator(R) 8.0\n";
    assert_eq!(
        analyze(ImportFormat::Ai, legacy),
        Err(AnalyzeError::Parse(ParseError::Unsupported(
            "legacy PostScript Illustrator file: save it with PDF compatibility or export SVG"
        )))
    );
    assert!(ImportFormat::Pdf.check_supported().is_ok());
}

#[test]
fn limits_are_typed_errors() {
    let bytes = one_page("", "0 0 m 1 1 l S 0 0 m 1 1 l S 0 0 m 1 1 l S");
    let small = ParseLimits { max_bytes: 100, ..ParseLimits::default() };
    assert!(matches!(parse_pdf_with(&bytes, &small), Err(ParseError::TooLarge { limit: 100, .. })));
    let cmds = ParseLimits { max_path_commands: 2, ..ParseLimits::default() };
    let doc = parse_pdf_with(&bytes, &cmds).unwrap();
    assert_eq!(doc.to_draft(PdfPages::One(0)).unwrap_err(), ParseError::TooManyCommands { limit: 2 });
    let ops = ParseLimits { max_entities: 5, ..ParseLimits::default() };
    let doc = parse_pdf_with(&bytes, &ops).unwrap();
    assert!(matches!(doc.to_draft(PdfPages::One(0)), Err(ParseError::TooManyEntities { limit: 5 })));
    // A compression bomb is stopped by the decoded-size cap, not by memory exhaustion.
    let bomb = zlib(&vec![b' '; 5 << 20]);
    let bytes = pdf(&[
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] /Contents 4 0 R >>".to_vec(),
        stream("/Filter /FlateDecode", &bomb),
    ]);
    let tight = ParseLimits { max_bytes: bytes.len().max(1 << 16), ..ParseLimits::default() };
    let doc = parse_pdf_with(&bytes, &tight).unwrap();
    let draft = doc.to_draft(PdfPages::One(0)).unwrap();
    assert!(draft.report.skipped.iter().any(|s| matches!(s.reason, SkipReason::Invalid(r) if r.contains("stream"))));
}

#[test]
fn page_import_commits_as_one_geometry_command() {
    let doc = parse_pdf(&one_page("", "0 0 m 72 0 l S 0 10 m 72 10 l S")).unwrap();
    let cmd = doc.import_command(burnburn_io::asset_hash(b"pdf"), PdfPages::One(0)).unwrap();
    assert!(matches!(cmd, burnburn_document::GeometryCommand::Import { items, .. } if items.len() == 2));
    let empty = parse_pdf(&one_page("", "BT ET")).unwrap();
    assert!(empty.import_command(burnburn_io::asset_hash(b"pdf"), PdfPages::One(0)).is_err());
}

#[test]
fn fixture_plate_imports_cut_layer_and_reports_guide_layer_and_text() {
    let bytes = std::fs::read(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-data/import/plate-layers.pdf"),
    )
    .unwrap();
    let doc = parse_pdf(&bytes).unwrap();
    assert!(close(doc.pages()[0].width_mm, 595.276 * MM, 1e-9));
    let draft = doc.to_draft(PdfPages::One(0)).unwrap();
    assert_eq!(draft.items.len(), 2, "plate and hole; the Guide layer is switched off");
    assert!(draft.items.iter().all(|i| i.name == "stroke #0000ff"));
    let plate = draft.items[0].path.bounds().unwrap();
    // Coordinates in the file have four decimals in points: 1e-4 pt = 3.5e-5 mm.
    assert!(close(plate.min().x, 20.0, 1e-3) && close(plate.max().y, 70.0, 1e-3), "{plate:?}");
    assert!(close(draft.items[0].path.signed_area().abs(), 5000.0, 0.5));
    let hole = &draft.items[1].path;
    assert!(hole.subpaths()[0].is_closed());
    // Four-Bézier circle: area within 0.06 % of π r² (k = 0.5523).
    assert!(close(hole.signed_area().abs(), std::f64::consts::PI * 25.0, 0.05), "{}", hole.signed_area());
    let kinds: Vec<&str> = draft.report.skipped.iter().map(|s| s.kind.as_str()).collect();
    assert!(kinds.contains(&"text") && kinds.contains(&"optional content"), "{kinds:?}");
}
