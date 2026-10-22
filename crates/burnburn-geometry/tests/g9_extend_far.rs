//! G9-editor S01: extending a Bézier end beyond three parameter lengths follows the cubic where that
//! is stable, and otherwise continues straight along the end tangent and says why. Oracles are
//! closed forms: the parabola `y = x^2 / 10` stays on its equation, the swinging cubic below turns
//! back on itself at a known parameter, and the tangent ray hits a wall at a known height.

#![allow(clippy::unwrap_used, clippy::unreachable)] // test helpers and let-else in tests

use burnburn_geometry::{EndpointRef, ExtendError, ExtendNote, Path, PathBuilder, Point, Segment};

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

/// The parabola `y = x^2 / 10` from the origin to (10, 10); it ends heading (1, 2).
fn parabola() -> Path {
    PathBuilder::new()
        .move_to(p(0.0, 0.0))
        .cubic_to(p(10.0 / 3.0, 0.0), p(20.0 / 3.0, 10.0 / 3.0), p(10.0, 10.0))
        .build()
        .unwrap()
}

/// A cubic that swings: from (0, 0) heading right, to (10, 10) heading up, and beyond its end the
/// polynomial curls round to the left and back down (at parameter 1 + s its velocity is
/// proportional to (-5 s^2 - 10 s, 5 - 5 s^2)).
fn swing() -> Path {
    PathBuilder::new().move_to(p(0.0, 0.0)).cubic_to(p(5.0, 0.0), p(10.0, 5.0), p(10.0, 10.0)).build().unwrap()
}

fn wall(x: f64) -> Path {
    Path::line(p(x, -5000.0), p(x, 5000.0)).unwrap()
}

#[test]
fn a_stable_cubic_is_followed_to_a_boundary_many_parameter_lengths_away() {
    // The wall x = 100 is 9 parameter lengths ahead, past the old reach of 3: the end stays on the
    // parabola y = x^2 / 10 and ends at (100, 1000), still one cubic.
    let (out, note) = parabola().extend_end_noted(EndpointRef::End(0), &[&wall(100.0)]).unwrap();
    let sp = &out.subpaths()[0];
    assert!(sp.end().distance(p(100.0, 1000.0)) < 1e-6, "{:?}", sp.end());
    assert_eq!(sp.segments().len(), 1, "one cubic: the curve is its own continuation");
    assert_eq!(note, ExtendNote::OwnCurve);
    let Segment::Cubic { .. } = sp.segments()[0] else { unreachable!() };
    // Every point of the longer curve lies on the parabola.
    for i in 0..=200 {
        let q = sp.segments()[0].point_at(sp.start(), f64::from(i) / 200.0);
        assert!((q.y - q.x * q.x / 10.0).abs() < 1e-6 * (1.0 + q.y.abs()), "{q:?}");
    }
}

#[test]
fn the_start_of_a_stable_cubic_is_followed_backwards_the_same_way() {
    // The same parabola drawn the other way round, from (10, 10) down to the origin: its start is
    // the far end, and the wall x = 100 lies 9 parameter lengths behind it.
    let reversed = PathBuilder::new()
        .move_to(p(10.0, 10.0))
        .cubic_to(p(20.0 / 3.0, 10.0 / 3.0), p(10.0 / 3.0, 0.0), p(0.0, 0.0))
        .build()
        .unwrap();
    let (out, note) = reversed.extend_end_noted(EndpointRef::Start(0), &[&wall(100.0)]).unwrap();
    let sp = &out.subpaths()[0];
    assert!(sp.start().distance(p(100.0, 1000.0)) < 1e-6, "{:?}", sp.start());
    assert!(sp.end().distance(p(0.0, 0.0)) < 1e-9);
    assert_eq!(sp.segments().len(), 1);
    assert_eq!(note, ExtendNote::OwnCurve);
    // Behind the start of the parabola itself the curve soon stands up steeply (85 degrees off its
    // heading at 6 parameter lengths): that is no longer one heading, so the start goes straight.
    let (_, note) = parabola().extend_end_noted(EndpointRef::Start(0), &[&wall(-60.0)]).unwrap();
    assert_eq!(note, ExtendNote::StraightTurns);
}

#[test]
fn a_boundary_the_cubic_would_meet_only_after_it_turns_back_is_reached_straight_with_the_reason() {
    // The wall x = -400 is met by the swinging cubic at about 3.6 parameter lengths past the end,
    // after it has turned away by far more than a quarter turn. The end continues straight up
    // (its heading is (0, 1)) to the horizontal wall at y = 50 and says why.
    let ceiling = Path::line(p(-600.0, 50.0), p(600.0, 50.0)).unwrap();
    let (out, note) = swing().extend_end_noted(EndpointRef::End(0), &[&wall(-400.0), &ceiling]).unwrap();
    let sp = &out.subpaths()[0];
    assert!(sp.end().distance(p(10.0, 50.0)) < 1e-6, "{:?}", sp.end());
    assert_eq!(sp.segments().len(), 2, "the drawn cubic, then a straight piece");
    assert!(matches!(sp.segments()[1], Segment::Line { .. }));
    assert_eq!(note, ExtendNote::StraightTurns);
}

#[test]
fn a_boundary_beyond_thirty_parameter_lengths_is_reached_straight_with_the_reason() {
    // The wall x = 400 is 39 parameter lengths ahead: too far for a polynomial that says nothing
    // about the drawn shape out there. Straight along (1, 2): y = 10 + 2 * 390 = 790. (The wall is
    // tall enough for the parabola itself, which is at y = 16000 there.)
    let tall = Path::line(p(400.0, -50_000.0), p(400.0, 50_000.0)).unwrap();
    let (out, note) = parabola().extend_end_noted(EndpointRef::End(0), &[&tall]).unwrap();
    assert!(out.subpaths()[0].end().distance(p(400.0, 790.0)) < 1e-6, "{:?}", out.subpaths()[0].end());
    assert_eq!(note, ExtendNote::StraightTooFar);
}

#[test]
fn a_boundary_the_cubic_never_meets_is_reached_straight_with_the_reason() {
    // The ceiling y = 50 is crossed by the tangent ray but not by the swinging cubic (it peaks at 20).
    let ceiling = Path::line(p(-600.0, 50.0), p(600.0, 50.0)).unwrap();
    let (out, note) = swing().extend_end_noted(EndpointRef::End(0), &[&ceiling]).unwrap();
    assert!(out.subpaths()[0].end().distance(p(10.0, 50.0)) < 1e-6);
    assert_eq!(note, ExtendNote::StraightMisses);
}

#[test]
fn within_three_parameter_lengths_nothing_changes_and_lines_say_nothing() {
    let (out, note) = parabola().extend_end_noted(EndpointRef::End(0), &[&wall(20.0)]).unwrap();
    assert!(out.subpaths()[0].end().distance(p(20.0, 40.0)) < 1e-6);
    assert_eq!(note, ExtendNote::OwnCurve);
    let line = Path::line(p(0.0, 0.0), p(4.0, 0.0)).unwrap();
    let (out, note) = line.extend_end_noted(EndpointRef::End(0), &[&wall(10.0)]).unwrap();
    assert!(out.subpaths()[0].end().distance(p(10.0, 0.0)) < 1e-9);
    assert_eq!(note, ExtendNote::Plain);
    // And `extend_end` is the same path without the note.
    assert_eq!(
        parabola().extend_end(EndpointRef::End(0), &[&wall(20.0)]).unwrap(),
        parabola().extend_end_noted(EndpointRef::End(0), &[&wall(20.0)]).unwrap().0
    );
}

#[test]
fn nothing_ahead_is_still_a_typed_refusal() {
    let behind = wall(5.0);
    assert_eq!(parabola().extend_end_noted(EndpointRef::End(0), &[&behind]), Err(ExtendError::NoBoundaryAhead));
}
