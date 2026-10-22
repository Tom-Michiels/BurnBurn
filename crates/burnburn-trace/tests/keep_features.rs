//! G7-trace, T06: parts and holes the "ignore less than" filter drops can be restored one by one,
//! and holes (or parts) can be protected from the filter beforehand. A kept feature is named by an
//! anchor: a vertex of its own outline in adjusted pixels (y down). Areas below are analytic
//! (π r²), the scene is rasterised by the independent supersampling rasterizer of `support`.

#![allow(clippy::unwrap_used)]

mod support;

use std::sync::Arc;

use burnburn_image::{ImageRecipe, PixelScale, Threshold};
use burnburn_trace::{
    ContourKind, ContourSettings, TraceRecipe, TraceRequest, TraceResult, recipe_from_engine, recipe_to_engine, trace,
};
use burnburn_types::{CancellationToken, RevisionId, StableId, TaskId};
use support::*;

const S: f64 = 0.1;

/// A part (disc r 4 mm at (6, 6)) with a pin hole (r 0.4 mm, 0.50 mm²) and a speck (r 0.6 mm,
/// 1.13 mm²) at (15, 6); and a small ring (outer r 1.2 mm = 4.5 mm², hole r 0.5 mm = 0.79 mm²)
/// at (6, 10.5)... kept apart from the rest by the image height of 12 mm.
fn scene() -> Arc<burnburn_image::SourceImage> {
    let part = |x: f64, y: f64| disc(6.0, 6.0, 4.0)(x, y) && !disc(6.0, 6.0, 0.4)(x, y);
    let speck = disc(15.0, 6.0, 0.6);
    let ring = |x: f64, y: f64| disc(25.0, 6.0, 1.2)(x, y) && !disc(25.0, 6.0, 0.5)(x, y);
    Arc::new(rasterize_levels(300, 120, S, &[(&part, 0), (&speck, 0), (&ring, 0)]))
}

fn recipe(min_area_mm2: f64, keep: Vec<[f64; 2]>) -> TraceRecipe {
    TraceRecipe {
        image: ImageRecipe {
            scale: Some(PixelScale::uniform_mm_per_pixel(S).unwrap()),
            threshold: Threshold::Fixed(127),
            ..ImageRecipe::default()
        },
        contour: ContourSettings { min_area_mm2, keep, ..ContourSettings::default() },
        ..TraceRecipe::default()
    }
}

fn run(min_area_mm2: f64, keep: Vec<[f64; 2]>) -> TraceResult {
    let req = TraceRequest {
        task: TaskId::from_raw(1),
        source_revision: RevisionId::INITIAL,
        source: scene(),
        recipe: recipe(min_area_mm2, keep),
    };
    trace(&req, &CancellationToken::never()).unwrap()
}

/// (parts, holes) in the result.
fn counts(r: &TraceResult) -> (usize, usize) {
    (r.stats.closed_shapes, r.stats.holes)
}

#[test]
fn the_filter_drops_the_pin_hole_the_speck_and_the_small_ring_and_reports_each_with_an_anchor() {
    let r = run(6.0, Vec::new());
    assert_eq!(counts(&r), (1, 0), "only the big part is left");
    // The pin hole and the speck are dropped as roots; the small ring (4.5 mm²) is dropped whole.
    let kinds: Vec<ContourKind> = r.removed.iter().map(|f| f.kind).collect();
    assert_eq!(kinds.iter().filter(|k| **k == ContourKind::Hole).count(), 1, "{kinds:?}");
    assert_eq!(kinds.iter().filter(|k| **k == ContourKind::Outer).count(), 2, "{kinds:?}");
    // An anchor lies on the feature itself: near the circle it belongs to (radius in px).
    let near = |a: [f64; 2], cx: f64, cy: f64, radius_mm: f64| {
        let (x, y) = ((a[0] * S) - cx, (120.0 - a[1]) * S - cy); // px (y down) -> mm (y up)
        ((x.hypot(y)) - radius_mm).abs() < 1.5 * S
    };
    let hole = r.removed.iter().find(|f| f.kind == ContourKind::Hole).unwrap();
    assert!(near(hole.anchor_px, 6.0, 6.0, 0.4), "{:?}", hole.anchor_px);
    assert!(r.removed.iter().any(|f| f.kind == ContourKind::Outer && near(f.anchor_px, 15.0, 6.0, 0.6)), "the speck");
}

#[test]
fn a_dropped_piece_comes_back_alone_when_its_anchor_is_kept() {
    let base = run(6.0, Vec::new());
    let speck = base
        .removed
        .iter()
        .find(|f| f.kind == ContourKind::Outer && f.area_mm2 < 2.0)
        .expect("the speck is among the dropped")
        .anchor_px;
    let r = run(6.0, vec![speck]);
    assert_eq!(counts(&r), (2, 0), "the big part and the speck; the pin hole and the ring stay away");
    assert_eq!(r.removed.len(), base.removed.len() - 1, "the restored piece is no longer reported dropped");
    assert_eq!(r.stats.keep_unmatched, 0);
    assert!(r.contours.iter().any(|c| (c.area_mm2 - std::f64::consts::PI * 0.36).abs() < 0.15), "the speck's area");
}

#[test]
fn a_hole_protected_beforehand_survives_a_filter_that_would_drop_it() {
    let plain = run(0.0, Vec::new());
    let hole = plain
        .contours
        .iter()
        .find(|c| c.kind == ContourKind::Hole && (c.area_mm2 - std::f64::consts::PI * 0.16).abs() < 0.1)
        .expect("the pin hole");
    let anchor = plain.contour_anchor_px(hole.id).expect("a traced contour has an anchor");
    let r = run(6.0, vec![anchor]);
    assert_eq!(counts(&r), (1, 1), "the big part with its pin hole; the speck and the ring are still dropped");
    assert!(r.removed.iter().all(|f| f.kind == ContourKind::Outer));
}

#[test]
fn keeping_an_inner_piece_keeps_the_part_around_it() {
    let plain = run(0.0, Vec::new());
    // The hole of the small ring: the one with area π·0.25.
    let hole = plain
        .contours
        .iter()
        .find(|c| c.kind == ContourKind::Hole && (c.area_mm2 - std::f64::consts::PI * 0.25).abs() < 0.1)
        .expect("the ring's hole");
    let anchor = plain.contour_anchor_px(hole.id).unwrap();
    let r = run(6.0, vec![anchor]);
    // The ring (outer 4.5 mm² < 6) is below the filter too, but it has to stay for its hole to exist.
    assert_eq!(counts(&r), (2, 1), "big part, and the ring with the hole that was kept");
}

#[test]
fn an_anchor_that_matches_no_outline_changes_nothing_and_is_counted() {
    let base = run(6.0, Vec::new());
    let r = run(6.0, vec![[290.0, 5.0], [150.0, 60.0]]);
    assert_eq!(counts(&r), counts(&base));
    assert_eq!(r.removed.len(), base.removed.len());
    assert_eq!(r.stats.keep_unmatched, 2);
}

#[test]
fn the_kept_anchors_are_part_of_the_recipe_key_and_survive_the_engine_form() {
    let source = scene().content_hash();
    let (a, b) = (recipe(6.0, Vec::new()), recipe(6.0, vec![[10.0, 20.5]]));
    assert_ne!(a.key(source).unwrap(), b.key(source).unwrap());
    assert_eq!(a.key(source).unwrap(), recipe(6.0, Vec::new()).key(source).unwrap());
    let engine = recipe_to_engine(&b);
    assert_eq!(recipe_from_engine(&engine).unwrap(), b);
    assert!(!recipe_to_engine(&a).params.contains_key("contour.keep"), "no anchors, no parameter: old keys stay");
    let mut bad = engine.clone();
    bad.params.insert(
        "contour.keep".into(),
        burnburn_document::engine::ParamValue::List(vec![burnburn_document::engine::ParamValue::Number(1.0)]),
    );
    assert!(recipe_from_engine(&bad).is_err());
    let mut nan = b.clone();
    nan.contour.keep = vec![[f64::NAN, 1.0]];
    assert!(nan.validate().is_err());
}
