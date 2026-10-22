//! G7-editor E05: trim and extend against curved boundaries, and extending a Bézier end. Oracles
//! are closed forms: the cubic below is `y = x² / 10` (`x(t) = 10 t`, `y(t) = 10 t²`), so every
//! point of the result can be checked against that equation, and boundaries are lines and circles.

#![allow(clippy::unwrap_used)] // test helpers

use burnburn_geometry::{EndpointRef, ExtendError, Path, PathBuilder, Point};

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn near(a: Point, b: Point) -> bool {
    a.distance(b) < 1e-6
}

/// A quadratic raised to degree three: the whole polynomial is known.
fn parabola() -> Path {
    PathBuilder::new()
        .move_to(p(0.0, 0.0))
        .cubic_to(p(10.0 / 3.0, 0.0), p(20.0 / 3.0, 10.0 / 3.0), p(10.0, 10.0))
        .build()
        .unwrap()
}

fn on_parabola(pt: Point) -> bool {
    (pt.y - pt.x * pt.x / 10.0).abs() < 1e-6
}

#[test]
fn a_line_extends_to_a_circle_and_stops_on_it() {
    let line = Path::line(p(0.0, 0.0), p(4.0, 0.0)).unwrap();
    let ring = Path::circle(p(10.0, 0.0), 3.0).unwrap();
    let out = line.extend_end(EndpointRef::End(0), &[&ring]).unwrap();
    assert!(near(out.subpaths()[0].end(), p(7.0, 0.0)), "{:?}", out.subpaths()[0].end());
}

#[test]
fn a_line_is_trimmed_between_the_crossings_of_a_circle() {
    let line = Path::line(p(0.0, 0.0), p(20.0, 0.0)).unwrap();
    let ring = Path::circle(p(10.0, 0.0), 3.0).unwrap();
    let out = line.trim_at(p(10.0, 0.0), &[&ring]).unwrap();
    assert_eq!(out.subpaths().len(), 2);
    assert!(near(out.subpaths()[0].end(), p(7.0, 0.0)) && near(out.subpaths()[1].start(), p(13.0, 0.0)));
}

#[test]
fn a_bezier_is_trimmed_at_a_curved_cutter() {
    // The circle crosses the parabola twice (x about 4.2 and 7.5). Picking before the first
    // crossing removes the piece from the start up to it: the new start lies on both curves.
    let ring = Path::circle(p(10.0, 0.0), 6.0).unwrap();
    let out = parabola().trim_at(p(2.0, 0.4), &[&ring]).unwrap();
    assert_eq!(out.subpaths().len(), 1);
    let start = out.subpaths()[0].start();
    assert!(on_parabola(start), "{start:?}");
    assert!((start.distance(p(10.0, 0.0)) - 6.0).abs() < 1e-6, "{start:?}");
    assert!(start.x > 4.0 && start.x < 4.5, "the first crossing, not the second: {start:?}");
    assert!(near(out.subpaths()[0].end(), p(10.0, 10.0)));
}

#[test]
fn a_bezier_end_extends_along_the_same_cubic_to_a_wall() {
    let wall = Path::line(p(15.0, -5.0), p(15.0, 40.0)).unwrap();
    let out = parabola().extend_end(EndpointRef::End(0), &[&wall]).unwrap();
    let sp = &out.subpaths()[0];
    assert_eq!(sp.segments().len(), 1, "still one cubic");
    assert!(near(sp.start(), p(0.0, 0.0)));
    assert!(near(sp.end(), p(15.0, 22.5)), "{:?}", sp.end());
    for i in 0..=20 {
        let pt = sp.segments()[0].point_at(sp.start(), f64::from(i) / 20.0);
        assert!(on_parabola(pt), "{pt:?}");
    }
}

#[test]
fn a_bezier_start_extends_backwards_and_a_curved_boundary_works() {
    let wall = Path::line(p(-5.0, -5.0), p(-5.0, 40.0)).unwrap();
    let out = parabola().extend_end(EndpointRef::Start(0), &[&wall]).unwrap();
    let sp = &out.subpaths()[0];
    assert!(near(sp.start(), p(-5.0, 2.5)), "{:?}", sp.start());
    assert!(near(sp.end(), p(10.0, 10.0)));

    let ring = Path::circle(p(15.0, 20.0), 5.0).unwrap();
    let out = parabola().extend_end(EndpointRef::End(0), &[&ring]).unwrap();
    let end = out.subpaths()[0].end();
    assert!(on_parabola(end) && (end.distance(p(15.0, 20.0)) - 5.0).abs() < 1e-6, "{end:?}");
}

#[test]
fn a_bezier_with_nothing_ahead_is_refused() {
    let behind = Path::line(p(-5.0, -5.0), p(-5.0, 40.0)).unwrap();
    assert_eq!(parabola().extend_end(EndpointRef::End(0), &[&behind]), Err(ExtendError::NoBoundaryAhead));
}
