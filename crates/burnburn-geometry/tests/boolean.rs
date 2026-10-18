//! Public contract: booleans and offsets behind the `i_overlay` adapter (E05, ADR 0007).
//! Oracles: exactly known areas of rectangles, closed-form circle areas with a chord-error
//! bound, and area identities (property tests).

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use std::f64::consts::PI;

use burnburn_geometry::{
    Affine, BooleanError, BooleanOp, BooleanOptions, FillRule, OffsetError, OffsetJoin, Path, PathBuilder, Point,
    ProductionTolerance, Rect, Vector, Winding, boolean, offset,
};
use burnburn_types::{Angle, CancellationSource, CancellationToken, Length};
use proptest::prelude::*;

fn rect(x: f64, y: f64, w: f64, h: f64) -> Path {
    Path::rectangle(Rect::new(Point::new(x, y), Length::from_mm(w).unwrap(), Length::from_mm(h).unwrap()).unwrap())
}

fn opts() -> BooleanOptions {
    BooleanOptions {
        fill: FillRule::EvenOdd,
        tolerance: ProductionTolerance::new(Length::from_mm(0.001).unwrap()).unwrap(),
    }
}

fn run(a: &Path, b: &Path, op: BooleanOp) -> Path {
    boolean(a, b, op, &opts(), &CancellationToken::never()).unwrap()
}

#[test]
fn two_overlapping_rectangles_have_exact_boolean_areas() {
    let (a, b) = (rect(0.0, 0.0, 10.0, 10.0), rect(5.0, 5.0, 10.0, 10.0));
    for (op, area) in [
        (BooleanOp::Union, 175.0),
        (BooleanOp::Intersection, 25.0),
        (BooleanOp::Difference, 75.0),
        (BooleanOp::Xor, 150.0),
    ] {
        let r = run(&a, &b, op);
        assert!((r.signed_area() - area).abs() < 1e-6, "{op:?}: {}", r.signed_area());
        assert!(r.subpaths().iter().all(|s| s.is_closed()));
    }
    // Union is one outer contour (counter-clockwise) with 8 corners.
    let u = run(&a, &b, BooleanOp::Union);
    assert_eq!(u.subpaths().len(), 1);
    assert_eq!(u.subpaths()[0].winding(), Some(Winding::CounterClockwise));
    assert_eq!(u.subpaths()[0].segments().len() + 1, 8);
}

#[test]
fn plate_with_holes_keeps_holes_clockwise_and_area_within_chord_error() {
    let plate = rect(0.0, 0.0, 100.0, 50.0);
    let holes = PathBuilder::new().build().unwrap();
    let mut holes_path = holes;
    for cx in [25.0, 75.0] {
        let c = Path::circle(Point::new(cx, 25.0), 5.0).unwrap();
        holes_path = run(&holes_path, &c, BooleanOp::Union);
    }
    let r = run(&plate, &holes_path, BooleanOp::Difference);
    assert_eq!(r.subpaths().len(), 3);
    let outer: Vec<_> = r.subpaths().iter().filter(|s| s.winding() == Some(Winding::CounterClockwise)).collect();
    let inner: Vec<_> = r.subpaths().iter().filter(|s| s.winding() == Some(Winding::Clockwise)).collect();
    assert_eq!((outer.len(), inner.len()), (1, 2));
    // Chords lie inside the circle: hole area is smaller by at most perimeter × tolerance.
    let exact = 5000.0 - 2.0 * PI * 25.0;
    let bound = 2.0 * (2.0 * PI * 5.0) * 0.001 + 1e-6;
    assert!((r.signed_area() - exact).abs() <= bound, "{} vs {exact}", r.signed_area());
}

#[test]
fn touching_edges_and_corners_merge_without_slivers() {
    // Shared edge: one 20×10 contour.
    let u = run(&rect(0.0, 0.0, 10.0, 10.0), &rect(10.0, 0.0, 10.0, 10.0), BooleanOp::Union);
    assert_eq!(u.subpaths().len(), 1);
    assert!((u.signed_area() - 200.0).abs() < 1e-6);
    // Shared corner only: area adds up and no degenerate contour appears.
    let c = run(&rect(0.0, 0.0, 10.0, 10.0), &rect(10.0, 10.0, 10.0, 10.0), BooleanOp::Union);
    assert!((c.signed_area() - 200.0).abs() < 1e-6);
    assert!(c.subpaths().iter().all(|s| s.signed_area().unwrap().abs() > 1.0), "{c:?}");
    // Touching intersection has no area.
    let i = run(&rect(0.0, 0.0, 10.0, 10.0), &rect(10.0, 0.0, 10.0, 10.0), BooleanOp::Intersection);
    assert!(i.signed_area().abs() < 1e-9 && i.subpaths().iter().all(|s| s.signed_area().unwrap().abs() > 1e-9));
}

#[test]
fn short_segments_and_nearly_coincident_points_are_stable() {
    // A 0.01 mm notch into the square (above the 0.001 mm tolerance) survives; area exact.
    let notched = Path::polyline(
        &[
            Point::new(0.0, 0.0),
            Point::new(5.0, 0.0),
            Point::new(5.0, 0.01),
            Point::new(5.01, 0.01),
            Point::new(5.01, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ],
        true,
    )
    .unwrap();
    let r = run(&notched, &rect(20.0, 20.0, 1.0, 1.0), BooleanOp::Union);
    assert!((r.signed_area() - (100.0 - 0.0001 + 1.0)).abs() < 1e-6, "{}", r.signed_area());
    // Edges 1e-9 mm apart (far below tolerance) are one edge: one contour, area 200.
    let gap = run(&rect(0.0, 0.0, 10.0, 10.0), &rect(10.0 + 1e-9, 0.0, 10.0, 10.0), BooleanOp::Union);
    assert_eq!(gap.subpaths().len(), 1, "{gap:?}");
    assert!((gap.signed_area() - 200.0).abs() < 1e-5);
}

#[test]
fn large_coordinates_keep_the_same_result() {
    let far = Vector::new(1.0e6, -2.0e6);
    let m = Affine::translate(far).unwrap();
    let (a, b) = (rect(0.0, 0.0, 10.0, 10.0).transform(&m), rect(5.0, 5.0, 10.0, 10.0).transform(&m));
    let r = run(&a, &b, BooleanOp::Union);
    assert!((r.signed_area() - 175.0).abs() < 1e-3, "{}", r.signed_area());
    let bb = r.bounds().unwrap();
    assert!((bb.min().x - 1.0e6).abs() < 1e-6 && (bb.max().y - (-2.0e6 + 15.0)).abs() < 1e-6, "{bb:?}");
}

#[test]
fn open_input_and_cancellation_are_typed_errors() {
    let open = Path::line(Point::new(0.0, 0.0), Point::new(10.0, 0.0)).unwrap();
    let r = boolean(&open, &rect(0.0, 0.0, 1.0, 1.0), BooleanOp::Union, &opts(), &CancellationToken::never());
    assert_eq!(r, Err(BooleanError::OpenContour { operand: 0, subpath: 0 }));
    let source = CancellationSource::new();
    source.cancel();
    let r = boolean(&rect(0.0, 0.0, 1.0, 1.0), &rect(0.0, 0.0, 1.0, 1.0), BooleanOp::Union, &opts(), &source.token());
    assert_eq!(r, Err(BooleanError::Cancelled));
}

#[test]
fn boolean_results_are_deterministic() {
    let (a, b) = (Path::circle(Point::new(0.0, 0.0), 7.0).unwrap(), rect(2.0, -3.0, 9.0, 4.0));
    assert_eq!(run(&a, &b, BooleanOp::Xor), run(&a, &b, BooleanOp::Xor));
}

#[test]
fn square_offsets_have_closed_form_areas() {
    let sq = rect(0.0, 0.0, 10.0, 10.0);
    let t = &opts().tolerance;
    let never = CancellationToken::never();
    let miter = offset(&sq, 1.0, OffsetJoin::Miter, t, &never).unwrap();
    assert!((miter.signed_area() - 144.0).abs() < 1e-6, "{}", miter.signed_area());
    // Round corners: 100 + 4·10·1 + π·1², chords inside the arcs.
    let round = offset(&sq, 1.0, OffsetJoin::Round, t, &never).unwrap();
    let exact = 140.0 + PI;
    assert!(round.signed_area() <= exact + 1e-6 && round.signed_area() > exact - 2.0 * PI * 0.001 - 1e-6);
    let inner = offset(&sq, -1.0, OffsetJoin::Miter, t, &never).unwrap();
    assert!((inner.signed_area() - 64.0).abs() < 1e-6);
    assert_eq!(offset(&sq, -6.0, OffsetJoin::Miter, t, &never), Err(OffsetError::Collapsed));
    assert_eq!(offset(&sq, f64::NAN, OffsetJoin::Miter, t, &never), Err(OffsetError::NotFinite));
}

#[test]
fn outward_offset_of_a_ring_shrinks_its_hole() {
    let ring = run(&rect(0.0, 0.0, 20.0, 20.0), &rect(5.0, 5.0, 10.0, 10.0), BooleanOp::Difference);
    let r = offset(&ring, 1.0, OffsetJoin::Miter, &opts().tolerance, &CancellationToken::never()).unwrap();
    // Outer 22×22, hole 8×8.
    assert!((r.signed_area() - (484.0 - 64.0)).abs() < 1e-6, "{}", r.signed_area());
    assert_eq!(r.subpaths().len(), 2);
}

fn arb_rect() -> impl Strategy<Value = Path> {
    (-50.0f64..50.0, -50.0f64..50.0, 0.5f64..40.0, 0.5f64..40.0).prop_map(|(x, y, w, h)| rect(x, y, w, h))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// |A ∪ B| + |A ∩ B| = |A| + |B|, |A ⊕ B| = |A ∪ B| − |A ∩ B|, |A − B| = |A| − |A ∩ B|.
    #[test]
    fn boolean_area_identities_hold(a in arb_rect(), b in arb_rect()) {
        let u = run(&a, &b, BooleanOp::Union).signed_area();
        let i = run(&a, &b, BooleanOp::Intersection).signed_area();
        let x = run(&a, &b, BooleanOp::Xor).signed_area();
        let d = run(&a, &b, BooleanOp::Difference).signed_area();
        let (aa, ab) = (a.signed_area(), b.signed_area());
        let eps = 1e-4;
        prop_assert!((u + i - aa - ab).abs() < eps, "u {u} i {i}");
        prop_assert!((x - (u - i)).abs() < eps);
        prop_assert!((d - (aa - i)).abs() < eps);
    }

    /// Rigid motions commute with booleans: areas are invariant.
    #[test]
    fn booleans_are_invariant_under_rigid_motion(a in arb_rect(), b in arb_rect(), deg in -180.0f64..180.0, tx in -500.0f64..500.0) {
        let m = Affine::rotate(Angle::from_degrees(deg).unwrap()).then(&Affine::translate(Vector::new(tx, -tx)).unwrap());
        let plain = run(&a, &b, BooleanOp::Union).signed_area();
        let moved = run(&a.transform(&m), &b.transform(&m), BooleanOp::Union).signed_area();
        prop_assert!((plain - moved).abs() < 1e-3, "{plain} vs {moved}");
    }
}

#[test]
fn thin_shape_in_large_bounds_collapses_with_a_typed_error() {
    // An L with 2 mm arms in 20×20 bounds: the bounds check cannot see the collapse at −1.5 mm.
    let l = Path::polyline(
        &[
            Point::new(0.0, 0.0),
            Point::new(20.0, 0.0),
            Point::new(20.0, 2.0),
            Point::new(2.0, 2.0),
            Point::new(2.0, 20.0),
            Point::new(0.0, 20.0),
        ],
        true,
    )
    .unwrap();
    let r = offset(&l, -1.5, OffsetJoin::Miter, &opts().tolerance, &CancellationToken::never());
    assert_eq!(r, Err(OffsetError::Collapsed));
}
