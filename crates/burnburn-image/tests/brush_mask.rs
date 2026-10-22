//! Brush masks (G7-trace, T03): a stroke of the pointer with a radius is a region shape. A pixel
//! belongs when its centre is within the radius of the polyline. Oracles are counted by hand.

#![allow(clippy::unwrap_used)]

use burnburn_image::{
    ImageError, ImageRecipe, PixelScale, RegionMask, RegionMode, RegionShape, SourceImage, Threshold, prepare,
};
use burnburn_types::{CancellationToken, CanonicalEncode, CanonicalWriter};

fn stroke(points: &[[f64; 2]], radius: f64) -> RegionShape {
    RegionShape::Stroke { points: points.to_vec(), radius }
}

fn inside(shape: &RegionShape, w: u32, h: u32) -> Vec<(u32, u32)> {
    (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| shape.contains_pixel(x, y)).collect()
}

#[test]
fn a_horizontal_stroke_covers_the_capsule_around_its_segment() {
    // Segment (3.5, 5.5)-(10.5, 5.5), radius 1: pixel centres within 1 of it.
    // Rows 4, 5 and 6 for x = 3..=10 (8 each, rows 4 and 6 are exactly 1 away), plus the cap
    // pixels (2, 5) and (11, 5) exactly 1 from the end points; the diagonal neighbours are √2 away.
    let covered = inside(&stroke(&[[3.5, 5.5], [10.5, 5.5]], 1.0), 16, 12);
    assert_eq!(covered.len(), 26);
    assert!(covered.contains(&(2, 5)) && covered.contains(&(11, 5)));
    assert!(!covered.contains(&(2, 4)) && !covered.contains(&(11, 6)));
    assert!(covered.iter().all(|&(_, y)| (4..=6).contains(&y)));
}

#[test]
fn a_single_point_is_a_disc_and_a_bent_stroke_follows_both_legs() {
    let disc = inside(&stroke(&[[5.5, 5.5]], 1.0), 12, 12);
    assert_eq!(disc, [(5, 4), (4, 5), (5, 5), (6, 5), (5, 6)], "centre and the four neighbours at distance 1");
    // An L: down from (2.5, 2.5) to (2.5, 8.5), then right to (8.5, 8.5); the corner pixel is covered once.
    let l = stroke(&[[2.5, 2.5], [2.5, 8.5], [8.5, 8.5]], 0.5);
    let covered = inside(&l, 12, 12);
    assert!(covered.contains(&(2, 5)) && covered.contains(&(5, 8)) && covered.contains(&(2, 8)));
    assert!(!covered.contains(&(5, 5)), "the inside of the L is not painted");
    assert_eq!(covered.len(), 13, "7 pixels down, 6 more to the right");
}

#[test]
fn a_stroke_needs_a_point_and_a_positive_finite_radius() {
    for bad in [
        stroke(&[], 1.0),
        stroke(&[[1.0, 1.0]], 0.0),
        stroke(&[[1.0, 1.0]], -2.0),
        stroke(&[[1.0, 1.0]], f64::NAN),
        stroke(&[[f64::INFINITY, 1.0]], 1.0),
    ] {
        assert_eq!(bad.validate(), Err(ImageError::BadRegion), "{bad:?}");
    }
    assert!(stroke(&[[1.0, 1.0], [2.0, 2.0]], 0.25).validate().is_ok());
}

#[test]
fn an_exclude_stroke_removes_foreground_and_a_keep_stroke_keeps_only_its_paint() {
    let img = SourceImage::from_gray8(10, 6, vec![0; 60]).unwrap(); // all black = all foreground
    let base = ImageRecipe {
        scale: Some(PixelScale::from_mm_per_pixel(0.1, 0.1).unwrap()),
        threshold: Threshold::Fixed(127),
        ..ImageRecipe::default()
    };
    let paint = stroke(&[[1.5, 2.5], [8.5, 2.5]], 0.5);
    let removed =
        ImageRecipe { regions: vec![RegionMask { mode: RegionMode::Exclude, shape: paint.clone() }], ..base.clone() };
    let p = prepare(&img, &removed, &CancellationToken::never()).unwrap();
    assert_eq!(p.mask.count(), 60 - 8, "the row of 8 pixels under the paint is background");
    let kept = ImageRecipe { regions: vec![RegionMask { mode: RegionMode::Include, shape: paint }], ..base };
    let p = prepare(&img, &kept, &CancellationToken::never()).unwrap();
    assert_eq!(p.mask.count(), 8);
}

#[test]
fn the_stroke_is_part_of_the_recipe_key() {
    let key = |shape: RegionShape| {
        let mut w = CanonicalWriter::new();
        RegionMask { mode: RegionMode::Exclude, shape }.encode(&mut w);
        w.finish().unwrap()
    };
    let a = key(stroke(&[[1.0, 1.0], [4.0, 4.0]], 1.0));
    assert_eq!(a, key(stroke(&[[1.0, 1.0], [4.0, 4.0]], 1.0)));
    assert_ne!(a, key(stroke(&[[1.0, 1.0], [4.0, 4.0]], 1.5)));
    assert_ne!(a, key(stroke(&[[1.0, 1.0], [4.0, 4.5]], 1.0)));
    assert_ne!(a, key(RegionShape::Polygon(vec![[1.0, 1.0], [4.0, 4.0], [4.0, 1.0]])));
}
