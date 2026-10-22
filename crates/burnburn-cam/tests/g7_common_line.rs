//! Common-line cutting (C12, G7-cam): parts that touch (or lie within the tolerance) share one cut.
//! Oracles are analytic: perimeters of rectangles minus the length of the edges they share.

#![allow(clippy::unwrap_used)]

mod common;

use burnburn_cam::CompileError;
use burnburn_document::process::{AdvancedOptions, CommonLine, ProcessCommand};
use burnburn_document::{CommandPayload, DocumentSession};
use burnburn_job::{Feature, Support};
use burnburn_types::{GeometryId, OperationId};
use common::*;
use proptest::prelude::*;

fn common_line_on(s: &mut DocumentSession, targets: Vec<GeometryId>, tolerance: f64, min_overlap: f64) -> OperationId {
    let op = add_op(s, line_op("Snijden", 10.0, 50.0), targets);
    let advanced = AdvancedOptions {
        common_line: Some(CommonLine::new(mm(tolerance), mm(min_overlap)).unwrap()),
        ..Default::default()
    };
    run(s, CommandPayload::Process(ProcessCommand::SetAdvanced { operation: op, advanced }));
    op
}

fn plain(s: &mut DocumentSession, targets: Vec<GeometryId>) -> OperationId {
    add_op(s, line_op("Snijden", 10.0, 50.0), targets)
}

fn cut_mm(s: &DocumentSession) -> f64 {
    cut_length(&plan_from_outside(s, &supporting(&[Feature::CommonLine])).unwrap())
}

/// Two 20 x 10 rectangles, the second `gap` mm to the right of the first, `lift` mm higher.
fn pair(gap: f64, lift: f64) -> (DocumentSession, Vec<GeometryId>) {
    let mut s = session();
    let a = rectangle(&mut s, 0.0, 0.0, 20.0, 10.0);
    let b = rectangle(&mut s, 20.0 + gap, lift, 20.0, 10.0);
    (s, vec![a, b])
}

#[test]
fn parts_whose_edges_only_nearly_coincide_share_their_common_edge_once() {
    // Edges that are bit-identical are cut once by the shared-edge policy of every operation
    // (IM32), option or not. A gap of 0.03 mm is not identical: only common-line cutting joins them.
    let (mut s, ids) = pair(0.03, 0.0);
    plain(&mut s, ids);
    assert!((cut_mm(&s) - 120.0).abs() < 1e-6, "the premise: both perimeters, {}", cut_mm(&s));

    let (mut s, ids) = pair(0.03, 0.0);
    common_line_on(&mut s, ids, 0.05, 2.0);
    // 2 x 60 mm of perimeter, the 10 mm edge they share is cut once.
    assert!((cut_mm(&s) - 110.0).abs() < 1e-6, "{}", cut_mm(&s));
}

#[test]
fn exactly_touching_parts_are_cut_once_with_or_without_the_option() {
    let (mut s, ids) = pair(0.0, 0.0);
    plain(&mut s, ids);
    assert!((cut_mm(&s) - 110.0).abs() < 1e-6, "{}", cut_mm(&s));
    let (mut s, ids) = pair(0.0, 0.0);
    common_line_on(&mut s, ids, 0.05, 2.0);
    assert!((cut_mm(&s) - 110.0).abs() < 1e-6, "{}", cut_mm(&s));
}

#[test]
fn parts_within_the_tolerance_share_the_cut_and_parts_beyond_it_do_not() {
    let (mut s, ids) = pair(0.03, 0.0);
    common_line_on(&mut s, ids, 0.05, 2.0);
    assert!((cut_mm(&s) - 110.0).abs() < 1e-6, "a gap of 0.03 is within 0.05: {}", cut_mm(&s));
    let (mut s, ids) = pair(0.06, 0.0);
    common_line_on(&mut s, ids, 0.05, 2.0);
    assert!((cut_mm(&s) - 120.0).abs() < 1e-6, "a gap of 0.06 is not: {}", cut_mm(&s));

}

#[test]
fn a_stretch_shorter_than_the_minimal_overlap_is_cut_twice() {
    // The second part is lifted by 8.5 mm: the edges share 1.5 mm.
    let (mut s, ids) = pair(0.0, 8.5);
    common_line_on(&mut s, ids.clone(), 0.05, 2.0);
    assert!((cut_mm(&s) - 120.0).abs() < 1e-6, "1.5 mm is below the minimum of 2: {}", cut_mm(&s));

    let (mut s, ids) = pair(0.0, 8.5);
    common_line_on(&mut s, ids, 0.05, 1.0);
    // The stretch within the tolerance of the other edge reaches the tolerance (0.05) past its end.
    assert!((cut_mm(&s) - (120.0 - 1.5 - 0.05)).abs() < 1e-6, "1.5 mm is above the minimum of 1: {}", cut_mm(&s));
}

#[test]
fn without_proof_for_common_line_cutting_the_plan_is_refused_by_name() {
    let (mut s, ids) = pair(0.0, 0.0);
    let op = common_line_on(&mut s, ids, 0.05, 2.0);
    match plan_from_outside(&s, &profile()) {
        Err(CompileError::UnsupportedFeature { operation, feature: Feature::CommonLine, support }) => {
            assert_eq!(operation, op);
            assert!(matches!(support, Support::Unverified { .. }), "{support:?}");
        }
        other => panic!("expected the named refusal, got {other:?}"),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(40))]

    /// A grid of n x m rectangles, columns 0.02 mm apart, cuts its outline and every inner edge
    /// once: the sum of all perimeters minus the shared edges, counted by hand.
    #[test]
    fn a_grid_of_touching_parts_is_cut_once_per_edge(n in 1u32..4, m in 1u32..4, w in 5.0f64..30.0, h in 5.0f64..30.0) {
        let mut s = session();
        let mut ids = Vec::new();
        for i in 0..n {
            for j in 0..m {
                ids.push(rectangle(&mut s, f64::from(i) * (w + 0.02), f64::from(j) * h, w, h));
            }
        }
        common_line_on(&mut s, ids, 0.05, 1.0);
        let cells = f64::from(n * m);
        let shared = f64::from((n - 1) * m) * h + f64::from(n * (m - 1)) * w;
        let expected = cells * 2.0 * (w + h) - shared;
        prop_assert!((cut_mm(&s) - expected).abs() < 1e-6, "{} vs {}", cut_mm(&s), expected);
    }
}
