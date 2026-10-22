//! Radial lens correction (G7-trace, T03). The oracle is geometry: a photo is rendered from a scene
//! of straight lines through the forward lens model, solved for each photo pixel with bisection in
//! the test (not with the code under test); after the correction the lines must be straight again.

#![allow(clippy::unwrap_used)]

use burnburn_image::{ImageError, ImageRecipe, LensDistortion, PixelScale, SourceImage, Threshold, prepare};
use burnburn_types::{CancellationToken, CanonicalEncode, CanonicalWriter};

const W: u32 = 201;
const H: u32 = 151;

/// Photo of a scene of vertical black lines at `lines_x` (undistorted pixel columns, 1.5 px wide),
/// seen through a lens with `k1`, `k2`: for each photo pixel the undistorted position is found by
/// bisection on the monotonic radial law.
fn photo(lines_x: &[f64], k1: f64, k2: f64) -> SourceImage {
    let (cx, cy) = (f64::from(W) / 2.0, f64::from(H) / 2.0);
    let half_diag = cx.hypot(cy);
    let mut px = Vec::new();
    for y in 0..H {
        for x in 0..W {
            let (dx, dy) = ((f64::from(x) + 0.5 - cx) / half_diag, (f64::from(y) + 0.5 - cy) / half_diag);
            let rd = dx.hypot(dy);
            let law = |ru: f64| ru * (1.0 + k1 * ru * ru + k2 * ru.powi(4));
            let (mut lo, mut hi) = (0.0f64, 2.0f64);
            for _ in 0..60 {
                let mid = (lo + hi) / 2.0;
                if law(mid) < rd { lo = mid } else { hi = mid }
            }
            let ru = (lo + hi) / 2.0;
            let s = if rd > 0.0 { ru / rd } else { 1.0 };
            let xu = cx + dx * s * half_diag;
            let dark = lines_x.iter().any(|&lx| (xu - lx).abs() < 0.75);
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

/// For each row in `rows`, the mean x of the dark pixels within `±window` of `near_x`.
fn line_positions(
    photo: &SourceImage,
    lens: Option<LensDistortion>,
    near_x: f64,
    rows: std::ops::Range<u32>,
) -> Vec<f64> {
    let p = prepare(photo, &recipe(lens), &CancellationToken::never()).unwrap();
    rows.map(|y| {
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
fn without_a_correction_a_barrel_distorted_line_is_bowed() {
    let img = photo(&[40.5], -0.15, 0.0);
    let xs = line_positions(&img, None, 40.0, 5..145);
    assert!(spread(&xs) > 3.0, "the fixture must really be bent, got {}", spread(&xs));
}

#[test]
fn the_matching_coefficients_make_barrel_distorted_lines_straight_again() {
    let img = photo(&[40.5, 160.5], -0.15, 0.0);
    let lens = Some(LensDistortion { k1: -0.15, k2: 0.0, p1: 0.0, p2: 0.0 });
    for near in [40.0, 160.0] {
        let xs = line_positions(&img, lens, near, 5..145);
        assert!(spread(&xs) < 1.5, "line near {near} still bowed by {}", spread(&xs));
    }
}

#[test]
fn pincushion_and_the_second_coefficient_are_corrected_too() {
    let img = photo(&[40.5], 0.12, -0.05);
    let xs = line_positions(&img, Some(LensDistortion { k1: 0.12, k2: -0.05, p1: 0.0, p2: 0.0 }), 40.0, 5..145);
    assert!(spread(&xs) < 1.5, "bowed by {}", spread(&xs));
}

#[test]
fn zero_coefficients_leave_the_image_as_it_is() {
    let img = photo(&[40.5, 100.5], -0.15, 0.0);
    let plain = prepare(&img, &recipe(None), &CancellationToken::never()).unwrap();
    let zero = prepare(
        &img,
        &recipe(Some(LensDistortion { k1: 0.0, k2: 0.0, p1: 0.0, p2: 0.0 })),
        &CancellationToken::never(),
    )
    .unwrap();
    assert_eq!(plain.mask, zero.mask);
}

#[test]
fn the_correction_is_part_of_the_recipe_key() {
    let key = |lens| {
        let mut w = CanonicalWriter::new();
        recipe(lens).encode(&mut w);
        w.finish().unwrap()
    };
    assert_ne!(key(None), key(Some(LensDistortion { k1: -0.1, k2: 0.0, p1: 0.0, p2: 0.0 })));
    assert_eq!(key(None), key(None));
}

#[test]
fn coefficients_that_are_not_a_lens_are_refused() {
    let img = photo(&[40.5], 0.0, 0.0);
    for lens in [
        LensDistortion { k1: f64::NAN, k2: 0.0, p1: 0.0, p2: 0.0 },
        LensDistortion { k1: 2.0, k2: 0.0, p1: 0.0, p2: 0.0 },
        // d r_d / d r_u = 1 + 3 k1 r² turns negative inside the image: the mapping folds over.
        LensDistortion { k1: -0.5, k2: 0.0, p1: 0.0, p2: 0.0 },
    ] {
        let err = prepare(&img, &recipe(Some(lens)), &CancellationToken::never()).unwrap_err();
        assert_eq!(err, ImageError::InvalidLens, "{lens:?}");
    }
}
