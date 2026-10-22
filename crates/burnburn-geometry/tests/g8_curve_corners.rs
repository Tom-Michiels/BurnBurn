//! G8-editor E05: fillet and chamfer of a corner whose sides are curves (line–arc, arc–arc,
//! line–Bézier), and extending a Bézier end past the reach of its own polynomial. Oracles are
//! closed forms (tangent circles of two circles, distance to a line) and an independent
//! distance-to-curve sampler; none of them repeats the Newton iteration of the implementation.

#![allow(clippy::unwrap_used, clippy::unreachable)] // test helpers and let-else in tests

use std::f64::consts::{FRAC_PI_2, PI};

use burnburn_geometry::{
    CornerCut, CornerError, EllipticalArc, EndpointRef, ExtendError, Path, PathBuilder, Point, Segment, Vector,
};
use burnburn_types::Angle;

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn rad(r: f64) -> Angle {
    Angle::from_radians(r).unwrap()
}

fn circle_arc(cx: f64, cy: f64, r: f64, start: f64, sweep: f64) -> EllipticalArc {
    EllipticalArc::circle(p(cx, cy), r, rad(start), rad(sweep)).unwrap()
}

/// The segments of subpath 0 with their start points.
fn pieces(path: &Path) -> Vec<(Point, Segment)> {
    let sp = &path.subpaths()[0];
    let mut from = sp.start();
    let mut out = Vec::new();
    for seg in sp.segments() {
        out.push((from, *seg));
        from = seg.end();
    }
    out
}

fn unit(v: Vector) -> (f64, f64) {
    let l = v.length();
    (v.x / l, v.y / l)
}

/// Unit tangent at the segment's end (`t = 1`) or its start (`t = 0`).
fn tangent(from: Point, seg: &Segment, end: bool) -> (f64, f64) {
    unit(seg.derivative(from, if end { 1.0 } else { 0.0 }))
}

fn same_direction(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6
}

/// Smallest distance from `q` to the segment, by dense sampling (independent of the library's
/// own nearest-point code).
fn distance_to(from: Point, seg: &Segment, q: Point) -> f64 {
    (0..=20000).map(|i| seg.point_at(from, f64::from(i) / 20000.0).distance(q)).fold(f64::INFINITY, f64::min)
}

fn arc_of(seg: &Segment) -> EllipticalArc {
    let Segment::Arc(a) = seg else { unreachable!("expected an arc, got {seg:?}") };
    *a
}

/// Corner at the origin: a line coming down the y axis, then the circle of radius 10 around
/// (0, 10) from its lowest point counter-clockwise.
fn line_then_circle() -> Path {
    PathBuilder::new()
        .move_to(p(0.0, 20.0))
        .line_to(p(0.0, 0.0))
        .arc(circle_arc(0.0, 10.0, 10.0, -FRAC_PI_2, FRAC_PI_2))
        .build()
        .unwrap()
}

#[test]
fn a_fillet_between_a_line_and_a_circle_is_the_tangent_circle_of_the_closed_form() {
    // Left of the line is x > 0, left of the arc is towards its centre: the fillet centre is at
    // x = r and at distance 10 - r from (0, 10), so (cy - 10)^2 = 100 - 20 r.
    let r = 2.0;
    let cy = 10.0 - (100.0_f64 - 20.0 * r).sqrt();
    let out = line_then_circle().cut_corner(p(0.1, 0.1), 1.0, CornerCut::Fillet { radius: r }).unwrap();
    let parts = pieces(&out);
    assert_eq!(parts.len(), 3, "line, fillet, rest of the circle: {parts:?}");
    // The line stops at the tangent point on the y axis.
    assert!(parts[0].1.end().distance(p(0.0, cy)) < 1e-6, "{:?}", parts[0].1.end());
    // The fillet is an arc of the right radius around (r, cy).
    let fillet = arc_of(&parts[1].1);
    assert!(fillet.center().distance(p(r, cy)) < 1e-6, "{:?}", fillet.center());
    assert!((fillet.semi_diameters().0.length() - r).abs() < 1e-9);
    assert!(fillet.sweep_parameter() > 0.0, "a left turn sweeps counter-clockwise");
    // The circle resumes where the ray from its centre through the fillet centre meets it.
    let (dx, dy) = (r / (10.0 - r), (cy - 10.0) / (10.0 - r));
    let tangent_on_circle = p(10.0 * dx, 10.0 + 10.0 * dy);
    assert!(parts[1].1.end().distance(tangent_on_circle) < 1e-6, "{:?}", parts[1].1.end());
    assert!(parts[2].0.distance(tangent_on_circle) < 1e-6);
    assert!(parts[2].1.end().distance(p(10.0, 10.0)) < 1e-9, "the far end is untouched");
    // Tangent continuity at both joints.
    assert!(same_direction(tangent(parts[0].0, &parts[0].1, true), tangent(parts[1].0, &parts[1].1, false)));
    assert!(same_direction(tangent(parts[1].0, &parts[1].1, true), tangent(parts[2].0, &parts[2].1, false)));
}

/// Two circles of radius 10 around (0, 0) and (12, 0), walked clockwise over their tops, meet
/// at (6, 8) in a valley.
fn valley_between_two_arcs() -> Path {
    let a = (8.0_f64).atan2(6.0);
    let b = (8.0_f64).atan2(-6.0);
    PathBuilder::new()
        .move_to(p(-10.0, 0.0))
        .arc(circle_arc(0.0, 0.0, 10.0, PI, -(PI - a)))
        .arc(circle_arc(12.0, 0.0, 10.0, b, -b))
        .build()
        .unwrap()
}

#[test]
fn a_fillet_between_two_arcs_sits_at_the_symmetric_tangent_circle() {
    // Outside both circles: |c| = |c - (12, 0)| = 10 + r, so c = (6, sqrt((10 + r)^2 - 36)).
    let r = 4.0;
    let cy = ((10.0_f64 + r).powi(2) - 36.0).sqrt();
    let out = valley_between_two_arcs().cut_corner(p(6.0, 7.9), 1.0, CornerCut::Fillet { radius: r }).unwrap();
    let parts = pieces(&out);
    assert_eq!(parts.len(), 3);
    let fillet = arc_of(&parts[1].1);
    assert!(fillet.center().distance(p(6.0, cy)) < 1e-6, "{:?}", fillet.center());
    assert!((fillet.semi_diameters().0.length() - r).abs() < 1e-9);
    let on_first = p(6.0 * 10.0 / 14.0, cy * 10.0 / 14.0);
    let on_second = p(12.0 - 6.0 * 10.0 / 14.0, cy * 10.0 / 14.0);
    assert!(parts[1].0.distance(on_first) < 1e-6, "{:?}", parts[1].0);
    assert!(parts[1].1.end().distance(on_second) < 1e-6, "{:?}", parts[1].1.end());
    // Lowest point of the fillet: straight below its centre.
    assert!(fillet.point_at_fraction(0.5).distance(p(6.0, cy - r)) < 1e-6);
    assert!(same_direction(tangent(parts[0].0, &parts[0].1, true), tangent(parts[1].0, &parts[1].1, false)));
    assert!(same_direction(tangent(parts[1].0, &parts[1].1, true), tangent(parts[2].0, &parts[2].1, false)));
}

#[test]
fn a_fillet_between_a_line_and_a_bezier_is_tangent_to_both_at_the_stated_radius() {
    // y = x^2 / 10 from the origin; the line comes down the y axis.
    let path = PathBuilder::new()
        .move_to(p(0.0, 10.0))
        .line_to(p(0.0, 0.0))
        .cubic_to(p(10.0 / 3.0, 0.0), p(20.0 / 3.0, 10.0 / 3.0), p(10.0, 10.0))
        .build()
        .unwrap();
    let r = 2.0;
    let out = path.cut_corner(p(0.1, 0.1), 1.0, CornerCut::Fillet { radius: r }).unwrap();
    let parts = pieces(&out);
    assert_eq!(parts.len(), 3);
    let fillet = arc_of(&parts[1].1);
    let c = fillet.center();
    assert!((fillet.semi_diameters().0.length() - r).abs() < 1e-9);
    // Distance r from the line x = 0 and from the parabola (independent sampler), and the
    // tangent points are the feet of those perpendiculars.
    assert!((c.x - r).abs() < 1e-6, "{c:?}");
    let parabola = Segment::Cubic { c1: p(10.0 / 3.0, 0.0), c2: p(20.0 / 3.0, 10.0 / 3.0), to: p(10.0, 10.0) };
    let d = distance_to(p(0.0, 0.0), &parabola, c);
    assert!((d - r).abs() < 1e-3, "{d}");
    assert!((parts[1].0.distance(c) - r).abs() < 1e-6 && (parts[1].1.end().distance(c) - r).abs() < 1e-6);
    let end = parts[1].1.end();
    assert!((end.y - end.x * end.x / 10.0).abs() < 1e-6, "the tangent point lies on the parabola: {end:?}");
    assert!(same_direction(tangent(parts[0].0, &parts[0].1, true), tangent(parts[1].0, &parts[1].1, false)));
    assert!(same_direction(tangent(parts[1].0, &parts[1].1, true), tangent(parts[2].0, &parts[2].1, false)));
    assert!(parts[2].1.end().distance(p(10.0, 10.0)) < 1e-9);
}

#[test]
fn chamfer_distances_on_a_curved_side_are_measured_along_the_curve() {
    // 3 mm down the line, 4 mm along the circle: 4 / 10 rad from its lowest point.
    let out = line_then_circle()
        .cut_corner(p(0.1, 1.0), 1.5, CornerCut::ChamferTwo { first: 3.0, second: 4.0 })
        .unwrap();
    let parts = pieces(&out);
    assert_eq!(parts.len(), 3);
    let on_circle = p(10.0 * 0.4_f64.sin(), 10.0 - 10.0 * 0.4_f64.cos());
    assert!(parts[0].1.end().distance(p(0.0, 3.0)) < 1e-6);
    let Segment::Line { to } = parts[1].1 else { unreachable!() };
    assert!(to.distance(on_circle) < 1e-6, "{to:?} vs {on_circle:?}");
    assert!(parts[2].0.distance(on_circle) < 1e-6);
    // The same pair the other way round puts 4 mm on the line and 3 mm on the circle.
    let swapped = line_then_circle()
        .cut_corner(p(1.0, 0.1), 1.5, CornerCut::ChamferTwo { first: 3.0, second: 4.0 })
        .unwrap();
    let parts = pieces(&swapped);
    assert!(parts[0].1.end().distance(p(0.0, 4.0)) < 1e-6, "{:?}", parts[0].1.end());
    assert!(parts[2].0.distance(p(10.0 * 0.3_f64.sin(), 10.0 - 10.0 * 0.3_f64.cos())) < 1e-6);
}

#[test]
fn an_equal_chamfer_between_two_arcs_cuts_the_same_arc_length_from_each() {
    let out = valley_between_two_arcs().cut_corner(p(6.0, 7.9), 1.0, CornerCut::ChamferEqual { distance: 2.0 }).unwrap();
    let parts = pieces(&out);
    assert_eq!(parts.len(), 3);
    // 2 mm on a radius-10 circle is 0.2 rad: back from angle a on the first, on from b on the second.
    let a = (8.0_f64).atan2(6.0);
    let b = (8.0_f64).atan2(-6.0);
    assert!(parts[0].1.end().distance(p(10.0 * (a + 0.2).cos(), 10.0 * (a + 0.2).sin())) < 1e-6);
    assert!(parts[2].0.distance(p(12.0 + 10.0 * (b - 0.2).cos(), 10.0 * (b - 0.2).sin())) < 1e-6);
}

#[test]
fn an_angled_chamfer_on_a_curved_side_meets_the_circle_at_the_stated_angle() {
    let out = line_then_circle()
        .cut_corner(p(0.1, 1.0), 1.5, CornerCut::ChamferAngle { distance: 3.0, angle_deg: 45.0 })
        .unwrap();
    let parts = pieces(&out);
    assert_eq!(parts.len(), 3);
    assert!(parts[0].1.end().distance(p(0.0, 3.0)) < 1e-6);
    let Segment::Line { to } = parts[1].1 else { unreachable!() };
    // On the circle around (0, 10) and on the ray leaving (0, 3) at 45 degrees off the line.
    assert!((to.distance(p(0.0, 10.0)) - 10.0).abs() < 1e-6, "{to:?}");
    assert!((to.x - (3.0 - to.y)).abs() < 1e-6, "{to:?}");
    assert!(to.y < 3.0 && to.x > 0.0);
    assert!(parts[2].0.distance(to) < 1e-9);
}

#[test]
fn curved_corner_cuts_report_typed_failures_and_leave_the_input_alone() {
    let before = line_then_circle();
    // Only 3 mm of circle is left: the tangent circle of radius 8 does not fit on it.
    let short = PathBuilder::new()
        .move_to(p(0.0, 20.0))
        .line_to(p(0.0, 0.0))
        .arc(circle_arc(0.0, 10.0, 10.0, -FRAC_PI_2, 0.3))
        .build()
        .unwrap();
    assert_eq!(short.cut_corner(p(0.1, 0.1), 1.0, CornerCut::Fillet { radius: 8.0 }), Err(CornerError::TooLarge));
    assert_eq!(
        before.cut_corner(p(0.1, 0.1), 1.0, CornerCut::ChamferEqual { distance: 30.0 }),
        Err(CornerError::TooLarge)
    );
    assert_eq!(before.cut_corner(p(0.1, 0.1), 1.0, CornerCut::Fillet { radius: -1.0 }), Err(CornerError::InvalidValue));
    // A circle is smooth at its seam: tangent continuous means there is no corner.
    let ring = Path::circle(p(0.0, 0.0), 5.0).unwrap();
    assert_eq!(ring.cut_corner(p(5.0, 0.0), 1.0, CornerCut::Fillet { radius: 1.0 }), Err(CornerError::Collinear));
    assert_eq!(before, line_then_circle());
}

// ---------------------------------------------------------------------------------------------
// Extending a Bézier past the reach of its polynomial

/// The parabola `y = x^2 / 10` from the origin to (10, 10); it ends heading (1, 2).
fn parabola() -> Path {
    PathBuilder::new()
        .move_to(p(0.0, 0.0))
        .cubic_to(p(10.0 / 3.0, 0.0), p(20.0 / 3.0, 10.0 / 3.0), p(10.0, 10.0))
        .build()
        .unwrap()
}

#[test]
fn a_bezier_end_far_from_its_boundary_continues_along_the_end_tangent() {
    // The wall x = 100 lies 9 parameter lengths ahead, beyond the cubic's own reach. The end
    // continues straight along (1, 2): the line from (10, 10) reaches the wall at y = 190.
    let wall = Path::line(p(100.0, -500.0), p(100.0, 500.0)).unwrap();
    let out = parabola().extend_end(EndpointRef::End(0), &[&wall]).unwrap();
    let sp = &out.subpaths()[0];
    assert!(sp.end().distance(p(100.0, 190.0)) < 1e-6, "{:?}", sp.end());
    // The drawn curve is unchanged up to the old end and the join is tangent continuous.
    let parts = pieces(&out);
    assert_eq!(parts.len(), 2);
    assert!(parts[1].0.distance(p(10.0, 10.0)) < 1e-9);
    let before = pieces(&parabola());
    assert_eq!(parts[0].1, before[0].1);
    assert!(same_direction(tangent(parts[0].0, &parts[0].1, true), tangent(parts[1].0, &parts[1].1, false)));
}

#[test]
fn a_bezier_start_far_from_its_boundary_continues_backwards_along_the_start_tangent() {
    // The start heads (1, 0) at the origin; the wall x = -60 lies behind it.
    let wall = Path::line(p(-60.0, -500.0), p(-60.0, 500.0)).unwrap();
    let out = parabola().extend_end(EndpointRef::Start(0), &[&wall]).unwrap();
    let sp = &out.subpaths()[0];
    assert!(sp.start().distance(p(-60.0, 0.0)) < 1e-6, "{:?}", sp.start());
    assert!(sp.end().distance(p(10.0, 10.0)) < 1e-9);
}

#[test]
fn a_boundary_within_reach_still_follows_the_cubic_and_nothing_ahead_is_refused() {
    let near_wall = Path::line(p(20.0, -500.0), p(20.0, 500.0)).unwrap();
    let out = parabola().extend_end(EndpointRef::End(0), &[&near_wall]).unwrap();
    let end = out.subpaths()[0].end();
    assert!((end.x - 20.0).abs() < 1e-6 && (end.y - 40.0).abs() < 1e-6, "still y = x^2/10: {end:?}");
    assert_eq!(out.subpaths()[0].segments().len(), 1, "still one cubic");
    // A wall behind the end, or one the tangent ray never meets, is refused.
    let behind = Path::line(p(5.0, -500.0), p(5.0, 500.0)).unwrap();
    assert_eq!(parabola().extend_end(EndpointRef::End(0), &[&behind]), Err(ExtendError::NoBoundaryAhead));
    let sideways = Path::line(p(11.0, -500.0), p(11.0, -400.0)).unwrap();
    assert_eq!(parabola().extend_end(EndpointRef::End(0), &[&sideways]), Err(ExtendError::NoBoundaryAhead));
}
