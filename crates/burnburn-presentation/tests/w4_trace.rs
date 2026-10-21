//! W4-UI-gaten, tracefamilie (T01–T08): gaten uit de storydekking die in de UI bedienbaar zijn.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path as FsPath, PathBuf};

use burnburn_document::UndoLabel;
use burnburn_document::trace::TraceLabel;
use burnburn_image::{DecodeLimits, decode, encode_png_gray8};
use burnburn_io::load_project;
use burnburn_presentation::{
    CommandKey, DocumentViewModel, HeadlessEditorHarness, SelectMode, TraceFormInput, TracerViewModel, UiIntent,
};
use burnburn_types::DocumentId;

fn put(h: &HeadlessEditorHarness, name: &str, bytes: &[u8]) -> PathBuf {
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    let path = PathBuf::from(format!("/work/{name}"));
    burnburn_io::fs::FileSystem::write_all(h.fs(), &path, bytes).unwrap();
    path
}

/// A black disc (radius 20 px) and a 2 × 2 px speck on white, 64 × 64, with 254 dpi (0.1 mm/px).
fn disc_and_speck_png() -> Vec<u8> {
    let mut gray = vec![255u8; 64 * 64];
    for y in 0..64i32 {
        for x in 0..64i32 {
            let inside = (x - 32).pow(2) + (y - 32).pow(2) <= 20 * 20;
            let speck = (2..4).contains(&x) && (2..4).contains(&y);
            if inside || speck {
                gray[(y * 64 + x) as usize] = 0;
            }
        }
    }
    let plain = encode_png_gray8(64, 64, &gray).unwrap();
    // pHYs after IHDR: 10 000 pixels per metre on both axes, unit metre.
    let mut data = 10_000u32.to_be_bytes().to_vec();
    data.extend(10_000u32.to_be_bytes());
    data.push(1);
    let mut body = b"pHYs".to_vec();
    body.extend(&data);
    let mut crc = 0xFFFF_FFFFu32;
    for &b in &body {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    let mut chunk = (data.len() as u32).to_be_bytes().to_vec();
    chunk.extend(&body);
    chunk.extend((!crc).to_be_bytes());
    let mut out = plain[..33].to_vec();
    out.extend(chunk);
    out.extend(&plain[33..]);
    out
}

fn open() -> (HeadlessEditorHarness, DocumentId, Vec<u8>) {
    let mut h = HeadlessEditorHarness::new();
    let bytes = disc_and_speck_png();
    let path = put(&h, "in.png", &bytes);
    let doc = h.new_document().expect("window");
    h.send(UiIntent::OpenTraceSource { document: doc, path });
    h.run_tasks();
    (h, doc, bytes)
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

// ---------------------------------------------------------------------------------------------
// T01
// ---------------------------------------------------------------------------------------------

/// Setting the scale only changes the physical size: the source keeps its original pixels, the
/// project stores the original file bytes, and no resampled copy is made.
#[test]
fn image_scale_keeps_original_pixels() {
    let (mut h, doc, bytes) = open();
    let original = decode(&bytes, DecodeLimits::default()).unwrap();
    let at_254 = tracer(&h, doc).result.expect("traced at the file's scale");
    edit(&mut h, doc, |f| f.dpi = "508".into());
    h.run_tasks();
    let t = tracer(&h, doc);
    assert_eq!((t.width_px, t.height_px), (64, 64), "the pixel grid does not change with the scale");
    let at_508 = t.result.expect("traced at the new scale");
    assert!((at_508.width_mm - at_254.width_mm / 2.0).abs() < 0.05, "only the physical size follows the scale");

    h.invoke(doc, CommandKey::ApplyTrace);
    let project = save_and_load(&mut h, doc);
    let stored: Vec<_> = project.assets.iter().collect();
    assert_eq!(stored.len(), 1, "one source asset, no derived copies");
    assert_eq!(stored[0].1.bytes.as_ref(), bytes.as_slice(), "the original file bytes");
    let decoded = decode(&stored[0].1.bytes, DecodeLimits::default()).unwrap();
    assert_eq!(decoded.content_hash(), original.content_hash(), "the original pixels");
}

// ---------------------------------------------------------------------------------------------
// T02: toepassen bewaart recept en engineversie als herbewerkbare bron
// ---------------------------------------------------------------------------------------------

/// "Pas toe als trace" stores the source, recipe and engine version as a trace of the document
/// (one undo entry); the result objects belong to it.
#[test]
fn applying_linked_stores_the_recipe_as_a_trace_in_one_undo_entry() {
    let (mut h, doc, bytes) = open();
    edit(&mut h, doc, |f| f.smoothness_mm = "0,2".into());
    h.run_tasks();
    let recipe_before = tracer(&h, doc).form.input;
    assert!(h.view().commands[&CommandKey::ApplyTraceLinked].enabled);

    h.invoke(doc, CommandKey::ApplyTraceLinked);
    let after = vm(&h, doc);
    assert_eq!(after.undo, Some(UndoLabel::Trace(TraceLabel::Apply)));
    assert_eq!(after.objects.len(), 2, "disc and speck");
    let project = save_and_load(&mut h, doc);
    let traces: Vec<_> = project.document.engine_layer().traces().collect();
    assert_eq!(traces.len(), 1, "the recipe is kept as a trace");
    let (_, definition) = traces[0];
    assert_eq!(definition.recipe.engine, burnburn_trace::ENGINE_VERSION, "engine and version travel along");
    let stored = project.assets.iter().find(|(h, _)| **h == definition.source).expect("the source asset");
    assert_eq!(stored.1.bytes.as_ref(), bytes.as_slice());
    let recipe = burnburn_trace::recipe_from_engine(&definition.recipe).unwrap();
    assert!((recipe.fit.tolerance_mm - 0.2).abs() < 1e-12, "the typed smoothness is in the recipe");

    h.invoke(doc, CommandKey::Undo);
    assert!(vm(&h, doc).objects.is_empty(), "one undo removes the trace and its objects");
    let _ = recipe_before;
}

/// The plain "Pas trace toe" keeps working: editable shapes without a link.
#[test]
fn the_plain_apply_still_makes_ordinary_editable_shapes() {
    let (mut h, doc, _) = open();
    h.invoke(doc, CommandKey::ApplyTrace);
    let project = save_and_load(&mut h, doc);
    assert_eq!(project.document.engine_layer().traces().count(), 0);
    assert_eq!(vm(&h, doc).objects.len(), 2);
}

/// A traced object can be sent back to the tracer with its stored recipe; applying again replaces
/// the outputs of the same trace (the objects keep their identity).
#[test]
fn a_linked_trace_reopens_in_the_tracer_with_its_recipe_and_retraces_in_place() {
    let (mut h, doc, _) = open();
    edit(&mut h, doc, |f| {
        f.dpi = "508".into();
        f.smoothness_mm = "0,2".into();
    });
    h.run_tasks();
    h.invoke(doc, CommandKey::ApplyTraceLinked);
    let ids: Vec<_> = vm(&h, doc).objects.iter().map(|o| o.id).collect();
    h.invoke(doc, CommandKey::CloseTracer);
    assert!(vm(&h, doc).tracer.is_none());
    assert!(!h.view().commands[&CommandKey::TraceEdit].enabled, "nothing selected");

    h.send(UiIntent::Select { document: doc, ids: vec![ids[0]], mode: SelectMode::Replace });
    assert!(h.view().commands[&CommandKey::TraceEdit].enabled, "a traced object is selected");
    h.invoke(doc, CommandKey::TraceEdit);
    h.run_tasks();
    let t = tracer(&h, doc);
    assert_eq!((t.form.input.dpi.as_str(), t.form.input.smoothness_mm.as_str()), ("508", "0.2"), "{:?}", t.form.input);
    assert!(t.result.is_some(), "traced again from the stored source");

    edit(&mut h, doc, |f| f.smoothness_mm = "0,5".into());
    h.run_tasks();
    h.invoke(doc, CommandKey::ApplyTraceLinked);
    let after = vm(&h, doc);
    assert_eq!(after.undo, Some(UndoLabel::Trace(TraceLabel::Retrace)));
    assert_eq!(after.objects.iter().map(|o| o.id).collect::<Vec<_>>(), ids, "same objects, new geometry");
    let project = save_and_load(&mut h, doc);
    assert_eq!(project.document.engine_layer().traces().count(), 1, "replaced, not added");
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(vm(&h, doc).objects.len(), 2, "the retrace is one undo entry");
}
