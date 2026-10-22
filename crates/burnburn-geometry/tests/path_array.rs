//! Copies along a path: analytic positions and orientations.

use burnburn_geometry::array::{ArrayError, MAX_COPIES, PathArray, path_array};
use burnburn_geometry::{Path, Point, ProductionTolerance};
use burnburn_types::Length;

fn tol() -> ProductionTolerance {
    ProductionTolerance::new(Length::from_mm(0.001).unwrap()).unwrap()
}

fn line(points: &[(f64, f64)]) -> Path {
    Path::polyline(&points.iter().map(|&(x, y)| Point::new(x, y)).collect::<Vec<_>>(), false).unwrap()
}

fn close(a: Point, b: Point) -> bool {
    a.distance(b) < 1e-9
}

#[test]
fn copies_are_spaced_evenly_along_a_straight_open_path_with_both_ends() {
    let along = line(&[(0.0, 0.0), (90.0, 0.0)]);
    let ts = path_array(&along, &PathArray { count: 4, orient: false }, &tol()).unwrap();
    assert_eq!(ts.len(), 4);
    for (k, t) in ts.iter().enumerate() {
        let moved = t.apply(Point::new(5.0, 7.0));
        assert!(close(moved, Point::new(5.0 + 30.0 * k as f64, 7.0)), "copy {k}: {moved:?}");
    }
}

#[test]
fn a_single_copy_is_the_source_itself() {
    let along = line(&[(3.0, 4.0), (30.0, 4.0)]);
    let ts = path_array(&along, &PathArray { count: 1, orient: true }, &tol()).unwrap();
    assert_eq!(ts.len(), 1);
    assert!(close(ts[0].apply(Point::new(1.0, 2.0)), Point::new(1.0, 2.0)));
}

#[test]
fn a_path_not_starting_at_the_origin_moves_the_source_by_the_distance_from_the_start() {
    let along = line(&[(100.0, 50.0), (100.0, 80.0)]);
    let ts = path_array(&along, &PathArray { count: 3, orient: false }, &tol()).unwrap();
    // The source is not rotated and moves by the displacement along the path: (0, 15) and (0, 30).
    assert!(close(ts[1].apply(Point::new(100.0, 50.0)), Point::new(100.0, 65.0)));
    assert!(close(ts[2].apply(Point::new(100.0, 50.0)), Point::new(100.0, 80.0)));
}

#[test]
fn oriented_copies_turn_with_the_direction_of_the_path() {
    // Right for 10 mm, then up for 10 mm: samples at 0, 10 and 20 mm of arc length.
    let along = line(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)]);
    let ts = path_array(&along, &PathArray { count: 3, orient: true }, &tol()).unwrap();
    // The source point one unit ahead of the path start.
    let ahead = Point::new(1.0, 0.0);
    assert!(close(ts[0].apply(ahead), ahead));
    // At the corner the path turns up: the copy keeps heading "ahead" along the new segment.
    assert!(close(ts[1].apply(ahead), Point::new(10.0, 1.0)), "{:?}", ts[1].apply(ahead));
    // At the end of the path the direction is that of the last segment.
    assert!(close(ts[2].apply(ahead), Point::new(10.0, 11.0)), "{:?}", ts[2].apply(ahead));
    // Without orientation the copies keep their heading.
    let plain = path_array(&along, &PathArray { count: 3, orient: false }, &tol()).unwrap();
    assert!(close(plain[2].apply(ahead), Point::new(11.0, 10.0)));
}

#[test]
fn a_closed_path_divides_its_length_by_the_count_and_never_repeats_the_start() {
    // A square of 40 mm perimeter: 4 copies sit on the corners, the fifth would be the start again.
    let square = Path::polyline(
        &[Point::new(0.0, 0.0), Point::new(10.0, 0.0), Point::new(10.0, 10.0), Point::new(0.0, 10.0)],
        true,
    )
    .unwrap();
    let ts = path_array(&square, &PathArray { count: 4, orient: false }, &tol()).unwrap();
    let at: Vec<Point> = ts.iter().map(|t| t.apply(Point::new(0.0, 0.0))).collect();
    let want = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
    for (p, w) in at.iter().zip(want) {
        assert!(close(*p, Point::new(w.0, w.1)), "{p:?} vs {w:?}");
    }
}

#[test]
fn copies_on_a_circle_sit_on_it_at_equal_angles() {
    let circle = Path::circle(Point::new(50.0, 50.0), 20.0).unwrap();
    let n = 8;
    let ts = path_array(&circle, &PathArray { count: n, orient: false }, &tol()).unwrap();
    // The source sits at the path start; every copy lands on the circle, 360/n degrees apart.
    let start = ts[0].apply(Point::new(70.0, 50.0));
    let centre = Point::new(50.0, 50.0);
    let a0 = (start.y - centre.y).atan2(start.x - centre.x);
    for (k, t) in ts.iter().enumerate() {
        let p = t.apply(Point::new(70.0, 50.0));
        assert!((p.distance(centre) - 20.0).abs() < 0.01, "copy {k} is {} from the centre", p.distance(centre));
        let da = ((p.y - centre.y).atan2(p.x - centre.x) - a0).rem_euclid(std::f64::consts::TAU);
        let want = std::f64::consts::TAU * k as f64 / f64::from(n);
        assert!((da - want).abs() < 1e-3, "copy {k}: {da} vs {want}");
    }
}

#[test]
fn counts_and_paths_that_cannot_work_are_refused() {
    let along = line(&[(0.0, 0.0), (10.0, 0.0)]);
    assert_eq!(
        path_array(&along, &PathArray { count: 0, orient: false }, &tol()),
        Err(ArrayError::OutOfRange { field: "count" })
    );
    assert_eq!(
        path_array(&along, &PathArray { count: (MAX_COPIES + 1) as u32, orient: false }, &tol()),
        Err(ArrayError::OutOfRange { field: "count" })
    );
    assert_eq!(
        path_array(&Path::default(), &PathArray { count: 3, orient: false }, &tol()),
        Err(ArrayError::OutOfRange { field: "path" })
    );
    let dot = line(&[(5.0, 5.0), (5.0, 5.0)]);
    assert_eq!(
        path_array(&dot, &PathArray { count: 3, orient: false }, &tol()),
        Err(ArrayError::OutOfRange { field: "path" })
    );
}
