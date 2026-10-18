//! Public contract of the binarisation pipeline. Oracles are hand-built images with known answers.

#![allow(clippy::unwrap_used)]

use burnburn_image::{
    AlphaPolicy, GrayImage, ImageError, ImageRecipe, Morphology, PixelRect, PixelScale, PreparedImage, QuarterTurn,
    RegionMask, RegionMode, RegionShape, SourceImage, Threshold, median_filter, otsu_threshold, prepare, to_gray,
};
use burnburn_types::{CancellationSource, CancellationToken};
use proptest::prelude::*;

fn scaled(recipe: ImageRecipe) -> ImageRecipe {
    ImageRecipe { scale: Some(PixelScale::from_mm_per_pixel(0.1, 0.2).unwrap()), ..recipe }
}

fn run(img: &SourceImage, recipe: ImageRecipe) -> PreparedImage {
    prepare(img, &scaled(recipe), &CancellationToken::never()).unwrap()
}

fn mask_rows(p: &PreparedImage) -> Vec<String> {
    (0..p.height())
        .map(|y| (0..p.width()).map(|x| if p.mask.get(x.into(), y.into()) { '#' } else { '.' }).collect())
        .collect()
}

/// Parses rows of `#` (black, 0) and `.` (white, 255).
fn art(rows: &[&str]) -> SourceImage {
    let w = rows[0].len() as u32;
    let px = rows.iter().flat_map(|r| r.bytes().map(|b| if b == b'#' { 0 } else { 255 })).collect();
    SourceImage::from_gray8(w, rows.len() as u32, px).unwrap()
}

#[test]
fn crop_and_quarter_turns_map_pixels_and_swap_scale() {
    let img = art(&["#..", "...", "..."]); // black pixel at top-left
    let fixed = ImageRecipe { threshold: Threshold::Fixed(127), ..ImageRecipe::default() };
    let cw = run(&img, ImageRecipe { rotation: QuarterTurn::Clockwise90, ..fixed.clone() });
    assert_eq!(mask_rows(&cw), ["..#", "...", "..."], "clockwise: top-left goes to top-right");
    assert_eq!(cw.scale.mm_per_pixel_x(), 0.2, "x/y pixel pitch swap after a quarter turn");
    let ccw = run(&img, ImageRecipe { rotation: QuarterTurn::CounterClockwise90, ..fixed.clone() });
    assert_eq!(mask_rows(&ccw), ["...", "...", "#.."]);
    let half = run(&img, ImageRecipe { rotation: QuarterTurn::Half, ..fixed.clone() });
    assert_eq!(mask_rows(&half), ["...", "...", "..#"]);
    assert_eq!(half.scale.mm_per_pixel_x(), 0.1);

    let wide = art(&["....", ".#..", "...."]);
    let crop = PixelRect { x: 1, y: 1, width: 3, height: 2 };
    let cropped = run(&wide, ImageRecipe { crop: Some(crop), ..fixed.clone() });
    assert_eq!(mask_rows(&cropped), ["#..", "..."]);
    let both = run(&wide, ImageRecipe { crop: Some(crop), rotation: QuarterTurn::Clockwise90, ..fixed.clone() });
    assert_eq!(mask_rows(&both), [".#", "..", ".."], "crop is applied before rotation");

    let outside = PixelRect { x: 2, y: 0, width: 3, height: 1 };
    let err = prepare(&wide, &scaled(ImageRecipe { crop: Some(outside), ..fixed }), &CancellationToken::never());
    assert_eq!(err.unwrap_err(), ImageError::CropOutOfBounds);
}

#[test]
fn grayscale_uses_rec709_and_explicit_alpha_policy() {
    let over_white = AlphaPolicy::CompositeOver { background: 255 };
    assert_eq!(to_gray([0.0, 0.0, 0.0, 255.0], over_white), 0.0);
    assert_eq!(to_gray([255.0, 255.0, 255.0, 255.0], over_white), 255.0);
    assert!((to_gray([255.0, 0.0, 0.0, 255.0], over_white) - 0.2126 * 255.0).abs() < 1e-3);
    assert!((to_gray([0.0, 255.0, 0.0, 255.0], over_white) - 0.7152 * 255.0).abs() < 1e-3);
    // A fully transparent black pixel is paper, not ink.
    assert_eq!(to_gray([0.0, 0.0, 0.0, 0.0], over_white), 255.0);
    assert_eq!(to_gray([0.0, 0.0, 0.0, 0.0], AlphaPolicy::CompositeOver { background: 0 }), 0.0);
    // Half-transparent black over white is mid gray.
    assert!((to_gray([0.0, 0.0, 0.0, 127.5], over_white) - 127.5).abs() < 1e-3);
    // Silhouette ignores colour: an opaque white pixel is foreground ink.
    assert_eq!(to_gray([255.0, 255.0, 255.0, 255.0], AlphaPolicy::Silhouette), 0.0);
    assert_eq!(to_gray([0.0, 0.0, 0.0, 0.0], AlphaPolicy::Silhouette), 255.0);
}

#[test]
fn otsu_separates_unbalanced_bimodal_image() {
    // 900 dark pixels around 40 and 100 light pixels around 180 (±10): every threshold that
    // separates the modes lies in [50, 170); Otsu must pick one of them.
    let mut values = Vec::new();
    for i in 0..900 {
        values.push(30.0 + (i % 21) as f32);
    }
    for i in 0..100 {
        values.push(170.0 + (i % 21) as f32);
    }
    let img = GrayImage { width: 1000, height: 1, values };
    let t = otsu_threshold(&img);
    assert!((50..170).contains(&t), "otsu picked {t}");
    let uniform = GrayImage { width: 4, height: 1, values: vec![90.0; 4] };
    assert_eq!(otsu_threshold(&uniform), 127, "documented fallback for a single gray level");
}

#[test]
fn threshold_and_invert_choose_foreground() {
    let img = SourceImage::from_gray8(3, 1, vec![10, 128, 250]).unwrap();
    let fixed = ImageRecipe { threshold: Threshold::Fixed(128), ..ImageRecipe::default() };
    assert_eq!(mask_rows(&run(&img, fixed.clone())), ["##."], "<= level is foreground");
    assert_eq!(mask_rows(&run(&img, ImageRecipe { invert: true, ..fixed })), ["..#"]);
}

#[test]
fn median_removes_isolated_speck_and_keeps_step_edge() {
    let mut values = vec![255.0; 49];
    values[24] = 0.0; // centre speck of a 7×7
    let img = GrayImage { width: 7, height: 7, values };
    let out = median_filter(&img, 1, &CancellationToken::never()).unwrap();
    assert!(out.values.iter().all(|&v| v == 255.0));

    // Left half black, right half white: a median keeps a straight step exactly.
    let step =
        GrayImage { width: 6, height: 5, values: (0..30).map(|i| if i % 6 < 3 { 0.0 } else { 255.0 }).collect() };
    assert_eq!(median_filter(&step, 2, &CancellationToken::never()).unwrap(), step);
    assert_eq!(
        median_filter(&step, 99, &CancellationToken::never()).unwrap_err(),
        ImageError::RadiusTooLarge { radius: 99, max: 16 }
    );
}

#[test]
fn region_masks_include_and_exclude_without_touching_source() {
    let img = art(&["#####", "#####", "#####", "#####"]);
    let fixed = ImageRecipe { threshold: Threshold::Fixed(127), ..ImageRecipe::default() };
    let include = RegionMask {
        mode: RegionMode::Include,
        shape: RegionShape::Rect(PixelRect { x: 1, y: 1, width: 3, height: 2 }),
    };
    let exclude = RegionMask {
        mode: RegionMode::Exclude,
        shape: RegionShape::Polygon(vec![[2.0, 0.0], [3.0, 0.0], [3.0, 4.0], [2.0, 4.0]]),
    };
    let p = run(&img, ImageRecipe { regions: vec![include, exclude], ..fixed.clone() });
    assert_eq!(mask_rows(&p), [".....", ".#.#.", ".#.#.", "....."]);
    // Field sign follows the mask everywhere, including pixels the masks overrode.
    for y in 0..p.height() {
        for x in 0..p.width() {
            assert_eq!(p.field_at(x, y) > 0.0, p.mask.get(x.into(), y.into()), "({x},{y})");
        }
    }
    let degenerate =
        RegionMask { mode: RegionMode::Exclude, shape: RegionShape::Polygon(vec![[0.0, 0.0], [1.0, 1.0]]) };
    let err = prepare(&img, &scaled(ImageRecipe { regions: vec![degenerate], ..fixed }), &CancellationToken::never());
    assert_eq!(err.unwrap_err(), ImageError::BadRegion);
}

#[test]
fn morphology_open_removes_speck_and_close_fills_pinhole() {
    let fixed = ImageRecipe { threshold: Threshold::Fixed(127), ..ImageRecipe::default() };
    let speck = art(&[
        "..........",
        ".#........",
        "..........",
        "....#####.",
        "....#####.",
        "....#####.",
        "....#####.",
        "....#####.",
        "..........",
    ]);
    let opened = run(&speck, ImageRecipe { morphology: vec![Morphology::Open(1)], ..fixed.clone() });
    let mut expected = mask_rows(&run(&speck, fixed.clone()));
    expected[1] = "..........".into();
    assert_eq!(mask_rows(&opened), expected, "speck removed, 5×5 square unchanged");

    let pinhole = art(&["......", ".####.", ".#.##.", ".####.", ".####.", "......"]);
    let closed = run(&pinhole, ImageRecipe { morphology: vec![Morphology::Close(1)], ..fixed });
    assert_eq!(mask_rows(&closed), ["......", ".####.", ".####.", ".####.", ".####.", "......"]);
}

#[test]
fn cancelled_preparation_returns_cancelled_not_a_partial_mask() {
    let img = SourceImage::from_gray8(64, 64, vec![0; 64 * 64]).unwrap();
    let source = CancellationSource::new();
    source.cancel();
    let err = prepare(&img, &scaled(ImageRecipe { median_radius: 2, ..ImageRecipe::default() }), &source.token());
    assert!(matches!(err, Err(ImageError::Cancelled(_))));
}

proptest! {
    /// The field/mask invariant holds for any gray image, threshold, inversion and opening.
    #[test]
    fn field_sign_always_matches_mask(px in proptest::collection::vec(any::<u8>(), 36), t in any::<u8>(), invert: bool, open: bool) {
        let img = SourceImage::from_gray8(6, 6, px).unwrap();
        let morphology = if open { vec![Morphology::Open(1)] } else { vec![] };
        let p = run(&img, ImageRecipe { threshold: Threshold::Fixed(t), invert, morphology, ..ImageRecipe::default() });
        for y in 0..6 {
            for x in 0..6 {
                prop_assert_eq!(p.field_at(x, y) > 0.0, p.mask.get(x.into(), y.into()));
            }
        }
    }
}
