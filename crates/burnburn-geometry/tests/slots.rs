//! W2-L2 Resize Slots. Oracles: panels with slots and tabs of known size, so areas, widths and
//! untouched vertices follow from the numbers chosen first; widening followed by narrowing must
//! restore the outline.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

use burnburn_geometry::{Affine, Path, Point, ResizeSlots, SlotSelect, SlotsError, Vector};
use burnburn_types::Angle;
use proptest::prelude::*;

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn poly(points: &[(f64, f64)]) -> Path {
    let pts: Vec<Point> = points.iter().map(|&(x, y)| p(x, y)).collect();
    Path::polyline(&pts, true).unwrap()
}

fn spec(from: f64, to: f64, select: SlotSelect) -> ResizeSlots {
    ResizeSlots { from_width: from, to_width: to, match_tolerance: 0.01, select }
}

/// A 60 × 30 panel (counter-clockwise) with notches of `depth` cut into the top edge at the given
/// `(left, right)` x ranges; `raised` turns them into tabs standing on the top edge instead.
fn panel(features: &[(f64, f64)], depth: f64, raised: bool) -> Path {
    let top = 30.0;
    let mut pts = vec![(0.0, 0.0), (60.0, 0.0), (60.0, top)];
    let mut sorted = features.to_vec();
    sorted.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (l, r) in sorted {
        let y = if raised { top + depth } else { top - depth };
        pts.extend([(r, top), (r, y), (l, y), (l, top)]);
    }
    pts.push((0.0, top));
    poly(&pts)
}

fn vertices(path: &Path) -> Vec<Point> {
    let sp = &path.subpaths()[0];
    std::iter::once(sp.start()).chain(sp.segments().iter().map(burnburn_geometry::Segment::end)).collect()
}

fn has(path: &Path, x: f64, y: f64) -> bool {
    vertices(path).iter().any(|v| near(v.x, x) && near(v.y, y))
}

#[test]
fn widening_slots_moves_only_their_walls_and_keeps_the_depth() {
    let features = [(10.0, 13.0), (25.0, 28.0), (40.0, 43.0)];
    let original = panel(&features, 4.0, false);
    assert!(near(original.signed_area(), 1800.0 - 3.0 * 12.0));
    let out = original.resize_slots(&spec(3.0, 3.6, SlotSelect::Slots)).unwrap();
    assert_eq!((out.resized, out.skipped), (3, 0));
    // Each slot grew by 0.3 on both sides around its unchanged centre line.
    for (l, r) in features {
        assert!(has(&out.path, l - 0.3, 26.0) && has(&out.path, r + 0.3, 26.0), "{:?}", vertices(&out.path));
        assert!(has(&out.path, l - 0.3, 30.0) && has(&out.path, r + 0.3, 30.0));
    }
    assert!(near(out.path.signed_area(), 1800.0 - 3.0 * 3.6 * 4.0), "{}", out.path.signed_area());
    // The rest of the panel is untouched and no vertex was added or lost.
    for corner in [(0.0, 0.0), (60.0, 0.0), (60.0, 30.0), (0.0, 30.0)] {
        assert!(has(&out.path, corner.0, corner.1));
    }
    assert_eq!(vertices(&out.path).len(), vertices(&original).len());
    assert_eq!(out.path.bounds(), original.bounds());
}

#[test]
fn narrowing_slots_gives_the_material_back() {
    let original = panel(&[(10.0, 13.0), (25.0, 28.0)], 4.0, false);
    let out = original.resize_slots(&spec(3.0, 2.4, SlotSelect::Slots)).unwrap();
    assert_eq!(out.resized, 2);
    assert!(near(out.path.signed_area(), 1800.0 - 2.0 * 2.4 * 4.0));
    assert!(has(&out.path, 10.3, 26.0) && has(&out.path, 12.7, 26.0));
}

#[test]
fn only_features_of_the_matching_width_are_touched() {
    let original = panel(&[(10.0, 13.0), (25.0, 30.0), (40.0, 43.005)], 4.0, false);
    let out = original.resize_slots(&spec(3.0, 4.0, SlotSelect::Slots)).unwrap();
    assert_eq!(out.resized, 2, "3.0 and 3.005 match within 0.01, 5.0 does not");
    assert!(has(&out.path, 25.0, 26.0) && has(&out.path, 30.0, 26.0), "the 5 mm slot stays");
    assert_eq!(original.resize_slots(&spec(7.0, 8.0, SlotSelect::Slots)), Err(SlotsError::NoSlotsFound));
}

#[test]
fn tabs_and_slots_are_told_apart() {
    let tabs = panel(&[(10.0, 13.0), (25.0, 28.0)], 3.0, true);
    assert_eq!(tabs.resize_slots(&spec(3.0, 3.5, SlotSelect::Slots)), Err(SlotsError::NoSlotsFound));
    let out = tabs.resize_slots(&spec(3.0, 3.5, SlotSelect::Tabs)).unwrap();
    assert_eq!(out.resized, 2);
    assert!(near(out.path.signed_area(), 1800.0 + 2.0 * 3.5 * 3.0), "{}", out.path.signed_area());
    assert!(has(&out.path, 9.75, 33.0) && has(&out.path, 13.25, 33.0));
    let notches = panel(&[(10.0, 13.0)], 4.0, false);
    assert_eq!(notches.resize_slots(&spec(3.0, 3.5, SlotSelect::Tabs)), Err(SlotsError::NoSlotsFound));
    assert_eq!(notches.resize_slots(&spec(3.0, 3.5, SlotSelect::Both)).unwrap().resized, 1);
}

/// A finger-jointed top edge: tabs and slots of 3 mm alternate between wider end tabs.
fn fingers() -> Path {
    poly(&[
        (0.0, 0.0),
        (60.0, 0.0),
        (60.0, 34.0),
        (53.0, 34.0),
        (53.0, 30.0),
        (50.0, 30.0),
        (50.0, 34.0),
        (47.0, 34.0),
        (47.0, 30.0),
        (44.0, 30.0),
        (44.0, 34.0),
        (0.0, 34.0),
    ])
}

#[test]
fn resizing_the_slots_of_a_finger_joint_takes_the_material_from_the_neighbouring_tabs() {
    let original = fingers();
    let out = original.resize_slots(&spec(3.0, 3.6, SlotSelect::Slots)).unwrap();
    assert_eq!((out.resized, out.skipped), (2, 0));
    // The slots are 3.6 wide, the tab between them 2.4, and the whole edge keeps its extent.
    assert!(has(&out.path, 53.3, 30.0) && has(&out.path, 49.7, 30.0));
    assert!(has(&out.path, 49.7, 34.0), "{:?}", vertices(&out.path));
    assert_eq!(out.path.bounds(), original.bounds());
    assert!(near(out.path.signed_area(), original.signed_area() - 2.0 * 0.6 * 4.0));
}

#[test]
fn a_tab_and_a_slot_that_share_a_wall_cannot_both_move_it() {
    let out = fingers().resize_slots(&spec(3.0, 3.6, SlotSelect::Both)).unwrap();
    // Slots go first; a 3 mm tab between two resized slots would need the same walls again.
    assert_eq!(out.resized, 2);
    assert_eq!(out.skipped, 1, "the 3 mm tab between the two slots");
    assert!(near(out.path.signed_area(), fingers().signed_area() - 2.0 * 0.6 * 4.0));
}

/// A 40 × 30 plate with a hole of the given size, rotated and moved.
fn plate_with_hole(hole_w: f64, hole_h: f64, ccw_hole: bool, m: &Affine) -> Path {
    let outer = poly(&[(0.0, 0.0), (40.0, 0.0), (40.0, 30.0), (0.0, 30.0)]);
    let (x0, y0, x1, y1) = (10.0, 5.0, 10.0 + hole_w, 5.0 + hole_h);
    let corners: Vec<(f64, f64)> = if ccw_hole {
        vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
    } else {
        vec![(x0, y0), (x0, y1), (x1, y1), (x1, y0)]
    };
    let hole = poly(&corners);
    Path::from_subpaths([outer.subpaths(), hole.subpaths()].concat()).transform(m)
}

fn rotation(deg: f64) -> Affine {
    Affine::rotate(Angle::from_degrees(deg).unwrap()).then(&Affine::translate(Vector::new(3.0, -8.0)).unwrap())
}

#[test]
fn a_rectangular_hole_changes_its_short_side_about_its_centre() {
    for m in [Affine::IDENTITY, rotation(30.0)] {
        for ccw in [false, true] {
            let original = plate_with_hole(3.0, 20.0, ccw, &m);
            let out = original.resize_slots(&spec(3.0, 4.0, SlotSelect::Slots)).unwrap();
            assert_eq!(out.resized, 1, "ccw = {ccw}");
            // The outer contour is untouched; the hole is 4 × 20 around the same centre.
            assert_eq!(out.path.subpaths()[0], original.subpaths()[0]);
            // Compare through the hole's own area: the winding of the hole varies.
            let hole_area = |path: &Path| path.subpaths()[1].signed_area().unwrap().abs();
            assert!(near(hole_area(&original), 60.0) && near(hole_area(&out.path), 80.0), "{ccw}");
            let centre = |path: &Path| {
                let v = vertices(&Path::from_subpaths(vec![path.subpaths()[1].clone()]));
                p(v.iter().map(|q| q.x).sum::<f64>() / 4.0, v.iter().map(|q| q.y).sum::<f64>() / 4.0)
            };
            let (c0, c1) = (centre(&original), centre(&out.path));
            assert!(near(c0.x, c1.x) && near(c0.y, c1.y), "{c0:?} {c1:?}");
            // The long side keeps its length.
            let hv = vertices(&Path::from_subpaths(vec![out.path.subpaths()[1].clone()]));
            let sides: Vec<f64> = (0..4).map(|i| hv[i].distance(hv[(i + 1) % 4])).collect();
            let (mut a, mut b) = (sides[0], sides[1]);
            if a > b {
                std::mem::swap(&mut a, &mut b);
            }
            assert!(near(a, 4.0) && near(b, 20.0), "{sides:?}");
        }
    }
}

#[test]
fn a_square_hole_with_both_sides_matching_is_resized_in_both_directions() {
    let out =
        plate_with_hole(3.0, 3.0, false, &Affine::IDENTITY).resize_slots(&spec(3.0, 2.0, SlotSelect::Slots)).unwrap();
    let hole = out.path.subpaths()[1].signed_area().unwrap().abs();
    assert!(near(hole, 4.0), "{hole}");
}

#[test]
fn a_hole_of_another_width_and_a_solid_rectangle_are_not_slots() {
    let original = plate_with_hole(5.0, 20.0, false, &Affine::IDENTITY);
    assert_eq!(original.resize_slots(&spec(3.0, 4.0, SlotSelect::Slots)), Err(SlotsError::NoSlotsFound));
    // A 3 × 20 rectangle that is not a hole is a part, not a slot.
    let part = poly(&[(0.0, 0.0), (3.0, 0.0), (3.0, 20.0), (0.0, 20.0)]);
    assert_eq!(part.resize_slots(&spec(3.0, 4.0, SlotSelect::Both)), Err(SlotsError::NoSlotsFound));
    // Tabs selection ignores holes.
    let slot_plate = plate_with_hole(3.0, 20.0, false, &Affine::IDENTITY);
    assert_eq!(slot_plate.resize_slots(&spec(3.0, 4.0, SlotSelect::Tabs)), Err(SlotsError::NoSlotsFound));
}

#[test]
fn slots_that_would_run_into_each_other_are_refused() {
    // Two slots with a 1 mm tab between them: both widened by 1 mm per side swallows the tab.
    let original = panel(&[(10.0, 13.0), (14.0, 17.0)], 4.0, false);
    assert_eq!(original.resize_slots(&spec(3.0, 5.0, SlotSelect::Slots)), Err(SlotsError::Collision));
    // Narrower is fine, and exactly closing the tab is still a collision.
    assert!(original.resize_slots(&spec(3.0, 2.0, SlotSelect::Slots)).is_ok());
    assert_eq!(original.resize_slots(&spec(3.0, 4.0, SlotSelect::Slots)), Err(SlotsError::Collision));
    // A slot cannot be narrowed to nothing.
    assert_eq!(original.resize_slots(&spec(3.0, 0.0, SlotSelect::Slots)), Err(SlotsError::InvalidParameter));
}

#[test]
fn bad_parameters_are_typed_errors() {
    let original = panel(&[(10.0, 13.0)], 4.0, false);
    for (from, to, tol) in [
        (0.0, 3.0, 0.01),
        (-3.0, 3.0, 0.01),
        (3.0, f64::NAN, 0.01),
        (f64::INFINITY, 3.0, 0.01),
        (3.0, 3.5, -0.1),
        (3.0, 3.5, f64::NAN),
    ] {
        let s = ResizeSlots { from_width: from, to_width: to, match_tolerance: tol, select: SlotSelect::Both };
        assert_eq!(original.resize_slots(&s), Err(SlotsError::InvalidParameter), "{from} {to} {tol}");
    }
}

#[test]
fn the_feature_is_found_wherever_the_outline_starts_and_curves_are_left_alone() {
    let features = [(10.0, 13.0), (25.0, 28.0)];
    let base = vertices(&panel(&features, 4.0, false));
    // Start the same contour inside a slot.
    let rotated: Vec<(f64, f64)> = base.iter().cycle().skip(5).take(base.len()).map(|q| (q.x, q.y)).collect();
    let out = poly(&rotated).resize_slots(&spec(3.0, 3.6, SlotSelect::Slots)).unwrap();
    assert_eq!(out.resized, 2);
    assert!(near(out.path.signed_area(), 1800.0 - 2.0 * 3.6 * 4.0));
    // A circle next to the panel is not straight-edged and is returned unchanged.
    let circle = Path::circle(p(100.0, 10.0), 3.0).unwrap();
    let both = Path::from_subpaths([panel(&features, 4.0, false).subpaths(), circle.subpaths()].concat());
    let out = both.resize_slots(&spec(3.0, 3.6, SlotSelect::Slots)).unwrap();
    assert_eq!(out.path.subpaths()[1], circle.subpaths()[0]);
    // An open polyline with the same U shape is not a slot.
    let open = Path::polyline(&[p(0.0, 0.0), p(0.0, 4.0), p(3.0, 4.0), p(3.0, 0.0)], false).unwrap();
    assert_eq!(open.resize_slots(&spec(3.0, 4.0, SlotSelect::Both)), Err(SlotsError::NoSlotsFound));
}

proptest! {
    #[test]
    fn widening_and_narrowing_back_restores_the_outline_and_changes_the_area_by_width_times_depth(
        count in 1usize..5, width in 1.0f64..4.0, extra in 0.2f64..2.0, depth in 1.0f64..8.0, gap in 5.0f64..9.0,
    ) {
        // Slots of one width on a long panel; gaps wide enough for the extra width.
        let mut features = Vec::new();
        let mut x = 6.0;
        for _ in 0..count {
            features.push((x, x + width));
            x += width + gap;
        }
        let pts_panel = |w: &[(f64, f64)]| {
            let mut pts = vec![(0.0, 0.0), (x + 6.0, 0.0), (x + 6.0, 30.0)];
            for &(l, r) in w.iter().rev() {
                pts.extend([(r, 30.0), (r, 30.0 - depth), (l, 30.0 - depth), (l, 30.0)]);
            }
            pts.push((0.0, 30.0));
            poly(&pts)
        };
        let original = pts_panel(&features);
        let widened = original.resize_slots(&spec(width, width + extra, SlotSelect::Slots)).unwrap();
        prop_assert_eq!(widened.resized, count);
        let expected_area = original.signed_area() - (count as f64) * extra * depth;
        prop_assert!((widened.path.signed_area() - expected_area).abs() < 1e-9);
        let back = widened.path.resize_slots(&spec(width + extra, width, SlotSelect::Slots)).unwrap();
        for (a, b) in vertices(&back.path).iter().zip(vertices(&original)) {
            prop_assert!(a.distance(b) < 1e-9, "{a:?} vs {b:?}");
        }
    }
}
