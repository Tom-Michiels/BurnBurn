//! Acceptance plan, text block: TX01 (typography reaches preview, paths and output alike), TX02
//! (CSV and serial policies) and TX03 (bake / next / auto-advance / failed send).
//!
//! TX01 follows one text from the shaper through the document and compile to the RD file and
//! compares three extents that are produced by independent code: the shaper's outline bounds
//! (preview), the document's derived objects (paths) and the cut motion decoded from the RD.
//! Font metrics come from the unmodified OFL fonts in `test-data/fonts`.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test code

mod acc_machine;
mod common;

use std::sync::Arc;

use burnburn_cam::preflight;
use burnburn_document::text::TextCommand;
use burnburn_document::{
    CommandPayload, DerivedOwner, DocumentCommand, DocumentSession, EvalStatus, GeometrySource,
};
use burnburn_generators::{
    DataTable, FrozenRows, JobClock, OnEnd, ProductionRecord, RowBinding, SerialCounter, SerialSpec, VariableError,
    CsvError,
};
use burnburn_geometry::{Affine, Point, Vector};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_text::{
    FontData, FontRef, FontSize, FontStyle, MemoryFontSource, TextEvaluator, TextMode, TextObject, shape,
};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId, StableId};
use common::*;
use proptest::prelude::*;

fn font(name: &str) -> Arc<FontData> {
    Arc::new(FontData::from_bytes(read_fixture("fonts", name).unwrap()).unwrap())
}

fn fonts() -> (MemoryFontSource, FontRef, FontRef) {
    let mut src = MemoryFontSource::new();
    let lib = src.insert("Liberation Sans", FontStyle::Regular, font("LiberationSans-Regular.ttf"));
    let biz = src.insert("BIZ UDPGothic", FontStyle::Regular, font("BIZUDPGothic-Regular.ttf"));
    (src, lib, biz)
}

fn text(content: &str, font: &FontRef) -> TextObject {
    let mut t = TextObject::new(content, font.clone(), FontSize::from_mm(Length::from_mm(10.0).unwrap()).unwrap());
    t.placement = Affine::translate(Vector::new(30.0, 50.0)).unwrap();
    t
}

/// The text in a fresh document with a cut on every output; returns (snapshot, output ids).
fn document_with(obj: &TextObject) -> (DocumentSession, Vec<GeometryId>) {
    let (src, _, _) = fonts();
    let ev = TextEvaluator::new(src);
    let mut s = new_session();
    let cmd = DocumentCommand {
        document: s.id(),
        expected: s.revision(),
        request: RequestId::from_raw(1),
        payload: CommandPayload::Text(TextCommand::Create { text: Box::new(obj.clone()) }),
    };
    s.apply_with(cmd, &[&ev]).unwrap();
    let snap = s.snapshot();
    let ids: Vec<GeometryId> = snap
        .objects()
        .filter(|(_, o)| matches!(o.source(), GeometrySource::Derived(k) if matches!(k.owner, DerivedOwner::Text(_))))
        .map(|(id, _)| id)
        .collect();
    drop(snap);
    if !ids.is_empty() {
        add_cut(&mut s, ids.clone());
    }
    (s, ids)
}

fn bounds_xy(p: &burnburn_geometry::Path) -> (Point, Point) {
    let b = p.bounds().unwrap();
    (b.min(), b.max())
}

/// (preview extent from the shaper, extent of the derived document objects, extent of the RD cuts)
fn three_extents(obj: &TextObject) -> ((Point, Point), (Point, Point), (Point, Point)) {
    let (src, _, _) = fonts();
    let shaped = shape(obj, &src).unwrap();
    let (s, ids) = document_with(obj);
    let snap = s.snapshot();
    let doc = ids
        .iter()
        .map(|id| snap.object(*id).unwrap().bounds().unwrap())
        .reduce(burnburn_geometry::Bounds::union)
        .unwrap();
    let plan = compile_plan(&snap);
    assert!(!preflight(&snap, &plan, &profile()).has_errors());
    let rd = decode(&rd_bytes(&plan));
    (bounds_xy(&shaped.outlines), (doc.min(), doc.max()), cut_extent(&cuts(&rd)))
}

fn same(a: (Point, Point), b: (Point, Point), tol: f64) -> bool {
    a.0.distance(b.0) <= tol && a.1.distance(b.1) <= tol
}

fn width(e: (Point, Point)) -> f64 {
    e.1.x - e.0.x
}

#[test]
fn tx01_preview_paths_and_output_show_the_same_kerned_text() {
    let (_, lib, _) = fonts();
    let mut obj = text("AVA To Wa", &lib);
    let (preview, paths, rd) = three_extents(&obj);
    assert!(same(preview, paths, 1e-9), "document objects are the shaper's outlines: {preview:?} {paths:?}");
    // The RD flattens curves within 0.001 mm and rounds to micrometres.
    assert!(same(preview, rd, 0.004), "RD {rd:?} vs preview {preview:?}");
    // Kerning pairs (AV, To, Wa) make the line narrower, and the RD sees exactly that difference.
    obj.layout.kerning = false;
    let (preview_plain, _, rd_plain) = three_extents(&obj);
    assert!(width(preview_plain) - width(preview) > 1.0, "kerning removes more than a millimetre");
    assert!(((width(rd_plain) - width(rd)) - (width(preview_plain) - width(preview))).abs() < 0.008);
}

#[test]
fn tx01_ligatures_change_all_three_in_step() {
    let (_, _, biz) = fonts();
    // BIZ UDPGothic has the latin `fi` ligature (Liberation Sans does not).
    let mut obj = text("fine office", &biz);
    let (p1, d1, r1) = three_extents(&obj);
    obj.layout.ligatures = false;
    let (p2, d2, r2) = three_extents(&obj);
    assert!(same(p1, d1, 1e-9) && same(p2, d2, 1e-9));
    assert!(same(p1, r1, 0.004) && same(p2, r2, 0.004));
    assert!((width(p1) - width(p2)).abs() > 0.5, "ligature on/off changes the width ({} vs {})", width(p1), width(p2));
}

#[test]
fn tx01_accents_composed_or_combining_are_drawn_above_the_letter() {
    let (src, lib, _) = fonts();
    let top = |s: &str| {
        let shaped = shape(&text(s, &lib), &src).unwrap();
        assert!(shaped.missing.is_empty(), "{s:?}: {:?}", shaped.missing);
        bounds_xy(&shaped.outlines).1.y
    };
    let plain = top("e");
    for accented in ["\u{e9}", "e\u{301}", "\u{c9}", "E\u{301}"] {
        let base = if accented.starts_with(['E', '\u{c9}']) { top("E") } else { plain };
        assert!(top(accented) > base + 0.4, "{accented:?} has an accent above the letter");
    }
    // And the accent reaches the output.
    let (p, _, rd) = three_extents(&text("\u{c9}t\u{e9}", &lib));
    assert!(same(p, rd, 0.004));
}

#[test]
fn tx01_a_fallback_font_draws_what_the_first_font_lacks_in_preview_paths_and_output() {
    let (src, lib, biz) = fonts();
    let mut obj = text("H\u{65e5}H", &lib);
    obj.fallback = vec![biz.clone()];
    let shaped = shape(&obj, &src).unwrap();
    assert!(shaped.missing.is_empty());
    assert_eq!(shaped.used_fonts.len(), 2, "both faces contributed");
    let (p, d, rd) = three_extents(&obj);
    assert!(same(p, d, 1e-9) && same(p, rd, 0.004));
    // Without the fallback the character is reported and takes no space, instead of tofu.
    obj.fallback.clear();
    let shaped = shape(&obj, &src).unwrap();
    assert_eq!(shaped.missing.len(), 1);
    assert_eq!(shaped.missing[0].ch, '\u{65e5}');
    let (p2, d2, rd2) = three_extents(&obj);
    assert!(same(p2, d2, 1e-9) && same(p2, rd2, 0.004), "the output omits it exactly like the preview");
    assert!(width(p) > width(p2) + 5.0, "the missing character is not drawn, not even as a box");
}

#[test]
fn tx01_single_line_text_is_open_strokes_in_preview_and_output() {
    let (src, lib, _) = fonts();
    let mut obj = text("A1-Z", &lib);
    obj.mode = TextMode::SingleLine;
    let shaped = shape(&obj, &src).unwrap();
    assert!(shaped.outlines.subpaths().iter().all(|s| !s.is_closed()));
    let (p, d, rd) = three_extents(&obj);
    assert!(same(p, d, 1e-9) && same(p, rd, 0.004));
    // The RD cut motion has exactly the stroke segments: no closing segment was added.
    let (s, ids) = document_with(&obj);
    let snap = s.snapshot();
    let strokes: usize = ids.iter().map(|id| snap.object(*id).unwrap().path().subpaths().len()).sum();
    let plan = compile_plan(&snap);
    let file = decode(&rd_bytes(&plan));
    let segments = cuts(&file).len();
    let expected: usize = ids
        .iter()
        .flat_map(|id| snap.object(*id).unwrap().path().subpaths().to_vec())
        .map(|sp| sp.segments().len())
        .sum();
    assert!(strokes >= 4);
    assert_eq!(segments, expected, "one cut per stroke segment, none for a closing edge");
}

#[test]
fn tx01_the_derived_outlines_are_valid_and_a_missing_glyph_is_not_a_failed_evaluation() {
    let (_, lib, _) = fonts();
    let (s, ids) = document_with(&text("H\u{65e5}", &lib));
    assert!(!ids.is_empty());
    let snap = s.snapshot();
    let record = snap
        .derived_record(match snap.object(ids[0]).unwrap().source() {
            GeometrySource::Derived(k) => k.owner,
            _ => panic!("derived"),
        })
        .cloned()
        .unwrap();
    assert_eq!(record.status, EvalStatus::Valid, "outlines of the drawable characters are current");
}

/// Bidirectional reordering is not part of v1 (`burnburn-text/src/shape.rs`): Hebrew is shaped
/// left to right, so the first letter of a right-to-left word lands on the wrong side.
#[test]
#[ignore = "gap: RTL (bidi) reordering is not implemented in burnburn-text v1; see docs/user-flows/acceptatie.md TX01"]
fn tx01_right_to_left_text_is_laid_out_from_the_right() {
    let (src, lib, _) = fonts();
    // "שלום": the logical first letter (shin) is the right-most glyph in correct visual order.
    let shaped = shape(&text("\u{5e9}\u{5dc}\u{5d5}\u{5dd}", &lib), &src).unwrap();
    assert!(shaped.missing.is_empty(), "Liberation Sans carries the Hebrew letters: {:?}", shaped.missing);
    let first = shaped.glyphs.iter().find(|g| g.cluster == 0).unwrap();
    let last = shaped.glyphs.iter().max_by_key(|g| g.cluster).unwrap();
    assert!(first.origin.x > last.origin.x, "the first letter of an RTL word sits to the right of the last");
}

// ---------------------------------------------------------------------------------------------
// TX02 CSV: quoted commas / newlines, missing cell, serial overflow
// ---------------------------------------------------------------------------------------------

fn table(csv: &str) -> Result<DataTable, CsvError> {
    DataTable::parse_csv(csv, ',')
}

#[test]
fn tx02_quoted_commas_newlines_and_quotes_stay_inside_their_cell() {
    let t = table("naam,adres,opm\r\n\"Jansen, J.\",\"Straat 1\nBus 2\",\"zei \"\"hoi\"\"\"\r\nPiet,Dorp 3,\r\n").unwrap();
    assert_eq!(t.headers, ["naam", "adres", "opm"]);
    assert_eq!(
        t.rows,
        vec![
            vec!["Jansen, J.".to_string(), "Straat 1\nBus 2".to_string(), "zei \"hoi\"".to_string()],
            vec!["Piet".to_string(), "Dorp 3".to_string(), String::new()],
        ]
    );
    // Semicolon files with a decimal comma keep their commas.
    let t = DataTable::parse_csv("a;b\n1,5;\"x;y\"\n", ';').unwrap();
    assert_eq!(t.rows, vec![vec!["1,5".to_string(), "x;y".to_string()]]);
}

#[test]
fn tx02_a_row_with_a_missing_cell_is_an_error_at_the_true_line_not_a_shifted_row() {
    // Record 1 spans lines 2-3 (quoted newline), the short record starts on line 4.
    let err = table("a,b\n\"x\ny\",1\nonly-one\n").unwrap_err();
    assert_eq!(err, CsvError::Ragged { line: 4, expected: 2, found: 1 });
    let err = table("a,b\n1,2,3\n").unwrap_err();
    assert_eq!(err, CsvError::Ragged { line: 2, expected: 2, found: 3 });
    assert!(matches!(table("a,b\n\"open,1\n"), Err(CsvError::UnterminatedQuote { line: 2 })));
    assert_eq!(table("a,a\n1,2\n").unwrap_err(), CsvError::DuplicateHeader { name: "a".into() });
    assert_eq!(table("").unwrap_err(), CsvError::Empty);
}

#[test]
fn tx02_csv_bytes_must_be_utf8_a_bom_is_dropped_and_nothing_is_decoded_by_guesswork() {
    let mut with_bom = b"\xEF\xBB\xBF".to_vec();
    with_bom.extend_from_slice("naam\n\u{c5}sa\n".as_bytes());
    let t = DataTable::from_utf8(&with_bom, ',').unwrap();
    assert_eq!(t.headers, ["naam"], "the BOM is not part of the first header");
    assert_eq!(t.rows[0][0], "\u{c5}sa");
    // Latin-1 'Å' (0xC5) is not valid UTF-8: refused with the offset of the first bad byte.
    let latin1 = b"naam\n\xC5sa\n";
    assert_eq!(DataTable::from_utf8(latin1, ',').unwrap_err(), CsvError::NotUtf8 { valid_up_to: 5 });
}

fn record_for(rows: &Arc<FrozenRows>, row: usize, serial: SerialCounter) -> ProductionRecord {
    ProductionRecord::builder(JobClock::new(1_781_751_845, -300).unwrap())
        .serial(serial)
        .row(Arc::clone(rows), RowBinding { rows: rows.hash, row })
        .unwrap()
        .build()
}

#[test]
fn tx02_an_empty_cell_or_unknown_column_stops_the_label_instead_of_printing_a_blank() {
    let rows = Arc::new(FrozenRows::freeze(table("naam,nr\nJansen,7\nPietersen,\n").unwrap(), "2026-06-17"));
    let serial = SerialCounter::new(SerialSpec::new(1, 999, 1, 3, OnEnd::Stop).unwrap());
    assert_eq!(record_for(&rows, 0, serial).resolve("{csv:naam} #{csv:nr} {serial}", 0).unwrap(), "Jansen #7 001");
    assert_eq!(
        record_for(&rows, 1, serial).resolve("{csv:naam} #{csv:nr}", 0).unwrap_err(),
        VariableError::MissingValue { column: "nr".into(), row: 1 }
    );
    assert_eq!(
        record_for(&rows, 0, serial).resolve("{csv:adres}", 0).unwrap_err(),
        VariableError::MissingColumn { column: "adres".into() }
    );
}

#[test]
fn tx02_serial_overflow_stops_or_wraps_by_policy_and_never_truncates_the_digits() {
    // Stop: after 999 the series is finished and the counter stays on 999.
    let mut c = SerialCounter::new(SerialSpec::new(998, 999, 1, 3, OnEnd::Stop).unwrap());
    assert_eq!(c.format(c.current()), "998");
    c.advance().unwrap();
    assert_eq!(c.format(c.current()), "999");
    assert_eq!(c.advance().unwrap_err(), VariableError::SerialExhausted);
    assert_eq!(c.current(), 999, "a refused advance does not move the counter");
    // Wrap: back to the start value.
    let mut w = SerialCounter::new(SerialSpec::new(998, 999, 1, 3, OnEnd::Wrap).unwrap());
    w.advance().unwrap();
    w.advance().unwrap();
    assert_eq!(w.current(), 998);
    // A value wider than the field is printed in full: "1000", never "000".
    let wide = SerialCounter::new(SerialSpec::new(1000, 1002, 1, 3, OnEnd::Stop).unwrap());
    assert_eq!(wide.format(wide.current()), "1000");
    // The extremes of i64 do not overflow.
    let mut m = SerialCounter::new(SerialSpec::new(i64::MAX - 1, i64::MAX, 1, 0, OnEnd::Wrap).unwrap());
    m.advance().unwrap();
    m.advance().unwrap();
    assert_eq!(m.current(), i64::MAX - 1);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

    /// Walking a wrapping series N times visits only its values, in order, and is periodic with
    /// the series length; a stopping series never leaves it.
    #[test]
    fn tx02_property_serial_walk_stays_on_the_series(
        start in -1000_i64..1000, len in 1_i64..40, by in 1_i64..7, width in 0_usize..8, steps in 0_usize..120,
    ) {
        let end = start + (len - 1) * by;
        let mut w = SerialCounter::new(SerialSpec::new(start, end, by, width, OnEnd::Wrap).unwrap());
        let mut s = SerialCounter::new(SerialSpec::new(start, end, by, width, OnEnd::Stop).unwrap());
        for k in 0..steps {
            prop_assert_eq!(w.current(), start + ((k as i64) % len) * by);
            w.advance().unwrap();
            if s.advance().is_err() { prop_assert_eq!(s.current(), end); }
            prop_assert!(s.current() >= start && s.current() <= end);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// TX03 bake, next, auto-advance, failed send (through the app controller and the fake controller)
// ---------------------------------------------------------------------------------------------

use acc_machine::{Rig, TIMEOUT, pump, rig, script_identity, script_upload_acks, upload_and_store};
use burnburn_machine::{Reply, Request};
use burnburn_presentation::{
    AdvancePolicy, CommandKey, HeadlessEditorHarness, SelectMode, SerialField, UiIntent, VariableEdit,
};
use burnburn_test_support::Step;
use burnburn_types::ContentHash;

fn prepare_harness(h: &mut HeadlessEditorHarness) {
    let (src, lib, _) = fonts();
    h.controller().attach_fonts(Arc::new(src), lib);
    h.controller().attach_clock(Arc::new(|| Some(JobClock::new(1_781_751_845, -300).unwrap())));
}

fn variable(h: &mut HeadlessEditorHarness, doc: DocumentId, edit: VariableEdit) {
    h.send(UiIntent::Variable { document: doc, edit });
}

/// The rig's rectangle job plus a variable text `PLAAT-{serial}`, serial 10..95 by 5, four digits.
fn text_rig(content: &str, is_variable: bool) -> Rig {
    let mut r = rig();
    prepare_harness(&mut r.h);
    let doc = r.doc;
    r.h.send(UiIntent::Select { document: doc, ids: vec![], mode: SelectMode::Replace });
    r.h.send(UiIntent::SelectTool { document: Some(doc), id: "sketch.text".into() });
    r.h.click(doc, (20.0, 50.0), Default::default());
    r.h.send(UiIntent::InsertText { document: doc, text: content.into() });
    r.h.invoke(doc, CommandKey::ToolSelect);
    if is_variable {
        variable(&mut r.h, doc, VariableEdit::SetVariable(true));
        for (field, value) in [
            (SerialField::Start, "10"),
            (SerialField::End, "95"),
            (SerialField::AdvanceBy, "5"),
            (SerialField::Digits, "4"),
        ] {
            variable(&mut r.h, doc, VariableEdit::Serial(field, value.into()));
        }
    }
    r.h.send(UiIntent::AddOperation { document: doc, draft: acc_machine::cut() });
    r
}

fn job_hash(r: &mut Rig) -> ContentHash {
    r.h.invoke(r.doc, CommandKey::Compile);
    r.h.run_tasks();
    r.h.invoke(r.doc, CommandKey::PrepareJob);
    r.h.run_tasks();
    r.h.document(r.doc).unwrap().job.expect("prepared job").rd_hash
}

fn serial(r: &Rig) -> String {
    r.h.document(r.doc).unwrap().variable_panel.unwrap().serial.current
}

#[test]
fn tx03_next_bake_and_literal_text_all_print_the_same_bytes_for_the_same_number() {
    let mut var = text_rig("SN-{serial}", true);
    let mut literal_10 = text_rig("SN-0010", false);
    let mut literal_15 = text_rig("SN-0015", false);
    let h10 = job_hash(&mut var);
    assert_eq!(h10, job_hash(&mut literal_10), "the variable job equals the literal one");

    // Next: the serial moves, the next job is the literal text of the next number.
    variable(&mut var.h, var.doc, VariableEdit::Next);
    assert_eq!(serial(&var), "15");
    let h15 = job_hash(&mut var);
    assert_ne!(h10, h15);
    assert_eq!(h15, job_hash(&mut literal_15));

    // Bake: the template is replaced by the resolved text in one undo step and prints the same.
    variable(&mut var.h, var.doc, VariableEdit::Bake);
    assert_eq!(var.h.text_contents(var.doc).last().map(String::as_str), Some("SN-0015"));
    assert_eq!(job_hash(&mut var), h15, "baking changes nothing about the output");
    var.h.invoke(var.doc, CommandKey::Undo);
    assert_eq!(var.h.text_contents(var.doc).last().map(String::as_str), Some("SN-{serial}"), "one undo un-bakes");
}

#[test]
fn tx03_a_failed_send_never_consumes_a_number_and_a_stored_one_consumes_exactly_one() {
    let mut r = text_rig("SN-{serial}", true);
    let doc = r.doc;
    variable(&mut r.h, doc, VariableEdit::Policy(AdvancePolicy::OnUploadConfirmed));
    let first = acc_machine::prepare(&mut r.h, doc);
    let bytes_first = first.rd_hash;

    // Failed send: the controller refuses the first chunk.
    script_identity(&r.peer, acc_machine::MODEL);
    r.peer.script(
        Step::expect_matching("chunk 0", |q| matches!(q, Request::UploadChunk { index: 0, .. })).reply(Reply::Nak),
    );
    r.h.press(Some(doc), first.upload.as_ref().unwrap());
    pump(&r);
    r.h.controller().sync_production();
    assert_eq!(serial(&r), "10", "a refused upload consumed no number");

    // Retry with the same job: same bytes, same number, and now it works.
    let again = acc_machine::prepare(&mut r.h, doc);
    assert_eq!(again.rd_hash, bytes_first, "the retry prints the same number");
    upload_and_store(&mut r, &again);
    r.h.controller().sync_production();
    assert_eq!(serial(&r), "15", "exactly one number consumed");
    r.h.controller().sync_production();
    assert_eq!(serial(&r), "15", "never twice for the same job");

    // The next job is the next number: no duplicate, no gap.
    let next = acc_machine::prepare(&mut r.h, doc);
    assert_ne!(next.rd_hash, bytes_first);
    let mut literal_15 = text_rig("SN-0015", false);
    assert_eq!(next.rd_hash, job_hash(&mut literal_15));
}

#[test]
fn tx03_auto_advance_is_only_by_the_chosen_event_and_manual_never_moves() {
    let mut r = text_rig("SN-{serial}", true);
    let doc = r.doc;
    variable(&mut r.h, doc, VariableEdit::Policy(AdvancePolicy::Manual));
    let job = acc_machine::prepare(&mut r.h, doc);
    upload_and_store(&mut r, &job);
    r.h.controller().sync_production();
    assert_eq!(serial(&r), "10", "Manual: stored does not advance");

    let mut r = text_rig("SN-{serial}", true);
    let doc = r.doc;
    variable(&mut r.h, doc, VariableEdit::Policy(AdvancePolicy::OnUploadConfirmed));
    let job = acc_machine::prepare(&mut r.h, doc);
    script_upload_acks(&r.peer, &job);
    r.h.press(Some(doc), job.upload.as_ref().unwrap());
    pump(&r);
    r.h.controller().sync_production();
    assert_eq!(serial(&r), "10", "ACKs only: not yet stored, not yet counted");
    let _ = TIMEOUT;
}
