//! G6-werk, T04: the operator's decisions on a centre-line trace belong to its recipe. Reopening the
//! trace, or tracing the same source with other settings, applies them where they still fit; a
//! decision that no longer fits is counted and never applied somewhere else.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path as FsPath, PathBuf};

use burnburn_image::encode_png_gray8;
use burnburn_presentation::{
    CenterlineEdit, CommandKey, DocumentViewModel, HeadlessEditorHarness, JunctionState, SelectMode, TraceFormInput,
    TraceMode, TracerViewModel, UiIntent,
};
use burnburn_types::DocumentId;

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

/// 254 dpi (0.1 mm per pixel) in the file.
fn png_of(gray: &[u8], w: u32, h: u32) -> Vec<u8> {
    let plain = encode_png_gray8(w, h, gray).unwrap();
    let mut data = 10_000u32.to_be_bytes().to_vec();
    data.extend_from_slice(&10_000u32.to_be_bytes());
    data.push(1);
    let mut body = b"pHYs".to_vec();
    body.extend_from_slice(&data);
    let mut chunk = (data.len() as u32).to_be_bytes().to_vec();
    chunk.extend_from_slice(&body);
    chunk.extend_from_slice(&crc32(&body).to_be_bytes());
    let mut out = plain[..33].to_vec();
    out.extend_from_slice(&chunk);
    out.extend_from_slice(&plain[33..]);
    out
}

/// A T of 6 px thick strokes: a bar over the top and a stem down from its middle.
fn t_png() -> Vec<u8> {
    let mut gray = vec![255u8; 64 * 64];
    for y in 0..64usize {
        for x in 0..64usize {
            let bar = (10..54).contains(&x) && (10..16).contains(&y);
            let stem = (29..35).contains(&x) && (10..56).contains(&y);
            if bar || stem {
                gray[y * 64 + x] = 0;
            }
        }
    }
    png_of(&gray, 64, 64)
}

fn tracer(h: &HeadlessEditorHarness, doc: DocumentId) -> TracerViewModel {
    h.document(doc).unwrap().tracer.expect("tracer panel")
}

fn edit(h: &mut HeadlessEditorHarness, doc: DocumentId, change: impl FnOnce(&mut TraceFormInput)) {
    let mut input = tracer(h, doc).form.input;
    change(&mut input);
    h.send(UiIntent::SetTraceForm { document: doc, input: Box::new(input) });
}

fn vm(h: &HeadlessEditorHarness, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).unwrap()
}

/// The T traced as centre lines, its junction decided, applied as a trace and the tracer closed.
fn decided_t() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    burnburn_io::fs::FileSystem::write_all(h.fs(), FsPath::new("/work/in.png"), &t_png()).unwrap();
    let doc = h.new_document().expect("window");
    h.send(UiIntent::OpenTraceSource { document: doc, path: PathBuf::from("/work/in.png") });
    h.run_tasks();
    edit(&mut h, doc, |f| f.mode = TraceMode::Centerline);
    h.run_tasks();
    let node = vm(&h, doc).centerline.unwrap().junctions[0].node;
    h.send(UiIntent::CenterlineEdit { document: doc, edit: CenterlineEdit::ResolveJunction { node, candidate: 1 } });
    assert_eq!(tracer(&h, doc).result.unwrap().components, 2);
    (h, doc)
}

fn reopen(h: &mut HeadlessEditorHarness, doc: DocumentId) {
    h.invoke(doc, CommandKey::ApplyTraceLinked);
    let ids: Vec<_> = vm(h, doc).objects.iter().map(|o| o.id).collect();
    h.invoke(doc, CommandKey::CloseTracer);
    h.send(UiIntent::Select { document: doc, ids: vec![ids[0]], mode: SelectMode::Replace });
    h.invoke(doc, CommandKey::TraceEdit);
    h.run_tasks();
}

#[test]
fn a_decided_centreline_trace_reopens_with_its_decision_applied() {
    let (mut h, doc) = decided_t();
    reopen(&mut h, doc);
    let panel = vm(&h, doc).centerline.expect("centre-line editing is offered again");
    assert!(matches!(panel.junctions[0].state, JunctionState::Resolved), "{:?}", panel.junctions);
    assert_eq!(panel.dropped, 0);
    assert_eq!(tracer(&h, doc).result.unwrap().components, 2, "the strokes the operator decided on");
}

#[test]
fn tracing_again_with_another_fit_budget_keeps_the_decision() {
    let (mut h, doc) = decided_t();
    edit(&mut h, doc, |f| f.smoothness_mm = "0,3".into());
    h.run_tasks();
    let panel = vm(&h, doc).centerline.unwrap();
    assert!(matches!(panel.junctions[0].state, JunctionState::Resolved));
    assert_eq!(tracer(&h, doc).result.unwrap().components, 2);
    assert_eq!(panel.dropped, 0);
}

#[test]
fn a_decision_whose_place_moved_is_dropped_and_counted_not_applied_elsewhere() {
    let (mut h, doc) = decided_t();
    // Another declared resolution moves every point in millimetres: the junction is not where the
    // decision was taken.
    edit(&mut h, doc, |f| f.dpi = "508".into());
    h.run_tasks();
    let panel = vm(&h, doc).centerline.unwrap();
    assert_eq!(panel.dropped, 1);
    assert!(matches!(panel.junctions[0].state, JunctionState::Ambiguous { .. }), "undecided again");
    assert_eq!(tracer(&h, doc).result.unwrap().components, 3);
}

#[test]
fn a_new_decision_after_a_retrace_is_added_to_the_decisions_in_force() {
    let (mut h, doc) = decided_t();
    edit(&mut h, doc, |f| f.smoothness_mm = "0,3".into());
    h.run_tasks();
    // Another decision on top: the T has only one junction; the three chains can be accepted as lines.
    let chain = vm(&h, doc).centerline.unwrap().chains[0].chain;
    h.send(UiIntent::CenterlineEdit { document: doc, edit: CenterlineEdit::ConfirmPrimitive { chain } });
    assert_eq!(vm(&h, doc).centerline.unwrap().edits, 2, "two decisions in force");
    edit(&mut h, doc, |f| f.smoothness_mm = "0,2".into());
    h.run_tasks();
    let panel = vm(&h, doc).centerline.unwrap();
    assert_eq!((panel.edits, panel.dropped), (2, 0), "both come back after another trace");
}
