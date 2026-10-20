//! W2-L2 Warp 4 points and Deform 16 points. Oracles: corners map to corners, closed forms of the
//! maps at their centre and along their edges, exact images of straight lines, affine grids that must
//! equal an affine transform, and an independent dense sampling that checks the error budget.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use burnburn_geometry::{
    Affine, Bounds, DeformGrid, Path, PathBuilder, Point, PointMap, ProductionTolerance, QuadWarp, Rect, Segment,
    Vector, WarpError, WarpKind, warp_path,
};
use burnburn_types::{Angle, CancellationSource, CancellationToken, Length};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn tol(v: f64) -> ProductionTolerance {
    ProductionTolerance::new(mm(v)).unwrap()
}

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn near(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() < eps
}

fn near_pt(a: Point, b: Point, eps: f64) -> bool {
    near(a.x, b.x, eps) && near(a.y, b.y, eps)
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Path {
    Path::rectangle(Rect::new(p(x, y), mm(w), mm(h)).unwrap())
}

fn src() -> Bounds {
    Bounds::from_corners(p(0.0, 0.0), p(100.0, 50.0))
}

fn never() -> CancellationToken {
    CancellationToken::never()
}

/// Distance from `q` to the polyline approximation of `path`.
fn distance_to(path: &Path, q: Point) -> f64 {
    let mut best = f64::INFINITY;
    for pl in path.flatten(&tol(0.0002)) {
        let n = pl.points.len();
        let count = if pl.closed { n } else { n.saturating_sub(1) };
        for i in 0..count {
            let (a, b) = (pl.points[i], pl.points[(i + 1) % n]);
            let ab = b - a;
            let t = if ab.dot(ab) > 0.0 { ((q - a).dot(ab) / ab.dot(ab)).clamp(0.0, 1.0) } else { 0.0 };
            best = best.min(q.distance(a + ab * t));
        }
    }
    best
}

/// Points on the original path, taken from the exact segments (no flattening of the original).
fn samples(path: &Path) -> Vec<Point> {
    let mut out = Vec::new();
    for sp in path.subpaths() {
        let mut from = sp.start();
        let mut segments: Vec<Segment> = sp.segments().to_vec();
        if sp.is_closed() && sp.end() != sp.start() {
            segments.push(Segment::Line { to: sp.start() });
        }
        for seg in &segments {
            for k in 0..400 {
                out.push(seg.point_at(from, f64::from(k) / 400.0));
            }
            from = seg.end();
        }
    }
    out
}

/// The warped path must stay within `budget` of the exact image of every sampled point.
fn assert_within_budget(original: &Path, warped: &Path, map: &impl PointMap, budget: f64) {
    for q in samples(original) {
        let image = map.eval(q).unwrap();
        let d = distance_to(warped, image);
        assert!(d <= budget + 0.0003, "image of {q:?} is {d} from the warped path (budget {budget})");
    }
}

fn vertices(path: &Path) -> Vec<Point> {
    path.subpaths()[0].segments().iter().map(Segment::end).collect()
}

fn skew_quad() -> [Point; 4] {
    // A convex but irregular quadrilateral around the source box.
    [p(5.0, -3.0), p(120.0, 10.0), p(90.0, 70.0), p(-10.0, 40.0)]
}

fn shoelace(q: &[Point; 4]) -> f64 {
    (0..4).map(|i| q[i].to_vector().cross(q[(i + 1) % 4].to_vector())).sum::<f64>() / 2.0
}

#[test]
fn the_corners_of_the_box_go_to_the_corners_of_the_quadrilateral_in_both_kinds() {
    for kind in [WarpKind::Projective, WarpKind::Bilinear] {
        let q = skew_quad();
        let warp = QuadWarp::new(src(), q, kind).unwrap();
        let warped = warp_path(&rect(0.0, 0.0, 100.0, 50.0), &warp, &tol(0.001), &never()).unwrap();
        // The edges of the box stay straight in both kinds, so the result is exactly the quadrilateral.
        assert_eq!(warped.subpaths().len(), 1);
        let sp = &warped.subpaths()[0];
        assert!(sp.is_closed());
        assert!(near_pt(sp.start(), q[0], 1e-9), "{kind:?}: {:?}", sp.start());
        let v = vertices(&warped);
        assert!(near_pt(v[0], q[1], 1e-9) && near_pt(v[1], q[2], 1e-9) && near_pt(v[2], q[3], 1e-9), "{kind:?}: {v:?}");
        assert!(sp.segments().iter().all(|s| matches!(s, Segment::Line { .. })), "{kind:?}: edges stay lines");
        assert!(near(warped.signed_area(), shoelace(&q), 1e-6), "{kind:?}");
    }
}

#[test]
fn the_centre_of_the_box_goes_where_each_kind_puts_it() {
    let q = skew_quad();
    // Bilinear: the average of the corners. Projective: where the diagonals of the quadrilateral cross.
    let mid = |warp: &QuadWarp| warp.eval(p(50.0, 25.0)).unwrap();
    let bilinear = mid(&QuadWarp::new(src(), q, WarpKind::Bilinear).unwrap());
    let average = p(q.iter().map(|c| c.x).sum::<f64>() / 4.0, q.iter().map(|c| c.y).sum::<f64>() / 4.0);
    assert!(near_pt(bilinear, average, 1e-9), "{bilinear:?}");
    let projective = mid(&QuadWarp::new(src(), q, WarpKind::Projective).unwrap());
    // Intersection of q0–q2 and q1–q3 by Cramer's rule.
    let (d1, d2) = (q[2] - q[0], q[3] - q[1]);
    let t = (q[1] - q[0]).cross(d2) / d1.cross(d2);
    let crossing = q[0] + d1 * t;
    assert!(near_pt(projective, crossing, 1e-9), "{projective:?} vs {crossing:?}");
}

#[test]
fn a_projective_warp_keeps_a_diagonal_straight_and_a_bilinear_one_bends_it() {
    let q = skew_quad();
    let diagonal = Path::line(p(0.0, 0.0), p(100.0, 50.0)).unwrap();
    let projective = QuadWarp::new(src(), q, WarpKind::Projective).unwrap();
    let w = warp_path(&diagonal, &projective, &tol(0.001), &never()).unwrap();
    assert_eq!(w.subpaths()[0].segments().len(), 1);
    assert!(matches!(w.subpaths()[0].segments()[0], Segment::Line { to } if near_pt(to, q[2], 1e-9)));

    let bilinear = QuadWarp::new(src(), q, WarpKind::Bilinear).unwrap();
    let w = warp_path(&diagonal, &bilinear, &tol(0.001), &never()).unwrap();
    assert!(w.subpaths()[0].segments().iter().any(|s| matches!(s, Segment::Cubic { .. })), "{w:?}");
    assert_within_budget(&diagonal, &w, &bilinear, 0.001);
    let centre = bilinear.eval(p(50.0, 25.0)).unwrap();
    assert!(distance_to(&w, centre) < 0.001);
}

#[test]
fn a_line_of_constant_height_stays_one_straight_segment_under_a_bilinear_warp() {
    let q = skew_quad();
    let bilinear = QuadWarp::new(src(), q, WarpKind::Bilinear).unwrap();
    let mid_line = Path::line(p(0.0, 25.0), p(100.0, 25.0)).unwrap();
    let w = warp_path(&mid_line, &bilinear, &tol(0.001), &never()).unwrap();
    // v = 0.5: the line runs between the midpoints of the left and right edges of the quadrilateral.
    let left = p((q[0].x + q[3].x) / 2.0, (q[0].y + q[3].y) / 2.0);
    let right = p((q[1].x + q[2].x) / 2.0, (q[1].y + q[2].y) / 2.0);
    assert!(near_pt(w.subpaths()[0].start(), left, 1e-9));
    assert!(matches!(w.subpaths()[0].segments(), [Segment::Line { to }] if near_pt(*to, right, 1e-9)), "{w:?}");
}

#[test]
fn an_identity_quadrilateral_and_a_translated_one_leave_the_shape_alone() {
    let b = src();
    let corners = [p(0.0, 0.0), p(100.0, 0.0), p(100.0, 50.0), p(0.0, 50.0)];
    let circle = Path::circle(p(50.0, 25.0), 20.0).unwrap();
    for kind in [WarpKind::Projective, WarpKind::Bilinear] {
        let same = QuadWarp::new(b, corners, kind).unwrap();
        let w = warp_path(&circle, &same, &tol(0.001), &never()).unwrap();
        assert_within_budget(&circle, &w, &same, 0.001);
        for q in samples(&circle).into_iter().step_by(37) {
            assert!(distance_to(&w, q) < 0.001, "{kind:?}");
        }
        let moved = corners.map(|c| c + Vector::new(30.0, -4.0));
        let shifted = QuadWarp::new(b, moved, kind).unwrap();
        let w = warp_path(&circle, &shifted, &tol(0.001), &never()).unwrap();
        let want = circle.transform(&Affine::translate(Vector::new(30.0, -4.0)).unwrap());
        for q in samples(&want).into_iter().step_by(37) {
            assert!(distance_to(&w, q) < 0.002, "{kind:?}");
        }
    }
}

#[test]
fn a_curved_path_stays_within_the_error_budget_under_a_strong_perspective() {
    let q = [p(0.0, 0.0), p(100.0, 0.0), p(70.0, 50.0), p(30.0, 50.0)];
    let warp = QuadWarp::new(src(), q, WarpKind::Projective).unwrap();
    let circle = Path::circle(p(50.0, 25.0), 22.0).unwrap();
    for budget in [0.05, 0.005, 0.0005] {
        let w = warp_path(&circle, &warp, &tol(budget), &never()).unwrap();
        assert_within_budget(&circle, &w, &warp, budget);
    }
    let coarse = warp_path(&circle, &warp, &tol(0.05), &never()).unwrap();
    let fine = warp_path(&circle, &warp, &tol(0.0005), &never()).unwrap();
    assert!(coarse.subpaths()[0].segments().len() < fine.subpaths()[0].segments().len());
}

#[test]
fn bad_quadrilaterals_sources_and_paths_are_typed_errors() {
    let corners = [p(0.0, 0.0), p(100.0, 0.0), p(100.0, 50.0), p(0.0, 50.0)];
    let flat = Bounds::from_corners(p(0.0, 0.0), p(100.0, 0.0));
    assert_eq!(QuadWarp::new(flat, corners, WarpKind::Projective), Err(WarpError::DegenerateSource));
    let line = [p(0.0, 0.0), p(1.0, 0.0), p(2.0, 0.0), p(3.0, 0.0)];
    assert_eq!(QuadWarp::new(src(), line, WarpKind::Bilinear), Err(WarpError::DegenerateQuad));
    // A dart (one reflex corner) and a bow tie.
    let dart = [p(0.0, 0.0), p(100.0, 0.0), p(40.0, 10.0), p(0.0, 50.0)];
    assert_eq!(QuadWarp::new(src(), dart, WarpKind::Projective), Err(WarpError::NotConvex));
    let bow = [p(0.0, 0.0), p(100.0, 50.0), p(100.0, 0.0), p(0.0, 50.0)];
    assert_eq!(QuadWarp::new(src(), bow, WarpKind::Bilinear), Err(WarpError::NotConvex));
    let nan = [p(f64::NAN, 0.0), corners[1], corners[2], corners[3]];
    assert_eq!(QuadWarp::new(src(), nan, WarpKind::Bilinear), Err(WarpError::NotFinite));
    // A mirrored (clockwise) quadrilateral is a valid warp that flips the shape.
    let mirrored = [corners[1], corners[0], corners[3], corners[2]];
    let m = QuadWarp::new(src(), mirrored, WarpKind::Bilinear).unwrap();
    let w = warp_path(&rect(0.0, 0.0, 100.0, 50.0), &m, &tol(0.001), &never()).unwrap();
    assert!(near(w.signed_area(), -5000.0, 1e-6), "{}", w.signed_area());

    let ok = QuadWarp::new(src(), corners, WarpKind::Projective).unwrap();
    assert_eq!(warp_path(&rect(90.0, 0.0, 30.0, 10.0), &ok, &tol(0.001), &never()), Err(WarpError::OutsideDomain));
    // A path exactly on the border of the box is inside.
    assert!(warp_path(&rect(0.0, 0.0, 100.0, 50.0), &ok, &tol(0.001), &never()).is_ok());
}

#[test]
fn cancellation_and_an_unreachable_budget_are_reported_not_hidden() {
    let corners = [p(0.0, 0.0), p(100.0, 0.0), p(70.0, 50.0), p(30.0, 50.0)];
    let warp = QuadWarp::new(src(), corners, WarpKind::Projective).unwrap();
    let circle = Path::circle(p(50.0, 25.0), 22.0).unwrap();
    let source = CancellationSource::new();
    let token = source.token();
    source.cancel();
    assert_eq!(warp_path(&circle, &warp, &tol(0.001), &token), Err(WarpError::Cancelled));
    // Below the resolution of the arithmetic the budget cannot be met.
    assert_eq!(warp_path(&circle, &warp, &tol(1e-16), &never()), Err(WarpError::BudgetExceeded));
}

// ---- Deform 16 ----

/// The regular net over `src()`.
fn regular() -> [[Point; 4]; 4] {
    let b = src();
    let mut net = [[Point::ORIGIN; 4]; 4];
    for (j, row) in net.iter_mut().enumerate() {
        for (i, c) in row.iter_mut().enumerate() {
            *c = p(b.min().x + b.width() * i as f64 / 3.0, b.min().y + b.height() * j as f64 / 3.0);
        }
    }
    net
}

#[test]
fn the_regular_net_is_the_identity_and_an_affine_net_is_that_affine_map() {
    let identity = DeformGrid::identity(src());
    assert_eq!(identity.net(), &regular());
    let shape = Path::circle(p(50.0, 25.0), 20.0).unwrap();
    let w = warp_path(&shape, &identity, &tol(0.001), &never()).unwrap();
    assert_within_budget(&shape, &w, &identity, 0.001);
    assert!(near_pt(identity.eval(p(13.0, 41.0)).unwrap(), p(13.0, 41.0), 1e-9));

    // A net built from an affine image of the regular grid reproduces that image exactly.
    let affine = Affine::scale(1.5, 0.8)
        .unwrap()
        .then(&Affine::rotate(Angle::from_degrees(25.0).unwrap()))
        .then(&Affine::translate(Vector::new(7.0, -9.0)).unwrap());
    let mut net = regular();
    for row in &mut net {
        for c in row.iter_mut() {
            *c = affine.apply(*c);
        }
    }
    let grid = DeformGrid::new(src(), net).unwrap();
    for q in [p(0.0, 0.0), p(100.0, 50.0), p(33.0, 17.0), p(81.0, 4.0)] {
        assert!(near_pt(grid.eval(q).unwrap(), affine.apply(q), 1e-9), "{q:?}");
    }
    let w = warp_path(&shape, &grid, &tol(0.001), &never()).unwrap();
    let want = shape.transform(&affine);
    for q in samples(&want).into_iter().step_by(23) {
        assert!(distance_to(&w, q) < 0.002);
    }
}

/// De Casteljau on four points, independent of the implementation's Bernstein form.
fn de_casteljau(c: [Point; 4], t: f64) -> Point {
    let l = |a: Point, b: Point| a.lerp(b, t);
    let (ab, bc, cd) = (l(c[0], c[1]), l(c[1], c[2]), l(c[2], c[3]));
    l(l(ab, bc), l(bc, cd))
}

fn bumped() -> DeformGrid {
    let mut net = regular();
    net[1][1] = p(20.0, 35.0);
    net[2][2] = p(75.0, 30.0);
    net[0][2] = p(70.0, -8.0);
    DeformGrid::new(src(), net).unwrap()
}

#[test]
fn a_deform_grid_interpolates_its_corners_and_follows_its_border_curves() {
    let grid = bumped();
    let net = *grid.net();
    for (q, want) in
        [(p(0.0, 0.0), net[0][0]), (p(100.0, 0.0), net[0][3]), (p(100.0, 50.0), net[3][3]), (p(0.0, 50.0), net[3][0])]
    {
        assert!(near_pt(grid.eval(q).unwrap(), want, 1e-9));
    }
    // The bottom edge is the Bézier curve of the bottom row of the net.
    for u in [0.1, 0.37, 0.5, 0.9] {
        let got = grid.eval(p(100.0 * u, 0.0)).unwrap();
        assert!(near_pt(got, de_casteljau(net[0], u), 1e-9), "u = {u}");
        let left = grid.eval(p(0.0, 50.0 * u)).unwrap();
        let column = [net[0][0], net[1][0], net[2][0], net[3][0]];
        assert!(near_pt(left, de_casteljau(column, u), 1e-9), "v = {u}");
    }
}

#[test]
fn the_middle_of_a_deform_grid_is_the_weighted_sum_of_its_net() {
    let grid = bumped();
    let net = grid.net();
    let w = [1.0 / 8.0, 3.0 / 8.0, 3.0 / 8.0, 1.0 / 8.0];
    let mut want = Vector::ZERO;
    for j in 0..4 {
        for i in 0..4 {
            want = want + net[j][i].to_vector() * (w[i] * w[j]);
        }
    }
    let got = grid.eval(p(50.0, 25.0)).unwrap();
    assert!(near_pt(got, want.to_point(), 1e-9), "{got:?} vs {want:?}");
}

trait ToPoint {
    fn to_point(self) -> Point;
}

impl ToPoint for Vector {
    fn to_point(self) -> Point {
        Point::new(self.x, self.y)
    }
}

#[test]
fn a_deformed_path_stays_within_the_error_budget_and_keeps_its_topology() {
    let grid = bumped();
    let shape = Path::circle(p(50.0, 25.0), 22.0).unwrap();
    let hole = Path::circle(p(50.0, 25.0), 8.0).unwrap();
    let both = Path::from_subpaths([shape.subpaths(), hole.subpaths()].concat());
    for budget in [0.02, 0.002] {
        let w = warp_path(&both, &grid, &tol(budget), &never()).unwrap();
        assert_eq!(w.subpaths().len(), 2);
        assert!(w.subpaths().iter().all(|s| s.is_closed()));
        assert_within_budget(&both, &w, &grid, budget);
    }
    // The warp is deterministic.
    let a = warp_path(&both, &grid, &tol(0.002), &never()).unwrap();
    let b = warp_path(&both, &grid, &tol(0.002), &never()).unwrap();
    assert_eq!(a, b);
}

#[test]
fn a_net_that_folds_the_plane_is_refused() {
    let mut net = regular();
    // Drag the inner point far past the opposite border.
    net[1][1] = p(95.0, 300.0);
    assert_eq!(DeformGrid::new(src(), net), Err(WarpError::Folded));
    let mut net = regular();
    net[2][1] = p(f64::INFINITY, 0.0);
    assert_eq!(DeformGrid::new(src(), net), Err(WarpError::NotFinite));
    assert_eq!(
        DeformGrid::new(Bounds::from_corners(p(0.0, 0.0), p(0.0, 10.0)), regular()),
        Err(WarpError::DegenerateSource)
    );
    assert_eq!(DeformGrid::identity(src()).with_point(4, 0, p(0.0, 0.0)), Err(WarpError::NoSuchControlPoint));
    let mut folded = regular();
    folded[1][1] = p(95.0, 300.0);
    assert_eq!(DeformGrid::identity(src()).with_point(1, 1, folded[1][1]), Err(WarpError::Folded));
    let moved = DeformGrid::identity(src()).with_point(1, 2, p(70.0, 22.0)).unwrap();
    assert_eq!(moved.net()[1][2], p(70.0, 22.0));
    assert_eq!(moved.net()[0][0], regular()[0][0]);
}

#[test]
fn an_open_path_a_cubic_and_a_hole_are_warped_with_their_structure_kept() {
    let q = skew_quad();
    let warp = QuadWarp::new(src(), q, WarpKind::Projective).unwrap();
    let curve =
        PathBuilder::new().move_to(p(5.0, 5.0)).cubic_to(p(30.0, 45.0), p(70.0, -0.0), p(95.0, 45.0)).build().unwrap();
    let w = warp_path(&curve, &warp, &tol(0.002), &never()).unwrap();
    assert!(!w.subpaths()[0].is_closed());
    assert!(near_pt(w.subpaths()[0].start(), warp.eval(p(5.0, 5.0)).unwrap(), 1e-9));
    assert!(near_pt(w.subpaths()[0].end(), warp.eval(p(95.0, 45.0)).unwrap(), 1e-9));
    assert_within_budget(&curve, &w, &warp, 0.002);
    assert_eq!(warp_path(&Path::default(), &warp, &tol(0.002), &never()).unwrap(), Path::default());
}
