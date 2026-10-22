//! G8-cam-werk, W06: template localisation with rotation and scale. Ground truth is analytic: the
//! product is a smooth function of the position relative to its centre (sums of Gaussians, asymmetric
//! so that it has one orientation, as in the G7 fixture). The frame is made by evaluating that
//! function at the position turned back by the angle and divided by the scale, so the oracle never
//! uses the resampling of the locator under test.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_image::GrayImage;
use burnburn_image::PixelRect;
use burnburn_image::locate::{LocateError, crop_gray};
use burnburn_image::locate_pose::{PoseMatch, PoseParams, locate_pose};
use burnburn_types::{CancellationSource, CancellationToken};
use proptest::prelude::*;

const W: u32 = 200;
const H: u32 = 160;
const HALF: i64 = 14;

fn speckle(x: i64, y: i64) -> f32 {
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 29;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 32;
    (h % 1000) as f32 / 1000.0 * 6.0 - 3.0
}

/// The product at `(u, v)` from its centre in its own axes: a bright blob with a dark tail and a
/// bright satellite, so that it has one orientation.
fn product(u: f64, v: f64) -> f64 {
    let blob = 90.0 * (-(u * u + v * v) / (2.0 * 5.0 * 5.0)).exp();
    let tail = -40.0 * (-((u + 7.0).powi(2) + (v - 4.0).powi(2)) / (2.0 * 3.0 * 3.0)).exp();
    let satellite = 55.0 * (-((u - 8.0).powi(2) + (v + 6.0).powi(2)) / (2.0 * 2.0 * 2.0)).exp();
    blob + tail + satellite
}

/// A product centred at `(cx, cy)`, turned by `angle` degrees (rotation matrix, y down) and `scale` times as large.
#[derive(Clone, Copy)]
struct Placed {
    cx: f64,
    cy: f64,
    angle: f64,
    scale: f64,
}

fn frame(placed: &[Placed]) -> GrayImage {
    let mut values = Vec::with_capacity((W * H) as usize);
    for y in 0..H {
        for x in 0..W {
            let mut value = 128.0 + f64::from(speckle(i64::from(x), i64::from(y)));
            for p in placed {
                // The pixel centre relative to the product, turned back and divided by the scale.
                let (dx, dy) = (f64::from(x) + 0.5 - p.cx, f64::from(y) + 0.5 - p.cy);
                let a = p.angle.to_radians();
                let (u, v) = ((a.cos() * dx + a.sin() * dy) / p.scale, (-a.sin() * dx + a.cos() * dy) / p.scale);
                if u.abs() <= 24.0 && v.abs() <= 24.0 {
                    value += product(u, v);
                }
            }
            values.push(value.clamp(0.0, 255.0) as f32);
        }
    }
    GrayImage { width: W, height: H, values }
}

/// A 29 x 29 window around an unturned, unscaled product at the pixel centre (60, 60).
fn template() -> GrayImage {
    let reference = frame(&[Placed { cx: 60.5, cy: 60.5, angle: 0.0, scale: 1.0 }]);
    crop_gray(&reference, PixelRect { x: (60 - HALF) as u32, y: (60 - HALF) as u32, width: 29, height: 29 }).unwrap()
}

fn never() -> CancellationToken {
    CancellationToken::never()
}

fn turned(step: f64) -> PoseParams {
    PoseParams { min_score: 0.7, angle_step_deg: Some(step), ..PoseParams::default() }
}

fn angle_gap(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

fn one(found: &[PoseMatch]) -> PoseMatch {
    assert_eq!(found.len(), 1, "{found:?}");
    found[0]
}

#[test]
fn a_product_turned_by_any_angle_is_found_where_it_is_with_that_angle() {
    let template = template();
    for angle in [0.0, 30.0, 90.0, 137.0, 200.0, -60.0] {
        let f = frame(&[Placed { cx: 100.5, cy: 80.5, angle, scale: 1.0 }]);
        let m = one(&locate_pose(&f, &template, &turned(10.0), &never()).unwrap());
        assert!((m.x_px - 100.5).abs() < 0.75 && (m.y_px - 80.5).abs() < 0.75, "{angle}: {m:?}");
        assert!(angle_gap(m.angle_deg, angle) < 3.0, "{angle}: {m:?}");
        assert!((m.scale - 1.0).abs() < 1e-9, "no scale search keeps scale 1: {m:?}");
        assert!(m.score > 0.9, "{angle}: {m:?}");
    }
}

#[test]
fn the_angle_is_reported_in_one_turn_and_a_positive_angle_is_clockwise_with_y_down() {
    let template = template();
    // The satellite of the product sits at (8, -6) in the product's axes: turned by +90 degrees it
    // is at (6, 8) from the centre, i.e. to the lower right on the screen.
    let f = frame(&[Placed { cx: 100.5, cy: 80.5, angle: 90.0, scale: 1.0 }]);
    let m = one(&locate_pose(&f, &template, &turned(10.0), &never()).unwrap());
    assert!(angle_gap(m.angle_deg, 90.0) < 3.0, "{m:?}");
    assert!(m.angle_deg > -180.0 && m.angle_deg <= 180.0, "{m:?}");
    let value_at = |x: usize, y: usize| f.values[y * W as usize + x];
    assert!(value_at(106, 88) > value_at(94, 72) + 20.0, "the satellite is lower right in the frame");
}

#[test]
fn a_larger_or_smaller_product_is_found_with_its_scale() {
    let template = template();
    let params = PoseParams { scales: vec![0.8, 1.0, 1.25], ..turned(15.0) };
    for scale in [0.8, 1.0, 1.25] {
        let f = frame(&[Placed { cx: 100.5, cy: 80.5, angle: 0.0, scale }]);
        let m = one(&locate_pose(&f, &template, &params, &never()).unwrap());
        assert!((m.scale - scale).abs() < 0.05, "{scale}: {m:?}");
        assert!((m.x_px - 100.5).abs() < 1.0 && (m.y_px - 80.5).abs() < 1.0, "{scale}: {m:?}");
        assert!(angle_gap(m.angle_deg, 0.0) < 3.0, "{scale}: {m:?}");
    }
}

#[test]
fn a_scale_between_two_grid_scales_is_refined() {
    let template = template();
    let params = PoseParams { scales: vec![0.8, 1.0, 1.25], ..turned(15.0) };
    let f = frame(&[Placed { cx: 100.5, cy: 80.5, angle: 0.0, scale: 1.1 }]);
    let m = one(&locate_pose(&f, &template, &params, &never()).unwrap());
    assert!((m.scale - 1.1).abs() < 0.04, "{m:?}");
}

#[test]
fn turned_and_scaled_together() {
    let template = template();
    let params = PoseParams { scales: vec![0.8, 1.0, 1.25], ..turned(15.0) };
    let f = frame(&[Placed { cx: 110.5, cy: 85.5, angle: 50.0, scale: 0.8 }]);
    let m = one(&locate_pose(&f, &template, &params, &never()).unwrap());
    assert!((m.x_px - 110.5).abs() < 1.0 && (m.y_px - 85.5).abs() < 1.0, "{m:?}");
    assert!(angle_gap(m.angle_deg, 50.0) < 3.0 && (m.scale - 0.8).abs() < 0.05, "{m:?}");
}

#[test]
fn two_copies_at_different_angles_are_two_matches_each_with_its_own_angle() {
    let template = template();
    let f = frame(&[
        Placed { cx: 50.5, cy: 50.5, angle: 20.0, scale: 1.0 },
        Placed { cx: 140.5, cy: 100.5, angle: 250.0, scale: 1.0 },
    ]);
    let found = locate_pose(&f, &template, &turned(10.0), &never()).unwrap();
    assert_eq!(found.len(), 2, "{found:?}");
    let at = |x: f64, y: f64| *found.iter().find(|m| (m.x_px - x).abs() < 1.0 && (m.y_px - y).abs() < 1.0).unwrap();
    assert!(angle_gap(at(50.5, 50.5).angle_deg, 20.0) < 3.0);
    assert!(angle_gap(at(140.5, 100.5).angle_deg, 250.0) < 3.0);
}

#[test]
fn without_a_rotation_search_a_turned_product_scores_clearly_lower_and_is_put_off_its_place() {
    let template = template();
    let f = frame(&[Placed { cx: 100.5, cy: 80.5, angle: 90.0, scale: 1.0 }]);
    let blind_params = PoseParams { min_score: 0.5, angle_step_deg: None, ..PoseParams::default() };
    let blind = locate_pose(&f, &template, &blind_params, &never()).unwrap();
    let seeing = one(&locate_pose(&f, &template, &turned(10.0), &never()).unwrap());
    assert!(seeing.score > 0.97, "{seeing:?}");
    // The blob alone still correlates, but the tail and the satellite do not agree: a lower
    // similarity and a position that is off by pixels.
    for m in &blind {
        assert!(m.score < seeing.score - 0.05, "{m:?} vs {seeing:?}");
        assert!((m.x_px - 100.5).hypot(m.y_px - 80.5) > 1.5, "{m:?}");
        assert_eq!(m.angle_deg, 0.0);
    }
    // The same frame with the product upright is found by the translation-only setting.
    let f = frame(&[Placed { cx: 100.5, cy: 80.5, angle: 0.0, scale: 1.0 }]);
    let m = one(&locate_pose(&f, &template, &blind_params, &never()).unwrap());
    assert!((m.x_px - 100.5).abs() < 0.5 && (m.y_px - 80.5).abs() < 0.5 && m.angle_deg == 0.0, "{m:?}");
}

#[test]
fn a_frame_without_the_product_gives_nothing_and_a_defective_one_scores_lower() {
    let template = template();
    let blank = frame(&[]);
    assert!(locate_pose(&blank, &template, &turned(10.0), &never()).unwrap().is_empty());
    // Half the blob only: a different product.
    let f = frame(&[Placed { cx: 100.5, cy: 80.5, angle: 33.0, scale: 1.0 }]);
    let good = one(&locate_pose(&f, &template, &turned(10.0), &never()).unwrap());
    let mut spoiled = f.clone();
    for y in 66..76 {
        for x in 96..110 {
            spoiled.values[y * W as usize + x] = 128.0;
        }
    }
    let worse = locate_pose(&spoiled, &template, &PoseParams { min_score: 0.3, ..turned(10.0) }, &never()).unwrap();
    assert!(worse.first().is_none_or(|m| m.score < good.score - 0.03), "{worse:?} vs {good:?}");
}

#[test]
fn the_result_is_the_same_every_time() {
    let template = template();
    let f = frame(&[
        Placed { cx: 50.5, cy: 50.5, angle: 20.0, scale: 1.0 },
        Placed { cx: 140.5, cy: 100.5, angle: 250.0, scale: 1.0 },
    ]);
    let a = locate_pose(&f, &template, &turned(10.0), &never()).unwrap();
    let b = locate_pose(&f, &template, &turned(10.0), &never()).unwrap();
    assert_eq!(a, b);
}

#[test]
fn parameters_that_mean_nothing_are_refused() {
    let template = template();
    let f = frame(&[]);
    let bad = [
        PoseParams { min_score: 0.0, ..PoseParams::default() },
        PoseParams { max_matches: 0, ..PoseParams::default() },
        PoseParams { angle_step_deg: Some(0.0), ..PoseParams::default() },
        PoseParams { angle_step_deg: Some(f64::NAN), ..PoseParams::default() },
        PoseParams { angle_step_deg: Some(120.0), ..PoseParams::default() },
        PoseParams { scales: vec![], ..PoseParams::default() },
        PoseParams { scales: vec![0.0], ..PoseParams::default() },
        PoseParams { scales: vec![1.0, f64::INFINITY], ..PoseParams::default() },
        PoseParams { angle_step_deg: Some(1.0), scales: vec![1.0; 6], ..PoseParams::default() },
    ];
    for params in bad {
        assert_eq!(locate_pose(&f, &template, &params, &never()), Err(LocateError::BadParams), "{params:?}");
    }
}

#[test]
fn a_flat_template_and_a_template_larger_than_the_frame_are_refused() {
    let flat = GrayImage { width: 20, height: 20, values: vec![100.0; 400] };
    assert_eq!(locate_pose(&frame(&[]), &flat, &PoseParams::default(), &never()), Err(LocateError::FlatTemplate));
    let small = GrayImage { width: 10, height: 10, values: (0..100).map(|i| i as f32).collect() };
    assert_eq!(
        locate_pose(&small, &template(), &PoseParams::default(), &never()),
        Err(LocateError::TemplateLargerThanFrame)
    );
}

#[test]
fn a_cancelled_search_returns_nothing() {
    let source = CancellationSource::new();
    let token = source.token();
    source.cancel();
    let f = frame(&[Placed { cx: 100.5, cy: 80.5, angle: 10.0, scale: 1.0 }]);
    assert_eq!(locate_pose(&f, &template(), &turned(10.0), &token), Err(LocateError::Cancelled));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10))]

    /// Wherever the product is and however it is turned, its pose is found.
    #[test]
    fn any_pose_is_recovered(angle in 0.0f64..360.0, cx in 50.0f64..150.0, cy in 45.0f64..115.0) {
        let template = template();
        let f = frame(&[Placed { cx, cy, angle, scale: 1.0 }]);
        let found = locate_pose(&f, &template, &turned(10.0), &never()).unwrap();
        prop_assert_eq!(found.len(), 1);
        let m = found[0];
        prop_assert!((m.x_px - cx).abs() < 0.75 && (m.y_px - cy).abs() < 0.75, "{:?} at ({}, {})", m, cx, cy);
        prop_assert!(angle_gap(m.angle_deg, angle) < 3.0, "{:?} for {}", m, angle);
    }
}

/// `cargo test --release -p burnburn-image --test g8_locate_pose -- --ignored --nocapture`: the cost
/// on a camera-sized frame (1280 x 720, an 87 x 87 template). Printed, not asserted on time.
#[test]
#[ignore = "measurement"]
fn measure_the_search_on_a_camera_sized_frame() {
    use std::time::Instant;
    let (w, h) = (1280u32, 720u32);
    let mut values = Vec::with_capacity((w * h) as usize);
    let p = Placed { cx: 700.5, cy: 400.5, angle: 70.0, scale: 3.0 };
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = (f64::from(x) + 0.5 - p.cx, f64::from(y) + 0.5 - p.cy);
            let a = p.angle.to_radians();
            let (u, v) = ((a.cos() * dx + a.sin() * dy) / p.scale, (-a.sin() * dx + a.cos() * dy) / p.scale);
            let mut value = 128.0 + f64::from(speckle(i64::from(x), i64::from(y)));
            if u.abs() <= 24.0 && v.abs() <= 24.0 {
                value += product(u, v);
            }
            values.push(value.clamp(0.0, 255.0) as f32);
        }
    }
    let big = GrayImage { width: w, height: h, values };
    // An 87 x 87 window around the unturned product at scale 3.
    let mut tv = Vec::new();
    for y in 0..87 {
        for x in 0..87 {
            let (u, v) = ((f64::from(x) - 43.0) / 3.0, (f64::from(y) - 43.0) / 3.0);
            tv.push((128.0 + product(u, v)).clamp(0.0, 255.0) as f32);
        }
    }
    let template = GrayImage { width: 87, height: 87, values: tv };
    let cases = [
        ("translation only", PoseParams { min_score: 0.7, angle_step_deg: None, ..PoseParams::default() }),
        ("rotation, 15 degrees", PoseParams { min_score: 0.7, angle_step_deg: Some(15.0), ..PoseParams::default() }),
        (
            "rotation 10 degrees, 3 scales",
            PoseParams {
                min_score: 0.7,
                angle_step_deg: Some(10.0),
                scales: vec![0.8, 1.0, 1.25],
                ..PoseParams::default()
            },
        ),
    ];
    for (label, params) in cases {
        let started = Instant::now();
        let found = locate_pose(&big, &template, &params, &never()).unwrap();
        let secs = started.elapsed().as_secs_f64();
        println!(
            "MEASURE 1280x720, template 87x87, {label}: {} match(es), {secs:.2} s, best {:?}",
            found.len(),
            found.first()
        );
    }
}
