//! G8-trace, T10: the A/B comparison with the stored local corrections. Variant A is what the tracer
//! shows (base trace plus corrections); variant B is the other recipe with the same corrections where
//! they still bind, and without them where they do not.

#![allow(clippy::unwrap_used, clippy::panic)]

mod support;

use std::collections::BTreeSet;
use std::sync::Arc;

use burnburn_geometry::Affine;
use burnburn_image::{ImageRecipe, PixelRect, PixelScale, RegionShape, SourceImage, Threshold};
use burnburn_trace::{
    Anchor, ContourKind, CorrectionRecord, CorrectionRequest, TraceRecipe, TraceRequest, TraceResult, compare_recipes,
    compare_recipes_corrected, retrace_region, trace,
};
use burnburn_types::{CancellationSource, CancellationToken, RevisionId, StableId, TaskId};
use support::*;

const S: f64 = 0.1;
const W: u32 = 200;
const H: u32 = 120;
const REGION: PixelRect = PixelRect { x: 30, y: 40, width: 90, height: 70 };

/// The scene of the correction tests: a ring with a light-gray counter, a rectangle with a faint bump,
/// a faint speck and a small black disc.
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

/// The operator's correction on the threshold-127 trace: the region traced at 200, holes protected.
fn corrected(source: &Arc<SourceImage>) -> Arc<TraceResult> {
    let base = base_with(source, 127);
    let protected: BTreeSet<_> = base.contours.iter().filter(|c| c.kind == ContourKind::Hole).map(|c| c.id).collect();
    let req = CorrectionRequest {
        task: TaskId::from_raw(2),
        previous: base,
        source: source.clone(),
        region: RegionShape::Rect(REGION),
        recipe: recipe(200),
        anchors: vec![Anchor { at_px: [60.0, 70.0] }],
        protected,
        node_target: None,
    };
    Arc::new(retrace_region(&req, &CancellationToken::never()).unwrap())
}

fn records(result: &TraceResult) -> Vec<CorrectionRecord> {
    result.corrections.clone()
}

#[test]
fn variant_a_is_the_trace_the_operator_sees_corrections_included() {
    let source = Arc::new(scene());
    let operator = corrected(&source);
    let plain = base_with(&source, 127);
    assert_ne!(operator.geometry_hash(), plain.geometry_hash(), "a fixture where the correction matters");

    // B is the same recipe: with the corrections it must equal A and the operator's result.
    let c = compare_recipes_corrected(
        &source,
        &recipe(127),
        &recipe(127),
        &records(&operator),
        TaskId::from_raw(5),
        &CancellationToken::never(),
    )
    .unwrap();
    assert_eq!(c.a.geometry_hash, operator.geometry_hash());
    assert_eq!(c.b.geometry_hash, operator.geometry_hash());
    assert_eq!((c.corrections.stored, c.corrections.a_applied, c.corrections.b_applied), (1, 1, 1));
    assert_eq!(c.a_result.corrections.len(), 1);
}

#[test]
fn a_correction_that_does_not_bind_on_variant_b_is_left_out_of_b_only() {
    let source = Arc::new(scene());
    let operator = corrected(&source);
    // Threshold 175 fills the ring's counter: the protected hole the correction was bound to is gone.
    let c = compare_recipes_corrected(
        &source,
        &recipe(127),
        &recipe(175),
        &records(&operator),
        TaskId::from_raw(5),
        &CancellationToken::never(),
    )
    .unwrap();
    assert_eq!(c.a.geometry_hash, operator.geometry_hash(), "A keeps the correction");
    assert_eq!(c.b.geometry_hash, base_with(&source, 175).geometry_hash(), "B is its plain trace");
    assert_eq!((c.corrections.stored, c.corrections.a_applied, c.corrections.b_applied), (1, 1, 0));
    assert_ne!(c.a.geometry_hash, c.b.geometry_hash);
}

#[test]
fn without_corrections_the_result_is_that_of_the_plain_comparison() {
    let source = Arc::new(scene());
    let plain =
        compare_recipes(&source, &recipe(127), &recipe(175), TaskId::from_raw(5), &CancellationToken::never()).unwrap();
    let with_none = compare_recipes_corrected(
        &source,
        &recipe(127),
        &recipe(175),
        &[],
        TaskId::from_raw(5),
        &CancellationToken::never(),
    )
    .unwrap();
    assert_eq!((with_none.a.geometry_hash, with_none.b.geometry_hash), (plain.a.geometry_hash, plain.b.geometry_hash));
    assert_eq!((with_none.corrections.stored, plain.corrections.stored), (0, 0));
}

#[test]
fn a_cancelled_comparison_gives_no_result() {
    let source = Arc::new(scene());
    let operator = corrected(&source);
    let cancel = CancellationSource::new();
    cancel.cancel();
    let r = compare_recipes_corrected(
        &source,
        &recipe(127),
        &recipe(175),
        &records(&operator),
        TaskId::from_raw(5),
        &cancel.token(),
    );
    assert!(matches!(r, Err(burnburn_trace::TraceError::Cancelled(_))));
    let _ = Affine::IDENTITY;
}
