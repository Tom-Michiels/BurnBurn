//! S05: when the constraints make the solver choose another geometric branch than the one drawn,
//! the evaluation says so (`Evaluation::flipped`) instead of silently jumping to it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test helpers outside #[test] fns

use burnburn_document::sketch::{EntityId, Relation, Sketch};
use burnburn_geometry::Point;
use burnburn_sketch::evaluate_sketch;
use burnburn_sketch::solver::{Flip, SketchSolverAdapter};
use burnburn_types::CancellationToken;

fn eval(s: &Sketch) -> burnburn_sketch::Evaluation {
    evaluate_sketch(s, &SketchSolverAdapter, &CancellationToken::never()).unwrap()
}

/// A fixed line along the x axis and a radius-3 circle drawn with its centre at (10, `drawn_y`),
/// tangent to the line, its centre fixed at (10, `fixed_y`). Returns the circle's id.
fn tangent_circle(drawn_y: f64, fixed_y: f64) -> (Sketch, EntityId) {
    let mut s = Sketch::new("raaklijn");
    let a = s.point(0.0, 0.0).unwrap();
    let b = s.point(20.0, 0.0).unwrap();
    let line = s.line(a, b).unwrap();
    let c = s.point(10.0, drawn_y).unwrap();
    let circle = s.circle(c, 3.0).unwrap();
    s.relate(Relation::Fix { point: a, at: Point::new(0.0, 0.0) }).unwrap();
    s.relate(Relation::Fix { point: b, at: Point::new(20.0, 0.0) }).unwrap();
    s.relate(Relation::Fix { point: c, at: Point::new(10.0, fixed_y) }).unwrap();
    s.relate(Relation::Tangent(line, circle)).unwrap();
    (s, circle)
}

#[test]
fn a_solution_on_the_drawn_side_reports_no_flip() {
    let (s, _) = tangent_circle(8.0, 8.0);
    let ev = eval(&s);
    assert!(ev.is_valid(), "{:?}", ev.health);
    assert!(ev.flipped.is_empty(), "{:?}", ev.flipped);
}

#[test]
fn branch_flip_is_reported_not_silent() {
    // Drawn above the line, fixed below it: the circle ends up tangent on the other side.
    let (s, circle) = tangent_circle(8.0, -8.0);
    let ev = eval(&s);
    assert!(ev.is_valid(), "{:?}", ev.health);
    assert_eq!(ev.flipped, vec![Flip::RadiusSign(solver_id(&s, circle))], "{:?}", ev.flipped);
}

/// Regression (found while wiring the S05 notice): the flip came with a negative solved radius,
/// which broke the circle's output (`InvalidDefinition`). The circle on the other side has the
/// positive radius; the flip is reported next to it.
#[test]
fn a_flipped_tangent_circle_keeps_a_positive_radius_and_its_output() {
    let (s, circle) = tangent_circle(8.0, -8.0);
    let ev = eval(&s);
    let solved = ev.solved.as_ref().unwrap();
    assert!((solved.radius(circle).unwrap() - 8.0).abs() < 1e-6, "{:?}", solved.radius(circle));
    assert_eq!(solved.outputs().unwrap().len(), 2, "the line and the circle");
    assert!(ev.failure().is_none());
    // The stored solution starts the next solve on the same side: no flip is reported again.
    let sol = solved.solution(ev.dof);
    assert!(sol.radii.values().all(|r| *r > 0.0), "{:?}", sol.radii);
}

/// The solver's id of a document entity (entities are lowered in id order, one each).
fn solver_id(s: &Sketch, id: EntityId) -> burnburn_sketch::solver::EntityId {
    let index = s.entities().position(|(e, _)| e == id).unwrap();
    // The solver ids are the lowering order; build one problem with enough entities to name it.
    let mut p = burnburn_sketch::solver::Problem::new();
    let mut last = None;
    for _ in 0..=index {
        last = Some(p.point(0.0, 0.0));
    }
    last.unwrap()
}
