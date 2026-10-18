//! Public contract: drawing primitives (E01). Oracles are closed-form areas and extents.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use std::f64::consts::PI;

use burnburn_geometry::{GeometryError, Path, Point, Segment, Shape, Winding};
use burnburn_types::{Angle, Length};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn deg(d: f64) -> Angle {
    Angle::from_degrees(d).unwrap()
}

#[test]
fn circle_is_exact_arcs_with_analytic_area_and_bounds() {
    let c = Shape::Circle { center: Point::new(50.0, 20.0), radius: mm(10.0) }.to_path().unwrap();
    let sp = &c.subpaths()[0];
    assert!(sp.is_closed() && sp.segments().iter().all(|s| matches!(s, Segment::Arc(_))));
    assert!((c.signed_area() - PI * 100.0).abs() < 1e-9, "{}", c.signed_area());
    let b = c.bounds().unwrap();
    assert!((b.min().x - 40.0).abs() < 1e-12 && (b.max().y - 30.0).abs() < 1e-12, "{b:?}");
}

#[test]
fn rotated_ellipse_has_analytic_bounds_and_area() {
    // Semi-axes 20 and 10 rotated by 90° → x extent ±10, y extent ±20; area π·20·10.
    let e =
        Shape::Ellipse { center: Point::ORIGIN, rx: mm(20.0), ry: mm(10.0), rotation: deg(90.0) }.to_path().unwrap();
    assert!((e.signed_area() - PI * 200.0).abs() < 1e-9);
    let b = e.bounds().unwrap();
    assert!((b.width() - 20.0).abs() < 1e-9 && (b.height() - 40.0).abs() < 1e-9, "{b:?}");
}

#[test]
fn polygon_and_star_areas_are_closed_form() {
    // Regular hexagon with circumradius r: area = 3√3/2 · r².
    let hex =
        Shape::Polygon { center: Point::ORIGIN, radius: mm(10.0), sides: 6, rotation: Angle::ZERO }.to_path().unwrap();
    assert!((hex.signed_area() - 1.5 * 3f64.sqrt() * 100.0).abs() < 1e-9);
    assert_eq!(hex.subpaths()[0].winding(), Some(Winding::CounterClockwise));
    // Star with n tips: 2n triangles of area ½·R·r·sin(π/n).
    let n = 5.0;
    let star = Shape::Star { center: Point::ORIGIN, outer: mm(10.0), inner: mm(4.0), points: 5, rotation: deg(90.0) }
        .to_path()
        .unwrap();
    assert!((star.signed_area() - 2.0 * n * 0.5 * 10.0 * 4.0 * (PI / n).sin()).abs() < 1e-9);
    assert_eq!(star.subpaths()[0].segments().len(), 9); // ten vertices, closing line implicit
}

#[test]
fn degenerate_shapes_are_typed_errors() {
    let r = Shape::Polygon { center: Point::ORIGIN, radius: mm(10.0), sides: 2, rotation: Angle::ZERO }.to_path();
    assert_eq!(r, Err(GeometryError::TooFewPoints));
    assert_eq!(Shape::Circle { center: Point::ORIGIN, radius: mm(0.0) }.to_path(), Err(GeometryError::DegenerateArc));
    assert_eq!(Path::polyline(&[Point::ORIGIN], false), Err(GeometryError::TooFewPoints));
    assert_eq!(Path::circle(Point::new(f64::NAN, 0.0), 1.0), Err(GeometryError::NotFinite));
}
