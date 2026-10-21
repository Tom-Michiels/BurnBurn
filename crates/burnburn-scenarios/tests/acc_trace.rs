//! Acceptance plan TR01: tracing a bitmap with holes, noise, transparency and an open (thin)
//! line. The bitmap is drawn here from known rectangles, so the expected contour hierarchy,
//! areas and positions are plain arithmetic; the error budget is the recipe's own distance
//! tolerance, checked against the analytic rectangle boundaries (not against the tracer).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test code

use std::sync::Arc;

use burnburn_geometry::{Path, Point};
use burnburn_image::{AlphaPolicy, ImageRecipe, PixelScale, SourceImage, Threshold};
use burnburn_trace::{ContourKind, FitSettings, TraceRecipe, TraceRequest, TraceResult, trace};
use burnburn_types::{CancellationToken, RevisionId, StableId, TaskId};

const MM_PER_PX: f64 = 0.1;
const W: usize = 220;
const H: usize = 130;

/// Pixel rectangle `[x0, x1) x [y0, y1)` (y down, as in the bitmap).
type R = (usize, usize, usize, usize);

const PLATE: R = (20, 20, 120, 100); // 10 x 8 mm
const HOLE: R = (50, 40, 80, 70); // 3 x 3 mm
const ISLAND: R = (58, 48, 72, 62); // 1.4 x 1.4 mm
const LINE: R = (140, 30, 190, 32); // 5 x 0.2 mm, the "open line" as drawn (a thin stroke)
const SPECKS: [R; 3] = [(10, 8, 12, 10), (200, 110, 202, 112), (130, 10, 132, 12)]; // 0.2 x 0.2 mm
const PINHOLE: R = (30, 30, 32, 32); // inside the plate, 0.2 x 0.2 mm
const GHOST: R = (140, 60, 200, 110); // black RGB but alpha 0: must not be traced
const FAINT: R = (140, 112, 200, 125); // black at alpha 100: composited over white it is light

fn inside(r: R, x: usize, y: usize) -> bool {
    x >= r.0 && x < r.2 && y >= r.1 && y < r.3
}

fn bitmap() -> SourceImage {
    let mut rgba = Vec::with_capacity(W * H * 4);
    for y in 0..H {
        for x in 0..W {
            let ink = (inside(PLATE, x, y) && !inside(HOLE, x, y) && !inside(PINHOLE, x, y))
                || inside(ISLAND, x, y)
                || inside(LINE, x, y)
                || SPECKS.iter().any(|s| inside(*s, x, y));
            let px = if ink {
                [0, 0, 0, 255]
            } else if inside(GHOST, x, y) {
                [0, 0, 0, 0]
            } else if inside(FAINT, x, y) {
                [0, 0, 0, 100]
            } else {
                [255, 255, 255, 255]
            };
            rgba.extend_from_slice(&px);
        }
    }
    SourceImage::from_rgba8(W as u32, H as u32, rgba).unwrap()
}

fn request(min_area: f64, alpha: AlphaPolicy) -> TraceRequest {
    let mut recipe = TraceRecipe {
        image: ImageRecipe {
            scale: Some(PixelScale::uniform_mm_per_pixel(MM_PER_PX).unwrap()),
            threshold: Threshold::Fixed(127),
            alpha,
            ..ImageRecipe::default()
        },
        fit: FitSettings { tolerance_mm: 0.02, ..FitSettings::default() },
        ..TraceRecipe::default()
    };
    recipe.contour.min_area_mm2 = min_area;
    TraceRequest { task: TaskId::from_raw(1), source_revision: RevisionId::INITIAL, source: Arc::new(bitmap()), recipe }
}

fn run(min_area: f64, alpha: AlphaPolicy) -> TraceResult {
    trace(&request(min_area, alpha), &CancellationToken::never()).unwrap()
}

/// mm rectangle (y up) of a pixel rectangle: the bitmap's origin is the bottom-left corner.
fn mm(r: R) -> (f64, f64, f64, f64) {
    let f = |v: usize| v as f64 * MM_PER_PX;
    (f(r.0), (H - r.3) as f64 * MM_PER_PX, f(r.2), (H - r.1) as f64 * MM_PER_PX)
}

fn dist_to_rect(p: Point, r: (f64, f64, f64, f64)) -> f64 {
    let (x0, y0, x1, y1) = r;
    let dx = (x0 - p.x).max(p.x - x1).max(0.0);
    let dy = (y0 - p.y).max(p.y - y1).max(0.0);
    if dx > 0.0 || dy > 0.0 {
        return dx.hypot(dy);
    }
    // inside: distance to the nearest edge
    (p.x - x0).min(x1 - p.x).min(p.y - y0).min(y1 - p.y)
}

fn area_of(r: R) -> f64 {
    ((r.2 - r.0) * (r.3 - r.1)) as f64 * MM_PER_PX * MM_PER_PX
}

#[test]
fn tr01_hierarchy_areas_and_boundaries_follow_the_drawn_bitmap() {
    // min area 0.1 mm2 removes the 0.04 mm2 specks and the pinhole, keeps the 1.0 mm2 line.
    let result = run(0.1, AlphaPolicy::default());
    let by_kind = |k: ContourKind| result.contours.iter().filter(|c| c.kind == k).count();
    assert_eq!(by_kind(ContourKind::Outer), 3, "plate, island in the hole, thin line");
    assert_eq!(by_kind(ContourKind::Hole), 1);
    // The ghost (alpha 0) and the faint (alpha 100) regions are paper, not ink.
    assert!(result.contours.iter().all(|c| c.area_mm2 > 0.5), "nothing unexpected was traced");

    // Tree: the hole belongs to the plate, the island to the hole.
    let plate = result.contours.iter().position(|c| (c.area_mm2 - area_of(PLATE)).abs() < 0.8).unwrap();
    let hole = result.contours.iter().position(|c| c.kind == ContourKind::Hole).unwrap();
    assert_eq!(result.contours[hole].parent, Some(plate));
    assert_eq!(result.contours[hole].depth, 1);
    let island = result.contours.iter().position(|c| (c.area_mm2 - area_of(ISLAND)).abs() < 0.15).unwrap();
    assert_eq!(result.contours[island].parent, Some(hole));
    assert_eq!(result.contours[island].depth, 2);

    // Areas within 2 % of the drawn rectangles (budget 0.02 mm on a 36 mm perimeter ~ 1.5 %).
    for (idx, r) in [(plate, PLATE), (hole, HOLE), (island, ISLAND), (result.contours.len() - 1, LINE)] {
        let _ = idx;
        let _ = r;
    }
    let near = |a: f64, b: f64| (a - b).abs() <= 0.02 * b + 0.02;
    assert!(near(result.contours[plate].area_mm2, area_of(PLATE)));
    assert!(near(result.contours[hole].area_mm2, area_of(HOLE)));
    assert!(near(result.contours[island].area_mm2, area_of(ISLAND)));
    let line = result
        .contours
        .iter()
        .find(|c| c.kind == ContourKind::Outer && c.parent.is_none() && c.area_mm2 < 2.0)
        .unwrap();
    assert!(near(line.area_mm2, area_of(LINE)), "the thin stroke keeps its 5 x 0.2 mm: {}", line.area_mm2);

    // Every traced boundary point lies within the budget of its drawn rectangle's boundary.
    for (idx, r) in [(plate, PLATE), (hole, HOLE), (island, ISLAND)] {
        let path = Path::from_subpaths(vec![result.contours[idx].path.clone()]);
        let lines = path.flatten(
            &burnburn_geometry::ProductionTolerance::new(burnburn_types::Length::from_mm(0.01).unwrap()).unwrap(),
        );
        let worst = lines[0].points.iter().map(|p| dist_to_rect(*p, mm(r))).fold(0.0, f64::max);
        assert!(worst <= 0.05, "{r:?}: traced boundary is {worst} mm off the drawn rectangle");
    }
    // Position: the plate's bounds are its drawn bounds in mm, y up.
    let b = Path::from_subpaths(vec![result.contours[plate].path.clone()]).bounds().unwrap();
    let want = mm(PLATE);
    assert!((b.min().x - want.0).abs() <= 0.05 && (b.min().y - want.1).abs() <= 0.05);
    assert!((b.max().x - want.2).abs() <= 0.05 && (b.max().y - want.3).abs() <= 0.05);
}

#[test]
fn tr01_noise_is_removed_only_on_request_and_every_removed_feature_is_reported() {
    let all = run(0.0, AlphaPolicy::default());
    assert!(all.removed.is_empty(), "nothing is dropped unless asked");
    // plate + island + line + 3 specks outside; holes: big hole + pinhole.
    assert_eq!(all.contours.iter().filter(|c| c.kind == ContourKind::Outer).count(), 3 + SPECKS.len());
    assert_eq!(all.contours.iter().filter(|c| c.kind == ContourKind::Hole).count(), 2);

    let cleaned = run(0.1, AlphaPolicy::default());
    assert_eq!(cleaned.removed.len(), SPECKS.len() + 1, "3 specks and the pinhole are reported");
    for r in &cleaned.removed {
        assert!((r.area_mm2 - 0.04).abs() < 0.02, "{r:?}");
    }
    let holes: Vec<_> = cleaned.removed.iter().filter(|r| r.kind == ContourKind::Hole).collect();
    assert_eq!(holes.len(), 1);
}

fn rgba_square(ink: [u8; 4]) -> SourceImage {
    // 60 x 40 px, transparent black everywhere except a 20 x 20 px square of colour `ink`.
    let mut rgba = Vec::new();
    for y in 0..40 {
        for x in 0..60 {
            let on = (20..40).contains(&x) && (10..30).contains(&y);
            rgba.extend_from_slice(if on { &ink } else { &[0, 0, 0, 0] });
        }
    }
    SourceImage::from_rgba8(60, 40, rgba).unwrap()
}

fn traced(source: SourceImage, alpha: AlphaPolicy) -> TraceResult {
    let mut req = request(0.0, alpha);
    req.source = Arc::new(source);
    trace(&req, &CancellationToken::never()).unwrap()
}

#[test]
fn tr01_transparent_pixels_are_paper_and_the_alpha_policy_decides_what_opaque_colour_means() {
    // Opaque black square on transparent black: ink under both policies, 2 x 2 mm. The transparent
    // pixels carry black RGB, which must not be read as ink.
    for alpha in [AlphaPolicy::default(), AlphaPolicy::Silhouette] {
        let r = traced(rgba_square([0, 0, 0, 255]), alpha);
        assert_eq!(r.contours.len(), 1, "{alpha:?}");
        assert!((r.contours[0].area_mm2 - 4.0).abs() < 0.1, "{alpha:?}: {}", r.contours[0].area_mm2);
    }
    // Opaque WHITE square on transparent: composited over white paper it is invisible; as a
    // silhouette (opacity only) it is the shape.
    assert!(traced(rgba_square([255, 255, 255, 255]), AlphaPolicy::default()).contours.is_empty());
    let sil = traced(rgba_square([255, 255, 255, 255]), AlphaPolicy::Silhouette);
    assert_eq!(sil.contours.len(), 1);
    assert!((sil.contours[0].area_mm2 - 4.0).abs() < 0.1);
    // Half-transparent black (alpha 100 of 255): gray 155 over paper, above the threshold 127.
    assert!(traced(rgba_square([0, 0, 0, 100]), AlphaPolicy::default()).contours.is_empty());
    // And a fully transparent bitmap traces nothing.
    assert!(traced(rgba_square([0, 0, 0, 0]), AlphaPolicy::default()).contours.is_empty());
    assert!(traced(rgba_square([0, 0, 0, 0]), AlphaPolicy::Silhouette).contours.is_empty());
}
