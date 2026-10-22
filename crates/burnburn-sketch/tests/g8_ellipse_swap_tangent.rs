//! G8-editor S01: a dimension that drives an ellipse through a = b (the axes change roles) still
//! converges, and an ellipse can be tangent to a circle or to another ellipse. Oracles are closed
//! forms: the semi-axes are the dimensions, a circle touching the end of the major axis is
//! `a + r` away, an ellipse touching it end to end is `a1 + a2` away.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test helpers outside #[test] fns

mod common;

use burnburn_document::sketch::{
    Along, ConstraintId, DimensionKind, DimensionRole, EntityId, Relation, Sketch, SketchCommand, SketchError,
};
use burnburn_document::{DerivedOwner, DocumentSession, GeometrySource};
use burnburn_geometry::Point;
use burnburn_sketch::solver::SketchSolverAdapter;
use burnburn_sketch::{Evaluation, Health, SketchEvaluator, evaluate_sketch};
use burnburn_types::CancellationToken;

fn eval(s: &Sketch) -> Evaluation {
    evaluate_sketch(s, &SketchSolverAdapter, &CancellationToken::never()).unwrap()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-5
}

/// Ellipse around (10, 5), horizontal, semi-major `a` given by a radius dimension, semi-minor 3
/// given by a minor-axis dimension of 6 mm.
struct Fixture {
    s: Sketch,
    c: EntityId,
    m: EntityId,
    e: EntityId,
    major: ConstraintId,
}

fn fixture(a: f64, major_as_distance: bool) -> Fixture {
    let mut s = Sketch::new("e");
    let c = s.point(10.0, 5.0).unwrap();
    let m = s.point(10.0 + a, 5.0).unwrap();
    let e = s.ellipse(c, m, 3.0).unwrap();
    s.relate(Relation::Fix { point: c, at: Point::new(10.0, 5.0) }).unwrap();
    s.relate(Relation::Horizontal(e)).unwrap();
    let text = format!("{a} mm");
    let major = if major_as_distance {
        s.dimension(DimensionKind::Distance { a: c, b: m, along: Along::Direct }, &text, DimensionRole::Driving)
            .unwrap()
    } else {
        s.dimension(DimensionKind::Radius { curve: e }, &text, DimensionRole::Driving).unwrap()
    };
    s.dimension(DimensionKind::MinorDiameter { ellipse: e }, "6 mm", DimensionRole::Driving).unwrap();
    Fixture { s, c, m, e, major }
}

fn output_extent(snap: &burnburn_document::DocumentSnapshot) -> (f64, f64, f64, f64) {
    let (_, o) = snap
        .objects()
        .find(|(_, o)| matches!(o.source(), GeometrySource::Derived(k) if matches!(k.owner, DerivedOwner::Sketch(_))))
        .expect("the ellipse is an output");
    let b = o.bounds().unwrap();
    (b.min().x, b.min().y, b.max().x, b.max().y)
}

fn set_major(session: &mut DocumentSession, ev: &SketchEvaluator<SketchSolverAdapter>, f: &Fixture, text: &str) {
    let snap = session.snapshot();
    let (id, _) = snap.sketch_layer().sketches().next().unwrap();
    let cmd = SketchCommand::SetDimensionValue { sketch: id, constraint: f.major, text: text.into() };
    session.apply_with(common::sketch_cmd(session, cmd), &[ev]).unwrap();
}

fn check_extent(session: &DocumentSession, half_width: f64, half_height: f64, what: &str) {
    let (x0, y0, x1, y1) = output_extent(&session.snapshot());
    assert!(
        near(x0, 10.0 - half_width)
            && near(x1, 10.0 + half_width)
            && near(y0, 5.0 - half_height)
            && near(y1, 5.0 + half_height),
        "{what}: {:?}",
        (x0, y0, x1, y1)
    );
}

#[test]
fn a_radius_dimension_that_drops_below_the_minor_axis_still_converges_and_comes_back() {
    let f = fixture(4.0, false);
    let ev = SketchEvaluator::new(SketchSolverAdapter);
    let mut session = common::session();
    session
        .apply_with(common::sketch_cmd(&session, SketchCommand::Create { sketch: Box::new(f.s.clone()) }), &[&ev])
        .unwrap();
    check_extent(&session, 4.0, 3.0, "a = 4, b = 3");
    // 2 < 3: the "major" axis is now the shorter one. One edit, one solve.
    set_major(&mut session, &ev, &f, "2 mm");
    check_extent(&session, 2.0, 3.0, "a = 2, b = 3");
    let snap = session.snapshot();
    let (_, sketch) = snap.sketch_layer().sketches().next().unwrap();
    assert!(
        sketch.solution().is_some_and(|s| s.minors.is_empty() || s.minors.get(&f.e).is_some_and(|m| near(*m, 3.0)))
    );
    // And back through a = b = 3 to a longer major axis.
    set_major(&mut session, &ev, &f, "5 mm");
    check_extent(&session, 5.0, 3.0, "a = 5, b = 3");
    set_major(&mut session, &ev, &f, "3 mm");
    check_extent(&session, 3.0, 3.0, "a = b: a circle");
}

#[test]
fn the_same_when_the_major_axis_is_given_by_a_distance_between_the_two_points() {
    // No radius dimension names the target, so the solver has to find the branch itself.
    let f = fixture(4.0, true);
    let ev = SketchEvaluator::new(SketchSolverAdapter);
    let mut session = common::session();
    session
        .apply_with(common::sketch_cmd(&session, SketchCommand::Create { sketch: Box::new(f.s.clone()) }), &[&ev])
        .unwrap();
    check_extent(&session, 4.0, 3.0, "a = 4");
    set_major(&mut session, &ev, &f, "2 mm");
    check_extent(&session, 2.0, 3.0, "a = 2 < b");
    set_major(&mut session, &ev, &f, "6 mm");
    check_extent(&session, 6.0, 3.0, "a = 6");
}

#[test]
fn a_one_shot_evaluation_with_the_target_on_the_other_side_of_the_minor_axis_is_fully_constrained() {
    // Drawn with a = 4, asked for a = 1.5 and b = 3: no stored solution to start from.
    let mut f = fixture(4.0, false);
    f.s.set_dimension_text(f.major, "1.5 mm").unwrap();
    let ev = eval(&f.s);
    assert_eq!(ev.health, Health::FullyConstrained, "{:?} {:?}", ev.health, ev.diagnostics);
    let solved = ev.solved.unwrap();
    assert!(near(solved.point(f.m).unwrap().distance(solved.point(f.c).unwrap()), 1.5));
    let _ = f.e;
}

// --- tangent to a circle ---------------------------------------------------------------------------------

/// Ellipse around (10, 5): semi-major 4 along x (fixed), semi-minor 2 as drawn.
fn ellipse_alone() -> (Sketch, EntityId, EntityId, EntityId) {
    let mut s = Sketch::new("e");
    let c = s.point(10.0, 5.0).unwrap();
    let m = s.point(14.0, 5.0).unwrap();
    let e = s.ellipse(c, m, 2.0).unwrap();
    s.relate(Relation::Fix { point: c, at: Point::new(10.0, 5.0) }).unwrap();
    s.relate(Relation::Horizontal(e)).unwrap();
    s.dimension(DimensionKind::Radius { curve: e }, "4 mm", DimensionRole::Driving).unwrap();
    (s, c, m, e)
}

#[test]
fn a_circle_outside_the_ellipse_grows_until_it_touches_the_end_of_the_major_axis() {
    let (mut s, _, _, e) = ellipse_alone();
    let cc = s.point(16.0, 5.0).unwrap();
    s.relate(Relation::Fix { point: cc, at: Point::new(16.0, 5.0) }).unwrap();
    let circle = s.circle(cc, 1.7).unwrap();
    s.relate(Relation::Tangent(e, circle)).unwrap();
    let ev = eval(&s);
    assert!(ev.is_valid(), "{:?} {:?}", ev.health, ev.diagnostics);
    assert_eq!(ev.health, Health::FullyConstrained, "the tangency used up the radius");
    let r = ev.solved.unwrap().radius(circle).unwrap();
    assert!(near(r, 2.0), "16 - 14 = 2: {r}");
}

#[test]
fn a_circle_inside_the_ellipse_grows_until_it_touches_the_minor_vertices() {
    let (mut s, c, _, e) = ellipse_alone();
    let circle = s.circle(c, 1.5).unwrap();
    s.relate(Relation::Tangent(circle, e)).unwrap();
    let ev = eval(&s);
    assert!(ev.is_valid(), "{:?} {:?}", ev.health, ev.diagnostics);
    let r = ev.solved.unwrap().radius(circle).unwrap();
    assert!(near(r, 2.0), "the nearest points of the ellipse are its minor vertices, 2 away: {r}");
}

#[test]
fn a_circle_of_given_radius_slides_along_a_line_until_it_touches() {
    let (mut s, c, _, e) = ellipse_alone();
    let cc = s.point(16.5, 5.0).unwrap();
    let level = s.line(c, cc).unwrap();
    s.relate(Relation::Horizontal(level)).unwrap();
    let circle = s.circle(cc, 1.5).unwrap();
    s.dimension(DimensionKind::Radius { curve: circle }, "1.5 mm", DimensionRole::Driving).unwrap();
    s.relate(Relation::Tangent(e, circle)).unwrap();
    let ev = eval(&s);
    assert!(ev.is_valid(), "{:?} {:?}", ev.health, ev.diagnostics);
    assert_eq!(ev.health, Health::FullyConstrained, "{:?}", ev.health);
    let at = ev.solved.unwrap().point(cc).unwrap();
    assert!(near(at.x, 15.5) && near(at.y, 5.0), "4 + 1.5 from the centre: {at:?}");
}

#[test]
fn a_tangent_circle_off_the_axes_touches_where_the_ellipse_equation_says() {
    // Circle of radius 1 above the ellipse, centre fixed at (10, 9): the nearest ellipse point is
    // the top (10, 7), so the circle must reach down to y = 8 only when r = 2: ask for the radius.
    let (mut s, _, _, e) = ellipse_alone();
    let cc = s.point(10.0, 9.0).unwrap();
    s.relate(Relation::Fix { point: cc, at: Point::new(10.0, 9.0) }).unwrap();
    let circle = s.circle(cc, 1.5).unwrap();
    s.relate(Relation::Tangent(circle, e)).unwrap();
    let ev = eval(&s);
    assert!(ev.is_valid(), "{:?} {:?}", ev.health, ev.diagnostics);
    let r = ev.solved.unwrap().radius(circle).unwrap();
    assert!(near(r, 2.0), "9 - 7 = 2: {r}");
}

// --- tangent to another ellipse -----------------------------------------------------------------------------

/// Two horizontal ellipses on one level, `a2` and `b2` for the second, whose centre slides along the
/// level line. The first is the one of `ellipse_alone`.
fn two_ellipses(drawn_c2: f64, a2: f64, b2: f64) -> (Sketch, EntityId, EntityId) {
    let (mut s, c, _, e1) = ellipse_alone();
    let c2 = s.point(drawn_c2, 5.0).unwrap();
    let m2 = s.point(drawn_c2 + a2, 5.0).unwrap();
    let e2 = s.ellipse(c2, m2, b2).unwrap();
    let level = s.line(c, c2).unwrap();
    s.relate(Relation::Horizontal(level)).unwrap();
    s.relate(Relation::Horizontal(e2)).unwrap();
    s.dimension(DimensionKind::Radius { curve: e2 }, &format!("{a2} mm"), DimensionRole::Driving).unwrap();
    s.relate(Relation::Tangent(e1, e2)).unwrap();
    (s, c2, e2)
}

#[test]
fn two_ellipses_touch_end_to_end_a1_plus_a2_apart() {
    let (s, c2, _) = two_ellipses(18.0, 3.0, 1.5);
    let ev = eval(&s);
    assert!(ev.is_valid(), "{:?} {:?}", ev.health, ev.diagnostics);
    assert_eq!(ev.health, Health::FullyConstrained, "{:?}", ev.health);
    let at = ev.solved.unwrap().point(c2).unwrap();
    assert!(near(at.x, 17.0) && near(at.y, 5.0), "4 + 3 from the first centre: {at:?}");
}

#[test]
fn a_small_ellipse_inside_a_large_one_touches_its_end_from_within() {
    // E2 (a = 2, b = 1) has a sharper end than E1 (radius of curvature 1 there, against 0.5): it
    // fits inside, and the right ends meet when the centres are a1 - a2 = 2 apart.
    let (s, c2, _) = two_ellipses(11.5, 2.0, 1.0);
    let ev = eval(&s);
    assert!(ev.is_valid(), "{:?} {:?}", ev.health, ev.diagnostics);
    let at = ev.solved.unwrap().point(c2).unwrap();
    assert!(near(at.x, 12.0), "4 - 2 from the first centre: {at:?}");
}

#[test]
fn the_tangency_follows_a_changed_dimension() {
    let (mut s, c2, e2) = two_ellipses(18.0, 3.0, 1.5);
    let radius_dim = s
        .constraints()
        .find_map(|(id, c)| match c {
            burnburn_document::sketch::SketchConstraint::Dimension(d)
                if matches!(d.kind, DimensionKind::Radius { curve } if curve == e2) =>
            {
                Some(id)
            }
            _ => None,
        })
        .unwrap();
    s.set_dimension_text(radius_dim, "2 mm").unwrap();
    let at = eval(&s).solved.unwrap().point(c2).unwrap();
    assert!(near(at.x, 16.0), "4 + 2: {at:?}");
}

#[test]
fn what_stays_refused() {
    let (mut s, c, _, e) = ellipse_alone();
    let cc = s.point(30.0, 30.0).unwrap();
    let circle = s.circle(cc, 3.0).unwrap();
    // Tangent is allowed now; equal radii of an ellipse and a circle are still meaningless.
    assert!(s.relate(Relation::Tangent(e, circle)).is_ok());
    assert!(matches!(s.relate(Relation::Equal(e, circle)), Err(SketchError::WrongEntityKind(_))));
    // A point is not something an ellipse is tangent to.
    assert!(matches!(s.relate(Relation::Tangent(e, c)), Err(SketchError::WrongEntityKind(_))));
}
