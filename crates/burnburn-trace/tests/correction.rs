//! IM19 local trace correction: a region is re-traced with its own recipe; everything outside the
//! region keeps identity and bit-identical segments, protected details never change, corrections
//! carry their source identity (region + recipe + base).

#![allow(clippy::unwrap_used)]

mod support;

use std::collections::BTreeSet;
use std::sync::Arc;

use burnburn_geometry::{Path, Point, Segment};
use burnburn_image::{ImageRecipe, PixelRect, PixelScale, RegionShape, SourceImage, Threshold};
use burnburn_trace::{
    ContourKind, ContourOrigin, CorrectionRequest, TraceRecipe, TraceRequest, TraceResult, TracedContour,
    correction_chain_key, retrace_region, trace,
};
use burnburn_types::{CancellationToken, CanonicalWriter, RevisionId, StableId, TaskId};
use support::*;

const S: f64 = 0.1;
const W: u32 = 200;
const H: u32 = 120;
/// Region in adjusted pixels: x 30..120, y 40..110 (mm x 3..12, y 1..8).
const REGION: PixelRect = PixelRect { x: 30, y: 40, width: 90, height: 70 };

/// Ring with a light-gray counter (150), a rectangle with a faint bump (180) on its left edge, a
/// faint speck (170) and a small black disc far from the region.
fn scene() -> SourceImage {
    let hole = disc(5.0, 6.0, 1.5);
    let ring = disc(5.0, 6.0, 3.5);
    let rect = convex(vec![(11.0, 2.0), (18.0, 2.0), (18.0, 10.0), (11.0, 10.0)]);
    let bump = disc(11.0, 6.0, 0.8);
    let speck = disc(8.8, 2.0, 0.5);
    let far = disc(2.0, 10.5, 0.8);
    rasterize_levels(W, H, S, &[(&hole, 150), (&ring, 0), (&rect, 0), (&bump, 180), (&speck, 170), (&far, 0)])
}

fn recipe(threshold: u8) -> TraceRecipe {
    TraceRecipe {
        image: ImageRecipe {
            scale: Some(PixelScale::uniform_mm_per_pixel(S).unwrap()),
            threshold: Threshold::Fixed(threshold),
            ..ImageRecipe::default()
        },
        ..TraceRecipe::default()
    }
}

fn base(source: &Arc<SourceImage>) -> TraceResult {
    let req = TraceRequest {
        task: TaskId::from_raw(1),
        source_revision: RevisionId::INITIAL,
        source: source.clone(),
        recipe: recipe(127),
    };
    trace(&req, &CancellationToken::never()).unwrap()
}

fn correction(
    previous: &Arc<TraceResult>,
    source: &Arc<SourceImage>,
    protected: BTreeSet<burnburn_trace::ContourId>,
) -> CorrectionRequest {
    CorrectionRequest {
        task: TaskId::from_raw(2),
        previous: previous.clone(),
        source: source.clone(),
        region: RegionShape::Rect(REGION),
        recipe: recipe(200),
        anchors: Vec::new(),
        protected,
        node_target: None,
    }
}

fn bytes(c: &TracedContour) -> Vec<u8> {
    let mut w = CanonicalWriter::new();
    burnburn_types::CanonicalEncode::encode(&Path::from_subpaths(vec![c.path.clone()]), &mut w);
    w.finish().unwrap()
}

fn to_px(p: Point) -> (f64, f64) {
    (p.x / S, f64::from(H) - p.y / S)
}

/// Distance (px) of a pixel-space point outside the region rectangle; 0 inside.
fn outside_by(p: (f64, f64)) -> f64 {
    let (x0, y0) = (f64::from(REGION.x), f64::from(REGION.y));
    let (x1, y1) = (x0 + f64::from(REGION.width), y0 + f64::from(REGION.height));
    let dx = (x0 - p.0).max(p.0 - x1).max(0.0);
    let dy = (y0 - p.1).max(p.1 - y1).max(0.0);
    dx.hypot(dy)
}

fn far_from_region(points: &[Point]) -> bool {
    points.iter().all(|&p| outside_by(to_px(p)) > 1.5)
}

/// (start, segment) pairs of a closed contour whose source span lies entirely away from the region.
fn segments_outside(c: &TracedContour) -> Vec<(Point, Segment)> {
    let n = c.source_points.len();
    let mut from = c.path.start();
    let mut out = Vec::new();
    for (seg, &(a, b)) in c.path.segments().iter().zip(&c.segment_spans) {
        let span: Vec<Point> = (a..=b).map(|i| c.source_points[i % n]).collect();
        if far_from_region(&span) {
            out.push((from, *seg));
        }
        from = seg.end();
    }
    out
}

fn all_segments(c: &TracedContour) -> Vec<(Point, Segment)> {
    let mut from = c.path.start();
    c.path
        .segments()
        .iter()
        .map(|s| {
            let pair = (from, *s);
            from = s.end();
            pair
        })
        .collect()
}

fn centroid(c: &TracedContour) -> Point {
    let n = c.source_points.len() as f64;
    Point::new(
        c.source_points.iter().map(|p| p.x).sum::<f64>() / n,
        c.source_points.iter().map(|p| p.y).sum::<f64>() / n,
    )
}

fn find(r: &TraceResult, kind: ContourKind, near: Point) -> &TracedContour {
    r.contours
        .iter()
        .filter(|c| c.kind == kind)
        .min_by(|a, b| centroid(a).distance(near).total_cmp(&centroid(b).distance(near)))
        .unwrap()
}

#[test]
fn retrace_region_preserves_protected_hole_and_untouched_segments() {
    let source = Arc::new(scene());
    let base = Arc::new(base(&source));
    assert_eq!((base.stats.closed_shapes, base.stats.holes), (3, 1), "base: ring, rectangle, far disc; one counter");
    let hole = find(&base, ContourKind::Hole, Point::new(5.0, 6.0)).clone();
    let ring = find(&base, ContourKind::Outer, Point::new(5.0, 6.0)).clone();
    let rect = find(&base, ContourKind::Outer, Point::new(14.5, 6.0)).clone();
    let far = find(&base, ContourKind::Outer, Point::new(2.0, 10.5)).clone();
    assert!(far_from_region(&far.source_points));

    let corrected =
        retrace_region(&correction(&base, &source, BTreeSet::from([hole.id])), &CancellationToken::never()).unwrap();

    // Untouched contour: same identity, bit-identical geometry.
    let far_now = corrected.contour(far.id).expect("far disc keeps its id");
    assert_eq!(bytes(far_now), bytes(&far), "contour outside the region is bit-identical");
    assert_eq!(far_now.origin, ContourOrigin::Trace);

    // Protected hole inside the region: unchanged although the local threshold would fill it.
    let hole_now = corrected.contour(hole.id).expect("protected hole keeps its id");
    assert_eq!(hole_now.kind, ContourKind::Hole);
    assert_eq!(bytes(hole_now), bytes(&hole), "protected hole is bit-identical");
    assert_eq!(corrected.stats.holes, 1);

    // Contours crossing the region keep their id; segments away from the region stay bit-identical.
    for old in [&rect, &ring] {
        let now = corrected.contour(old.id).expect("crossing contour keeps its id");
        let kept = segments_outside(old);
        assert!(!kept.is_empty());
        let now_segments = all_segments(now);
        for pair in &kept {
            assert!(now_segments.contains(pair), "segment outside the region must be bit-identical: {pair:?}");
        }
        assert_eq!(now.origin, ContourOrigin::Correction(corrected.corrections[0].id));
    }
    let rect_now = corrected.contour(rect.id).unwrap();
    assert!(rect_now.area_mm2 > rect.area_mm2 + 0.5, "the bump inside the region was added");

    // New component (the faint speck) gets a fresh id above all previous ones and carries the correction.
    let record = &corrected.corrections[0];
    let speck = find(&corrected, ContourKind::Outer, Point::new(8.8, 2.0));
    assert!(centroid(speck).distance(Point::new(8.8, 2.0)) < 0.2);
    assert!(speck.id.0 >= base.next_contour_id, "new contours never reuse ids");
    assert_eq!(speck.origin, ContourOrigin::Correction(record.id));
    assert_eq!(record.created, vec![speck.id]);

    // Source identity of the correction.
    assert_eq!(record.region, RegionShape::Rect(REGION));
    assert_eq!(record.local_key, recipe(200).key(source.content_hash()).unwrap());
    assert_eq!(record.base_inputs, base.recipe_key);
    assert_ne!(corrected.recipe_key, base.recipe_key);
    assert_eq!(corrected.recipe_key, correction_chain_key(base.base_key, &corrected.corrections).unwrap());
    assert_eq!(corrected.base_key, base.base_key);

    // Control: without protection the same local threshold closes the counter.
    let unprotected =
        retrace_region(&correction(&base, &source, BTreeSet::new()), &CancellationToken::never()).unwrap();
    assert_eq!(unprotected.stats.holes, 0, "protection is what keeps the hole");
    assert!(unprotected.contour(hole.id).is_none());
    assert!(unprotected.corrections[0].removed.contains(&hole.id));
}

#[test]
fn anchor_in_the_region_becomes_an_exact_node_of_the_refit() {
    let source = Arc::new(scene());
    let base = Arc::new(base(&source));
    let rect = find(&base, ContourKind::Outer, Point::new(14.5, 6.0)).clone();
    // On the corrected left edge of the rectangle (bump at x = 11 mm − 0.8 mm), pixel coordinates.
    let anchor_mm = Point::new(10.2, 6.0);
    let mut req = correction(&base, &source, BTreeSet::new());
    req.anchors = vec![burnburn_trace::Anchor { at_px: [anchor_mm.x / S, f64::from(H) - anchor_mm.y / S] }];
    let corrected = retrace_region(&req, &CancellationToken::never()).unwrap();
    let now = corrected.contour(rect.id).unwrap();
    let nodes: Vec<Point> = now.path.segments().iter().map(|s| s.end()).collect();
    let nearest = nodes.iter().map(|p| p.distance(anchor_mm)).fold(f64::INFINITY, f64::min);
    assert!(nearest < 1e-9, "anchor must be a node exactly ({nearest} mm)");
    assert_eq!(corrected.corrections[0].anchors, req.anchors);
    // Without the anchor the refit has no node there.
    let free = retrace_region(&correction(&base, &source, BTreeSet::new()), &CancellationToken::never()).unwrap();
    let free_nearest = free
        .contour(rect.id)
        .unwrap()
        .path
        .segments()
        .iter()
        .map(|s| s.end().distance(anchor_mm))
        .fold(f64::INFINITY, f64::min);
    assert!(free_nearest > 1e-6);
}

#[test]
fn node_target_raises_tolerance_only_in_refitted_parts_and_reports_the_outcome() {
    let source = Arc::new(scene());
    let base = Arc::new(base(&source));
    let free = retrace_region(&correction(&base, &source, BTreeSet::new()), &CancellationToken::never()).unwrap();
    let far = find(&base, ContourKind::Outer, Point::new(2.0, 10.5)).clone();

    let mut req = correction(&base, &source, BTreeSet::new());
    req.node_target = Some(burnburn_trace::NodeTarget { max_nodes: free.stats.nodes - 4, max_tolerance_mm: 0.4 });
    let reduced = retrace_region(&req, &CancellationToken::never()).unwrap();
    let outcome = reduced.corrections[0].node_outcome.unwrap();
    assert!(outcome.met, "{outcome:?}");
    assert_eq!(outcome.nodes, reduced.stats.nodes);
    assert!(reduced.stats.nodes <= free.stats.nodes - 4);
    assert!(outcome.tolerance_mm > 0.05 && outcome.tolerance_mm <= 0.4);
    assert_eq!(bytes(reduced.contour(far.id).unwrap()), bytes(&far), "untouched contours are not simplified");

    // Unreachable target: reported as not met, geometry still within the maximum tolerance.
    req.node_target = Some(burnburn_trace::NodeTarget { max_nodes: 3, max_tolerance_mm: 0.2 });
    let capped = retrace_region(&req, &CancellationToken::never()).unwrap();
    let outcome = capped.corrections[0].node_outcome.unwrap();
    assert!(!outcome.met);
    assert!(outcome.tolerance_mm <= 0.2);
}

#[test]
fn deviation_map_localises_the_largest_fitting_error() {
    let source = Arc::new(scene());
    let mut r = recipe(127);
    r.fit.tolerance_mm = 0.08;
    let req = TraceRequest { task: TaskId::from_raw(1), source_revision: RevisionId::INITIAL, source, recipe: r };
    let result = trace(&req, &CancellationToken::never()).unwrap();
    let map = burnburn_trace::deviation_map(&result).unwrap();
    let preview = burnburn_trace::comparison_preview(&result).unwrap();
    assert_eq!(map.cells_mm.len(), (W * H) as usize);
    assert_eq!(map.segments.len(), result.stats.nodes, "one entry per fitted segment");
    let cell_max = map.cells_mm.iter().copied().fold(0.0f32, f32::max);
    assert!((f64::from(cell_max) - preview.max_deviation_mm).abs() < 1e-6);
    assert!(cell_max > 0.0 && f64::from(cell_max) <= 0.08 + 1e-9);
    let worst = map.worst.unwrap();
    assert!((worst.max_mm - preview.max_deviation_mm).abs() < 1e-12);
    let (x, y) = to_px(worst.worst_at);
    let cell = map.cells_mm[(y.floor() as u32 * W + x.floor() as u32) as usize];
    assert_eq!(cell, cell_max, "the worst point is in the hottest cell");
    // Cells far from every boundary are zero.
    assert_eq!(map.cells_mm[(60 * W + 145) as usize], 0.0, "inside the rectangle");
    assert_eq!(result.contour(worst.contour_id).map(|c| c.id), Some(worst.contour_id));
}

#[test]
fn cancelled_correction_returns_cancelled_and_mismatched_frame_is_refused() {
    let source = Arc::new(scene());
    let base = Arc::new(base(&source));
    let cancel = burnburn_types::CancellationSource::new();
    cancel.cancel();
    let err = retrace_region(&correction(&base, &source, BTreeSet::new()), &cancel.token()).unwrap_err();
    assert!(matches!(err, burnburn_trace::TraceError::Cancelled(_)), "{err:?}");

    let mut req = correction(&base, &source, BTreeSet::new());
    req.recipe.image.scale = Some(PixelScale::uniform_mm_per_pixel(0.2).unwrap());
    assert_eq!(
        retrace_region(&req, &CancellationToken::never()).unwrap_err(),
        burnburn_trace::TraceError::CorrectionMismatch("image.scale")
    );

    let mut req = correction(&base, &source, BTreeSet::from([burnburn_trace::ContourId(99)]));
    req.protected.insert(burnburn_trace::ContourId(99));
    assert_eq!(
        retrace_region(&req, &CancellationToken::never()).unwrap_err(),
        burnburn_trace::TraceError::UnknownContour(burnburn_trace::ContourId(99))
    );
    // The correction's key makes an older result stale for the corrected inputs.
    let corrected = retrace_region(&correction(&base, &source, BTreeSet::new()), &CancellationToken::never()).unwrap();
    assert!(base.check_fresh(TaskId::from_raw(1), corrected.recipe_key).is_err());
    assert!(corrected.check_fresh(TaskId::from_raw(2), corrected.recipe_key).is_ok());
}
