//! Plain-data types of the GRBL backend in the window (G5-grbl-ui): the preflight findings the
//! G-code lowering names, and what an export wrote. Nothing here talks to a machine.

use std::collections::HashMap;
use std::path::PathBuf;

use burnburn_gcode::{GrblProfile, RefusalKind, check};
use burnburn_job::{JobPlan, JobPlanId, PlanEvent, SourceRef};
use burnburn_types::{GeometryId, OperationId, RevisionId};

/// One reason the GRBL backend cannot write the plan, with where it comes from. Hosts say
/// [`GrblFinding::kind`] in words and select [`GrblFinding::geometry`] when the row is clicked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrblFinding {
    pub kind: RefusalKind,
    /// The operation the refused plan event belongs to; `None` for the plan or profile as a whole.
    pub operation: Option<OperationId>,
    /// The object to go to when the finding is clicked: the source of the refused event or, for
    /// an operation-wide refusal (air assist), the first object that operation burns.
    pub geometry: Option<GeometryId>,
}

/// The last G-code export of a document: exactly the plan the preview showed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GcodeExportViewModel {
    pub plan: JobPlanId,
    pub plan_revision: RevisionId,
    pub path: PathBuf,
    pub bytes: usize,
    pub lines: usize,
}

fn origin(event: &PlanEvent) -> Option<SourceRef> {
    match *event {
        PlanEvent::Cut { origin, .. }
        | PlanEvent::ScanRun { origin, .. }
        | PlanEvent::PowerRun { origin, .. }
        | PlanEvent::Dot { origin }
        | PlanEvent::RampRun { origin, .. } => origin,
        _ => None,
    }
}

/// Every reason the GRBL backend refuses `plan`, one finding per reason and object (a long path
/// outside the bed is one finding, not one per segment), in plan order.
pub(crate) fn findings(plan: &JobPlan, grbl: &GrblProfile) -> Vec<GrblFinding> {
    let events = plan.events();
    let mut operation_of = Vec::with_capacity(events.len());
    let mut first_object: HashMap<OperationId, GeometryId> = HashMap::new();
    let mut current = None;
    for event in events {
        if let PlanEvent::OperationBoundary { operation, .. } = *event {
            current = Some(operation);
        }
        operation_of.push(current);
        if let Some(source) = origin(event) {
            first_object.entry(source.operation).or_insert(source.geometry);
        }
    }
    let mut out: Vec<GrblFinding> = Vec::new();
    for refusal in check(plan, grbl) {
        let operation = refusal.event.and_then(|i| operation_of.get(i).copied().flatten());
        let geometry = refusal
            .event
            .and_then(|i| events.get(i).and_then(origin).map(|s| s.geometry))
            .or_else(|| operation.and_then(|o| first_object.get(&o).copied()));
        let finding = GrblFinding { kind: refusal.kind, operation, geometry };
        if !out.contains(&finding) {
            out.push(finding);
        }
    }
    out
}
