//! G5-UI-gaten, tracefamilie: een centerline-trace als herbewerkbare bron (T04).

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path as FsPath, PathBuf};

use burnburn_document::UndoLabel;
use burnburn_document::trace::TraceLabel;
use burnburn_image::encode_png_gray8;
use burnburn_io::load_project;
use burnburn_presentation::{
    CommandKey, DocumentViewModel, HeadlessEditorHarness, SelectMode, TraceFormInput, TraceMode, TracerViewModel,
    UiIntent,
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

/// A black bar of 40 x 6 px on white, 64 x 16, at 254 dpi (0.1 mm per pixel).
fn bar_png() -> Vec<u8> {
    let mut gray = vec![255u8; 64 * 16];
    for y in 5..11 {
        for x in 10..50 {
            gray[y * 64 + x] = 0;
        }
    }
    let plain = encode_png_gray8(64, 16, &gray).unwrap();
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

fn centre_lines() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    burnburn_io::fs::FileSystem::write_all(h.fs(), FsPath::new("/work/in.png"), &bar_png()).unwrap();
    let doc = h.new_document().expect("window");
    h.send(UiIntent::OpenTraceSource { document: doc, path: PathBuf::from("/work/in.png") });
    h.run_tasks();
    edit(&mut h, doc, |f| f.mode = TraceMode::Centerline);
    h.run_tasks();
    (h, doc)
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

fn save_and_load(h: &mut HeadlessEditorHarness, doc: DocumentId) -> burnburn_io::Project {
    h.fs().create_dir_all_for_test(FsPath::new("/out"));
    let path = PathBuf::from("/out/p.burnburn");
    h.send(UiIntent::Save { document: doc, path: Some(path.clone()) });
    h.run_tasks();
    load_project(h.fs(), &path).expect("saved project")
}

/// "Pas toe als trace" also works for a centre line: an open path, kept as a trace with the
/// centreline engine, one undo entry.
#[test]
fn applying_a_centre_line_linked_stores_a_centreline_trace_with_an_open_path() {
    let (mut h, doc) = centre_lines();
    assert!(h.view().commands[&CommandKey::ApplyTraceLinked].enabled, "also in centre-line mode");
    h.invoke(doc, CommandKey::ApplyTraceLinked);
    let after = vm(&h, doc);
    assert_eq!(after.undo, Some(UndoLabel::Trace(TraceLabel::Apply)));
    assert_eq!(after.objects.len(), 1, "the bar has one centre line");
    let scene = h.scene(doc).unwrap();
    assert!(scene.items[0].path.subpaths().iter().all(|s| !s.is_closed()), "open path stays open");
    let project = save_and_load(&mut h, doc);
    let traces: Vec<_> = project.document.engine_layer().traces().collect();
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].1.recipe.engine, burnburn_trace::CENTERLINE_ENGINE_VERSION);
    h.invoke(doc, CommandKey::Undo);
    assert!(vm(&h, doc).objects.is_empty());
}

/// A stored centre-line trace reopens in centre-line mode with its recipe; applying again replaces
/// the strokes of the same trace and the objects keep their identity.
#[test]
fn a_centreline_trace_reopens_in_centreline_mode_and_retraces_in_place() {
    let (mut h, doc) = centre_lines();
    edit(&mut h, doc, |f| f.smoothness_mm = "0,2".into());
    h.run_tasks();
    h.invoke(doc, CommandKey::ApplyTraceLinked);
    let ids: Vec<_> = vm(&h, doc).objects.iter().map(|o| o.id).collect();
    h.invoke(doc, CommandKey::CloseTracer);
    h.send(UiIntent::Select { document: doc, ids: vec![ids[0]], mode: SelectMode::Replace });
    assert!(h.view().commands[&CommandKey::TraceEdit].enabled);
    h.invoke(doc, CommandKey::TraceEdit);
    h.run_tasks();
    let t = tracer(&h, doc);
    assert_eq!(t.form.input.mode, TraceMode::Centerline, "the stored engine decides the mode");
    assert_eq!(t.form.input.smoothness_mm, "0.2");
    assert!(t.result.is_some(), "traced again from the stored source");

    edit(&mut h, doc, |f| f.smoothness_mm = "0,4".into());
    h.run_tasks();
    h.invoke(doc, CommandKey::ApplyTraceLinked);
    let after = vm(&h, doc);
    assert_eq!(after.undo, Some(UndoLabel::Trace(TraceLabel::Retrace)));
    assert_eq!(after.objects.iter().map(|o| o.id).collect::<Vec<_>>(), ids);
    assert_eq!(save_and_load(&mut h, doc).document.engine_layer().traces().count(), 1, "replaced, not added");
}
