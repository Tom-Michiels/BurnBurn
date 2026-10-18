//! W1-L2a Round Corners. Oracles: closed-form fillet areas (per corner `r²(1/tan(θ/2) − (π−θ)/2)`
//! is removed), arc centres and tangent points derived by hand, and shared-segment limits.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers and let-else in tests

use std::f64::consts::PI;

use burnburn_geometry::{
    NodeEdit, NodeEditError, NodeRef, Path, PathBuilder, Point, Rect, RoundError, Segment, Winding,
};
use burnburn_types::{Angle, Length};
use proptest::prelude::*;

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn node(subpath: usize, node: usize) -> NodeRef {
    NodeRef { subpath, node }
}

fn rect(w: f64, h: f64) -> Path {
    Path::rectangle(Rect::new(p(0.0, 0.0), Length::from_mm(w).unwrap(), Length::from_mm(h).unwrap()).unwrap())
}

fn arcs(path: &Path) -> Vec<(Point, f64)> {
    path.subpaths()
        .iter()
        .flat_map(|sp| sp.segments())
        .filter_map(|s| match s {
            Segment::Arc(a) => Some((a.center(), a.semi_diameters().0.length())),
            _ => None,
        })
        .collect()
}

fn near(a: Point, b: Point) -> bool {
    a.distance(b) < 1e-9
}

/// Every segment must start where the previous one ended (within float noise).
fn assert_continuous(path: &Path) {
    for sp in path.subpaths() {
        let mut at = sp.start();
        for seg in sp.segments() {
            if let Segment::Arc(a) = seg {
                assert!(near(a.start_point(), at), "arc starts at {:?}, previous ends at {at:?}", a.start_point());
            }
            at = seg.end();
        }
        if sp.is_closed() {
            assert!(at.distance(sp.start()) < 1e-9 || sp.segments().len() > 1);
        }
    }
}

#[test]
fn rounding_every_corner_of_a_square_removes_the_closed_form_area() {
    let out = rect(10.0, 10.0).round_corners(2.0, None).unwrap();
    assert_eq!((out.rounded, out.limited, out.skipped), (4, 0, 0));
    let expected = 100.0 - 4.0 * 2.0 * 2.0 * (1.0 - PI / 4.0);
    assert!((out.path.signed_area() - expected).abs() < 1e-9, "{} vs {expected}", out.path.signed_area());
    assert_eq!(out.path.bounds(), rect(10.0, 10.0).bounds(), "the straight sides keep the extent");
    let mut centres = arcs(&out.path);
    centres.sort_by(|a, b| a.0.x.total_cmp(&b.0.x).then(a.0.y.total_cmp(&b.0.y)));
    let want = [p(2.0, 2.0), p(2.0, 8.0), p(8.0, 2.0), p(8.0, 8.0)];
    assert_eq!(centres.len(), 4);
    for ((c, r), w) in centres.iter().zip(want) {
        assert!(near(*c, w) && (r - 2.0).abs() < 1e-12, "{c:?} r={r}");
    }
    assert_continuous(&out.path);
}

#[test]
fn an_equilateral_triangle_loses_the_sixty_degree_fillet_area() {
    let tri = Path::polyline(&[p(0.0, 0.0), p(10.0, 0.0), p(5.0, 75f64.sqrt())], true).unwrap();
    let out = tri.round_corners(1.0, None).unwrap();
    // θ = 60°: 1/tan(30°) − (π − π/3)/2 = √3 − π/3 per corner.
    let expected = 25.0 * 3f64.sqrt() - 3.0 * (3f64.sqrt() - PI / 3.0);
    assert!((out.path.signed_area() - expected).abs() < 1e-9, "{} vs {expected}", out.path.signed_area());
    assert_eq!(out.rounded, 3);
}

#[test]
fn short_sides_limit_the_radius_and_are_consumed_completely() {
    // 10 × 2 with r = 5: the 2 mm sides can give at most 1 mm to each of their two corners.
    let out = rect(10.0, 2.0).round_corners(5.0, None).unwrap();
    assert_eq!((out.rounded, out.limited), (4, 4));
    let expected = 20.0 - (4.0 - PI) * 1.0;
    assert!((out.path.signed_area() - expected).abs() < 1e-9);
    assert!(arcs(&out.path).iter().all(|(_, r)| (r - 1.0).abs() < 1e-9), "{:?}", arcs(&out.path));
    // 10 × 4 with r = 3: d = 3 would need 6 mm of the 4 mm side → both ends get 2 mm, r = 2.
    let out = rect(10.0, 4.0).round_corners(3.0, None).unwrap();
    assert!(arcs(&out.path).iter().all(|(_, r)| (r - 2.0).abs() < 1e-9));
    assert!(out.path.subpaths()[0].segments().iter().all(|s| s.end().x.is_finite()));
}

#[test]
fn a_tiny_contour_becomes_a_circle_of_half_its_side() {
    let out = rect(0.001, 0.001).round_corners(5.0, None).unwrap();
    assert!((out.path.signed_area() - PI * 0.0005 * 0.0005).abs() < 1e-12);
}

#[test]
fn only_the_requested_nodes_are_rounded() {
    let out = rect(10.0, 10.0).round_corners(2.0, Some(&[node(0, 2)])).unwrap();
    assert_eq!(out.rounded, 1);
    assert_eq!(arcs(&out.path), vec![(p(8.0, 8.0), 2.0)]);
    let expected = 100.0 - 4.0 * (1.0 - PI / 4.0);
    assert!((out.path.signed_area() - expected).abs() < 1e-9);
    // Node `len` of a closed subpath is node 0 and rounds the start corner; the start moves onto the arc.
    let start = rect(10.0, 10.0).round_corners(2.0, Some(&[node(0, 3)])).unwrap();
    assert_eq!(arcs(&start.path).len(), 1);
}

#[test]
fn open_ends_collinear_nodes_and_curves_are_skipped_and_counted() {
    let open = PathBuilder::new().move_to(p(0.0, 0.0)).line_to(p(10.0, 0.0)).line_to(p(10.0, 10.0)).build().unwrap();
    let out = open.round_corners(3.0, None).unwrap();
    assert_eq!((out.rounded, out.skipped), (1, 0), "only the interior node of an open path is a corner");
    let sp = &out.path.subpaths()[0];
    assert_eq!((sp.start(), sp.end()), (p(0.0, 0.0), p(10.0, 10.0)));
    assert!(!sp.is_closed());
    // Requesting the end nodes explicitly: skipped, not an error, as long as something was rounded.
    let both = open.round_corners(3.0, Some(&[node(0, 0), node(0, 1), node(0, 2)])).unwrap();
    assert_eq!((both.rounded, both.skipped), (1, 2));

    let straight = Path::polyline(&[p(0.0, 0.0), p(5.0, 0.0), p(10.0, 0.0), p(10.0, 10.0)], false).unwrap();
    let out = straight.round_corners(1.0, None).unwrap();
    assert_eq!(out.rounded, 1, "the collinear node is not a corner");

    let curved = PathBuilder::new()
        .move_to(p(0.0, 0.0))
        .cubic_to(p(3.0, 3.0), p(6.0, 3.0), p(10.0, 0.0))
        .line_to(p(10.0, -10.0))
        .build()
        .unwrap();
    assert_eq!(curved.round_corners(1.0, None), Err(RoundError::NothingToRound));
}

#[test]
fn invalid_radius_and_cornerless_paths_are_typed_errors() {
    let sq = rect(10.0, 10.0);
    for r in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert_eq!(sq.round_corners(r, None), Err(RoundError::InvalidRadius), "r = {r}");
    }
    assert_eq!(Path::circle(p(0.0, 0.0), 5.0).unwrap().round_corners(1.0, None), Err(RoundError::NothingToRound));
    assert_eq!(Path::default().round_corners(1.0, None), Err(RoundError::NothingToRound));
}

#[test]
fn coincident_points_and_clockwise_input_stay_finite_and_keep_their_direction() {
    let dup = Path::polyline(&[p(0.0, 0.0), p(10.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)], true).unwrap();
    let out = dup.round_corners(2.0, None).unwrap();
    assert!(out.path.signed_area().is_finite() && out.path.signed_area() > 90.0);
    assert!(out.rounded >= 3, "the corners next to the doubled vertex are skipped, the others rounded");
    let cw = Path::polyline(&[p(0.0, 0.0), p(0.0, 10.0), p(10.0, 10.0), p(10.0, 0.0)], true).unwrap();
    let out = cw.round_corners(2.0, None).unwrap();
    assert_eq!(out.path.subpaths()[0].winding(), Some(Winding::Clockwise));
    assert!((out.path.signed_area() + (100.0 - 4.0 * (4.0 - PI))).abs() < 1e-9);
    assert_continuous(&out.path);
}

#[test]
fn a_nearly_straight_corner_gets_a_huge_tangent_length_but_stays_within_its_segments() {
    // 179.9° interior angle: d = r/tan(θ/2) ≈ r·tiny, harmless; 0.1° spike: d huge, limited by the sides.
    let shallow = Path::polyline(&[p(0.0, 0.0), p(10.0, 0.0), p(20.0, 0.017_453)], false).unwrap();
    assert_eq!(shallow.round_corners(1.0, None).unwrap().rounded, 1);
    let spike = Path::polyline(&[p(0.0, 0.0), p(10.0, 0.0), p(0.0, 0.0174)], false).unwrap();
    let out = spike.round_corners(1.0, None).unwrap();
    assert_eq!((out.rounded, out.limited), (1, 1));
    let b = out.path.bounds().unwrap();
    assert!(b.min().x >= -1e-9 && b.max().x <= 10.0 + 1e-9, "the fillet never leaves the original extent");
}

#[test]
fn the_round_corner_node_edit_matches_rounding_that_node() {
    let sq = rect(10.0, 10.0);
    let edit = sq.edit_nodes(&NodeEdit::RoundCorner { node: node(0, 1), radius: 2.0 }).unwrap();
    assert_eq!(edit, sq.round_corners(2.0, Some(&[node(0, 1)])).unwrap().path);
    // Limited by the neighbouring sides rather than rejected: d = 10 at a right angle gives r = 10.
    let big = sq.edit_nodes(&NodeEdit::RoundCorner { node: node(0, 1), radius: 50.0 }).unwrap();
    assert_eq!(arcs(&big).len(), 1);
    let (c, r) = arcs(&big)[0];
    assert!(near(c, p(0.0, 10.0)) && (r - 10.0).abs() < 1e-9, "{c:?} {r}");
}

#[test]
fn the_round_corner_node_edit_names_why_it_refuses() {
    let open = PathBuilder::new().move_to(p(0.0, 0.0)).line_to(p(10.0, 0.0)).line_to(p(10.0, 10.0)).build().unwrap();
    assert_eq!(
        open.edit_nodes(&NodeEdit::RoundCorner { node: node(0, 0), radius: 1.0 }),
        Err(NodeEditError::NotACorner(node(0, 0)))
    );
    assert_eq!(
        open.edit_nodes(&NodeEdit::RoundCorner { node: node(0, 1), radius: -1.0 }),
        Err(NodeEditError::InvalidParameter)
    );
    assert_eq!(
        open.edit_nodes(&NodeEdit::RoundCorner { node: node(0, 7), radius: 1.0 }),
        Err(NodeEditError::NoSuchNode(node(0, 7)))
    );
    let tri = Path::polyline(&[p(0.0, 0.0), p(5.0, 0.0), p(10.0, 0.0), p(5.0, 5.0)], true).unwrap();
    assert_eq!(
        tri.edit_nodes(&NodeEdit::RoundCorner { node: node(0, 1), radius: 1.0 }),
        Err(NodeEditError::NotACorner(node(0, 1)))
    );
}

proptest! {
    /// A regular n-gon rounded with a radius that fits loses exactly n fillet areas.
    #[test]
    fn regular_polygon_fillet_area_matches_the_closed_form(n in 3u32..12, big_r in 5.0f64..100.0, frac in 0.05f64..0.9) {
        let poly = Path::regular_polygon(p(3.0, -2.0), big_r, n, Angle::ZERO).unwrap();
        let theta = PI - 2.0 * PI / f64::from(n);
        let side = 2.0 * big_r * (PI / f64::from(n)).sin();
        // Largest radius whose tangent length fits half a side, scaled by `frac`.
        let r = frac * (side / 2.0) * (theta / 2.0).tan();
        let out = poly.round_corners(r, None).unwrap();
        prop_assert_eq!((out.rounded, out.limited), (n as usize, 0));
        let per_corner = r * r * (1.0 / (theta / 2.0).tan() - (PI - theta) / 2.0);
        let expected = poly.signed_area() - f64::from(n) * per_corner;
        prop_assert!((out.path.signed_area() - expected).abs() < 1e-7 * expected.abs().max(1.0));
        prop_assert!(out.path.signed_area() < poly.signed_area());
    }

    /// Rounding never grows the outline: bounds only shrink, direction is kept.
    #[test]
    fn rounding_never_leaves_the_original_bounds(w in 0.5f64..80.0, h in 0.5f64..80.0, r in 0.01f64..200.0) {
        let sq = rect(w, h);
        let out = sq.round_corners(r, None).unwrap();
        let (a, b) = (sq.bounds().unwrap(), out.path.bounds().unwrap());
        prop_assert!(b.min().x >= a.min().x - 1e-9 && b.max().x <= a.max().x + 1e-9);
        prop_assert!(b.min().y >= a.min().y - 1e-9 && b.max().y <= a.max().y + 1e-9);
        prop_assert!(out.path.signed_area() > 0.0 && out.path.signed_area() <= sq.signed_area() + 1e-9);
    }
}
