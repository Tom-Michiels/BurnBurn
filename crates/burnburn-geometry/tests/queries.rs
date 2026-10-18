//! Public contract: nearest point, hit-testing, intersections and snapping (E03, E04).
//! Oracles are analytic (distances to lines and circles, known crossing points).

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use burnburn_geometry::{
    Bounds, HitKind, HitTestTolerance, Path, PathBuilder, Point, Rect, SegmentRef, SnapFilter, SnapKind, SpatialIndex,
    hit_test, snap_candidates,
};
use burnburn_types::Length;

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn tol(mm: f64) -> HitTestTolerance {
    HitTestTolerance::new(Length::from_mm(mm).unwrap()).unwrap()
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Path {
    Path::rectangle(Rect::new(p(x, y), Length::from_mm(w).unwrap(), Length::from_mm(h).unwrap()).unwrap())
}

#[test]
fn nearest_point_on_line_cubic_and_circle_is_analytic() {
    let line = Path::line(p(0.0, 0.0), p(10.0, 0.0)).unwrap();
    let n = line.nearest(p(3.0, 4.0)).unwrap();
    assert_eq!(n.at, SegmentRef { subpath: 0, segment: 0 });
    assert!((n.distance - 4.0).abs() < 1e-12 && (n.t - 0.3).abs() < 1e-12, "{n:?}");

    let circle = Path::circle(p(0.0, 0.0), 10.0).unwrap();
    let c = circle.nearest(p(3.0, 4.0)).unwrap();
    assert!((c.distance - 5.0).abs() < 1e-9, "{c:?}");
    assert!((c.point.x - 6.0).abs() < 1e-7 && (c.point.y - 8.0).abs() < 1e-7, "{c:?}");

    // y(t) = 30t(1−t) peaks at (5, 7.5); straight above it the nearest point is the apex.
    let cubic =
        PathBuilder::new().move_to(p(0.0, 0.0)).cubic_to(p(0.0, 10.0), p(10.0, 10.0), p(10.0, 0.0)).build().unwrap();
    let k = cubic.nearest(p(5.0, 20.0)).unwrap();
    assert!((k.distance - 12.5).abs() < 1e-9 && (k.t - 0.5).abs() < 1e-7, "{k:?}");
}

#[test]
fn nearest_includes_the_implicit_closing_line() {
    // Rectangle (0,0)-(10,10): the closing line runs from (0,10) down to (0,0) = segment 3.
    let r = rect(0.0, 0.0, 10.0, 10.0);
    let n = r.nearest(p(-2.0, 5.0)).unwrap();
    assert_eq!(n.at, SegmentRef { subpath: 0, segment: 3 });
    assert!((n.distance - 2.0).abs() < 1e-12);
}

#[test]
fn hit_test_distinguishes_edge_interior_and_miss() {
    let r = rect(0.0, 0.0, 10.0, 10.0);
    assert!(matches!(r.hit(p(10.2, 5.0), &tol(0.5)), Some(HitKind::Edge(_))));
    assert_eq!(r.hit(p(5.0, 5.0), &tol(0.5)), Some(HitKind::Interior));
    assert_eq!(r.hit(p(11.0, 5.0), &tol(0.5)), None);
    // A hole (clockwise inner contour) is not interior.
    let ring = PathBuilder::new()
        .move_to(p(0.0, 0.0))
        .line_to(p(10.0, 0.0))
        .line_to(p(10.0, 10.0))
        .line_to(p(0.0, 10.0))
        .close()
        .move_to(p(3.0, 3.0))
        .line_to(p(3.0, 7.0))
        .line_to(p(7.0, 7.0))
        .line_to(p(7.0, 3.0))
        .close()
        .build()
        .unwrap();
    assert_eq!(ring.hit(p(5.0, 5.0), &tol(0.5)), None);
    assert_eq!(ring.hit(p(1.5, 5.0), &tol(0.5)), Some(HitKind::Interior));
    // Open paths have no interior.
    let open = Path::polyline(&[p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0)], false).unwrap();
    assert_eq!(open.hit(p(8.0, 2.0), &tol(0.5)), None);
}

#[test]
fn line_line_line_circle_and_cubic_intersections_are_found() {
    let a = Path::line(p(0.0, 0.0), p(10.0, 10.0)).unwrap();
    let b = Path::line(p(0.0, 10.0), p(10.0, 0.0)).unwrap();
    let x = a.intersections(&b);
    assert_eq!(x.len(), 1);
    assert!((x[0].point.x - 5.0).abs() < 1e-12 && (x[0].point.y - 5.0).abs() < 1e-12);
    assert!((x[0].a.1 - 0.5).abs() < 1e-12 && (x[0].b.1 - 0.5).abs() < 1e-12);

    // Horizontal line y = 6 through a circle r = 10: x = ±8.
    let circle = Path::circle(p(0.0, 0.0), 10.0).unwrap();
    let h = Path::line(p(-20.0, 6.0), p(20.0, 6.0)).unwrap();
    let mut xs: Vec<f64> = h.intersections(&circle).iter().map(|i| i.point.x).collect();
    xs.sort_by(f64::total_cmp);
    assert_eq!(xs.len(), 2, "{xs:?}");
    assert!((xs[0] + 8.0).abs() < 1e-9 && (xs[1] - 8.0).abs() < 1e-9, "{xs:?}");

    // Vertical line x = 5 through the cubic apex: one crossing at (5, 7.5).
    let cubic =
        PathBuilder::new().move_to(p(0.0, 0.0)).cubic_to(p(0.0, 10.0), p(10.0, 10.0), p(10.0, 0.0)).build().unwrap();
    let v = Path::line(p(5.0, -1.0), p(5.0, 20.0)).unwrap();
    let c = v.intersections(&cubic);
    assert_eq!(c.len(), 1, "{c:?}");
    assert!((c[0].point.y - 7.5).abs() < 1e-9);

    // Two circles r = 5 at distance 8: crossings at x = 4, y = ±3.
    let c1 = Path::circle(p(0.0, 0.0), 5.0).unwrap();
    let c2 = Path::circle(p(8.0, 0.0), 5.0).unwrap();
    let mut cc: Vec<Point> = c1.intersections(&c2).iter().map(|i| i.point).collect();
    cc.sort_by(|a, b| a.y.total_cmp(&b.y));
    assert_eq!(cc.len(), 2, "{cc:?}");
    assert!((cc[0].x - 4.0).abs() < 1e-7 && (cc[0].y + 3.0).abs() < 1e-7, "{cc:?}");
}

#[test]
fn parallel_and_disjoint_lines_do_not_intersect() {
    let a = Path::line(p(0.0, 0.0), p(10.0, 0.0)).unwrap();
    let b = Path::line(p(0.0, 1.0), p(10.0, 1.0)).unwrap();
    assert!(a.intersections(&b).is_empty());
    let c = Path::line(p(11.0, -1.0), p(11.0, 1.0)).unwrap();
    assert!(a.intersections(&c).is_empty());
}

#[test]
fn self_intersection_of_a_figure_eight_is_located() {
    let bow = Path::polyline(&[p(0.0, 0.0), p(10.0, 10.0), p(10.0, 0.0), p(0.0, 10.0)], true).unwrap();
    let x = bow.self_intersections();
    assert_eq!(x.len(), 1, "{x:?}");
    assert!((x[0].point.x - 5.0).abs() < 1e-12 && (x[0].point.y - 5.0).abs() < 1e-12);
    // A convex polygon has none (shared vertices of neighbours are not crossings).
    assert!(rect(0.0, 0.0, 10.0, 10.0).self_intersections().is_empty());
}

#[test]
fn snap_candidates_name_endpoint_midpoint_center_and_intersection() {
    let line = Path::line(p(0.0, 0.0), p(10.0, 0.0)).unwrap();
    let cross = Path::line(p(5.0, -5.0), p(5.0, 5.0)).unwrap();
    let circle = Path::circle(p(30.0, 0.0), 5.0).unwrap();
    let shapes = [(1u32, &line), (2u32, &cross), (3u32, &circle)];
    let all = SnapFilter::all();

    let end = snap_candidates(&shapes, p(0.2, 0.1), &tol(1.0), &all);
    assert_eq!(end[0].kind, SnapKind::Endpoint);
    assert_eq!((end[0].point, end[0].source), (p(0.0, 0.0), 1));

    let mid = snap_candidates(&shapes, p(5.3, 0.2), &tol(1.0), &all);
    // Midpoint of `line` and the intersection with `cross` coincide at (5,0); both are named.
    let kinds: Vec<SnapKind> = mid.iter().map(|c| c.kind).collect();
    assert!(kinds.contains(&SnapKind::Midpoint) && kinds.contains(&SnapKind::Intersection), "{kinds:?}");
    let inter = mid.iter().find(|c| c.kind == SnapKind::Intersection).unwrap();
    assert_eq!(inter.point, p(5.0, 0.0));
    assert_eq!(inter.other, Some(2));

    let center = snap_candidates(&shapes, p(30.4, 0.3), &tol(1.0), &all);
    assert_eq!(center[0].kind, SnapKind::Center);
    assert!(center[0].point.distance(p(30.0, 0.0)) < 1e-12);
    assert_eq!(center.iter().filter(|c| c.kind == SnapKind::Center).count(), 1, "one center per circle");

    // Nearest is the fallback on a plain edge; the filter suppresses kinds.
    let near = snap_candidates(&shapes, p(2.0, 0.3), &tol(1.0), &all);
    assert_eq!(near[0].kind, SnapKind::Nearest);
    assert!((near[0].point.y).abs() < 1e-12 && (near[0].point.x - 2.0).abs() < 1e-12);
    let none = snap_candidates(&shapes, p(0.2, 0.1), &tol(1.0), &SnapFilter::none());
    assert!(none.is_empty());
    // Out of tolerance: nothing.
    assert!(snap_candidates(&shapes, p(0.0, 3.0), &tol(1.0), &all).is_empty());
}

#[test]
fn spatial_index_returns_exactly_the_overlapping_items() {
    let items: Vec<(usize, Bounds)> = (0..200)
        .map(|i| {
            let x = (i % 20) as f64 * 10.0;
            let y = (i / 20) as f64 * 10.0;
            (i, Bounds::from_corners(p(x, y), p(x + 5.0, y + 5.0)))
        })
        .collect();
    let index = SpatialIndex::build(items.clone());
    assert_eq!(index.len(), 200);
    let q = Bounds::from_corners(p(12.0, 12.0), p(31.0, 21.0));
    let mut got = index.query_rect(&q);
    got.sort_unstable();
    let mut want: Vec<usize> = items.iter().filter(|(_, b)| b.intersects(&q)).map(|(i, _)| *i).collect();
    want.sort_unstable();
    assert_eq!(got, want);
    assert!(!want.is_empty());
    assert!(index.query_point(p(7.0, 7.0), 1.0).is_empty());
    assert_eq!(index.query_point(p(7.0, 7.0), 2.9), vec![0]);
}

#[test]
fn hit_test_orders_edges_by_distance_then_smaller_interiors_first() {
    // A small square inside a big one; a point inside both hits the small one first.
    let big = rect(0.0, 0.0, 100.0, 100.0);
    let small = rect(40.0, 40.0, 10.0, 10.0);
    let line = Path::line(p(0.0, 45.0), p(100.0, 45.0)).unwrap();
    let paths = [big, small, line];
    let index = SpatialIndex::build(paths.iter().enumerate().filter_map(|(i, p)| p.bounds().map(|b| (i, b))));
    let hits = hit_test(&index, |i| paths.get(i), p(42.0, 45.3), &tol(0.5));
    let order: Vec<usize> = hits.iter().map(|h| h.key).collect();
    assert_eq!(order, vec![2, 1, 0], "edge first, then the smaller interior");
    assert!(matches!(hits[0].kind, HitKind::Edge(_)));
}
