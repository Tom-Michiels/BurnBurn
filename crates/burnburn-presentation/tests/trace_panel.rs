//! W2-L1-beeld, headless: the tracer panel (T01 scale, T02 silhouettes with holes, T04 centre
//! lines, T10 recipes). Input is raw field text, every change is one trace task, an older
//! result is never shown.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path as FsPath, PathBuf};

use burnburn_document::{EditLabel, UndoLabel};
use burnburn_geometry::Segment;
use burnburn_image::encode_png_gray8;
use burnburn_presentation::{
    CommandKey, HeadlessEditorHarness, ImageField, ImageIssue, ThresholdChoice, TraceFormInput, TraceMode, TracePreset,
    TracerViewModel, UiIntent,
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

fn png(width: u32, height: u32, gray: &[u8], pixels_per_metre: Option<u32>) -> Vec<u8> {
    let plain = encode_png_gray8(width, height, gray).unwrap();
    let Some(ppm) = pixels_per_metre else { return plain };
    let mut data = ppm.to_be_bytes().to_vec();
    data.extend_from_slice(&ppm.to_be_bytes());
    data.push(1);
    let mut body = b"pHYs".to_vec();
    body.extend_from_slice(&data);
    let mut chunk = (data.len() as u32).to_be_bytes().to_vec();
    chunk.extend_from_slice(&body);
    chunk.extend_from_slice(&crc32(&body).to_be_bytes());
    let at = 8 + 25;
    let mut out = plain[..at].to_vec();
    out.extend_from_slice(&chunk);
    out.extend_from_slice(&plain[at..]);
    out
}

/// 254 dpi = 10 000 pixels per metre = 0.1 mm per pixel.
const DPI_254: Option<u32> = Some(10_000);

/// A black disc (radius 20 px) and a 2 × 2 px speck on white, 64 × 64.
fn disc_and_speck() -> Vec<u8> {
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
    gray
}

/// A black bar of 40 × 6 px on white, 64 × 16.
fn bar() -> Vec<u8> {
    let mut gray = vec![255u8; 64 * 16];
    for y in 5..11 {
        for x in 10..50 {
            gray[y * 64 + x] = 0;
        }
    }
    gray
}

fn open(bytes: &[u8]) -> (HeadlessEditorHarness, DocumentId) {
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

fn form(h: &HeadlessEditorHarness, doc: DocumentId) -> TraceFormInput {
    tracer(h, doc).form.input
}

fn edit(h: &mut HeadlessEditorHarness, doc: DocumentId, change: impl FnOnce(&mut TraceFormInput)) {
    let mut input = form(h, doc);
    change(&mut input);
    h.send(UiIntent::SetTraceForm { document: doc, input: Box::new(input) });
}

#[test]
fn the_file_resolution_is_proposed_and_starts_the_trace() {
    let (h, doc) = open(&png(64, 64, &disc_and_speck(), DPI_254));
    let t = tracer(&h, doc);
    assert_eq!(t.form.dpi_suggestion.map(|d| d.round()), Some(254.0));
    assert_eq!(t.form.input.dpi, "254");
    assert!(!t.needs_scale);
    let result = t.result.expect("traced at the proposed scale");
    assert_eq!(result.components, 2, "disc and speck");
    assert!((result.mm_per_pixel.0 - 0.1).abs() < 1e-3, "{:?}", result.mm_per_pixel);
}

#[test]
fn without_a_resolution_in_the_file_nothing_is_traced_until_the_user_gives_one() {
    let (mut h, doc) = open(&png(64, 64, &disc_and_speck(), None));
    let t = tracer(&h, doc);
    assert!(t.form.dpi_suggestion.is_none() && t.form.input.dpi.is_empty() && t.needs_scale && t.result.is_none());
    assert!(h.take_task_requests().is_empty());
    edit(&mut h, doc, |f| f.dpi = "254".into());
    assert!(tracer(&h, doc).computing);
    h.run_tasks();
    let t = tracer(&h, doc);
    assert!(!t.needs_scale && t.result.is_some());
}

/// "Ignore less than": the speck (0.04 mm²) goes at 0.1 mm², the disc (≈ 12.6 mm²) stays.
#[test]
fn ignore_less_than_drops_specks_by_area() {
    let (mut h, doc) = open(&png(64, 64, &disc_and_speck(), DPI_254));
    assert_eq!(tracer(&h, doc).result.unwrap().components, 2);
    edit(&mut h, doc, |f| f.ignore_below_mm2 = "0,1".into());
    h.run_tasks();
    let r = tracer(&h, doc).result.unwrap();
    assert_eq!(r.components, 1, "only the disc is left");
    assert!((r.width_mm - 4.0).abs() < 0.2, "disc of radius 20 px at 0.1 mm: {}", r.width_mm);
}

#[test]
fn optimise_off_gives_straight_segments_only_and_smoothness_is_the_fit_budget() {
    let (mut h, doc) = open(&png(64, 64, &disc_and_speck(), DPI_254));
    let curved = |h: &HeadlessEditorHarness| {
        let r = tracer(h, doc).result.unwrap();
        r.preview.subpaths().iter().flat_map(|s| s.segments().iter()).any(|s| !matches!(s, Segment::Line { .. }))
    };
    assert!(curved(&h), "curves by default");
    edit(&mut h, doc, |f| f.curves = false);
    h.run_tasks();
    assert!(!curved(&h), "polygon simplification only");
    edit(&mut h, doc, |f| f.smoothness_mm = "0,2".into());
    h.run_tasks();
    let t = tracer(&h, doc);
    assert!((t.result.unwrap().fit_tolerance_mm - 0.2).abs() < 1e-9);
}

/// Inverted: the white background is the shape, the disc and the speck are its holes (T02).
#[test]
fn invert_makes_the_background_the_shape_with_holes() {
    let (mut h, doc) = open(&png(64, 64, &disc_and_speck(), DPI_254));
    edit(&mut h, doc, |f| f.invert = true);
    h.run_tasks();
    let r = tracer(&h, doc).result.unwrap();
    assert_eq!((r.components, r.holes), (1, 2));
}

#[test]
fn a_fixed_threshold_uses_the_typed_gray_level() {
    // Mid-gray disc (150) on white: Otsu finds it, a fixed level below 150 does not.
    let gray: Vec<u8> = disc_and_speck().iter().map(|&v| if v == 0 { 150 } else { v }).collect();
    let (mut h, doc) = open(&png(64, 64, &gray, DPI_254));
    assert!(tracer(&h, doc).result.unwrap().components >= 1);
    edit(&mut h, doc, |f| {
        f.threshold_choice = ThresholdChoice::Fixed;
        f.threshold = "100".into();
    });
    h.run_tasks();
    let t = tracer(&h, doc);
    assert_eq!(t.result.map_or(0, |r| r.components), 0, "nothing is darker than 100");
    edit(&mut h, doc, |f| f.threshold = "200".into());
    h.run_tasks();
    assert!(tracer(&h, doc).result.unwrap().components >= 1);
}

#[test]
fn invalid_input_names_the_field_and_starts_no_task() {
    let (mut h, doc) = open(&png(64, 64, &disc_and_speck(), DPI_254));
    let before = tracer(&h, doc).result.unwrap();
    let cases: [(fn(&mut TraceFormInput), ImageField, ImageIssue); 5] = [
        (|f| f.dpi = "abc".into(), ImageField::Dpi, ImageIssue::InvalidNumber(ImageField::Dpi)),
        (
            |f| {
                f.threshold_choice = ThresholdChoice::Fixed;
                f.threshold = "300".into();
            },
            ImageField::TraceThreshold,
            ImageIssue::OutOfRange(ImageField::TraceThreshold),
        ),
        (
            |f| f.ignore_below_mm2 = "-1".into(),
            ImageField::TraceIgnoreBelow,
            ImageIssue::OutOfRange(ImageField::TraceIgnoreBelow),
        ),
        (
            |f| f.smoothness_mm = "0".into(),
            ImageField::TraceSmoothness,
            ImageIssue::OutOfRange(ImageField::TraceSmoothness),
        ),
        (
            |f| f.smoothness_mm = "x".into(),
            ImageField::TraceSmoothness,
            ImageIssue::InvalidNumber(ImageField::TraceSmoothness),
        ),
    ];
    for (change, field, issue) in cases {
        let original = form(&h, doc);
        edit(&mut h, doc, change);
        let t = tracer(&h, doc);
        assert_eq!(t.form.issue, Some((field, issue)), "{field:?}");
        assert!(h.take_task_requests().is_empty() && !t.computing, "{field:?}");
        assert_eq!(t.result.as_ref(), Some(&before), "the valid result stays");
        h.send(UiIntent::SetTraceForm { document: doc, input: Box::new(original) });
        h.run_tasks();
        assert!(tracer(&h, doc).form.issue.is_none());
    }
}

#[test]
fn a_result_for_an_older_form_is_cancelled_and_never_shown() {
    let (mut h, doc) = open(&png(64, 64, &disc_and_speck(), DPI_254));
    let width_at_254 = tracer(&h, doc).result.unwrap().width_mm;
    edit(&mut h, doc, |f| f.dpi = "127".into());
    let older = h.take_task_requests();
    edit(&mut h, doc, |f| f.dpi = "508".into());
    let newer = h.take_task_requests();
    assert!(older.iter().all(|r| r.cancel.is_cancelled()));
    for r in older {
        h.complete_despite_cancel(r);
    }
    assert!((tracer(&h, doc).result.unwrap().width_mm - width_at_254).abs() < 1e-9, "the 254 dpi result stays");
    for r in newer {
        h.complete(r);
    }
    let width = tracer(&h, doc).result.unwrap().width_mm;
    assert!((width - width_at_254 / 2.0).abs() < 0.05, "508 dpi halves the size: {width} vs {width_at_254}");
}

#[test]
fn applying_the_trace_is_one_undo_entry_and_closing_leaves_the_document_alone() {
    let (mut h, doc) = open(&png(64, 64, &disc_and_speck(), DPI_254));
    let revision = h.document(doc).unwrap().revision;
    h.invoke(doc, CommandKey::ApplyTrace);
    let vm = h.document(doc).unwrap();
    assert_eq!(vm.undo, Some(UndoLabel::Edit(EditLabel::Import)));
    assert_eq!(vm.objects.len(), 2);
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(h.document(doc).unwrap().objects.len(), 0);
    h.invoke(doc, CommandKey::CloseTracer);
    assert!(h.document(doc).unwrap().tracer.is_none());
    assert!(h.document(doc).unwrap().revision > revision, "only the apply and its undo changed it");
}

mod recipes {
    use super::*;

    #[test]
    fn a_preset_replaces_the_recipe_but_keeps_the_resolution() {
        let (mut h, doc) = open(&png(64, 64, &disc_and_speck(), DPI_254));
        h.send(UiIntent::ApplyTracePreset { document: doc, preset: TracePreset::Technical });
        assert!(tracer(&h, doc).computing, "a preset starts a trace");
        h.run_tasks();
        let t = tracer(&h, doc);
        assert_eq!(t.form.preset, Some(TracePreset::Technical));
        assert!(!t.form.input.curves);
        assert_eq!(t.form.input.dpi, "254", "the declared resolution is not a recipe value");
        assert!(t.result.is_some());
        edit(&mut h, doc, |f| f.smoothness_mm = "0,3".into());
        assert_eq!(tracer(&h, doc).form.preset, None, "no longer the recipe it was");
    }

    #[test]
    fn the_photo_preset_uses_the_local_threshold() {
        let (mut h, doc) = open(&png(64, 64, &disc_and_speck(), DPI_254));
        h.send(UiIntent::ApplyTracePreset { document: doc, preset: TracePreset::Photo });
        h.run_tasks();
        let t = tracer(&h, doc);
        assert_eq!(t.form.input.threshold_choice, ThresholdChoice::Adaptive);
        assert_eq!(t.form.preset, Some(TracePreset::Photo));
    }

    #[test]
    fn the_handwriting_preset_switches_to_centre_lines() {
        let (mut h, doc) = open(&png(64, 16, &bar(), DPI_254));
        h.send(UiIntent::ApplyTracePreset { document: doc, preset: TracePreset::Handwriting });
        h.run_tasks();
        let t = tracer(&h, doc);
        assert_eq!((t.form.input.mode, t.form.preset), (TraceMode::Centerline, Some(TracePreset::Handwriting)));
        assert!(t.result.is_some());
    }
}

mod centerline {
    use super::*;

    fn centre_lines() -> (HeadlessEditorHarness, DocumentId) {
        let (mut h, doc) = open(&png(64, 16, &bar(), DPI_254));
        edit(&mut h, doc, |f| f.mode = TraceMode::Centerline);
        h.run_tasks();
        (h, doc)
    }

    /// T04: a 6 px thick bar has one open centre line along its length, no holes.
    #[test]
    fn a_stroke_gives_one_open_centre_line() {
        let (h, doc) = centre_lines();
        let r = tracer(&h, doc).result.expect("centre-line result");
        assert_eq!((r.components, r.holes), (1, 0));
        assert!(r.height_mm < 0.2, "a line has no thickness: {} mm", r.height_mm);
        assert!(r.width_mm > 3.0 && r.width_mm <= 4.0, "{} mm long", r.width_mm);
    }

    #[test]
    fn applying_a_centre_line_adds_an_open_path_as_one_undo_entry() {
        let (mut h, doc) = centre_lines();
        h.invoke(doc, CommandKey::ApplyTrace);
        let vm = h.document(doc).unwrap();
        assert_eq!((vm.objects.len(), vm.undo), (1, Some(UndoLabel::Edit(EditLabel::Import))));
        let scene = h.scene(doc).unwrap();
        let path = &scene.items[0].path;
        assert!(path.subpaths().iter().all(|s| !s.is_closed()), "open path stays open");
        h.invoke(doc, CommandKey::Undo);
        assert!(h.document(doc).unwrap().objects.is_empty());
    }

    #[test]
    fn a_centre_line_for_an_older_form_is_dropped_and_switching_back_shows_the_outline() {
        let (mut h, doc) = open(&png(64, 16, &bar(), DPI_254));
        edit(&mut h, doc, |f| f.mode = TraceMode::Centerline);
        let older = h.take_task_requests();
        edit(&mut h, doc, |f| f.smoothness_mm = "0,3".into());
        let newer = h.take_task_requests();
        for r in older {
            h.complete_despite_cancel(r);
        }
        assert!(tracer(&h, doc).result.is_none() || tracer(&h, doc).form.input.mode == TraceMode::Contour);
        for r in newer {
            h.complete(r);
        }
        let t = tracer(&h, doc);
        assert!((t.result.as_ref().unwrap().fit_tolerance_mm - 0.3).abs() < 1e-9, "the newest form's result");
        edit(&mut h, doc, |f| f.mode = TraceMode::Contour);
        h.run_tasks();
        let r = tracer(&h, doc).result.unwrap();
        assert!(r.height_mm > 0.5, "the outline has the bar's thickness again: {}", r.height_mm);
    }
}
