//! W2-L1-bewerken: containment and coincidence of outlines (auto-group, select contained,
//! delete duplicates).

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use burnburn_geometry::{Path, Point, Rect, Subpath, Vector, coincident, encloses};
use burnburn_types::Length;

fn rect(x: f64, y: f64, w: f64, h: f64) -> Path {
    Path::rectangle(Rect::new(Point::new(x, y), Length::from_mm(w).unwrap(), Length::from_mm(h).unwrap()).unwrap())
}

fn shifted(p: &Path, dx: f64) -> Path {
    p.transform(&burnburn_geometry::Affine::translate(Vector::new(dx, 0.0)).unwrap())
}

#[test]
fn an_outline_inside_another_is_enclosed_but_not_the_other_way_round() {
    let outer = rect(0.0, 0.0, 20.0, 20.0);
    let inner = rect(5.0, 5.0, 4.0, 4.0);
    assert!(encloses(&outer, &inner, 1e-6));
    assert!(!encloses(&inner, &outer, 1e-6));
}

#[test]
fn touching_overlapping_and_separate_outlines_are_not_enclosed() {
    let outer = rect(0.0, 0.0, 20.0, 20.0);
    assert!(!encloses(&outer, &rect(0.0, 5.0, 4.0, 4.0), 1e-6), "sharing an edge is not inside");
    assert!(!encloses(&outer, &rect(15.0, 5.0, 10.0, 4.0), 1e-6), "overlapping is not inside");
    assert!(!encloses(&outer, &rect(30.0, 0.0, 4.0, 4.0), 1e-6));
    assert!(!encloses(&outer, &outer, 1e-6), "an outline does not enclose itself");
}

#[test]
fn a_hole_of_a_compound_outline_is_not_inside_the_material() {
    // A frame: outer square with a square hole as second subpath (even-odd).
    let sub = |x: f64, y: f64, s: f64| -> Subpath { rect(x, y, s, s).subpaths()[0].clone() };
    let frame = Path::from_subpaths(vec![sub(0.0, 0.0, 20.0), sub(5.0, 5.0, 10.0)]);
    assert!(!encloses(&frame, &rect(8.0, 8.0, 2.0, 2.0), 1e-6), "inside the hole means outside the material");
    assert!(encloses(&frame, &rect(1.0, 1.0, 2.0, 2.0), 1e-6));
}

#[test]
fn open_paths_enclose_nothing() {
    let open = Path::line(Point::new(0.0, 0.0), Point::new(10.0, 0.0)).unwrap();
    assert!(!encloses(&open, &rect(1.0, -1.0, 1.0, 1.0), 1e-6));
    assert!(!encloses(&rect(0.0, 0.0, 20.0, 20.0), &Path::default(), 1e-6));
}

#[test]
fn coincident_outlines_are_the_same_drawing_within_the_tolerance() {
    let a = rect(0.0, 0.0, 10.0, 5.0);
    assert!(coincident(&a, &a.clone(), 1e-9));
    assert!(coincident(&a, &shifted(&a, 0.0004), 0.001));
    assert!(!coincident(&a, &shifted(&a, 0.01), 0.001));
    assert!(!coincident(&a, &rect(0.0, 0.0, 10.0, 6.0), 0.001));
    assert!(!coincident(&a, &Path::default(), 0.001));
    assert!(coincident(&Path::default(), &Path::default(), 0.001), "two empty paths are trivially the same");
}

#[test]
fn a_line_and_a_closed_outline_with_equal_endpoints_are_not_coincident() {
    let line = Path::line(Point::new(0.0, 0.0), Point::new(10.0, 0.0)).unwrap();
    let flat = Path::from_subpaths(vec![
        Subpath::new(Point::new(0.0, 0.0), vec![burnburn_geometry::Segment::Line { to: Point::new(10.0, 0.0) }], true)
            .unwrap(),
    ]);
    assert!(!coincident(&line, &flat, 1e-6));
}
