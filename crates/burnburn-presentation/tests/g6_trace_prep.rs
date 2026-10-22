//! G6-werk, T03: preparing a photo or an uneven scan for tracing. A local threshold next to the
//! global one, a four-point plane correction that shows its transform, and masks that keep or remove
//! foreground. All of it is part of the recipe; the source pixels are never touched.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path as FsPath, PathBuf};

use burnburn_image::encode_png_gray8;
use burnburn_io::load_project;
use burnburn_presentation::{
    CommandKey, DocumentViewModel, HeadlessEditorHarness, MaskMode, PrepField, PrepIssue, ThresholdChoice,
    TraceFormInput, TracePrepIntent, TracePrepViewModel, TracerViewModel, UiIntent,
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

fn image(w: usize, h: usize, pixel: impl Fn(usize, usize) -> u8) -> Vec<u8> {
    let mut gray = vec![255u8; w * h];
    for y in 0..h {
        for x in 0..w {
            gray[y * w + x] = pixel(x, y);
        }
    }
    png_of(&gray, w as u32, h as u32)
}

/// Light that fades from 220 on the left to 100 on the right, with two objects darker than
/// their surroundings (by 100 levels): A at x 8…17 and B at x 40…49 (both 10 × 10 px).
fn uneven_light() -> Vec<u8> {
    image(64, 32, |x, y| {
        let bg = 220.0 - 120.0 * x as f64 / 63.0;
        let object = ((8..18).contains(&x) || (40..50).contains(&x)) && (11..21).contains(&y);
        (if object { bg - 100.0 } else { bg }).round().max(0.0) as u8
    })
}

/// A black trapezoid photographed on white: corners (10,8), (54,8), (60,40), (4,40).
fn trapezoid() -> Vec<u8> {
    image(64, 48, |x, y| {
        let (x, y) = (x as f64 + 0.5, y as f64 + 0.5);
        if !(8.0..=40.0).contains(&y) {
            return 255;
        }
        let t = (y - 8.0) / 32.0;
        let (left, right) = (10.0 - 6.0 * t, 54.0 + 6.0 * t);
        if (left..=right).contains(&x) { 0 } else { 255 }
    })
}

/// White with a dark object at x 8…17 and a dark shadow blob at x 40…55.
fn object_and_shadow() -> Vec<u8> {
    image(64, 32, |x, y| {
        let object = (8..18).contains(&x) && (10..22).contains(&y);
        let shadow = (40..56).contains(&x) && (4..28).contains(&y);
        if object || shadow { 0 } else { 255 }
    })
}

fn opened(bytes: &[u8]) -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    burnburn_io::fs::FileSystem::write_all(h.fs(), FsPath::new("/work/in.png"), bytes).unwrap();
    let doc = h.new_document().expect("window");
    h.send(UiIntent::OpenTraceSource { document: doc, path: PathBuf::from("/work/in.png") });
    h.run_tasks();
    (h, doc)
}

fn tracer(h: &HeadlessEditorHarness, doc: DocumentId) -> TracerViewModel {
    h.document(doc).unwrap().tracer.expect("tracer panel")
}

fn vm(h: &HeadlessEditorHarness, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).unwrap()
}

fn prep(h: &HeadlessEditorHarness, doc: DocumentId) -> TracePrepViewModel {
    vm(h, doc).trace_prep.expect("the preparation panel is offered while a tracer is open")
}

fn send(h: &mut HeadlessEditorHarness, doc: DocumentId, intent: TracePrepIntent) {
    h.send(UiIntent::TracePrep { document: doc, intent });
    h.run_tasks();
}

fn edit(h: &mut HeadlessEditorHarness, doc: DocumentId, change: impl FnOnce(&mut TraceFormInput)) {
    let mut input = tracer(h, doc).form.input;
    change(&mut input);
    h.send(UiIntent::SetTraceForm { document: doc, input: Box::new(input) });
    h.run_tasks();
}

fn shapes(h: &HeadlessEditorHarness, doc: DocumentId) -> (usize, f64, f64) {
    let r = tracer(h, doc).result.expect("a trace result");
    (r.components, r.width_mm, r.height_mm)
}

fn s(text: &str) -> String {
    text.to_owned()
}

fn corners(c: [[&str; 2]; 4]) -> [[String; 2]; 4] {
    c.map(|p| p.map(s))
}

// ---------------------------------------------------------------------------------------------
// Local threshold
// ---------------------------------------------------------------------------------------------

#[test]
fn the_global_and_the_local_threshold_are_chosen_separately_and_the_local_one_follows_uneven_light() {
    let (mut h, doc) = opened(&uneven_light());
    edit(&mut h, doc, |f| {
        f.threshold_choice = ThresholdChoice::Fixed;
        f.threshold = s("130");
    });
    let p = prep(&h, doc);
    assert!(!p.threshold.local, "the global threshold is in force");
    let (components, width, _) = shapes(&h, doc);
    // 130 takes the dark right side of the fading light for foreground and joins B to it.
    assert_eq!(components, 2, "A, and B joined to the shadowed strip on the right");
    assert!(width > 5.0, "the strip runs to the edge of the image: {width}");

    send(&mut h, doc, TracePrepIntent::UseLocalThreshold { radius: s("8"), offset: s("10") });
    let p = prep(&h, doc);
    assert_eq!((p.threshold.local, p.threshold.radius.as_str(), p.threshold.offset.as_str()), (true, "8", "10"));
    assert_eq!(tracer(&h, doc).form.input.threshold_choice, ThresholdChoice::Adaptive);
    let (components, width, height) = shapes(&h, doc);
    assert_eq!(components, 2, "both objects, whatever the light");
    assert!((width - 4.2).abs() < 0.3 && (height - 1.0).abs() < 0.3, "A…B spans 4.2 x 1.0 mm: {width} x {height}");

    send(&mut h, doc, TracePrepIntent::UseGlobalThreshold);
    assert!(!prep(&h, doc).threshold.local);
    assert_eq!(tracer(&h, doc).form.input.threshold_choice, ThresholdChoice::Otsu);
}

#[test]
fn a_bad_local_window_is_refused_where_it_was_typed_and_changes_nothing() {
    let (mut h, doc) = opened(&uneven_light());
    let before = shapes(&h, doc);
    for (radius, offset, field, out_of_range) in [
        ("abc", "10", PrepField::Radius, false),
        ("0", "10", PrepField::Radius, true),
        ("300", "10", PrepField::Radius, true),
        ("8", "x", PrepField::Offset, false),
        ("8", "256", PrepField::Offset, true),
        ("8", "-1", PrepField::Offset, true),
    ] {
        send(&mut h, doc, TracePrepIntent::UseLocalThreshold { radius: s(radius), offset: s(offset) });
        let want = if out_of_range { PrepIssue::OutOfRange(field) } else { PrepIssue::InvalidNumber(field) };
        let p = prep(&h, doc);
        assert_eq!(p.issue, Some(want), "{radius:?} {offset:?}");
        assert!(!p.threshold.local);
    }
    assert_eq!(shapes(&h, doc), before);
}

// ---------------------------------------------------------------------------------------------
// Four-point correction
// ---------------------------------------------------------------------------------------------

#[test]
fn four_corners_correct_the_plane_and_the_panel_shows_the_transform() {
    let (mut h, doc) = opened(&trapezoid());
    let (_, plain_width, _) = shapes(&h, doc);
    assert!((plain_width - 5.7).abs() < 0.3, "the skewed photo: {plain_width}");
    let want = corners([["10", "8"], ["54", "8"], ["60", "40"], ["4", "40"]]);
    send(&mut h, doc, TracePrepIntent::ApplyPerspective { corners: want.clone(), width: s("40"), height: s("32") });
    let p = prep(&h, doc);
    assert_eq!(p.issue, None);
    assert!(p.perspective.on);
    assert_eq!(p.perspective.corners, want);
    assert_eq!((p.perspective.width.as_str(), p.perspective.height.as_str()), ("40", "32"));
    let t = p.perspective.transform.expect("the resulting plane transform is shown");
    assert!(t.residual_px < 1e-6, "four corners fit exactly: {}", t.residual_px);
    // The transform sends each corner to a corner of the corrected rectangle.
    let apply = |x: f64, y: f64| {
        let m = t.matrix;
        let w = m[6] * x + m[7] * y + m[8];
        ((m[0] * x + m[1] * y + m[2]) / w, (m[3] * x + m[4] * y + m[5]) / w)
    };
    for ((x, y), (u, v)) in [(10.0, 8.0), (54.0, 8.0), (60.0, 40.0), (4.0, 40.0)].into_iter().zip([
        (0.0, 0.0),
        (40.0, 0.0),
        (40.0, 32.0),
        (0.0, 32.0),
    ]) {
        let (gu, gv) = apply(x, y);
        assert!((gu - u).abs() < 1e-6 && (gv - v).abs() < 1e-6, "({x},{y}) -> ({gu},{gv}) not ({u},{v})");
    }
    let (components, width, height) = shapes(&h, doc);
    assert_eq!(components, 1);
    assert!((width - 4.0).abs() < 0.3 && (height - 3.2).abs() < 0.3, "the corrected plate: {width} x {height}");

    send(&mut h, doc, TracePrepIntent::ClearPerspective);
    let p = prep(&h, doc);
    assert!(!p.perspective.on && p.perspective.transform.is_none());
    assert!((shapes(&h, doc).1 - plain_width).abs() < 1e-9, "the photo as it was");
}

#[test]
fn the_size_of_the_corrected_image_defaults_to_the_longer_opposite_edges() {
    let (mut h, doc) = opened(&trapezoid());
    send(
        &mut h,
        doc,
        TracePrepIntent::ApplyPerspective {
            corners: corners([["10", "8"], ["54", "8"], ["60", "40"], ["4", "40"]]),
            width: s(""),
            height: s(""),
        },
    );
    let p = prep(&h, doc);
    // Top edge 44, bottom edge 56 (longer); both side edges are hypot(6, 32) ≈ 32.56.
    assert_eq!((p.perspective.width.as_str(), p.perspective.height.as_str()), ("56", "33"));
}

#[test]
fn corners_that_span_no_plane_or_cannot_be_read_are_refused_and_leave_the_recipe_alone() {
    let (mut h, doc) = opened(&trapezoid());
    let before = shapes(&h, doc);
    let apply = |h: &mut HeadlessEditorHarness, c: [[&str; 2]; 4], w: &str, hh: &str| {
        send(h, doc, TracePrepIntent::ApplyPerspective { corners: corners(c), width: s(w), height: s(hh) });
        prep(h, doc)
    };
    // Three corners on one line.
    let p = apply(&mut h, [["0", "0"], ["20", "0"], ["40", "0"], ["0", "30"]], "40", "30");
    assert_eq!(p.issue, Some(PrepIssue::DegenerateCorners));
    assert!(!p.perspective.on);
    // A corner that is not a number is named.
    let p = apply(&mut h, [["0", "0"], ["20", "0"], ["x", "30"], ["0", "30"]], "40", "30");
    assert_eq!(p.issue, Some(PrepIssue::InvalidNumber(PrepField::CornerX(2))));
    // A size of zero.
    let p = apply(&mut h, [["0", "0"], ["20", "0"], ["20", "30"], ["0", "30"]], "0", "30");
    assert_eq!(p.issue, Some(PrepIssue::OutOfRange(PrepField::OutWidth)));
    assert!(!p.perspective.on);
    assert_eq!(shapes(&h, doc), before);
}

// ---------------------------------------------------------------------------------------------
// Masks
// ---------------------------------------------------------------------------------------------

fn add_mask(h: &mut HeadlessEditorHarness, doc: DocumentId, mode: MaskMode, rect: [&str; 4]) {
    send(
        h,
        doc,
        TracePrepIntent::AddMask { mode, x: s(rect[0]), y: s(rect[1]), width: s(rect[2]), height: s(rect[3]) },
    );
}

#[test]
fn a_remove_mask_drops_the_shadow_and_a_keep_mask_traces_only_inside_it() {
    let (mut h, doc) = opened(&object_and_shadow());
    assert_eq!(shapes(&h, doc).0, 2, "the object and its shadow");

    add_mask(&mut h, doc, MaskMode::Remove, ["36", "0", "24", "32"]);
    let p = prep(&h, doc);
    assert_eq!(p.issue, None);
    assert_eq!(p.masks.len(), 1);
    assert_eq!((p.masks[0].mode, p.masks[0].x, p.masks[0].width), (MaskMode::Remove, 36, 24));
    assert_eq!(shapes(&h, doc).0, 1, "the shadow is background now");

    send(&mut h, doc, TracePrepIntent::RemoveMask { index: 0 });
    assert!(prep(&h, doc).masks.is_empty());
    assert_eq!(shapes(&h, doc).0, 2, "and back");

    add_mask(&mut h, doc, MaskMode::Keep, ["0", "0", "30", "32"]);
    let (components, width, _) = shapes(&h, doc);
    assert_eq!(components, 1);
    assert!((width - 1.0).abs() < 0.3, "only the object at x 8…17: {width}");
}

#[test]
fn a_mask_outside_the_image_or_unreadable_is_refused_and_so_is_an_unknown_mask_number() {
    let (mut h, doc) = opened(&object_and_shadow());
    add_mask(&mut h, doc, MaskMode::Remove, ["60", "0", "10", "10"]);
    assert_eq!(prep(&h, doc).issue, Some(PrepIssue::MaskOutsideImage));
    add_mask(&mut h, doc, MaskMode::Remove, ["0", "0", "0", "10"]);
    assert_eq!(prep(&h, doc).issue, Some(PrepIssue::OutOfRange(PrepField::MaskWidth)));
    add_mask(&mut h, doc, MaskMode::Remove, ["-1", "0", "10", "10"]);
    assert_eq!(prep(&h, doc).issue, Some(PrepIssue::OutOfRange(PrepField::MaskX)));
    add_mask(&mut h, doc, MaskMode::Remove, ["a", "0", "10", "10"]);
    assert_eq!(prep(&h, doc).issue, Some(PrepIssue::InvalidNumber(PrepField::MaskX)));
    send(&mut h, doc, TracePrepIntent::RemoveMask { index: 3 });
    assert_eq!(prep(&h, doc).issue, Some(PrepIssue::UnknownMask));
    assert!(prep(&h, doc).masks.is_empty());
    assert_eq!(shapes(&h, doc).0, 2);
}

#[test]
fn what_is_prepared_travels_with_the_applied_trace_and_the_source_bytes_stay_as_they_were() {
    let original = object_and_shadow();
    let (mut h, doc) = opened(&original);
    add_mask(&mut h, doc, MaskMode::Remove, ["36", "0", "24", "32"]);
    send(&mut h, doc, TracePrepIntent::UseLocalThreshold { radius: s("5"), offset: s("20") });
    h.invoke(doc, CommandKey::ApplyTraceLinked);
    assert_eq!(vm(&h, doc).objects.len(), 1, "the shadow is not in the trace");

    h.fs().create_dir_all_for_test(FsPath::new("/out"));
    let path = PathBuf::from("/out/p.burnburn");
    h.send(UiIntent::Save { document: doc, path: Some(path.clone()) });
    h.run_tasks();
    let project = load_project(h.fs(), &path).expect("saved project");
    let (_, definition) = project.document.engine_layer().traces().next().expect("a trace");
    let regions = definition.recipe.get("image.regions").expect("masks are recipe parameters");
    assert!(matches!(regions, burnburn_document::engine::ParamValue::List(items) if items.len() == 1));
    assert_eq!(
        definition.recipe.get("image.threshold"),
        Some(&burnburn_document::engine::ParamValue::Text("adaptive".into()))
    );
    let asset = project.assets.get(&definition.source).expect("the source asset");
    assert_eq!(&*asset.bytes, original.as_slice(), "the stored source is the file as it was");
}
