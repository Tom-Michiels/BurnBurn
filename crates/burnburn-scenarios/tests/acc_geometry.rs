//! Acceptance plan, geometry block: GE01-GE05. The geometry kernel is a core crate, so these run
//! against its public API. Oracles are analytic: areas of polygons built by hand, exact circle
//! area from the arc representation, distance of exact curve samples to the flattened chords.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test code

use burnburn_geometry::{
    BooleanError, BooleanOp, BooleanOptions, DiagnosticTolerance, EndpointRef, FillRule, IssueKind, OffsetError,
    OffsetJoin, Path, PathBuilder, Point, ProductionTolerance, Rect, TrimError, boolean, diagnose, join_shapes, offset,
};
use burnburn_types::{CancellationToken, Length};
use proptest::prelude::*;

fn len(mm: f64) -> Length {
    Length::from_mm(mm).unwrap()
}

fn prod(mm: f64) -> ProductionTolerance {
    ProductionTolerance::new(len(mm)).unwrap()
}

fn opts(fill: FillRule) -> BooleanOptions {
    BooleanOptions { fill, tolerance: prod(0.001) }
}

fn poly(points: &[(f64, f64)]) -> Path {
    let pts: Vec<Point> = points.iter().map(|&(x, y)| Point::new(x, y)).collect();
    Path::polyline(&pts, true).unwrap()
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Path {
    Path::rectangle(Rect::new(Point::new(x, y), len(w), len(h)).unwrap())
}

fn run(a: &Path, b: &Path, op: BooleanOp, fill: FillRule) -> Result<Path, BooleanError> {
    boolean(a, b, op, &opts(fill), &CancellationToken::never())
}

/// Net area: outer contours positive, holes negative (the kernel's output winding contract).
fn area(p: &Path) -> f64 {
    p.signed_area()
}

fn approx(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

// ---------------------------------------------------------------------------------------------
// GE01 touching / overlapping polygons, holes, open curves
// ---------------------------------------------------------------------------------------------

#[test]
fn ge01_overlapping_squares_have_the_inclusion_exclusion_areas() {
    // Two 10 x 10 squares overlapping in a 5 x 5 corner: |A|+|B|-|A∩B| = 175.
    let (a, b) = (rect(0.0, 0.0, 10.0, 10.0), rect(5.0, 5.0, 10.0, 10.0));
    let f = FillRule::EvenOdd;
    assert!(approx(area(&run(&a, &b, BooleanOp::Union, f).unwrap()), 175.0, 1e-3));
    assert!(approx(area(&run(&a, &b, BooleanOp::Intersection, f).unwrap()), 25.0, 1e-3));
    assert!(approx(area(&run(&a, &b, BooleanOp::Difference, f).unwrap()), 75.0, 1e-3));
    assert!(approx(area(&run(&a, &b, BooleanOp::Xor, f).unwrap()), 150.0, 1e-3));
}

#[test]
fn ge01_edge_touching_squares_merge_to_one_outline_and_share_no_area() {
    let (a, b) = (rect(0.0, 0.0, 10.0, 10.0), rect(10.0, 0.0, 10.0, 10.0));
    let union = run(&a, &b, BooleanOp::Union, FillRule::EvenOdd).unwrap();
    assert_eq!(union.subpaths().len(), 1, "the shared edge disappears");
    assert!(approx(area(&union), 200.0, 1e-3));
    let both = run(&a, &b, BooleanOp::Intersection, FillRule::EvenOdd).unwrap();
    assert!(approx(area(&both), 0.0, 1e-9), "a shared edge has no area");
}

#[test]
fn ge01_fill_rule_decides_what_a_same_wound_hole_is() {
    // 20 x 20 outer square with a 10 x 10 inner square, both counter-clockwise.
    let outer = rect(0.0, 0.0, 20.0, 20.0);
    let inner = rect(5.0, 5.0, 10.0, 10.0);
    let mut subs = outer.subpaths().to_vec();
    subs.extend(inner.subpaths().iter().cloned());
    let ring = Path::from_subpaths(subs);
    let nothing = Path::default();
    let even_odd = run(&ring, &nothing, BooleanOp::Union, FillRule::EvenOdd).unwrap();
    assert!(approx(area(&even_odd), 300.0, 1e-3), "even-odd: the inner square is a hole");
    assert_eq!(even_odd.subpaths().len(), 2);
    let non_zero = run(&ring, &nothing, BooleanOp::Union, FillRule::NonZero).unwrap();
    assert!(approx(area(&non_zero), 400.0, 1e-3), "non-zero: same winding fills the inner square");
    assert_eq!(non_zero.subpaths().len(), 1);
    // A clockwise inner contour is a hole under both rules.
    let hole = poly(&[(5.0, 5.0), (5.0, 15.0), (15.0, 15.0), (15.0, 5.0)]); // clockwise
    let mut subs = outer.subpaths().to_vec();
    subs.extend(hole.subpaths().iter().cloned());
    let wound = Path::from_subpaths(subs);
    for rule in [FillRule::EvenOdd, FillRule::NonZero] {
        assert!(approx(area(&run(&wound, &nothing, BooleanOp::Union, rule).unwrap()), 300.0, 1e-3), "{rule:?}");
    }
}

#[test]
fn ge01_a_shape_inside_the_hole_stays_a_separate_island() {
    let outer = rect(0.0, 0.0, 20.0, 20.0);
    let hole = rect(5.0, 5.0, 10.0, 10.0);
    let ring = run(&outer, &hole, BooleanOp::Difference, FillRule::EvenOdd).unwrap();
    assert!(approx(area(&ring), 300.0, 1e-3));
    let island = rect(8.0, 8.0, 4.0, 4.0);
    let both = run(&ring, &island, BooleanOp::Union, FillRule::EvenOdd).unwrap();
    assert!(approx(area(&both), 316.0, 1e-3));
    assert_eq!(both.subpaths().len(), 3, "outer, hole and island");
}

#[test]
fn ge01_open_curves_are_refused_naming_the_operand() {
    let open = Path::line(Point::new(0.0, 0.0), Point::new(10.0, 0.0)).unwrap();
    let square = rect(0.0, 0.0, 10.0, 10.0);
    assert_eq!(
        run(&open, &square, BooleanOp::Union, FillRule::EvenOdd),
        Err(BooleanError::OpenContour { operand: 0, subpath: 0 })
    );
    assert_eq!(
        run(&square, &open, BooleanOp::Union, FillRule::EvenOdd),
        Err(BooleanError::OpenContour { operand: 1, subpath: 0 })
    );
}

#[test]
fn ge01_circle_booleans_follow_the_exact_circle_area_within_the_flattening_budget() {
    let c = Path::circle(Point::new(0.0, 0.0), 10.0).unwrap();
    let exact = std::f64::consts::PI * 100.0;
    assert!(approx(c.signed_area(), exact, 1e-9), "arc representation is exact");
    let u = run(&c, &c, BooleanOp::Union, FillRule::EvenOdd).unwrap();
    // The inscribed polygon loses at most perimeter * tolerance.
    let bound = 2.0 * std::f64::consts::PI * 10.0 * 0.001;
    assert!(area(&u) <= exact + 1e-9 && exact - area(&u) <= bound, "{} vs {exact}", area(&u));
}

/// Area of a polygonal path measured relative to its own bounds corner. `Path::signed_area` sums
/// cross products of absolute coordinates, which loses digits beyond ~1e8 mm (noted in the dev
/// note); the boolean's own output is exact, so the oracle must not add that error.
fn local_area(p: &Path) -> f64 {
    let Some(b) = p.bounds() else { return 0.0 };
    let o = b.min();
    p.subpaths()
        .iter()
        .map(|sp| {
            let mut pts = vec![sp.start()];
            pts.extend(sp.segments().iter().map(|s| s.end()));
            let n = pts.len();
            (0..n)
                .map(|i| {
                    let (a, b) = (pts[i], pts[(i + 1) % n]);
                    (a.x - o.x) * (b.y - o.y) - (b.x - o.x) * (a.y - o.y)
                })
                .sum::<f64>()
                / 2.0
        })
        .sum()
}

// ---------------------------------------------------------------------------------------------
// GE02 self-intersection, 1 micrometre edge, huge coordinate
// ---------------------------------------------------------------------------------------------

#[test]
fn ge02_a_bow_tie_is_diagnosed_at_its_crossing_and_filled_by_the_even_odd_rule() {
    let bow = poly(&[(0.0, 0.0), (10.0, 10.0), (10.0, 0.0), (0.0, 10.0)]);
    let issues = diagnose(&[(1_u32, &bow)], &DiagnosticTolerance::new(len(0.01)).unwrap());
    let crossing: Vec<_> = issues.iter().filter(|i| matches!(i.kind, IssueKind::SelfIntersection)).collect();
    assert_eq!(crossing.len(), 1, "{issues:?}");
    assert!(crossing[0].location.distance(Point::new(5.0, 5.0)) < 1e-9);
    assert_eq!(crossing[0].object, 1);
    // Even-odd: two triangles of 25 mm² each, plus a far-away 1 x 1 square.
    let far = rect(100.0, 100.0, 1.0, 1.0);
    let u = run(&bow, &far, BooleanOp::Union, FillRule::EvenOdd).unwrap();
    assert!(approx(u.subpaths().iter().filter_map(|s| s.signed_area()).map(f64::abs).sum::<f64>(), 51.0, 1e-3));
}

#[test]
fn ge02_a_one_micrometre_edge_is_reported_and_does_not_break_a_boolean() {
    let sliver = poly(&[(0.0, 0.0), (10.0, 0.0), (10.0, 0.001), (0.0, 0.001)]);
    let tol = DiagnosticTolerance::new(len(0.01)).unwrap();
    let issues = diagnose(&[(7_u32, &sliver)], &tol);
    let short: Vec<f64> = issues
        .iter()
        .filter_map(|i| match i.kind {
            IssueKind::ZeroLength { length } => Some(length),
            _ => None,
        })
        .collect();
    assert_eq!(short.len(), 2, "both 1 um sides are named: {issues:?}");
    assert!(short.iter().all(|l| approx(*l, 0.001, 1e-12)));
    // A tolerance below the edge does not flag it.
    let strict = DiagnosticTolerance::new(len(0.0005)).unwrap();
    assert!(diagnose(&[(7_u32, &sliver)], &strict).iter().all(|i| !matches!(i.kind, IssueKind::ZeroLength { .. })));
    // The boolean stays finite and within the grid error: |A| = 0.01 mm².
    let u = run(&sliver, &sliver, BooleanOp::Union, FillRule::EvenOdd).unwrap();
    assert!(area(&u) >= 0.0 && approx(area(&u), 0.01, 0.01 * 0.5), "{}", area(&u));
}

#[test]
fn ge02_a_huge_coordinate_is_a_typed_outcome_never_a_crash_or_a_wrong_area() {
    for exp in [3, 6, 9, 12, 15, 18, 30, 100] {
        let big = 10f64.powi(exp);
        let a = rect(big, big, 10.0, 10.0);
        let b = rect(big + 5.0, big + 5.0, 10.0, 10.0);
        match run(&a, &b, BooleanOp::Intersection, FillRule::EvenOdd) {
            // When it is accepted, the 5 x 5 overlap must still be right to within the grid.
            // (Beyond 1e13 mm an f64 cannot even hold a 10 mm square, so only the first
            // range has an exact expectation.)
            Ok(p) if exp <= 12 => assert!(approx(local_area(&p), 25.0, 0.5), "1e{exp}: area {}", local_area(&p)),
            Ok(_) => {}
            Err(e) => assert_eq!(e, BooleanError::OutOfRange, "1e{exp}"),
        }
    }
    // NaN / infinity never get as far as a boolean: path construction refuses them.
    assert!(Path::polyline(&[Point::new(0.0, 0.0), Point::new(f64::NAN, 1.0)], false).is_err());
    assert!(Path::polyline(&[Point::new(0.0, 0.0), Point::new(f64::INFINITY, 1.0)], false).is_err());
    // An offset reports a huge shape as a typed error as well.
    let huge = rect(0.0, 0.0, 1e12, 1e12);
    let r = offset(&huge, 1.0, OffsetJoin::Round, &prod(0.001), &CancellationToken::never());
    assert!(matches!(r, Ok(_) | Err(OffsetError::OutOfRange)));
}

// ---------------------------------------------------------------------------------------------
// GE03 curve fit / flatten on circles and a cusp
// ---------------------------------------------------------------------------------------------

fn dist_point_segment(p: Point, a: Point, b: Point) -> f64 {
    let (abx, aby) = (b.x - a.x, b.y - a.y);
    let l2 = abx * abx + aby * aby;
    let t = if l2 == 0.0 { 0.0 } else { (((p.x - a.x) * abx + (p.y - a.y) * aby) / l2).clamp(0.0, 1.0) };
    ((p.x - a.x - t * abx).hypot(p.y - a.y - t * aby)).abs()
}

fn dist_to_polyline(p: Point, pts: &[Point], closed: bool) -> f64 {
    let n = pts.len();
    let mut best = f64::INFINITY;
    for i in 0..n {
        let j = if i + 1 < n { i + 1 } else if closed { 0 } else { break };
        best = best.min(dist_point_segment(p, pts[i], pts[j]));
    }
    best
}

fn shoelace(pts: &[Point]) -> f64 {
    let n = pts.len();
    (0..n).map(|i| pts[i].x * pts[(i + 1) % n].y - pts[(i + 1) % n].x * pts[i].y).sum::<f64>() / 2.0
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, failure_persistence: None, ..ProptestConfig::default() })]

    /// Every chord of a flattened circle stays within the budget of the true circle, the polygon
    /// is inscribed (area below pi r^2, deficit bounded by perimeter * budget) and stays closed.
    #[test]
    fn ge03_flattened_circle_stays_within_the_budget_and_closed(r in 0.5_f64..800.0, t in 1e-4_f64..0.2) {
        let c = Path::circle(Point::new(3.0, -4.0), r).unwrap();
        let lines = c.flatten(&prod(t));
        prop_assert_eq!(lines.len(), 1);
        prop_assert!(lines[0].closed);
        let pts = &lines[0].points;
        let centre = Point::new(3.0, -4.0);
        for p in pts { prop_assert!((p.distance(centre) - r).abs() < 1e-6 * r.max(1.0)); }
        let n = pts.len();
        for i in 0..n {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            let mid = Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
            prop_assert!(r - mid.distance(centre) <= t + 1e-9, "chord {i} sags {}", r - mid.distance(centre));
        }
        let a = shoelace(pts);
        let exact = std::f64::consts::PI * r * r;
        prop_assert!(a <= exact + 1e-6 && exact - a <= 2.0 * std::f64::consts::PI * r * t + 1e-6);
    }
}

#[test]
fn ge03_a_cusp_is_flattened_within_the_budget_and_stays_closed() {
    // Cubic with B'(1/2) = 0: P3 = P0 + P1 - P2. The curve turns back on itself at (7.5, 5).
    let (p0, p1, p2, p3) = (Point::new(0.0, 0.0), Point::new(10.0, 10.0), Point::new(10.0, 0.0), Point::new(0.0, 10.0));
    let path = PathBuilder::new().move_to(p0).cubic_to(p1, p2, p3).close().build().unwrap();
    let t = 0.01;
    let line = &path.flatten(&prod(t))[0];
    assert!(line.closed);
    let b = |u: f64| {
        let m = 1.0 - u;
        let f = |a: f64, b: f64, c: f64, d: f64| m * m * m * a + 3.0 * m * m * u * b + 3.0 * m * u * u * c + u * u * u * d;
        Point::new(f(p0.x, p1.x, p2.x, p3.x), f(p0.y, p1.y, p2.y, p3.y))
    };
    assert!(b(0.5).distance(Point::new(7.5, 5.0)) < 1e-12, "the test curve really has its cusp at (7.5, 5)");
    for i in 0..=4000 {
        let p = b(f64::from(i) / 4000.0);
        let d = dist_to_polyline(p, &line.points, true);
        assert!(d <= t + 1e-9, "curve point at u={} is {d} from the chords", f64::from(i) / 4000.0);
    }
    assert_eq!(line.points[0], p0);
}

#[test]
fn ge03_optimizing_a_flattened_circle_keeps_every_vertex_within_the_budget_and_the_contour_closed() {
    let c = Path::circle(Point::new(0.0, 0.0), 50.0).unwrap();
    let line = &c.flatten(&prod(0.0005))[0];
    let dense = Path::polyline(&line.points, true).unwrap();
    let tol = prod(0.01);
    let out = dense.optimize(&tol, &CancellationToken::never()).unwrap();
    assert!(out.removed_nodes > 0, "a dense circle polygon has nodes to spare");
    assert_eq!(out.path.subpaths().len(), 1);
    assert!(out.path.subpaths()[0].is_closed(), "closure is kept");
    let kept = &out.path.flatten(&prod(0.0005))[0].points;
    for p in &line.points {
        let d = dist_to_polyline(*p, kept, true);
        assert!(d <= 0.01 + 1e-9, "input vertex {d} from the optimised outline");
    }
}

// ---------------------------------------------------------------------------------------------
// GE04 offset, kerf and fillet at a narrow neck
// ---------------------------------------------------------------------------------------------

/// Two 20 x 20 squares joined by a 20 mm long, 2 mm wide neck (y 9..11).
fn dumbbell() -> Path {
    poly(&[
        (0.0, 0.0),
        (20.0, 0.0),
        (20.0, 9.0),
        (40.0, 9.0),
        (40.0, 0.0),
        (60.0, 0.0),
        (60.0, 20.0),
        (40.0, 20.0),
        (40.0, 11.0),
        (20.0, 11.0),
        (20.0, 20.0),
        (0.0, 20.0),
    ])
}

fn inward(d: f64) -> Result<Path, OffsetError> {
    offset(&dumbbell(), -d, OffsetJoin::Miter, &prod(0.001), &CancellationToken::never())
}

#[test]
fn ge04_an_inward_offset_keeps_the_neck_until_it_is_eaten_and_then_splits_in_two() {
    // The neck is 2 mm wide: it survives an offset of 0.9 mm (0.2 mm wide) ...
    let thin = inward(0.9).unwrap();
    assert_eq!(thin.subpaths().len(), 1, "the 0.2 mm neck still connects the squares");
    // ... and is gone at 1.1 mm: two separate squares of (20 - 2.2)^2 = 316.84 mm².
    let split = inward(1.1).unwrap();
    assert_eq!(split.subpaths().len(), 2, "the neck vanished, the part splits");
    let mut areas: Vec<f64> = split.subpaths().iter().filter_map(|s| s.signed_area()).collect();
    areas.sort_by(f64::total_cmp);
    for a in areas {
        assert!(approx(a, 316.84, 0.05), "{a}");
    }
}

#[test]
fn ge04_an_offset_larger_than_the_inradius_reports_collapse_instead_of_an_empty_success() {
    let sq = rect(0.0, 0.0, 10.0, 10.0);
    let r = offset(&sq, -5.0, OffsetJoin::Round, &prod(0.001), &CancellationToken::never());
    assert_eq!(r, Err(OffsetError::Collapsed));
    let r = offset(&sq, -20.0, OffsetJoin::Round, &prod(0.001), &CancellationToken::never());
    assert_eq!(r, Err(OffsetError::Collapsed));
}

#[test]
fn ge04_kerf_growth_of_a_square_matches_the_analytic_rounded_area() {
    // Outward round offset of a 10 x 10 square by 2: 100 + perimeter*2 + pi*2^2.
    let sq = rect(0.0, 0.0, 10.0, 10.0);
    let o = offset(&sq, 2.0, OffsetJoin::Round, &prod(0.001), &CancellationToken::never()).unwrap();
    let exact = 100.0 + 40.0 * 2.0 + std::f64::consts::PI * 4.0;
    assert!(approx(area(&o), exact, 0.05), "{} vs {exact}", area(&o));
}

#[test]
fn ge04_fillets_at_the_neck_are_limited_and_the_limit_is_reported() {
    let out = dumbbell().round_corners(5.0, None).unwrap();
    assert!(out.limited > 0, "the 2 mm stubs cannot carry a 5 mm radius: {out:?}");
    assert_eq!(out.rounded + out.skipped, 12, "every corner is accounted for: {out:?}");
    assert_eq!(out.path.subpaths().len(), 1);
    assert!(out.path.subpaths()[0].is_closed());
}

// ---------------------------------------------------------------------------------------------
// GE05 trim / extend / join with a tolerance just below / at / above the limit
// ---------------------------------------------------------------------------------------------

fn two_pieces(gap: f64) -> (Path, Path) {
    (
        Path::line(Point::new(0.0, 0.0), Point::new(10.0, 0.0)).unwrap(),
        Path::line(Point::new(10.0 + gap, 0.0), Point::new(20.0 + gap, 0.0)).unwrap(),
    )
}

fn joined(gap: f64, tol: f64) -> burnburn_geometry::JoinOutcome {
    let (a, b) = two_pieces(gap);
    join_shapes(&[&a, &b], &DiagnosticTolerance::new(len(tol)).unwrap(), &CancellationToken::never()).unwrap()
}

#[test]
fn ge05_join_threshold_is_inclusive_and_the_same_for_every_pair() {
    for (gap, expect) in [(0.0999, true), (0.1, true), (0.1001, false)] {
        let out = joined(gap, 0.1);
        assert_eq!(out.merged == 1, expect, "gap {gap}: {out:?}");
        assert_eq!(out.path.subpaths().len(), if expect { 1 } else { 2 }, "gap {gap}");
    }
}

#[test]
fn ge05_a_joined_piece_snaps_exactly_and_an_unjoined_one_is_untouched() {
    let ok = joined(0.05, 0.1);
    let sp = &ok.path.subpaths()[0];
    assert_eq!(sp.start(), Point::new(0.0, 0.0));
    assert_eq!(sp.end(), Point::new(20.05, 0.0), "the far end of the second piece did not move");
    let apart = joined(0.5, 0.1);
    assert_eq!(apart.path.subpaths().len(), 2);
    assert_eq!(apart.path.subpaths()[1].start(), Point::new(10.5, 0.0), "no ghost connection, no move");
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, failure_persistence: None, ..ProptestConfig::default() })]

    /// Joined exactly when gap <= tolerance, and a join never changes the number of pieces any
    /// other way: 2 pieces in, 1 or 2 out.
    #[test]
    fn ge05_property_join_decision_is_gap_le_tolerance(gap in 0.0_f64..3.0, tol in 0.001_f64..3.0) {
        prop_assume!((gap - tol).abs() > 1e-9);
        let out = joined(gap, tol);
        prop_assert_eq!(out.path.subpaths().len(), if gap < tol { 1 } else { 2 });
    }
}

#[test]
fn ge05_trim_and_extend_decide_by_exact_crossing_not_by_proximity() {
    let line = Path::line(Point::new(0.0, 0.0), Point::new(10.0, 0.0)).unwrap();
    let wall = |x: f64| Path::line(Point::new(x, -5.0), Point::new(x, 5.0)).unwrap();
    // Trim: a wall in the middle removes the picked half.
    let trimmed = line.trim_line(Point::new(8.0, 0.0), &[&wall(5.0)]).unwrap();
    assert_eq!(trimmed.bounds().unwrap().max().x, 5.0);
    // A wall that only touches the end or lies beyond it is no crossing.
    for x in [10.0, 10.001] {
        assert_eq!(line.trim_line(Point::new(8.0, 0.0), &[&wall(x)]), Err(TrimError::NoIntersection), "x={x}");
    }
    // Extend: only a boundary ahead of the end counts.
    let extended = line.extend_line(EndpointRef::End(0), &[&wall(10.001)]).unwrap();
    assert!(approx(extended.bounds().unwrap().max().x, 10.001, 1e-9));
    assert!(line.extend_line(EndpointRef::End(0), &[&wall(9.999)]).is_err(), "a boundary behind the end is not ahead");
}
