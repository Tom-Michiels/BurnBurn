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
    /// The head position (µm) the plan was placed on ("start from current position"); uploading
    /// is refused once the head is known to be elsewhere or its position is unknown.
    pub(super) placed_from: Option<(i64, i64)>,
    /// The production record the job's variable text was resolved against: the counters commit
    /// for exactly this record on the chosen job event (W3-UI-tekst).
    pub(super) record: Option<burnburn_generators::ProductionRecord>,
}

/// The names of the parts (the drawn objects) of the project a job was prepared from; none for a job
/// without its project in memory.
fn part_names(job: &PreparedJob) -> Vec<String> {
    let Some(inputs) = job.inputs.as_ref() else { return Vec::new() };
    let snapshot = &inputs.project.document;
    snapshot.draw_order().into_iter().filter_map(|id| snapshot.object(id)).map(|o| o.name().to_owned()).collect()
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
    /// The GRBL backend's configuration; `None` is a Ruida machine (G5-grbl-ui).
    pub(super) grbl: Option<burnburn_gcode::GrblProfile>,
    /// What the user declared about the controller (M01): the identity the check before an upload
    /// compares with. Never filled from a controller's reply.
    pub(super) declared: Declared,
}

/// The model and firmware the user declared for a machine; `None` is not declared.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Declared {
    pub(super) model: Option<String>,
    pub(super) firmware: Option<String>,
}

impl Declared {
    /// Blank text is not a declaration.
    pub(super) fn of(model: &str, firmware: &str) -> Declared {
        let clean = |t: &str| Some(t.trim()).filter(|t| !t.is_empty()).map(str::to_owned);
        Declared { model: clean(model), firmware: clean(firmware) }
    }

    pub(super) fn is_none(&self) -> bool {
        self.model.is_none() && self.firmware.is_none()
    }
}

/// What of the machine in use was last handed to the file writer, to see when it changes (M01).
/// The profile is compared by identity of its `Arc`: a changed profile is a new one.
#[derive(Debug, Clone)]
pub(super) struct ActiveSeen {
    profile: Arc<MachineProfile>,
    grbl: Option<burnburn_gcode::GrblProfile>,
    declared: Declared,
}

impl ActiveSeen {
    pub(super) fn of(machine: &Machine) -> ActiveSeen {
        ActiveSeen {
            profile: Arc::clone(&machine.profile),
            grbl: machine.grbl.clone(),
            declared: machine.declared.clone(),
        }
    }

    /// True when `machine` is what was seen.
    pub(super) fn is(&self, machine: &Machine) -> bool {
        Arc::ptr_eq(&self.profile, &machine.profile) && self.grbl == machine.grbl && self.declared == machine.declared
    }
}

/// What the profile library keeps next to a machine's profile.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct Extras {
    pub(super) grbl: Option<burnburn_gcode::GrblProfile>,
    pub(super) declared: Declared,
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
        self.released_jobs = history.approved_jobs()?;
        self.approved_plans = self.released_jobs.iter().map(|j| j.plan.hash()).collect();
        self.job_parts = self.released_jobs.iter().map(|j| (j.plan.hash(), history.job_parts(j.plan))).collect();
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
    /// The designer releases the prepared job of `document` for the operator (W01). With a
    /// history the release is made durable first; a release that cannot be kept is not given.
    pub(super) fn approve_job(&mut self, document: DocumentId) -> AppUpdate {
        let Some(job) = self.sessions.get(&document).and_then(|s| s.prepared.as_ref()).map(|p| Arc::clone(&p.job))
        else {
            return AppUpdate::default();
        };
        if let Some(history) = self.history.as_ref()
            && history.record_approval(&job, self.role).is_err()
        {
            return error(Some(document), MessageKey::IntentNotRecorded);
        }
        self.approved_plans.insert(job.plan.hash());
        self.job_parts.insert(job.plan.hash(), part_names(&job));
        if !self.released_jobs.iter().any(|j| j.plan == job.plan) {
            self.released_jobs.push(Arc::clone(&job));
        }
        if let Some(p) = self.sessions.get_mut(&document).and_then(|s| s.prepared.as_mut()) {
            p.approved = true;
        }
        AppUpdate { view_changed: true, ..AppUpdate::default() }
    }

    /// Released jobs that no window holds (they come from the history), as prepared jobs.
    fn stored_released(&self) -> impl Iterator<Item = Prepared> + '_ {
        self.released_jobs
            .iter()
            .filter(|j| !self.sessions.values().filter_map(|s| s.prepared.as_ref()).any(|p| p.job.plan == j.plan))
            .map(|j| Prepared { job: Arc::clone(j), approved: true, placed_from: None, record: None })
    }

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
        let mut jobs: Vec<OperatorJobRow> = self
            .sessions
            .iter()
            .filter_map(|(id, s)| s.prepared.as_ref().filter(|p| p.approved).map(|p| (*id, p)))
            .map(|(document, p)| OperatorJobRow {
                document: Some(document),
                name: p.job.name.clone(),
                revision: p.job.plan_revision,
                bounds: p.job.inputs.as_ref().and_then(|i| i.plan.bounds()),
                material: p.job.inputs.as_ref().and_then(|i| plate_material(&i.project.document)),
                plan: p.job.plan,
                rd_hash: p.job.rd_hash,
                upload: status.as_ref().map(|st| self.upload_button(st, p)),
            })
            .collect();
        // Released jobs from the history that no window holds: the operator needs no project open.
        jobs.extend(self.stored_released().map(|p| OperatorJobRow {
            document: None,
            name: p.job.name.clone(),
            revision: p.job.plan_revision,
            bounds: p.job.inputs.as_ref().and_then(|i| i.plan.bounds()),
            material: p.job.inputs.as_ref().and_then(|i| plate_material(&i.project.document)),
            plan: p.job.plan,
            rd_hash: p.job.rd_hash,
            upload: status.as_ref().map(|st| self.upload_button(st, &p)),
        }));
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

    /// Does the job hold the part an order names? The file name of the project is the job's name; the
    /// names of the drawn objects in it are its parts, compared without case and surrounding spaces.
    fn holds_part(&self, job: &PreparedJob, part: &str) -> bool {
        if job.name == part {
            return true;
        }
        let wanted = part.trim().to_lowercase();
        if wanted.is_empty() {
            return false;
        }
        let from_inputs;
        let names = if job.inputs.is_some() {
            from_inputs = part_names(job);
            &from_inputs
        } else {
            match self.job_parts.get(&job.plan.hash()) {
                Some(names) => names,
                None => return false,
            }
        };
        names.iter().any(|n| n.trim().to_lowercase() == wanted)
    }

    /// Approved prepared jobs that are the order's part (by the file name or the name of a part in
    /// them), one per plan: the jobs of the open windows first, then the released jobs only the
    /// history holds (no window needed, W01/W02).
    pub(super) fn candidates(&self, part: &str) -> Vec<Prepared> {
        let mut out: Vec<Prepared> = Vec::new();
        let released = self.stored_released().filter(|p| self.holds_part(&p.job, part));
        for p in self
            .sessions
            .values()
            .filter_map(|s| s.prepared.as_ref())
            .filter(|p| p.approved && self.holds_part(&p.job, part))
        {
            if !out.iter().any(|q| q.job.plan == p.job.plan) {
                out.push(p.clone());
            }
        }
        for p in released {
            if !out.iter().any(|q| q.job.plan == p.job.plan) {
                out.push(p);
            }
        }
        out
    }

    pub(super) fn order_job(&self, order: &Order) -> Option<Prepared> {
        let candidates = self.candidates(&order.draft.part);
        match (order.chosen, candidates.len()) {
            (Some(plan), _) => candidates.into_iter().find(|p| p.job.plan.hash() == plan),
            (None, 1) => candidates.into_iter().next(),
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
            remnant_link: self.remnant_link(o, job.as_ref()),
            job: job.as_ref().map(|p| p.job.plan),
            needs_choice: job.is_none() && candidates.len() > 1,
            candidates,
            upload: job.as_ref().zip(status).map(|(p, st)| self.upload_button(st, p)),
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

    /// The prepared job an upload action names, also when only the history holds it: a released
    /// job after a restart is a job without a window (`document` is `None`).
    pub(super) fn job_or_released(&self, document: Option<DocumentId>, plan: JobPlanId) -> Option<Prepared> {
        self.job_for(document, plan).cloned().or_else(|| {
            if document.is_some() {
                return None;
            }
            self.stored_released().find(|p| p.job.plan == plan)
        })
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
            (MachineAction::Upload, ActionSubject::Job(plan)) => match self.job_or_released(document, plan) {
                Some(p) if self.role == Role::Operator && !p.approved => {
                    return (error(document, MessageKey::NotAllowedForRole), None);
                }
                Some(p) if p.placed_from.is_some_and(|at| !position_is(&port.status(), at)) => {
                    return (error(document, MessageKey::LaserRefused(crate::laser::LaserWhy::PlacementStale)), None);
                }
                Some(p) => Some(Arc::clone(&p.job)),
                None => return (error(document, MessageKey::MachineActionRefused), None),
            },
            _ => None,
        };
        self.sync_declared_identity(&port);
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

impl AppController {
    /// Hands the identity the user declared for the active machine to the port, so the check
    /// before an upload compares the controller's reply with it (M01). Nothing declared (and
    /// nothing handed over before) leaves the port as the host configured it. A port that is busy
    /// with an execution keeps its identity for that execution; the next action hands it over again.
    fn sync_declared_identity(&mut self, port: &Arc<dyn MachinePort>) {
        let declared = self.machine.as_ref().map(|m| m.declared.clone()).unwrap_or_default();
        if declared.is_none() && !self.declared_pushed {
            return;
        }
        if port.declare_identity(declared.model.clone(), declared.firmware.clone()).is_ok() {
            self.declared_pushed = !declared.is_none();
        }
    }
}

/// Is the head known to be at `(x, y)` micrometres? An unknown position is never a match.
fn position_is(status: &MachineStatus, (x, y): (i64, i64)) -> bool {
    status.live.position.x.um() == Some(x) && status.live.position.y.um() == Some(y)
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

/// The material of the plates of a project when there is one and they all agree; a mix of
/// materials (or none named) says nothing.
pub(super) fn plate_material(document: &burnburn_document::DocumentSnapshot) -> Option<String> {
    let mut materials =
        document.structure().plates().map(|(_, p)| p.material.trim().to_owned()).filter(|m| !m.is_empty());
    let first = materials.next()?;
    materials.all(|m| m == first).then_some(first)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use burnburn_document::structure::{Plate, StructureCommand};
    use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
    use burnburn_types::{DocumentId, Length, RequestId, StableId};

    fn with_plates(materials: &[&str]) -> DocumentSession {
        let mut s = DocumentSession::new(DocumentId::from_raw(1));
        for (i, m) in materials.iter().enumerate() {
            let plate = Plate {
                name: format!("Plaat {i}"),
                width: Length::from_mm(600.0).unwrap(),
                height: Length::from_mm(400.0).unwrap(),
                material: (*m).to_owned(),
                thickness: None,
            };
            let cmd = DocumentCommand {
                document: s.id(),
                expected: s.revision(),
                request: RequestId::from_raw(i as u64 + 1),
                payload: CommandPayload::Structure(StructureCommand::CreatePlate { plate }),
            };
            s.apply(cmd).unwrap();
        }
        s
    }

    #[test]
    fn the_material_is_named_only_when_the_plates_agree() {
        assert_eq!(plate_material(&with_plates(&["Berken", "Berken"]).snapshot()), Some("Berken".to_owned()));
        assert_eq!(plate_material(&with_plates(&["Berken", "Multiplex"]).snapshot()), None, "a mix says nothing");
        assert_eq!(plate_material(&with_plates(&[]).snapshot()), None);
        assert_eq!(plate_material(&with_plates(&["", "  "]).snapshot()), None, "no material named");
        assert_eq!(plate_material(&with_plates(&["Berken", ""]).snapshot()), Some("Berken".to_owned()));
    }
}
