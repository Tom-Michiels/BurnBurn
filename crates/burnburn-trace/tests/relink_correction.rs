//! G9-trace: linking a correction to the contours of another variant (the A/B comparison) although its
//! bindings no longer hold. The scene of `compare_corrected.rs`: at threshold 175 the ring's counter (gray
//! 150) is filled, so the hole the correction protected is gone and the correction does not bind.

#![allow(clippy::unwrap_used, clippy::panic)]

mod support;

use std::collections::BTreeSet;
use std::sync::Arc;

use burnburn_image::{ImageRecipe, PixelRect, PixelScale, RegionShape, SourceImage, Threshold};
use burnburn_trace::{
    Anchor, ContourKind, CorrectionRecord, CorrectionRequest, Reapplied, TraceError, TraceRecipe, TraceRequest,
    TraceResult, compare_recipes_corrected, compare_recipes_corrected_relinking, reapply_correction, relink_correction,
    replay_corrections, replay_corrections_relinking, retrace_region, trace,
};
use burnburn_types::{CancellationSource, CancellationToken, RevisionId, StableId, TaskId};
use support::*;

const S: f64 = 0.1;
const W: u32 = 200;
const H: u32 = 120;
const REGION: PixelRect = PixelRect { x: 30, y: 40, width: 90, height: 70 };

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

fn area(r: &TraceResult) -> f64 {
    r.contours.iter().filter(|c| c.kind != ContourKind::Hole).map(|c| c.area_mm2).sum()
}

fn fixture() -> (Arc<SourceImage>, Arc<TraceResult>, CorrectionRecord, Arc<TraceResult>) {
    let source = Arc::new(scene());
    let operator = corrected(&source);
    let record = operator.corrections[0].clone();
    let b = base_with(&source, 175);
    (source, operator, record, b)
}

#[test]
fn the_fixture_really_does_not_bind_on_b() {
    let (source, _, record, b) = fixture();
    let r = reapply_correction(&record, &b, &source, TaskId::from_raw(7), &CancellationToken::never()).unwrap();
    assert!(matches!(r, Reapplied::NeedsRebind { .. }));
}

#[test]
fn relinking_applies_the_region_and_recipe_to_the_contours_now_there() {
    let (source, _, record, b) = fixture();
    let plain = area(&b);
    let r = relink_correction(&record, &b, &source, TaskId::from_raw(7), &CancellationToken::never()).unwrap();
    assert_eq!(r.result.corrections.len(), 1, "the correction is in the lineage of the result");
    assert_eq!(r.result.base_key, b.base_key);
    // The correction traces its area at 200: the half of the faint bump (gray 180, radius 0.8 mm) that sticks
    // out of the rectangle joins the part. Analytic: pi * 0.8^2 / 2 = 1.005 mm2.
    let gained = area(&r.result) - plain;
    assert!((0.7..=1.3).contains(&gained), "the bump joined the part: {gained} mm2");
    // The one protected contour (the ring's counter) does not exist at 175.
    assert_eq!((r.protected_kept, r.protected_lost), (0, 1));
}

#[test]
fn a_protected_contour_is_found_again_by_kind_and_bounds_not_by_id() {
    let (source, _, record, _) = fixture();
    // Threshold 130 gives the same contours within a fraction of a pixel, so the counter is found again.
    let near = base_with(&source, 130);
    let r = relink_correction(&record, &near, &source, TaskId::from_raw(7), &CancellationToken::never()).unwrap();
    assert_eq!((r.protected_kept, r.protected_lost), (1, 0));
    assert_eq!(r.result.corrections[0].protected.len(), 1);
}

#[test]
fn replaying_with_the_index_relinks_and_without_it_reports_unbound() {
    let (source, _, record, b) = fixture();
    let records = [record];
    let cancel = CancellationToken::never();
    let plain = replay_corrections(&b, &source, &records, TaskId::from_raw(7), &cancel).unwrap();
    assert_eq!((plain.applied, plain.unbound.len(), plain.relinked.len()), (0, 1, 0));
    let linked =
        replay_corrections_relinking(&b, &source, &records, &BTreeSet::from([0]), TaskId::from_raw(7), &cancel)
            .unwrap();
    assert_eq!((linked.applied, linked.unbound.len(), linked.relinked.clone()), (1, 0, vec![0]));
    assert!(area(&linked.result) > area(&plain.result) + 0.7);
    // An index that is not unbound changes nothing.
    let other = replay_corrections_relinking(&b, &source, &records, &BTreeSet::from([5]), TaskId::from_raw(7), &cancel)
        .unwrap();
    assert_eq!((other.applied, other.unbound.len()), (0, 1));
}

#[test]
fn the_comparison_lists_what_did_not_bind_on_b_and_relinks_on_request() {
    let (source, operator, record, b) = fixture();
    let records = [record];
    let cancel = CancellationToken::never();
    let c =
        compare_recipes_corrected(&source, &recipe(127), &recipe(175), &records, TaskId::from_raw(5), &cancel).unwrap();
    assert_eq!(c.b_unbound.len(), 1);
    assert_eq!((c.b_unbound[0].index, c.b_unbound[0].lost.len()), (0, 1));
    assert!(c.b_relinked.is_empty());
    assert_eq!(c.b.geometry_hash, b.geometry_hash());

    let linked = compare_recipes_corrected_relinking(
        &source,
        &recipe(127),
        &recipe(175),
        &records,
        &BTreeSet::from([0]),
        TaskId::from_raw(5),
        &cancel,
    )
    .unwrap();
    assert_eq!((linked.corrections.a_applied, linked.corrections.b_applied), (1, 1));
    assert!(linked.b_unbound.is_empty());
    assert_eq!(linked.b_relinked, vec![0]);
    assert_eq!(linked.a.geometry_hash, operator.geometry_hash(), "A is untouched");
    assert_ne!(linked.b.geometry_hash, b.geometry_hash());
}

#[test]
fn relinking_refuses_another_source_and_honours_cancellation() {
    let (source, _, record, b) = fixture();
    let other = Arc::new(rasterize_levels(W, H, S, &[]));
    let r = relink_correction(&record, &b, &other, TaskId::from_raw(7), &CancellationToken::never());
    assert!(matches!(r, Err(TraceError::CorrectionMismatch("source"))), "{r:?}");
    let cancel = CancellationSource::new();
    cancel.cancel();
    let r = relink_correction(&record, &b, &source, TaskId::from_raw(7), &cancel.token());
    assert!(matches!(r, Err(TraceError::Cancelled(_))));
}
