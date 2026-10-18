//! W1-L5a image modes in the compiler: option semantics (negative, dot width correction,
//! pass-through, scan angle, crop, mask, sharpen, colour conversion), the new dither modes and
//! the processed bitmap. Expected pixel patterns come from the independent reference in
//! `docs/dev-notes/IM18-raster.md` or from hand calculation in the comments.

#![allow(clippy::unwrap_used)]

mod common;

use burnburn_cam::{CompileError, compile_with, processed_bitmap, required_features};
use burnburn_document::process::{
    DitherMode, GrayMethod, ImageCrop, ImageRender, ImageSettings, ImageSharpen, OperationDraft, OperationMode,
    ScreenSettings,
};
use burnburn_document::{DocumentSession, DocumentSnapshot, GeometryObject, GeometrySource, ObjectKind};
use burnburn_geometry::{Path, PathBuilder, Point};
use burnburn_image::{PixelFormat, RasterError, SourceImage};
use burnburn_job::{Feature, PlanEvent, PlanMode, ScanAxis};
use burnburn_types::{CancellationToken, DocumentId, GeometryId, OperationId, StableId};
use common::*;

fn never() -> CancellationToken {
    CancellationToken::never()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn base(source: &SourceImage, mode: DitherMode) -> ImageSettings {
    ImageSettings::new(source.content_hash(), mm(0.1), mm(0.1), ImageRender::Dither(mode)).unwrap()
}

fn draft(settings: ImageSettings) -> OperationDraft {
    OperationDraft {
        name: "Graveren".into(),
        mode: OperationMode::Image(settings),
        settings: settings_for_image(),
        output_enabled: true,
    }
}

fn settings_for_image() -> burnburn_document::process::ProcessSettings {
    settings(100.0, 60.0)
}

/// A frame of `w × h` mm at (10, 10) with one image operation.
fn scene(settings: ImageSettings, w: f64, h: f64) -> (DocumentSession, OperationId) {
    let mut s = session();
    let frame = rectangle(&mut s, 10.0, 10.0, w, h);
    let op = add_op(&mut s, draft(settings), vec![frame]);
    (s, op)
}

fn plan(s: &DocumentSession, source: &SourceImage) -> Result<burnburn_job::JobPlan, CompileError> {
    compile_with(&s.snapshot(), &profile(), &options(), &vec![source.clone()], &never())
}

/// A profile that accepts angled scans (they are unverified on the default profile).
fn angled_profile() -> burnburn_job::MachineProfile {
    use burnburn_job::{MachineCapabilities, Support};
    let caps = MachineCapabilities::conservative().with(Feature::ScanAngle, Support::Supported);
    profile().with_capabilities(caps).unwrap()
}

fn plan_angled(s: &DocumentSession, source: &SourceImage) -> Result<burnburn_job::JobPlan, CompileError> {
    compile_with(&s.snapshot(), &angled_profile(), &options(), &vec![source.clone()], &never())
}

/// `(min_x, max_x, y)` of every scan run.
fn runs(events: &[PlanEvent]) -> Vec<(f64, f64, f64)> {
    events
        .iter()
        .filter_map(|e| match *e {
            PlanEvent::ScanRun { line, .. } => {
                let (a, b) = (line.from.x_mm(), line.to.x_mm());
                Some((a.min(b), a.max(b), line.from.y_mm()))
            }
            _ => None,
        })
        .collect()
}

/// Rebuilds the burn pattern of a `cols × rows` raster of 0.1 mm dots whose lower-left corner
/// is (10, 10) from the planned runs: `#` = a run covers the cell.
fn pattern(events: &[PlanEvent], cols: usize, rows: usize) -> Vec<String> {
    let mut grid = vec![vec!['.'; cols]; rows];
    for (x0, x1, y) in runs(events) {
        let row = rows - 1 - ((y - 10.0) / 0.1 - 0.5).round() as usize;
        let (c0, c1) = (((x0 - 10.0) / 0.1).round() as usize, ((x1 - 10.0) / 0.1).round() as usize);
        for cell in &mut grid[row][c0..c1] {
            *cell = '#';
        }
    }
    grid.into_iter().map(|r| r.into_iter().collect()).collect()
}

fn gradient_8x4() -> SourceImage {
    let px: Vec<u8> = (0..4).flat_map(|y| (0..8).map(move |x| 30 + 27 * x + 10 * y)).collect();
    SourceImage::from_gray8(8, 4, px).unwrap()
}

// ---- negative, dot width correction, pass-through ----------------------------------------------

#[test]
fn negative_image_burns_the_light_pixels() {
    let source = SourceImage::from_gray8(2, 1, vec![0, 255]).unwrap();
    let threshold = base(&source, DitherMode::Threshold { level: 128 });
    let (s, _) = scene(threshold, 0.2, 0.1);
    let normal = runs(plan(&s, &source).unwrap().events());
    assert!(near(normal[0].0, 10.0) && near(normal[0].1, 10.1) && normal.len() == 1, "{normal:?}");
    let (s, _) = scene(threshold.with_negative(true), 0.2, 0.1);
    let negative = runs(plan(&s, &source).unwrap().events());
    assert!(near(negative[0].0, 10.1) && near(negative[0].1, 10.2) && negative.len() == 1, "{negative:?}");
}

#[test]
fn dot_width_correction_shortens_every_run_at_both_ends() {
    // Pixels black, white, black, black → runs [10.0, 10.1] and [10.2, 10.4].
    let source = SourceImage::from_gray8(4, 1, vec![0, 255, 0, 0]).unwrap();
    let t = base(&source, DitherMode::Threshold { level: 128 });
    let at = |dwc: f64| {
        let t = t.with_dot_width_correction(mm(dwc)).unwrap();
        let (s, _) = scene(t, 0.4, 0.1);
        let mut r = runs(plan(&s, &source).unwrap().events());
        r.sort_by(|a, b| a.0.total_cmp(&b.0));
        r
    };
    let plain = at(0.0);
    assert_eq!(plain.len(), 2);
    // 0.04 mm in total: 0.02 off each end.
    let trimmed = at(0.04);
    assert!(near(trimmed[0].0, 10.02) && near(trimmed[0].1, 10.08), "{trimmed:?}");
    assert!(near(trimmed[1].0, 10.22) && near(trimmed[1].1, 10.38), "{trimmed:?}");
    // 0.1 mm eats the single dot completely; the double dot keeps 0.1 mm.
    let eaten = at(0.1);
    assert_eq!(eaten.len(), 1, "{eaten:?}");
    assert!(near(eaten[0].0, 10.25) && near(eaten[0].1, 10.35), "{eaten:?}");
}

#[test]
fn pass_through_replicates_source_pixels_without_filtering() {
    // 3 source pixels over 0.6 mm at 0.1 mm pitch: each source pixel becomes two dots.
    let source = SourceImage::from_gray8(3, 1, vec![0, 255, 0]).unwrap();
    let (s, _) = scene(base(&source, DitherMode::PassThrough), 0.6, 0.1);
    let mut r = runs(plan(&s, &source).unwrap().events());
    r.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert_eq!(r.len(), 2, "{r:?}");
    assert!(near(r[0].0, 10.0) && near(r[0].1, 10.2), "{r:?}");
    assert!(near(r[1].0, 10.4) && near(r[1].1, 10.6), "{r:?}");
}

#[test]
fn pass_through_skips_pixels_when_shrinking_instead_of_averaging() {
    // 255, 0, 255, 0 onto two dots: averaging would give 128/128 (white at threshold 128);
    // pass-through takes source pixels 1 and 3, both black.
    let source = SourceImage::from_gray8(4, 1, vec![255, 0, 255, 0]).unwrap();
    let (s, _) = scene(base(&source, DitherMode::PassThrough), 0.2, 0.1);
    let r = runs(plan(&s, &source).unwrap().events());
    assert!(r.len() == 1 && near(r[0].0, 10.0) && near(r[0].1, 10.2), "{r:?}");
    let (s, _) = scene(base(&source, DitherMode::Threshold { level: 128 }), 0.2, 0.1);
    assert!(runs(plan(&s, &source).unwrap().events()).is_empty(), "the area average is white");
}

#[test]
fn pass_through_with_a_scan_angle_is_refused() {
    let source = SourceImage::from_gray8(2, 1, vec![0, 255]).unwrap();
    let s0 = base(&source, DitherMode::PassThrough).with_scan_angle(30.0).unwrap();
    let (s, op) = scene(s0, 0.2, 0.1);
    assert_eq!(plan_angled(&s, &source).unwrap_err(), CompileError::UnsupportedMode { operation: op });
}

// ---- scan angle ---------------------------------------------------------------------------------

#[test]
fn scan_angle_90_scans_columns_of_the_same_image() {
    // A black 1×1 source over 0.2 × 0.3 mm: columns at x = 10.05 and 10.15, 0.3 mm long.
    let source = SourceImage::from_gray8(1, 1, vec![0]).unwrap();
    let s0 = base(&source, DitherMode::Threshold { level: 128 }).with_scan_angle(90.0).unwrap();
    let (s, _) = scene(s0, 0.2, 0.3);
    let p = plan(&s, &source).unwrap();
    assert!(matches!(p.settings()[0].mode, PlanMode::Scan { axis: ScanAxis::Y, .. }), "{:?}", p.settings()[0].mode);
    let lines: Vec<(f64, f64, f64, f64)> = p
        .events()
        .iter()
        .filter_map(|e| match *e {
            PlanEvent::ScanRun { line, .. } => {
                Some((line.from.x_mm(), line.from.y_mm(), line.to.x_mm(), line.to.y_mm()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(lines.len(), 2, "{lines:?}");
    for (fx, fy, tx, ty) in lines {
        assert!(near(fx, tx) && (near(fx, 10.05) || near(fx, 10.15)), "vertical at a column centre");
        assert!(near((fy - ty).abs(), 0.3), "{fy}..{ty}");
    }
}

#[test]
fn an_oblique_scan_angle_keeps_the_image_in_place_and_needs_the_scan_angle_feature() {
    // Left half black, right half white over a 2 × 2 mm frame, scanned at 45°: every burnt run
    // lies left of the frame centre (± one dot), none outside the frame.
    let source = SourceImage::from_gray8(2, 1, vec![0, 255]).unwrap();
    let s0 = base(&source, DitherMode::Threshold { level: 128 }).with_scan_angle(45.0).unwrap();
    let (s, op) = scene(s0, 2.0, 2.0);
    let p = plan_angled(&s, &source).unwrap();
    assert!(matches!(p.settings()[0].mode, PlanMode::AngledScan { angle_deg, .. } if near(angle_deg, 45.0)));
    let events: Vec<_> = p
        .events()
        .iter()
        .filter_map(|e| match *e {
            PlanEvent::ScanRun { line, .. } => Some(line),
            _ => None,
        })
        .collect();
    assert!(events.len() > 10, "{} runs", events.len());
    let mut total = 0.0;
    for l in &events {
        for p in [l.from, l.to] {
            // A run ends on the edge of its last dot, which may overhang the frame by half a dot.
            assert!(p.x_mm() >= 10.0 - 0.06 && p.x_mm() <= 12.0 + 0.06, "{p:?}");
            assert!(p.y_mm() >= 10.0 - 0.06 && p.y_mm() <= 12.0 + 0.06, "{p:?}");
            assert!(p.x_mm() <= 11.0 + 0.15, "burn belongs to the black left half: {p:?}");
        }
        total += l.from.distance(l.to);
    }
    // Black area 1 × 2 mm at line interval 0.1 → about 20 mm of scan line (± edge cells).
    assert!((total - 20.0).abs() < 2.0, "total scanned length {total}");
    let features = required_features(s.snapshot().process().operation(op).unwrap());
    assert!(features.contains(&Feature::ScanAngle), "{features:?}");
}

// ---- crop, mask, gray method, sharpen ---------------------------------------------------------

#[test]
fn crop_stretches_only_the_window_over_the_frame() {
    // Source black, black, white, white. Cropping the white half burns nothing; cropping the
    // black half burns the whole frame as one run.
    let source = SourceImage::from_gray8(4, 1, vec![0, 0, 255, 255]).unwrap();
    let t = base(&source, DitherMode::Threshold { level: 128 });
    let crop = |x| Some(ImageCrop { x, y: 0, width: 2, height: 1 });
    let (s, _) = scene(t.with_crop(crop(2)).unwrap(), 0.4, 0.1);
    assert!(runs(plan(&s, &source).unwrap().events()).is_empty());
    let (s, _) = scene(t.with_crop(crop(0)).unwrap(), 0.4, 0.1);
    let r = runs(plan(&s, &source).unwrap().events());
    assert!(r.len() == 1 && near(r[0].0, 10.0) && near(r[0].1, 10.4), "{r:?}");
}

#[test]
fn a_crop_outside_the_source_is_a_compile_error() {
    let source = SourceImage::from_gray8(4, 1, vec![0, 0, 255, 255]).unwrap();
    let t = base(&source, DitherMode::Threshold { level: 128 });
    let outside = ImageCrop { x: 3, y: 0, width: 2, height: 1 };
    let (s, op) = scene(t.with_crop(Some(outside)).unwrap(), 0.4, 0.1);
    assert_eq!(
        plan(&s, &source).unwrap_err(),
        CompileError::Raster { operation: op, error: RasterError::CropOutOfBounds }
    );
}

fn ring_with_hole() -> (DocumentSession, GeometryId, Path) {
    // Even-odd: a 1 × 1 mm square at (10, 10) with a 0.4 × 0.4 mm hole at (10.3, 10.3).
    let ring = PathBuilder::new()
        .move_to(Point::new(10.0, 10.0))
        .line_to(Point::new(11.0, 10.0))
        .line_to(Point::new(11.0, 11.0))
        .line_to(Point::new(10.0, 11.0))
        .close()
        .move_to(Point::new(10.3, 10.3))
        .line_to(Point::new(10.7, 10.3))
        .line_to(Point::new(10.7, 10.7))
        .line_to(Point::new(10.3, 10.7))
        .close()
        .build()
        .unwrap();
    let id = GeometryId::from_raw(1);
    let object = GeometryObject::new(id, String::new(), ObjectKind::Path, GeometrySource::Drawn, ring.clone());
    let snap = DocumentSnapshot::from_parts(vec![object], 2).unwrap();
    (DocumentSession::from_snapshot(DocumentId::from_raw(1), snap), id, ring)
}

#[test]
fn mask_burns_only_inside_the_geometry() {
    let source = SourceImage::from_gray8(1, 1, vec![0]).unwrap();
    let t = base(&source, DitherMode::Threshold { level: 128 });
    let total = |t: ImageSettings| {
        let (mut s, id, _) = ring_with_hole();
        add_op(&mut s, draft(t), vec![id]);
        let p = plan(&s, &source).unwrap();
        let r = runs(p.events());
        (r.len(), r.iter().map(|(a, b, _)| b - a).sum::<f64>())
    };
    // Without a mask the black image fills the 1 × 1 bounds: 10 rows of one 1 mm run.
    let (n, len) = total(t);
    assert_eq!(n, 10);
    assert!(near(len, 10.0), "{len}");
    // With the mask the 4 × 4 cells of the hole stay white: 4 rows split in two runs, and
    // 100 − 16 cells burn.
    let (n, len) = total(t.with_mask(true));
    assert_eq!(n, 6 + 8, "6 full rows and 4 rows with two runs");
    assert!((len - 8.4).abs() < 1e-9, "{len}");
}

#[test]
fn the_processed_bitmap_equals_what_the_laser_scans_including_the_mask() {
    let source = SourceImage::from_gray8(1, 1, vec![0]).unwrap();
    let t = base(&source, DitherMode::Threshold { level: 128 }).with_mask(true);
    let (mut s, id, ring) = ring_with_hole();
    add_op(&mut s, draft(t), vec![id]);
    let p = plan(&s, &source).unwrap();
    let scanned = pattern(p.events(), 10, 10);
    let tolerance = options().tolerance;
    let bitmap = processed_bitmap(&t, &source, &ring, &tolerance, &never()).unwrap();
    assert_eq!((bitmap.width(), bitmap.height()), (10, 10));
    let from_bitmap: Vec<String> =
        bitmap.to_gray8().chunks(10).map(|row| row.iter().map(|&v| if v == 0 { '#' } else { '.' }).collect()).collect();
    assert_eq!(from_bitmap, scanned);
    assert_eq!(from_bitmap[4], "###....###", "hole cells 3..=6 are white in the bitmap");
}

#[test]
fn colour_to_gray_method_decides_which_pixel_burns() {
    // Red then green pixel. Rec. 709 gray = 54 and 182 → the red one burns; the red channel
    // alone gives 255 and 0 → the green one burns.
    let source = SourceImage::new(2, 1, PixelFormat::Rgb8, vec![255, 0, 0, 0, 255, 0]).unwrap();
    let t = base(&source, DitherMode::Threshold { level: 128 });
    let (s, _) = scene(t, 0.2, 0.1);
    assert_eq!(pattern(plan(&s, &source).unwrap().events(), 2, 1), ["#."]);
    let (s, _) = scene(t.with_gray_method(GrayMethod::Red), 0.2, 0.1);
    assert_eq!(pattern(plan(&s, &source).unwrap().events(), 2, 1), [".#"]);
}

#[test]
fn sharpening_is_applied_on_the_physical_grid_before_the_threshold() {
    // [0, 0, 90, 0, 0]: unsharp radius 1 dot, amount 1 lifts the centre to 150 (white).
    let source = SourceImage::from_gray8(5, 1, vec![0, 0, 90, 0, 0]).unwrap();
    let t = base(&source, DitherMode::Threshold { level: 128 });
    let (s, _) = scene(t, 0.5, 0.1);
    assert_eq!(pattern(plan(&s, &source).unwrap().events(), 5, 1), ["#####"]);
    let sharp = t.with_sharpen(Some(ImageSharpen::new(1.0, mm(0.1)).unwrap()));
    let (s, _) = scene(sharp, 0.5, 0.1);
    assert_eq!(pattern(plan(&s, &source).unwrap().events(), 5, 1), ["##.##"]);
}

// ---- new dither modes in the compiler ---------------------------------------------------------

#[test]
fn halftone_mode_prints_the_screen_of_the_physical_grid() {
    // 8 × 8 dots of mid gray, cell 0.4 mm = 4 dots, angle 0: the golden image of the kernel.
    let source = SourceImage::from_gray8(8, 8, vec![128; 64]).unwrap();
    let t = base(&source, DitherMode::Halftone).with_screen(ScreenSettings::new(mm(0.4), 0.0).unwrap());
    let (s, _) = scene(t, 0.8, 0.8);
    assert_eq!(
        pattern(plan(&s, &source).unwrap().events(), 8, 8),
        [".##..##.", "########", "########", ".##..##.", ".##..##.", "########", "########", ".##..##."]
    );
}

#[test]
fn newsprint_is_the_45_degree_screen_and_ignores_the_screen_angle() {
    let source = SourceImage::from_gray8(8, 8, vec![128; 64]).unwrap();
    let t = base(&source, DitherMode::Newsprint).with_screen(ScreenSettings::new(mm(0.4), 0.0).unwrap());
    let (s, _) = scene(t, 0.8, 0.8);
    assert_eq!(
        pattern(plan(&s, &source).unwrap().events(), 8, 8),
        [".###...#", "#.#..##.", "##..###.", "#...###.", "..##...#", ".###...#", ".###...#", "#...###."]
    );
}

#[test]
fn sketch_mode_burns_the_edges_of_the_image() {
    let px: Vec<u8> = (0..5).flat_map(|_| [0, 0, 0, 255, 255, 255]).collect();
    let source = SourceImage::from_gray8(6, 5, px).unwrap();
    let t = base(&source, DitherMode::Sketch { level: 128 });
    let (s, _) = scene(t, 0.6, 0.5);
    assert_eq!(pattern(plan(&s, &source).unwrap().events(), 6, 5), ["..##.."; 5]);
}

#[test]
fn every_new_dither_mode_is_a_plain_dither_feature() {
    let source = SourceImage::from_gray8(1, 1, vec![0]).unwrap();
    for mode in [DitherMode::Newsprint, DitherMode::Halftone, DitherMode::Sketch { level: 9 }, DitherMode::PassThrough]
    {
        let (s, op) = scene(base(&source, mode), 0.1, 0.1);
        let features = required_features(s.snapshot().process().operation(op).unwrap());
        assert_eq!(features, vec![Feature::ImageDither], "{mode:?}");
    }
}

// ---- save processed bitmap ---------------------------------------------------------------------

#[test]
fn the_processed_bitmap_is_exactly_the_scanned_pattern() {
    let source = gradient_8x4();
    let t = base(&source, DitherMode::Atkinson);
    let (s, _) = scene(t, 0.8, 0.4);
    let scanned = pattern(plan(&s, &source).unwrap().events(), 8, 4);
    assert_eq!(scanned, ["###.....", "####....", "##..##..", "##......"], "independent Atkinson reference");
    let outline = {
        let p = |x, y| Point::new(x, y);
        PathBuilder::new()
            .move_to(p(10.0, 10.0))
            .line_to(p(10.8, 10.0))
            .line_to(p(10.8, 10.4))
            .line_to(p(10.0, 10.4))
            .close()
            .build()
            .unwrap()
    };
    let bitmap = processed_bitmap(&t, &source, &outline, &options().tolerance, &never()).unwrap();
    let rows: Vec<String> =
        bitmap.to_gray8().chunks(8).map(|r| r.iter().map(|&v| if v == 0 { '#' } else { '.' }).collect()).collect();
    assert_eq!(rows, scanned);
    let png = burnburn_image::encode_png_gray8(bitmap.width(), bitmap.height(), &bitmap.to_gray8()).unwrap();
    let back = burnburn_image::decode(&png, burnburn_image::DecodeLimits::default()).unwrap();
    assert_eq!(back.samples(), &bitmap.to_gray8()[..], "the file holds the processed pixels");
}
