//! The confirmation before a job is started (M06): which job, on which machine, from which
//! revision, on which material and where, and what is known about the controller's start. Nothing
//! is sent by showing it; the start goes out only when the user confirms exactly this view.
//!
//! The panel is honest about evidence: a start has no hardware-proven wire form for any controller
//! yet (`docs/hardware/evidence.md`), so the view says so instead of promising a start. A start that
//! was sent once is never offered again, also not after a lost reply.

use burnburn_geometry::Bounds;
use burnburn_machine::ExecutionState;
use burnburn_types::RevisionId;

use crate::production::ActionId;

/// What is known about the controller's ability to start a stored job.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum StartEvidence {
    /// No hardware evidence for the start request on this controller: the controller may refuse it
    /// or ignore it. `needs` names the evidence that is missing.
    NotProven { needs: &'static str },
}

/// The evidence that is missing for a start, as `docs/hardware/evidence.md` lists it.
pub const START_NEEDS: &str = "capture of the job start (START_PROCESS) and the status that proves the job runs, on the concrete model and firmware";

/// The confirmation on screen. Copy `action` and `seen` into `UiIntent::ConfirmStart`.
#[derive(Debug, Clone, PartialEq)]
pub struct StartConfirmViewModel {
    pub job_name: String,
    pub plan: burnburn_job::JobPlanId,
    /// The document revision the job was compiled from.
    pub revision: RevisionId,
    /// The machine profile the job was prepared for.
    pub profile_id: String,
    pub profile_revision: u32,
    /// The machine the start would go to.
    pub machine: String,
    /// What the user declared about the controller (never what it replied); `None`: not declared.
    pub declared_model: Option<String>,
    pub declared_firmware: Option<String>,
    /// The material of the project's plates when they agree.
    pub material: Option<String>,
    /// The size and place of the job on the bed, from the frozen plan.
    pub bounds: Option<Bounds>,
    /// The head position the job was placed on, in machine-frame mm, if it was placed on it.
    pub placed_from_mm: Option<(f64, f64)>,
    pub evidence: StartEvidence,
    pub action: ActionId,
    pub seen: Option<ExecutionState>,
}
