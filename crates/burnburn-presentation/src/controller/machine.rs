//! Machine service, production orders and the operator view (IM31): everything that talks to the
//! machine port or shows its status. Never touches a document.

use std::sync::Arc;

use burnburn_job::{JobPlanId, MachineProfile};
use burnburn_machine::{Evidence, ExecutionId, ExecutionState, FailReason, UnknownReason};
use burnburn_ruida::RuidaProfile;
use burnburn_types::{ContentHash, DocumentId};

use crate::production::{
    ActionId, ActionSubject, Dispatched, HistoryStore, HistoryStoreError, MachineAction, MachineError, MachineIntent,
    MachinePort, MachineStatus, OrderDraft, OrderId, Outcome, PreparedJob, Role, StoredOrder, file_may_remain,
    reconcilable,
};
use crate::view_model::{
    MachineButton, MachineStatusKey, MachineViewModel, MessageKey, OperatorJobRow, OperatorViewModel, OrderViewModel,
};

use super::{AppController, AppUpdate, Session, error};

/// A frozen job and whether the designer released it for the operator.
#[derive(Clone)]
pub(super) struct Prepared {
    pub(super) job: Arc<PreparedJob>,
    pub(super) approved: bool,
}

/// An order (IM31) with the explicitly chosen job, if any.
#[derive(Clone)]
pub(super) struct Order {
    pub(super) id: OrderId,
    pub(super) draft: OrderDraft,
    /// Hash of the chosen `JobPlanId` (so an order from the history can name it).
    pub(super) chosen: Option<ContentHash>,
}

/// The active machine (B11): profile for compiling, controller dialect for RD.
#[derive(Clone)]
pub(super) struct Machine {
    pub(super) profile: Arc<MachineProfile>,
    pub(super) ruida: RuidaProfile,
}

impl AppController {
    /// Connects the machine service. Kept out of `AppPorts` so hosts without a machine are unchanged.
    pub fn attach_machine(&mut self, port: Arc<dyn MachinePort>) {
        self.machine_port = Some(port);
    }

    /// Connects the production history (orders and their executions are kept there, IM31) and
    /// loads what it holds.
    pub fn attach_history(&mut self, history: Arc<dyn HistoryStore>) -> Result<(), HistoryStoreError> {
        let stored = history.orders()?;
        self.orders = stored.orders.into_iter().map(|o| Order { id: o.id, draft: o.draft, chosen: o.chosen }).collect();
        self.order_executions = stored.executions.into_iter().collect();
        self.history = Some(history);
        Ok(())
    }
}

impl AppController {
    /// Makes an order change durable before it is shown (no history: kept in memory only).
    pub(super) fn store_order(&self, order: &Order) -> Result<(), AppUpdate> {
        let Some(history) = self.history.as_ref() else { return Ok(()) };
        let stored = StoredOrder { id: order.id, draft: order.draft.clone(), chosen: order.chosen };
        history.record_order(&stored).map_err(|_| error(None, MessageKey::IntentNotRecorded))
    }
}

impl AppController {
    /// Role of the person at this controller (from configuration, never from a `UiIntent`).
    pub fn set_role(&mut self, role: Role) {
        self.role = role;
    }

    pub fn role(&self) -> Role {
        self.role
    }

    /// Compact operator view (IM31): approved jobs, machine status and orders; no editor functions.
    pub fn operator_view(&self) -> OperatorViewModel {
        let status = self.machine_port.as_ref().map(|p| p.status());
        let jobs = self
            .sessions
            .iter()
            .filter_map(|(id, s)| s.prepared.as_ref().filter(|p| p.approved).map(|p| (*id, p)))
            .map(|(document, p)| OperatorJobRow {
                document,
                name: p.job.name.clone(),
                plan: p.job.plan,
                rd_hash: p.job.rd_hash,
                upload: status.as_ref().map(|st| self.upload_button(st, p)),
            })
            .collect();
        let orders = self.orders.iter().map(|o| self.order_view(o, status.as_ref())).collect();
        OperatorViewModel { machine: status.as_ref().map(machine_view_of), jobs, orders }
    }

    /// Machine panel: status from evidence, controls with their idempotency keys.
    pub fn machine_view(&self) -> Option<MachineViewModel> {
        self.machine_port.as_ref().map(|p| machine_view_of(&p.status()))
    }

    pub(super) fn upload_button(&self, status: &MachineStatus, p: &Prepared) -> MachineButton {
        let state = status.current.as_ref().map(|c| c.execution.state.clone());
        let idle = state.as_ref().is_none_or(ExecutionState::is_terminal);
        MachineButton {
            action: status.action_id(MachineAction::Upload, ActionSubject::Job(p.job.plan)),
            enabled: idle && (self.role == Role::Designer || p.approved),
            seen: state,
            label: MessageKey::MachineAction(MachineAction::Upload),
        }
    }

    /// Approved prepared jobs whose name is the order's part, one per plan.
    pub(super) fn candidates(&self, part: &str) -> Vec<&Prepared> {
        let mut out: Vec<&Prepared> = Vec::new();
        for p in self.sessions.values().filter_map(|s| s.prepared.as_ref()) {
            if p.approved && p.job.name == part && !out.iter().any(|q| q.job.plan == p.job.plan) {
                out.push(p);
            }
        }
        out
    }

    pub(super) fn order_job(&self, order: &Order) -> Option<&Prepared> {
        let candidates = self.candidates(&order.draft.part);
        match (order.chosen, candidates.as_slice()) {
            (Some(plan), _) => candidates.into_iter().find(|p| p.job.plan.hash() == plan),
            (None, [only]) => Some(*only),
            _ => None,
        }
    }

    pub(super) fn order_view(&self, o: &Order, status: Option<&MachineStatus>) -> OrderViewModel {
        let candidates: Vec<JobPlanId> = self.candidates(&o.draft.part).iter().map(|p| p.job.plan).collect();
        let job = self.order_job(o);
        let completed = status.map_or(0, |st| {
            st.executions()
                .filter(|r| r.execution.state == ExecutionState::Completed)
                .filter(|r| self.order_executions.get(&r.execution.id) == Some(&o.id))
                .count()
        });
        OrderViewModel {
            id: o.id,
            name: o.draft.name.clone(),
            part: o.draft.part.clone(),
            quantity: o.draft.quantity,
            completed: u32::try_from(completed).unwrap_or(u32::MAX),
            remnant: o.draft.remnant.clone(),
            job: job.map(|p| p.job.plan),
            needs_choice: job.is_none() && candidates.len() > 1,
            candidates,
            upload: job.zip(status).map(|(p, st)| self.upload_button(st, p)),
        }
    }

    /// The prepared job an upload action names: in `document` if given, else in any window.
    pub(super) fn job_for(&self, document: Option<DocumentId>, plan: JobPlanId) -> Option<&Prepared> {
        let mut sessions: Box<dyn Iterator<Item = &Session>> = match document {
            Some(d) => Box::new(self.sessions.get(&d).into_iter()),
            None => Box::new(self.sessions.values()),
        };
        sessions.find_map(|s| s.prepared.as_ref().filter(|p| p.job.plan == plan))
    }

    /// Sends one machine action to the machine service. Never touches a document.
    pub(super) fn machine_action(
        &mut self,
        document: Option<DocumentId>,
        action: ActionId,
        seen: Option<ExecutionState>,
        order: Option<OrderId>,
    ) -> (AppUpdate, Option<ExecutionId>) {
        let Some(port) = self.machine_port.clone() else {
            return (error(document, MessageKey::MachineActionRefused), None);
        };
        let job = match (action.kind, action.subject) {
            (MachineAction::Upload, ActionSubject::Job(plan)) => match self.job_for(document, plan) {
                Some(p) if self.role == Role::Operator && !p.approved => {
                    return (error(document, MessageKey::NotAllowedForRole), None);
                }
                Some(p) => Some(Arc::clone(&p.job)),
                None => return (error(document, MessageKey::MachineActionRefused), None),
            },
            _ => None,
        };
        let machine = port.status().machine;
        let intent = MachineIntent { action, machine, job, required_state: seen, order, role: self.role };
        let changed = AppUpdate { view_changed: true, ..AppUpdate::default() };
        match port.dispatch(intent) {
            Ok(Dispatched::Sent(execution)) => (changed, execution),
            Ok(Dispatched::AlreadyHandled(_)) => (changed, None),
            Err(MachineError::NotRecorded(_)) => (error(document, MessageKey::IntentNotRecorded), None),
            Err(MachineError::AlreadyRecorded) => (changed, None),
            Err(_) => (error(document, MessageKey::MachineActionRefused), None),
        }
    }
}

/// Machine panel from the service status. Status and controls follow evidence only.
pub(super) fn machine_view_of(status: &MachineStatus) -> MachineViewModel {
    let current = status.current.as_ref();
    let mut buttons = Vec::new();
    let mut seen = None;
    if let Some(record) = current {
        let e = &record.execution;
        let subject = ActionSubject::Execution(e.id);
        seen = Some(e.state.clone());
        let live = !e.state.is_terminal();
        let mut add = |kind: MachineAction, enabled: bool| {
            buttons.push(MachineButton {
                action: status.action_id(kind, subject),
                enabled,
                seen: Some(e.state.clone()),
                label: MessageKey::MachineAction(kind),
            });
        };
        add(MachineAction::ConfirmStored, e.state == ExecutionState::UploadAcked);
        add(MachineAction::Start, e.state == ExecutionState::Stored && !e.start_sent);
        add(MachineAction::QueryStatus, live);
        add(MachineAction::Cancel, live);
        if matches!(e.state, ExecutionState::Unknown(_)) {
            for outcome in Outcome::ALL {
                add(MachineAction::RecordOutcome(outcome), reconcilable(e));
            }
        }
    }
    buttons.push(MachineButton {
        action: status.action_id(MachineAction::Reconnect, ActionSubject::Machine),
        enabled: true,
        seen,
        label: MessageKey::MachineAction(MachineAction::Reconnect),
    });
    let (status_key, message, progress) = match current.map(|r| &r.execution) {
        None => (MachineStatusKey::Idle, None, None),
        Some(e) => match &e.state {
            ExecutionState::VerifyingIdentity => (MachineStatusKey::VerifyingIdentity, None, None),
            ExecutionState::Reverifying => (MachineStatusKey::Reverifying, None, None),
            ExecutionState::Uploading { acked, total } => (MachineStatusKey::Uploading, None, Some((*acked, *total))),
            ExecutionState::UploadAcked => {
                let total = current.map_or(0, |r| {
                    r.execution.evidence.iter().filter(|x| matches!(x.evidence, Evidence::ChunkAcked { .. })).count()
                });
                (
                    MachineStatusKey::UploadedNotConfirmed,
                    Some(MessageKey::UploadedNotStoredConfirmed),
                    Some((total, total)),
                )
            }
            ExecutionState::Stored => (MachineStatusKey::Stored, None, None),
            ExecutionState::StartRequested => {
                (MachineStatusKey::StartRequested, Some(MessageKey::StartRequestedAwaitingStatus), None)
            }
            ExecutionState::Running => (MachineStatusKey::Running, None, None),
            ExecutionState::Completed => (MachineStatusKey::Completed, None, None),
            ExecutionState::Unknown(reason) => (
                MachineStatusKey::Unknown,
                Some(match reason {
                    UnknownReason::StopRequested => MessageKey::StopSentOutcomeUnknown,
                    UnknownReason::UploadChunkUnconfirmed { .. } => MessageKey::UploadUnconfirmedCheckController,
                    _ => MessageKey::StatusUnknownCheckMachine,
                }),
                None,
            ),
            ExecutionState::Failed(reason) => (
                MachineStatusKey::Failed,
                Some(match reason {
                    FailReason::UploadRejected { .. } => MessageKey::UploadRejected,
                    FailReason::NotStored => MessageKey::NotStored,
                    FailReason::IdentityMismatch => MessageKey::IdentityMismatch,
                    FailReason::IdentityUnconfirmed => MessageKey::IdentityUnconfirmed,
                    _ => MessageKey::ControllerFault,
                }),
                None,
            ),
            ExecutionState::Cancelled => {
                (MachineStatusKey::Cancelled, file_may_remain(e).then_some(MessageKey::ControllerFileMayRemain), None)
            }
        },
    };
    MachineViewModel {
        machine: status.machine.clone(),
        status: status_key,
        message,
        execution: current.map(|r| r.execution.id),
        job: current.map(|r| r.job.plan),
        job_name: current.map(|r| r.job.name.clone()),
        progress,
        buttons,
        last_action: status.last_action.as_ref().map(|h| (h.action, h.result.is_ok())),
    }
}
