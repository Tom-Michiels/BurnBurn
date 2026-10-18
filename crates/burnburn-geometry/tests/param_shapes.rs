//! W1-L2b: parametric basic shapes. Oracles are closed-form: vertices on circles, equal sides,
//! exact areas and bounds, and "transform then resolve = resolve then transform".

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use burnburn_geometry::{
    Affine, Bounds, GeometryError, MAX_SIDES, ParametricShape, Path, Point, Segment, ShapeParamError, ShapeParams,
    Vector,
};
use burnburn_types::Angle;
use proptest::prelude::*;

const EPS: f64 = 1e-9;

fn deg(d: f64) -> Angle {
    Angle::from_degrees(d).unwrap()
}

/// Unit circle → circle of radius `r` around `c`.
fn circle_frame(c: Point, r: f64) -> Affine {
    Affine::scale(r, r).unwrap().then(&Affine::translate(c.to_vector()).unwrap())
}

/// The corner points of the first subpath (start plus every segment end), without the closing duplicate.
fn vertices(path: &Path, subpath: usize) -> Vec<Point> {
    let sp = &path.subpaths()[subpath];
    let mut v = vec![sp.start()];
    v.extend(sp.segments().iter().map(Segment::end));
    if v.len() > 1 && v[0].distance(*v.last().unwrap()) < EPS {
        v.pop();
    }
    v
}

fn polygon(sides: u32, c: Point, r: f64) -> ParametricShape {
    ParametricShape::new(ShapeParams::Polygon { sides }, circle_frame(c, r)).unwrap()
}

#[test]
fn triangle_is_equilateral_with_closed_form_area_and_apex_up() {
    let c = Point::new(30.0, 40.0);
    let path = polygon(3, c, 10.0).to_path().unwrap();
    let v = vertices(&path, 0);
    assert_eq!(v.len(), 3);
    // First vertex at the top (+y), the others 120° apart on the circle.
    assert!(v[0].distance(Point::new(30.0, 50.0)) < EPS, "{:?}", v[0]);
    for p in &v {
        assert!((p.distance(c) - 10.0).abs() < EPS);
    }
    let side = 10.0 * 3f64.sqrt();
    for i in 0..3 {
        assert!((v[i].distance(v[(i + 1) % 3]) - side).abs() < EPS);
    }
    assert!((path.signed_area() - 3.0 * 3f64.sqrt() / 4.0 * 100.0).abs() < 1e-9, "{}", path.signed_area());
}

#[test]
fn regular_polygons_have_vertices_on_the_circle_equal_sides_and_exact_area() {
    for n in 3..=24u32 {
        let c = Point::new(-5.0, 7.0);
        let r = 12.5;
        let path = polygon(n, c, r).to_path().unwrap();
        assert_eq!(path.subpaths().len(), 1);
        let v = vertices(&path, 0);
        assert_eq!(v.len(), n as usize, "n = {n}");
        let side = 2.0 * r * (PI / f64::from(n)).sin();
        for i in 0..v.len() {
            assert!((v[i].distance(c) - r).abs() < EPS, "n = {n} vertex {i}");
            assert!((v[i].distance(v[(i + 1) % v.len()]) - side).abs() < 1e-8, "n = {n} side {i}");
            // Vertex k sits at 90° + k·360°/n.
            let a = FRAC_PI_2 + TAU * i as f64 / f64::from(n);
            assert!(v[i].distance(Point::new(c.x + r * a.cos(), c.y + r * a.sin())) < 1e-8, "n = {n} angle {i}");
        }
        let area = 0.5 * f64::from(n) * r * r * (TAU / f64::from(n)).sin();
        assert!((path.signed_area() - area).abs() < 1e-8, "n = {n}");
    }
}

#[test]
fn pentagon_and_octagon_presets_are_polygons_with_five_and_eight_sides() {
    assert_eq!(vertices(&polygon(5, Point::ORIGIN, 1.0).to_path().unwrap(), 0).len(), 5);
    assert_eq!(vertices(&polygon(8, Point::ORIGIN, 1.0).to_path().unwrap(), 0).len(), 8);
}

#[test]
fn star_alternates_tips_and_valleys_with_exact_area() {
    for n in 3..=12u32 {
        let (c, r, ratio) = (Point::new(1.0, 2.0), 20.0, 0.4);
        let shape = ParametricShape::new(ShapeParams::Star { points: n, ratio }, circle_frame(c, r)).unwrap();
        let path = shape.to_path().unwrap();
        let v = vertices(&path, 0);
        assert_eq!(v.len(), 2 * n as usize);
        for (i, p) in v.iter().enumerate() {
            let expected = if i % 2 == 0 { r } else { r * ratio };
            assert!((p.distance(c) - expected).abs() < 1e-8, "n = {n} vertex {i}");
        }
        // 2n triangles of area ½·R·r·sin(π/n).
        let area = f64::from(n) * r * (r * ratio) * (PI / f64::from(n)).sin();
        assert!((path.signed_area() - area).abs() < 1e-7, "n = {n}");
        // First tip points up.
        assert!(v[0].distance(Point::new(c.x, c.y + r)) < EPS);
    }
}

#[test]
fn double_star_is_two_stars_half_a_step_apart() {
    let (c, r, ratio, n) = (Point::new(0.0, 0.0), 10.0, 0.5, 5u32);
    let single = ParametricShape::new(ShapeParams::Star { points: n, ratio }, circle_frame(c, r)).unwrap();
    let double = ParametricShape::new(ShapeParams::DoubleStar { points: n, ratio }, circle_frame(c, r)).unwrap();
    let (s, d) = (single.to_path().unwrap(), double.to_path().unwrap());
    assert_eq!(d.subpaths().len(), 2);
    // First subpath is the single star; the second is it rotated by π/n.
    assert_eq!(vertices(&d, 0), vertices(&s, 0));
    let rot = s.transform(&Affine::rotate(Angle::from_radians(PI / f64::from(n)).unwrap()));
    for (a, b) in vertices(&d, 1).iter().zip(vertices(&rot, 0)) {
        assert!(a.distance(b) < 1e-8, "{a:?} vs {b:?}");
    }
    for i in 0..2 {
        assert!((d.subpaths()[i].signed_area().unwrap() - s.signed_area()).abs() < 1e-8);
    }
}

#[test]
fn double_star_of_three_points_has_its_six_tips_on_a_regular_hexagon() {
    // Two 3-point stars half a step (60°) apart: their six tips form a regular hexagon, at 30° + k·60°.
    let r = 10.0;
    let d = ParametricShape::new(ShapeParams::DoubleStar { points: 3, ratio: 0.5 }, circle_frame(Point::ORIGIN, r))
        .unwrap()
        .to_path()
        .unwrap();
    // Tip vertices of both stars: 6 points at 60° steps on the circle (hexagon).
    let mut tips: Vec<Point> = Vec::new();
    for i in 0..2 {
        tips.extend(vertices(&d, i).into_iter().step_by(2));
    }
    assert_eq!(tips.len(), 6);
    for p in &tips {
        assert!((p.distance(Point::ORIGIN) - r).abs() < EPS);
    }
    let mut angles: Vec<f64> = tips.iter().map(|p| p.y.atan2(p.x).to_degrees().rem_euclid(360.0)).collect();
    angles.sort_by(f64::total_cmp);
    for (i, a) in angles.iter().enumerate() {
        assert!((a - (30.0 + 60.0 * i as f64)).abs() < 1e-7, "{angles:?}");
    }
}

#[test]
fn rounded_rectangle_has_closed_form_area_bounds_and_tangent_arcs() {
    let params = ShapeParams::RoundedRectangle { width: 40.0, height: 20.0, radius: 5.0 };
    let frame = Affine::translate(Vector::new(100.0, 50.0)).unwrap();
    let path = ParametricShape::new(params, frame).unwrap().to_path().unwrap();
    // Centred on the frame origin.
    let b = path.bounds().unwrap();
    assert!((b.min().x - 80.0).abs() < EPS && (b.max().x - 120.0).abs() < EPS);
    assert!((b.min().y - 40.0).abs() < EPS && (b.max().y - 60.0).abs() < EPS);
    assert!((path.signed_area() - (800.0 - (4.0 - PI) * 25.0)).abs() < 1e-9, "{}", path.signed_area());
    let arcs = path.subpaths()[0].segments().iter().filter(|s| matches!(s, Segment::Arc(_))).count();
    assert_eq!(arcs, 4);
    assert_eq!(path.subpaths()[0].segments().len(), 8);
    assert!(path.subpaths()[0].is_closed());
}

#[test]
fn rounded_rectangle_without_radius_is_a_plain_rectangle_and_full_radius_a_stadium() {
    let sharp =
        ParametricShape::new(ShapeParams::RoundedRectangle { width: 10.0, height: 6.0, radius: 0.0 }, Affine::IDENTITY)
            .unwrap()
            .to_path()
            .unwrap();
    assert!((sharp.signed_area() - 60.0).abs() < EPS);
    assert!(sharp.subpaths()[0].segments().iter().all(|s| matches!(s, Segment::Line { .. })));
    // Radius = half the short side: stadium, area = w·h − (4 − π)·r².
    let stadium =
        ParametricShape::new(ShapeParams::RoundedRectangle { width: 10.0, height: 6.0, radius: 3.0 }, Affine::IDENTITY)
            .unwrap()
            .to_path()
            .unwrap();
    assert!((stadium.signed_area() - (60.0 - (4.0 - PI) * 9.0)).abs() < 1e-9);
}

#[test]
fn invalid_parameters_are_typed_errors() {
    let id = Affine::IDENTITY;
    let err = |p| ParametricShape::new(p, id).unwrap_err();
    assert_eq!(err(ShapeParams::Polygon { sides: 2 }), ShapeParamError::TooFewSides);
    assert_eq!(err(ShapeParams::Polygon { sides: MAX_SIDES + 1 }), ShapeParamError::TooMany);
    assert_eq!(err(ShapeParams::Star { points: 1, ratio: 0.5 }), ShapeParamError::TooFewPoints);
    assert_eq!(err(ShapeParams::DoubleStar { points: MAX_SIDES + 1, ratio: 0.5 }), ShapeParamError::TooMany);
    for ratio in [0.0, 1.0, -0.1, 1.5, f64::NAN, f64::INFINITY] {
        assert_eq!(err(ShapeParams::Star { points: 5, ratio }), ShapeParamError::RatioOutOfRange, "{ratio}");
    }
    let rr = |w, h, r| ShapeParams::RoundedRectangle { width: w, height: h, radius: r };
    assert_eq!(err(rr(0.0, 5.0, 0.0)), ShapeParamError::NonPositiveSize);
    assert_eq!(err(rr(5.0, f64::NAN, 0.0)), ShapeParamError::NonPositiveSize);
    assert_eq!(err(rr(10.0, 6.0, 3.1)), ShapeParamError::RadiusOutOfRange);
    assert_eq!(err(rr(10.0, 6.0, -1.0)), ShapeParamError::RadiusOutOfRange);
    // A collapsed placement has no area.
    let flat = Affine::scale(1.0, 0.0).unwrap();
    assert_eq!(
        ParametricShape::new(ShapeParams::Polygon { sides: 5 }, flat).unwrap_err(),
        ShapeParamError::Geometry(GeometryError::Singular)
    );
}

#[test]
fn fit_makes_the_resolved_bounds_equal_the_target() {
    let target = Bounds::from_corners(Point::new(10.0, -4.0), Point::new(70.0, 36.0));
    let cases = [
        ShapeParams::Polygon { sides: 3 },
        ShapeParams::Polygon { sides: 5 },
        ShapeParams::Polygon { sides: 8 },
        ShapeParams::Star { points: 5, ratio: 0.4 },
        ShapeParams::DoubleStar { points: 6, ratio: 0.6 },
        ShapeParams::RoundedRectangle { width: 1.0, height: 1.0, radius: 0.0 },
    ];
    for params in cases {
        let b = ParametricShape::fit(params, target).unwrap().to_path().unwrap().bounds().unwrap();
        assert!(b.min().distance(target.min()) < 1e-8 && b.max().distance(target.max()) < 1e-8, "{params:?}: {b:?}");
    }
}

#[test]
fn fit_of_a_rounded_rectangle_takes_the_box_and_clamps_the_radius() {
    let b = Bounds::from_corners(Point::new(0.0, 0.0), Point::new(40.0, 10.0));
    let fit = |r| ParametricShape::fit(ShapeParams::RoundedRectangle { width: 1.0, height: 1.0, radius: r }, b);
    assert_eq!(*fit(3.0).unwrap().params(), ShapeParams::RoundedRectangle { width: 40.0, height: 10.0, radius: 3.0 });
    assert_eq!(*fit(99.0).unwrap().params(), ShapeParams::RoundedRectangle { width: 40.0, height: 10.0, radius: 5.0 });
    assert_eq!(fit(-1.0).unwrap_err(), ShapeParamError::RadiusOutOfRange);
    let flat = Bounds::from_corners(Point::new(0.0, 0.0), Point::new(40.0, 0.0));
    assert_eq!(
        ParametricShape::fit(ShapeParams::Polygon { sides: 5 }, flat).unwrap_err(),
        ShapeParamError::NonPositiveSize
    );
}

#[test]
fn natural_size_matches_the_resolved_bounds_of_the_unplaced_shape() {
    let cases = [
        ShapeParams::Polygon { sides: 3 },
        ShapeParams::Polygon { sides: 6 },
        ShapeParams::Star { points: 5, ratio: 0.4 },
        ShapeParams::DoubleStar { points: 7, ratio: 0.5 },
        ShapeParams::RoundedRectangle { width: 8.0, height: 3.0, radius: 1.0 },
    ];
    for params in cases {
        let (w, h) = params.natural_size();
        let b = ParametricShape::new(params, Affine::IDENTITY).unwrap().to_path().unwrap().bounds().unwrap();
        assert!((b.width() - w).abs() < 1e-9 && (b.height() - h).abs() < 1e-9, "{params:?}");
    }
    // Analytic: equilateral triangle on the unit circle is √3 wide and 1.5 high.
    let (w, h) = ShapeParams::Polygon { sides: 3 }.natural_size();
    assert!((w - 3f64.sqrt()).abs() < EPS && (h - 1.5).abs() < EPS);
}

#[test]
fn changing_the_parameters_keeps_the_placement() {
    let a = polygon(5, Point::new(3.0, 4.0), 9.0);
    let b = a.with_params(ShapeParams::Polygon { sides: 8 }).unwrap();
    assert_eq!(a.frame(), b.frame());
    let v = vertices(&b.to_path().unwrap(), 0);
    assert_eq!(v.len(), 8);
    assert!(v.iter().all(|p| (p.distance(Point::new(3.0, 4.0)) - 9.0).abs() < EPS));
}

#[test]
fn rounded_rectangle_follows_similarity_and_axis_scales_and_refuses_shear() {
    let rr = ParametricShape::new(
        ShapeParams::RoundedRectangle { width: 20.0, height: 10.0, radius: 2.0 },
        Affine::translate(Vector::new(5.0, 5.0)).unwrap(),
    )
    .unwrap();
    // Uniform scale ×2 about the origin: size and radius double.
    let m = Affine::scale(2.0, 2.0).unwrap();
    let t = rr.transformed(&m).unwrap();
    assert_eq!(*t.params(), ShapeParams::RoundedRectangle { width: 40.0, height: 20.0, radius: 4.0 });
    assert_paths_close(&t.to_path().unwrap(), &rr.to_path().unwrap().transform(&m));
    // Rotation by 30° keeps the parameters and rotates the outline.
    let r = Affine::rotate(deg(30.0));
    let t = rr.transformed(&r).unwrap();
    assert_eq!(*t.params(), *rr.params());
    assert_paths_close(&t.to_path().unwrap(), &rr.to_path().unwrap().transform(&r));
    // Axis-aligned stretch: width ×3, height ×0.5; the radius follows the smaller factor and is clamped.
    let s = Affine::scale(3.0, 0.5).unwrap();
    let t = rr.transformed(&s).unwrap();
    assert_eq!(*t.params(), ShapeParams::RoundedRectangle { width: 60.0, height: 5.0, radius: 1.0 });
    let b = t.to_path().unwrap().bounds().unwrap();
    assert!((b.width() - 60.0).abs() < EPS && (b.height() - 5.0).abs() < EPS);
    // Mirror keeps the shape parametric.
    assert!(rr.transformed(&Affine::mirror_x()).is_some());
    // Shear is not a rectangle any more: no parametric form.
    let shear = Affine::new([1.0, 0.0, 0.5, 1.0, 0.0, 0.0]).unwrap();
    assert!(rr.transformed(&shear).is_none());
}

fn assert_paths_close(a: &Path, b: &Path) {
    let (ba, bb) = (a.bounds().unwrap(), b.bounds().unwrap());
    assert!(ba.min().distance(bb.min()) < 1e-8 && ba.max().distance(bb.max()) < 1e-8, "{ba:?} vs {bb:?}");
    assert!((a.signed_area() - b.signed_area()).abs() < 1e-7, "{} vs {}", a.signed_area(), b.signed_area());
    assert_eq!(a.subpaths().len(), b.subpaths().len());
    for (x, y) in a.subpaths().iter().zip(b.subpaths()) {
        assert!(x.start().distance(y.start()) < 1e-8);
        assert_eq!(x.segments().len(), y.segments().len());
    }
}

fn affine_strategy() -> impl Strategy<Value = Affine> {
    (0.2f64..5.0, 0.2f64..5.0, -180.0f64..180.0, -50.0f64..50.0, -50.0f64..50.0, any::<bool>()).prop_map(
        |(sx, sy, rot, tx, ty, mirror)| {
            let mut m = Affine::scale(sx, sy).unwrap().then(&Affine::rotate(deg(rot)));
            if mirror {
                m = m.then(&Affine::mirror_x());
            }
            m.then(&Affine::translate(Vector::new(tx, ty)).unwrap())
        },
    )
}

proptest! {
    /// Transforming the definition and resolving equals resolving and transforming the outline.
    #[test]
    fn transformed_definition_resolves_to_the_transformed_outline(
        m in affine_strategy(), sides in 3u32..20, ratio in 0.05f64..0.95, which in 0u8..3,
    ) {
        let params = match which {
            0 => ShapeParams::Polygon { sides },
            1 => ShapeParams::Star { points: sides, ratio },
            _ => ShapeParams::DoubleStar { points: sides, ratio },
        };
        let shape = ParametricShape::new(params, circle_frame(Point::new(2.0, -3.0), 7.0)).unwrap();
        let moved = shape.transformed(&m).expect("polygon family is closed under affine maps");
        prop_assert_eq!(*moved.params(), params);
        let (a, b) = (moved.to_path().unwrap(), shape.to_path().unwrap().transform(&m));
        let (ba, bb) = (a.bounds().unwrap(), b.bounds().unwrap());
        prop_assert!(ba.min().distance(bb.min()) < 1e-7 && ba.max().distance(bb.max()) < 1e-7);
        prop_assert!((a.signed_area() - b.signed_area()).abs() < 1e-6 * (1.0 + b.signed_area().abs()));
    }

    /// `fit` reproduces any target box exactly.
    #[test]
    fn fit_reproduces_any_target_box(
        x in -100.0f64..100.0, y in -100.0f64..100.0, w in 0.5f64..200.0, h in 0.5f64..200.0,
        sides in 3u32..20, ratio in 0.05f64..0.95, which in 0u8..3,
    ) {
        let params = match which {
            0 => ShapeParams::Polygon { sides },
            1 => ShapeParams::Star { points: sides, ratio },
            _ => ShapeParams::DoubleStar { points: sides, ratio },
        };
        let target = Bounds::from_corners(Point::new(x, y), Point::new(x + w, y + h));
        let b = ParametricShape::fit(params, target).unwrap().to_path().unwrap().bounds().unwrap();
        prop_assert!(b.min().distance(target.min()) < 1e-7 && b.max().distance(target.max()) < 1e-7);
    }
}
