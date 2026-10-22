//! G9-editor E08: the direction of a guide at an arc length is the true tangent of the drawn curve,
//! not the direction of the first chord of its flattening. Oracles: the analytic tangent of a
//! circle (`(-sin, cos)` at angle `s / R`), the direction of a straight line, and the sign that
//! follows the travel direction (reversed guides run the other way).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_generators::{guide_frame_at, guide_tangent_at};
use burnburn_geometry::{Path, Point, ProductionTolerance, Shape};
use burnburn_types::Length;

fn tol(mm: f64) -> ProductionTolerance {
    ProductionTolerance::new(Length::from_mm(mm).unwrap()).unwrap()
}

fn circle(r: f64) -> Path {
    Shape::Circle { center: Point::new(0.0, 0.0), radius: Length::from_mm(r).unwrap() }.to_path().unwrap()
}

fn angle_between(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 * b.1 - a.1 * b.0).atan2(a.0 * b.0 + a.1 * b.1).abs()
}

/// How the first chord of the flattening differs from the tangent on a circle of radius 40 at
/// the flattening tolerance of the window: the case the true tangent fixes.
#[test]
fn the_chord_is_visibly_off_the_tangent_on_a_circle_and_the_tangent_is_not() {
    let guide = circle(40.0);
    let t = tol(0.01);
    let mut worst_chord: f64 = 0.0;
    let mut worst_tangent: f64 = 0.0;
    for k in 0..40 {
        let at = f64::from(k) * 5.0;
        let theta = at / 40.0;
        // The circle of `Shape::Circle` starts at angle 0 and runs counter-clockwise (the signed
        // area of the path says so; the sign is checked below).
        let exact = (-theta.sin(), theta.cos());
        let (_, chord) = guide_frame_at(&guide, &t, false, at).unwrap();
        let (_, tangent) = guide_tangent_at(&guide, &t, false, at).unwrap();
        worst_chord = worst_chord.max(angle_between((chord.x, chord.y), exact));
        worst_tangent = worst_tangent.max(angle_between((tangent.x, tangent.y), exact));
        assert!((tangent.length() - 1.0).abs() < 1e-9, "a unit vector");
    }
    assert!(worst_chord > 0.004, "the chord is off by {worst_chord} rad: the case this fixes");
    // The arc length of the flattening is a hair shorter than the curve's (about theta * 7e-5 rad
    // at the far side of this circle): the tangent follows the curve to within 1e-3 rad, the chord only to 0.02.
    assert!(worst_tangent < 1e-3, "the tangent is within {worst_tangent} rad of the analytic one");
}

#[test]
fn a_reversed_guide_runs_the_other_way() {
    let guide = circle(40.0);
    let t = tol(0.01);
    let total = std::f64::consts::TAU * 40.0;
    // Reversed travel: arc length s runs clockwise, from the same start point.
    for s in [3.0, 25.0, 100.0, 200.0] {
        let theta: f64 = -s / 40.0;
        let exact = (theta.sin(), -theta.cos());
        let (_, tangent) = guide_tangent_at(&guide, &t, true, s).unwrap();
        assert!(angle_between((tangent.x, tangent.y), exact) < 1e-3, "s = {s}: {tangent:?} vs {exact:?}");
        assert!(s < total);
    }
}

#[test]
fn a_straight_guide_has_its_own_direction_and_the_point_is_the_same_as_the_frame() {
    let guide = Path::line(Point::new(10.0, 10.0), Point::new(10.0, 110.0)).unwrap();
    let t = tol(0.01);
    let (p, d) = guide_tangent_at(&guide, &t, false, 30.0).unwrap();
    assert!((d.x).abs() < 1e-12 && (d.y - 1.0).abs() < 1e-12, "{d:?}");
    let (q, _) = guide_frame_at(&guide, &t, false, 30.0).unwrap();
    assert!(p.distance(q) < 1e-12);
    let (_, back) = guide_tangent_at(&guide, &t, true, 30.0).unwrap();
    assert!((back.y + 1.0).abs() < 1e-12, "reversed: {back:?}");
}

#[test]
fn beyond_either_end_the_tangent_of_that_end_continues() {
    let guide = circle(40.0);
    let t = tol(0.01);
    // The end of a closed circle is its start: the tangent there is (0, 1).
    let total = std::f64::consts::TAU * 40.0;
    let (_, past) = guide_tangent_at(&guide, &t, false, total + 5.0).unwrap();
    assert!(angle_between((past.x, past.y), (0.0, 1.0)) < 1e-3, "{past:?}");
    let (_, before) = guide_tangent_at(&guide, &t, false, -5.0).unwrap();
    assert!(angle_between((before.x, before.y), (0.0, 1.0)) < 1e-3, "{before:?}");
}

#[test]
fn a_polyline_corner_keeps_the_direction_of_each_side() {
    let guide = Path::polyline(&[Point::new(0.0, 0.0), Point::new(50.0, 0.0), Point::new(50.0, 50.0)], false).unwrap();
    let t = tol(0.01);
    let (_, first) = guide_tangent_at(&guide, &t, false, 20.0).unwrap();
    let (_, second) = guide_tangent_at(&guide, &t, false, 70.0).unwrap();
    assert!((first.x - 1.0).abs() < 1e-12 && first.y.abs() < 1e-12, "{first:?}");
    assert!(second.x.abs() < 1e-12 && (second.y - 1.0).abs() < 1e-12, "{second:?}");
}
