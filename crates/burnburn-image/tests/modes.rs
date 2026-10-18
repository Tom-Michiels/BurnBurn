//! W1-L5a image modes and adjustments. Expected pixel patterns come from the independent Python
//! reference in `docs/dev-notes/IM18-raster.md` (own transcription of each kernel from its
//! published definition; numeric area integration for the halftone spot function), or are
//! derived by hand in the comments. They do not call back into the implementation.

#![allow(clippy::unwrap_used)]

use burnburn_image::{
    AlphaPolicy, BitImage, DecodeLimits, DitherAlgorithm, EngraveMode, EngraveParams, Engraved, GrayConversion,
    GrayImage, PixelRect, RasterError, Sharpen, SourceImage, ToneCurve, decode, dither, encode_png_gray8, engrave,
    halftone, resample_area_with, resample_nearest, sketch, unsharp,
};
use burnburn_types::CancellationToken;
use proptest::prelude::*;

fn never() -> CancellationToken {
    CancellationToken::never()
}

fn gray(rows: &[&[u8]]) -> GrayImage {
    let h = rows.len() as u32;
    let w = rows[0].len() as u32;
    GrayImage { width: w, height: h, values: rows.iter().flat_map(|r| r.iter().map(|&v| f32::from(v))).collect() }
}

fn constant(w: u32, h: u32, v: u8) -> GrayImage {
    GrayImage { width: w, height: h, values: vec![f32::from(v); (w * h) as usize] }
}

/// `#` = burn, `.` = white, one string per row.
fn art(b: &BitImage) -> Vec<String> {
    (0..b.height()).map(|y| (0..b.width()).map(|x| if b.get(x, y) { '#' } else { '.' }).collect()).collect()
}

fn bits_of(e: Engraved) -> BitImage {
    match e {
        Engraved::Bits(b) => b,
        Engraved::Levels { .. } => BitImage::new(0, 0, Vec::new()).unwrap(),
    }
}

fn params(mode: EngraveMode) -> EngraveParams {
    EngraveParams { tone: ToneCurve::IDENTITY, sharpen: None, negative: false, mode, serpentine: false }
}

// ---- error-diffusion kernels: golden images -------------------------------------------------

fn gradient_8x4() -> GrayImage {
    // v(x, y) = 30 + 27 x + 10 y
    let rows: Vec<Vec<u8>> = (0..4).map(|y| (0..8).map(|x| 30 + 27 * x + 10 * y).collect()).collect();
    let refs: Vec<&[u8]> = rows.iter().map(Vec::as_slice).collect();
    gray(&refs)
}

fn diffused(algorithm: DitherAlgorithm) -> Vec<String> {
    art(&dither(&gradient_8x4(), &ToneCurve::IDENTITY, algorithm, false, &never()).unwrap())
}

#[test]
fn floyd_steinberg_gradient_matches_independent_reference() {
    assert_eq!(diffused(DitherAlgorithm::FloydSteinberg), ["###.#...", "#.#.#...", "##.#..#.", "##.#...."]);
}

#[test]
fn jarvis_judice_ninke_gradient_matches_independent_reference() {
    assert_eq!(diffused(DitherAlgorithm::Jarvis), ["###.....", "####.#..", "#..#....", "##..#..."]);
}

#[test]
fn stucki_gradient_matches_independent_reference() {
    assert_eq!(diffused(DitherAlgorithm::Stucki), ["###.#...", "##.#....", "##.#....", "#.#.#..."]);
}

#[test]
fn atkinson_gradient_matches_independent_reference() {
    assert_eq!(diffused(DitherAlgorithm::Atkinson), ["###.....", "####....", "##..##..", "##......"]);
}

#[test]
fn ordered_bayer_4_matches_the_textbook_matrix() {
    // Standard 4×4 Bayer matrix [[0,8,2,10],[12,4,14,6],[3,11,1,9],[15,7,13,5]]; burn when
    // (v + ½)/256 < (M + ½)/16. Input v(x, y) = (16 x + 64 y) mod 256.
    let rows: Vec<Vec<u8>> = (0..4).map(|y| (0..8).map(|x| ((16 * x + 64 * y) % 256) as u8).collect()).collect();
    let refs: Vec<&[u8]> = rows.iter().map(Vec::as_slice).collect();
    let b = dither(&gray(&refs), &ToneCurve::IDENTITY, DitherAlgorithm::Ordered { size: 4 }, false, &never()).unwrap();
    assert_eq!(art(&b), ["####.#.#", "#.#.#.#.", ".#......", "#...####"]);
}

// ---- halftone / newsprint -------------------------------------------------------------------

#[test]
fn halftone_light_gray_grows_a_small_dot_in_the_cell_centre() {
    // cell 4, angle 0, gray 191 (darkness 0.251): the four pixels around each cell centre have
    // r = 0.177 cell, disc area π r² = 0.098 ≤ 0.251 → burn; the next ring has r = 0.395,
    // area 0.491 > 0.251 → white. Hand-derived; the numeric reference agrees.
    let b = halftone(&constant(8, 8, 191), 4.0, 0.0, &never()).unwrap();
    assert_eq!(
        art(&b),
        ["........", ".##..##.", ".##..##.", "........", "........", ".##..##.", ".##..##.", "........"]
    );
}

#[test]
fn halftone_mid_gray_dots_touch_into_a_checker_of_discs() {
    let b = halftone(&constant(8, 8, 128), 4.0, 0.0, &never()).unwrap();
    assert_eq!(
        art(&b),
        [".##..##.", "########", "########", ".##..##.", ".##..##.", "########", "########", ".##..##."]
    );
}

#[test]
fn halftone_screen_angle_rotates_the_dot_lattice() {
    let at_45 = halftone(&constant(8, 8, 128), 4.0, 45.0, &never()).unwrap();
    assert_eq!(
        art(&at_45),
        [".###...#", "#.#..##.", "##..###.", "#...###.", "..##...#", ".###...#", ".###...#", "#...###."]
    );
    let at_30 = halftone(&constant(8, 8, 100), 4.0, 30.0, &never()).unwrap();
    assert_eq!(
        art(&at_30),
        [".###....", "####...#", "#...#.##", "#..###.#", ".#.###..", "###.##..", "###..###", "##...###"]
    );
}

#[test]
fn halftone_extremes_are_blank_and_solid_and_bad_cells_are_rejected() {
    assert_eq!(halftone(&constant(8, 8, 255), 4.0, 0.0, &never()).unwrap().count_on(), 0);
    assert_eq!(halftone(&constant(8, 8, 0), 4.0, 17.0, &never()).unwrap().count_on(), 64);
    assert_eq!(
        halftone(&constant(8, 8, 0), 0.0, 0.0, &never()).unwrap_err(),
        RasterError::InvalidTone { field: "cell" }
    );
    assert_eq!(
        halftone(&constant(8, 8, 0), 4.0, f64::NAN, &never()).unwrap_err(),
        RasterError::InvalidTone { field: "angle" }
    );
}

proptest! {
    #[test]
    fn halftone_burn_fraction_tracks_darkness(v in 0u8..=255, angle in 0.0f64..90.0) {
        let b = halftone(&constant(64, 64, v), 16.0, angle, &never()).unwrap();
        let darkness = 1.0 - f64::from(v) / 255.0;
        let fraction = b.count_on() as f64 / (64.0 * 64.0);
        prop_assert!((fraction - darkness).abs() < 0.08, "v={v} angle={angle}: {fraction} vs {darkness}");
    }
}

// ---- sketch ---------------------------------------------------------------------------------

#[test]
fn sketch_burns_a_two_pixel_wide_line_on_a_vertical_step_edge() {
    // Sobel gx = 4·255 = 1020 on both pixels beside the edge → m = 255 ≥ 128; flat parts m = 0.
    let img = gray(&[&[0u8, 0, 0, 255, 255, 255] as &[u8]; 5]);
    assert_eq!(art(&sketch(&img, 128, &never()).unwrap()), ["..##..", "..##..", "..##..", "..##..", "..##.."]);
}

#[test]
fn sketch_level_decides_which_edges_survive() {
    let step = gray(&[&[255; 6], &[255; 6], &[255; 6], &[0; 6], &[0; 6], &[0; 6]]);
    assert_eq!(
        art(&sketch(&step, 200, &never()).unwrap()),
        ["......", "......", "######", "######", "......", "......"]
    );
    // A level above the strongest possible response burns nothing but a 255 step at level 255.
    assert_eq!(sketch(&step, 255, &never()).unwrap().count_on(), 12);
    let weak = gray(&[&[100; 4], &[100; 4], &[140; 4], &[140; 4]]);
    // |gy| = 4·40 = 160 → m = 40: below 100, above 40.
    assert_eq!(sketch(&weak, 100, &never()).unwrap().count_on(), 0);
    assert_eq!(sketch(&weak, 40, &never()).unwrap().count_on(), 8);
}

#[test]
fn sketch_follows_a_diagonal_edge() {
    let diag: Vec<Vec<u8>> = (0..6).map(|y| (0..6).map(|x| if x > y { 255 } else { 0 }).collect()).collect();
    let refs: Vec<&[u8]> = diag.iter().map(Vec::as_slice).collect();
    assert_eq!(
        art(&sketch(&gray(&refs), 100, &never()).unwrap()),
        ["##....", ".##...", "..##..", "...##.", "....##", ".....#"]
    );
}

// ---- unsharp --------------------------------------------------------------------------------

#[test]
fn unsharp_boosts_a_peak_and_clips_the_undershoot() {
    // Box blur radius 1 of [0,0,90,0,0] at the peak = 30; v + amount·(v − blur) = 90 + 60 = 150;
    // neighbours: blur 30 → 0 − 30 clips to 0.
    let out = unsharp(&gray(&[&[0, 0, 90, 0, 0]]), 1, 1.0, &never()).unwrap();
    assert_eq!(out.values, vec![0.0, 0.0, 150.0, 0.0, 0.0]);
}

#[test]
fn unsharp_steepens_an_edge_by_the_given_amount() {
    // [0,0,0,200,200,200], r=1 (borders replicate), amount ½:
    // x=2: blur (0+0+200)/3 → 0 − 33.3 → 0; x=3: blur (0+200+200)/3 = 133.33 → 200 + 33.33 = 233.
    let out = unsharp(&gray(&[&[0, 0, 0, 200, 200, 200]]), 1, 0.5, &never()).unwrap();
    assert_eq!(out.values, vec![0.0, 0.0, 0.0, 233.0, 200.0, 200.0]);
}

#[test]
fn unsharp_with_zero_amount_or_radius_is_the_identity() {
    let img = gradient_8x4();
    assert_eq!(unsharp(&img, 3, 0.0, &never()).unwrap(), img);
    assert_eq!(unsharp(&img, 0, 2.0, &never()).unwrap(), img);
    assert_eq!(unsharp(&img, 1, -1.0, &never()).unwrap_err(), RasterError::InvalidTone { field: "amount" });
}

proptest! {
    #[test]
    fn unsharp_leaves_flat_images_alone_and_stays_in_range(v in 0u8..=255, r in 1u32..4, amount in 0.0f64..5.0) {
        let flat = constant(7, 5, v);
        prop_assert_eq!(unsharp(&flat, r, amount, &never()).unwrap(), flat);
        let noisy = gradient_8x4();
        let out = unsharp(&noisy, r, amount, &never()).unwrap();
        prop_assert!(out.values.iter().all(|&p| (0.0..=255.0).contains(&p) && p.fract() == 0.0));
    }
}

// ---- colour to gray and crop ----------------------------------------------------------------

fn one_rgba(r: u8, g: u8, b: u8, a: u8) -> SourceImage {
    SourceImage::from_rgba8(1, 1, vec![r, g, b, a]).unwrap()
}

fn converted(src: &SourceImage, c: GrayConversion) -> f32 {
    resample_area_with(src, None, AlphaPolicy::default(), c, 1, 1, &never()).unwrap().values[0]
}

#[test]
fn colour_to_gray_conversions_follow_their_definitions() {
    let px = one_rgba(200, 100, 50, 255);
    // 709: 42.52 + 71.52 + 3.61 = 117.65 → 118;  601: 59.8 + 58.7 + 5.7 = 124.2 → 124;
    // mean 116.67 → 117;  lightness (200 + 50)/2 = 125.
    assert_eq!(converted(&px, GrayConversion::Rec709), 118.0);
    assert_eq!(converted(&px, GrayConversion::Rec601), 124.0);
    assert_eq!(converted(&px, GrayConversion::Average), 117.0);
    assert_eq!(converted(&px, GrayConversion::Lightness), 125.0);
    assert_eq!(converted(&px, GrayConversion::Red), 200.0);
    assert_eq!(converted(&px, GrayConversion::Green), 100.0);
    assert_eq!(converted(&px, GrayConversion::Blue), 50.0);
}

#[test]
fn transparent_pixels_composite_over_paper_after_the_colour_conversion() {
    // (200, 0, 0) at alpha 128 over white, a = 128/255:
    // red  200 a + 255 (1 − a) = 227.4 → 227;  green 0 + 127 = 127;
    // 709  42.52 a + 127 = 148.3 → 148;  lightness (200 + 0)/2 = 100 → 100 a + 127 = 177.2 → 177.
    let px = one_rgba(200, 0, 0, 128);
    assert_eq!(converted(&px, GrayConversion::Red), 227.0);
    assert_eq!(converted(&px, GrayConversion::Green), 127.0);
    assert_eq!(converted(&px, GrayConversion::Rec709), 148.0);
    assert_eq!(converted(&px, GrayConversion::Lightness), 177.0);
}

#[test]
fn crop_resamples_only_the_window() {
    let src = SourceImage::from_gray8(4, 1, vec![10, 20, 30, 40]).unwrap();
    let crop = Some(PixelRect { x: 1, y: 0, width: 2, height: 1 });
    let g = resample_area_with(&src, crop, AlphaPolicy::default(), GrayConversion::Rec709, 2, 1, &never()).unwrap();
    assert_eq!(g.values, vec![20.0, 30.0]);
    // 2 → 1 averages the window, not the whole row: (20 + 30)/2 = 25.
    let one = resample_area_with(&src, crop, AlphaPolicy::default(), GrayConversion::Rec709, 1, 1, &never()).unwrap();
    assert_eq!(one.values, vec![25.0]);
}

#[test]
fn crop_outside_the_image_or_without_area_is_an_error() {
    let src = SourceImage::from_gray8(4, 1, vec![10, 20, 30, 40]).unwrap();
    for rect in [
        PixelRect { x: 3, y: 0, width: 2, height: 1 },
        PixelRect { x: 0, y: 0, width: 0, height: 1 },
        PixelRect { x: 0, y: 1, width: 1, height: 1 },
        PixelRect { x: u32::MAX, y: 0, width: 2, height: 1 },
    ] {
        let err = resample_area_with(&src, Some(rect), AlphaPolicy::default(), GrayConversion::Rec709, 1, 1, &never());
        assert_eq!(err.unwrap_err(), RasterError::CropOutOfBounds, "{rect:?}");
    }
}

#[test]
fn nearest_resampling_replicates_and_skips_without_filtering() {
    let src = SourceImage::from_gray8(4, 1, vec![10, 20, 30, 40]).unwrap();
    let go = |crop, w, h| {
        resample_nearest(&src, crop, AlphaPolicy::default(), GrayConversion::Rec709, w, h, &never()).unwrap().values
    };
    // 4 → 2: centres of the output pixels fall in source pixels ⌊1·4/4⌋ = 1 and ⌊3·4/4⌋ = 3.
    assert_eq!(go(None, 2, 1), vec![20.0, 40.0]);
    // 4 → 8 replicates each pixel twice; 4 → 4 is the identity.
    assert_eq!(go(None, 8, 1), vec![10.0, 10.0, 20.0, 20.0, 30.0, 30.0, 40.0, 40.0]);
    assert_eq!(go(None, 4, 1), vec![10.0, 20.0, 30.0, 40.0]);
    // 4 → 3 : ⌊1·4/6⌋ = 0, ⌊3·4/6⌋ = 2, ⌊5·4/6⌋ = 3.
    assert_eq!(go(None, 3, 1), vec![10.0, 30.0, 40.0]);
    // Rows work the same way, and a crop limits the choice to the window.
    assert_eq!(
        go(Some(PixelRect { x: 1, y: 0, width: 2, height: 1 }), 4, 2),
        vec![20.0, 20.0, 30.0, 30.0, 20.0, 20.0, 30.0, 30.0]
    );
    let bad = resample_nearest(
        &src,
        Some(PixelRect { x: 3, y: 0, width: 2, height: 1 }),
        AlphaPolicy::default(),
        GrayConversion::Rec709,
        1,
        1,
        &never(),
    );
    assert_eq!(bad.unwrap_err(), RasterError::CropOutOfBounds);
}

// ---- engrave pipeline: order, negative, pass-through, grayscale, processed bitmap ------------

fn threshold(level: u8) -> EngraveMode {
    EngraveMode::Dither(DitherAlgorithm::Threshold { level })
}

#[test]
fn negative_image_swaps_burn_and_white() {
    let img = gray(&[&[0, 255]]);
    let normal = bits_of(engrave(&img, &params(threshold(128)), &never()).unwrap());
    assert_eq!(art(&normal), ["#."]);
    let negative =
        bits_of(engrave(&img, &EngraveParams { negative: true, ..params(threshold(128)) }, &never()).unwrap());
    assert_eq!(art(&negative), [".#"]);
}

#[test]
fn tone_is_applied_before_the_negative() {
    // 100 + brightness 0.2 → 151 (100/255 + 0.2 = 0.592 → 151); negative → 104 < 128 burns.
    // The opposite order would give 155 → +0.2 → 206 and stay white.
    let tone = ToneCurve::new(1.0, 0.2, 1.0).unwrap();
    let p = EngraveParams { tone, negative: true, ..params(threshold(128)) };
    assert_eq!(art(&bits_of(engrave(&gray(&[&[100]]), &p, &never()).unwrap())), ["#"]);
}

#[test]
fn sharpening_comes_before_tone_and_dither() {
    // After the unsharp of the peak example the centre is 150 (white at level 128), so a plain
    // threshold sees [0,0,150,0,0] → burn, burn, white, burn, burn; without it 90 would burn.
    let img = gray(&[&[0, 0, 90, 0, 0]]);
    let plain = bits_of(engrave(&img, &params(threshold(128)), &never()).unwrap());
    assert_eq!(art(&plain), ["#####"]);
    let sharp = EngraveParams { sharpen: Some(Sharpen { radius_px: 1, amount: 1.0 }), ..params(threshold(128)) };
    assert_eq!(art(&bits_of(engrave(&img, &sharp, &never()).unwrap())), ["##.##"]);
}

#[test]
fn engrave_with_a_dither_equals_the_plain_dither_of_the_toned_image() {
    let img = gradient_8x4();
    let tone = ToneCurve::new(1.4, 0.1, 1.2).unwrap();
    for algorithm in [DitherAlgorithm::FloydSteinberg, DitherAlgorithm::Stucki, DitherAlgorithm::Ordered { size: 8 }] {
        let direct = dither(&img, &tone, algorithm, true, &never()).unwrap();
        let p = EngraveParams { tone, serpentine: true, ..params(EngraveMode::Dither(algorithm)) };
        assert_eq!(bits_of(engrave(&img, &p, &never()).unwrap()), direct, "{algorithm:?}");
    }
}

#[test]
fn pass_through_uses_the_pixels_unchanged() {
    // Tone, sharpen and negative are ignored: the bitmap is thresholded at 128 as it is.
    let img = gray(&[&[0, 127, 128, 255]]);
    let p = EngraveParams {
        tone: ToneCurve::new(3.0, 0.5, 2.0).unwrap(),
        sharpen: Some(Sharpen { radius_px: 1, amount: 3.0 }),
        negative: true,
        ..params(EngraveMode::PassThrough)
    };
    assert_eq!(art(&bits_of(engrave(&img, &p, &never()).unwrap())), ["##.."]);
}

#[test]
fn grayscale_keeps_levels_and_the_negative_inverts_them() {
    let img = gray(&[&[0, 128, 255]]);
    let e = engrave(&img, &params(EngraveMode::Grayscale), &never()).unwrap();
    assert_eq!(e, Engraved::Levels { width: 3, height: 1, levels: vec![0, 128, 255] });
    let n = engrave(&img, &EngraveParams { negative: true, ..params(EngraveMode::Grayscale) }, &never()).unwrap();
    assert_eq!(n, Engraved::Levels { width: 3, height: 1, levels: vec![255, 127, 0] });
}

#[test]
fn halftone_and_newsprint_and_sketch_modes_reach_the_kernels() {
    let flat = constant(8, 8, 128);
    let half =
        bits_of(engrave(&flat, &params(EngraveMode::Halftone { cell_px: 4.0, angle_deg: 0.0 }), &never()).unwrap());
    assert_eq!(half, halftone(&flat, 4.0, 0.0, &never()).unwrap());
    // Newsprint is the 45° screen.
    let news = bits_of(engrave(&flat, &params(EngraveMode::Newsprint { cell_px: 4.0 }), &never()).unwrap());
    assert_eq!(news, halftone(&flat, 4.0, 45.0, &never()).unwrap());
    let edges = gray(&[&[0u8, 0, 255, 255] as &[u8]; 3]);
    let s = bits_of(engrave(&edges, &params(EngraveMode::Sketch { level: 128 }), &never()).unwrap());
    assert_eq!(s, sketch(&edges, 128, &never()).unwrap());
}

#[test]
fn processed_bitmap_png_holds_exactly_the_engraved_pixels() {
    let img = gradient_8x4();
    let e = engrave(&img, &params(EngraveMode::Dither(DitherAlgorithm::Atkinson)), &never()).unwrap();
    let gray8 = e.to_gray8();
    // Burn = 0, white = 255, row-major, top row first.
    let expected: Vec<u8> = ["###.....", "####....", "##..##..", "##......"]
        .iter()
        .flat_map(|r| r.bytes().map(|c| if c == b'#' { 0 } else { 255 }))
        .collect();
    assert_eq!(gray8, expected);
    let png = encode_png_gray8(e.width(), e.height(), &gray8).unwrap();
    let back = decode(&png, DecodeLimits::default()).unwrap();
    assert_eq!((back.width(), back.height()), (8, 4));
    assert_eq!(back.samples(), &expected[..]);
}

#[test]
fn processed_grayscale_png_keeps_the_power_levels() {
    let e = engrave(&gray(&[&[0, 64], &[128, 255]]), &params(EngraveMode::Grayscale), &never()).unwrap();
    assert_eq!(e.to_gray8(), vec![0, 64, 128, 255]);
    let png = encode_png_gray8(2, 2, &e.to_gray8()).unwrap();
    assert_eq!(decode(&png, DecodeLimits::default()).unwrap().samples(), &[0, 64, 128, 255]);
    assert!(encode_png_gray8(2, 2, &[0, 1, 2]).is_err(), "buffer length must match the size");
}
