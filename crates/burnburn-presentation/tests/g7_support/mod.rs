//! Shared helpers of the golf-7 trace tests: a PNG with a 254 dpi header, the tracer opened on
//! bytes, and short ways to send the preparation and pointing intents.

#![allow(dead_code, clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path as FsPath, PathBuf};

use burnburn_image::encode_png_gray8;
use burnburn_presentation::{
    DocumentViewModel, HeadlessEditorHarness, TraceFormInput, TracePick, TracePrepIntent, TracePrepViewModel,
    TracerViewModel, UiIntent,
};
use burnburn_types::DocumentId;

pub fn crc32(data: &[u8]) -> u32 {
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
pub fn png_of(gray: &[u8], w: u32, h: u32) -> Vec<u8> {
    png_of_dpi(gray, w, h, 10_000)
}

/// A gray PNG whose header says `pixels_per_metre` (1000 = 25.4 dpi = 1 mm per pixel).
pub fn png_of_dpi(gray: &[u8], w: u32, h: u32, pixels_per_metre: u32) -> Vec<u8> {
    let plain = encode_png_gray8(w, h, gray).unwrap();
    let mut data = pixels_per_metre.to_be_bytes().to_vec();
    data.extend_from_slice(&pixels_per_metre.to_be_bytes());
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

pub fn gray_image(w: usize, h: usize, pixel: impl Fn(usize, usize) -> u8) -> Vec<u8> {
    let mut gray = vec![255u8; w * h];
    for y in 0..h {
        for x in 0..w {
            gray[y * w + x] = pixel(x, y);
        }
    }
    gray
}

pub fn image(w: usize, h: usize, pixel: impl Fn(usize, usize) -> u8) -> Vec<u8> {
    png_of(&gray_image(w, h, pixel), w as u32, h as u32)
}

pub fn inside_disc(x: usize, y: usize, cx: f64, cy: f64, r: f64) -> bool {
    (x as f64 + 0.5 - cx).hypot(y as f64 + 0.5 - cy) <= r
}

/// 200 x 120 px: a ring (50, 60) r 35 with a light-gray counter (150, r 15); a black rectangle
/// x 110…180, y 20…100 with a faint bump (180) on its left edge; a faint speck (170) at (88, 100) and a
/// small black disc far away at (20, 15). At a global threshold of 127: ring with its counter,
/// rectangle and the far disc (three parts, one hole).
pub fn scene() -> Vec<u8> {
    png_of(&scene_gray(), 200, 120)
}

/// The pixels of [`scene`] (200 × 120).
pub fn scene_gray() -> Vec<u8> {
    gray_image(200, 120, |x, y| {
        let mut v = 255u8;
        if inside_disc(x, y, 50.0, 60.0, 35.0) {
            v = 0;
        }
        if inside_disc(x, y, 50.0, 60.0, 15.0) {
            v = 150;
        }
        if (110..180).contains(&x) && (20..100).contains(&y) {
            v = 0;
        }
        if inside_disc(x, y, 110.0, 60.0, 8.0) && v == 255 {
            v = 180;
        }
        if inside_disc(x, y, 88.0, 100.0, 5.0) {
            v = 170;
        }
        if inside_disc(x, y, 20.0, 15.0, 8.0) {
            v = 0;
        }
        v
    })
}

pub fn opened_with(bytes: &[u8]) -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    burnburn_io::fs::FileSystem::write_all(h.fs(), FsPath::new("/work/in.png"), bytes).unwrap();
    let doc = h.new_document().expect("window");
    h.send(UiIntent::OpenTraceSource { document: doc, path: PathBuf::from("/work/in.png") });
    h.run_tasks();
    (h, doc)
}

/// The scene above with the global threshold fixed at 127.
pub fn opened_scene() -> (HeadlessEditorHarness, DocumentId) {
    let (mut h, doc) = opened_with(&scene());
    edit(&mut h, doc, |f| {
        f.threshold_choice = burnburn_presentation::ThresholdChoice::Fixed;
        f.threshold = "127".to_owned();
    });
    (h, doc)
}

pub fn tracer(h: &HeadlessEditorHarness, doc: DocumentId) -> TracerViewModel {
    h.document(doc).unwrap().tracer.expect("tracer panel")
}

pub fn vm(h: &HeadlessEditorHarness, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).unwrap()
}

pub fn prep(h: &HeadlessEditorHarness, doc: DocumentId) -> TracePrepViewModel {
    vm(h, doc).trace_prep.expect("the preparation panel")
}

pub fn edit(h: &mut HeadlessEditorHarness, doc: DocumentId, change: impl FnOnce(&mut TraceFormInput)) {
    let mut input = tracer(h, doc).form.input;
    change(&mut input);
    h.send(UiIntent::SetTraceForm { document: doc, input: Box::new(input) });
    h.run_tasks();
}

pub fn send(h: &mut HeadlessEditorHarness, doc: DocumentId, intent: TracePrepIntent) {
    h.send(UiIntent::TracePrep { document: doc, intent });
    h.run_tasks();
}

pub fn pick(h: &mut HeadlessEditorHarness, doc: DocumentId, pick: TracePick) {
    h.send(UiIntent::TracePick { document: doc, pick });
    h.run_tasks();
}

pub fn retrace(h: &mut HeadlessEditorHarness, doc: DocumentId, rect: [&str; 4], threshold: &str, anchors: &str) {
    send(
        h,
        doc,
        TracePrepIntent::RetraceRegion {
            x: rect[0].to_owned(),
            y: rect[1].to_owned(),
            width: rect[2].to_owned(),
            height: rect[3].to_owned(),
            threshold: threshold.to_owned(),
            anchors: anchors.to_owned(),
        },
    );
}

pub fn counts(h: &HeadlessEditorHarness, doc: DocumentId) -> (usize, usize) {
    let r = tracer(h, doc).result.expect("a trace result");
    (r.components, r.holes)
}

pub fn subpaths(h: &HeadlessEditorHarness, doc: DocumentId) -> Vec<burnburn_geometry::Subpath> {
    tracer(h, doc).result.expect("a trace result").preview.subpaths().to_vec()
}

pub fn s(text: &str) -> String {
    text.to_owned()
}
