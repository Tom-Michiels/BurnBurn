//! G6-werk T05: local corrections belong to the stored recipe. A corrected trace keeps its region,
//! local threshold, anchors and protected details with the bindings it was made on; tracing the
//! same source again applies them, and a correction whose contours changed is reported for
//! re-linking instead of being applied to something else.

#![allow(clippy::unwrap_used, clippy::panic)]

mod support;

use std::collections::BTreeSet;
use std::sync::Arc;

use burnburn_document::engine::ParamValue;
use burnburn_document::trace::TraceCommand;
use burnburn_geometry::{Affine, Point};
use burnburn_image::{ImageRecipe, PixelRect, PixelScale, RegionShape, SourceImage, Threshold};
use burnburn_trace::{
    Anchor, ContourKind, CorrectionRequest, TraceRecipe, TraceRequest, TraceResult, corrections_from_engine,
    replay_corrections, retrace_region, trace, trace_command,
};
use burnburn_types::{CancellationToken, ContentHash, RevisionId, StableId, TaskId};
use support::*;

const S: f64 = 0.1;
const W: u32 = 200;
const H: u32 = 120;
const REGION: PixelRect = PixelRect { x: 30, y: 40, width: 90, height: 70 };

/// The scene of the IM19 tests: a ring with a light-gray counter (150), a rectangle with a faint bump
/// (180), a faint speck (170) and a small black disc far from the region.
fn scene() -> SourceImage {
    let hole = disc(5.0, 6.0, 1.5);
    let ring = disc(5.0, 6.0, 3.5);
    let rect = convex(vec![(11.0, 2.0), (18.0, 2.0), (18.0, 10.0), (11.0, 10.0)]);
    let bump = disc(11.0, 6.0, 0.8);
    let speck = disc(8.8, 2.0, 0.5);
    let far = disc(2.0, 10.5, 0.8);
    rasterize_levels(W, H, S, &[(&hole, 150), (&ring, 0), (&rect, 0), (&bump, 180), (&speck, 170), (&far, 0)])
}

fn recipe(threshold: u8) -> TraceRecipe {
    TraceRecipe {
        image: ImageRecipe {
            scale: Some(PixelScale::uniform_mm_per_pixel(S).unwrap()),
            threshold: Threshold::Fixed(threshold),
            ..ImageRecipe::default()
        },
        ..TraceRecipe::default()
    }
}

fn base_with(source: &Arc<SourceImage>, threshold: u8) -> Arc<TraceResult> {
    let req = TraceRequest {
        task: TaskId::from_raw(1),
        source_revision: RevisionId::INITIAL,
        source: source.clone(),
        recipe: recipe(threshold),
    };
    Arc::new(trace(&req, &CancellationToken::never()).unwrap())
}

fn correct(base: &Arc<TraceResult>, source: &Arc<SourceImage>, protect_holes: bool) -> Arc<TraceResult> {
    let protected: BTreeSet<_> = if protect_holes {
        base.contours.iter().filter(|c| c.kind == ContourKind::Hole).map(|c| c.id).collect()
    } else {
        BTreeSet::new()
    };
    let req = CorrectionRequest {
        task: TaskId::from_raw(2),
        previous: base.clone(),
        source: source.clone(),
        region: RegionShape::Rect(REGION),
        recipe: recipe(200),
        anchors: vec![Anchor { at_px: [60.0, 70.0] }],
        protected,
        node_target: None,
    };
    Arc::new(retrace_region(&req, &CancellationToken::never()).unwrap())
}

fn stored(result: &TraceResult) -> burnburn_document::engine::EngineRecipe {
    let TraceCommand::Create { definition, .. } =
        trace_command(result, ContentHash::from_bytes([3; 32]), "x", Affine::IDENTITY, None).unwrap()
    else {
        panic!("a Create")
    };
    definition.recipe
}

#[test]
fn a_corrected_trace_stores_its_corrections_with_their_bindings_and_reads_them_back() {
    let source = Arc::new(scene());
    let base = base_with(&source, 127);
    let corrected = correct(&base, &source, true);
    let want = &corrected.corrections[0];
    assert!(!want.touched.is_empty() && !want.protected.is_empty(), "a fixture with something to bind");

    let engine = stored(&corrected);
    assert!(engine.get("trace.corrections").is_some(), "the corrections are recipe parameters");
    let back = corrections_from_engine(&engine, &corrected.recipe, source.content_hash()).unwrap();
    assert_eq!(back.len(), 1);
    let got = &back[0];
    assert_eq!(got.region, want.region);
    assert_eq!(got.anchors, want.anchors);
    assert_eq!(got.protected, want.protected);
    assert_eq!(got.touched, want.touched);
    assert_eq!(got.untouched_in_region, want.untouched_in_region);
    assert_eq!(got.recipe.image.threshold, Threshold::Fixed(200));
    assert_eq!(got.local_key, want.local_key);
}

#[test]
fn a_plain_trace_stores_no_corrections() {
    let source = Arc::new(scene());
    let base = base_with(&source, 127);
    let engine = stored(&base);
    assert!(engine.get("trace.corrections").is_none());
    assert!(corrections_from_engine(&engine, &base.recipe, source.content_hash()).unwrap().is_empty());
}

#[test]
fn tracing_again_applies_the_stored_corrections_and_gives_the_same_geometry() {
    let source = Arc::new(scene());
    let corrected = correct(&base_with(&source, 127), &source, true);
    let records = corrections_from_engine(&stored(&corrected), &corrected.recipe, source.content_hash()).unwrap();

    let fresh = base_with(&source, 127);
    let replay =
        replay_corrections(&fresh, &source, &records, TaskId::from_raw(9), &CancellationToken::never()).unwrap();
    assert_eq!((replay.applied, replay.unbound.len(), replay.failed.len()), (1, 0, 0));
    assert_eq!(replay.result.geometry_hash(), corrected.geometry_hash(), "the same contours as the operator got");
    assert_eq!(replay.result.corrections.len(), 1);
    assert_ne!(replay.result.geometry_hash(), fresh.geometry_hash(), "and not the plain trace");
}

#[test]
fn a_correction_whose_contours_changed_is_reported_for_relinking_and_not_applied() {
    let source = Arc::new(scene());
    let corrected = correct(&base_with(&source, 127), &source, true);
    let records = corrections_from_engine(&stored(&corrected), &corrected.recipe, source.content_hash()).unwrap();

    // Another threshold fills the ring's counter: the protected hole the correction was bound to is gone.
    let other = base_with(&source, 175);
    let replay =
        replay_corrections(&other, &source, &records, TaskId::from_raw(9), &CancellationToken::never()).unwrap();
    assert_eq!(replay.applied, 0);
    assert_eq!(replay.unbound.len(), 1);
    assert_eq!(replay.unbound[0].index, 0);
    assert!(!replay.unbound[0].lost.is_empty(), "the hole it was bound to");
    assert_eq!(replay.result.geometry_hash(), other.geometry_hash(), "nothing was applied to something else");
    assert!(replay.result.corrections.is_empty());
}

#[test]
fn a_damaged_corrections_list_is_refused() {
    let source = Arc::new(scene());
    let corrected = correct(&base_with(&source, 127), &source, true);
    let hash = source.content_hash();
    let mut not_a_list = stored(&corrected);
    not_a_list.params.insert("trace.corrections".into(), ParamValue::Text("rect".into()));
    assert!(corrections_from_engine(&not_a_list, &corrected.recipe, hash).is_err());
    let mut short = stored(&corrected);
    short.params.insert(
        "trace.corrections".into(),
        ParamValue::List(vec![ParamValue::List(vec![ParamValue::Text("rect".into()), ParamValue::Integer(1)])]),
    );
    assert!(corrections_from_engine(&short, &corrected.recipe, hash).is_err());
    let mut unknown = stored(&corrected);
    unknown.params.insert(
        "trace.corrections".into(),
        ParamValue::List(vec![ParamValue::List(vec![ParamValue::Text("blob".into())])]),
    );
    assert!(corrections_from_engine(&unknown, &corrected.recipe, hash).is_err());
}

#[test]
fn the_point_of_an_anchor_is_kept_exactly() {
    let source = Arc::new(scene());
    let corrected = correct(&base_with(&source, 127), &source, false);
    let records = corrections_from_engine(&stored(&corrected), &corrected.recipe, source.content_hash()).unwrap();
    assert_eq!(records[0].anchors, vec![Anchor { at_px: [60.0, 70.0] }]);
    let _ = Point::ORIGIN;
}
