//! Solver contract tests (IM11 beslisproef). Generic over [`Solver`] so a fallback solver can run
//! the same suite.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test helpers outside #[test] fns

use burnburn_geometry::Point;
use burnburn_sketch::solver::{
    Along, Constraint as C, ConstraintIndex, ConstraintState, Coordinate, EntityId, Problem, SketchSolverAdapter,
    SolveError, SolveStatus, Solver,
};
use burnburn_types::{CancellationSource, CancellationToken};

const TOL: f64 = 1e-9;

fn solver() -> impl Solver {
    SketchSolverAdapter
}

fn solve(p: &Problem) -> burnburn_sketch::solver::SolveReport {
    solver().solve(p, &CancellationToken::never()).expect("solve runs")
}

fn pt(r: &burnburn_sketch::solver::SolveReport, id: EntityId) -> Point {
    r.point(id).expect("point solved")
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= TOL
}

/// Axis-aligned rectangle p0..p3 counter-clockwise from the origin, drawn roughly.
struct Rect {
    p: [EntityId; 4],
    l: [EntityId; 4],
}

fn rect(pb: &mut Problem, w: f64, h: f64, with_width: bool) -> Rect {
    let p0 = pb.point(0.3, -0.2);
    let p1 = pb.point(w + 3.0, 1.0);
    let p2 = pb.point(w - 2.0, h + 1.5);
    let p3 = pb.point(-1.0, h - 2.0);
    let l = [pb.line(p0, p1), pb.line(p1, p2), pb.line(p2, p3), pb.line(p3, p0)];
    pb.constrain(C::Fix { point: p0, at: Point::new(0.0, 0.0) });
    pb.constrain(C::Horizontal(l[0]));
    pb.constrain(C::Vertical(l[1]));
    pb.constrain(C::Horizontal(l[2]));
    pb.constrain(C::Vertical(l[3]));
    if with_width {
        pb.constrain(C::Distance { a: p0, b: p1, value: w, along: Along::X });
    }
    pb.constrain(C::Distance { a: p0, b: p3, value: h, along: Along::Y });
    Rect { p: [p0, p1, p2, p3], l }
}

#[test]
fn fully_determined_rectangle_solves_exactly_with_zero_dof() {
    let mut pb = Problem::new();
    let r = rect(&mut pb, 120.0, 80.0, true);
    let rep = solve(&pb);
    assert_eq!(rep.status, SolveStatus::Solved);
    assert_eq!(rep.dof, 0);
    assert!(rep.free.is_empty());
    let p2 = pt(&rep, r.p[2]);
    assert!(close(p2.x, 120.0) && close(p2.y, 80.0), "{p2:?}");
    assert!(rep.constraints.iter().all(|s| *s == ConstraintState::Satisfied));
}

#[test]
fn underdetermined_rectangle_reports_remaining_dof_and_free_coordinates() {
    let mut pb = Problem::new();
    let r = rect(&mut pb, 120.0, 80.0, false);
    let rep = solve(&pb);
    assert_eq!(rep.status, SolveStatus::Solved);
    assert_eq!(rep.dof, 1);
    let free: Vec<_> = rep.free.iter().map(|f| (f.entity, f.coordinate)).collect();
    assert!(free.contains(&(r.p[1], Coordinate::X)) && free.contains(&(r.p[2], Coordinate::X)), "{free:?}");
    assert!(!free.iter().any(|(_, c)| *c == Coordinate::Y), "heights are fixed: {free:?}");
    // The free direction stays where it is drawn (minimal-norm steps), within reason.
    assert!((pt(&rep, r.p[1]).x - 123.0).abs() < 3.0);
}

#[test]
fn redundant_constraint_is_reported_and_solve_still_succeeds() {
    let mut pb = Problem::new();
    let r = rect(&mut pb, 120.0, 80.0, true);
    let dup = pb.constrain(C::Length { line: r.l[0], value: 120.0 });
    let rep = solve(&pb);
    assert_eq!(rep.status, SolveStatus::Solved);
    assert!(rep.redundant().any(|c| c == dup), "{:?}", rep.constraints);
    assert_eq!(rep.conflicting().count(), 0);
}

#[test]
fn conflicting_dimension_is_named_with_what_it_contradicts() {
    let mut pb = Problem::new();
    let r = rect(&mut pb, 120.0, 80.0, true);
    let bad = pb.constrain(C::Length { line: r.l[0], value: 100.0 });
    let rep = solve(&pb);
    assert_ne!(rep.status, SolveStatus::Solved);
    match &rep.constraints[bad.0] {
        ConstraintState::Conflicting { with } => assert!(!with.is_empty()),
        other => panic!("expected conflict, got {other:?}"),
    }
}

#[test]
fn infeasible_triangle_is_not_solved_without_requiring_a_conflict_claim() {
    // |ab| = 10 but |ac| + |cb| = 4: no solution. Either status is acceptable as long as it is not Solved.
    let mut pb = Problem::new();
    let a = pb.point(0.0, 0.0);
    let b = pb.point(10.0, 0.0);
    let c = pb.point(5.0, 1.0);
    pb.constrain(C::Fix { point: a, at: Point::new(0.0, 0.0) });
    pb.constrain(C::Distance { a, b, value: 10.0, along: Along::Direct });
    pb.constrain(C::Distance { a, b: c, value: 2.0, along: Along::Direct });
    pb.constrain(C::Distance { a: c, b, value: 2.0, along: Along::Direct });
    let rep = solve(&pb);
    assert!(!rep.is_solved());
    assert!(rep.residual > rep.tolerance);
    assert!(
        rep.constraints.iter().any(|s| !matches!(s, ConstraintState::Satisfied | ConstraintState::Redundant { .. })),
        "some constraint must be reported as not holding: {:?}",
        rep.constraints
    );
}

#[test]
fn large_dimension_change_keeps_the_drawn_branch() {
    // Triangle with apex drawn above the base; the side length changes a lot. The mirrored
    // solution (apex below) satisfies the same equations and must not be chosen.
    let mut pb = Problem::new();
    let o = pb.point(0.0, 0.0);
    let a = pb.point(40.0, 0.0);
    let apex = pb.point(20.0, 5.0);
    let base = pb.line(o, a);
    pb.constrain(C::Fix { point: o, at: Point::new(0.0, 0.0) });
    pb.constrain(C::Horizontal(base));
    pb.constrain(C::Length { line: base, value: 40.0 });
    pb.constrain(C::Distance { a: o, b: apex, value: 60.0, along: Along::Direct });
    pb.constrain(C::Distance { a, b: apex, value: 60.0, along: Along::Direct });
    let rep = solve(&pb);
    assert_eq!(rep.status, SolveStatus::Solved);
    let p = pt(&rep, apex);
    assert!(p.y > 0.0, "apex flipped below the base: {p:?}");
    assert!(close(p.x, 20.0) && close(p.y, (3600.0f64 - 400.0).sqrt()), "{p:?}");
    assert!(rep.flipped.is_empty(), "{:?}", rep.flipped);
}

#[test]
fn point_on_line_coincident_and_perpendicular_hold() {
    let mut pb = Problem::new();
    let a = pb.point(0.0, 0.0);
    let b = pb.point(50.0, 10.0);
    let l = pb.line(a, b);
    let c = pb.point(20.0, 9.0);
    let d = pb.point(30.0, 30.0);
    let m = pb.line(c, d);
    let e = pb.point(19.0, 8.0);
    pb.constrain(C::Fix { point: a, at: Point::new(0.0, 0.0) });
    pb.constrain(C::Fix { point: b, at: Point::new(50.0, 10.0) });
    pb.constrain(C::PointOn { point: c, curve: l });
    pb.constrain(C::Perpendicular(l, m));
    pb.constrain(C::Coincident(e, c));
    let rep = solve(&pb);
    assert_eq!(rep.status, SolveStatus::Solved);
    let (pc, pd, pe) = (pt(&rep, c), pt(&rep, d), pt(&rep, e));
    let cross = 50.0 * pc.y - 10.0 * pc.x;
    assert!(cross.abs() < 1e-9, "c not on line: {pc:?}");
    let dot = 50.0 * (pd.x - pc.x) + 10.0 * (pd.y - pc.y);
    assert!(dot.abs() < 1e-9, "not perpendicular");
    assert!(close(pe.x, pc.x) && close(pe.y, pc.y));
}

#[test]
fn diameter_radius_equal_and_concentric_hold() {
    let mut pb = Problem::new();
    let c1 = pb.point(10.0, 10.0);
    let c2 = pb.point(11.0, 9.0);
    let c3 = pb.point(40.0, 10.0);
    let h1 = pb.circle(c1, 2.5);
    let h2 = pb.circle(c2, 5.0);
    let h3 = pb.circle(c3, 1.0);
    pb.constrain(C::Fix { point: c1, at: Point::new(10.0, 10.0) });
    pb.constrain(C::Diameter { curve: h1, value: 4.0 });
    pb.constrain(C::Concentric(h1, h2));
    pb.constrain(C::Radius { curve: h2, value: 6.0 });
    pb.constrain(C::Equal(h1, h3));
    let rep = solve(&pb);
    assert_eq!(rep.status, SolveStatus::Solved);
    assert!(close(rep.radius(h1).expect("r1"), 2.0));
    assert!(close(rep.radius(h2).expect("r2"), 6.0));
    assert!(close(rep.radius(h3).expect("r3"), 2.0));
    let p2 = pt(&rep, c2);
    assert!(close(p2.x, 10.0) && close(p2.y, 10.0));
}

#[test]
fn symmetric_points_mirror_about_a_line() {
    let mut pb = Problem::new();
    let m0 = pb.point(60.0, 0.0);
    let m1 = pb.point(60.0, 80.0);
    let axis = pb.line(m0, m1);
    let a = pb.point(10.0, 10.0);
    let b = pb.point(105.0, 12.0);
    pb.constrain(C::Fix { point: m0, at: Point::new(60.0, 0.0) });
    pb.constrain(C::Fix { point: m1, at: Point::new(60.0, 80.0) });
    pb.constrain(C::Fix { point: a, at: Point::new(10.0, 10.0) });
    pb.constrain(C::Symmetric { a, b, about: axis });
    let rep = solve(&pb);
    assert_eq!(rep.status, SolveStatus::Solved);
    assert_eq!(rep.dof, 0);
    let pb_ = pt(&rep, b);
    assert!(close(pb_.x, 110.0) && close(pb_.y, 10.0), "{pb_:?}");
}

#[test]
fn line_arc_tangency_holds_on_the_drawn_side() {
    // A horizontal line y = 0 and an arc of radius 10 drawn above it, tangent.
    let mut pb = Problem::new();
    let a = pb.point(0.0, 0.0);
    let b = pb.point(50.0, 0.0);
    let l = pb.line(a, b);
    let c = pb.point(25.0, 12.0);
    let s = pb.point(35.0, 12.0);
    let e = pb.point(15.0, 12.0);
    let arc = pb.add(burnburn_sketch::solver::Entity::Arc { center: c, start: s, end: e, clockwise: false });
    pb.constrain(C::Fix { point: a, at: Point::new(0.0, 0.0) });
    pb.constrain(C::Fix { point: b, at: Point::new(50.0, 0.0) });
    pb.constrain(C::Radius { curve: arc, value: 10.0 });
    pb.constrain(C::Tangent(l, arc));
    let rep = solve(&pb);
    assert_eq!(rep.status, SolveStatus::Solved);
    let pc = pt(&rep, c);
    assert!(close(pc.y, 10.0), "centre must be one radius above the line: {pc:?}");
}

#[test]
fn two_runs_are_bit_identical() {
    let mut pb = Problem::new();
    rect(&mut pb, 120.0, 80.0, true);
    let a = solve(&pb);
    let b = solve(&pb);
    assert_eq!(format!("{a:?}"), format!("{b:?}"));
    for (x, y) in a.entities.iter().zip(&b.entities) {
        assert_eq!(format!("{x:?}"), format!("{y:?}"));
    }
}

#[test]
fn cancelled_solve_returns_cancelled_not_a_result() {
    let mut pb = Problem::new();
    rect(&mut pb, 120.0, 80.0, true);
    let src = CancellationSource::new();
    src.cancel();
    assert_eq!(solver().solve(&pb, &src.token()), Err(SolveError::Cancelled));
}

#[test]
fn invalid_reference_and_value_are_typed_errors() {
    let mut pb = Problem::new();
    let a = pb.point(0.0, 0.0);
    pb.constrain(C::Horizontal(a)); // a point, not a line
    assert!(matches!(solver().solve(&pb, &CancellationToken::never()), Err(SolveError::InvalidReference { .. })));

    let mut pb = Problem::new();
    let a = pb.point(0.0, 0.0);
    let b = pb.point(1.0, 0.0);
    let l = pb.line(a, b);
    pb.constrain(C::Length { line: l, value: f64::NAN });
    assert!(matches!(
        solver().solve(&pb, &CancellationToken::never()),
        Err(SolveError::InvalidValue { constraint: ConstraintIndex(0), .. })
    ));
}
