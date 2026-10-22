//! G7-editor S01: the semi-minor axis of an ellipse is a driving dimension, and a point can lie on
//! an ellipse and a line can be tangent to it. Oracles are closed forms of the ellipse: a point
//! (x, y) on an axis-aligned ellipse with semi-axes a and b satisfies (x/a)^2 + (y/b)^2 = 1, and a
//! line with direction at angle t to the major axis is tangent when its distance from the centre
//! is sqrt(a^2 sin^2 t + b^2 cos^2 t).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test helpers outside #[test] fns

mod common;

use burnburn_document::sketch::{
    Along, DimensionKind, DimensionRole, EntityId, Relation, Sketch, SketchCommand, SketchError,
};
use burnburn_document::{DerivedOwner, GeometrySource};
use burnburn_geometry::Point;
use burnburn_sketch::SketchEvaluator;
use burnburn_sketch::solver::SketchSolverAdapter;
use burnburn_sketch::{Diagnosis, Evaluation, Health, evaluate_sketch};
use burnburn_types::CancellationToken;

fn eval(s: &Sketch) -> Evaluation {
    evaluate_sketch(s, &SketchSolverAdapter, &CancellationToken::never()).unwrap()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

struct Fixture {
    s: Sketch,
    c: EntityId,
    m: EntityId,
    e: EntityId,
}

/// Ellipse around (10, 5), horizontal major axis with semi-major 4, semi-minor 2 as drawn; the
/// centre is fixed and the major axis is level and 4 long: only the minor axis is not constrained.
fn ellipse() -> Fixture {
    let mut s = Sketch::new("e");
    let c = s.point(10.0, 5.0).unwrap();
    let m = s.point(14.0, 5.0).unwrap();
    let e = s.ellipse(c, m, 2.0).unwrap();
    s.relate(Relation::Fix { point: c, at: Point::new(10.0, 5.0) }).unwrap();
    s.relate(Relation::Horizontal(e)).unwrap();
    s.dimension(DimensionKind::Radius { curve: e }, "4 mm", DimensionRole::Driving).unwrap();
    Fixture { s, c, m, e }
}

fn bounds_of(ev: &Evaluation) -> (f64, f64, f64, f64) {
    let outputs = ev.solved.as_ref().unwrap().outputs().unwrap();
    let b = outputs[0].1.bounds().unwrap();
    (b.min().x, b.min().y, b.max().x, b.max().y)
}

// --- the minor axis as a dimension ----------------------------------------------------------------

#[test]
fn the_minor_axis_is_a_driving_dimension() {
    let mut f = ellipse();
    f.s.dimension(DimensionKind::MinorDiameter { ellipse: f.e }, "6 mm", DimensionRole::Driving).unwrap();
    let ev = eval(&f.s);
    assert!(ev.is_valid(), "{:?}", ev.health);
    let (x0, y0, x1, y1) = bounds_of(&ev);
    assert!(near(x0, 6.0) && near(x1, 14.0), "the major axis is untouched: {x0} {x1}");
    assert!(near(y0, 2.0) && near(y1, 8.0), "semi-minor 3 around y = 5: {y0} {y1}");
    assert_eq!(ev.health, Health::FullyConstrained, "centre, major axis and minor axis are all given");
}

#[test]
fn the_minor_axis_follows_a_named_parameter() {
    let mut f = ellipse();
    f.s.set_parameter("kleine_as", "10 mm / 2").unwrap();
    f.s.dimension(DimensionKind::MinorDiameter { ellipse: f.e }, "kleine_as", DimensionRole::Driving).unwrap();
    let ev = eval(&f.s);
    assert!(ev.is_valid(), "{:?} {:?} {:?}", ev.health, ev.diagnostics, ev.parameter_error);
    let (_, y0, _, y1) = bounds_of(&ev);
    assert!(near(y1 - y0, 5.0), "{}", y1 - y0);
    f.s.set_parameter("kleine_as", "20 mm").unwrap();
    let (_, y0, _, y1) = bounds_of(&eval(&f.s));
    assert!(near(y1 - y0, 20.0), "the parameter moves the dimension: {}", y1 - y0);
}

#[test]
fn a_driven_minor_axis_measures_and_does_not_constrain() {
    let mut f = ellipse();
    let id = f.s.dimension(DimensionKind::MinorDiameter { ellipse: f.e }, "0 mm", DimensionRole::Driven).unwrap();
    let ev = eval(&f.s);
    assert!(ev.is_valid());
    let Some(Diagnosis::Measured(v)) = ev.diagnostics.get(&id) else { panic!("{:?}", ev.diagnostics.get(&id)) };
    assert!(matches!(v, burnburn_sketch::param::Value::Length(l) if near(l.mm(), 4.0)), "{v:?}: twice the drawn 2");
    let (_, y0, _, y1) = bounds_of(&ev);
    assert!(near(y1 - y0, 4.0));
}

#[test]
fn a_zero_or_negative_minor_axis_is_a_dimension_error_not_a_panic() {
    for text in ["0 mm", "-3 mm"] {
        let mut f = ellipse();
        let id = f.s.dimension(DimensionKind::MinorDiameter { ellipse: f.e }, text, DimensionRole::Driving).unwrap();
        let ev = eval(&f.s);
        assert!(!ev.is_valid(), "{text}: {:?}", ev.health);
        assert!(ev.solved.is_none());
        assert!(ev.failure().is_some());
        let _ = id;
    }
}

#[test]
fn the_minor_axis_of_a_circle_or_a_line_is_refused() {
    let mut s = Sketch::new("x");
    let c = s.point(0.0, 0.0).unwrap();
    let circle = s.circle(c, 3.0).unwrap();
    let r = s.dimension(DimensionKind::MinorDiameter { ellipse: circle }, "4 mm", DimensionRole::Driving);
    assert!(matches!(r, Err(SketchError::WrongEntityKind(_))), "{r:?}");
}

// --- a point on an ellipse ----------------------------------------------------------------------------

#[test]
fn a_point_on_an_ellipse_lies_on_it() {
    let mut f = ellipse();
    // P 2 mm to the right of the centre, drawn above the ellipse: x = 2 gives y = b sqrt(1 - (2/4)^2).
    let p = f.s.point(12.0, 6.6).unwrap();
    f.s.relate(Relation::PointOn { point: p, curve: f.e }).unwrap();
    f.s.dimension(DimensionKind::Distance { a: f.c, b: p, along: Along::X }, "2 mm", DimensionRole::Driving).unwrap();
    let ev = eval(&f.s);
    assert!(ev.is_valid(), "{:?} {:?}", ev.health, ev.diagnostics);
    let at = ev.solved.unwrap().point(p).unwrap();
    assert!(near(at.x, 12.0), "{at:?}");
    let want = 5.0 + 2.0 * (1.0f64 - 0.25).sqrt();
    assert!(near(at.y, want), "{} vs {want}", at.y);
}

#[test]
fn a_point_on_an_ellipse_keeps_the_side_it_was_drawn_on() {
    let mut f = ellipse();
    let p = f.s.point(12.0, 3.4).unwrap(); // below the centre line
    f.s.relate(Relation::PointOn { point: p, curve: f.e }).unwrap();
    f.s.dimension(DimensionKind::Distance { a: f.c, b: p, along: Along::X }, "2 mm", DimensionRole::Driving).unwrap();
    let at = eval(&f.s).solved.unwrap().point(p).unwrap();
    assert!(near(at.y, 5.0 - 2.0 * (1.0f64 - 0.25).sqrt()), "{at:?}");
}

#[test]
fn the_point_follows_when_the_minor_axis_changes() {
    let mut f = ellipse();
    let p = f.s.point(12.0, 6.6).unwrap();
    f.s.relate(Relation::PointOn { point: p, curve: f.e }).unwrap();
    f.s.dimension(DimensionKind::Distance { a: f.c, b: p, along: Along::X }, "2 mm", DimensionRole::Driving).unwrap();
    let minor = f.s.dimension(DimensionKind::MinorDiameter { ellipse: f.e }, "4 mm", DimensionRole::Driving).unwrap();
    assert!(near(eval(&f.s).solved.unwrap().point(p).unwrap().y, 5.0 + 2.0 * 0.75f64.sqrt()));
    f.s.set_dimension_text(minor, "10 mm").unwrap();
    let ev = eval(&f.s);
    let at = ev.solved.unwrap().point(p).unwrap();
    assert!(near(at.y, 5.0 + 5.0 * 0.75f64.sqrt()), "b = 5 now: {at:?}");
}

#[test]
fn a_point_on_a_rotated_ellipse_satisfies_the_ellipse_equation() {
    let mut s = Sketch::new("r");
    let c = s.point(0.0, 0.0).unwrap();
    let m = s.point(3.0, 4.0).unwrap(); // semi-major 5 at an angle of atan2(4, 3)
    let e = s.ellipse(c, m, 3.0).unwrap();
    s.relate(Relation::Fix { point: c, at: Point::new(0.0, 0.0) }).unwrap();
    s.relate(Relation::Fix { point: m, at: Point::new(3.0, 4.0) }).unwrap();
    let p = s.point(2.0, 3.5).unwrap();
    s.relate(Relation::PointOn { point: p, curve: e }).unwrap();
    s.dimension(DimensionKind::Distance { a: c, b: p, along: Along::X }, "1.5 mm", DimensionRole::Driving).unwrap();
    let ev = eval(&s);
    assert!(ev.is_valid(), "{:?}", ev.health);
    let at = ev.solved.unwrap().point(p).unwrap();
    let (cos, sin) = (3.0 / 5.0, 4.0 / 5.0);
    let (u, v) = (at.x * cos + at.y * sin, -at.x * sin + at.y * cos);
    let on = (u / 5.0).powi(2) + (v / 3.0).powi(2);
    assert!(near(on, 1.0), "(u/a)^2 + (v/b)^2 = {on} for {at:?}");
}

// --- a line tangent to an ellipse -------------------------------------------------------------------------

#[test]
fn a_level_line_tangent_to_the_ellipse_touches_its_top() {
    let mut f = ellipse();
    let (p, q) = (f.s.point(5.0, 7.5).unwrap(), f.s.point(15.0, 7.5).unwrap());
    let line = f.s.line(p, q).unwrap();
    f.s.relate(Relation::Horizontal(line)).unwrap();
    f.s.relate(Relation::Tangent(line, f.e)).unwrap();
    let ev = eval(&f.s);
    assert!(ev.is_valid(), "{:?} {:?}", ev.health, ev.diagnostics);
    let solved = ev.solved.unwrap();
    assert!(near(solved.point(p).unwrap().y, 7.0), "top of the ellipse: {:?}", solved.point(p));
}

#[test]
fn a_level_line_below_the_ellipse_stays_below() {
    let mut f = ellipse();
    let (p, q) = (f.s.point(5.0, 2.5).unwrap(), f.s.point(15.0, 2.5).unwrap());
    let line = f.s.line(p, q).unwrap();
    f.s.relate(Relation::Horizontal(line)).unwrap();
    f.s.relate(Relation::Tangent(f.e, line)).unwrap(); // the ellipse may be named first
    let solved = eval(&f.s).solved.unwrap();
    assert!(near(solved.point(p).unwrap().y, 3.0), "bottom of the ellipse: {:?}", solved.point(p));
}

#[test]
fn a_slanted_tangent_has_the_distance_from_the_centre_the_ellipse_demands() {
    let mut f = ellipse();
    // A line at 30 degrees to the major axis through (8, 9) and (18, 14.77...) — only its angle and
    // one point are given; tangency fixes how far it sits from the centre.
    let angle = 30.0f64.to_radians();
    let (p, q) = (f.s.point(8.0, 9.0).unwrap(), f.s.point(8.0 + 10.0 * angle.cos(), 9.0 + 10.0 * angle.sin()).unwrap());
    let line = f.s.line(p, q).unwrap();
    let guide = f.s.line(f.c, f.m).unwrap();
    f.s.relate(Relation::Tangent(line, f.e)).unwrap();
    f.s.dimension(DimensionKind::Angle { from: guide, to: line }, "30 deg", DimensionRole::Driving).unwrap();
    let ev = eval(&f.s);
    assert!(ev.is_valid(), "{:?} {:?}", ev.health, ev.diagnostics);
    let solved = ev.solved.unwrap();
    let (pp, qq) = (solved.point(p).unwrap(), solved.point(q).unwrap());
    let (dx, dy) = (qq.x - pp.x, qq.y - pp.y);
    let len = dx.hypot(dy);
    let centre = Point::new(10.0, 5.0);
    let dist = ((centre.x - pp.x) * dy - (centre.y - pp.y) * dx).abs() / len;
    let (a, b, t) = (4.0f64, 2.0f64, 30.0f64.to_radians());
    let want = (a * a * t.sin().powi(2) + b * b * t.cos().powi(2)).sqrt();
    assert!(near(dist, want), "distance {dist} vs {want}");
}

#[test]
fn tangency_follows_the_minor_axis_dimension() {
    let mut f = ellipse();
    let (p, q) = (f.s.point(5.0, 9.0).unwrap(), f.s.point(15.0, 9.0).unwrap());
    let line = f.s.line(p, q).unwrap();
    f.s.relate(Relation::Horizontal(line)).unwrap();
    f.s.relate(Relation::Tangent(line, f.e)).unwrap();
    f.s.dimension(DimensionKind::MinorDiameter { ellipse: f.e }, "8 mm", DimensionRole::Driving).unwrap();
    let solved = eval(&f.s).solved.unwrap();
    assert!(near(solved.point(p).unwrap().y, 9.0), "centre 5 + semi-minor 4: {:?}", solved.point(p));
}

// --- what stays refused, and the degrees of freedom ---------------------------------------------------------

#[test]
fn an_ellipse_is_still_never_equal_to_a_circle() {
    // Tangent to a circle became possible in G8 (`g8_ellipse_swap_tangent.rs`).
    let f = ellipse();
    let mut s = f.s;
    let cc = s.point(30.0, 30.0).unwrap();
    let circle = s.circle(cc, 3.0).unwrap();
    assert!(s.relate(Relation::Tangent(f.e, circle)).is_ok());
    assert!(matches!(s.relate(Relation::Equal(f.e, circle)), Err(SketchError::WrongEntityKind(_))));
    // And a line can still not lie on an ellipse: only points can.
    let (p, q) = (s.point(0.0, 0.0).unwrap(), s.point(1.0, 1.0).unwrap());
    let line = s.line(p, q).unwrap();
    assert!(matches!(s.relate(Relation::PointOn { point: line, curve: f.e }), Err(SketchError::WrongEntityKind(_))));
}

#[test]
fn a_relation_does_not_make_the_unconstrained_minor_axis_a_freedom() {
    // Without a minor-axis dimension the drawn semi-minor stays as it is (it was never a freedom):
    // the tangent line has only its own freedom left.
    let mut f = ellipse();
    let (p, q) = (f.s.point(5.0, 7.5).unwrap(), f.s.point(15.0, 7.5).unwrap());
    let line = f.s.line(p, q).unwrap();
    f.s.relate(Relation::Horizontal(line)).unwrap();
    f.s.relate(Relation::Tangent(line, f.e)).unwrap();
    let ev = eval(&f.s);
    // The line keeps sliding along itself: two x coordinates, nothing else.
    assert_eq!(ev.health, Health::UnderConstrained { dof: 2 }, "{:?}", ev.health);
    let (_, y0, _, y1) = bounds_of(&ev);
    assert!(near(y1 - y0, 4.0), "the drawn minor axis of 2 x 2: {}", y1 - y0);
}

#[test]
fn contradicting_the_ellipse_is_a_conflict_that_names_the_constraints() {
    let mut f = ellipse();
    let p = f.s.point(12.0, 6.6).unwrap();
    let on = f.s.relate(Relation::PointOn { point: p, curve: f.e }).unwrap();
    // x = 2 and y = 5 + 2.5 cannot both hold: the ellipse is only 2 high.
    let dx =
        f.s.dimension(DimensionKind::Distance { a: f.c, b: p, along: Along::X }, "2 mm", DimensionRole::Driving)
            .unwrap();
    let dy =
        f.s.dimension(DimensionKind::Distance { a: f.c, b: p, along: Along::Y }, "2.5 mm", DimensionRole::Driving)
            .unwrap();
    let ev = eval(&f.s);
    assert!(matches!(ev.health, Health::Conflict | Health::NotConverged), "{:?}", ev.health);
    assert!(ev.solved.is_none() && ev.failure().is_some());
    let set = ev.conflict_set();
    assert!(!set.is_empty(), "the failure names constraints: {:?}", ev.diagnostics);
    assert!(set.contains(&on) || set.contains(&dx) || set.contains(&dy), "the new constraints take part: {set:?}");
    assert!(
        set.iter().all(|c| f.s.constraints().any(|(id, _)| id == *c)),
        "only the sketch's own constraints: {set:?}"
    );
}

#[test]
fn a_minor_axis_larger_than_the_major_swaps_the_roles_of_the_axes_and_still_works() {
    let mut f = ellipse();
    f.s.dimension(DimensionKind::MinorDiameter { ellipse: f.e }, "12 mm", DimensionRole::Driving).unwrap();
    let (p, q) = (f.s.point(5.0, 11.5).unwrap(), f.s.point(15.0, 11.5).unwrap());
    let line = f.s.line(p, q).unwrap();
    f.s.relate(Relation::Horizontal(line)).unwrap();
    f.s.relate(Relation::Tangent(line, f.e)).unwrap();
    let r = f.s.point(12.0, 9.5).unwrap();
    f.s.relate(Relation::PointOn { point: r, curve: f.e }).unwrap();
    f.s.dimension(DimensionKind::Distance { a: f.c, b: r, along: Along::X }, "2 mm", DimensionRole::Driving).unwrap();
    let ev = eval(&f.s);
    assert!(ev.is_valid(), "{:?} {:?}", ev.health, ev.diagnostics);
    let (x0, y0, x1, y1) = bounds_of(&ev);
    assert!(near(x0, 6.0) && near(x1, 14.0) && near(y0, -1.0) && near(y1, 11.0), "{:?}", (x0, y0, x1, y1));
    let solved = ev.solved.unwrap();
    assert!(near(solved.point(p).unwrap().y, 11.0), "tangent at the top: {:?}", solved.point(p));
    assert!(near(solved.point(r).unwrap().y, 5.0 + 6.0 * 0.75f64.sqrt()), "{:?}", solved.point(r));
}

#[test]
fn the_same_inputs_give_the_same_bits() {
    let build = || {
        let mut f = ellipse();
        let p = f.s.point(12.0, 6.6).unwrap();
        f.s.relate(Relation::PointOn { point: p, curve: f.e }).unwrap();
        f.s.dimension(DimensionKind::Distance { a: f.c, b: p, along: Along::X }, "2 mm", DimensionRole::Driving)
            .unwrap();
        f.s.dimension(DimensionKind::MinorDiameter { ellipse: f.e }, "5 mm", DimensionRole::Driving).unwrap();
        (eval(&f.s), p)
    };
    let ((a, pa), (b, pb)) = (build(), build());
    let (qa, qb) = (a.solved.unwrap().point(pa).unwrap(), b.solved.unwrap().point(pb).unwrap());
    assert_eq!((qa.x.to_bits(), qa.y.to_bits()), (qb.x.to_bits(), qb.y.to_bits()));
}

// --- the solved minor axis is stored with the solution --------------------------------------------------------

fn output_height(snap: &burnburn_document::DocumentSnapshot) -> f64 {
    let (_, o) = snap
        .objects()
        .find(|(_, o)| matches!(o.source(), GeometrySource::Derived(k) if matches!(k.owner, DerivedOwner::Sketch(_))))
        .expect("the ellipse is an output");
    let b = o.bounds().unwrap();
    b.max().y - b.min().y
}

#[test]
fn the_solved_minor_axis_is_stored_and_stays_when_the_dimension_is_removed() {
    let mut f = ellipse();
    let minor = f.s.dimension(DimensionKind::MinorDiameter { ellipse: f.e }, "6 mm", DimensionRole::Driving).unwrap();
    let p = f.s.point(12.0, 6.6).unwrap();
    f.s.relate(Relation::PointOn { point: p, curve: f.e }).unwrap();
    let ev = SketchEvaluator::new(SketchSolverAdapter);
    let mut session = common::session();
    session.apply_with(common::sketch_cmd(&session, SketchCommand::Create { sketch: Box::new(f.s) }), &[&ev]).unwrap();
    let snap = session.snapshot();
    let (id, sketch) = snap.sketch_layer().sketches().next().unwrap();
    let stored = sketch.solution().expect("a stored solution").minors.get(&f.e).copied();
    assert!(stored.is_some_and(|m| near(m, 3.0)), "{stored:?}");
    assert!(near(output_height(&snap), 6.0));

    // Taking the dimension away leaves the ellipse as it was solved: it was never a freedom.
    session
        .apply_with(
            common::sketch_cmd(&session, SketchCommand::RemoveConstraint { sketch: id, constraint: minor }),
            &[&ev],
        )
        .unwrap();
    assert!(near(output_height(&session.snapshot()), 6.0), "the last solved minor axis stays");
}

#[test]
fn an_ellipse_nobody_dimensioned_stores_nothing_extra() {
    let f = ellipse();
    let ev = SketchEvaluator::new(SketchSolverAdapter);
    let mut session = common::session();
    session.apply_with(common::sketch_cmd(&session, SketchCommand::Create { sketch: Box::new(f.s) }), &[&ev]).unwrap();
    let snap = session.snapshot();
    let sketch = snap.sketch_layer().sketches().next().unwrap().1;
    assert!(sketch.solution().unwrap().minors.is_empty(), "the drawn minor axis is the document's own value");
}
