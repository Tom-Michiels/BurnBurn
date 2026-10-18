//! W1-L2a Cut Shapes. Oracles: exact areas of axis-aligned overlaps, piece counts of bands and
//! holes, and lengths of a line cut by a square.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers and let-else in tests

use burnburn_geometry::{
    BooleanOptions, CutError, CutResult, CutSide, FillRule, Path, PathBuilder, Point, ProductionTolerance, Rect,
    cut_shapes,
};
use burnburn_types::{CancellationSource, CancellationToken, Length};
use proptest::prelude::*;

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Path {
    Path::rectangle(Rect::new(p(x, y), Length::from_mm(w).unwrap(), Length::from_mm(h).unwrap()).unwrap())
}

fn options() -> BooleanOptions {
    BooleanOptions {
        fill: FillRule::EvenOdd,
        tolerance: ProductionTolerance::new(Length::from_mm(0.001).unwrap()).unwrap(),
    }
}

fn cut(subject: &Path, cutter: &Path) -> CutResult {
    cut_shapes(&[subject], cutter, &options(), &CancellationToken::never()).unwrap().remove(0)
}

fn area(r: &CutResult, side: CutSide) -> f64 {
    r.pieces.iter().filter(|c| c.side == side).map(|c| c.path.signed_area()).sum()
}

fn count(r: &CutResult, side: CutSide) -> usize {
    r.pieces.iter().filter(|c| c.side == side).count()
}

/// Total length of a polyline path (lines only).
fn length(path: &Path) -> f64 {
    path.subpaths()
        .iter()
        .map(|sp| {
            let mut at = sp.start();
            sp.segments()
                .iter()
                .map(|s| {
                    let d = at.distance(s.end());
                    at = s.end();
                    d
                })
                .sum::<f64>()
        })
        .sum()
}

#[test]
fn a_rectangle_cutter_splits_a_square_into_inside_and_outside_halves() {
    let r = cut(&rect(0.0, 0.0, 10.0, 10.0), &rect(5.0, -5.0, 10.0, 20.0));
    assert!(r.changed);
    assert_eq!((count(&r, CutSide::Inside), count(&r, CutSide::Outside)), (1, 1));
    assert!((area(&r, CutSide::Inside) - 50.0).abs() < 1e-6);
    assert!((area(&r, CutSide::Outside) - 50.0).abs() < 1e-6);
    assert!(r.pieces.iter().all(|c| c.path.subpaths().iter().all(|s| s.is_closed())));
}

#[test]
fn a_cutter_edge_on_the_subject_edge_still_splits_exactly() {
    let r = cut(&rect(0.0, 0.0, 10.0, 10.0), &rect(5.0, 0.0, 5.0, 10.0));
    assert!(r.changed);
    assert!((area(&r, CutSide::Inside) - 50.0).abs() < 1e-6 && (area(&r, CutSide::Outside) - 50.0).abs() < 1e-6);
}

#[test]
fn a_band_through_the_middle_leaves_two_separate_outside_islands() {
    let r = cut(&rect(0.0, 0.0, 10.0, 10.0), &rect(-1.0, 4.0, 12.0, 2.0));
    assert_eq!((count(&r, CutSide::Inside), count(&r, CutSide::Outside)), (1, 2));
    assert!((area(&r, CutSide::Inside) - 20.0).abs() < 1e-6);
    for c in r.pieces.iter().filter(|c| c.side == CutSide::Outside) {
        assert!((c.path.signed_area() - 40.0).abs() < 1e-6);
        assert_eq!(c.path.subpaths().len(), 1);
    }
}

#[test]
fn a_cutter_inside_the_subject_cuts_a_disc_out_and_leaves_a_hole() {
    let disc = Path::circle(p(5.0, 5.0), 3.0).unwrap();
    let r = cut(&rect(0.0, 0.0, 10.0, 10.0), &disc);
    assert_eq!((count(&r, CutSide::Inside), count(&r, CutSide::Outside)), (1, 1));
    let outside = r.pieces.iter().find(|c| c.side == CutSide::Outside).unwrap();
    assert_eq!(outside.path.subpaths().len(), 2, "the square keeps the disc as a hole");
    let (inside, rest) = (area(&r, CutSide::Inside), area(&r, CutSide::Outside));
    assert!((inside + rest - 100.0).abs() < 1e-6, "no area is lost or duplicated: {inside} + {rest}");
    // A polygon inscribed with chord error 0.001 mm loses at most perimeter × 0.001 of the disc area.
    let budget = 2.0 * std::f64::consts::PI * 3.0 * 0.001;
    assert!((inside - 9.0 * std::f64::consts::PI).abs() < budget, "flattened disc within chord error");
}

#[test]
fn a_hole_stays_with_the_piece_that_contains_it() {
    let plate = Path::from_subpaths(vec![
        rect(0.0, 0.0, 20.0, 10.0).subpaths()[0].clone(),
        Path::polyline(&[p(2.0, 2.0), p(2.0, 8.0), p(6.0, 8.0), p(6.0, 2.0)], true).unwrap().subpaths()[0].clone(),
    ]);
    let r = cut(&plate, &rect(10.0, -1.0, 20.0, 12.0));
    assert!((area(&r, CutSide::Outside) - (100.0 - 24.0)).abs() < 1e-6, "left half keeps its hole");
    assert!((area(&r, CutSide::Inside) - 100.0).abs() < 1e-6);
    let left = r.pieces.iter().find(|c| c.side == CutSide::Outside).unwrap();
    assert_eq!(left.path.subpaths().len(), 2);
}

#[test]
fn shapes_that_the_cutter_does_not_cross_are_returned_untouched() {
    let far = rect(100.0, 100.0, 5.0, 5.0);
    let r = cut(&far, &rect(0.0, 0.0, 10.0, 10.0));
    assert!(!r.changed);
    assert_eq!(r.pieces, vec![burnburn_geometry::CutPiece { path: far, side: CutSide::Outside }]);
    let inner = rect(2.0, 2.0, 3.0, 3.0);
    let r = cut(&inner, &rect(0.0, 0.0, 10.0, 10.0));
    assert!(!r.changed);
    assert_eq!(r.pieces[0].side, CutSide::Inside);
    assert_eq!(r.pieces[0].path, inner, "the original path, not a flattened copy");
}

#[test]
fn an_open_line_is_cut_into_outside_inside_outside_pieces() {
    let line = Path::line(p(-5.0, 5.0), p(15.0, 5.0)).unwrap();
    let r = cut(&line, &rect(0.0, 0.0, 10.0, 10.0));
    assert!(r.changed);
    let sides: Vec<CutSide> = r.pieces.iter().map(|c| c.side).collect();
    assert_eq!(sides, vec![CutSide::Outside, CutSide::Inside, CutSide::Outside]);
    let lengths: Vec<f64> = r.pieces.iter().map(|c| length(&c.path)).collect();
    for (got, want) in lengths.iter().zip([5.0, 10.0, 5.0]) {
        assert!((got - want).abs() < 1e-9, "{lengths:?}");
    }
    assert!(r.pieces.iter().all(|c| c.path.subpaths().iter().all(|s| !s.is_closed())));
}

#[test]
fn a_curve_is_split_exactly_not_flattened() {
    let arc =
        PathBuilder::new().move_to(p(-2.0, 0.0)).cubic_to(p(0.0, 8.0), p(8.0, 8.0), p(12.0, 0.0)).build().unwrap();
    let r = cut(&arc, &rect(0.0, -1.0, 10.0, 10.0));
    assert_eq!(r.pieces.len(), 3);
    assert!(r.pieces.iter().all(|c| {
        c.path.subpaths()[0].segments().iter().all(|s| matches!(s, burnburn_geometry::Segment::Cubic { .. }))
    }));
    // The pieces join up: each ends where the next starts.
    for w in r.pieces.windows(2) {
        assert!(w[0].path.subpaths()[0].end().distance(w[1].path.subpaths()[0].start()) < 1e-9);
    }
    let first = &r.pieces[0].path.subpaths()[0];
    assert_eq!(first.start(), p(-2.0, 0.0));
    assert!((first.end().x).abs() < 1e-6, "the first cut is on the cutter's left edge x = 0");
}

#[test]
fn a_line_that_only_touches_the_cutter_is_not_cut() {
    // x + y = 0 passes through the cutter's corner (0,0) without entering it.
    let line = Path::line(p(-5.0, 5.0), p(5.0, -5.0)).unwrap();
    let r = cut(&line, &rect(0.0, 0.0, 10.0, 10.0));
    assert!(!r.changed);
    assert_eq!(r.pieces.len(), 1);
    // Ending exactly on the cutter's edge does not split either.
    let ends = Path::line(p(-5.0, 5.0), p(0.0, 5.0)).unwrap();
    assert!(!cut(&ends, &rect(0.0, 0.0, 10.0, 10.0)).changed);
}

#[test]
fn several_subjects_are_cut_independently_and_in_order() {
    let a = rect(0.0, 0.0, 10.0, 10.0);
    let b = rect(100.0, 0.0, 10.0, 10.0);
    let out = cut_shapes(&[&a, &b], &rect(5.0, -5.0, 10.0, 20.0), &options(), &CancellationToken::never()).unwrap();
    assert_eq!((out[0].changed, out[1].changed), (true, false));
}

#[test]
fn an_open_cutter_and_a_cancelled_token_are_typed_errors() {
    let subject = rect(0.0, 0.0, 10.0, 10.0);
    let open = Path::line(p(0.0, 0.0), p(10.0, 10.0)).unwrap();
    assert_eq!(cut_shapes(&[&subject], &open, &options(), &CancellationToken::never()), Err(CutError::OpenCutter));
    let source = CancellationSource::new();
    source.cancel();
    assert_eq!(
        cut_shapes(&[&subject], &rect(5.0, 0.0, 10.0, 10.0), &options(), &source.token()),
        Err(CutError::Cancelled)
    );
}

proptest! {
    /// Inside + outside partition the subject; the inside part is the overlap of the rectangles.
    #[test]
    fn rectangle_cuts_partition_the_area(
        (sx, sy, sw, sh) in (-20.0f64..20.0, -20.0f64..20.0, 1.0f64..30.0, 1.0f64..30.0),
        (cx, cy, cw, ch) in (-20.0f64..20.0, -20.0f64..20.0, 1.0f64..30.0, 1.0f64..30.0),
    ) {
        let (subject, cutter) = (rect(sx, sy, sw, sh), rect(cx, cy, cw, ch));
        let r = cut(&subject, &cutter);
        let ox = ((sx + sw).min(cx + cw) - sx.max(cx)).max(0.0);
        let oy = ((sy + sh).min(cy + ch) - sy.max(cy)).max(0.0);
        prop_assert!((area(&r, CutSide::Inside) - ox * oy).abs() < 1e-3, "inside {} vs {}", area(&r, CutSide::Inside), ox * oy);
        prop_assert!((area(&r, CutSide::Inside) + area(&r, CutSide::Outside) - sw * sh).abs() < 1e-3);
    }
}
