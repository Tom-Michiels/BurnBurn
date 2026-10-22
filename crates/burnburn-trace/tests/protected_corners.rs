//! G8-trace, T06: protected corners. A mild corner (a turn below the corner angle) is smoothed over
//! by the fit when the tolerance allows; naming it in `contour.corners` makes the fit keep a node
//! exactly there. The scene is a house outline whose roof apex is a crease that turns by only
//! 2 · atan(0.6 / 6) = 11.4°, far below the default 60°; the apex is at (8.0, 8.6) mm, so its
//! distance to the fitted path is the analytic oracle.

#![allow(clippy::unwrap_used)]

mod support;

use std::sync::Arc;

use burnburn_geometry::{Point, Segment};
use burnburn_image::{ImageRecipe, PixelScale, Threshold};
use burnburn_trace::{
    ContourSettings, FitSettings, TraceError, TraceRecipe, TraceRequest, TraceResult, recipe_from_engine,
    recipe_to_engine, trace,
};
use burnburn_types::{CancellationToken, RevisionId, StableId, TaskId};
use support::*;

const S: f64 = 0.1;
const W: u32 = 160;
const H: u32 = 130;
/// The roof apex in mm (y up) and in adjusted pixels (y down).
const APEX_MM: (f64, f64) = (8.0, 8.6);
const APEX_PX: [f64; 2] = [80.0, 44.0];

fn scene() -> Arc<burnburn_image::SourceImage> {
    let house = convex(vec![(2.0, 2.0), (14.0, 2.0), (14.0, 8.0), (8.0, 8.6), (2.0, 8.0)]);
    Arc::new(rasterize(W, H, S, house))
}

fn recipe(corners: Vec<[f64; 2]>) -> TraceRecipe {
    TraceRecipe {
        image: ImageRecipe {
            scale: Some(PixelScale::uniform_mm_per_pixel(S).unwrap()),
            threshold: Threshold::Fixed(127),
            ..ImageRecipe::default()
        },
        contour: ContourSettings { corners, ..ContourSettings::default() },
        // A generous budget: a smooth curve may round the apex off within 0.6 mm.
        fit: FitSettings { tolerance_mm: 0.6, ..FitSettings::default() },
    }
}

fn run(corners: Vec<[f64; 2]>) -> Result<TraceResult, TraceError> {
    let req = TraceRequest {
        task: TaskId::from_raw(1),
        source_revision: RevisionId::INITIAL,
        source: scene(),
        recipe: recipe(corners),
    };
    trace(&req, &CancellationToken::never())
}

/// Distance in mm from the apex to the fitted outline, sampled finely.
fn apex_distance(r: &TraceResult) -> f64 {
    let apex = Point::new(APEX_MM.0, APEX_MM.1);
    let path = burnburn_geometry::Path::from_subpaths(vec![r.contours[0].path.clone()]);
    sample_path(&path, 0.01).iter().map(|p| p.distance(apex)).fold(f64::INFINITY, f64::min)
}

/// Distance in mm from the apex to the nearest node (segment end) of the outline.
fn apex_node_distance(r: &TraceResult) -> f64 {
    let apex = Point::new(APEX_MM.0, APEX_MM.1);
    let sub = &r.contours[0].path;
    let ends = std::iter::once(sub.start()).chain(sub.segments().iter().map(Segment::end));
    ends.map(|p| p.distance(apex)).fold(f64::INFINITY, f64::min)
}

#[test]
fn without_a_protected_corner_a_generous_fit_rounds_the_mild_apex_off() {
    let r = run(Vec::new()).unwrap();
    assert!(apex_distance(&r) > 0.15, "the fixture must really lose the apex, got {}", apex_distance(&r));
}

#[test]
fn a_click_near_the_apex_finds_the_sharpest_vertex_of_the_outline() {
    let plain = run(Vec::new()).unwrap();
    // The click lands almost two pixels beside the apex, on the roof: not on the sharpest vertex.
    let click = [APEX_PX[0] + 1.0, APEX_PX[1] + 1.5];
    let corner = plain.corner_near_px(click, burnburn_trace::CORNER_REACH_PX).expect("a vertex within reach");
    // The vertex is on the apex to within the raster (half a pixel), not merely the nearest to the click.
    assert!((corner[0] - APEX_PX[0]).hypot(corner[1] - APEX_PX[1]) <= 0.75, "{corner:?} for {APEX_PX:?}");
    // Nothing near: nothing found.
    assert_eq!(plain.corner_near_px([5.0, 5.0], burnburn_trace::CORNER_REACH_PX), None);
    assert_eq!(plain.corner_near_px([f64::NAN, 5.0], burnburn_trace::CORNER_REACH_PX), None);
}

#[test]
fn a_protected_corner_keeps_a_node_exactly_on_the_apex() {
    let plain = run(Vec::new()).unwrap();
    let corner = plain.corner_near_px([APEX_PX[0] + 1.0, APEX_PX[1] + 1.5], burnburn_trace::CORNER_REACH_PX).unwrap();
    let r = run(vec![corner]).unwrap();
    assert!(apex_distance(&r) < 0.08, "the path passes through the apex: {}", apex_distance(&r));
    assert!(apex_node_distance(&r) < 0.1, "and has a node there: {}", apex_node_distance(&r));
    assert_eq!(r.stats.corners_unmatched, 0);
}

#[test]
fn a_protected_corner_does_not_change_the_rest_of_the_outline() {
    let plain = run(Vec::new()).unwrap();
    let kept = run(vec![APEX_PX]).unwrap();
    // The four sharp corners are corners in both; the nodes far from the apex are the same.
    let far = |r: &TraceResult| -> Vec<(i64, i64)> {
        let sub = &r.contours[0].path;
        std::iter::once(sub.start())
            .chain(sub.segments().iter().map(Segment::end))
            .filter(|p| p.distance(Point::new(APEX_MM.0, APEX_MM.1)) > 2.0)
            .map(|p| ((p.x * 100.0).round() as i64, (p.y * 100.0).round() as i64))
            .collect()
    };
    let (mut a, mut b) = (far(&plain), far(&kept));
    a.sort_unstable();
    b.sort_unstable();
    assert_eq!(a, b);
}

#[test]
fn a_corner_that_names_no_outline_is_counted_and_changes_nothing() {
    let plain = run(Vec::new()).unwrap();
    let r = run(vec![[5.0, 5.0]]).unwrap();
    assert_eq!(r.stats.corners_unmatched, 1);
    assert_eq!(r.contours[0].path, plain.contours[0].path);
}

#[test]
fn corners_belong_to_the_recipe_key_and_the_stored_recipe_and_must_be_finite() {
    let key = |c: Vec<[f64; 2]>| recipe(c).key(scene().content_hash()).unwrap();
    assert_eq!(key(Vec::new()), key(Vec::new()));
    assert_ne!(key(Vec::new()), key(vec![APEX_PX]));
    assert_ne!(key(vec![APEX_PX]), key(vec![[APEX_PX[0] + 1.0, APEX_PX[1]]]));

    let with = recipe(vec![APEX_PX, [10.5, 20.25]]);
    assert_eq!(recipe_from_engine(&recipe_to_engine(&with)).unwrap(), with);
    assert!(!recipe_to_engine(&recipe(Vec::new())).params.contains_key("contour.corners"), "only when there are any");

    assert!(matches!(run(vec![[f64::NAN, 1.0]]), Err(TraceError::InvalidRecipe("contour.corners"))));
}

// ---- which contour a click means (T05) ----

#[test]
fn a_click_means_the_contour_whose_outline_is_near_or_else_the_one_around_it() {
    let r = run(Vec::new()).unwrap();
    let id = r.contours[0].id;
    // The house spans x 20…140 px and y 20…110 px (mm 2…14 and 2…8.6, picture 130 px high).
    assert_eq!(r.contour_at_px([20.0, 70.0], 1.5), Some(id), "on the left wall");
    assert_eq!(r.contour_at_px([21.0, 70.0], 1.5), Some(id), "a pixel inside the wall, within reach");
    assert_eq!(r.contour_at_px([18.5, 70.0], 1.5), Some(id), "a pixel and a half outside, within reach");
    assert_eq!(r.contour_at_px([80.0, 80.0], 1.5), Some(id), "in the body of the house");
    assert_eq!(r.contour_at_px([5.0, 5.0], 1.5), None, "outside");
    assert_eq!(r.contour_at_px([17.0, 70.0], 1.5), None, "outside and out of reach");
    assert_eq!(r.contour_at_px([f64::NAN, 70.0], 1.5), None);
}
