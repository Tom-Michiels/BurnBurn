//! Public contract of contour tracing, with oracles from independent vector references
//! (support::rasterize) rather than from the tracer's own formulas.

#![allow(clippy::unwrap_used)]

mod support;

use std::f64::consts::PI;
use std::sync::Arc;

use burnburn_geometry::{Point, Segment};
use burnburn_image::{ImageError, ImageRecipe, PixelScale, ScaleError, Threshold};
use burnburn_trace::{ContourKind, FitSettings, TraceError, TraceRecipe, TraceRequest, TraceResult, trace};
use burnburn_types::{CancellationToken, Length, RevisionId, StableId, TaskId};
use support::*;

fn request(img: burnburn_image::SourceImage, mm_per_px: f64, fit: FitSettings) -> TraceRequest {
    TraceRequest {
        task: TaskId::from_raw(1),
        source_revision: RevisionId::INITIAL,
        source: Arc::new(img),
        recipe: TraceRecipe {
            image: ImageRecipe {
                scale: Some(PixelScale::uniform_mm_per_pixel(mm_per_px).unwrap()),
                threshold: Threshold::Fixed(127),
                ..ImageRecipe::default()
            },
            fit,
            ..TraceRecipe::default()
        },
    }
}

fn fit(tolerance_mm: f64) -> FitSettings {
    FitSettings { tolerance_mm, ..FitSettings::default() }
}

fn run(req: &TraceRequest) -> TraceResult {
    trace(req, &CancellationToken::never()).unwrap()
}

#[test]
fn traced_ring_preserves_hole_at_declared_physical_scale() {
    // Ring: outer radius 20 mm, inner 10 mm, 10 px/mm, centred in a 44 mm square image.
    let (c, ro, ri) = (22.0, 20.0, 10.0);
    let img = rasterize(440, 440, 0.1, |x, y| {
        let d2 = (x - c).powi(2) + (y - c).powi(2);
        d2 <= ro * ro && d2 >= ri * ri
    });
    let result = run(&request(img, 0.1, fit(0.02)));

    assert_eq!(result.contours.len(), 2, "one outer contour and one hole");
    let outer = result.contours.iter().position(|k| k.kind == ContourKind::Outer).unwrap();
    let hole = result.contours.iter().position(|k| k.kind == ContourKind::Hole).unwrap();
    assert_eq!(result.contours[outer].parent, None);
    assert_eq!(result.contours[hole].parent, Some(outer), "hole is nested in the outer contour");
    assert_eq!((result.contours[outer].depth, result.contours[hole].depth), (0, 1));
    assert_eq!(result.components.len(), 1);
    assert_eq!(result.components[0].holes, vec![hole]);

    for (idx, r) in [(outer, ro), (hole, ri)] {
        let path = burnburn_geometry::Path::from_subpaths(vec![result.contours[idx].path.clone()]);
        let pts = sample_path(&path, 0.05);
        let worst = pts.iter().map(|p| (p.distance(Point::new(c, c)) - r).abs()).fold(0.0, f64::max);
        assert!(worst <= 0.05, "radius {r}: max radial error {worst} mm");
        let reverse = directed(&circle_samples(c, c, r, 0.05), &pts);
        assert!(reverse <= 0.05, "radius {r}: reference-to-trace distance {reverse} mm");
    }
    let area = result.path().signed_area();
    let expected = PI * (ro * ro - ri * ri);
    assert!((area - expected).abs() <= 0.01 * expected, "area {area} vs {expected}");
    assert!(result.contours[outer].path.signed_area().unwrap() > 0.0, "outer counter-clockwise (y-up)");
    assert!(result.contours[hole].path.signed_area().unwrap() < 0.0, "hole clockwise");
}

#[test]
fn separate_components_and_an_island_form_the_expected_tree() {
    // Three discs, a rectangle, and a target (ring with a disc island in its hole), 10 px/mm.
    let shapes: Vec<Box<dyn Fn(f64, f64) -> bool>> = vec![
        Box::new(disc(6.0, 6.0, 3.0)),
        Box::new(disc(16.0, 6.0, 2.0)),
        Box::new(disc(26.0, 6.0, 1.0)),
        Box::new(convex(vec![(3.0, 12.0), (29.0, 12.0), (29.0, 15.0), (3.0, 15.0)])),
        Box::new(|x: f64, y: f64| {
            let d2 = (x - 16.0).powi(2) + (y - 26.0).powi(2);
            (25.0..=64.0).contains(&d2) || d2 <= 4.0
        }),
    ];
    let img = rasterize(320, 360, 0.1, |x, y| shapes.iter().any(|s| s(x, y)));
    let result = run(&request(img, 0.1, fit(0.02)));

    let outers = result.contours.iter().filter(|c| c.kind == ContourKind::Outer).count();
    let holes = result.contours.iter().filter(|c| c.kind == ContourKind::Hole).count();
    assert_eq!((outers, holes), (6, 1), "5 parts + island; 1 hole");
    assert_eq!(result.components.len(), 6);
    let island = result.contours.iter().find(|c| c.depth == 2).unwrap();
    assert_eq!(island.kind, ContourKind::Outer);
    let parent = &result.contours[island.parent.unwrap()];
    assert_eq!(parent.kind, ContourKind::Hole);
    assert_eq!(result.stats.closed_shapes, 6);
    assert_eq!(result.stats.holes, 1);

    // Every contour lies within 0.06 mm of the union reference boundary (bidirectional per part).
    let mut reference = circle_samples(6.0, 6.0, 3.0, 0.05);
    reference.extend(circle_samples(16.0, 6.0, 2.0, 0.05));
    reference.extend(circle_samples(26.0, 6.0, 1.0, 0.05));
    reference.extend(polygon_samples(&[(3.0, 12.0), (29.0, 12.0), (29.0, 15.0), (3.0, 15.0)], 0.05));
    for r in [8.0, 5.0, 2.0] {
        reference.extend(circle_samples(16.0, 26.0, r, 0.05));
    }
    let traced = sample_path(&result.path(), 0.05);
    let d = hausdorff(&traced, &reference);
    assert!(d <= 0.06, "bidirectional Hausdorff {d} mm");
}

#[test]
fn square_keeps_four_sharp_corners_within_budget() {
    let budget = 0.05;
    // Axis-aligned 30 mm square at a non-integer pixel offset, and the same square rotated 20°.
    let a = (5.37, 4.81);
    let axis = vec![(a.0, a.1), (a.0 + 30.0, a.1), (a.0 + 30.0, a.1 + 30.0), (a.0, a.1 + 30.0)];
    let (cx, cy, h, t) = (21.0, 21.0, 15.0, 20f64.to_radians());
    let rotated: Vec<(f64, f64)> = [(-h, -h), (h, -h), (h, h), (-h, h)]
        .iter()
        .map(|&(x, y)| (cx + x * t.cos() - y * t.sin(), cy + x * t.sin() + y * t.cos()))
        .collect();
    for poly in [axis, rotated] {
        let img = rasterize(420, 420, 0.1, convex(poly.clone()));
        let result = run(&request(img, 0.1, fit(budget)));
        assert_eq!(result.contours.len(), 1);
        let sub = &result.contours[0].path;
        assert_eq!(sub.segments().len(), 4, "four straight sides: {:?}", sub.segments());
        assert!(sub.segments().iter().all(|s| matches!(s, Segment::Line { .. })));
        for &(x, y) in &poly {
            let corner = Point::new(x, y);
            let nearest = sub.segments().iter().map(|s| s.end().distance(corner)).fold(f64::INFINITY, f64::min);
            assert!(nearest <= budget, "corner ({x:.2},{y:.2}) rounded or moved by {nearest} mm");
        }
        let path = burnburn_geometry::Path::from_subpaths(vec![sub.clone()]);
        let d = hausdorff(&sample_path(&path, 0.05), &polygon_samples(&poly, 0.05));
        assert!(d <= budget, "Hausdorff {d}");
    }
}

#[test]
fn explicit_scale_sets_output_millimetres_and_missing_scale_is_reported() {
    // A 20×10 px black rectangle inside a 40×30 px image (exactly pixel aligned).
    let img = rasterize(40, 30, 1.0, convex(vec![(10.0, 10.0), (30.0, 10.0), (30.0, 20.0), (10.0, 20.0)]));
    let mut req = request(img, 0.5, fit(0.01));
    let bounds = run(&req).path().bounds().unwrap();
    assert!((bounds.width() - 10.0).abs() < 1e-6 && (bounds.height() - 5.0).abs() < 1e-6, "{bounds:?}");

    req.recipe.image.scale = Some(PixelScale::from_dpi(254.0, 127.0).unwrap()); // 0.1 × 0.2 mm/px
    let b = run(&req).path().bounds().unwrap();
    assert!((b.width() - 2.0).abs() < 1e-6 && (b.height() - 2.0).abs() < 1e-6, "{b:?}");
    assert!((b.min().x - 1.0).abs() < 1e-6 && (b.min().y - 2.0).abs() < 1e-6, "y-up from bottom edge: {b:?}");

    req.recipe.image.scale = Some(PixelScale::from_known_distance(20.0, Length::from_mm(50.0).unwrap()).unwrap());
    let b = run(&req).path().bounds().unwrap();
    assert!((b.width() - 50.0).abs() < 1e-6, "{b:?}");

    req.recipe.image.scale = None;
    let err = trace(&req, &CancellationToken::never()).unwrap_err();
    assert_eq!(err, TraceError::Image(ImageError::Scale(ScaleError::NeedsScale)));
}

#[test]
fn small_specks_and_pinholes_are_removed_and_reported_not_silently_dropped() {
    // 20 mm square with a 0.4 mm pinhole and a 1.5 mm hole; a 0.4 mm speck outside. 10 px/mm.
    let square = convex(vec![(2.0, 2.0), (22.0, 2.0), (22.0, 22.0), (2.0, 22.0)]);
    let pinhole = convex(vec![(6.0, 6.0), (6.4, 6.0), (6.4, 6.4), (6.0, 6.4)]);
    let hole = convex(vec![(12.0, 12.0), (13.5, 12.0), (13.5, 13.5), (12.0, 13.5)]);
    let speck = convex(vec![(24.0, 24.0), (24.4, 24.0), (24.4, 24.4), (24.0, 24.4)]);
    let img = rasterize(260, 260, 0.1, |x, y| (square(x, y) && !pinhole(x, y) && !hole(x, y)) || speck(x, y));
    let mut req = request(img, 0.1, fit(0.02));
    let all = run(&req);
    assert_eq!((all.stats.closed_shapes, all.stats.holes), (2, 2), "nothing dropped by default");
    assert!(all.removed.is_empty());

    req.recipe.contour.min_area_mm2 = 0.5;
    let filtered = run(&req);
    assert_eq!((filtered.stats.closed_shapes, filtered.stats.holes), (1, 1));
    let mut kinds: Vec<_> = filtered.removed.iter().map(|r| r.kind).collect();
    kinds.sort_by_key(|k| *k == ContourKind::Hole);
    assert_eq!(kinds, [ContourKind::Outer, ContourKind::Hole], "speck and pinhole are both reported");
    for r in &filtered.removed {
        assert!((r.area_mm2 - 0.16).abs() < 0.02, "{r:?}");
    }
    let kept_hole = filtered.contours.iter().find(|c| c.kind == ContourKind::Hole).unwrap();
    assert!((kept_hole.area_mm2 - 2.25).abs() < 0.05);
}
