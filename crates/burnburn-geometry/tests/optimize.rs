//! W1-L2a Optimize Shapes. The contract is a geometric error budget: every point of the input
//! lies within the tolerance of the output, the output never grows, and a second pass is a no-op.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers and let-else in tests

use burnburn_geometry::{OptimizeError, Path, PathBuilder, Point, ProductionTolerance, Segment};
use burnburn_types::{CancellationSource, CancellationToken, Length};
use proptest::prelude::*;

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn tol(mm: f64) -> ProductionTolerance {
    ProductionTolerance::new(Length::from_mm(mm).unwrap()).unwrap()
}

fn optimize(path: &Path, t: f64) -> burnburn_geometry::OptimizeOutcome {
    path.optimize(&tol(t), &CancellationToken::never()).unwrap()
}

fn nodes(path: &Path) -> usize {
    path.subpaths().iter().map(|s| s.segments().len() + 1).sum()
}

/// Largest distance from sampled points of `from` to the outline of `to`.
fn max_deviation(from: &Path, to: &Path) -> f64 {
    let mut worst: f64 = 0.0;
    for sp in from.subpaths() {
        let mut at = sp.start();
        worst = worst.max(to.nearest(at).map_or(f64::INFINITY, |n| n.distance));
        for seg in sp.segments() {
            for i in 1..=24 {
                let q = seg.point_at(at, f64::from(i) / 24.0);
                worst = worst.max(to.nearest(q).map_or(f64::INFINITY, |n| n.distance));
            }
            at = seg.end();
        }
    }
    worst
}

#[test]
fn collinear_nodes_disappear_and_the_corner_stays() {
    let pts: Vec<Point> = (0..=10).map(|i| p(f64::from(i), 0.0)).chain([p(10.0, 5.0)]).collect();
    let path = Path::polyline(&pts, false).unwrap();
    let out = optimize(&path, 0.01);
    let kept: Vec<Point> = std::iter::once(out.path.subpaths()[0].start())
        .chain(out.path.subpaths()[0].segments().iter().map(Segment::end))
        .collect();
    assert_eq!(kept, vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 5.0)]);
    assert_eq!(out.removed_nodes, 9);
}

#[test]
fn jitter_inside_the_budget_is_removed_and_outside_it_is_kept() {
    let jittery = Path::polyline(&[p(0.0, 0.0), p(5.0, 0.004), p(10.0, 0.0)], false).unwrap();
    assert_eq!(nodes(&optimize(&jittery, 0.01).path), 2);
    let bumpy = Path::polyline(&[p(0.0, 0.0), p(5.0, 0.04), p(10.0, 0.0)], false).unwrap();
    let out = optimize(&bumpy, 0.01);
    assert_eq!((nodes(&out.path), out.removed_nodes), (3, 0));
    assert_eq!(out.path, bumpy, "nothing to do leaves the path bit-identical");
}

#[test]
fn a_nearly_straight_cubic_becomes_a_line_and_a_real_curve_stays() {
    let flat =
        PathBuilder::new().move_to(p(0.0, 0.0)).cubic_to(p(3.0, 0.001), p(7.0, -0.001), p(10.0, 0.0)).build().unwrap();
    let out = optimize(&flat, 0.01);
    assert_eq!(out.path.subpaths()[0].segments(), &[Segment::Line { to: p(10.0, 0.0) }]);
    assert_eq!(out.flattened_curves, 1);
    let curved =
        PathBuilder::new().move_to(p(0.0, 0.0)).cubic_to(p(3.0, 4.0), p(7.0, 4.0), p(10.0, 0.0)).build().unwrap();
    assert_eq!(optimize(&curved, 0.01).path, curved);
}

#[test]
fn flattening_a_curve_and_merging_lines_together_stay_within_one_budget() {
    // A shallow cubic (deviation ≈ 0.045) followed by a long line that is nearly collinear with it.
    let path = PathBuilder::new()
        .move_to(p(0.0, 0.0))
        .cubic_to(p(3.0, 0.06), p(7.0, 0.06), p(10.0, 0.0))
        .line_to(p(20.0, 0.008))
        .build()
        .unwrap();
    for t in [0.05, 0.1, 0.5] {
        let out = optimize(&path, t);
        assert!(max_deviation(&path, &out.path) <= t + 1e-9, "t = {t}: {}", max_deviation(&path, &out.path));
    }
}

#[test]
fn the_start_node_of_a_closed_path_survives_even_when_collinear() {
    // A square whose start sits in the middle of the bottom side, with extra nodes on every side.
    let pts =
        [p(5.0, 0.0), p(10.0, 0.0), p(10.0, 5.0), p(10.0, 10.0), p(5.0, 10.0), p(0.0, 10.0), p(0.0, 5.0), p(0.0, 0.0)];
    let path = Path::polyline(&pts, true).unwrap();
    let out = optimize(&path, 0.01);
    let sp = &out.path.subpaths()[0];
    assert_eq!(sp.start(), p(5.0, 0.0), "the explicit start point is a user choice");
    assert!(sp.is_closed());
    assert_eq!(nodes(&out.path), 5, "the start node and the four corners; the closing line is implicit");
    assert!((out.path.signed_area() - 100.0).abs() < 1e-9);
}

#[test]
fn arcs_circles_and_tiny_contours_are_left_alone() {
    let circle = Path::circle(p(0.0, 0.0), 5.0).unwrap();
    let out = optimize(&circle, 0.01);
    assert_eq!((out.path, out.removed_nodes), (circle, 0));
    // A contour smaller than the budget is not deleted nor reduced below a triangle.
    let tiny = Path::polyline(&[p(0.0, 0.0), p(0.004, 0.0), p(0.004, 0.004), p(0.0, 0.004)], true).unwrap();
    let out = optimize(&tiny, 0.01);
    assert_eq!(out.path, tiny);
}

#[test]
fn holes_and_separate_subpaths_are_optimised_independently() {
    let outer = Path::polyline(&[p(0.0, 0.0), p(5.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)], true).unwrap();
    let hole = Path::polyline(&[p(2.0, 2.0), p(2.0, 5.0), p(2.0, 8.0), p(8.0, 8.0), p(8.0, 2.0)], true).unwrap();
    let both = Path::from_subpaths(vec![outer.subpaths()[0].clone(), hole.subpaths()[0].clone()]);
    let out = optimize(&both, 0.01);
    assert_eq!(out.path.subpaths().len(), 2);
    assert_eq!(out.removed_nodes, 2);
    assert!((out.path.signed_area() - both.signed_area()).abs() < 1e-9);
}

#[test]
fn coincident_and_repeated_points_collapse() {
    let dup = Path::polyline(&[p(0.0, 0.0), p(0.0, 0.0), p(10.0, 0.0), p(10.0, 0.0), p(10.0, 10.0)], false).unwrap();
    let out = optimize(&dup, 0.001);
    assert_eq!(nodes(&out.path), 3);
}

#[test]
fn a_cancelled_token_stops_the_work() {
    let source = CancellationSource::new();
    source.cancel();
    let path = Path::polyline(&[p(0.0, 0.0), p(1.0, 0.0), p(2.0, 0.0)], false).unwrap();
    assert_eq!(path.optimize(&tol(0.01), &source.token()), Err(OptimizeError::Cancelled));
}

fn arb_points(n: std::ops::Range<usize>) -> impl Strategy<Value = Vec<Point>> {
    proptest::collection::vec((-50.0f64..50.0, -50.0f64..50.0), n)
        .prop_map(|v| v.into_iter().map(|(x, y)| p(x, y)).collect())
}

proptest! {
    /// Points inserted on the segments of a polyline are collinear noise: they always go.
    #[test]
    fn nodes_inserted_on_straight_segments_always_disappear(corners in arb_points(2..12), extra in 1usize..6) {
        let mut pts = vec![corners[0]];
        for w in corners.windows(2) {
            for i in 1..=extra {
                pts.push(w[0].lerp(w[1], i as f64 / (extra + 1) as f64));
            }
            pts.push(w[1]);
        }
        let path = Path::polyline(&pts, false).unwrap();
        let out = optimize(&path, 1e-6);
        prop_assert!(nodes(&out.path) <= corners.len(), "{} > {}", nodes(&out.path), corners.len());
        prop_assert!(max_deviation(&path, &out.path) <= 1e-6 + 1e-9);
    }

    #[test]
    fn open_polylines_stay_within_budget_never_grow_and_are_idempotent(pts in arb_points(2..40), t in 0.01f64..8.0) {
        let path = Path::polyline(&pts, false).unwrap();
        let out = optimize(&path, t);
        prop_assert!(max_deviation(&path, &out.path) <= t + 1e-9);
        prop_assert!(nodes(&out.path) <= nodes(&path));
        prop_assert_eq!(nodes(&path) - nodes(&out.path), out.removed_nodes);
        // Ends are kept and the output only uses input vertices.
        let sp = &out.path.subpaths()[0];
        prop_assert_eq!((sp.start(), sp.end()), (pts[0], *pts.last().unwrap()));
        for seg in sp.segments() {
            prop_assert!(pts.contains(&seg.end()));
        }
        let again = optimize(&out.path, t);
        prop_assert_eq!(again.removed_nodes, 0);
    }

    #[test]
    fn closed_polygons_stay_within_budget_and_keep_at_least_three_nodes(pts in arb_points(3..40), t in 0.01f64..8.0) {
        let path = Path::polyline(&pts, true).unwrap();
        let out = optimize(&path, t);
        prop_assert!(max_deviation(&path, &out.path) <= t + 1e-9);
        let sp = &out.path.subpaths()[0];
        prop_assert!(sp.is_closed() && sp.start() == pts[0] && sp.segments().len() >= 2);
        prop_assert!(nodes(&out.path) <= nodes(&path));
    }
}
