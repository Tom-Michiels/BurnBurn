//! Tangential lens terms (G8-trace, T03): Brown–Conrady `p1`, `p2` next to the radial `k1`, `k2`.
//! The oracles are written out here: a hand-computed value of the forward law, and a photo rendered
//! from a scene of straight lines through that law, inverted by fixed-point iteration in the test.

#![allow(clippy::unwrap_used)]

use burnburn_image::{ImageError, ImageRecipe, LensDistortion, PixelScale, SourceImage, Threshold, prepare};
use burnburn_types::{CancellationToken, CanonicalEncode, CanonicalWriter};

const W: u32 = 201;
const H: u32 = 151;

/// Photo of vertical lines at `lines_x` (undistorted pixel columns) seen through a lens `(k1, k2, p1,
/// p2)`. For each photo pixel the undistorted position `u` solves `d = radial(u)·u + tangential(u)`
/// by iterating `u ← (d − tangential(u)) / radial(u)`.
fn photo(lines_x: &[f64], (k1, k2, p1, p2): (f64, f64, f64, f64)) -> SourceImage {
    let (cx, cy) = (f64::from(W) / 2.0, f64::from(H) / 2.0);
    let half_diag = cx.hypot(cy);
    let mut px = Vec::new();
    for y in 0..H {
        for x in 0..W {
            let (xd, yd) = ((f64::from(x) + 0.5 - cx) / half_diag, (f64::from(y) + 0.5 - cy) / half_diag);
            let (mut xu, mut yu) = (xd, yd);
            for _ in 0..200 {
                let r2 = xu * xu + yu * yu;
                let radial = 1.0 + k1 * r2 + k2 * r2 * r2;
                let tx = 2.0 * p1 * xu * yu + p2 * (r2 + 2.0 * xu * xu);
                let ty = p1 * (r2 + 2.0 * yu * yu) + 2.0 * p2 * xu * yu;
                xu = (xd - tx) / radial;
                yu = (yd - ty) / radial;
            }
            let dark = lines_x.iter().any(|&lx| (cx + xu * half_diag - lx).abs() < 0.75);
            px.push(if dark { 0 } else { 255 });
        }
    }
    SourceImage::from_gray8(W, H, px).unwrap()
}

fn recipe(lens: Option<LensDistortion>) -> ImageRecipe {
    ImageRecipe {
        scale: Some(PixelScale::from_mm_per_pixel(0.1, 0.1).unwrap()),
        threshold: Threshold::Fixed(127),
        lens,
        ..ImageRecipe::default()
    }
}

/// For each row in `rows`, the mean x of the dark pixels within 20 px of `near_x`.
fn line_positions(photo: &SourceImage, lens: Option<LensDistortion>, near_x: f64) -> Vec<f64> {
    let p = prepare(photo, &recipe(lens), &CancellationToken::never()).unwrap();
    (5..145u32)
        .map(|y| {
            let xs: Vec<f64> = (0..p.width())
                .filter(|&x| p.mask.get(x.into(), y.into()) && (f64::from(x) + 0.5 - near_x).abs() < 20.0)
                .map(|x| f64::from(x) + 0.5)
                .collect();
            assert!(!xs.is_empty(), "no line near x = {near_x} in row {y}");
            xs.iter().sum::<f64>() / xs.len() as f64
        })
        .collect()
}

fn spread(v: &[f64]) -> f64 {
    v.iter().copied().fold(f64::MIN, f64::max) - v.iter().copied().fold(f64::MAX, f64::min)
}

#[test]
fn distort_follows_the_tangential_formula_by_hand() {
    // 200 × 100 image: centre (100, 50), half diagonal √12500. The point x = 0.5, y = 0 in
    // normalised units has r² = 0.25. With k1 = k2 = 0, p1 = 0.02, p2 = 0.01:
    //   x_d = x + 2 p1 x y + p2 (r² + 2x²) = 0.5 + 0 + 0.01 · 0.75 = 0.5075
    //   y_d = y + p1 (r² + 2y²) + 2 p2 x y = 0 + 0.02 · 0.25 + 0   = 0.005
    let lens = LensDistortion { k1: 0.0, k2: 0.0, p1: 0.02, p2: 0.01 };
    let hd = 100f64.hypot(50.0);
    let q = lens.distort(200, 100, [100.0 + 0.5 * hd, 50.0]);
    assert!((q[0] - (100.0 + 0.5075 * hd)).abs() < 1e-9, "x = {}", q[0]);
    assert!((q[1] - (50.0 + 0.005 * hd)).abs() < 1e-9, "y = {}", q[1]);
    // A point off both axes, radial and tangential together (x = 0.4, y = 0.3, r² = 0.25):
    let both = LensDistortion { k1: -0.1, k2: 0.0, p1: 0.02, p2: 0.01 };
    let q = both.distort(200, 100, [100.0 + 0.4 * hd, 50.0 + 0.3 * hd]);
    let radial = 1.0 - 0.1 * 0.25;
    let xd = 0.4 * radial + 2.0 * 0.02 * 0.12 + 0.01 * (0.25 + 2.0 * 0.16);
    let yd = 0.3 * radial + 0.02 * (0.25 + 2.0 * 0.09) + 2.0 * 0.01 * 0.12;
    assert!((q[0] - (100.0 + xd * hd)).abs() < 1e-9 && (q[1] - (50.0 + yd * hd)).abs() < 1e-9, "{q:?}");
}

#[test]
fn without_a_correction_a_decentred_lens_bows_a_vertical_line() {
    let img = photo(&[40.5], (0.0, 0.0, 0.03, -0.06));
    let xs = line_positions(&img, None, 40.0);
    assert!(spread(&xs) > 3.0, "the fixture must really be bent, got {}", spread(&xs));
}

#[test]
fn the_matching_tangential_terms_make_lines_straight_again() {
    let model = (-0.05, 0.0, 0.03, -0.06);
    let img = photo(&[40.5, 160.5], model);
    let lens = Some(LensDistortion { k1: model.0, k2: model.1, p1: model.2, p2: model.3 });
    for near in [40.0, 160.0] {
        let xs = line_positions(&img, lens, near);
        assert!(spread(&xs) < 1.5, "line near {near} still bowed by {}", spread(&xs));
    }
    // The radial terms alone leave most of the bow.
    let radial_only = Some(LensDistortion::radial(model.0, model.1));
    assert!(spread(&line_positions(&img, radial_only, 40.0)) > 3.0);
}

#[test]
fn a_purely_radial_lens_keeps_its_recipe_key_and_tangential_terms_change_it() {
    // The encoding written before the tangential terms existed.
    let mut old = CanonicalWriter::new();
    old.tag("lens-v1");
    old.f64(-0.1);
    old.f64(0.02);
    let mut now = CanonicalWriter::new();
    LensDistortion::radial(-0.1, 0.02).encode(&mut now);
    assert_eq!(now.finish().unwrap(), old.finish().unwrap(), "a radial lens must encode exactly as before");

    let key = |lens| {
        let mut w = CanonicalWriter::new();
        recipe(Some(lens)).encode(&mut w);
        w.finish().unwrap()
    };
    let a = key(LensDistortion::radial(-0.1, 0.02));
    let b = key(LensDistortion { k1: -0.1, k2: 0.02, p1: 0.001, p2: 0.0 });
    let c = key(LensDistortion { k1: -0.1, k2: 0.02, p1: 0.0, p2: 0.001 });
    assert!(a != b && a != c && b != c);
}

#[test]
fn tangential_terms_that_are_not_a_lens_are_refused() {
    let img = photo(&[40.5], (0.0, 0.0, 0.0, 0.0));
    for lens in [
        LensDistortion { k1: 0.0, k2: 0.0, p1: f64::NAN, p2: 0.0 },
        LensDistortion { k1: 0.0, k2: 0.0, p1: 0.0, p2: f64::INFINITY },
        LensDistortion { k1: 0.0, k2: 0.0, p1: 1.5, p2: 0.0 },
        // The mapping folds over inside the image: d x_d / d x = 1 + 6 p2 x < 0 near x = 0.5.
        LensDistortion { k1: 0.0, k2: 0.0, p1: 0.0, p2: -0.9 },
    ] {
        let err = prepare(&img, &recipe(Some(lens)), &CancellationToken::never()).unwrap_err();
        assert_eq!(err, ImageError::InvalidLens, "{lens:?}");
    }
    let fine = LensDistortion { k1: -0.05, k2: 0.0, p1: 0.03, p2: -0.06 };
    assert!(prepare(&img, &recipe(Some(fine)), &CancellationToken::never()).is_ok());
}
