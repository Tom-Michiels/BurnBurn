//! Public contract: sketch constructions (S-draw). Oracles are closed-form centres, radii, areas
//! and extents worked out by hand, not the implementation's formulas.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use std::f64::consts::{FRAC_PI_2, PI, SQRT_2};

use burnburn_geometry::{
    ConstructError, Point, Rect, Segment, Vector, arc_center_start_end, arc_tangent, arc_through_three,
    circle_tangent_three_lines, circle_tangent_two_lines, circle_tangent_two_lines_radius, circle_through_three,
    ellipse_from_axes, polygon_circumscribed, polygon_from_edge, polygon_inscribed, rectangle_three_point,
    rounded_rectangle, slot_along_arc, slot_center_to_center, slot_overall,
};
use burnburn_types::{Angle, Length};

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn near_pt(a: Point, b: Point) -> bool {
    near(a.x, b.x) && near(a.y, b.y)
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect::new(p(x, y), Length::from_mm(w).unwrap(), Length::from_mm(h).unwrap()).unwrap()
}

fn rad(r: f64) -> Angle {
    Angle::from_radians(r).unwrap()
}

#[test]
fn circle_through_three_points_has_known_centre_and_radius() {
    let c = circle_through_three(p(0.0, 0.0), p(10.0, 0.0), p(0.0, 10.0)).unwrap();
    assert!(near_pt(c.center, p(5.0, 5.0)), "{c:?}");
    assert!(near(c.radius, 50.0_f64.sqrt()), "{c:?}");
}

#[test]
fn circle_through_three_is_independent_of_point_order_and_offset() {
    let c = circle_through_three(p(1e6 + 10.0, 1e6), p(1e6, 1e6 + 10.0), p(1e6 - 10.0, 1e6)).unwrap();
    assert!(near_pt(c.center, p(1e6, 1e6)) && near(c.radius, 10.0), "{c:?}");
}

#[test]
fn circle_through_collinear_or_coincident_points_is_a_typed_error() {
    assert_eq!(circle_through_three(p(0.0, 0.0), p(1.0, 1.0), p(2.0, 2.0)), Err(ConstructError::Collinear));
    assert_eq!(circle_through_three(p(0.0, 0.0), p(0.0, 0.0), p(2.0, 2.0)), Err(ConstructError::Coincident));
    assert_eq!(circle_through_three(p(f64::NAN, 0.0), p(1.0, 1.0), p(2.0, 5.0)), Err(ConstructError::NotFinite));
}

#[test]
fn circle_tangent_to_both_axes_follows_the_sector_of_the_cursor() {
    let x_axis = (p(0.0, 0.0), p(1.0, 0.0));
    let y_axis = (p(0.0, 0.0), p(0.0, 1.0));
    let c = circle_tangent_two_lines(x_axis, y_axis, p(3.0, 3.5)).unwrap();
    // Projection of the cursor onto the bisector y = x: (3.25, 3.25); radius = distance to both axes.
    assert!(near_pt(c.center, p(3.25, 3.25)) && near(c.radius, 3.25), "{c:?}");
    let c = circle_tangent_two_lines(x_axis, y_axis, p(-2.0, 2.0)).unwrap();
    assert!(near_pt(c.center, p(-2.0, 2.0)) && near(c.radius, 2.0), "{c:?}");
}

#[test]
fn circle_tangent_to_two_parallel_lines_fills_the_strip() {
    let c = circle_tangent_two_lines((p(0.0, 0.0), p(1.0, 0.0)), (p(0.0, 10.0), p(1.0, 10.0)), p(3.0, 4.0)).unwrap();
    assert!(near_pt(c.center, p(3.0, 5.0)) && near(c.radius, 5.0), "{c:?}");
    // A cursor outside the strip still gives the circle inside it.
    let c = circle_tangent_two_lines((p(0.0, 0.0), p(1.0, 0.0)), (p(0.0, 10.0), p(1.0, 10.0)), p(3.0, 40.0)).unwrap();
    assert!(near_pt(c.center, p(3.0, 5.0)) && near(c.radius, 5.0), "{c:?}");
}

#[test]
fn circle_tangent_to_two_lines_with_given_radius() {
    let x_axis = (p(0.0, 0.0), p(1.0, 0.0));
    let y_axis = (p(0.0, 0.0), p(0.0, 1.0));
    let c = circle_tangent_two_lines_radius(x_axis, y_axis, 2.0, p(5.0, 5.0)).unwrap();
    assert!(near_pt(c.center, p(2.0, 2.0)) && near(c.radius, 2.0), "{c:?}");
    let c = circle_tangent_two_lines_radius(x_axis, y_axis, 2.0, p(-5.0, 5.0)).unwrap();
    assert!(near_pt(c.center, p(-2.0, 2.0)), "{c:?}");
    assert_eq!(
        circle_tangent_two_lines_radius(x_axis, (p(0.0, 5.0), p(1.0, 5.0)), 2.0, p(0.0, 1.0)),
        Err(ConstructError::Parallel)
    );
    assert_eq!(
        circle_tangent_two_lines_radius(x_axis, y_axis, 0.0, p(1.0, 1.0)),
        Err(ConstructError::RadiusNotPositive)
    );
}

#[test]
fn degenerate_tangent_lines_are_typed_errors() {
    let x_axis = (p(0.0, 0.0), p(1.0, 0.0));
    assert_eq!(
        circle_tangent_two_lines(x_axis, (p(2.0, 2.0), p(2.0, 2.0)), p(1.0, 1.0)),
        Err(ConstructError::Coincident)
    );
    // The cursor on the intersection gives radius zero.
    let y_axis = (p(0.0, 0.0), p(0.0, 1.0));
    assert_eq!(circle_tangent_two_lines(x_axis, y_axis, p(0.0, 0.0)), Err(ConstructError::NoSolution));
}

#[test]
fn circle_tangent_to_three_lines_is_the_incircle_or_an_excircle_near_the_cursor() {
    // 3-4-5 triangle: inradius (3 + 4 - 5) / 2 = 1, centre (1, 1).
    let l1 = (p(0.0, 0.0), p(4.0, 0.0));
    let l2 = (p(4.0, 0.0), p(0.0, 3.0));
    let l3 = (p(0.0, 3.0), p(0.0, 0.0));
    let c = circle_tangent_three_lines(l1, l2, l3, p(1.2, 0.8)).unwrap();
    assert!(near_pt(c.center, p(1.0, 1.0)) && near(c.radius, 1.0), "{c:?}");
    // Excircle opposite the right angle: radius (3 + 4 + 5) / 2 - 5 ... = s = 6, r_c = Area / (s - c) = 6 / 1 = 6.
    let c = circle_tangent_three_lines(l1, l2, l3, p(6.0, 6.0)).unwrap();
    assert!(near_pt(c.center, p(6.0, 6.0)) && near(c.radius, 6.0), "{c:?}");
}

#[test]
fn three_parallel_lines_have_no_tangent_circle() {
    let h = |y: f64| (p(0.0, y), p(1.0, y));
    assert_eq!(circle_tangent_three_lines(h(0.0), h(4.0), h(9.0), p(0.0, 2.0)), Err(ConstructError::NoSolution));
}

#[test]
fn arc_through_three_points_passes_through_the_middle_and_keeps_direction() {
    let a = arc_through_three(p(10.0, 0.0), p(0.0, 10.0), p(-10.0, 0.0)).unwrap();
    assert!(near_pt(a.start_point(), p(10.0, 0.0)) && near_pt(a.end_point(), p(-10.0, 0.0)));
    assert!(near_pt(a.point_at_fraction(0.5), p(0.0, 10.0)), "{:?}", a.point_at_fraction(0.5));
    assert!(near(a.sweep_parameter(), PI), "counter-clockwise half turn: {}", a.sweep_parameter());
    // Reversed click order runs clockwise through the same middle.
    let a = arc_through_three(p(-10.0, 0.0), p(0.0, 10.0), p(10.0, 0.0)).unwrap();
    assert!(near(a.sweep_parameter(), -PI), "{}", a.sweep_parameter());
    assert!(near_pt(a.point_at_fraction(0.5), p(0.0, 10.0)));
    // The long way round: more than a half turn.
    let a = arc_through_three(p(10.0, 0.0), p(-10.0, 0.0), p(0.0, 10.0)).unwrap();
    assert!(near(a.sweep_parameter(), 1.5 * PI) || near(a.sweep_parameter(), -1.5 * PI), "{}", a.sweep_parameter());
    assert!(near_pt(a.point_at_fraction(2.0 / 3.0), p(-10.0, 0.0)), "passes through the second click");
}

#[test]
fn arc_through_collinear_points_is_a_typed_error() {
    assert!(matches!(arc_through_three(p(0.0, 0.0), p(1.0, 0.0), p(2.0, 0.0)), Err(ConstructError::Collinear)));
}

#[test]
fn arc_from_centre_start_and_end_direction() {
    // End given beyond the circle: only its direction counts.
    let a = arc_center_start_end(p(0.0, 0.0), p(10.0, 0.0), p(0.0, 25.0), true).unwrap();
    assert!(near_pt(a.end_point(), p(0.0, 10.0)) && near(a.sweep_parameter(), FRAC_PI_2));
    let a = arc_center_start_end(p(0.0, 0.0), p(10.0, 0.0), p(0.0, 25.0), false).unwrap();
    assert!(near_pt(a.end_point(), p(0.0, 10.0)) && near(a.sweep_parameter(), -1.5 * PI), "{}", a.sweep_parameter());
    assert!(arc_center_start_end(p(0.0, 0.0), p(0.0, 0.0), p(1.0, 1.0), true).is_err());
    assert!(arc_center_start_end(p(0.0, 0.0), p(1.0, 0.0), p(0.0, 0.0), true).is_err());
}

#[test]
fn tangent_arc_leaves_the_start_along_the_tangent() {
    let a = arc_tangent(p(0.0, 0.0), Vector::new(1.0, 0.0), p(10.0, 10.0)).unwrap();
    // Centre (0, 10), radius 10, quarter turn counter-clockwise.
    assert!(near_pt(a.center(), p(0.0, 10.0)), "{:?}", a.center());
    assert!(near_pt(a.start_point(), p(0.0, 0.0)) && near_pt(a.end_point(), p(10.0, 10.0)));
    assert!(near(a.sweep_parameter(), FRAC_PI_2));
    // End on the other side of the tangent: clockwise.
    let a = arc_tangent(p(0.0, 0.0), Vector::new(1.0, 0.0), p(10.0, -10.0)).unwrap();
    assert!(near_pt(a.center(), p(0.0, -10.0)) && near(a.sweep_parameter(), -FRAC_PI_2), "{a:?}");
    // The tangent need not be unit length.
    let a = arc_tangent(p(0.0, 0.0), Vector::new(0.0, 7.0), p(-10.0, 10.0)).unwrap();
    assert!(near_pt(a.center(), p(-10.0, 0.0)), "{:?}", a.center());
}

#[test]
fn tangent_arc_to_a_point_on_its_tangent_line_is_a_typed_error() {
    assert_eq!(arc_tangent(p(0.0, 0.0), Vector::new(1.0, 0.0), p(5.0, 0.0)), Err(ConstructError::Collinear));
    assert!(arc_tangent(p(0.0, 0.0), Vector::new(0.0, 0.0), p(5.0, 1.0)).is_err());
    assert!(arc_tangent(p(1.0, 1.0), Vector::new(1.0, 0.0), p(1.0, 1.0)).is_err());
}

#[test]
fn three_point_rectangle_extends_the_edge_to_the_third_click() {
    let corners = rectangle_three_point(p(0.0, 0.0), p(4.0, 0.0), p(1.0, 3.0)).unwrap();
    let expect = [p(0.0, 0.0), p(4.0, 0.0), p(4.0, 3.0), p(0.0, 3.0)];
    assert!(corners.iter().zip(expect).all(|(a, b)| near_pt(*a, b)), "{corners:?}");
    // Rotated 45° and on the other side.
    let c = rectangle_three_point(p(0.0, 0.0), p(2.0, 2.0), p(2.0, 0.0)).unwrap();
    assert!(near_pt(c[3], p(1.0, -1.0)) && near_pt(c[2], p(3.0, 1.0)), "{c:?}");
}

#[test]
fn three_point_rectangle_needs_width_and_length() {
    assert_eq!(rectangle_three_point(p(0.0, 0.0), p(4.0, 0.0), p(9.0, 0.0)), Err(ConstructError::Collinear));
    assert_eq!(rectangle_three_point(p(1.0, 1.0), p(1.0, 1.0), p(9.0, 0.0)), Err(ConstructError::Coincident));
}

#[test]
fn rounded_rectangle_has_analytic_area_and_unchanged_bounds() {
    let path = rounded_rectangle(rect(0.0, 0.0, 20.0, 10.0), 2.0).unwrap();
    // 4 corner quarter discs replace 4 corner squares: area = 200 - 4 r^2 (1 - pi/4).
    let expect = 200.0 - 16.0 * (1.0 - PI / 4.0);
    assert!(near(path.signed_area(), expect), "{} vs {expect}", path.signed_area());
    let b = path.bounds().unwrap();
    assert!(near(b.min().x, 0.0) && near(b.min().y, 0.0) && near(b.max().x, 20.0) && near(b.max().y, 10.0), "{b:?}");
    assert_eq!(path.subpaths().len(), 1);
    assert!(path.subpaths()[0].is_closed());
    let arcs = path.subpaths()[0].segments().iter().filter(|s| matches!(s, Segment::Arc(_))).count();
    assert_eq!(arcs, 4);
}

#[test]
fn rounded_rectangle_radius_up_to_half_the_short_side_is_a_stadium() {
    let path = rounded_rectangle(rect(0.0, 0.0, 20.0, 10.0), 5.0).unwrap();
    let expect = 20.0 * 10.0 - (4.0 - PI) * 25.0;
    assert!(near(path.signed_area(), expect), "{}", path.signed_area());
}

#[test]
fn rounded_rectangle_rejects_oversized_or_non_positive_radius() {
    assert_eq!(rounded_rectangle(rect(0.0, 0.0, 20.0, 10.0), 5.01).unwrap_err(), ConstructError::RadiusTooLarge);
    assert_eq!(rounded_rectangle(rect(0.0, 0.0, 20.0, 10.0), 0.0).unwrap_err(), ConstructError::RadiusNotPositive);
    assert_eq!(rounded_rectangle(rect(0.0, 0.0, 20.0, 10.0), -1.0).unwrap_err(), ConstructError::RadiusNotPositive);
    assert_eq!(rounded_rectangle(rect(0.0, 0.0, 20.0, 10.0), f64::NAN).unwrap_err(), ConstructError::NotFinite);
}

#[test]
fn ellipse_from_centre_major_end_and_minor_point() {
    let e = ellipse_from_axes(p(1.0, 1.0), p(5.0, 1.0), p(2.0, 3.0)).unwrap();
    assert!(near_pt(e.center, p(1.0, 1.0)) && near(e.rx, 4.0) && near(e.ry, 2.0) && near(e.rotation.radians(), 0.0));
    let e = ellipse_from_axes(p(1.0, 1.0), p(1.0, 5.0), p(-1.0, 2.0)).unwrap();
    assert!(near(e.rx, 4.0) && near(e.ry, 2.0) && near(e.rotation.radians(), FRAC_PI_2), "{e:?}");
}

#[test]
fn ellipse_needs_distinct_axis_end_and_an_off_axis_minor_point() {
    assert_eq!(ellipse_from_axes(p(0.0, 0.0), p(0.0, 0.0), p(1.0, 1.0)), Err(ConstructError::Coincident));
    assert_eq!(ellipse_from_axes(p(0.0, 0.0), p(4.0, 0.0), p(2.0, 0.0)), Err(ConstructError::Collinear));
}

#[test]
fn polygon_inscribed_puts_the_first_vertex_on_the_click() {
    let s = polygon_inscribed(p(0.0, 0.0), p(0.0, 10.0), 6).unwrap();
    assert!(near(s.radius, 10.0) && s.sides == 6 && near(s.rotation.radians(), FRAC_PI_2), "{s:?}");
    assert_eq!(polygon_inscribed(p(0.0, 0.0), p(0.0, 10.0), 2), Err(ConstructError::TooFewSides));
    assert_eq!(polygon_inscribed(p(1.0, 1.0), p(1.0, 1.0), 5), Err(ConstructError::Coincident));
}

#[test]
fn polygon_circumscribed_puts_an_edge_midpoint_on_the_click() {
    // Square with apothem 5: circumradius 5 sqrt 2, vertices (5, -5) and (5, 5) around the click (5, 0).
    let s = polygon_circumscribed(p(0.0, 0.0), p(5.0, 0.0), 4).unwrap();
    assert!(near(s.radius, 5.0 * SQRT_2), "{s:?}");
    let path = burnburn_geometry::Shape::Polygon {
        center: s.center,
        radius: Length::from_mm(s.radius).unwrap(),
        sides: s.sides,
        rotation: s.rotation,
    }
    .to_path()
    .unwrap();
    assert!(near(path.signed_area(), 4.0 * 25.0), "a square of side 10: {}", path.signed_area());
    let b = path.bounds().unwrap();
    assert!(near(b.max().x, 5.0) && near(b.min().x, -5.0) && near(b.max().y, 5.0), "{b:?}");
}

#[test]
fn polygon_from_edge_uses_the_two_clicks_as_one_side() {
    let s = polygon_from_edge(p(0.0, 0.0), p(2.0, 0.0), 4).unwrap();
    assert!(near_pt(s.center, p(1.0, 1.0)) && near(s.radius, SQRT_2), "{s:?}");
    let path = burnburn_geometry::Shape::Polygon {
        center: s.center,
        radius: Length::from_mm(s.radius).unwrap(),
        sides: s.sides,
        rotation: s.rotation,
    }
    .to_path()
    .unwrap();
    let sp = &path.subpaths()[0];
    assert!(near_pt(sp.start(), p(0.0, 0.0)) && near_pt(sp.segments()[0].end(), p(2.0, 0.0)), "{path:?}");
    assert_eq!(polygon_from_edge(p(0.0, 0.0), p(0.0, 0.0), 4), Err(ConstructError::Coincident));
    assert_eq!(polygon_from_edge(p(0.0, 0.0), p(1.0, 0.0), 1), Err(ConstructError::TooFewSides));
}

#[test]
fn slot_centre_to_centre_has_stadium_area_and_extent() {
    let path = slot_center_to_center(p(0.0, 0.0), p(10.0, 0.0), 4.0).unwrap();
    assert!(near(path.signed_area(), 10.0 * 4.0 + PI * 4.0), "{}", path.signed_area());
    let b = path.bounds().unwrap();
    assert!(near(b.min().x, -2.0) && near(b.max().x, 12.0) && near(b.min().y, -2.0) && near(b.max().y, 2.0), "{b:?}");
    // Direction independent.
    let rev = slot_center_to_center(p(10.0, 0.0), p(0.0, 0.0), 4.0).unwrap();
    assert!(near(rev.signed_area(), path.signed_area()));
}

#[test]
fn slot_centre_to_centre_rejects_degenerate_input() {
    assert_eq!(slot_center_to_center(p(0.0, 0.0), p(0.0, 0.0), 4.0), Err(ConstructError::Coincident));
    assert_eq!(slot_center_to_center(p(0.0, 0.0), p(5.0, 0.0), 0.0), Err(ConstructError::RadiusNotPositive));
    assert_eq!(slot_center_to_center(p(0.0, 0.0), p(5.0, 0.0), f64::INFINITY), Err(ConstructError::NotFinite));
}

#[test]
fn overall_slot_spans_exactly_the_two_clicks() {
    let path = slot_overall(p(0.0, 0.0), p(0.0, 14.0), 4.0).unwrap();
    let b = path.bounds().unwrap();
    assert!(near(b.min().y, 0.0) && near(b.max().y, 14.0) && near(b.min().x, -2.0) && near(b.max().x, 2.0), "{b:?}");
    assert!(near(path.signed_area(), 10.0 * 4.0 + PI * 4.0), "{}", path.signed_area());
    assert_eq!(slot_overall(p(0.0, 0.0), p(3.0, 0.0), 4.0), Err(ConstructError::TooShort));
    assert_eq!(slot_overall(p(0.0, 0.0), p(4.0, 0.0), 4.0), Err(ConstructError::TooShort), "that is a circle");
}

#[test]
fn arc_slot_has_annular_sector_plus_two_caps_area() {
    // Quarter turn, centreline radius 10, width 2: area = sweep R w + pi h^2 = 10 pi + pi.
    let path = slot_along_arc(p(0.0, 0.0), 10.0, rad(0.0), rad(FRAC_PI_2), 2.0).unwrap();
    assert!(near(path.signed_area(), 11.0 * PI), "{}", path.signed_area());
    let b = path.bounds().unwrap();
    // x from the inner end cap at the top (-1) to the outer arc (11) plus the cap on the x axis; y from -1 to 11.
    assert!(near(b.min().x, -1.0) && near(b.max().x, 11.0) && near(b.min().y, -1.0) && near(b.max().y, 11.0), "{b:?}");
}

#[test]
fn arc_slot_with_negative_sweep_equals_the_same_slot_counter_clockwise() {
    let ccw = slot_along_arc(p(0.0, 0.0), 10.0, rad(0.0), rad(FRAC_PI_2), 2.0).unwrap();
    let cw = slot_along_arc(p(0.0, 0.0), 10.0, rad(FRAC_PI_2), rad(-FRAC_PI_2), 2.0).unwrap();
    assert!(near(ccw.signed_area(), cw.signed_area()));
    let (a, b) = (ccw.bounds().unwrap(), cw.bounds().unwrap());
    assert!(near_pt(a.min(), b.min()) && near_pt(a.max(), b.max()));
}

#[test]
fn arc_slot_wider_than_its_radius_is_a_typed_error() {
    assert_eq!(slot_along_arc(p(0.0, 0.0), 10.0, rad(0.0), rad(1.0), 20.0), Err(ConstructError::RadiusTooLarge));
    assert_eq!(slot_along_arc(p(0.0, 0.0), 10.0, rad(0.0), rad(0.0), 2.0), Err(ConstructError::Coincident));
    assert_eq!(slot_along_arc(p(0.0, 0.0), 0.0, rad(0.0), rad(1.0), 2.0), Err(ConstructError::RadiusNotPositive));
}
