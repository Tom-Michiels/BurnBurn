//! G7-werk, W06: template localisation in camera frames. Ground truth is analytic: a product is a
//! smooth function of the distance to its centre (sums of Gaussians, asymmetric so it has one
//! orientation), rendered on a mid-grey background with a deterministic speckle. The matcher must
//! find every copy where it was drawn, say how alike it is (a similarity, never millimetres), report
//! two alike copies as two matches instead of picking one, and rank a defective copy lower.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_image::PixelRect;
use burnburn_image::locate::{LocateError, LocateMatch, LocateParams, crop_gray, gray_of_source, locate};
use burnburn_image::{GrayImage, SourceImage};
use burnburn_types::{CancellationSource, CancellationToken};
use proptest::prelude::*;

const W: u32 = 160;
const H: u32 = 120;
/// The template is a 29 x 29 window around the centre of a product.
const HALF: i64 = 14;

fn speckle(x: i64, y: i64) -> f32 {
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 29;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 32;
    (h % 1000) as f32 / 1000.0 * 6.0 - 3.0
}

/// The product at distance `(dx, dy)` from its centre (pixels): a bright blob with a dark tail and a
/// small bright satellite, so that it has one orientation. `defect` leaves the satellite out.
fn product(dx: f64, dy: f64, defect: bool) -> f64 {
    let blob = 90.0 * (-(dx * dx + dy * dy) / (2.0 * 5.0 * 5.0)).exp();
    let tail = -40.0 * (-((dx + 7.0).powi(2) + (dy - 4.0).powi(2)) / (2.0 * 3.0 * 3.0)).exp();
    let satellite = if defect { 0.0 } else { 55.0 * (-((dx - 8.0).powi(2) + (dy + 6.0).powi(2)) / (2.0 * 2.0 * 2.0)).exp() };
    blob + tail + satellite
}

/// A frame with products centred on the pixel indices `at` (fractions allowed).
fn frame(at: &[(f64, f64, bool)]) -> GrayImage {
    let mut values = Vec::with_capacity((W * H) as usize);
    for y in 0..H {
        for x in 0..W {
            let mut v = 128.0 + f64::from(speckle(i64::from(x), i64::from(y)));
            for (cx, cy, defect) in at {
                let (dx, dy) = (f64::from(x) - cx, f64::from(y) - cy);
                if dx.abs() <= HALF as f64 + 1.0 && dy.abs() <= HALF as f64 + 1.0 {
                    v += product(dx, dy, *defect);
                }
            }
            values.push(v.clamp(0.0, 255.0) as f32);
        }
    }
    GrayImage { width: W, height: H, values }
}

/// What the operator would crop from a photo of one genuine product at (60, 60).
fn template() -> GrayImage {
    let reference = frame(&[(60.0, 60.0, false)]);
    crop_gray(&reference, PixelRect { x: 46, y: 46, width: 29, height: 29 }).unwrap()
}

fn token() -> CancellationToken {
    CancellationToken::never()
}

fn find(frame: &GrayImage, params: &LocateParams) -> Vec<LocateMatch> {
    locate(frame, &template(), params, &token()).unwrap()
}

fn near(m: &LocateMatch, x: f64, y: f64, tol: f64) -> bool {
    (m.x_px - x).abs() <= tol && (m.y_px - y).abs() <= tol
}

#[test]
fn every_copy_is_found_where_it_was_drawn_in_pixel_edge_coordinates() {
    let f = frame(&[(100.0, 70.0, false), (30.0, 95.0, false)]);
    let found = find(&f, &LocateParams { min_score: 0.9, ..LocateParams::default() });
    assert_eq!(found.len(), 2, "{found:?}");
    // Pixel (100, 70) covers [100, 101) x [70, 71): its centre is (100.5, 70.5).
    assert!(found.iter().any(|m| near(m, 100.5, 70.5, 0.02)), "{found:?}");
    assert!(found.iter().any(|m| near(m, 30.5, 95.5, 0.02)), "{found:?}");
    assert!(found.iter().all(|m| m.score > 0.97 && m.score <= 1.0 + 1e-9), "{found:?}");
}

#[test]
fn a_copy_between_pixels_is_found_to_a_tenth_of_a_pixel() {
    for (cx, cy) in [(100.3, 69.8), (88.5, 40.5), (50.75, 80.25)] {
        let f = frame(&[(cx, cy, false)]);
        let found = find(&f, &LocateParams { min_score: 0.8, ..LocateParams::default() });
        assert_eq!(found.len(), 1, "{found:?} for ({cx}, {cy})");
        assert!(near(&found[0], cx + 0.5, cy + 0.5, 0.1), "{:?} vs ({}, {})", found[0], cx + 0.5, cy + 0.5);
    }
}

#[test]
fn two_alike_copies_are_two_matches_with_alike_scores_and_a_fixed_order() {
    let f = frame(&[(110.0, 40.0, false), (45.0, 85.0, false)]);
    let found = find(&f, &LocateParams::default());
    assert_eq!(found.len(), 2);
    assert!((found[0].score - found[1].score).abs() < 0.02, "alike copies score alike: {found:?}");
    assert_eq!(found, find(&f, &LocateParams::default()), "deterministic");
    // Sorted by score, best first.
    assert!(found[0].score >= found[1].score);
}

#[test]
fn a_defective_look_alike_scores_lower_and_a_strict_threshold_leaves_it_out() {
    let f = frame(&[(110.0, 40.0, false), (45.0, 85.0, true)]);
    let lenient = find(&f, &LocateParams { min_score: 0.5, ..LocateParams::default() });
    assert_eq!(lenient.len(), 2, "{lenient:?}");
    assert!(near(&lenient[0], 110.5, 40.5, 0.05), "the genuine copy ranks first: {lenient:?}");
    assert!(lenient[0].score - lenient[1].score > 0.03, "{lenient:?}");
    let strict = find(&f, &LocateParams { min_score: 0.97, ..LocateParams::default() });
    assert_eq!(strict.len(), 1);
    assert!(near(&strict[0], 110.5, 40.5, 0.05));
}

#[test]
fn the_neighbourhood_of_a_peak_is_not_a_second_match() {
    let f = frame(&[(80.0, 60.0, false)]);
    let found = find(&f, &LocateParams { min_score: 0.2, max_matches: 50, ..LocateParams::default() });
    // The default separation is half the template: no two matches closer than that.
    for (i, a) in found.iter().enumerate() {
        for b in &found[i + 1..] {
            assert!((a.x_px - b.x_px).hypot(a.y_px - b.y_px) >= 14.0, "{a:?} {b:?}");
        }
    }
    assert!(near(&found[0], 80.5, 60.5, 0.05));
}

#[test]
fn no_copy_means_no_match_and_the_number_of_matches_is_capped() {
    let empty = frame(&[]);
    assert!(find(&empty, &LocateParams::default()).is_empty());
    let f = frame(&[(40.0, 30.0, false), (100.0, 30.0, false), (40.0, 90.0, false), (100.0, 90.0, false)]);
    let capped = find(&f, &LocateParams { max_matches: 3, ..LocateParams::default() });
    assert_eq!(capped.len(), 3);
}

#[test]
fn the_coarse_search_agrees_with_the_exhaustive_one() {
    let f = frame(&[(100.3, 69.8, false), (30.0, 95.0, false), (70.0, 25.5, true)]);
    let params = LocateParams { min_score: 0.5, ..LocateParams::default() };
    let coarse = find(&f, &params);
    let exact = find(&f, &LocateParams { coarse_factor: Some(1), ..params });
    assert_eq!(coarse.len(), exact.len(), "{coarse:?} vs {exact:?}");
    for (c, e) in coarse.iter().zip(&exact) {
        assert!(near(c, e.x_px, e.y_px, 0.05) && (c.score - e.score).abs() < 0.01, "{c:?} vs {e:?}");
    }
}

#[test]
fn what_cannot_be_matched_is_a_typed_error() {
    let f = frame(&[(80.0, 60.0, false)]);
    let ok = LocateParams::default();
    let flat = GrayImage { width: 20, height: 20, values: vec![100.0; 400] };
    assert_eq!(locate(&f, &flat, &ok, &token()), Err(LocateError::FlatTemplate), "a plain patch is no pattern");
    let tiny = GrayImage { width: 3, height: 3, values: (0..9).map(|v| v as f32).collect() };
    assert_eq!(locate(&f, &tiny, &ok, &token()), Err(LocateError::TemplateTooSmall));
    let huge = frame(&[]);
    let small_frame = crop_gray(&huge, PixelRect { x: 0, y: 0, width: 20, height: 20 }).unwrap();
    assert_eq!(locate(&small_frame, &template(), &ok, &token()), Err(LocateError::TemplateLargerThanFrame));
    for bad in [
        LocateParams { min_score: 0.0, ..ok },
        LocateParams { min_score: 1.5, ..ok },
        LocateParams { min_score: f64::NAN, ..ok },
        LocateParams { max_matches: 0, ..ok },
        LocateParams { min_separation_px: Some(-1.0), ..ok },
        LocateParams { coarse_factor: Some(0), ..ok },
    ] {
        assert_eq!(locate(&f, &template(), &bad, &token()), Err(LocateError::BadParams), "{bad:?}");
    }
}

#[test]
fn a_cancelled_search_is_cancelled_and_never_a_partial_result() {
    let f = frame(&[(80.0, 60.0, false)]);
    let source = CancellationSource::new();
    let token = source.token();
    source.cancel();
    assert_eq!(locate(&f, &template(), &LocateParams::default(), &token), Err(LocateError::Cancelled));
}

#[test]
fn a_crop_is_the_sub_image_and_outside_the_frame_it_is_refused() {
    let f = frame(&[(80.0, 60.0, false)]);
    let c = crop_gray(&f, PixelRect { x: 10, y: 20, width: 5, height: 4 }).unwrap();
    assert_eq!((c.width, c.height), (5, 4));
    assert_eq!(c.values[0], f.values[20 * W as usize + 10]);
    assert_eq!(c.values[3 * 5 + 4], f.values[23 * W as usize + 14]);
    assert_eq!(crop_gray(&f, PixelRect { x: 158, y: 0, width: 5, height: 5 }), Err(LocateError::BadCrop));
    assert_eq!(crop_gray(&f, PixelRect { x: 0, y: 0, width: 0, height: 5 }), Err(LocateError::BadCrop));
}

#[test]
fn a_camera_frame_becomes_grey_the_way_the_tracer_sees_it() {
    // Black, white and a mid red: Rec. 709 luma of gamma-encoded sRGB.
    let rgba = vec![0, 0, 0, 255, 255, 255, 255, 255, 200, 0, 0, 255];
    let source = SourceImage::from_rgba8(3, 1, rgba).unwrap();
    let g = gray_of_source(&source);
    assert_eq!((g.width, g.height), (3, 1));
    assert!((g.values[0] - 0.0).abs() < 1e-3 && (g.values[1] - 255.0).abs() < 1e-3);
    assert!((g.values[2] - 0.2126 * 200.0).abs() < 1e-2, "{}", g.values[2]);
}

proptest! {
    /// Similarity ignores lighting: a frame with its brightness and contrast changed (as long as
    /// nothing clips) is matched at the same places with the same scores.
    #[test]
    fn matches_do_not_depend_on_brightness_and_contrast(gain in 0.3f32..1.0, offset in -20.0f32..40.0) {
        let f = frame(&[(100.0, 70.0, false), (35.0, 90.0, true)]);
        let params = LocateParams { min_score: 0.5, ..LocateParams::default() };
        let base = locate(&f, &template(), &params, &token()).unwrap();
        let relit = GrayImage {
            values: f.values.iter().map(|v| (v * gain + offset).clamp(0.0, 255.0)).collect(),
            ..f.clone()
        };
        let again = locate(&relit, &template(), &params, &token()).unwrap();
        prop_assert_eq!(base.len(), again.len());
        for (a, b) in base.iter().zip(&again) {
            prop_assert!(near(a, b.x_px, b.y_px, 0.05), "{:?} vs {:?}", a, b);
            prop_assert!((a.score - b.score).abs() < 0.02, "{:?} vs {:?}", a, b);
        }
    }
}
