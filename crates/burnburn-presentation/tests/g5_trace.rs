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

// ---------------------------------------------------------------------------------------------
// T04/T08: knooppunten beslissen en technische vormen bevestigen
// ---------------------------------------------------------------------------------------------

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

/// A ring: centre (32, 32), outer radius 20 px, inner 14 px.
fn ring_png() -> Vec<u8> {
    let mut gray = vec![255u8; 64 * 64];
    for y in 0..64i32 {
        for x in 0..64i32 {
            let d2 = (x - 32).pow(2) + (y - 32).pow(2);
            if (14 * 14..=20 * 20).contains(&d2) {
                gray[(y * 64 + x) as usize] = 0;
            }
        }
    }
    png_of(&gray, 64, 64)
}

fn centre_lines_of(bytes: &[u8]) -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    burnburn_io::fs::FileSystem::write_all(h.fs(), FsPath::new("/work/in.png"), bytes).unwrap();
    let doc = h.new_document().expect("window");
    h.send(UiIntent::OpenTraceSource { document: doc, path: PathBuf::from("/work/in.png") });
    h.run_tasks();
    edit(&mut h, doc, |f| f.mode = TraceMode::Centerline);
    h.run_tasks();
    (h, doc)
}

/// At a T the tracer does not decide which branches continue each other: three strokes end at the
/// junction until the operator picks a pairing; then two strokes remain and the choice travels
/// with the trace that is applied.
#[test]
fn an_ambiguous_junction_stays_open_until_the_operator_picks_a_pairing() {
    use burnburn_presentation::{CenterlineEdit, JunctionState};
    let (mut h, doc) = centre_lines_of(&t_png());
    let panel = vm(&h, doc).centerline.expect("centre-line editing is offered in centre-line mode");
    assert_eq!(panel.junctions.len(), 1, "one T junction");
    assert!(matches!(panel.junctions[0].state, JunctionState::Ambiguous { candidates: 3 }));
    assert_eq!(tracer(&h, doc).result.unwrap().components, 3, "every stroke ends at the junction");

    let node = panel.junctions[0].node;
    h.send(UiIntent::CenterlineEdit { document: doc, edit: CenterlineEdit::ResolveJunction { node, candidate: 0 } });
    let after = tracer(&h, doc);
    assert_eq!(after.result.unwrap().components, 2, "two branches now run through the junction");
    assert!(matches!(vm(&h, doc).centerline.unwrap().junctions[0].state, JunctionState::Resolved));

    // A candidate that does not exist, or a junction that does not exist, changes nothing.
    let edits = vm(&h, doc).centerline.unwrap().edits;
    h.send(UiIntent::CenterlineEdit { document: doc, edit: CenterlineEdit::ResolveJunction { node, candidate: 9 } });
    h.send(UiIntent::CenterlineEdit {
        document: doc,
        edit: CenterlineEdit::ResolveJunction { node: 999, candidate: 0 },
    });
    assert_eq!(vm(&h, doc).centerline.unwrap().edits, edits);

    h.invoke(doc, CommandKey::ApplyTraceLinked);
    assert_eq!(vm(&h, doc).objects.len(), 2, "the decision decides what is applied");
    let project = save_and_load(&mut h, doc);
    let (_, definition) = project.document.engine_layer().traces().next().unwrap();
    assert!(definition.recipe.get("centerline.edits").is_some(), "operator decisions are recorded");
}

/// An accepted circle candidate replaces the free curve of a ring by an exact circle; nothing is
/// replaced until the operator accepts, and a straight bar offers a line.
#[test]
fn an_accepted_circle_candidate_becomes_a_primitive() {
    use burnburn_presentation::{CenterlineEdit, PrimitiveKind};
    let (mut h, doc) = centre_lines_of(&ring_png());
    let panel = vm(&h, doc).centerline.expect("panel");
    assert_eq!(panel.chains.len(), 1, "one closed loop");
    let chain = &panel.chains[0];
    assert!(chain.confirmed.is_none(), "nothing is replaced on its own");
    let circle = chain.candidates.iter().find(|c| c.kind == PrimitiveKind::Circle).expect("a circle candidate");
    assert!(circle.within_tolerance && circle.max_residual_mm < 0.12, "{circle:?}");

    h.send(UiIntent::CenterlineEdit { document: doc, edit: CenterlineEdit::ConfirmPrimitive { chain: chain.chain } });
    let after = vm(&h, doc).centerline.unwrap();
    assert_eq!(after.chains[0].confirmed, Some(PrimitiveKind::Circle));
    h.invoke(doc, CommandKey::ApplyTrace);
    let scene = h.scene(doc).unwrap();
    let path = &scene.items[0].path;
    assert!(
        path.subpaths().iter().flat_map(|s| s.segments()).all(|s| matches!(s, burnburn_geometry::Segment::Arc(_))),
        "an exact circle, not a polyline"
    );
}
