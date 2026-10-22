//! G6-werk, T05: one wrong area is traced again with its own threshold and optional anchors, while
//! everything outside it stays exactly as it was. The correction is part of the recipe: the applied
//! trace keeps it, reopening applies it again, and when the source link of a correction is lost
//! after another trace it is reported and not applied to something else.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path as FsPath, PathBuf};

use burnburn_image::encode_png_gray8;
use burnburn_presentation::{
    CommandKey, CorrectionStatus, DocumentViewModel, HeadlessEditorHarness, PrepField, PrepIssue, SelectMode,
    ThresholdChoice, TraceFormInput, TracePrepIntent, TracePrepViewModel, TracerViewModel, UiIntent,
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

/// 200 x 120 px: a ring (50, 60) r 35 with a light-gray counter (150, r 15); a black rectangle
/// x 110…180, y 20…100 with a faint bump (180) on its left edge; a faint speck (170) at (88, 100) and a
/// small black disc far away at (20, 15). The tracer starts at a global threshold of 127.
fn scene() -> Vec<u8> {
    let (w, h) = (200usize, 120usize);
    let mut gray = vec![255u8; w * h];
    for y in 0..h {
        for x in 0..w {
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
            gray[y * w + x] = v;
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
    });
    (h, doc)
}

fn tracer(h: &HeadlessEditorHarness, doc: DocumentId) -> TracerViewModel {
    h.document(doc).unwrap().tracer.expect("tracer panel")
}

fn vm(h: &HeadlessEditorHarness, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).unwrap()
}

fn prep(h: &HeadlessEditorHarness, doc: DocumentId) -> TracePrepViewModel {
    vm(h, doc).trace_prep.expect("preparation panel")
}

fn edit(h: &mut HeadlessEditorHarness, doc: DocumentId, change: impl FnOnce(&mut TraceFormInput)) {
    let mut input = tracer(h, doc).form.input;
    change(&mut input);
    h.send(UiIntent::SetTraceForm { document: doc, input: Box::new(input) });
    h.run_tasks();
}

fn send(h: &mut HeadlessEditorHarness, doc: DocumentId, intent: TracePrepIntent) {
    h.send(UiIntent::TracePrep { document: doc, intent });
    h.run_tasks();
}

fn retrace(h: &mut HeadlessEditorHarness, doc: DocumentId, rect: [&str; 4], threshold: &str, anchors: &str) {
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

fn counts(h: &HeadlessEditorHarness, doc: DocumentId) -> (usize, usize) {
    let r = tracer(h, doc).result.expect("a trace result");
    (r.components, r.holes)
}

fn subpaths(h: &HeadlessEditorHarness, doc: DocumentId) -> Vec<burnburn_geometry::Subpath> {
    tracer(h, doc).result.expect("a trace result").preview.subpaths().to_vec()
}

const REGION: [&str; 4] = ["30", "40", "90", "70"];

#[test]
fn a_region_traced_again_with_its_own_threshold_leaves_everything_outside_it_identical() {
    let (mut h, doc) = opened();
    assert_eq!(counts(&h, doc), (3, 1), "ring with its counter, rectangle, far disc");
    let before = subpaths(&h, doc);
    let bounds = |s: &burnburn_geometry::Subpath| burnburn_geometry::Path::from_subpaths(vec![s.clone()]).bounds();
    let far_before: Vec<_> = before.iter().filter(|s| bounds(s).is_some_and(|b| b.max().x < 3.0)).cloned().collect();
    assert_eq!(far_before.len(), 1, "the disc far from the region");

    retrace(&mut h, doc, REGION, "200", "");
    assert_eq!(prep(&h, doc).issue, None);
    // 200 fills the counter (150) and picks up the faint speck (170) inside the region.
    assert_eq!(counts(&h, doc), (4, 0));
    let after = subpaths(&h, doc);
    assert!(after.contains(&far_before[0]), "the contour outside the region is bit-identical");

    let rows = prep(&h, doc).corrections;
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].x, rows[0].y, rows[0].width, rows[0].height), (30, 40, 90, 70));
    assert_eq!(rows[0].threshold, "200");
    assert_eq!(rows[0].status, CorrectionStatus::Applied);
}

#[test]
fn a_correction_is_stored_with_the_trace_and_applied_again_when_it_is_reopened() {
    let (mut h, doc) = opened();
    retrace(&mut h, doc, REGION, "200", "60, 70");
    let corrected = counts(&h, doc);
    assert_eq!(prep(&h, doc).corrections[0].anchors, 1);

    h.invoke(doc, CommandKey::ApplyTraceLinked);
    let ids: Vec<_> = vm(&h, doc).objects.iter().map(|o| o.id).collect();
    assert_eq!(ids.len(), 4, "the corrected shapes are in the document");
    h.invoke(doc, CommandKey::CloseTracer);
    h.send(UiIntent::Select { document: doc, ids: vec![ids[0]], mode: SelectMode::Replace });
    h.invoke(doc, CommandKey::TraceEdit);
    h.run_tasks();
    assert_eq!(counts(&h, doc), corrected, "traced again from the stored recipe, correction included");
    let rows = prep(&h, doc).corrections;
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].threshold.as_str(), rows[0].anchors, rows[0].status), ("200", 1, CorrectionStatus::Applied));
}

#[test]
fn another_base_trace_that_loses_the_source_link_reports_the_correction_and_does_not_apply_it() {
    let (mut h, doc) = opened();
    retrace(&mut h, doc, REGION, "200", "");
    // A global threshold of 175 fills the counter by itself: the hole the correction was bound to is gone.
    edit(&mut h, doc, |f| f.threshold = "175".to_owned());
    let rows = prep(&h, doc).corrections;
    assert_eq!(rows.len(), 1, "the correction is still listed");
    match rows[0].status {
        CorrectionStatus::NeedsRelink { lost, .. } => assert!(lost >= 1, "a contour it was bound to is gone"),
        other => panic!("not applied, said so: {other:?}"),
    }
    let not_applied = subpaths(&h, doc);
    // Back to where the correction fits: it applies again.
    edit(&mut h, doc, |f| f.threshold = "127".to_owned());
    assert_eq!(prep(&h, doc).corrections[0].status, CorrectionStatus::Applied);
    assert_eq!(counts(&h, doc), (4, 0));

    // At 175 again, removing the correction changes nothing in what is shown: it was not applied.
    edit(&mut h, doc, |f| f.threshold = "175".to_owned());
    send(&mut h, doc, TracePrepIntent::RemoveCorrection { index: 0 });
    assert!(prep(&h, doc).corrections.is_empty());
    assert_eq!(subpaths(&h, doc), not_applied);
    edit(&mut h, doc, |f| f.threshold = "127".to_owned());
    assert_eq!(counts(&h, doc), (3, 1), "the plain trace");
}

#[test]
fn a_correction_that_cannot_be_read_or_placed_is_refused_and_changes_nothing() {
    let (mut h, doc) = opened();
    let before = counts(&h, doc);
    let cases: [([&str; 4], &str, &str, PrepIssue); 6] = [
        (["30", "40", "900", "70"], "200", "", PrepIssue::MaskOutsideImage),
        (["30", "40", "0", "70"], "200", "", PrepIssue::OutOfRange(PrepField::MaskWidth)),
        (REGION, "256", "", PrepIssue::OutOfRange(PrepField::RegionThreshold)),
        (REGION, "abc", "", PrepIssue::InvalidNumber(PrepField::RegionThreshold)),
        (REGION, "200", "60", PrepIssue::InvalidNumber(PrepField::Anchors)),
        (REGION, "200", "600, 70", PrepIssue::MaskOutsideImage),
    ];
    for (rect, threshold, anchors, want) in cases {
        retrace(&mut h, doc, rect, threshold, anchors);
        assert_eq!(prep(&h, doc).issue, Some(want), "{rect:?} {threshold:?} {anchors:?}");
    }
    assert!(prep(&h, doc).corrections.is_empty());
    assert_eq!(counts(&h, doc), before);
    send(&mut h, doc, TracePrepIntent::RemoveCorrection { index: 2 });
    assert_eq!(prep(&h, doc).issue, Some(PrepIssue::UnknownMask));
}

#[test]
fn in_centre_line_mode_there_is_no_contour_trace_to_correct() {
    let (mut h, doc) = opened();
    edit(&mut h, doc, |f| f.mode = burnburn_presentation::TraceMode::Centerline);
    retrace(&mut h, doc, REGION, "200", "");
    assert_eq!(prep(&h, doc).issue, Some(PrepIssue::NothingToCorrect));
    assert!(prep(&h, doc).corrections.is_empty());
}
