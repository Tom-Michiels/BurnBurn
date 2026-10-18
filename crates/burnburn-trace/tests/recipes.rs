//! IM24 advanced recipes: local (adaptive) threshold, four-point perspective with residual, A/B
//! comparison, batch recipes, and corrections that refuse to land on an unrelated new component.

#![allow(clippy::unwrap_used)]

mod support;

use std::collections::BTreeSet;
use std::sync::Arc;

use burnburn_geometry::Point;
use burnburn_image::{
    ImageError, ImageRecipe, Perspective, PixelRect, PixelScale, RegionShape, SourceImage, Threshold,
};
use burnburn_trace::{
    BatchItem, BatchProblem, ContourKind, CorrectionRequest, Reapplied, TraceError, TraceRecipe, TraceRequest,
    TraceResult, compare_recipes, reapply_correction, retrace_region, trace, trace_batch,
};
use burnburn_types::{CancellationToken, RevisionId, StableId, TaskId};
use support::*;

const S: f64 = 0.1;

fn recipe_with(threshold: Threshold) -> TraceRecipe {
    TraceRecipe {
        image: ImageRecipe {
            scale: Some(PixelScale::uniform_mm_per_pixel(S).unwrap()),
            threshold,
            ..ImageRecipe::default()
        },
        ..TraceRecipe::default()
    }
}

fn run(source: SourceImage, recipe: TraceRecipe) -> TraceResult {
    let req = TraceRequest {
        task: TaskId::from_raw(1),
        source_revision: RevisionId::INITIAL,
        source: Arc::new(source),
        recipe,
    };
    trace(&req, &CancellationToken::never()).unwrap()
}

/// Gray image from a per-sample function (8×8 samples per pixel), mm coordinates y-up.
fn shade(w: u32, h: u32, f: impl Fn(f64, f64) -> f64) -> SourceImage {
    let mut px = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0.0;
            for sy in 0..SS {
                for sx in 0..SS {
                    let fx = (f64::from(x) + (f64::from(sx) + 0.5) / f64::from(SS)) * S;
                    let fy = (f64::from(h - y - 1) + (f64::from(sy) + 0.5) / f64::from(SS)) * S;
                    sum += f(fx, fy);
                }
            }
            px.push((sum / f64::from(SS * SS)).round().clamp(0.0, 255.0) as u8);
        }
    }
    SourceImage::from_gray8(w, h, px).unwrap()
}

#[test]
fn local_threshold_separates_ink_from_shadow_where_the_global_threshold_fails() {
    // Paper falls linearly from 255 (left) to 40 (right); ink reflects 30 % of the local light.
    let left = disc(4.0, 5.0, 2.0);
    let right = disc(9.0, 5.0, 2.0);
    let img = || {
        shade(200, 100, |x, y| {
            let light = 255.0 - (255.0 - 40.0) * x / 20.0;
            if left(x, y) || right(x, y) { 0.3 * light } else { light }
        })
    };
    let global = run(img(), recipe_with(Threshold::Otsu));
    let disc_area = std::f64::consts::PI * 4.0;
    assert!(
        global.contours.iter().any(|c| c.area_mm2 > 2.0 * disc_area),
        "control: a global threshold turns the shadow into (part of) a part"
    );

    let local = run(img(), recipe_with(Threshold::Adaptive { radius: 30, offset: 20 }));
    assert_eq!((local.stats.closed_shapes, local.stats.holes), (2, 0));
    for c in &local.contours {
        assert!(
            (c.area_mm2 - std::f64::consts::PI * 4.0).abs() < 0.1 * std::f64::consts::PI * 4.0,
            "area {}",
            c.area_mm2
        );
    }
    // The local threshold is a recipe parameter and part of the key.
    assert_ne!(
        recipe_with(Threshold::Adaptive { radius: 30, offset: 20 }).key(img().content_hash()).unwrap(),
        recipe_with(Threshold::Adaptive { radius: 31, offset: 20 }).key(img().content_hash()).unwrap()
    );
    // Out-of-range window is a typed error.
    let req = TraceRequest {
        task: TaskId::from_raw(1),
        source_revision: RevisionId::INITIAL,
        source: Arc::new(img()),
        recipe: recipe_with(Threshold::Adaptive { radius: 10_000, offset: 20 }),
    };
    assert!(matches!(
        trace(&req, &CancellationToken::never()),
        Err(TraceError::Image(ImageError::RadiusTooLarge { .. }))
    ));
}

/// Unit square → quad (corner order (0,0), (1,0), (1,1), (0,1)), closed-form projective map
/// (Heckbert 1989), as row-major 3×3; used only to *render* the photo.
fn square_to_quad(q: [[f64; 2]; 4]) -> [f64; 9] {
    let [p0, p1, p2, p3] = q;
    let (dx1, dx2, dx3) = (p1[0] - p2[0], p3[0] - p2[0], p0[0] - p1[0] + p2[0] - p3[0]);
    let (dy1, dy2, dy3) = (p1[1] - p2[1], p3[1] - p2[1], p0[1] - p1[1] + p2[1] - p3[1]);
    let den = dx1 * dy2 - dx2 * dy1;
    let g = (dx3 * dy2 - dx2 * dy3) / den;
    let h = (dx1 * dy3 - dx3 * dy1) / den;
    [
        p1[0] - p0[0] + g * p1[0],
        p3[0] - p0[0] + h * p3[0],
        p0[0],
        p1[1] - p0[1] + g * p1[1],
        p3[1] - p0[1] + h * p3[1],
        p0[1],
        g,
        h,
        1.0,
    ]
}

fn invert(m: [f64; 9]) -> [f64; 9] {
    let [a, b, c, d, e, f, g, h, i] = m;
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    [
        (e * i - f * h) / det,
        (c * h - b * i) / det,
        (b * f - c * e) / det,
        (f * g - d * i) / det,
        (a * i - c * g) / det,
        (c * d - a * f) / det,
        (d * h - e * g) / det,
        (b * g - a * h) / det,
        (a * e - b * d) / det,
    ]
}

fn apply(m: &[f64; 9], p: [f64; 2]) -> [f64; 2] {
    let w = m[6] * p[0] + m[7] * p[1] + m[8];
    [(m[0] * p[0] + m[1] * p[1] + m[2]) / w, (m[3] * p[0] + m[4] * p[1] + m[5]) / w]
}

#[test]
fn four_point_perspective_rectifies_a_flat_square_and_reports_its_residual() {
    // Photo 200×160 px of a 10×10 mm sheet with a 6×6 mm black square at (2..8, 2..8) mm.
    let quad = [[20.0, 15.0], [170.0, 30.0], [150.0, 140.0], [35.0, 120.0]];
    let to_photo = square_to_quad(quad);
    let to_sheet = invert(to_photo);
    let (pw, ph) = (200u32, 160u32);
    let mut px = Vec::new();
    for y in 0..ph {
        for x in 0..pw {
            let mut ink = 0;
            for sy in 0..SS {
                for sx in 0..SS {
                    let p = [
                        f64::from(x) + (f64::from(sx) + 0.5) / f64::from(SS),
                        f64::from(y) + (f64::from(sy) + 0.5) / f64::from(SS),
                    ];
                    let [u, v] = apply(&to_sheet, p);
                    let (mx, my) = (u * 10.0, 10.0 - v * 10.0);
                    ink += u32::from((2.0..=8.0).contains(&mx) && (2.0..=8.0).contains(&my));
                }
            }
            px.push((255.0 * (1.0 - f64::from(ink) / f64::from(SS * SS))).round() as u8);
        }
    }
    let photo = SourceImage::from_gray8(pw, ph, px).unwrap();
    let rect = [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]];
    let perspective = |extra: Option<([f64; 2], [f64; 2])>| {
        let mut points: Vec<([f64; 2], [f64; 2])> = quad.iter().copied().zip(rect).collect();
        points.extend(extra);
        Perspective { points, width: 100, height: 100 }
    };
    let mut recipe = recipe_with(Threshold::Fixed(127));
    recipe.image.perspective = Some(perspective(None));
    recipe.fit.tolerance_mm = 0.03;
    let req = TraceRequest {
        task: TaskId::from_raw(1),
        source_revision: RevisionId::INITIAL,
        source: Arc::new(photo.clone()),
        recipe: recipe.clone(),
    };
    let result = trace(&req, &CancellationToken::never()).unwrap();
    assert_eq!((result.width_px, result.height_px), (100, 100), "rectified working image");
    assert_eq!(result.stats.closed_shapes, 1);
    let outer = &result.contours[0];
    assert!((outer.area_mm2 - 36.0).abs() < 0.5, "area {}", outer.area_mm2);
    let nodes: Vec<Point> = outer.path.segments().iter().map(|s| s.end()).collect();
    for want in [Point::new(2.0, 2.0), Point::new(8.0, 2.0), Point::new(8.0, 8.0), Point::new(2.0, 8.0)] {
        let d = nodes.iter().map(|p| p.distance(want)).fold(f64::INFINITY, f64::min);
        assert!(d <= 0.1, "corner {want:?}: {d:.3} mm");
    }
    let prepared = burnburn_image::prepare(&photo, &recipe.image, &CancellationToken::never()).unwrap();
    let report = prepared.perspective.unwrap();
    assert!(report.residual_px < 1e-6, "four exact points: residual {}", report.residual_px);
    // The reported plane transform maps the photo quad onto the rectangle.
    for (q, r) in quad.iter().zip(rect) {
        let p = report.homography.apply(*q).unwrap();
        assert!((p[0] - r[0]).abs() < 1e-6 && (p[1] - r[1]).abs() < 1e-6);
    }

    // A fifth, inconsistent pair shows up in the residual.
    let centre_photo = apply(&to_photo, [0.5, 0.5]);
    let mut noisy = recipe.image.clone();
    noisy.perspective = Some(perspective(Some(([centre_photo[0] + 3.0, centre_photo[1]], [50.0, 50.0]))));
    let report = burnburn_image::prepare(&photo, &noisy, &CancellationToken::never()).unwrap().perspective.unwrap();
    assert!(report.residual_px > 0.5, "residual {}", report.residual_px);

    // Collinear points are refused.
    let mut bad = recipe.image.clone();
    bad.perspective = Some(Perspective {
        points: vec![
            ([0.0, 0.0], [0.0, 0.0]),
            ([10.0, 0.0], [100.0, 0.0]),
            ([20.0, 0.0], [100.0, 100.0]),
            ([30.0, 0.0], [0.0, 100.0]),
        ],
        width: 100,
        height: 100,
    });
    assert_eq!(
        burnburn_image::prepare(&photo, &bad, &CancellationToken::never()).unwrap_err(),
        ImageError::DegeneratePerspective
    );
}

fn blobs() -> SourceImage {
    rasterize(200, 120, S, |x, y| disc(4.0, 6.0, 3.0)(x, y) || disc(12.0, 6.0, 2.5)(x, y) || disc(17.0, 4.0, 1.5)(x, y))
}

#[test]
fn ab_comparison_reports_counts_per_variant_and_identical_inputs_give_identical_geometry_hash() {
    let source = Arc::new(blobs());
    let mut fine = recipe_with(Threshold::Otsu);
    fine.fit.tolerance_mm = 0.01;
    let mut coarse = fine.clone();
    coarse.fit.tolerance_mm = 0.1;
    coarse.fit.curves = false;
    let cmp = compare_recipes(&source, &fine, &coarse, TaskId::from_raw(1), &CancellationToken::never()).unwrap();
    assert_eq!((cmp.a.closed_shapes, cmp.b.closed_shapes), (3, 3));
    assert!(cmp.a.nodes > cmp.b.nodes, "{} vs {}", cmp.a.nodes, cmp.b.nodes);
    assert_eq!(cmp.a.nodes, cmp.a_result.stats.nodes);
    assert_ne!(cmp.a.recipe_key, cmp.b.recipe_key);
    assert_ne!(cmp.a.geometry_hash, cmp.b.geometry_hash);
    assert!(cmp.b.max_deviation_mm <= 0.1 + 1e-9 && cmp.a.max_deviation_mm <= 0.01 + 1e-9);
    assert_eq!((cmp.a_result.width_px, cmp.a_result.scale), (cmp.b_result.width_px, cmp.b_result.scale), "same frame");

    let same = compare_recipes(&source, &fine, &fine, TaskId::from_raw(1), &CancellationToken::never()).unwrap();
    assert_eq!(same.a.geometry_hash, same.b.geometry_hash);
    assert_eq!(same.a, same.b);
    // The hash is of the geometry: an independent trace with the same inputs matches.
    assert_eq!(run(blobs(), fine.clone()).geometry_hash(), same.a.geometry_hash);
}

#[test]
fn batch_reports_problems_per_image_and_keeps_going() {
    let mut recipe = recipe_with(Threshold::Fixed(127));
    recipe.image.crop = Some(PixelRect { x: 0, y: 0, width: 150, height: 100 });
    recipe.contour.min_area_mm2 = 0.5;
    let speckled = rasterize(200, 120, S, |x, y| disc(5.0, 6.0, 2.0)(x, y) || disc(12.0, 9.0, 0.2)(x, y));
    let items = vec![
        BatchItem { name: "good".into(), source: Arc::new(blobs()) },
        BatchItem {
            name: "blank".into(),
            source: Arc::new(SourceImage::from_gray8(200, 120, vec![255; 200 * 120]).unwrap()),
        },
        BatchItem { name: "small".into(), source: Arc::new(SourceImage::from_gray8(50, 50, vec![255; 2500]).unwrap()) },
        BatchItem { name: "speckled".into(), source: Arc::new(speckled) },
    ];
    let report =
        trace_batch(&items, &recipe, TaskId::from_raw(7), RevisionId::INITIAL, &CancellationToken::never()).unwrap();
    let names: Vec<&str> = report.outcomes.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(names, ["good", "blank", "small", "speckled"]);
    assert!(report.outcomes[0].problems.is_empty());
    assert_eq!(report.outcomes[0].summary.as_ref().unwrap().closed_shapes, 2, "third blob is outside the crop");
    assert_eq!(report.outcomes[1].problems, vec![BatchProblem::NoGeometry]);
    assert_eq!(report.outcomes[2].problems, vec![BatchProblem::Failed(TraceError::Image(ImageError::CropOutOfBounds))]);
    assert!(report.outcomes[2].summary.is_none());
    assert_eq!(report.outcomes[3].problems, vec![BatchProblem::RemovedFeatures { count: 1 }]);
    assert_eq!(report.problem_count(), 3);
    assert_eq!(report.recipe_key_by_item[0], Some(recipe.key(blobs().content_hash()).unwrap()));

    let cancel = burnburn_types::CancellationSource::new();
    cancel.cancel();
    assert!(matches!(
        trace_batch(&items, &recipe, TaskId::from_raw(8), RevisionId::INITIAL, &cancel.token()),
        Err(TraceError::Cancelled(_))
    ));
}

/// Scene of the IM19 test: ring with a light counter, rectangle with a faint bump, a faint speck.
fn scene() -> SourceImage {
    let hole = disc(5.0, 6.0, 1.5);
    let ring = disc(5.0, 6.0, 3.5);
    let rect = convex(vec![(11.0, 2.0), (18.0, 2.0), (18.0, 10.0), (11.0, 10.0)]);
    let bump = disc(11.0, 6.0, 0.8);
    let speck = disc(8.8, 2.0, 0.5);
    let far = disc(2.0, 10.5, 0.8);
    rasterize_levels(200, 120, S, &[(&hole, 150), (&ring, 0), (&rect, 0), (&bump, 180), (&speck, 170), (&far, 0)])
}

#[test]
fn old_region_patch_is_not_applied_to_new_unrelated_component() {
    let source = Arc::new(scene());
    let base = |threshold: u8, tolerance: f64| {
        let mut r = recipe_with(Threshold::Fixed(threshold));
        r.fit.tolerance_mm = tolerance;
        let req = TraceRequest {
            task: TaskId::from_raw(1),
            source_revision: RevisionId::INITIAL,
            source: source.clone(),
            recipe: r,
        };
        Arc::new(trace(&req, &CancellationToken::never()).unwrap())
    };
    let a = base(127, 0.05);
    let hole = a.contours.iter().find(|c| c.kind == ContourKind::Hole).unwrap().id;
    let req = CorrectionRequest {
        task: TaskId::from_raw(2),
        previous: a.clone(),
        source: source.clone(),
        // Region around the bump of the rectangle only.
        region: RegionShape::Rect(PixelRect { x: 95, y: 40, width: 30, height: 40 }),
        recipe: recipe_with(Threshold::Fixed(200)),
        anchors: Vec::new(),
        protected: BTreeSet::from([hole]),
        node_target: None,
    };
    let corrected = retrace_region(&req, &CancellationToken::never()).unwrap();
    let record = corrected.corrections[0].clone();
    assert!(!record.touched.is_empty(), "the rectangle was touched");

    // Re-segmentation of a changed image: the rectangle is gone and an unrelated blob now lies
    // inside the old patch region. Contour ids of the new base may coincide with the old ones.
    let new_source = Arc::new(scene_with_new_component());
    let req_b = TraceRequest {
        task: TaskId::from_raw(3),
        source_revision: RevisionId::INITIAL,
        source: new_source.clone(),
        recipe: recipe_with(Threshold::Fixed(127)),
    };
    let b = Arc::new(trace(&req_b, &CancellationToken::never()).unwrap());
    match reapply_correction(&record, &b, &new_source, TaskId::from_raw(4), &CancellationToken::never()).unwrap() {
        Reapplied::NeedsRebind { lost, new_in_region } => {
            assert!(!new_in_region.is_empty() || !lost.is_empty(), "the binding is reported as broken");
            assert!(!new_in_region.is_empty(), "the unrelated component in the region is named");
        }
        Reapplied::Applied(r) => panic!("old patch silently landed on a new component: {:?}", r.corrections),
    }

    // Same components, only a different fit tolerance: the correction still binds and is applied.
    let c = base(127, 0.03);
    match reapply_correction(&record, &c, &source, TaskId::from_raw(5), &CancellationToken::never()).unwrap() {
        Reapplied::Applied(r) => {
            assert_eq!(r.corrections.len(), 1);
            assert_eq!(r.corrections[0].base_inputs, c.recipe_key);
            assert_eq!(r.corrections[0].region, record.region);
            let rect = r.contours.iter().find(|k| record.touched.iter().any(|t| t.id == k.id)).unwrap();
            assert!(rect.area_mm2 > 56.5, "bump added again: {}", rect.area_mm2);
        }
        Reapplied::NeedsRebind { lost, new_in_region } => panic!("should bind: lost {lost:?}, new {new_in_region:?}"),
    }
}

/// The scene with an additional black blob inside the old correction region (x 9.5–12.5 mm,
/// y 4–8 mm) that did not exist before, and without the rectangle.
fn scene_with_new_component() -> SourceImage {
    let hole = disc(5.0, 6.0, 1.5);
    let ring = disc(5.0, 6.0, 3.5);
    let blob = disc(11.0, 6.0, 1.0);
    let far = disc(2.0, 10.5, 0.8);
    rasterize_levels(200, 120, S, &[(&hole, 150), (&ring, 0), (&blob, 0), (&far, 0)])
}
