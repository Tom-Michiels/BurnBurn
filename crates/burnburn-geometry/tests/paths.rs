//! Public contract: path construction, exact bounds, signed area and winding.
//! Oracles are analytic (hand-derived extrema and areas), not the implementation formulas.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use std::f64::consts::PI;

use burnburn_geometry::{Affine, EllipticalArc, GeometryError, Path, PathBuilder, Point, Rect, Winding};
use burnburn_types::{Angle, Length};

const EPS: f64 = 1e-9;

fn deg(d: f64) -> Angle {
    Angle::from_degrees(d).unwrap()
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Path {
    Path::rectangle(Rect::new(Point::new(x, y), Length::from_mm(w).unwrap(), Length::from_mm(h).unwrap()).unwrap())
}

#[test]
fn rectangle_area_and_winding_are_analytic() {
    let p = rect(5.0, 5.0, 120.0, 80.0);
    assert!((p.signed_area() - 9600.0).abs() < EPS, "{}", p.signed_area());
    assert_eq!(p.subpaths()[0].winding(), Some(Winding::CounterClockwise));
}

#[test]
fn mirroring_reverses_winding() {
    let p = rect(5.0, 5.0, 120.0, 80.0);
    let m = p.transform(&Affine::mirror_x());
    assert_eq!(m.subpaths()[0].winding(), Some(Winding::Clockwise));
    assert!((m.signed_area() + 9600.0).abs() < EPS, "{}", m.signed_area());
    let b = m.bounds().unwrap();
    assert!((b.min().x + 125.0).abs() < EPS && (b.max().x + 5.0).abs() < EPS, "{b:?}");
}

#[test]
fn cubic_bezier_bounds_match_analytic_extrema() {
    // B(t) for P0=(0,0) P1=(0,10) P2=(10,10) P3=(10,0): y(t) = 30t(1-t), max 7.5 at t=1/2.
    // The control polygon reaches y=10; exact bounds must not.
    let p = PathBuilder::new()
        .move_to(Point::new(0.0, 0.0))
        .cubic_to(Point::new(0.0, 10.0), Point::new(10.0, 10.0), Point::new(10.0, 0.0))
        .build()
        .unwrap();
    let b = p.bounds().unwrap();
    assert!((b.max().y - 7.5).abs() < EPS, "{b:?}");
    assert!(b.min().y.abs() < EPS && b.min().x.abs() < EPS && (b.max().x - 10.0).abs() < EPS, "{b:?}");

    // Control points overshoot the endpoints in x; the oracle is dense sampling of the Bernstein form.
    let q = PathBuilder::new()
        .move_to(Point::new(0.0, 0.0))
        .cubic_to(Point::new(-10.0, 5.0), Point::new(20.0, 5.0), Point::new(10.0, 0.0))
        .build()
        .unwrap();
    let bq = q.bounds().unwrap();
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for i in 0..=100_000 {
        let t = i as f64 / 100_000.0;
        let mt = 1.0 - t;
        let x = 3.0 * mt * mt * t * -10.0 + 3.0 * mt * t * t * 20.0 + t * t * t * 10.0;
        lo = lo.min(x);
        hi = hi.max(x);
    }
    assert!((bq.min().x - lo).abs() < 1e-6 && (bq.max().x - hi).abs() < 1e-6, "{bq:?} vs [{lo}, {hi}]");
    assert!(bq.min().x < -1.0 && bq.max().x > 10.5, "overshoot beyond endpoints is included: {bq:?}");
}

#[test]
fn arc_bounds_include_axis_extrema_inside_sweep() {
    // Quarter circle r=10 from 45° to 135°: passes the top (0,10) but no x extremum.
    let arc = EllipticalArc::circle(Point::ORIGIN, 10.0, deg(45.0), deg(90.0)).unwrap();
    let p = PathBuilder::new().move_to(arc.start_point()).arc(arc).build().unwrap();
    let b = p.bounds().unwrap();
    let h = 10.0 / 2f64.sqrt();
    assert!((b.max().y - 10.0).abs() < EPS, "{b:?}");
    assert!((b.min().y - h).abs() < EPS, "{b:?}");
    assert!((b.min().x + h).abs() < EPS && (b.max().x - h).abs() < EPS, "{b:?}");
    assert_eq!(p.subpaths()[0].segments().len(), 1, "no connecting line when already at arc start");
}

#[test]
fn full_circle_signed_area_is_pi_r_squared() {
    let ccw = EllipticalArc::circle(Point::new(3.0, 4.0), 10.0, deg(0.0), deg(360.0)).unwrap();
    let p = PathBuilder::new().move_to(ccw.start_point()).arc(ccw).close().build().unwrap();
    assert!((p.signed_area() - PI * 100.0).abs() < EPS, "{}", p.signed_area());
    let cw = EllipticalArc::circle(Point::new(3.0, 4.0), 10.0, deg(0.0), deg(-360.0)).unwrap();
    let q = PathBuilder::new().move_to(cw.start_point()).arc(cw).close().build().unwrap();
    assert_eq!(q.subpaths()[0].winding(), Some(Winding::Clockwise));
}

#[test]
fn circle_under_nonuniform_scale_is_exact_ellipse() {
    let c = EllipticalArc::circle(Point::ORIGIN, 10.0, deg(0.0), deg(360.0)).unwrap();
    let p = PathBuilder::new().move_to(c.start_point()).arc(c).close().build().unwrap();
    let e = p.transform(&Affine::scale(2.0, 1.0).unwrap().then(&Affine::rotate(deg(90.0))));
    let b = e.bounds().unwrap();
    // After scaling x by 2 and a quarter turn, the ellipse is 20 wide and 40 tall.
    assert!((b.width() - 20.0).abs() < EPS && (b.height() - 40.0).abs() < EPS, "{b:?}");
    assert!((e.signed_area() - 2.0 * PI * 100.0).abs() < EPS, "{}", e.signed_area());
}

#[test]
fn path_with_hole_has_two_subpaths_with_opposite_winding() {
    let p = PathBuilder::new()
        .move_to(Point::new(0.0, 0.0))
        .line_to(Point::new(100.0, 0.0))
        .line_to(Point::new(100.0, 50.0))
        .line_to(Point::new(0.0, 50.0))
        .close()
        // Hole drawn clockwise.
        .move_to(Point::new(10.0, 10.0))
        .line_to(Point::new(10.0, 20.0))
        .line_to(Point::new(30.0, 20.0))
        .line_to(Point::new(30.0, 10.0))
        .close()
        .build()
        .unwrap();
    assert_eq!(p.subpaths().len(), 2);
    assert_eq!(p.subpaths()[0].winding(), Some(Winding::CounterClockwise));
    assert_eq!(p.subpaths()[1].winding(), Some(Winding::Clockwise));
    assert!((p.signed_area() - (5000.0 - 200.0)).abs() < EPS, "{}", p.signed_area());
}

#[test]
fn invalid_path_input_is_a_typed_error() {
    assert_eq!(PathBuilder::new().line_to(Point::new(1.0, 1.0)).build(), Err(GeometryError::MissingStart));
    assert_eq!(
        PathBuilder::new().move_to(Point::ORIGIN).line_to(Point::new(f64::NAN, 0.0)).build(),
        Err(GeometryError::NotFinite)
    );
    assert_eq!(EllipticalArc::circle(Point::ORIGIN, 0.0, deg(0.0), deg(90.0)), Err(GeometryError::DegenerateArc));
    assert!(Path::line(Point::ORIGIN, Point::new(1.0, 0.0)).unwrap().subpaths()[0].winding().is_none());
}
