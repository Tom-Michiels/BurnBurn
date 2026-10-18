//! IM32 (CAM part): a boundary shared exactly by two target geometries of one Line operation
//! is cut once unless the operation explicitly asks to cut it per region.

#![allow(clippy::unwrap_used)]

mod common;

use burnburn_cam::{Finding, Severity, compile, preflight};
use burnburn_document::process::{AdvancedOptions, ProcessCommand, SharedEdgePolicy};
use burnburn_document::{CommandPayload, DocumentSession};
use burnburn_job::preview;
use burnburn_types::{CancellationToken, GeometryId, OperationId};
use common::*;

/// Two 10 × 10 squares that share the edge x = 10 (as two colour regions would).
fn regions() -> (DocumentSession, GeometryId, GeometryId, OperationId) {
    let mut s = session();
    let a = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let b = rectangle(&mut s, 10.0, 0.0, 10.0, 10.0);
    let op = add_op(&mut s, line_op("Snijden", 10.0, 50.0), vec![a, b]);
    (s, a, b, op)
}

#[test]
fn shared_colour_boundary_is_not_scheduled_twice_without_explicit_policy() {
    let (s, a, b, op) = regions();
    let plan = compile(&s.snapshot(), &profile(), &options(), &CancellationToken::never()).unwrap();
    // 40 + 40 − 10: the shared edge is cut once; its second occurrence is laser-off travel.
    let pv = preview(&plan, &profile());
    assert!((pv.cut_length_mm - 70.0).abs() < 1e-9, "cut length {}", pv.cut_length_mm);
    // The default is visible: preflight warns that a shared boundary was merged.
    let report = preflight(&s.snapshot(), &plan, &profile());
    let shared = Finding::SharedBoundaryCutOnce { operation: op, geometries: vec![a, b] };
    assert!(report.findings.contains(&shared), "{:?}", report.findings);
    assert_eq!(shared.severity(), Severity::Warning);
}

#[test]
fn explicit_cut_per_region_policy_cuts_the_shared_boundary_for_each_region() {
    let (mut s, _, _, op) = regions();
    let advanced = AdvancedOptions { shared_edges: SharedEdgePolicy::CutPerRegion, ..Default::default() };
    run(&mut s, CommandPayload::Process(ProcessCommand::SetAdvanced { operation: op, advanced }));
    let plan = compile(&s.snapshot(), &profile(), &options(), &CancellationToken::never()).unwrap();
    assert!((preview(&plan, &profile()).cut_length_mm - 80.0).abs() < 1e-9);
    let report = preflight(&s.snapshot(), &plan, &profile());
    assert!(!report.findings.iter().any(|f| matches!(f, Finding::SharedBoundaryCutOnce { .. })), "explicit choice");
}

#[test]
fn nearly_coincident_edges_are_not_merged() {
    // 1 µm apart is a different edge: no tolerance-based merging.
    let mut s = session();
    let a = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0);
    let b = rectangle(&mut s, 10.001, 0.0, 10.0, 10.0);
    add_op(&mut s, line_op("Snijden", 10.0, 50.0), vec![a, b]);
    let plan = compile(&s.snapshot(), &profile(), &options(), &CancellationToken::never()).unwrap();
    assert!((preview(&plan, &profile()).cut_length_mm - 80.0).abs() < 1e-9);
}
