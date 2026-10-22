//! G9-cam-werk, W06: template localisation of a mirrored product, of a product seen at an angle
//! (a homography) and of a product partly outside the frame. Ground truth is analytic as in
//! `g8_locate_pose`: the product is a smooth function of the position relative to its centre and
//! the frame is made by evaluating it at the position the forward model sends back, so the oracle
//! never uses the resampling of the locator under test.
//!
//! Forward model (the contract of `PoseMatch`): a template offset `(ox, oy)` from its centre is
//! mirrored (`ox -> -ox`) when the product is mirrored, giving `m`; with the template's half
//! diagonal `R` the view divides by `w = 1 + (tx * m.x + ty * m.y) / R`, and the offset in the frame
//! is `scale * Rot(angle) * m / w` from the centre of the product.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_image::GrayImage;
use burnburn_image::PixelRect;
use burnburn_image::locate::{LocateError, crop_gray};
use burnburn_image::locate_pose::{PoseMatch, PoseParams, locate_pose};
use burnburn_types::CancellationToken;

const W: u32 = 200;
const H: u32 = 160;
const HALF: i64 = 14;
/// Half the diagonal of the 29 x 29 template.
const R: f64 = 20.506_096_654_409_877;

fn speckle(x: i64, y: i64) -> f32 {
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 29;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 32;
    (h % 1000) as f32 / 1000.0 * 6.0 - 3.0
}

/// A blob with three features of different strength and size at different distances: the cyclic
/// order of the features is what tells the product from its mirror image (the G8 product, with two
/// features, is nearly its own mirror image up to a turn).
fn product(u: f64, v: f64) -> f64 {
    let blob = 90.0 * (-(u * u + v * v) / (2.0 * 4.0 * 4.0)).exp();
    let tail = -45.0 * (-((u + 10.0).powi(2) + (v - 6.0).powi(2)) / (2.0 * 3.0 * 3.0)).exp();
    let satellite = 60.0 * (-((u - 11.0).powi(2) + (v + 8.0).powi(2)) / (2.0 * 2.5 * 2.5)).exp();
    let third = 50.0 * (-((u - 4.0).powi(2) + (v - 12.0).powi(2)) / (2.0 * 2.0 * 2.0)).exp();
    blob + tail + satellite + third
}

#[derive(Clone, Copy)]
struct Placed {
    cx: f64,
    cy: f64,
    angle: f64,
    scale: f64,
    mirrored: bool,
    tilt: (f64, f64),
}

impl Placed {
    fn at(cx: f64, cy: f64) -> Placed {
        Placed { cx, cy, angle: 0.0, scale: 1.0, mirrored: false, tilt: (0.0, 0.0) }
    }

    /// The template offset that lands on the frame offset `(dx, dy)`, or `None` when the view does not reach it.
    fn template_offset(&self, dx: f64, dy: f64) -> Option<(f64, f64)> {
        // d * w = A m  with  w = 1 + t . m / R   =>   (A - d t^T / R) m = d.
        let a = self.angle.to_radians();
        let (c, s) = (a.cos() * self.scale, a.sin() * self.scale);
        let (tx, ty) = (self.tilt.0 / R, self.tilt.1 / R);
        let (m11, m12, m21, m22) = (c - dx * tx, -s - dx * ty, s - dy * tx, c - dy * ty);
        let det = m11 * m22 - m12 * m21;
        if det.abs() < 1e-12 {
            return None;
        }
        let (mx, my) = ((m22 * dx - m12 * dy) / det, (-m21 * dx + m11 * dy) / det);
        if 1.0 + self.tilt.0 * mx / R + self.tilt.1 * my / R <= 0.1 {
            return None;
        }
        Some((if self.mirrored { -mx } else { mx }, my))
    }
}

fn frame_sized(w: u32, h: u32, placed: &[Placed]) -> GrayImage {
    let mut values = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let mut value = 128.0 + f64::from(speckle(i64::from(x), i64::from(y)));
            for p in placed {
                let (dx, dy) = (f64::from(x) + 0.5 - p.cx, f64::from(y) + 0.5 - p.cy);
                if let Some((u, v)) = p.template_offset(dx, dy)
                    && u.abs() <= 24.0
                    && v.abs() <= 24.0
                {
                    value += product(u, v);
                }
            }
            values.push(value.clamp(0.0, 255.0) as f32);
        }
    }
    GrayImage { width: w, height: h, values }
}

fn frame(placed: &[Placed]) -> GrayImage {
    frame_sized(W, H, placed)
}

fn template() -> GrayImage {
    let reference = frame(&[Placed::at(60.5, 60.5)]);
    crop_gray(&reference, PixelRect { x: (60 - HALF) as u32, y: (60 - HALF) as u32, width: 29, height: 29 }).unwrap()
}

fn never() -> CancellationToken {
    CancellationToken::never()
}

fn base() -> PoseParams {
    PoseParams { min_score: 0.85, angle_step_deg: Some(10.0), ..PoseParams::default() }
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
fn a_mirrored_product_is_found_as_mirrored_with_its_angle_when_mirroring_is_asked_for() {
    let template = template();
    let params = PoseParams { mirror: true, ..base() };
    for angle in [0.0, 40.0, 135.0, -70.0] {
        let f = frame(&[Placed { angle, mirrored: true, ..Placed::at(100.5, 80.5) }]);
        let m = one(&locate_pose(&f, &template, &params, &never()).unwrap());
        assert!(m.mirrored, "{angle}: {m:?}");
        assert!((m.x_px - 100.5).abs() < 0.75 && (m.y_px - 80.5).abs() < 0.75, "{angle}: {m:?}");
        assert!(angle_gap(m.angle_deg, angle) < 3.0, "{angle}: {m:?}");
        assert!(m.score > 0.9, "{angle}: {m:?}");
    }
}

#[test]
fn a_mirrored_product_is_not_taken_for_the_original_unless_mirroring_is_asked_for() {
    let template = template();
    let f = frame(&[Placed { angle: 40.0, mirrored: true, ..Placed::at(100.5, 80.5) }]);
    let asking = one(&locate_pose(&f, &template, &PoseParams { mirror: true, ..base() }, &never()).unwrap());
    assert!(asking.score > 0.95 && asking.mirrored, "{asking:?}");
    // Without it the best the original can do is clearly worse (the blob alone resembles it).
    let strict = locate_pose(&f, &template, &PoseParams { min_score: 0.5, ..base() }, &never()).unwrap();
    assert!(strict.iter().all(|m| !m.mirrored && m.score < asking.score - 0.04), "{strict:?} vs {asking:?}");
}

#[test]
fn an_original_product_is_not_reported_as_mirrored_when_mirroring_is_allowed() {
    let template = template();
    let f = frame(&[Placed { angle: 25.0, ..Placed::at(100.5, 80.5) }]);
    let m = one(&locate_pose(&f, &template, &PoseParams { mirror: true, ..base() }, &never()).unwrap());
    assert!(!m.mirrored && m.score > 0.9, "{m:?}");
    assert!(angle_gap(m.angle_deg, 25.0) < 3.0, "{m:?}");
}

#[test]
fn an_original_and_a_mirrored_product_in_one_frame_are_told_apart() {
    let template = template();
    let f = frame(&[
        Placed { angle: 15.0, ..Placed::at(50.5, 50.5) },
        Placed { angle: 200.0, mirrored: true, ..Placed::at(140.5, 100.5) },
    ]);
    let found = locate_pose(&f, &template, &PoseParams { mirror: true, ..base() }, &never()).unwrap();
    assert_eq!(found.len(), 2, "{found:?}");
    let at = |x: f64, y: f64| *found.iter().find(|m| (m.x_px - x).abs() < 1.0 && (m.y_px - y).abs() < 1.0).unwrap();
    assert!(!at(50.5, 50.5).mirrored && at(140.5, 100.5).mirrored);
}

#[test]
fn a_product_seen_at_an_angle_is_found_with_its_tilt_when_perspective_is_asked_for() {
    let template = template();
    let params = PoseParams { max_tilt: Some(0.4), ..base() };
    for (angle, tilt) in [(0.0, (0.25, 0.0)), (0.0, (0.0, -0.3)), (30.0, (-0.15, 0.2)), (-80.0, (0.3, 0.1))] {
        let f = frame(&[Placed { angle, tilt, ..Placed::at(100.5, 80.5) }]);
        let m = one(&locate_pose(&f, &template, &params, &never()).unwrap());
        assert!((m.x_px - 100.5).abs() < 1.0 && (m.y_px - 80.5).abs() < 1.0, "{angle} {tilt:?}: {m:?}");
        assert!(angle_gap(m.angle_deg, angle) < 4.0, "{angle} {tilt:?}: {m:?}");
        assert!((m.tilt.0 - tilt.0).abs() < 0.07 && (m.tilt.1 - tilt.1).abs() < 0.07, "{angle} {tilt:?}: {m:?}");
        assert!(m.score > 0.93, "{angle} {tilt:?}: {m:?}");
    }
}

#[test]
fn a_tilted_product_scores_clearly_lower_when_only_a_flat_view_is_tried() {
    let template = template();
    let f = frame(&[Placed { tilt: (0.45, 0.0), ..Placed::at(100.5, 80.5) }]);
    let flat = locate_pose(&f, &template, &PoseParams { min_score: 0.5, ..base() }, &never()).unwrap();
    let seeing = one(&locate_pose(&f, &template, &PoseParams { max_tilt: Some(0.6), ..base() }, &never()).unwrap());
    assert!(flat.iter().all(|m| m.score < seeing.score - 0.03 && m.tilt == (0.0, 0.0)), "{flat:?} vs {seeing:?}");
}

#[test]
fn mirrored_turned_scaled_and_tilted_at_once() {
    let template = template();
    let params = PoseParams { mirror: true, max_tilt: Some(0.4), scales: vec![0.8, 1.0, 1.25], ..base() };
    let f = frame(&[Placed {
        angle: 100.0,
        scale: 1.1,
        mirrored: true,
        tilt: (0.15, -0.2),
        ..Placed::at(110.5, 85.5)
    }]);
    let m = one(&locate_pose(&f, &template, &params, &never()).unwrap());
    assert!(m.mirrored && m.score > 0.93, "{m:?}");
    assert!((m.x_px - 110.5).abs() < 1.2 && (m.y_px - 85.5).abs() < 1.2, "{m:?}");
    assert!(angle_gap(m.angle_deg, 100.0) < 4.0 && (m.scale - 1.1).abs() < 0.06, "{m:?}");
}

#[test]
fn a_product_partly_outside_the_frame_is_found_and_marked_as_partly_visible() {
    let template = template();
    let params = PoseParams { min_visible: 0.5, ..base() };
    // Left edge: the centre is 6 pixels inside, so (6 + 14.5) / 29 of the template is in the frame.
    let left = frame(&[Placed::at(6.5, 80.5)]);
    let m = one(&locate_pose(&left, &template, &params, &never()).unwrap());
    assert!((m.x_px - 6.5).abs() < 1.0 && (m.y_px - 80.5).abs() < 1.0, "{m:?}");
    assert!((m.visible - (6.5 + 14.5) / 29.0).abs() < 0.05, "{m:?}");
    assert!(m.visible < 1.0 && m.score > 0.9, "{m:?}");
    // Bottom right corner: about 57 % of the template.
    let corner = frame(&[Placed::at(f64::from(W) - 8.5, f64::from(H) - 6.5)]);
    let m = one(&locate_pose(&corner, &template, &params, &never()).unwrap());
    assert!((m.x_px - (f64::from(W) - 8.5)).abs() < 1.0 && (m.y_px - (f64::from(H) - 6.5)).abs() < 1.0, "{m:?}");
    assert!((m.visible - (8.5 + 14.5) / 29.0 * (6.5 + 14.5) / 29.0).abs() < 0.05, "{m:?}");
}

#[test]
fn a_centre_outside_the_frame_still_counts_when_enough_of_the_product_is_inside() {
    let template = template();
    // The centre is 2 pixels outside the frame: (14.5 - 2) / 29 = 43 % of the width is inside.
    let f = frame(&[Placed::at(-2.0, 80.5)]);
    let params = PoseParams { min_visible: 0.4, ..base() };
    let m = one(&locate_pose(&f, &template, &params, &never()).unwrap());
    assert!((m.x_px + 2.0).abs() < 1.2 && (m.y_px - 80.5).abs() < 1.0, "{m:?}");
    assert!(m.visible > 0.38 && m.visible < 0.5, "{m:?}");
}

#[test]
fn a_partly_visible_product_is_not_found_unless_that_is_asked_for() {
    let template = template();
    let left = frame(&[Placed::at(6.5, 80.5)]);
    assert!(locate_pose(&left, &template, &base(), &never()).unwrap().is_empty());
    // Asking for more than is visible reports nothing that is less than that (a shifted pose that
    // lies wholly inside may still resemble the cut product, and then it says so by its place).
    let strict = PoseParams { min_visible: 0.9, ..base() };
    for m in locate_pose(&left, &template, &strict, &never()).unwrap() {
        assert!(m.visible >= 0.9 && m.x_px > 12.0, "{m:?}");
    }
}

#[test]
fn a_whole_product_is_reported_as_wholly_visible_and_a_noise_frame_gives_nothing_even_when_partial_is_allowed() {
    let template = template();
    let params = PoseParams { min_visible: 0.5, ..base() };
    let f = frame(&[Placed { angle: 20.0, ..Placed::at(100.5, 80.5) }]);
    let m = one(&locate_pose(&f, &template, &params, &never()).unwrap());
    assert_eq!(m.visible, 1.0, "{m:?}");
    assert!(locate_pose(&frame(&[]), &template, &params, &never()).unwrap().is_empty());
}

#[test]
fn a_turned_product_cut_by_the_edge_is_found_with_its_angle() {
    let template = template();
    let params = PoseParams { min_visible: 0.5, ..base() };
    let f = frame(&[Placed { angle: 60.0, ..Placed::at(f64::from(W) - 9.5, 40.5) }]);
    let m = one(&locate_pose(&f, &template, &params, &never()).unwrap());
    assert!(m.visible < 1.0, "{m:?}");
    assert!(angle_gap(m.angle_deg, 60.0) < 4.0, "{m:?}");
    assert!((m.x_px - (f64::from(W) - 9.5)).abs() < 1.2 && (m.y_px - 40.5).abs() < 1.2, "{m:?}");
}

#[test]
fn the_new_parameters_must_mean_something() {
    let template = template();
    let f = frame(&[]);
    let bad = [
        PoseParams { max_tilt: Some(0.0), ..PoseParams::default() },
        PoseParams { max_tilt: Some(0.9), ..PoseParams::default() },
        PoseParams { max_tilt: Some(f64::NAN), ..PoseParams::default() },
        PoseParams { min_visible: 0.0, ..PoseParams::default() },
        PoseParams { min_visible: 1.5, ..PoseParams::default() },
        PoseParams { min_visible: f64::NAN, ..PoseParams::default() },
        // The grid of poses counts the mirror and the tilts: 72 angles x 2 x 5 tilts x 3 scales.
        PoseParams {
            angle_step_deg: Some(5.0),
            mirror: true,
            max_tilt: Some(0.3),
            scales: vec![0.8, 1.0, 1.25],
            ..PoseParams::default()
        },
    ];
    for params in bad {
        assert_eq!(locate_pose(&f, &template, &params, &never()), Err(LocateError::BadParams), "{params:?}");
    }
}

#[test]
fn the_result_with_every_option_is_the_same_every_time() {
    let template = template();
    let params = PoseParams { mirror: true, max_tilt: Some(0.4), min_visible: 0.5, ..base() };
    let f = frame(&[
        Placed { angle: 15.0, tilt: (0.2, 0.0), ..Placed::at(50.5, 50.5) },
        Placed { angle: 200.0, mirrored: true, ..Placed::at(150.5, 100.5) },
        Placed::at(5.5, 120.5),
    ]);
    let a = locate_pose(&f, &template, &params, &never()).unwrap();
    let b = locate_pose(&f, &template, &params, &never()).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.len(), 3, "{a:?}");
}
