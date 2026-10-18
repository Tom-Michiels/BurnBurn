//! W1-L2a Close/Join selected shapes: nearby ends fuse within an explicit tolerance, nothing else does.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers and let-else in tests

use burnburn_geometry::{DiagnosticTolerance, JoinError, Path, Point, join_shapes};
use burnburn_types::{CancellationSource, CancellationToken, Length};
use proptest::prelude::*;

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn tol(mm: f64) -> DiagnosticTolerance {
    DiagnosticTolerance::new(Length::from_mm(mm).unwrap()).unwrap()
}

fn line(a: Point, b: Point) -> Path {
    Path::line(a, b).unwrap()
}

fn join(paths: &[&Path], t: f64) -> burnburn_geometry::JoinOutcome {
    join_shapes(paths, &tol(t), &CancellationToken::never()).unwrap()
}

#[test]
fn two_lines_with_a_small_gap_become_one_polyline() {
    let (a, b) = (line(p(0.0, 0.0), p(10.0, 0.0)), line(p(10.05, 0.0), p(10.0, 10.0)));
    let out = join(&[&a, &b], 0.1);
    assert_eq!((out.merged, out.closed), (1, 0));
    assert_eq!(out.path.subpaths().len(), 1);
    let sp = &out.path.subpaths()[0];
    assert_eq!((sp.start(), sp.end(), sp.segments().len()), (p(0.0, 0.0), p(10.0, 10.0), 2));
    assert!(!sp.is_closed());
    assert_eq!(sp.segments()[0], burnburn_geometry::Segment::Line { to: p(10.0, 0.0) }, "snapped, no bridging line");
}

#[test]
fn gaps_beyond_the_tolerance_stay_open() {
    let (a, b) = (line(p(0.0, 0.0), p(10.0, 0.0)), line(p(10.5, 0.0), p(10.0, 10.0)));
    let out = join(&[&a, &b], 0.1);
    assert_eq!((out.merged, out.closed, out.path.subpaths().len()), (0, 0, 2));
    assert_eq!(out.path.subpaths()[0], a.subpaths()[0]);
    assert_eq!(out.path.subpaths()[1], b.subpaths()[0]);
}

#[test]
fn four_loose_sides_close_into_one_contour_whatever_their_direction() {
    // Sides drawn in mixed directions with 0.01 mm gaps.
    let sides = [
        line(p(0.0, 0.0), p(10.0, 0.0)),
        line(p(10.01, 10.0), p(10.0, 0.01)), // reversed
        line(p(10.0, 10.0), p(0.0, 10.0)),
        line(p(0.0, 0.01), p(0.01, 10.0)), // reversed
    ];
    let refs: Vec<&Path> = sides.iter().collect();
    let out = join(&refs, 0.05);
    assert_eq!(out.path.subpaths().len(), 1);
    assert!(out.path.subpaths()[0].is_closed());
    assert_eq!((out.merged, out.closed), (3, 1));
    assert!((out.path.signed_area().abs() - 100.0).abs() < 0.5, "{}", out.path.signed_area());
}

#[test]
fn only_the_nearest_ends_fuse_at_a_three_way_meeting_point() {
    // Three ends within the tolerance of each other: the closest pair joins, the third stays loose.
    let a = line(p(0.0, 0.0), p(10.0, 0.0));
    let b = line(p(10.02, 0.0), p(20.0, 0.0));
    let c = line(p(10.05, 0.0), p(10.05, 10.0));
    let out = join(&[&a, &b, &c], 0.1);
    assert_eq!(out.path.subpaths().len(), 2);
    assert_eq!(out.merged, 1);
}

#[test]
fn closed_shapes_and_far_apart_designs_are_not_connected() {
    let sq = Path::polyline(&[p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)], true).unwrap();
    let far = line(p(50.0, 0.0), p(60.0, 0.0));
    let out = join(&[&sq, &far], 1.0);
    assert_eq!(out.path.subpaths().len(), 2);
    assert_eq!((out.merged, out.closed), (0, 0));
    assert_eq!(out.path.subpaths()[0], sq.subpaths()[0]);
}

#[test]
fn a_single_open_shape_whose_ends_meet_is_closed() {
    let almost = Path::polyline(&[p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 0.02)], false).unwrap();
    let out = join(&[&almost], 0.05);
    assert_eq!((out.merged, out.closed), (0, 1));
    assert!(out.path.subpaths()[0].is_closed());
}

#[test]
fn nothing_to_join_returns_the_input_unchanged() {
    let a = line(p(0.0, 0.0), p(10.0, 0.0));
    let out = join(&[&a], 0.1);
    assert_eq!(out.path, a);
    let empty = join(&[], 0.1);
    assert!(empty.path.is_empty());
}

#[test]
fn a_cancelled_token_stops_the_join() {
    let source = CancellationSource::new();
    source.cancel();
    let a = line(p(0.0, 0.0), p(10.0, 0.0));
    assert_eq!(join_shapes(&[&a], &tol(0.1), &source.token()), Err(JoinError::Cancelled));
}

proptest! {
    /// A chain of segments, shuffled and flipped, with jitter below half the tolerance, rejoins into one piece.
    #[test]
    fn a_shuffled_and_flipped_chain_rejoins_into_one_polyline(
        steps in proptest::collection::vec((3.0f64..20.0, -10.0f64..10.0), 2..8),
        flips in proptest::collection::vec(any::<bool>(), 8),
        order in proptest::collection::vec(any::<u32>(), 8),
        jitter in proptest::collection::vec(-0.03f64..0.03, 16),
    ) {
        let mut pts = vec![p(0.0, 0.0)];
        for (dx, dy) in &steps {
            let last = *pts.last().unwrap();
            pts.push(p(last.x + dx, last.y + dy));
        }
        let mut pieces: Vec<(u32, Path)> = (0..pts.len() - 1)
            .map(|i| {
                let (a, b) = (pts[i], pts[i + 1]);
                let (a, b) = (p(a.x + jitter[2 * i], a.y), p(b.x, b.y + jitter[2 * i + 1]));
                (order[i], if flips[i] { line(b, a) } else { line(a, b) })
            })
            .collect();
        pieces.sort_by_key(|(k, _)| *k);
        let refs: Vec<&Path> = pieces.iter().map(|(_, q)| q).collect();
        let out = join(&refs, 0.1);
        prop_assert_eq!(out.path.subpaths().len(), 1);
        prop_assert_eq!(out.merged, pieces.len() - 1);
    }
}
