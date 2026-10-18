//! Public contract: separate tolerance types and flattening within the declared budget.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use burnburn_geometry::{
    DisplayTolerance, EllipticalArc, HitTestTolerance, PathBuilder, Point, ProductionTolerance, ToleranceError,
};
use burnburn_types::{Angle, Length};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

#[test]
fn tolerances_reject_non_positive_values() {
    assert_eq!(ProductionTolerance::new(mm(0.0)), Err(ToleranceError::NotPositive));
    assert_eq!(DisplayTolerance::new(mm(-0.1)), Err(ToleranceError::NotPositive));
    assert_eq!(HitTestTolerance::new(mm(0.0)), Err(ToleranceError::NotPositive));
    assert_eq!(ProductionTolerance::new(mm(0.01)).unwrap().mm(), 0.01);
}

/// Distance from `q` to segment `a`–`b`.
fn dist_to_segment(q: Point, a: Point, b: Point) -> f64 {
    let ab = b - a;
    let len2 = ab.dot(ab);
    let t = if len2 == 0.0 { 0.0 } else { ((q - a).dot(ab) / len2).clamp(0.0, 1.0) };
    q.distance(a + ab * t)
}

fn dist_to_polyline(q: Point, pts: &[Point]) -> f64 {
    pts.windows(2).map(|w| dist_to_segment(q, w[0], w[1])).fold(f64::MAX, f64::min)
}

#[test]
fn flattened_circle_stays_within_production_tolerance() {
    let r = 50.0;
    let circle = EllipticalArc::circle(Point::ORIGIN, r, Angle::ZERO, Angle::from_degrees(360.0).unwrap()).unwrap();
    let path = PathBuilder::new().move_to(circle.start_point()).arc(circle).close().build().unwrap();
    let tol = ProductionTolerance::new(mm(0.01)).unwrap();
    let poly = &path.flatten(&tol)[0];
    assert!(poly.closed);
    assert!(poly.points.len() > 8, "a circle needs more than a handful of chords: {}", poly.points.len());
    for w in poly.points.windows(2) {
        assert!((w[0].distance(Point::ORIGIN) - r).abs() < 1e-9, "vertices lie on the circle");
        // The largest chord deviation of a circle is at the chord midpoint.
        let mid = w[0].lerp(w[1], 0.5);
        assert!(r - mid.distance(Point::ORIGIN) <= 0.01 + 1e-12, "sagitta exceeds tolerance");
    }
    let coarse = path.flatten(&DisplayTolerance::new(mm(1.0)).unwrap());
    assert!(coarse[0].points.len() < poly.points.len(), "a coarse screen budget gives fewer chords");
}

#[test]
fn flattened_cubic_stays_within_tolerance() {
    let (p0, p1, p2, p3) =
        (Point::new(0.0, 0.0), Point::new(-10.0, 40.0), Point::new(60.0, 40.0), Point::new(50.0, 0.0));
    let path = PathBuilder::new().move_to(p0).cubic_to(p1, p2, p3).build().unwrap();
    let tol = 0.005;
    let poly = &path.flatten(&ProductionTolerance::new(mm(tol)).unwrap())[0];
    // Independent oracle: dense samples of the Bernstein form must be near the polyline.
    for i in 0..=2000 {
        let t = i as f64 / 2000.0;
        let mt = 1.0 - t;
        let q = Point::new(
            mt * mt * mt * p0.x + 3.0 * mt * mt * t * p1.x + 3.0 * mt * t * t * p2.x + t * t * t * p3.x,
            mt * mt * mt * p0.y + 3.0 * mt * mt * t * p1.y + 3.0 * mt * t * t * p2.y + t * t * t * p3.y,
        );
        let d = dist_to_polyline(q, &poly.points);
        assert!(d <= tol + 1e-9, "t={t}: deviation {d}");
    }
    assert_eq!(*poly.points.last().unwrap(), p3, "endpoint is exact");
}
