//! W2-L2 Tangent Circle. Oracles: classical closed forms (incircle and excircles, the Descartes
//! circle theorem), circles worked out by hand, and an independent tangency check against the
//! targets. The property test samples a circle first and builds targets that touch it.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use burnburn_geometry::{
    Circle, EllipticalArc, Point, Segment, TangentError, TangentTarget, nearest_circle, tangent_circles,
    tangent_circles_radius,
};
use burnburn_types::Angle;
use proptest::prelude::*;

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn pt(x: f64, y: f64) -> TangentTarget {
    TangentTarget::Point(p(x, y))
}

fn line(a: (f64, f64), b: (f64, f64)) -> TangentTarget {
    TangentTarget::Line((p(a.0, a.1), p(b.0, b.1)))
}

fn circ(x: f64, y: f64, r: f64) -> TangentTarget {
    TangentTarget::Circle(Circle { center: p(x, y), radius: r })
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-7
}

fn has(circles: &[Circle], x: f64, y: f64, r: f64) -> bool {
    circles.iter().any(|c| near(c.center.x, x) && near(c.center.y, y) && near(c.radius, r))
}

/// Independent check: the distance from the circle to the target.
fn gap(t: &TangentTarget, c: &Circle) -> f64 {
    match t {
        TangentTarget::Point(q) => (q.distance(c.center) - c.radius).abs(),
        TangentTarget::Line((a, b)) => {
            let d = *b - *a;
            let dist = ((c.center - *a).cross(d) / d.length()).abs();
            (dist - c.radius).abs()
        }
        TangentTarget::Circle(o) => {
            let d = o.center.distance(c.center);
            (d - (o.radius + c.radius)).abs().min((d - (o.radius - c.radius).abs()).abs())
        }
    }
}

fn assert_tangent(targets: &[TangentTarget], circles: &[Circle]) {
    for c in circles {
        assert!(c.radius > 0.0);
        for t in targets {
            assert!(gap(t, c) < 1e-6, "{c:?} is not tangent to {t:?} (gap {})", gap(t, c));
        }
    }
}

#[test]
fn three_points_have_exactly_the_circumcircle() {
    let r = tangent_circles([pt(0.0, 0.0), pt(10.0, 0.0), pt(0.0, 10.0)]).unwrap();
    assert_eq!(r.len(), 1);
    assert!(has(&r, 5.0, 5.0, 50f64.sqrt()), "{r:?}");
}

#[test]
fn two_points_and_a_line_give_two_circles() {
    // Through (0,1) and (0,4), touching the x axis: centres (±2, 2.5), radius 2.5.
    let t = [pt(0.0, 1.0), pt(0.0, 4.0), line((-9.0, 0.0), (9.0, 0.0))];
    let r = tangent_circles(t).unwrap();
    assert_eq!(r.len(), 2, "{r:?}");
    assert!(has(&r, 2.0, 2.5, 2.5) && has(&r, -2.0, 2.5, 2.5), "{r:?}");
    assert_tangent(&t, &r);
}

#[test]
fn the_second_solution_of_two_points_at_equal_height_lies_at_infinity() {
    // Through (±3, 4) touching y = 0: only the circle with centre (0, 25/8).
    let t = [pt(-3.0, 4.0), pt(3.0, 4.0), line((-9.0, 0.0), (9.0, 0.0))];
    let r = tangent_circles(t).unwrap();
    assert_eq!(r.len(), 1, "{r:?}");
    assert!(has(&r, 0.0, 3.125, 3.125), "{r:?}");
}

#[test]
fn a_3_4_5_triangle_has_one_incircle_and_three_excircles() {
    let t = [line((0.0, 0.0), (4.0, 0.0)), line((0.0, 0.0), (0.0, 3.0)), line((4.0, 0.0), (0.0, 3.0))];
    let r = tangent_circles(t).unwrap();
    assert_eq!(r.len(), 4, "{r:?}");
    // Incircle r = 1; excircles r = Area / (s - side) = 6/1, 6/2, 6/3 with centres worked out by hand.
    assert!(has(&r, 1.0, 1.0, 1.0), "{r:?}");
    assert!(has(&r, 6.0, 6.0, 6.0), "opposite the hypotenuse: {r:?}");
    assert!(has(&r, 3.0, -3.0, 3.0), "{r:?}");
    assert!(has(&r, -2.0, 2.0, 2.0), "{r:?}");
    assert!(r.windows(2).all(|w| w[0].radius <= w[1].radius), "smallest first: {r:?}");
}

#[test]
fn parallel_lines_and_a_circle_give_four_circles() {
    let t = [line((-9.0, 0.0), (9.0, 0.0)), line((-9.0, 4.0), (9.0, 4.0)), circ(0.0, 2.0, 1.0)];
    let r = tangent_circles(t).unwrap();
    assert_eq!(r.len(), 4, "{r:?}");
    for (x, y) in [(3.0, 2.0), (-3.0, 2.0), (1.0, 2.0), (-1.0, 2.0)] {
        assert!(has(&r, x, y, 2.0), "({x},{y}): {r:?}");
    }
    assert_tangent(&t, &r);
}

#[test]
fn three_mutually_tangent_unit_circles_have_the_descartes_soddy_circles() {
    let h = 3f64.sqrt();
    let t = [circ(0.0, 0.0, 1.0), circ(2.0, 0.0, 1.0), circ(1.0, h, 1.0)];
    let r = tangent_circles(t).unwrap();
    // Curvatures 3 ± 2√3: the small inner circle and the large outer one.
    let inner = 1.0 / (3.0 + 2.0 * h);
    let outer = 1.0 / (2.0 * h - 3.0);
    let centroid = (1.0, h / 3.0);
    assert!(has(&r, centroid.0, centroid.1, inner), "{r:?}");
    assert!(has(&r, centroid.0, centroid.1, outer), "{r:?}");
    assert_tangent(&t, &r);
    // The targets themselves are not answers.
    assert!(!has(&r, 0.0, 0.0, 1.0));
}

#[test]
fn concentric_circles_have_no_common_tangent_circle() {
    let t = [circ(0.0, 0.0, 2.0), circ(0.0, 0.0, 4.0), circ(0.0, 0.0, 6.0)];
    assert_eq!(tangent_circles(t), Err(TangentError::NoSolution));
}

#[test]
fn repeated_targets_are_refused() {
    assert_eq!(tangent_circles([pt(1.0, 1.0), pt(1.0, 1.0), pt(4.0, 0.0)]), Err(TangentError::Coincident));
    assert_eq!(
        tangent_circles([line((0.0, 0.0), (1.0, 0.0)), line((5.0, 0.0), (9.0, 0.0)), pt(0.0, 3.0)]),
        Err(TangentError::Coincident)
    );
    assert_eq!(
        tangent_circles([circ(0.0, 0.0, 1.0), circ(0.0, 0.0, 1.0), pt(9.0, 9.0)]),
        Err(TangentError::Coincident)
    );
    // A zero-length line is not a line.
    assert_eq!(
        tangent_circles([line((1.0, 1.0), (1.0, 1.0)), pt(2.0, 2.0), pt(5.0, 0.0)]),
        Err(TangentError::Coincident)
    );
    assert_eq!(tangent_circles([pt(f64::NAN, 0.0), pt(1.0, 0.0), pt(0.0, 1.0)]), Err(TangentError::NotFinite));
}

#[test]
fn a_point_at_the_centre_of_a_circle_is_touched_from_inside_by_circles_of_half_its_radius() {
    // Through the centre of the r = 5 circle and tangent to it: radius 2.5 around a point 2.5 away.
    // With the line y = 2 the centre must also lie 2.5 from the line: (±√6, -0.5).
    let t = [circ(0.0, 0.0, 5.0), pt(0.0, 0.0), line((-30.0, 2.0), (30.0, 2.0))];
    let r = tangent_circles(t).unwrap();
    assert_eq!(r.len(), 2, "{r:?}");
    let h = 6f64.sqrt();
    assert!(has(&r, h, -0.5, 2.5) && has(&r, -h, -0.5, 2.5), "{r:?}");
    assert_tangent(&t, &r);
    // A line too far away for such a circle.
    let t = [circ(0.0, 0.0, 5.0), pt(0.0, 0.0), line((-30.0, 20.0), (30.0, 20.0))];
    assert_eq!(tangent_circles(t), Err(TangentError::NoSolution));
}

#[test]
fn a_known_radius_gives_every_circle_of_that_size() {
    // Radius 2 through (0,3) touching y = 0: centres (±√3, 2).
    let t = [pt(0.0, 3.0), line((-9.0, 0.0), (9.0, 0.0))];
    let r = tangent_circles_radius(t, 2.0).unwrap();
    assert_eq!(r.len(), 2, "{r:?}");
    let h = 3f64.sqrt();
    assert!(has(&r, -h, 2.0, 2.0) && has(&r, h, 2.0, 2.0), "{r:?}");
    assert_tangent(&t, &r);
    // Two lines crossing at a right angle: four circles, one per quadrant.
    let t = [line((-9.0, 0.0), (9.0, 0.0)), line((0.0, -9.0), (0.0, 9.0))];
    let r = tangent_circles_radius(t, 1.5).unwrap();
    assert_eq!(r.len(), 4, "{r:?}");
    for (x, y) in [(1.5, 1.5), (-1.5, 1.5), (1.5, -1.5), (-1.5, -1.5)] {
        assert!(has(&r, x, y, 1.5), "{r:?}");
    }
}

#[test]
fn two_circles_with_a_known_radius_meet_where_both_offsets_cross() {
    // Circles r=3 at (±4, 0); radius 1 touching both from outside: centres at distance 4 from both,
    // i.e. (0, ±√(16-16)) … use r=2: distance 5 from both centres → (0, ±3).
    let t = [circ(-4.0, 0.0, 3.0), circ(4.0, 0.0, 3.0)];
    let r = tangent_circles_radius(t, 2.0).unwrap();
    assert!(has(&r, 0.0, 3.0, 2.0) && has(&r, 0.0, -3.0, 2.0), "{r:?}");
    assert_tangent(&t, &r);
}

#[test]
fn a_strip_with_its_half_width_as_radius_has_infinitely_many_circles() {
    let t = [line((-9.0, 0.0), (9.0, 0.0)), line((-9.0, 4.0), (9.0, 4.0))];
    assert_eq!(tangent_circles_radius(t, 2.0), Err(TangentError::Degenerate));
    assert_eq!(tangent_circles_radius(t, 1.0), Err(TangentError::NoSolution));
    assert_eq!(tangent_circles_radius(t, 3.0), Err(TangentError::NoSolution));
}

#[test]
fn radius_input_is_validated() {
    let t = [pt(0.0, 0.0), pt(5.0, 0.0)];
    assert_eq!(tangent_circles_radius(t, 0.0), Err(TangentError::RadiusNotPositive));
    assert_eq!(tangent_circles_radius(t, -1.0), Err(TangentError::RadiusNotPositive));
    assert_eq!(tangent_circles_radius(t, f64::NAN), Err(TangentError::NotFinite));
    // Points further apart than the diameter cannot both lie on the circle.
    assert_eq!(tangent_circles_radius(t, 1.0), Err(TangentError::NoSolution));
    assert_eq!(tangent_circles_radius([pt(1.0, 1.0), pt(1.0, 1.0)], 1.0), Err(TangentError::Coincident));
}

#[test]
fn the_circle_nearest_the_pointer_is_chosen() {
    let t = [pt(0.0, 1.0), pt(0.0, 4.0), line((-9.0, 0.0), (9.0, 0.0))];
    let r = tangent_circles(t).unwrap();
    let right = nearest_circle(&r, p(6.0, 3.0)).unwrap();
    assert!(near(right.center.x, 2.0));
    let left = nearest_circle(&r, p(-6.0, 3.0)).unwrap();
    assert!(near(left.center.x, -2.0));
    assert_eq!(nearest_circle(&[], p(0.0, 0.0)), None);
}

#[test]
fn contact_points_lie_on_both_the_circle_and_the_target() {
    let c = Circle { center: p(2.0, 2.5), radius: 2.5 };
    let on_line = line((-9.0, 0.0), (9.0, 0.0));
    let q = on_line.contact(&c);
    assert!(near(q.x, 2.0) && near(q.y, 0.0), "{q:?}");
    let q = pt(0.0, 1.0).contact(&c);
    assert!(near(q.x, 0.0) && near(q.y, 1.0));
    // Externally tangent circle: the contact is on the line of centres, `radius` from our centre.
    let outer = circ(7.0, 2.5, 2.5);
    let q = outer.contact(&c);
    assert!(near(q.x, 4.5) && near(q.y, 2.5), "{q:?}");
    // Our circle inside a larger target: the touch is the point of ours farthest from the target's centre.
    let around = circ(2.0, -5.0, 10.0);
    let q = around.contact(&c);
    assert!(near(q.x, 2.0) && near(q.y, 5.0), "{q:?}");
    // The target inside ours: the touch is on the target, towards its centre.
    let inner = circ(2.0, 3.5, 1.5);
    let q = inner.contact(&c);
    assert!(near(q.x, 2.0) && near(q.y, 5.0), "{q:?}");
}

#[test]
fn only_straight_and_circular_segments_become_targets() {
    let line_seg = Segment::Line { to: p(4.0, 0.0) };
    assert_eq!(TangentTarget::from_segment(p(0.0, 0.0), &line_seg), Some(line((0.0, 0.0), (4.0, 0.0))));
    assert_eq!(TangentTarget::from_segment(p(4.0, 0.0), &Segment::Line { to: p(4.0, 0.0) }), None);
    let arc =
        EllipticalArc::circle(p(1.0, 1.0), 3.0, Angle::from_degrees(0.0).unwrap(), Angle::from_degrees(90.0).unwrap())
            .unwrap();
    assert_eq!(TangentTarget::from_segment(arc.start_point(), &Segment::Arc(arc)), Some(circ(1.0, 1.0, 3.0)));
    let ellipse = EllipticalArc::new(
        p(0.0, 0.0),
        burnburn_geometry::Vector::new(4.0, 0.0),
        burnburn_geometry::Vector::new(0.0, 2.0),
        Angle::from_degrees(0.0).unwrap(),
        Angle::from_degrees(90.0).unwrap(),
    )
    .unwrap();
    assert_eq!(TangentTarget::from_segment(ellipse.start_point(), &Segment::Arc(ellipse)), None);
    let cubic = Segment::Cubic { c1: p(1.0, 1.0), c2: p(2.0, 1.0), to: p(3.0, 0.0) };
    assert_eq!(TangentTarget::from_segment(p(0.0, 0.0), &cubic), None);
}

#[derive(Debug, Clone, Copy)]
enum Kind {
    Point,
    Line,
    CircleOutside,
    CircleInside,
    CircleAround,
}

fn kind() -> impl Strategy<Value = Kind> {
    prop_oneof![
        Just(Kind::Point),
        Just(Kind::Line),
        Just(Kind::CircleOutside),
        Just(Kind::CircleInside),
        Just(Kind::CircleAround)
    ]
}

/// A target that touches `known` at the given angle.
fn touching(known: Circle, kind: Kind, angle: f64, size: f64) -> TangentTarget {
    let u = p(angle.cos(), angle.sin()).to_vector();
    let contact = known.center + u * known.radius;
    match kind {
        Kind::Point => TangentTarget::Point(contact),
        Kind::Line => {
            let along = burnburn_geometry::Vector::new(-u.y, u.x);
            TangentTarget::Line((contact - along * 5.0, contact + along * 5.0))
        }
        // Another circle on the far side of the contact.
        Kind::CircleOutside => TangentTarget::Circle(Circle { center: contact + u * size, radius: size }),
        // A smaller circle inside `known`, touching it at the contact.
        Kind::CircleInside => {
            let r = known.radius * 0.5 * (size / (size + 1.0)).max(0.1);
            TangentTarget::Circle(Circle { center: contact - u * r, radius: r })
        }
        // A larger circle around `known`, touching it at the contact.
        Kind::CircleAround => {
            let r = known.radius + size;
            TangentTarget::Circle(Circle { center: contact - u * r, radius: r })
        }
    }
}

proptest! {
    #[test]
    fn a_circle_sampled_first_is_among_the_solutions_for_targets_that_touch_it(
        cx in -50.0f64..50.0, cy in -50.0f64..50.0, r in 2.0f64..30.0,
        kinds in (kind(), kind(), kind()), base in 0.0f64..std::f64::consts::TAU,
        sep in (0.9f64..2.0, 0.9f64..2.0), size in 0.5f64..20.0,
    ) {
        let known = Circle { center: p(cx, cy), radius: r };
        // Contacts at least 0.9 rad (≈ 50°) apart, so no two targets coincide.
        let angles = [base, base + sep.0 + 0.1, base + sep.0 + sep.1 + 0.2];
        let targets = [
            touching(known, kinds.0, angles[0], size),
            touching(known, kinds.1, angles[1], size * 1.7),
            touching(known, kinds.2, angles[2], size * 0.6),
        ];
        match tangent_circles(targets) {
            Ok(found) => {
                assert_tangent(&targets, &found);
                let hit = found.iter().any(|c| {
                    (c.center.x - cx).abs() < 1e-5 * (1.0 + cx.abs() + r) && (c.center.y - cy).abs() < 1e-5 * (1.0 + cy.abs() + r) && (c.radius - r).abs() < 1e-5 * (1.0 + r)
                });
                prop_assert!(hit, "known circle missing in {found:?} for {targets:?}");
            }
            // Two equal-geometry targets are the only legitimate refusal for a known solution.
            Err(e) => prop_assert!(matches!(e, TangentError::Coincident), "{e:?} for {targets:?}"),
        }
    }
}
