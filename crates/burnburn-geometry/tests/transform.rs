//! Public contract: affine transforms, inverses and their failure modes.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use burnburn_geometry::{Affine, GeometryError, Point, Vector};
use burnburn_types::Angle;

/// Roundtrip budget: a few ulps of the coordinate magnitude (~1e3 mm) through two matrix products.
const ROUNDTRIP_EPS_MM: f64 = 1e-9;

#[test]
fn quarter_turn_maps_axes_analytically() {
    let r = Affine::rotate(Angle::from_degrees(90.0).unwrap());
    let p = r.apply(Point::new(1.0, 0.0));
    assert!(p.distance(Point::new(0.0, 1.0)) < 1e-15, "{p:?}");
}

#[test]
fn affine_inverse_restores_points() {
    let t = Affine::scale(2.0, 0.5)
        .unwrap()
        .then(&Affine::rotate(Angle::from_degrees(30.0).unwrap()))
        .then(&Affine::translate(Vector::new(10.0, -5.0)).unwrap());
    let inv = t.inverse().unwrap();
    for p in [Point::new(0.0, 0.0), Point::new(120.0, 80.0), Point::new(-950.5, 433.25)] {
        let moved = t.apply(p);
        assert!(moved.distance(p) > 1.0, "transform must actually move {p:?}");
        let back = inv.apply(moved);
        assert!(back.distance(p) < ROUNDTRIP_EPS_MM, "{p:?} -> {back:?}");
    }
}

#[test]
fn singular_matrix_inverse_is_typed_error() {
    assert_eq!(Affine::scale(0.0, 1.0).unwrap().inverse(), Err(GeometryError::Singular));
    // Rank-1 matrix with non-zero entries: both columns parallel.
    assert_eq!(Affine::new([1.0, 2.0, 2.0, 4.0, 3.0, 3.0]).unwrap().inverse(), Err(GeometryError::Singular));
}

#[test]
fn non_finite_affine_is_rejected() {
    assert_eq!(Affine::new([1.0, 0.0, 0.0, f64::NAN, 0.0, 0.0]), Err(GeometryError::NotFinite));
    assert_eq!(Affine::translate(Vector::new(f64::INFINITY, 0.0)), Err(GeometryError::NotFinite));
}
