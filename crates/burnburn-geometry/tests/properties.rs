//! Property tests (owner: Testing; see docs/coordination/testing.md). Invariants that must hold
//! for every well-conditioned transform and every finite path, not only for the analytical
//! fixtures in `transform.rs` / `units_and_bounds.rs`.
//!
//! Oracles are mathematical identities (inverse, translation equivariance, convex hull,
//! area scaling by the determinant), never a copy of the production formula.

#![allow(clippy::unwrap_used)] // proptest strategies and helpers outside #[test] fns

use burnburn_geometry::{Affine, Bounds, GeometryError, Path, PathBuilder, Point, Vector};
use burnburn_types::Angle;
use proptest::prelude::*;

/// Bed-scale coordinates (mm).
const COORD: f64 = 2000.0;
/// Roundtrip budget: scales are limited to [0.1, 10] (condition number <= 100) and coordinates
/// to ~3e4 mm after transform, i.e. a few ulps (~4e-12) amplified 100x. 1e-8 mm is far below any
/// production tolerance and still catches a wrong inverse by orders of magnitude.
const ROUNDTRIP_EPS_MM: f64 = 1e-8;
/// Translation only adds once per coordinate: a few ulps of 4000 mm.
const TRANSLATION_EPS_MM: f64 = 1e-9;

fn coord() -> impl Strategy<Value = f64> {
    -COORD..COORD
}

fn point() -> impl Strategy<Value = Point> {
    (coord(), coord()).prop_map(|(x, y)| Point::new(x, y))
}

fn scale_factor() -> impl Strategy<Value = f64> {
    (0.1f64..10.0, any::<bool>()).prop_map(|(s, neg)| if neg { -s } else { s })
}

/// scale -> rotate -> translate: every invertible, well-conditioned affine (incl. mirrors) up to
/// shear is reachable this way, plus a second rotation for non-axis-aligned scaling.
fn affine() -> impl Strategy<Value = Affine> {
    (-180.0f64..180.0, scale_factor(), scale_factor(), -180.0f64..180.0, coord(), coord()).prop_map(
        |(pre, sx, sy, post, tx, ty)| {
            Affine::rotate(Angle::from_degrees(pre).unwrap())
                .then(&Affine::scale(sx, sy).unwrap())
                .then(&Affine::rotate(Angle::from_degrees(post).unwrap()))
                .then(&Affine::translate(Vector::new(tx, ty)).unwrap())
        },
    )
}

#[derive(Debug, Clone)]
enum Seg {
    Line(Point),
    Cubic(Point, Point, Point),
}

fn segments() -> impl Strategy<Value = Vec<Seg>> {
    prop::collection::vec(
        prop_oneof![point().prop_map(Seg::Line), (point(), point(), point()).prop_map(|(a, b, c)| Seg::Cubic(a, b, c)),],
        1..8,
    )
}

fn build(start: Point, segs: &[Seg], closed: bool) -> Path {
    let mut b = PathBuilder::new().move_to(start);
    for s in segs {
        b = match *s {
            Seg::Line(to) => b.line_to(to),
            Seg::Cubic(c1, c2, to) => b.cubic_to(c1, c2, to),
        };
    }
    if closed {
        b = b.close();
    }
    b.build().unwrap()
}

/// All points that define the path (endpoints and control points).
fn defining_points(start: Point, segs: &[Seg]) -> Vec<Point> {
    let mut pts = vec![start];
    for s in segs {
        match *s {
            Seg::Line(to) => pts.push(to),
            Seg::Cubic(c1, c2, to) => pts.extend([c1, c2, to]),
        }
    }
    pts
}

fn endpoints(start: Point, segs: &[Seg]) -> Vec<Point> {
    let mut pts = vec![start];
    pts.extend(segs.iter().map(|s| match *s {
        Seg::Line(to) | Seg::Cubic(_, _, to) => to,
    }));
    pts
}

fn bounds_of(points: &[Point]) -> Bounds {
    let mut b = Bounds::from_point(points[0]);
    for p in &points[1..] {
        b.include(*p);
    }
    b
}

fn close(a: Point, b: Point, eps: f64) -> bool {
    a.distance(b) <= eps
}

proptest! {
    #[test]
    fn transform_inverse_restores_every_point(m in affine(), p in point()) {
        let inv = m.inverse().unwrap();
        let back = inv.apply(m.apply(p));
        prop_assert!(close(back, p, ROUNDTRIP_EPS_MM), "{p:?} -> {back:?} via {m:?}");
        let forth = m.apply(inv.apply(p));
        prop_assert!(close(forth, p, ROUNDTRIP_EPS_MM), "{p:?} -> {forth:?} via inverse of {m:?}");
    }

    #[test]
    fn transform_then_inverse_is_identity(m in affine()) {
        let id = m.then(&m.inverse().unwrap()).coefficients();
        let expected = Affine::IDENTITY.coefficients();
        // Linear part is dimensionless; translation part is in mm.
        for i in 0..4 {
            prop_assert!((id[i] - expected[i]).abs() < 1e-12, "{id:?}");
        }
        for i in 4..6 {
            prop_assert!((id[i] - expected[i]).abs() < ROUNDTRIP_EPS_MM, "{id:?}");
        }
    }

    #[test]
    fn inverse_preserves_orientation_flag(m in affine()) {
        prop_assert_eq!(m.inverse().unwrap().is_mirroring(), m.is_mirroring());
    }

    #[test]
    fn collapsed_axis_is_singular(sx in scale_factor(), angle in -180.0f64..180.0, t in point()) {
        let m = Affine::scale(sx, 0.0).unwrap()
            .then(&Affine::rotate(Angle::from_degrees(angle).unwrap()))
            .then(&Affine::translate(t.to_vector()).unwrap());
        prop_assert_eq!(m.inverse(), Err(GeometryError::Singular));
    }

    #[test]
    fn bounds_follow_translation(start in point(), segs in segments(), closed in any::<bool>(), v in point()) {
        let path = build(start, &segs, closed);
        let v = v.to_vector();
        let before = path.bounds().unwrap();
        let after = path.transform(&Affine::translate(v).unwrap()).bounds().unwrap();
        prop_assert!(close(after.min(), before.min() + v, TRANSLATION_EPS_MM), "{before:?} + {v:?} -> {after:?}");
        prop_assert!(close(after.max(), before.max() + v, TRANSLATION_EPS_MM), "{before:?} + {v:?} -> {after:?}");
    }

    /// Curve bounds lie between the endpoint box and the control-polygon box (convex hull).
    #[test]
    fn bounds_lie_between_endpoints_and_control_hull(start in point(), segs in segments()) {
        let b = build(start, &segs, false).bounds().unwrap();
        let inner = bounds_of(&endpoints(start, &segs));
        let outer = bounds_of(&defining_points(start, &segs));
        let eps = 1e-9;
        prop_assert!(b.min().x <= inner.min().x + eps && b.min().y <= inner.min().y + eps, "{b:?} vs {inner:?}");
        prop_assert!(b.max().x >= inner.max().x - eps && b.max().y >= inner.max().y - eps, "{b:?} vs {inner:?}");
        prop_assert!(b.min().x >= outer.min().x - eps && b.min().y >= outer.min().y - eps, "{b:?} vs {outer:?}");
        prop_assert!(b.max().x <= outer.max().x + eps && b.max().y <= outer.max().y + eps, "{b:?} vs {outer:?}");
    }

    /// Green's theorem: an affine map scales signed area by its determinant (also for cubics).
    #[test]
    fn signed_area_scales_with_determinant(start in point(), segs in segments(), m in affine()) {
        let path = build(start, &segs, true);
        let area = path.signed_area();
        let mapped = path.transform(&m).signed_area();
        let expected = m.determinant() * area;
        let tol = 1e-6 + 1e-9 * expected.abs().max(1.0) * 1e3;
        prop_assert!((mapped - expected).abs() <= tol, "area {area}, det {}, got {mapped}", m.determinant());
    }
}
