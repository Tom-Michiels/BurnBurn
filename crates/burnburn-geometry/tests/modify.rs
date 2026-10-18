//! Public contract: corner fillet/chamfer, curve trim/extend/break, splines and conics
//! (SolidCraft sketch tools). Oracles are closed-form areas, tangent points and a Cox–de Boor
//! evaluator written independently in this file.

#![allow(clippy::unwrap_used, clippy::unreachable)] // test helpers and let-else in tests

use std::f64::consts::{FRAC_PI_2, PI};

use burnburn_geometry::{
    BreakError, CornerCut, CornerError, EllipticalArc, EndpointRef, ExtendError, GeometryError, Path, Point, Rect,
    Segment, TrimError,
};
use burnburn_types::{Angle, Length};
use proptest::prelude::*;

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Path {
    Path::rectangle(Rect::new(p(x, y), Length::from_mm(w).unwrap(), Length::from_mm(h).unwrap()).unwrap())
}

fn near(a: Point, b: Point) -> bool {
    a.distance(b) < 1e-9
}

fn rad(r: f64) -> Angle {
    Angle::from_radians(r).unwrap()
}

// ---------------------------------------------------------------------------------------------
// Fillet and chamfer

#[test]
fn fillet_of_a_right_angle_touches_both_sides_five_mm_from_the_corner() {
    let corner = Path::polyline(&[p(10.0, 0.0), p(0.0, 0.0), p(0.0, 10.0)], false).unwrap();
    let out = corner.cut_corner(p(0.5, 0.5), 2.0, CornerCut::Fillet { radius: 5.0 }).unwrap();
    let sp = &out.subpaths()[0];
    assert_eq!(sp.start(), p(10.0, 0.0));
    let segs = sp.segments();
    assert_eq!(segs.len(), 3, "side, arc, side: {segs:?}");
    assert!(matches!(segs[0], Segment::Line { to } if near(to, p(5.0, 0.0))), "{:?}", segs[0]);
    let Segment::Arc(arc) = segs[1] else { unreachable!("middle segment must be the fillet arc") };
    assert!(near(arc.center(), p(5.0, 5.0)), "{:?}", arc.center());
    assert!(near(arc.start_point(), p(5.0, 0.0)) && near(arc.end_point(), p(0.0, 5.0)));
    let (u, _) = arc.semi_diameters();
    assert!((u.length() - 5.0).abs() < 1e-9);
    // A right turn rounds clockwise through a quarter turn.
    assert!((arc.sweep_parameter() + FRAC_PI_2).abs() < 1e-9, "{}", arc.sweep_parameter());
    assert!(matches!(segs[2], Segment::Line { to } if near(to, p(0.0, 10.0))));
}

#[test]
fn fillet_of_a_left_turn_sweeps_counter_clockwise() {
    let corner = Path::polyline(&[p(0.0, 10.0), p(0.0, 0.0), p(10.0, 0.0)], false).unwrap();
    let out = corner.cut_corner(p(0.2, 0.2), 2.0, CornerCut::Fillet { radius: 3.0 }).unwrap();
    let Segment::Arc(arc) = out.subpaths()[0].segments()[1] else { unreachable!("fillet arc") };
    assert!(near(arc.center(), p(3.0, 3.0)));
    assert!((arc.sweep_parameter() - FRAC_PI_2).abs() < 1e-9);
}

#[test]
fn fillet_of_an_acute_corner_has_the_tangent_length_r_over_tan_half_the_angle() {
    // 60° corner at the origin between +x and the 60° ray; r = 2 → tangent length 2/tan 30°.
    let b = p(10.0 * (PI / 3.0).cos(), 10.0 * (PI / 3.0).sin());
    let corner = Path::polyline(&[p(10.0, 0.0), p(0.0, 0.0), b], false).unwrap();
    let out = corner.cut_corner(p(0.3, 0.1), 2.0, CornerCut::Fillet { radius: 2.0 }).unwrap();
    let Segment::Line { to } = out.subpaths()[0].segments()[0] else { unreachable!("first side") };
    let d = 2.0 / (PI / 6.0).tan();
    assert!(near(to, p(d, 0.0)), "{to:?} vs {d}");
}

#[test]
fn rounding_each_corner_of_a_rectangle_removes_the_analytic_area() {
    let (w, h, r) = (30.0, 20.0, 4.0);
    // Corners in ring order, including the start corner and the closing corner.
    for corner in [p(0.0, 0.0), p(w, 0.0), p(w, h), p(0.0, h)] {
        let pick =
            p(corner.x + if corner.x == 0.0 { 0.3 } else { -0.3 }, corner.y + if corner.y == 0.0 { 0.3 } else { -0.3 });
        let out = rect(0.0, 0.0, w, h).cut_corner(pick, 2.0, CornerCut::Fillet { radius: r }).unwrap();
        let expected = w * h - (4.0 - PI) * r * r / 4.0;
        assert!((out.signed_area() - expected).abs() < 1e-9, "{corner:?}: {} vs {expected}", out.signed_area());
        assert_eq!(out.subpaths().len(), 1);
        assert!(out.subpaths()[0].is_closed());
    }
}

#[test]
fn chamfer_variants_remove_the_analytic_triangle() {
    let square = || rect(0.0, 0.0, 10.0, 10.0);
    let pick = p(0.3, 0.3);
    let equal = square().cut_corner(pick, 2.0, CornerCut::ChamferEqual { distance: 3.0 }).unwrap();
    assert!((equal.signed_area() - (100.0 - 4.5)).abs() < 1e-9);
    // Two distances: the first applies to the side nearer to the pick (the bottom edge here).
    let two = square().cut_corner(p(2.0, 0.1), 2.5, CornerCut::ChamferTwo { first: 3.0, second: 4.0 }).unwrap();
    assert!((two.signed_area() - (100.0 - 6.0)).abs() < 1e-9);
    let sp = &two.subpaths()[0];
    let pts: Vec<Point> = std::iter::once(sp.start()).chain(sp.segments().iter().map(Segment::end)).collect();
    assert!(pts.iter().any(|q| near(*q, p(3.0, 0.0))) && pts.iter().any(|q| near(*q, p(0.0, 4.0))), "{pts:?}");
    let swapped = square().cut_corner(p(0.1, 2.0), 2.5, CornerCut::ChamferTwo { first: 3.0, second: 4.0 }).unwrap();
    let sp = &swapped.subpaths()[0];
    let pts: Vec<Point> = std::iter::once(sp.start()).chain(sp.segments().iter().map(Segment::end)).collect();
    assert!(pts.iter().any(|q| near(*q, p(0.0, 3.0))) && pts.iter().any(|q| near(*q, p(4.0, 0.0))), "{pts:?}");
    // Distance 3 at 45° on a right angle: the other side is cut at 3 as well.
    let a45 = square().cut_corner(pick, 2.0, CornerCut::ChamferAngle { distance: 3.0, angle_deg: 45.0 }).unwrap();
    assert!((a45.signed_area() - (100.0 - 4.5)).abs() < 1e-9);
    // 30°: d2 = 3·sin 30° / sin 120°; removed area = d1·d2/2 on a right angle.
    let a30 =
        square().cut_corner(p(2.0, 0.1), 2.5, CornerCut::ChamferAngle { distance: 3.0, angle_deg: 30.0 }).unwrap();
    let d2 = 3.0 * (PI / 6.0).sin() / (2.0 * PI / 3.0).sin();
    assert!((a30.signed_area() - (100.0 - 3.0 * d2 / 2.0)).abs() < 1e-9, "{}", a30.signed_area());
}

#[test]
fn corner_cuts_report_typed_failures_and_never_change_the_input() {
    let square = rect(0.0, 0.0, 10.0, 10.0);
    let before = square.clone();
    assert_eq!(square.cut_corner(p(5.0, 5.0), 1.0, CornerCut::Fillet { radius: 2.0 }), Err(CornerError::NoCorner));
    assert_eq!(square.cut_corner(p(0.0, 0.0), 1.0, CornerCut::Fillet { radius: 20.0 }), Err(CornerError::TooLarge));
    assert_eq!(square.cut_corner(p(0.0, 0.0), 1.0, CornerCut::Fillet { radius: 0.0 }), Err(CornerError::InvalidValue));
    assert_eq!(
        square.cut_corner(p(0.0, 0.0), 1.0, CornerCut::Fillet { radius: f64::NAN }),
        Err(CornerError::InvalidValue)
    );
    // 120° chamfer angle on a right angle never meets the other side.
    assert_eq!(
        square.cut_corner(p(0.0, 0.0), 1.0, CornerCut::ChamferAngle { distance: 2.0, angle_deg: 120.0 }),
        Err(CornerError::InvalidValue)
    );
    let flat = Path::polyline(&[p(0.0, 0.0), p(5.0, 0.0), p(10.0, 0.0)], false).unwrap();
    assert_eq!(flat.cut_corner(p(5.0, 0.0), 1.0, CornerCut::Fillet { radius: 1.0 }), Err(CornerError::Collinear));
    let rounded = Path::circle(p(0.0, 0.0), 5.0).unwrap();
    assert_eq!(rounded.cut_corner(p(5.0, 0.0), 1.0, CornerCut::Fillet { radius: 1.0 }), Err(CornerError::CurvedSide));
    assert_eq!(square, before);
}

proptest! {
    #[test]
    fn fillet_area_and_containment_hold_for_any_valid_radius(
        w in 5.0f64..80.0, h in 5.0f64..80.0, f in 0.01f64..1.0, which in 0usize..4,
    ) {
        let r = f * w.min(h) / 2.0;
        let corners = [p(0.0, 0.0), p(w, 0.0), p(w, h), p(0.0, h)];
        let c = corners[which];
        let pick = p(c.x + if c.x == 0.0 { 0.01 } else { -0.01 }, c.y + if c.y == 0.0 { 0.01 } else { -0.01 });
        let out = rect(0.0, 0.0, w, h).cut_corner(pick, 0.5, CornerCut::Fillet { radius: r }).unwrap();
        let expected = w * h - (4.0 - PI) * r * r / 4.0;
        prop_assert!((out.signed_area() - expected).abs() < 1e-6 * w * h);
        let b = out.bounds().unwrap();
        prop_assert!(b.min().x >= -1e-9 && b.min().y >= -1e-9 && b.max().x <= w + 1e-9 && b.max().y <= h + 1e-9);
    }
}

// ---------------------------------------------------------------------------------------------
// Trim, extend, break

#[test]
fn trimming_a_circle_between_two_crossings_leaves_the_open_far_arc() {
    let circle = Path::circle(p(0.0, 0.0), 10.0).unwrap();
    let cutter = Path::line(p(0.0, -20.0), p(0.0, 20.0)).unwrap();
    let out = circle.trim_at(p(10.0, 0.0), &[&cutter]).unwrap();
    assert_eq!(out.subpaths().len(), 1);
    let sp = &out.subpaths()[0];
    assert!(!sp.is_closed());
    assert!(near(sp.start(), p(0.0, 10.0)) && near(sp.end(), p(0.0, -10.0)), "{:?} {:?}", sp.start(), sp.end());
    let b = out.bounds().unwrap();
    assert!((b.min().x + 10.0).abs() < 1e-9 && b.max().x.abs() < 1e-9 && (b.height() - 20.0).abs() < 1e-9, "{b:?}");
}

#[test]
fn trimming_a_circle_with_a_single_crossing_is_refused() {
    let circle = Path::circle(p(0.0, 0.0), 10.0).unwrap();
    let cutter = Path::line(p(0.0, 0.0), p(0.0, 20.0)).unwrap();
    assert_eq!(circle.trim_at(p(10.0, 0.0), &[&cutter]), Err(TrimError::NoIntersection));
}

#[test]
fn trimming_a_line_agrees_with_the_existing_line_trim() {
    let line = Path::line(p(0.0, 0.0), p(10.0, 0.0)).unwrap();
    let c1 = Path::line(p(3.0, -1.0), p(3.0, 1.0)).unwrap();
    let c2 = Path::line(p(7.0, -1.0), p(7.0, 1.0)).unwrap();
    for pick in [p(5.0, 0.1), p(1.0, 0.0), p(9.0, 0.0)] {
        let old = line.trim_line(pick, &[&c1, &c2]).unwrap();
        let new = line.trim_at(pick, &[&c1, &c2]).unwrap();
        assert_eq!(old.subpaths().len(), new.subpaths().len(), "{pick:?}");
        for (a, b) in old.subpaths().iter().zip(new.subpaths()) {
            assert!(near(a.start(), b.start()) && near(a.end(), b.end()), "{pick:?}: {a:?} vs {b:?}");
        }
    }
}

#[test]
fn trimming_an_open_arc_removes_the_piece_between_cutters() {
    let arc = EllipticalArc::circle(p(0.0, 0.0), 10.0, rad(0.0), rad(PI)).unwrap();
    let path = burnburn_geometry::PathBuilder::new().move_to(arc.start_point()).arc(arc).build().unwrap();
    let c1 = Path::line(p(-5.0, 0.0), p(-5.0, 20.0)).unwrap();
    let c2 = Path::line(p(5.0, 0.0), p(5.0, 20.0)).unwrap();
    let out = path.trim_at(p(0.0, 10.0), &[&c1, &c2]).unwrap();
    assert_eq!(out.subpaths().len(), 2);
    assert!(near(out.subpaths()[0].start(), p(10.0, 0.0)));
    let x = 10.0 * (1.0 - 0.25f64).sqrt();
    assert!(near(out.subpaths()[0].end(), p(5.0, x)), "{:?}", out.subpaths()[0].end());
    assert!(near(out.subpaths()[1].start(), p(-5.0, x)) && near(out.subpaths()[1].end(), p(-10.0, 0.0)));
}

#[test]
fn trimming_a_cubic_splits_it_at_the_crossing_without_changing_the_curve() {
    let curve = Path::spline_fit(&[p(0.0, 0.0), p(5.0, 4.0), p(10.0, 0.0)], false).unwrap();
    let cutter = Path::line(p(5.0, -10.0), p(5.0, 10.0)).unwrap();
    let out = curve.trim_at(p(8.0, 2.0), &[&cutter]).unwrap();
    assert_eq!(out.subpaths().len(), 1);
    assert!(near(out.subpaths()[0].start(), p(0.0, 0.0)));
    assert!(near(out.subpaths()[0].end(), p(5.0, 4.0)), "{:?}", out.subpaths()[0].end());
}

#[test]
fn extending_an_arc_end_runs_along_its_circle_to_the_boundary() {
    let arc = EllipticalArc::circle(p(0.0, 0.0), 10.0, rad(0.0), rad(FRAC_PI_2)).unwrap();
    let path = burnburn_geometry::PathBuilder::new().move_to(arc.start_point()).arc(arc).build().unwrap();
    let wall = Path::line(p(-6.0, -20.0), p(-6.0, 20.0)).unwrap();
    let out = path.extend_end(EndpointRef::End(0), &[&wall]).unwrap();
    let Segment::Arc(longer) = out.subpaths()[0].segments()[0] else { unreachable!("still an arc") };
    assert!(near(longer.end_point(), p(-6.0, 8.0)), "{:?}", longer.end_point());
    assert!(near(longer.start_point(), p(10.0, 0.0)));
    let sweep = 0.6f64.acos().max(PI - 0.6f64.acos());
    assert!((longer.sweep_parameter() - sweep).abs() < 1e-9, "{}", longer.sweep_parameter());

    let floor = Path::line(p(-20.0, -6.0), p(20.0, -6.0)).unwrap();
    let out = path.extend_end(EndpointRef::Start(0), &[&floor]).unwrap();
    let sp = &out.subpaths()[0];
    assert!(near(sp.start(), p(8.0, -6.0)), "{:?}", sp.start());
    assert!(near(sp.end(), p(0.0, 10.0)));

    let far = Path::line(p(50.0, 50.0), p(60.0, 50.0)).unwrap();
    assert_eq!(path.extend_end(EndpointRef::End(0), &[&far]), Err(ExtendError::NoBoundaryAhead));
}

#[test]
fn extending_a_line_through_extend_end_matches_extend_line() {
    let line = Path::line(p(0.0, 0.0), p(4.0, 0.0)).unwrap();
    let wall = Path::line(p(10.0, -5.0), p(10.0, 5.0)).unwrap();
    assert_eq!(
        line.extend_end(EndpointRef::End(0), &[&wall]).unwrap(),
        line.extend_line(EndpointRef::End(0), &[&wall]).unwrap()
    );
}

#[test]
fn breaking_an_open_line_gives_two_pieces_and_a_closed_contour_one_open_ring() {
    let line = Path::line(p(0.0, 0.0), p(10.0, 0.0)).unwrap();
    let out = line.break_at(p(4.0, 0.3)).unwrap();
    assert_eq!(out.subpaths().len(), 2);
    assert!(near(out.subpaths()[0].start(), p(0.0, 0.0)) && near(out.subpaths()[0].end(), p(4.0, 0.0)));
    assert!(near(out.subpaths()[1].start(), p(4.0, 0.0)) && near(out.subpaths()[1].end(), p(10.0, 0.0)));
    assert_eq!(line.break_at(p(0.0, 0.0)), Err(BreakError::AtEnd));

    let ring = rect(0.0, 0.0, 10.0, 10.0).break_at(p(5.0, 0.2)).unwrap();
    assert_eq!(ring.subpaths().len(), 1);
    let sp = &ring.subpaths()[0];
    assert!(!sp.is_closed());
    assert!(near(sp.start(), p(5.0, 0.0)) && near(sp.end(), p(5.0, 0.0)));
    assert_eq!(sp.segments().len(), 5, "{:?}", sp.segments());
    assert_eq!(Path::default().break_at(p(0.0, 0.0)), Err(BreakError::EmptyPath));
}

// ---------------------------------------------------------------------------------------------
// Splines and conics

fn endpoints(path: &Path) -> Vec<Point> {
    let sp = &path.subpaths()[0];
    std::iter::once(sp.start()).chain(sp.segments().iter().map(Segment::end)).collect()
}

#[test]
fn fit_spline_passes_through_every_point_with_continuous_tangents() {
    let pts = [p(0.0, 0.0), p(4.0, 5.0), p(9.0, 3.0), p(14.0, 8.0)];
    let s = Path::spline_fit(&pts, false).unwrap();
    assert_eq!(s.subpaths().len(), 1);
    assert!(!s.subpaths()[0].is_closed());
    assert_eq!(s.subpaths()[0].segments().len(), 3);
    for (got, want) in endpoints(&s).iter().zip(pts) {
        assert!(near(*got, want), "{got:?} vs {want:?}");
    }
    // Tangent of the end of segment k equals the tangent of the start of segment k+1.
    let sp = &s.subpaths()[0];
    let mut from = sp.start();
    let mut starts = vec![from];
    for seg in sp.segments() {
        from = seg.end();
        starts.push(from);
    }
    for k in 0..2 {
        let a = sp.segments()[k].derivative(starts[k], 1.0);
        let b = sp.segments()[k + 1].derivative(starts[k + 1], 0.0);
        assert!((a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9, "{a:?} vs {b:?}");
    }
}

#[test]
fn fit_spline_through_collinear_points_stays_on_the_line_and_closed_fit_wraps() {
    let s = Path::spline_fit(&[p(0.0, 0.0), p(1.0, 1.0), p(5.0, 5.0), p(9.0, 9.0)], false).unwrap();
    let sp = &s.subpaths()[0];
    let mut from = sp.start();
    for seg in sp.segments() {
        for i in 0..=10 {
            let q = seg.point_at(from, f64::from(i) / 10.0);
            assert!((q.x - q.y).abs() < 1e-9, "{q:?}");
        }
        from = seg.end();
    }
    let ring = Path::spline_fit(&[p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)], true).unwrap();
    assert!(ring.subpaths()[0].is_closed());
    assert_eq!(ring.subpaths()[0].segments().len(), 4, "one cubic per fit point when closed");
    assert!(ring.signed_area() > 0.0);
    assert_eq!(Path::spline_fit(&[p(0.0, 0.0)], false), Err(GeometryError::TooFewPoints));
    assert_eq!(Path::spline_fit(&[p(0.0, 0.0), p(f64::NAN, 1.0)], false), Err(GeometryError::NotFinite));
}

/// Cox–de Boor B-spline value on the open uniform knot vector of `n` control points (degree 3).
fn de_boor_reference(ctrl: &[Point], u: f64) -> Point {
    let n = ctrl.len();
    let spans = n - 3;
    let mut knots = vec![0.0; 4];
    knots.extend((1..spans).map(|i| i as f64));
    knots.extend(std::iter::repeat_n(spans as f64, 4));
    fn basis(i: usize, k: usize, u: f64, t: &[f64], last: f64) -> f64 {
        if k == 0 {
            let inside = u >= t[i] && u < t[i + 1] || (u == last && t[i] < t[i + 1] && t[i + 1] == last);
            return if inside { 1.0 } else { 0.0 };
        }
        let mut v = 0.0;
        let d1 = t[i + k] - t[i];
        if d1 > 0.0 {
            v += (u - t[i]) / d1 * basis(i, k - 1, u, t, last);
        }
        let d2 = t[i + k + 1] - t[i + 1];
        if d2 > 0.0 {
            v += (t[i + k + 1] - u) / d2 * basis(i + 1, k - 1, u, t, last);
        }
        v
    }
    let (mut x, mut y) = (0.0, 0.0);
    for (i, c) in ctrl.iter().enumerate() {
        let w = basis(i, 3, u, &knots, spans as f64);
        x += w * c.x;
        y += w * c.y;
    }
    p(x, y)
}

#[test]
fn control_spline_of_four_points_is_that_bezier_and_longer_ones_match_cox_de_boor() {
    let four = [p(0.0, 0.0), p(2.0, 6.0), p(8.0, 6.0), p(10.0, 0.0)];
    let s = Path::spline_control(&four, false).unwrap();
    let sp = &s.subpaths()[0];
    assert_eq!(sp.segments().len(), 1);
    let Segment::Cubic { c1, c2, to } = sp.segments()[0] else { unreachable!("a cubic") };
    assert!(near(sp.start(), four[0]) && near(c1, four[1]) && near(c2, four[2]) && near(to, four[3]));

    let ctrl = [p(0.0, 0.0), p(3.0, 8.0), p(7.0, -2.0), p(11.0, 9.0), p(14.0, 1.0), p(18.0, 6.0)];
    let s = Path::spline_control(&ctrl, false).unwrap();
    let sp = &s.subpaths()[0];
    assert_eq!(sp.segments().len(), ctrl.len() - 3);
    let mut from = sp.start();
    for (k, seg) in sp.segments().iter().enumerate() {
        for i in 0..=8 {
            let t = f64::from(i) / 8.0;
            let got = seg.point_at(from, t);
            let want = de_boor_reference(&ctrl, k as f64 + t);
            assert!(got.distance(want) < 1e-9, "segment {k} t={t}: {got:?} vs {want:?}");
        }
        from = seg.end();
    }
    assert!(near(sp.start(), ctrl[0]) && near(from, ctrl[5]), "clamped ends");
    assert_eq!(Path::spline_control(&ctrl[..3], false), Err(GeometryError::TooFewPoints));
}

#[test]
fn closed_control_spline_is_a_periodic_b_spline_inside_the_control_hull() {
    let ctrl = [p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)];
    let s = Path::spline_control(&ctrl, true).unwrap();
    let sp = &s.subpaths()[0];
    assert!(sp.is_closed());
    assert_eq!(sp.segments().len(), 4);
    // Periodic uniform cubic B-spline knot values: (P[i-1] + 4 P[i] + P[i+1]) / 6.
    let knot = |i: usize| {
        let (a, b, c) = (ctrl[(i + 3) % 4], ctrl[i], ctrl[(i + 1) % 4]);
        p((a.x + 4.0 * b.x + c.x) / 6.0, (a.y + 4.0 * b.y + c.y) / 6.0)
    };
    let pts = endpoints(&s);
    let wanted: Vec<Point> = (0..4).map(knot).collect();
    // The ring may start at any knot; every knot value must appear.
    for w in &wanted {
        assert!(pts.iter().any(|q| near(*q, *w)), "{w:?} missing in {pts:?}");
    }
    let b = s.bounds().unwrap();
    assert!(b.min().x >= 0.0 && b.max().x <= 10.0 && b.min().y >= 0.0 && b.max().y <= 10.0);
}

#[test]
fn conic_with_rho_one_half_is_the_parabola_and_a_circle_weight_gives_a_circle() {
    let (a, b, apex) = (p(0.0, 0.0), p(20.0, 0.0), p(10.0, 20.0));
    let c = Path::conic(a, b, apex, 0.5).unwrap();
    let sp = &c.subpaths()[0];
    let n = sp.segments().len();
    assert!(n >= 4);
    assert!(near(sp.start(), a) && near(sp.end(), b));
    let quad = |t: f64| {
        let (m, mt) = (t, 1.0 - t);
        p(mt * mt * a.x + 2.0 * mt * m * apex.x + m * m * b.x, mt * mt * a.y + 2.0 * mt * m * apex.y + m * m * b.y)
    };
    let mut from = sp.start();
    for (k, seg) in sp.segments().iter().enumerate() {
        for i in 0..=4 {
            let t = f64::from(i) / 4.0;
            let want = quad((k as f64 + t) / n as f64);
            assert!(seg.point_at(from, t).distance(want) < 1e-9);
        }
        from = seg.end();
    }

    // Quarter of the unit circle: weight √2/2 ⇒ rho = w / (1 + w).
    let w = 0.5f64.sqrt();
    let q = Path::conic(p(1.0, 0.0), p(0.0, 1.0), p(1.0, 1.0), w / (1.0 + w)).unwrap();
    let sp = &q.subpaths()[0];
    let mut from = sp.start();
    for seg in sp.segments() {
        for i in 0..=8 {
            let r = seg.point_at(from, f64::from(i) / 8.0);
            assert!((r.x.hypot(r.y) - 1.0).abs() < 1e-4, "{r:?}");
        }
        from = seg.end();
    }
}

#[test]
fn conic_rejects_invalid_rho_and_coincident_ends() {
    let (a, b, apex) = (p(0.0, 0.0), p(20.0, 0.0), p(10.0, 20.0));
    assert_eq!(Path::conic(a, b, apex, 0.0), Err(GeometryError::InvalidParameter));
    assert_eq!(Path::conic(a, b, apex, 1.0), Err(GeometryError::InvalidParameter));
    assert_eq!(Path::conic(a, b, apex, f64::NAN), Err(GeometryError::InvalidParameter));
    assert_eq!(Path::conic(a, a, apex, 0.5), Err(GeometryError::InvalidParameter));
}
