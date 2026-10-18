//! Preview data derived from one frozen plan (IM15, C07).

use burnburn_types::OperationId;

use crate::{JobPlan, JobPlanId, MachinePoint, MachineProfile, PlanEvent, SourceRef};

/// What a preview segment represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MoveKind {
    /// Laser off between burning moves.
    Travel,
    /// Vector cut/score.
    Cut,
    /// Laser-on scan run.
    Scan,
    /// Laser-off run-up/run-out of a scan. On controllers that scan natively the firmware
    /// performs this motion itself; its exact shape and timing are firmware-dependent.
    Overscan,
}

/// One drawable, traceable segment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewMove {
    pub kind: MoveKind,
    pub from: MachinePoint,
    pub to: MachinePoint,
    /// Index of the plan event this segment belongs to.
    pub event: usize,
    pub operation: Option<OperationId>,
    /// Source geometry + operation for burning moves (click → select source, C07).
    pub source: Option<SourceRef>,
}

/// Everything a preview shows, for exactly one plan.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanPreview {
    /// The plan this preview was derived from; equal to the id of the exported/uploaded plan.
    pub plan: JobPlanId,
    pub moves: Vec<PreviewMove>,
    pub cut_length_mm: f64,
    pub scan_length_mm: f64,
    pub overscan_length_mm: f64,
    pub travel_length_mm: f64,
    /// Constant-velocity **estimate** in seconds: every move at its nominal speed, no
    /// acceleration, no firmware overscan behaviour. Not a measured duration.
    pub estimated_seconds: f64,
}

/// Derives preview data from `plan`. Travel time uses the profile's travel speed; burning and
/// overscan moves use the speed of their settings block.
pub fn preview(plan: &JobPlan, profile: &MachineProfile) -> PlanPreview {
    let mut pv = PlanPreview {
        plan: plan.id(),
        moves: Vec::new(),
        cut_length_mm: 0.0,
        scan_length_mm: 0.0,
        overscan_length_mm: 0.0,
        travel_length_mm: 0.0,
        estimated_seconds: 0.0,
    };
    let travel_speed = profile.travel_speed_mm_s();
    let mut at = plan.start();
    let mut operation = None;
    let mut speed = travel_speed;
    for (event, e) in plan.events().iter().enumerate() {
        let mut push = |kind, from: MachinePoint, to: MachinePoint, source, v: f64| {
            let len = from.distance(to);
            match kind {
                MoveKind::Travel => pv.travel_length_mm += len,
                MoveKind::Cut => pv.cut_length_mm += len,
                MoveKind::Scan => pv.scan_length_mm += len,
                MoveKind::Overscan => pv.overscan_length_mm += len,
            }
            pv.estimated_seconds += len / v;
            pv.moves.push(PreviewMove { kind, from, to, event, operation, source });
        };
        match *e {
            PlanEvent::OperationBoundary { operation: op, settings, .. } => {
                operation = Some(op);
                speed = plan.settings().get(settings.get()).map_or(travel_speed, |s| s.speed_mm_s);
            }
            PlanEvent::Travel { to } => {
                push(MoveKind::Travel, at, to, None, travel_speed);
                at = to;
            }
            PlanEvent::Cut { to, origin } => {
                push(MoveKind::Cut, at, to, origin, speed);
                at = to;
            }
            PlanEvent::FocusOffset { .. } => {}
            PlanEvent::ScanRun { line, origin } | PlanEvent::PowerRun { line, origin, .. } => {
                let (run_up, run_out) = line.overscan_ends();
                if run_up != line.from {
                    push(MoveKind::Overscan, run_up, line.from, None, speed);
                }
                push(MoveKind::Scan, line.from, line.to, origin, speed);
                if run_out != line.to {
                    push(MoveKind::Overscan, line.to, run_out, None, speed);
                }
                at = line.to;
            }
        }
    }
    pv
}
