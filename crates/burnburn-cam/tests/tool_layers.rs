//! Tool layers T1/T2 in the compiler and preflight (LY01, G5-cam): guide and registration
//! geometry is part of the document but never of a job, and it is not a mistake to leave it there.

#![allow(clippy::unwrap_used)]

mod common;

use burnburn_cam::{Finding, compile, preflight};
use burnburn_document::CommandPayload;
use burnburn_document::process::{ProcessCommand, ToolLayer};
use burnburn_job::PlanEvent;
use burnburn_types::CancellationToken;
use common::*;

#[test]
fn a_tool_layer_adds_nothing_to_the_plan_and_is_not_reported_as_forgotten() {
    let mut with_tool = session();
    let part = rectangle(&mut with_tool, 100.0, 100.0, 50.0, 50.0);
    // A guide far outside the bed: on a tool layer it is not a cut and cannot be "outside the bed".
    let guide = rectangle(&mut with_tool, 5000.0, 5000.0, 10.0, 10.0);
    add_op(&mut with_tool, line_op("Snijden", 100.0, 20.0), vec![part]);
    run(
        &mut with_tool,
        CommandPayload::Process(ProcessCommand::AddToolLayer { tool: ToolLayer::T1, targets: vec![guide] }),
    );

    let mut without = session();
    let part = rectangle(&mut without, 100.0, 100.0, 50.0, 50.0);
    add_op(&mut without, line_op("Snijden", 100.0, 20.0), vec![part]);

    let cancel = CancellationToken::never();
    let a = compile(&with_tool.snapshot(), &profile(), &options(), &cancel).unwrap();
    let b = compile(&without.snapshot(), &profile(), &options(), &cancel).unwrap();
    // Ids differ (the guide took one), the motion must not.
    let motion = |p: &burnburn_job::JobPlan| -> Vec<(bool, f64, f64)> {
        p.events()
            .iter()
            .filter_map(|e| match e {
                PlanEvent::Travel { to } => Some((false, to.x_mm(), to.y_mm())),
                PlanEvent::Cut { to, .. } => Some((true, to.x_mm(), to.y_mm())),
                _ => None,
            })
            .collect()
    };
    assert_eq!(motion(&a), motion(&b), "the tool layer changes no motion");

    let report = preflight(&with_tool.snapshot(), &a, &profile());
    assert!(report.findings.is_empty(), "{:?}", report.findings);
}

#[test]
fn geometry_on_no_layer_at_all_is_still_reported() {
    let mut s = session();
    let part = rectangle(&mut s, 100.0, 100.0, 50.0, 50.0);
    let forgotten = rectangle(&mut s, 300.0, 100.0, 50.0, 50.0);
    add_op(&mut s, line_op("Snijden", 100.0, 20.0), vec![part]);
    let plan = compile(&s.snapshot(), &profile(), &options(), &CancellationToken::never()).unwrap();
    let report = preflight(&s.snapshot(), &plan, &profile());
    assert_eq!(report.findings, vec![Finding::UnassignedGeometry { geometry: forgotten }]);
}
