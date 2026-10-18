//! IM18 raster production: exact area resampling, tone tables and deterministic dithering.
//! Expected values are hand-computed (comments) or analytic bounds, not the implementation.

#![allow(clippy::unwrap_used)]

use burnburn_image::{
    AlphaPolicy, BitImage, DitherAlgorithm, GrayImage, RasterError, SourceImage, ToneCurve, dither, resample_area,
};
use burnburn_types::CancellationToken;
use proptest::prelude::*;

fn never() -> CancellationToken {
    CancellationToken::never()
}

fn row(values: &[u8]) -> SourceImage {
    SourceImage::from_gray8(values.len() as u32, 1, values.to_vec()).unwrap()
}

fn constant(w: u32, h: u32, v: f32) -> GrayImage {
    GrayImage { width: w, height: h, values: vec![v; (w * h) as usize] }
}

fn bits(b: &BitImage) -> Vec<bool> {
    b.bits().to_vec()
}

#[test]
fn resampling_averages_source_pixel_areas_exactly() {
    let src = row(&[0, 100, 200, 255]);
    let alpha = AlphaPolicy::default();
    // 4 → 2: pairs (0+100)/2 = 50 and (200+255)/2 = 227.5 → 228 (half up).
    assert_eq!(resample_area(&src, alpha, 2, 1, &never()).unwrap().values, vec![50.0, 228.0]);
    // 4 → 3: each output pixel covers 4/3 source pixels:
    // (3·0 + 1·100)/4 = 25, (2·100 + 2·200)/4 = 150, (1·200 + 3·255)/4 = 241.25 → 241.
    assert_eq!(resample_area(&src, alpha, 3, 1, &never()).unwrap().values, vec![25.0, 150.0, 241.0]);
    // Upsampling a single pixel copies it.
    let one = row(&[128]);
    assert_eq!(resample_area(&one, alpha, 3, 2, &never()).unwrap().values, vec![128.0; 6]);
    assert_eq!(
        resample_area(&one, alpha, 0, 2, &never()).unwrap_err(),
        RasterError::BadOutputSize { width: 0, height: 2 }
    );
}

#[test]
fn tone_tables_follow_the_documented_curve() {
    let id = ToneCurve::IDENTITY.table();
    assert!(id.iter().enumerate().all(|(i, &v)| usize::from(v) == i), "identity is exact");
    // gamma 2: (128/255)² · 255 = 64.25 → 64.
    assert_eq!(ToneCurve::new(2.0, 0.0, 1.0).unwrap().table()[128], 64);
    // brightness +0.5 lifts black to 0.5 · 255 = 127.5 → 128; white stays clipped at 255.
    let bright = ToneCurve::new(1.0, 0.5, 1.0).unwrap().table();
    assert_eq!((bright[0], bright[255]), (128, 255));
    // contrast 0 maps everything to mid gray.
    assert!(ToneCurve::new(1.0, 0.0, 0.0).unwrap().table().iter().all(|&v| v == 128));
    assert_eq!(ToneCurve::new(0.0, 0.0, 1.0).unwrap_err(), RasterError::InvalidTone { field: "gamma" });
    assert_eq!(ToneCurve::new(1.0, f64::NAN, 1.0).unwrap_err(), RasterError::InvalidTone { field: "brightness" });
    assert_eq!(ToneCurve::new(1.0, 0.0, 11.0).unwrap_err(), RasterError::InvalidTone { field: "contrast" });
}

#[test]
fn threshold_burns_below_level() {
    let img = GrayImage { width: 4, height: 1, values: vec![0.0, 127.0, 128.0, 255.0] };
    let b = dither(&img, &ToneCurve::IDENTITY, DitherAlgorithm::Threshold { level: 128 }, false, &never()).unwrap();
    assert_eq!(bits(&b), vec![true, true, false, false]);
}

#[test]
fn floyd_steinberg_matches_hand_computed_row() {
    // 128 → white, error −127; next: 128 + trunc(−127·7/16 = −55.6) = 73 → burn, error 73;
    // next: 128 + trunc(73·7/16 = 31.9) = 159 → white, error −96; last: 128 − 42 = 86 → burn.
    let b =
        dither(&constant(4, 1, 128.0), &ToneCurve::IDENTITY, DitherAlgorithm::FloydSteinberg, false, &never()).unwrap();
    assert_eq!(bits(&b), vec![false, true, false, true]);
}

#[test]
fn serpentine_runs_odd_rows_right_to_left() {
    // Reference values from an independent Python transcription of Floyd–Steinberg with
    // truncating integer shares (see docs/dev-notes/IM18-raster.md).
    let img = GrayImage {
        width: 5,
        height: 2,
        values: [200u8, 90, 160, 60, 230, 100, 180, 50, 140, 210].map(f32::from).to_vec(),
    };
    let fs = |serpentine| {
        bits(&dither(&img, &ToneCurve::IDENTITY, DitherAlgorithm::FloydSteinberg, serpentine, &never()).unwrap())
    };
    let (f, t) = (false, true);
    assert_eq!(fs(false), vec![f, t, f, t, f, t, f, t, f, f]);
    assert_eq!(fs(true), vec![f, t, f, t, f, t, f, t, t, f]);
}

#[test]
fn ordered_dither_accepts_only_bayer_sizes() {
    let img = constant(4, 4, 100.0);
    let err = dither(&img, &ToneCurve::IDENTITY, DitherAlgorithm::Ordered { size: 3 }, false, &never()).unwrap_err();
    assert_eq!(err, RasterError::UnsupportedOrderedSize { size: 3 });
}

proptest! {
    /// A Bayer matrix holds every threshold once per tile, so a constant gray `v` burns
    /// `n²·(1 − (v + ½)/256)` dots per tile, up to one dot.
    #[test]
    fn ordered_dither_burn_fraction_tracks_the_gray_value(v in 0u8..=255, k in 0usize..3) {
        let n = [2u8, 4, 8][k];
        let img = constant(16, 16, f32::from(v));
        let b = dither(&img, &ToneCurve::IDENTITY, DitherAlgorithm::Ordered { size: n }, false, &never()).unwrap();
        let fraction = b.count_on() as f64 / 256.0;
        let expected = 1.0 - (f64::from(v) + 0.5) / 256.0;
        let n2 = f64::from(n) * f64::from(n);
        prop_assert!((fraction - expected).abs() <= 1.0 / n2 + 1e-12, "v={} n={} {} vs {}", v, n, fraction, expected);
    }

    /// Error diffusion keeps the mean tone of a constant patch (Atkinson deliberately does not).
    #[test]
    fn error_diffusion_preserves_mean_tone(v in 0u8..=255, k in 0usize..3, serpentine in any::<bool>()) {
        let algo = [DitherAlgorithm::FloydSteinberg, DitherAlgorithm::Jarvis, DitherAlgorithm::Stucki][k];
        let img = constant(48, 48, f32::from(v));
        let b = dither(&img, &ToneCurve::IDENTITY, algo, serpentine, &never()).unwrap();
        let fraction = b.count_on() as f64 / (48.0 * 48.0);
        let expected = 1.0 - f64::from(v) / 255.0;
        prop_assert!((fraction - expected).abs() < 0.05, "{:?} v={} {} vs {}", algo, v, fraction, expected);
    }

    /// Resampling never leaves the range of the source values (box filter is a convex mix).
    #[test]
    fn resampling_stays_within_source_range(px in proptest::collection::vec(any::<u8>(), 1..24), ow in 1u32..30) {
        let src = row(&px);
        let out = resample_area(&src, AlphaPolicy::default(), ow, 1, &never()).unwrap();
        let (lo, hi) = (f32::from(*px.iter().min().unwrap()), f32::from(*px.iter().max().unwrap()));
        prop_assert!(out.values.iter().all(|&v| v >= lo && v <= hi && v.fract() == 0.0));
    }
}
