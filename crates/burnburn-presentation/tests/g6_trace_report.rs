//! G6-werk, T06: what simplifying cost. The fit budget is in millimetres; the report names the
//! largest deviations with their place, and every part or hole the "ignore less than" filter took
//! away with where it was, also when it sits inside another shape.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path as FsPath, PathBuf};

use burnburn_image::encode_png_gray8;
use burnburn_presentation::{
    DocumentViewModel, HeadlessEditorHarness, PrepReport, ThresholdChoice, TraceFormInput, TracerViewModel, UiIntent,
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

fn inside_disc(x: usize, y: usize, cx: f64, cy: f64, r: f64) -> bool {
    (x as f64 + 0.5 - cx).hypot(y as f64 + 0.5 - cy) <= r
}

/// 200 x 120 px (20 x 12 mm): a big disc at (60, 60) r 40; a speck at (150, 40) r 5 (0.785 mm²); a ring
/// at (150, 85) r 20 with a tiny counter r 3 (0.283 mm²) in its middle.
fn scene() -> Vec<u8> {
    let (w, h) = (200usize, 120usize);
    let mut gray = vec![255u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut black = inside_disc(x, y, 60.0, 60.0, 40.0)
                || inside_disc(x, y, 150.0, 40.0, 5.0)
                || inside_disc(x, y, 150.0, 85.0, 20.0);
            if inside_disc(x, y, 150.0, 85.0, 3.0) {
                black = false;
            }
            if black {
                gray[y * w + x] = 0;
            }
        }
    }
    png_of(&gray, w as u32, h as u32)
}

fn opened() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    burnburn_io::fs::FileSystem::write_all(h.fs(), FsPath::new("/work/in.png"), &scene()).unwrap();
    let doc = h.new_document().expect("window");
    h.send(UiIntent::OpenTraceSource { document: doc, path: PathBuf::from("/work/in.png") });
    h.run_tasks();
    edit(&mut h, doc, |f| {
        f.threshold_choice = ThresholdChoice::Fixed;
        f.threshold = "127".to_owned();
        f.ignore_below_mm2 = "0".to_owned();
    });
    (h, doc)
}

fn tracer(h: &HeadlessEditorHarness, doc: DocumentId) -> TracerViewModel {
    h.document(doc).unwrap().tracer.expect("tracer panel")
}

fn vm(h: &HeadlessEditorHarness, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).unwrap()
}

fn edit(h: &mut HeadlessEditorHarness, doc: DocumentId, change: impl FnOnce(&mut TraceFormInput)) {
    let mut input = tracer(h, doc).form.input;
    change(&mut input);
    h.send(UiIntent::SetTraceForm { document: doc, input: Box::new(input) });
    h.run_tasks();
}

fn report(h: &HeadlessEditorHarness, doc: DocumentId) -> PrepReport {
    vm(h, doc).trace_prep.expect("panel").report.expect("a report once there is a result")
}

#[test]
fn the_report_names_the_largest_deviations_in_mm_with_their_place_and_the_budget_in_force() {
    let (mut h, doc) = opened();
    edit(&mut h, doc, |f| f.smoothness_mm = "0,2".to_owned());
    let r = report(&h, doc);
    assert_eq!(r.tolerance_mm, 0.2);
    assert!(!r.worst.is_empty() && r.worst.len() <= 3);
    assert!(r.worst.windows(2).all(|w| w[0].mm >= w[1].mm), "largest first");
    assert_eq!(r.worst[0].mm, r.max_deviation_mm);
    assert!(r.max_deviation_mm > 0.0 && r.max_deviation_mm <= 0.2 + 1e-6, "{}", r.max_deviation_mm);
    assert!(r.p95_deviation_mm <= r.max_deviation_mm);
    for d in &r.worst {
        assert!((0.0..=20.0).contains(&d.x_mm) && (0.0..=12.0).contains(&d.y_mm), "inside the 20 x 12 mm image: {d:?}");
    }
    assert_eq!((r.parts, r.holes), (3, 1), "disc, speck, ring and the counter");
    assert!(r.removed.is_empty(), "nothing is dropped at \"ignore less than 0\"");
}

#[test]
fn a_hole_dropped_by_the_filter_is_located_inside_the_ring_that_holds_it() {
    let (mut h, doc) = opened();
    edit(&mut h, doc, |f| f.ignore_below_mm2 = "0,5".to_owned());
    let r = report(&h, doc);
    assert_eq!(r.removed.len(), 1);
    let gone = r.removed[0];
    assert!(gone.hole, "a hole, not a part");
    assert!((gone.area_mm2 - 0.283).abs() < 0.1, "{}", gone.area_mm2);
    assert!((gone.x_mm - 15.0).abs() < 0.2 && (gone.y_mm - 3.5).abs() < 0.2, "{gone:?}: at the middle of the ring");
    assert_eq!(r.holes, 0);
}

#[test]
fn a_part_dropped_by_the_filter_is_located_too() {
    let (mut h, doc) = opened();
    edit(&mut h, doc, |f| f.ignore_below_mm2 = "1".to_owned());
    let r = report(&h, doc);
    assert_eq!(r.removed.len(), 2, "the speck and the counter");
    let speck = r.removed.iter().find(|f| !f.hole).expect("a part");
    assert!((speck.area_mm2 - 0.785).abs() < 0.15, "{}", speck.area_mm2);
    assert!((speck.x_mm - 15.0).abs() < 0.2 && (speck.y_mm - 8.0).abs() < 0.2, "{speck:?}");
    assert_eq!(r.parts, 2);
}

#[test]
fn there_is_no_report_without_a_contour_result() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    assert!(vm(&h, doc).trace_prep.is_none());
}
