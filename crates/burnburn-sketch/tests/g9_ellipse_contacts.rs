//! G9-editor S01: an ellipse touching another ellipse also from a deep overlap (the solution that
//! moves the second ellipse least wins), and an arc touching an ellipse. Oracles are closed forms:
//! two horizontal ellipses on one level touch end to end `a1 + a2` apart (outside) or `a1 - a2`
//! apart (the smaller inside the larger), and a round curve around a point touches an ellipse at the
//! distance of the nearest point of the ellipse.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test helpers outside #[test] fns

use burnburn_document::sketch::{DimensionKind, DimensionRole, EntityId, Relation, Sketch, SketchError};
use burnburn_geometry::Point;
use burnburn_sketch::solver::SketchSolverAdapter;
use burnburn_sketch::{Evaluation, evaluate_sketch};
use burnburn_types::CancellationToken;

fn eval(s: &Sketch) -> Evaluation {
    evaluate_sketch(s, &SketchSolverAdapter, &CancellationToken::never()).unwrap()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-5
}

/// Ellipse around (10, 5): semi-major 4 along x (fixed), semi-minor 2 as drawn.
fn ellipse_alone() -> (Sketch, EntityId, EntityId, EntityId) {
    let mut s = Sketch::new("e");
    let c = s.point(10.0, 5.0).unwrap();
    let m = s.point(14.0, 5.0).unwrap();
    let e = s.ellipse(c, m, 2.0).unwrap();
    s.relate(Relation::Fix { point: c, at: Point::new(10.0, 5.0) }).unwrap();
    s.relate(Relation::Horizontal(e)).unwrap();
    s.dimension(DimensionKind::Radius { curve: e }, "4 mm", DimensionRole::Driving).unwrap();
    (s, c, m, e)
}

/// The first ellipse of `ellipse_alone` and a second horizontal one (`a2`, `b2`) whose centre slides
/// along the level line and is drawn at `drawn_c2`.
fn two_ellipses(drawn_c2: f64, a2: f64, b2: f64) -> (Sketch, EntityId) {
    let (mut s, c, _, e1) = ellipse_alone();
    let c2 = s.point(drawn_c2, 5.0).unwrap();
    let m2 = s.point(drawn_c2 + a2, 5.0).unwrap();
    let e2 = s.ellipse(c2, m2, b2).unwrap();
    let level = s.line(c, c2).unwrap();
    s.relate(Relation::Horizontal(level)).unwrap();
    s.relate(Relation::Horizontal(e2)).unwrap();
    s.dimension(DimensionKind::Radius { curve: e2 }, &format!("{a2} mm"), DimensionRole::Driving).unwrap();
    s.relate(Relation::Tangent(e1, e2)).unwrap();
    (s, c2)
}

fn solved_centre(drawn_c2: f64, a2: f64, b2: f64) -> f64 {
    let (s, c2) = two_ellipses(drawn_c2, a2, b2);
    let ev = eval(&s);
    assert!(ev.is_valid(), "drawn at {drawn_c2}: {:?} {:?}", ev.health, ev.diagnostics);
    let at = ev.solved.unwrap().point(c2).unwrap();
    assert!(near(at.y, 5.0), "on the level: {at:?}");
    at.x
}

#[test]
fn a_light_overlap_still_touches_from_the_outside() {
    // 2 mm of overlap (the end of the second ellipse at 12 + 3 = 15... drawn at 15: 4 + 3 = 7 apart wanted).
    assert!(near(solved_centre(15.0, 3.0, 1.5), 17.0));
    assert!(near(solved_centre(16.0, 3.0, 1.5), 17.0));
}

#[test]
fn a_deep_overlap_ends_in_the_touch_that_moves_the_second_ellipse_least() {
    // E2 (a = 3, b = 1.5) is sharper at its ends than E1 (a = 4, b = 2), so it fits inside E1 with
    // its right end on E1's right end when the centres are a1 - a2 = 1 apart; the other tangency is
    // outside, 7 apart. Drawn deep inside, the near one is meant.
    assert!(near(solved_centre(12.0, 3.0, 1.5), 11.0), "drawn 5 mm from the outside touch");
    assert!(near(solved_centre(13.0, 3.0, 1.5), 11.0));
    assert!(near(solved_centre(10.4, 3.0, 1.5), 11.0), "inside, a little to the left of the middle");
    // And on the other side of the middle the left end is the one that touches.
    assert!(near(solved_centre(9.6, 3.0, 1.5), 9.0));
}

#[test]
fn a_deep_overlap_with_the_touch_outside_nearest_is_outside() {
    // E2 (a = 2, b = 0.8) lies across the right end of E1: drawn at 13.5 (its left end at 11.5, deep
    // in E1), the nearest touch is the inside one at 4 - 2 = 2 apart, 12.
    assert!(near(solved_centre(13.5, 2.0, 0.8), 12.0));
    // And one drawn half a unit from touching on the outside is still the outside touch, 6 apart.
    assert!(near(solved_centre(16.5, 2.0, 0.8), 16.0));
}

#[test]
fn a_deep_overlap_works_in_any_direction_not_only_along_x() {
    // The second ellipse slides along the vertical through the first centre. Inside, its top meets
    // the top of E1 (y = 7) at a centre height of 7 - 1.5 = 5.5; outside it sits on top at 5 + 2 + 1.5 = 8.5.
    for (drawn, wanted) in [(6.2, 5.5), (5.9, 5.5), (7.9, 8.5)] {
        let (mut s, c, _, e1) = ellipse_alone();
        let c2 = s.point(10.0, drawn).unwrap();
        let m2 = s.point(13.0, drawn).unwrap();
        let e2 = s.ellipse(c2, m2, 1.5).unwrap();
        let level = s.line(c, c2).unwrap();
        s.relate(Relation::Vertical(level)).unwrap();
        s.relate(Relation::Horizontal(e2)).unwrap();
        s.dimension(DimensionKind::Radius { curve: e2 }, "3 mm", DimensionRole::Driving).unwrap();
        s.relate(Relation::Tangent(e1, e2)).unwrap();
        let ev = eval(&s);
        assert!(ev.is_valid(), "drawn at {drawn}: {:?} {:?}", ev.health, ev.diagnostics);
        let at = ev.solved.unwrap().point(c2).unwrap();
        assert!(near(at.x, 10.0) && near(at.y, wanted), "drawn at {drawn}: {at:?}");
    }
}

// --- an arc touching an ellipse -----------------------------------------------------------------------------

/// An arc around `centre` from `centre + (r, 0)` to `centre + (0, r)`.
fn quarter_arc(s: &mut Sketch, centre: (f64, f64), r: f64) -> (EntityId, EntityId) {
    let c = s.point(centre.0, centre.1).unwrap();
    let a = s.point(centre.0 + r, centre.1).unwrap();
    let b = s.point(centre.0, centre.1 + r).unwrap();
    s.relate(Relation::Fix { point: c, at: Point::new(centre.0, centre.1) }).unwrap();
    (s.arc(c, a, b, false).unwrap(), a)
}

#[test]
fn an_arc_outside_the_ellipse_grows_until_it_touches_the_end_of_the_major_axis() {
    let (mut s, _, _, e) = ellipse_alone();
    let (arc, start) = quarter_arc(&mut s, (16.0, 5.0), 1.7);
    s.relate(Relation::Tangent(e, arc)).unwrap();
    let ev = eval(&s);
    assert!(ev.is_valid(), "{:?} {:?}", ev.health, ev.diagnostics);
    let solved = ev.solved.unwrap();
    let r = solved.point(start).unwrap().distance(Point::new(16.0, 5.0));
    assert!(near(r, 2.0), "16 - 14 = 2: {r}");
}

#[test]
fn an_arc_inside_the_ellipse_grows_until_it_touches_the_minor_vertices() {
    let (mut s, _, _, e) = ellipse_alone();
    let (arc, start) = quarter_arc(&mut s, (10.0, 5.0), 1.5);
    s.relate(Relation::Tangent(arc, e)).unwrap();
    let ev = eval(&s);
    assert!(ev.is_valid(), "{:?} {:?}", ev.health, ev.diagnostics);
    let r = ev.solved.unwrap().point(start).unwrap().distance(Point::new(10.0, 5.0));
    assert!(near(r, 2.0), "the nearest points of the ellipse are its minor vertices: {r}");
}

#[test]
fn an_arc_of_given_radius_slides_along_a_line_until_it_touches() {
    let (mut s, c, _, e) = ellipse_alone();
    let cc = s.point(16.5, 5.0).unwrap();
    let a = s.point(18.0, 5.0).unwrap();
    let b = s.point(16.5, 6.5).unwrap();
    let level = s.line(c, cc).unwrap();
    s.relate(Relation::Horizontal(level)).unwrap();
    let arc = s.arc(cc, a, b, false).unwrap();
    s.dimension(DimensionKind::Radius { curve: arc }, "1.5 mm", DimensionRole::Driving).unwrap();
    s.relate(Relation::Tangent(e, arc)).unwrap();
    let ev = eval(&s);
    assert!(ev.is_valid(), "{:?} {:?}", ev.health, ev.diagnostics);
    let at = ev.solved.unwrap().point(cc).unwrap();
    assert!(near(at.x, 15.5) && near(at.y, 5.0), "4 + 1.5 from the centre: {at:?}");
}

#[test]
fn what_stays_refused_next_to_an_arc() {
    let (mut s, _, _, e) = ellipse_alone();
    let (arc, _) = quarter_arc(&mut s, (30.0, 30.0), 3.0);
    assert!(s.relate(Relation::Tangent(e, arc)).is_ok());
    assert!(matches!(s.relate(Relation::Equal(e, arc)), Err(SketchError::WrongEntityKind(_))));
}
