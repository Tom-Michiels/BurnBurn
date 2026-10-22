//! The confirmation before a start (M06): `RequestStart` opens it for the stored job that was not
//! started, `ConfirmStart` sends the one start the confirmation was about, `CancelStart` closes it.
//! The confirmation follows the execution it was opened for: when that execution is not stored and
//! unstarted any more, it is gone and confirming it sends nothing.

use burnburn_machine::{ExecutionId, ExecutionState};
use burnburn_types::DocumentId;

use super::machine::plate_material;
use super::{AppController, AppUpdate, error};
use crate::production::{ActionId, ActionSubject, ExecutionRecord, MachineAction, MachineStatus};
use crate::start_confirm::{START_NEEDS, StartConfirmViewModel, StartEvidence};
use crate::view_model::MessageKey;

/// A start the user asked for and has not confirmed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PendingStart {
    pub(super) execution: ExecutionId,
}

/// The execution that a start may still go to: stored, and no start sent for it.
fn startable(status: &MachineStatus) -> Option<&ExecutionRecord> {
    status.current.as_ref().filter(|r| r.execution.state == ExecutionState::Stored && !r.execution.start_sent)
}

impl AppController {
    /// The confirmation on screen, if there is one and it is still about a startable execution.
    pub(super) fn start_confirm_view(&self) -> Option<StartConfirmViewModel> {
        let pending = self.pending_start.as_ref()?;
        let status = self.machine_port.as_ref()?.status();
        let record = startable(&status).filter(|r| r.execution.id == pending.execution)?;
        let job = &record.job;
        let placed_from_mm = self
            .job_or_released(None, job.plan)
            .and_then(|p| p.placed_from)
            .map(|(x, y)| (x as f64 / 1000.0, y as f64 / 1000.0));
        let declared = self.machine.as_ref().map(|m| m.declared.clone()).unwrap_or_default();
        Some(StartConfirmViewModel {
            job_name: job.name.clone(),
            plan: job.plan,
            revision: job.plan_revision,
            profile_id: job.profile.id.clone(),
            profile_revision: job.profile.revision,
            machine: status.machine.to_string(),
            declared_model: declared.model,
            declared_firmware: declared.firmware,
            material: job.inputs.as_ref().and_then(|i| plate_material(&i.project.document)),
            bounds: job.inputs.as_ref().and_then(|i| i.plan.bounds()),
            placed_from_mm,
            evidence: StartEvidence::NotProven { needs: START_NEEDS },
            action: status.action_id(MachineAction::Start, ActionSubject::Execution(record.execution.id)),
            seen: Some(record.execution.state.clone()),
        })
    }

    /// The Start control was used: opens the confirmation; nothing is sent.
    pub(super) fn request_start(&mut self, document: Option<DocumentId>) -> AppUpdate {
        let Some(port) = self.machine_port.clone() else {
            return error(document, MessageKey::MachineActionRefused);
        };
        let status = port.status();
        match startable(&status) {
            Some(record) => {
                self.pending_start = Some(PendingStart { execution: record.execution.id });
                AppUpdate { view_changed: true, ..AppUpdate::default() }
            }
            None => {
                self.pending_start = None;
                error(document, MessageKey::MachineActionRefused)
            }
        }
    }

    /// The user confirmed what is on screen: exactly one start for that execution.
    pub(super) fn confirm_start(
        &mut self,
        document: Option<DocumentId>,
        action: ActionId,
        seen: Option<ExecutionState>,
    ) -> AppUpdate {
        let Some(pending) = self.pending_start.take() else {
            return error(document, MessageKey::MachineActionRefused);
        };
        let about_it =
            action.kind == MachineAction::Start && action.subject == ActionSubject::Execution(pending.execution);
        let current = self.machine_port.as_ref().is_some_and(|p| {
            let status = p.status();
            startable(&status).is_some_and(|r| r.execution.id == pending.execution)
        });
        if !(about_it && current) {
            return error(document, MessageKey::MachineActionRefused);
        }
        self.machine_action(document, action, seen, None).0
    }

    /// Closes the confirmation without starting.
    pub(super) fn cancel_start(&mut self) -> AppUpdate {
        self.pending_start = None;
        AppUpdate { view_changed: true, ..AppUpdate::default() }
    }
}
