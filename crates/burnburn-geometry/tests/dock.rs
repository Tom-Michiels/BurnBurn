//! W2-L2 Dock and Move Together (E04). Oracles are hand-worked distances between simple shapes,
//! bounding-box arithmetic and the invariant that a docked shape touches but does not overlap.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use burnburn_geometry::{
    Bounds, Direction, DockContact, DockDirection, DockError, Path, PathBuilder, Point, ProductionTolerance, Rect,
    TogetherAnchor, TogetherError, Vector, dock_offset, move_together_offsets,
};
use burnburn_types::Length;
use proptest::prelude::*;

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn tol() -> ProductionTolerance {
    ProductionTolerance::new(mm(0.001)).unwrap()
}

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Path {
    Path::rectangle(Rect::new(p(x, y), mm(w), mm(h)).unwrap())
}

fn poly(points: &[(f64, f64)]) -> Path {
    let mut b = PathBuilder::new().move_to(p(points[0].0, points[0].1));
    for &(x, y) in &points[1..] {
        b = b.line_to(p(x, y));
    }
    b.close().build().unwrap()
}

fn dock(m: &Path, r: &Path, d: DockDirection, c: DockContact, gap: f64) -> Result<Vector, DockError> {
    dock_offset(&[m], &[r], d, c, gap, &tol())
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn a_rectangle_docks_flush_against_a_wall_in_all_four_directions() {
    let wall = rect(20.0, 0.0, 5.0, 30.0);
    let to_right = dock(&rect(0.0, 5.0, 4.0, 4.0), &wall, DockDirection::Right, DockContact::Geometry, 0.0).unwrap();
    assert!(near(to_right.x, 16.0) && near(to_right.y, 0.0), "{to_right:?}");
    let to_left = dock(&rect(40.0, 5.0, 4.0, 4.0), &wall, DockDirection::Left, DockContact::Geometry, 0.0).unwrap();
    assert!(near(to_left.x, -15.0) && near(to_left.y, 0.0), "{to_left:?}");
    let floor = rect(0.0, 0.0, 30.0, 2.0);
    let down = dock(&rect(5.0, 12.0, 4.0, 4.0), &floor, DockDirection::Down, DockContact::Geometry, 0.0).unwrap();
    assert!(near(down.y, -10.0) && near(down.x, 0.0), "{down:?}");
    let ceiling = rect(0.0, 20.0, 30.0, 2.0);
    let up = dock(&rect(5.0, 5.0, 4.0, 4.0), &ceiling, DockDirection::Up, DockContact::Geometry, 0.0).unwrap();
    assert!(near(up.y, 11.0) && near(up.x, 0.0), "{up:?}");
}

#[test]
fn a_gap_leaves_clearance_along_the_dock_direction() {
    let wall = rect(20.0, 0.0, 5.0, 30.0);
    let v = dock(&rect(0.0, 5.0, 4.0, 4.0), &wall, DockDirection::Right, DockContact::Geometry, 1.5).unwrap();
    assert!(near(v.x, 14.5), "{v:?}");
}

#[test]
fn a_triangle_tip_enters_a_v_notch_deeper_than_its_bounding_box_would() {
    // Reference: a block with a V-shaped notch whose bottom is at x = 30; walls open to x = 20.
    let block = poly(&[(20.0, 0.0), (20.0, 3.0), (30.0, 5.0), (20.0, 7.0), (20.0, 10.0), (40.0, 10.0), (40.0, 0.0)]);
    // Moving: a triangle pointing right with its tip at (8, 5) and base on x = 0.
    let tri = poly(&[(0.0, 4.0), (8.0, 5.0), (0.0, 6.0)]);
    let by_geometry = dock(&tri, &block, DockDirection::Right, DockContact::Geometry, 0.0).unwrap();
    assert!(near(by_geometry.x, 22.0), "tip reaches the notch bottom: {by_geometry:?}");
    let by_bounds = dock(&tri, &block, DockDirection::Right, DockContact::Bounds, 0.0).unwrap();
    assert!(near(by_bounds.x, 12.0), "boxes stop at the block's front face: {by_bounds:?}");
}

#[test]
fn a_circle_docks_with_its_extreme_point_on_the_wall_within_the_flattening_budget() {
    let circle = Path::circle(p(0.0, 5.0), 3.0).unwrap();
    let wall = rect(10.0, 0.0, 2.0, 10.0);
    let v = dock(&circle, &wall, DockDirection::Right, DockContact::Geometry, 0.0).unwrap();
    // The circle's extreme point is at x = 3; the flattened outline lies inside the circle.
    assert!(v.x >= 7.0 - 1e-9 && v.x <= 7.0 + 0.001 + 1e-9, "{v:?}");
}

#[test]
fn an_l_shape_hooks_around_a_corner_that_bounding_boxes_ignore() {
    // An L whose foot reaches in front of the stem: the foot touches the wall first when
    // the wall is only as tall as the foot.
    let l = poly(&[(0.0, 0.0), (6.0, 0.0), (6.0, 2.0), (2.0, 2.0), (2.0, 8.0), (0.0, 8.0)]);
    let low_wall = rect(12.0, 0.0, 3.0, 2.0);
    let g = dock(&l, &low_wall, DockDirection::Right, DockContact::Geometry, 0.0).unwrap();
    assert!(near(g.x, 6.0), "{g:?}");
    // A wall only next to the stem's top: the stem (x = 2) is the front there.
    let high_wall = rect(12.0, 4.0, 3.0, 4.0);
    let g = dock(&l, &high_wall, DockDirection::Right, DockContact::Geometry, 0.0).unwrap();
    assert!(near(g.x, 10.0), "{g:?}");
}

#[test]
fn a_flat_edge_is_stopped_by_the_tip_of_a_pointed_reference() {
    // The tip at y = 2 lies between the horizontal rays of the box's corners: only the reference's
    // own vertex can find the contact.
    let tip = poly(&[(20.0, 2.0), (26.0, 0.0), (26.0, 4.0)]);
    let v = dock(&rect(0.0, 0.0, 4.0, 4.0), &tip, DockDirection::Right, DockContact::Geometry, 0.0).unwrap();
    assert!(near(v.x, 16.0), "{v:?}");
    let v = dock(&rect(40.0, 0.0, 4.0, 4.0), &tip, DockDirection::Left, DockContact::Geometry, 0.0).unwrap();
    assert!(near(v.x, -14.0), "{v:?}");
}

#[test]
fn several_reference_objects_act_as_one_wall() {
    let (near_wall, far_wall) = (rect(20.0, 0.0, 2.0, 10.0), rect(30.0, 0.0, 2.0, 10.0));
    let moving = rect(0.0, 2.0, 4.0, 4.0);
    let v = dock_offset(&[&moving], &[&far_wall, &near_wall], DockDirection::Right, DockContact::Geometry, 0.0, &tol())
        .unwrap();
    assert!(near(v.x, 16.0), "{v:?}");
}

#[test]
fn a_group_of_moving_objects_travels_as_one_body() {
    let (a, b) = (rect(0.0, 0.0, 2.0, 2.0), rect(5.0, 0.0, 2.0, 2.0));
    let wall = rect(20.0, 0.0, 2.0, 2.0);
    let v = dock_offset(&[&a, &b], &[&wall], DockDirection::Right, DockContact::Geometry, 0.0, &tol()).unwrap();
    assert!(near(v.x, 13.0), "the front-most member decides: {v:?}");
}

#[test]
fn docking_needs_a_reference_ahead_and_no_overlap() {
    let moving = rect(0.0, 0.0, 4.0, 4.0);
    // Beside the sweep corridor.
    assert_eq!(
        dock(&moving, &rect(20.0, 10.0, 2.0, 2.0), DockDirection::Right, DockContact::Geometry, 0.0),
        Err(DockError::NoContact)
    );
    // Behind.
    assert_eq!(
        dock(&moving, &rect(-10.0, 0.0, 2.0, 4.0), DockDirection::Right, DockContact::Geometry, 0.0),
        Err(DockError::NoContact)
    );
    // Overlapping.
    assert_eq!(
        dock(&moving, &rect(3.0, 1.0, 5.0, 2.0), DockDirection::Right, DockContact::Geometry, 0.0),
        Err(DockError::Overlapping)
    );
    // One contained in the other.
    assert_eq!(
        dock(&rect(1.0, 1.0, 1.0, 1.0), &moving, DockDirection::Right, DockContact::Geometry, 0.0),
        Err(DockError::Overlapping)
    );
    assert_eq!(
        dock(&moving, &rect(3.0, 1.0, 5.0, 2.0), DockDirection::Right, DockContact::Bounds, 0.0),
        Err(DockError::Overlapping)
    );
    assert_eq!(
        dock(&moving, &Path::default(), DockDirection::Right, DockContact::Geometry, 0.0),
        Err(DockError::EmptyGeometry)
    );
    assert_eq!(
        dock(&moving, &rect(8.0, 0.0, 1.0, 1.0), DockDirection::Right, DockContact::Geometry, f64::NAN),
        Err(DockError::NotFinite)
    );
    assert_eq!(
        dock(&moving, &rect(8.0, 0.0, 1.0, 1.0), DockDirection::Right, DockContact::Geometry, -0.5),
        Err(DockError::InvalidGap)
    );
}

#[test]
fn an_object_that_already_touches_stays_where_it_is() {
    let v =
        dock(&rect(0.0, 0.0, 4.0, 4.0), &rect(4.0, 1.0, 2.0, 2.0), DockDirection::Right, DockContact::Geometry, 0.0)
            .unwrap();
    assert!(near(v.x, 0.0), "{v:?}");
}

proptest! {
    /// For axis-aligned boxes both contact modes must agree with plain box arithmetic.
    #[test]
    fn boxes_dock_like_box_arithmetic_in_either_mode(
        mx in -50.0f64..50.0, my in -50.0f64..50.0, mw in 1.0f64..20.0, mh in 1.0f64..20.0,
        gapx in 0.5f64..80.0, rw in 1.0f64..20.0, u in 0.0f64..1.0, rh in 1.0f64..20.0,
        gap in 0.0f64..3.0,
    ) {
        let moving = rect(mx, my, mw, mh);
        // The reference sits to the right and overlaps the moving box's vertical extent.
        let ry = my - rh + 0.25 + u * (mh + rh - 0.5);
        let reference = rect(mx + mw + gapx, ry, rw, rh);
        let expected = gapx - gap;
        for mode in [DockContact::Bounds, DockContact::Geometry] {
            let v = dock(&moving, &reference, DockDirection::Right, mode, gap).unwrap();
            prop_assert!((v.x - expected).abs() < 1e-9 && v.y == 0.0, "{mode:?}: {v:?} != {expected}");
        }
    }

    /// After docking with no gap, the shapes touch without overlapping: moving them back by any
    /// distance leaves them apart and moving further would overlap (for convex polygons).
    #[test]
    fn a_docked_polygon_touches_the_reference_without_overlapping(
        sides in 3u32..9, radius in 1.0f64..8.0, rot in 0.0f64..std::f64::consts::TAU, dist in 12.0f64..40.0, cy in -3.0f64..3.0,
    ) {
        let moving = Path::regular_polygon(p(0.0, 0.0), radius, sides, burnburn_types::Angle::from_radians(rot).unwrap())
            .unwrap();
        let wall = rect(dist, -20.0, 3.0, 40.0 + cy);
        let v = dock(&moving, &wall, DockDirection::Right, DockContact::Geometry, 0.0).unwrap();
        let moved = moving.transform(&burnburn_geometry::Affine::translate(v).unwrap());
        let b = moved.bounds().unwrap();
        prop_assert!(b.max().x <= dist + 1e-9, "no overlap: {} > {}", b.max().x, dist);
        prop_assert!(b.max().x >= dist - 1e-9, "touching: {} < {}", b.max().x, dist);
    }
}

fn bx(x0: f64, x1: f64) -> Bounds {
    Bounds::from_corners(p(x0, 0.0), p(x1, 5.0))
}

fn together(b: &[Bounds], a: TogetherAnchor, gap: f64) -> Result<Vec<f64>, TogetherError> {
    move_together_offsets(b, Direction::Horizontal, a, gap).map(|v| v.iter().map(|o| o.x).collect())
}

#[test]
fn objects_close_in_on_the_leftmost_one() {
    // Boxes at [0,2], [5,8], [20,21]; the first stays.
    let b = [bx(0.0, 2.0), bx(5.0, 8.0), bx(20.0, 21.0)];
    let v = together(&b, TogetherAnchor::Start, 0.0).unwrap();
    assert_eq!(v, vec![0.0, -3.0, -15.0]);
}

#[test]
fn the_anchor_object_decides_which_side_closes_in() {
    let b = [bx(0.0, 2.0), bx(5.0, 8.0), bx(20.0, 21.0)];
    // The middle object stays; the outer two come in from both sides.
    let v = together(&b, TogetherAnchor::Object(1), 0.0).unwrap();
    assert_eq!(v, vec![3.0, 0.0, -12.0]);
    // The last one stays: everything moves right.
    let v = together(&b, TogetherAnchor::End, 0.0).unwrap();
    assert_eq!(v, vec![15.0, 12.0, 0.0]);
}

#[test]
fn a_gap_is_left_between_neighbours_and_overlaps_are_not_pushed_apart() {
    let b = [bx(0.0, 4.0), bx(10.0, 12.0), bx(13.0, 14.0)];
    let v = together(&b, TogetherAnchor::Start, 1.0).unwrap();
    assert_eq!(v, vec![0.0, -5.0, -5.0]);
    // Second overlaps the first: it stays; the third closes in on the second.
    let b = [bx(0.0, 4.0), bx(3.0, 6.0), bx(10.0, 11.0)];
    let v = together(&b, TogetherAnchor::Start, 0.0).unwrap();
    assert_eq!(v, vec![0.0, 0.0, -4.0]);
}

#[test]
fn an_object_nested_in_the_anchor_does_not_pull_the_next_one_into_it() {
    // [6,8] lies inside the anchor [0,10]; the third object must still stop at the anchor's edge.
    let b = [bx(0.0, 10.0), bx(6.0, 8.0), bx(20.0, 25.0)];
    let v = together(&b, TogetherAnchor::Start, 0.0).unwrap();
    assert_eq!(v, vec![0.0, 0.0, -10.0]);
}

#[test]
fn the_start_anchor_is_the_object_with_the_lowest_edge_not_the_lowest_centre() {
    // The small box has the lower centre, but the wide one starts further left.
    let b = [bx(2.0, 4.0), bx(0.0, 10.0), bx(20.0, 25.0)];
    let v = together(&b, TogetherAnchor::Start, 0.0).unwrap();
    assert_eq!(v, vec![0.0, 0.0, -10.0]);
    let v = together(&b, TogetherAnchor::End, 0.0).unwrap();
    // Mirror image: the object ending furthest right stays; in centre order the wide box then the small one close in.
    assert_eq!(v, vec![6.0, 10.0, 0.0]);
}

#[test]
fn vertical_moves_use_the_y_axis_only() {
    let b = [Bounds::from_corners(p(0.0, 0.0), p(5.0, 2.0)), Bounds::from_corners(p(3.0, 10.0), p(8.0, 12.0))];
    let v = move_together_offsets(&b, Direction::Vertical, TogetherAnchor::Start, 0.0).unwrap();
    assert_eq!(v, vec![Vector::ZERO, Vector::new(0.0, -8.0)]);
}

#[test]
fn the_result_does_not_depend_on_the_selection_order() {
    let b = [bx(0.0, 2.0), bx(5.0, 8.0), bx(20.0, 21.0), bx(9.0, 10.0)];
    let base = together(&b, TogetherAnchor::Start, 0.5).unwrap();
    for order in [[3usize, 0, 2, 1], [1, 2, 3, 0], [2, 3, 1, 0]] {
        let shuffled: Vec<Bounds> = order.iter().map(|&i| b[i]).collect();
        let got = together(&shuffled, TogetherAnchor::Start, 0.5).unwrap();
        for (k, &i) in order.iter().enumerate() {
            assert!(near(got[k], base[i]), "order {order:?}: {got:?} vs {base:?}");
        }
    }
    // An explicit reference follows the object, not the position in the selection.
    let base = together(&b, TogetherAnchor::Object(1), 0.0).unwrap();
    let order = [2usize, 1, 3, 0];
    let shuffled: Vec<Bounds> = order.iter().map(|&i| b[i]).collect();
    let got = together(&shuffled, TogetherAnchor::Object(1), 0.0).unwrap();
    for (k, &i) in order.iter().enumerate() {
        assert!(near(got[k], base[i]), "{got:?} vs {base:?}");
    }
}

#[test]
fn bad_input_is_a_typed_error() {
    let b = [bx(0.0, 2.0), bx(5.0, 8.0)];
    assert_eq!(together(&b[..1], TogetherAnchor::Start, 0.0), Err(TogetherError::TooFew));
    assert_eq!(together(&b, TogetherAnchor::Object(2), 0.0), Err(TogetherError::NoSuchReference(2)));
    assert_eq!(together(&b, TogetherAnchor::Start, -1.0), Err(TogetherError::InvalidGap));
    assert_eq!(together(&b, TogetherAnchor::Start, f64::NAN), Err(TogetherError::InvalidGap));
}

proptest! {
    #[test]
    fn move_together_keeps_sizes_the_anchor_and_never_adds_space(
        spans in prop::collection::vec((-40.0f64..40.0, 0.5f64..8.0), 2..8),
        gap in 0.0f64..2.0, pick in 0usize..8,
    ) {
        let b: Vec<Bounds> = spans.iter().map(|&(x, w)| bx(x, x + w)).collect();
        let a = pick % b.len();
        let anchor = TogetherAnchor::Object(a);
        let offsets = move_together_offsets(&b, Direction::Horizontal, anchor, gap).unwrap();
        prop_assert_eq!(offsets[a], Vector::ZERO);
        let moved: Vec<Bounds> = b.iter().zip(&offsets).map(|(bb, o)| {
            Bounds::from_corners(p(bb.min().x + o.x, bb.min().y + o.y), p(bb.max().x + o.x, bb.max().y + o.y))
        }).collect();
        let span = |v: &[Bounds]| {
            let lo = v.iter().map(|b| b.min().x).fold(f64::INFINITY, f64::min);
            let hi = v.iter().map(|b| b.max().x).fold(f64::NEG_INFINITY, f64::max);
            hi - lo
        };
        prop_assert!(span(&moved) <= span(&b) + 1e-9, "the arrangement never grows");
        for o in &offsets {
            prop_assert_eq!(o.y, 0.0);
        }
        // Idempotent: closing in a second time changes nothing.
        let again = move_together_offsets(&moved, Direction::Horizontal, anchor, gap).unwrap();
        for o in again {
            prop_assert!(o.x.abs() < 1e-9, "second pass moved by {}", o.x);
        }
    }
}
