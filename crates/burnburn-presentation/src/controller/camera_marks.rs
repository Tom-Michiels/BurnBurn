//! The job that burns the workspace-alignment marks (W3-UI-camera). A small cross per mark, built as
//! a plan of its own (not from a document), lowered to RD like every prepared job and handed to the
//! machine service as an ordinary upload, which starts nothing: starting stays an explicit step in
//! the machine panel. Proposing sends nothing; only `ConfirmMarksJob` for exactly the proposed plan
//! does, and every refusal of the machine service comes back typed.

use std::sync::Arc;

use burnburn_document::DocumentSession;
use burnburn_geometry::Point;
use burnburn_io::Project;
use burnburn_job::{
    JobPlanBuilder, JobPlanId, MachinePoint, MachineProfile, OperationSettings, PlanEvent, PlanMode, PlanSource,
    PowerRatio,
};
use burnburn_machine::{ActorError, ExecutionState};
use burnburn_ruida::lower;
use burnburn_types::{CanonicalWriter, ContentHash, DocumentId, HashDomain, OperationId, RevisionId, StableId};

use crate::camera_workspace::{CameraWhy, MachineWhy, MarksConfirm, MarksProposalView};
use crate::production::{
    ActionId, ActionSubject, Dispatched, JobInputs, MachineAction, MachineError, MachineIntent, PreparedJob, Role,
};

use super::AppController;
use super::camera_wizards::{AlignWizard, Mark};

/// Length of each arm of a mark's cross, from the mark's centre.
const ARM_MM: f64 = 3.0;
const COMPILER_VERSION: &str = "alignment-marks-1";
const JOB_NAME: &str = "Uitlijnmarkeringen";

/// A prepared marks job and what it was built from.
pub(super) struct Proposal {
    pub(super) job: Arc<PreparedJob>,
    pub(super) marks_mm: Vec<[f64; 2]>,
    pub(super) power_percent: f64,
    pub(super) speed_mm_s: f64,
    /// The job was handed to the machine service.
    pub(super) sent: bool,
}

/// The operation id of the marks block: not a document operation (the frame job uses `u64::MAX`).
fn marks_operation() -> OperationId {
    OperationId::from_raw(u64::MAX - 1)
}

fn why_of(e: &MachineError) -> MachineWhy {
    match e {
        MachineError::Actor(ActorError::IdentityNotConfigured) => MachineWhy::IdentityNotConfigured,
        MachineError::Actor(ActorError::Busy) => MachineWhy::Busy,
        MachineError::StateChanged { .. } => MachineWhy::StateChanged,
        MachineError::NotRecorded(_) => MachineWhy::NotRecorded,
        MachineError::WrongMachine => MachineWhy::WrongMachine,
        _ => MachineWhy::Other,
    }
}

fn percent(text: &str) -> Result<f64, CameraWhy> {
    match crate::field::parse_number(text, &["%"]) {
        Ok(v) if v.is_finite() && v > 0.0 && v <= 100.0 => Ok(v),
        _ => Err(CameraWhy::InvalidNumber),
    }
}

fn speed(text: &str) -> Result<f64, CameraWhy> {
    match crate::field::parse_number(text, &["mm/s"]) {
        Ok(v) if v.is_finite() && v > 0.0 => Ok(v),
        _ => Err(CameraWhy::InvalidNumber),
    }
}

impl AppController {
    pub(super) fn propose_marks_job(&mut self, power_text: &str, speed_text: &str) -> Result<(), CameraWhy> {
        let machine = self.machine.clone().ok_or(CameraWhy::NoMachineProfile)?;
        let power = percent(power_text)?;
        let speed_mm_s = speed(speed_text)?;
        if speed_mm_s > machine.profile.max_speed_mm_s() {
            return Err(CameraWhy::SpeedTooHigh);
        }
        let uuid = (self.ports.new_project_uuid)();
        let wizard = self.camera_ws.align.as_mut().ok_or(CameraWhy::NoAlignmentWizard)?;
        let marks_mm: Vec<[f64; 2]> = wizard.marks.iter().map(|m: &Mark| m.bed_mm).collect();
        let plan = marks_plan(&machine.profile, &marks_mm, power, speed_mm_s)?;
        let program = lower(&plan, &machine.ruida).map_err(|_| CameraWhy::MarksJobFailed)?;
        let rd = program.to_rd_file(machine.ruida.magic).map_err(|_| CameraWhy::MarksJobFailed)?;
        let plan = Arc::new(plan);
        // The history freezes the project with the job; a marks job belongs to no document.
        let empty = DocumentSession::new(DocumentId::from_raw(u64::MAX)).snapshot();
        let project = Project::new(uuid, (*empty).clone());
        wizard.proposal = Some(Proposal {
            job: Arc::new(PreparedJob {
                plan: plan.id(),
                plan_revision: RevisionId::INITIAL,
                profile: plan.source().machine_profile.clone(),
                rd_hash: ContentHash::of(HashDomain::Rd, &rd),
                rd: Arc::from(rd),
                name: JOB_NAME.to_owned(),
                inputs: Some(Arc::new(JobInputs { plan, project: Arc::new(project), project_path: None })),
            }),
            marks_mm,
            power_percent: power,
            speed_mm_s,
            sent: false,
        });
        wizard.error = None;
        Ok(())
    }

    pub(super) fn confirm_marks_job(
        &mut self,
        plan: JobPlanId,
        action: ActionId,
        seen: Option<ExecutionState>,
    ) -> Result<(), CameraWhy> {
        let wizard = self.camera_ws.align.as_ref().ok_or(CameraWhy::NoAlignmentWizard)?;
        let proposal = wizard.proposal.as_ref().filter(|p| p.job.plan == plan).ok_or(CameraWhy::NotProposed)?;
        if action.kind != MachineAction::Upload || action.subject != ActionSubject::Job(plan) {
            return Err(CameraWhy::NotProposed);
        }
        if self.role != Role::Designer {
            return Err(CameraWhy::NotAllowedForRole);
        }
        let port = self.machine_port.clone().ok_or(CameraWhy::NoMachine)?;
        let job = Arc::clone(&proposal.job);
        let intent = MachineIntent {
            action,
            machine: port.status().machine,
            job: Some(job),
            required_state: seen,
            order: None,
            role: self.role,
        };
        let sent = match port.dispatch(intent) {
            Ok(Dispatched::Sent(_)) => Ok(()),
            Ok(Dispatched::AlreadyHandled(done)) => done.result.map(|_| ()),
            Err(e) => Err(e),
        };
        match sent {
            Ok(()) => {
                if let Some(p) = self.camera_ws.align.as_mut().and_then(|w| w.proposal.as_mut()) {
                    p.sent = true;
                }
                Ok(())
            }
            Err(e) => Err(CameraWhy::MachineRefused(why_of(&e))),
        }
    }

    /// The proposal as the panel shows it, with the confirmation the machine allows right now.
    pub(super) fn proposal_view(&self, wizard: &AlignWizard) -> Option<MarksProposalView> {
        let p = wizard.proposal.as_ref()?;
        let plan = p.job.plan;
        let (confirm, why_not) = match self.machine_port.as_ref() {
            None => (None, Some(CameraWhy::NoMachine)),
            Some(port) => {
                let status = port.status();
                let state = status.current.as_ref().map(|c| c.execution.state.clone());
                if state.as_ref().is_some_and(|s| !s.is_terminal()) {
                    (None, Some(CameraWhy::MachineRefused(MachineWhy::Busy)))
                } else if self.role != Role::Designer {
                    (None, Some(CameraWhy::NotAllowedForRole))
                } else {
                    let action = status.action_id(MachineAction::Upload, ActionSubject::Job(plan));
                    (Some(MarksConfirm { action, seen: state }), None)
                }
            }
        };
        Some(MarksProposalView {
            plan,
            name: p.job.name.clone(),
            marks_mm: p.marks_mm.clone(),
            power_percent: p.power_percent,
            speed_mm_s: p.speed_mm_s,
            confirm,
            confirm_why_not: why_not,
            sent: p.sent,
        })
    }
}

/// One cross per mark: a horizontal and a vertical stroke centred on it, laser on only along the
/// strokes. Every end of every stroke must lie on the bed.
fn marks_plan(
    profile: &MachineProfile,
    marks_mm: &[[f64; 2]],
    power_percent: f64,
    speed_mm_s: f64,
) -> Result<burnburn_job::JobPlan, CameraWhy> {
    let frame = *profile.frame();
    let bed = |x: f64, y: f64| -> Result<MachinePoint, CameraWhy> {
        let p = frame.to_machine(Point::new(x, y)).map_err(|_| CameraWhy::MarksOutsideBed)?;
        frame.contains(p).then_some(p).ok_or(CameraWhy::MarksOutsideBed)
    };
    let mut strokes: Vec<(MachinePoint, MachinePoint)> = Vec::new();
    for &[x, y] in marks_mm {
        strokes.push((bed(x - ARM_MM, y)?, bed(x + ARM_MM, y)?));
        strokes.push((bed(x, y - ARM_MM)?, bed(x, y + ARM_MM)?));
    }
    let start = strokes.first().map(|s| s.0).ok_or(CameraWhy::MarksIncomplete)?;
    let mut canonical = CanonicalWriter::new();
    canonical.tag("alignment-marks-v1");
    for (a, b) in &strokes {
        for v in [a.x_mm(), a.y_mm(), b.x_mm(), b.y_mm()] {
            canonical.f64(v);
        }
    }
    canonical.f64(power_percent);
    canonical.f64(speed_mm_s);
    let source = PlanSource {
        document_revision: RevisionId::INITIAL,
        source_hash: ContentHash::of(
            HashDomain::PlanSource,
            &canonical.finish().map_err(|_| CameraWhy::MarksJobFailed)?,
        ),
        machine_profile: profile.revision().clone(),
        compiler_version: COMPILER_VERSION.to_owned(),
    };
    let build = || -> Result<burnburn_job::JobPlan, burnburn_job::PlanError> {
        let mut b = JobPlanBuilder::new(source.clone(), frame, start);
        let power = PowerRatio::new(power_percent / 100.0)?;
        let settings = b.add_settings(OperationSettings {
            mode: PlanMode::Line,
            speed_mm_s,
            min_power: power,
            max_power: power,
            air_assist: false,
        })?;
        b.push(PlanEvent::OperationBoundary { operation: marks_operation(), settings, pass: 0 })?;
        for (from, to) in &strokes {
            b.push(PlanEvent::Travel { to: *from })?;
            b.push(PlanEvent::Cut { to: *to, origin: None })?;
        }
        b.finish()
    };
    build().map_err(|_| CameraWhy::MarksJobFailed)
}
