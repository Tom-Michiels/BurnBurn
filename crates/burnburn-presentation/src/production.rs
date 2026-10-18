//! Production flow (gate C, IM31): prepared jobs, machine intents with idempotency keys and the
//! machine service that owns the one connection to a machine.
//!
//! Machine actions never enter document history. Every action carries an [`ActionId`] derived
//! from what the user saw (action kind, subject, generation), so a double click, a second window or
//! a resend after a reconnect is a no-op that shows the earlier outcome. The service records the
//! intent through [`HistoryStore`] *before* it reaches the actor, and status shown to the user
//! follows evidence only (ACK ≠ Stored ≠ Completed).

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use burnburn_io::Project;
use burnburn_job::{JobPlan, JobPlanId, ProfileRevision};
use burnburn_machine::{
    ActorError, ControllerPort, ControllerStatus, Evidence, Execution, ExecutionActor, ExecutionId, ExecutionState,
    FailReason, MachineId, RdArtifact, UnknownReason,
};
use burnburn_types::{ContentHash, RevisionId};

/// A frozen job revision ready for the machine: exactly the plan the preview showed and the RD
/// bytes an export of that plan writes. Never changed by later edits (W01).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedJob {
    pub plan: JobPlanId,
    /// Document revision the plan was compiled from.
    pub plan_revision: RevisionId,
    pub profile: ProfileRevision,
    pub rd_hash: ContentHash,
    pub rd: Arc<[u8]>,
    /// Document title when the job was prepared (operator list, order matching).
    pub name: String,
    /// What the history freezes with the job (plan and project snapshot). `None` for a job
    /// rebuilt from the history, which is frozen already.
    pub inputs: Option<Arc<JobInputs>>,
}

/// The compiled plan and the project snapshot a job was prepared from (frozen as blobs).
#[derive(Debug, Clone)]
pub struct JobInputs {
    pub plan: Arc<JobPlan>,
    pub project: Arc<Project>,
    /// Where the project was saved, if it was (shown with the job in the history).
    pub project_path: Option<PathBuf>,
}

/// Two inputs are the same when they freeze the same plan (the plan id is a content hash).
impl PartialEq for JobInputs {
    fn eq(&self, other: &Self) -> bool {
        self.plan.id() == other.plan.id() && self.project_path == other.project_path
    }
}

impl Eq for JobInputs {}

/// Identity of one order (IM31), allocated by the controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OrderId(pub u64);

/// A new order: `quantity` pieces of `part` (matched against approved job names), optionally on a
/// named remnant plate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderDraft {
    pub name: String,
    pub part: String,
    pub quantity: u32,
    pub remnant: Option<String>,
}

/// Who works at a controller (IM31). An operator runs approved, prepared jobs and edits nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Designer,
    Operator,
}

/// What a machine control does. Each is its own intent; upload never implies start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum MachineAction {
    Upload,
    /// Read back whether the uploaded job is stored (idempotent query).
    ConfirmStored,
    Start,
    QueryStatus,
    /// Before a start: local only. After a start: one software stop, outcome stays unknown.
    Cancel,
    /// Re-establish the transport. Never resends anything.
    Reconnect,
    /// The operator states what happened to an execution whose outcome is unknown, after a
    /// status query showed the controller is not running (reconciliation; never automatic).
    RecordOutcome(Outcome),
}

/// What the operator saw happen to a job whose outcome is unknown. Only `RanToCompletion`
/// counts as a produced piece.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Outcome {
    RanToCompletion,
    DidNotRun,
    Partial,
}

impl Outcome {
    pub const ALL: [Outcome; 3] = [Outcome::RanToCompletion, Outcome::DidNotRun, Outcome::Partial];

    /// The terminal state the execution takes once the statement is recorded.
    pub fn state(self) -> ExecutionState {
        match self {
            Outcome::RanToCompletion => ExecutionState::Completed,
            Outcome::DidNotRun => ExecutionState::Failed(FailReason::OperatorDidNotRun),
            Outcome::Partial => ExecutionState::Failed(FailReason::OperatorPartial),
        }
    }
}

/// What an action is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ActionSubject {
    Job(JobPlanId),
    Execution(ExecutionId),
    Machine,
}

/// Idempotency key of one user action (contract IM31). Derived from the shared machine status, so
/// every window offers the same id for the same button; `generation` counts earlier handled
/// actions of this kind on this subject.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ActionId {
    pub kind: MachineAction,
    pub subject: ActionSubject,
    pub generation: u32,
}

/// One machine action as sent by presentation to the machine service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineIntent {
    pub action: ActionId,
    pub machine: MachineId,
    /// For [`MachineAction::Upload`]: the prepared job to upload.
    pub job: Option<Arc<PreparedJob>>,
    /// The execution state the user saw when choosing the action; the service refuses the action
    /// when the machine has moved on (stale button, other window).
    pub required_state: Option<ExecutionState>,
    /// For [`MachineAction::Upload`]: the order this piece is made for (linked durably).
    pub order: Option<OrderId>,
    /// Who acts (recorded with an outcome statement).
    pub role: Role,
}

/// What [`HistoryStore::record_intent`] persists before an action is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentRecord {
    pub action: ActionId,
    pub machine: MachineId,
    pub execution: ExecutionId,
    pub plan: JobPlanId,
    pub rd_hash: ContentHash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryStoreError(pub String);

/// Whether a record was new. `AlreadyRecorded`: the same [`ActionId`] is in the history (also
/// from before a restart) — the action must not be sent again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recorded {
    Fresh,
    AlreadyRecorded,
}

/// A new execution for an upload: the job is frozen, the execution created (and linked to its
/// order) and the upload intent recorded, in that order, before any byte is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadRecord {
    pub action: ActionId,
    pub machine: MachineId,
    pub job: Arc<PreparedJob>,
    pub order: Option<OrderId>,
}

/// The operator's statement about an execution whose outcome is unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutcomeRecord {
    pub action: ActionId,
    pub execution: ExecutionId,
    pub outcome: Outcome,
    pub role: Role,
}

/// One execution as the history knows it after a restart. Non-terminal executions come back
/// as `Unknown`; nothing in them is ever resumed automatically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveredExecution {
    pub execution: Execution,
    pub job: Arc<PreparedJob>,
    pub order: Option<OrderId>,
}

/// What a machine service needs from the history when it starts (presentation read model).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecoveredMachine {
    /// Executions of this machine, oldest first.
    pub executions: Vec<RecoveredExecution>,
    /// How many actions of each kind were recorded per subject, so a button never reuses the id
    /// of an action from before the restart.
    pub handled: Vec<((MachineAction, ActionSubject), u32)>,
}

/// One order as stored (IM31). `chosen` is the hash of the explicitly chosen `JobPlanId`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredOrder {
    pub id: OrderId,
    pub draft: OrderDraft,
    pub chosen: Option<ContentHash>,
}

/// Orders and the executions made for them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoredOrders {
    pub orders: Vec<StoredOrder>,
    pub executions: Vec<(ExecutionId, OrderId)>,
}

/// Port to the production history (ADR 0008; implemented over `burnburn-history` by runtime as
/// `JournalHistoryStore`). Every `record_*` returns only once the record is durable; an error
/// means: do not send the action.
pub trait HistoryStore: Send + Sync {
    /// Freezes the job (once), creates the execution and records the upload intent. Returns the
    /// execution id allocated by the history.
    fn record_upload(&self, record: &UploadRecord) -> Result<(ExecutionId, Recorded), HistoryStoreError>;

    /// Records a start or stop intent for an existing execution.
    fn record_intent(&self, record: &IntentRecord) -> Result<Recorded, HistoryStoreError>;

    /// Records an outcome statement (reconciliation of an unknown execution).
    fn record_outcome(&self, record: &OutcomeRecord) -> Result<Recorded, HistoryStoreError>;

    /// Journals what the actor observed (new evidence, state).
    fn record_observed(&self, execution: &Execution) -> Result<(), HistoryStoreError> {
        let _ = execution;
        Ok(())
    }

    /// Executions and action counts of `machine` (after a restart).
    fn recover(&self, machine: &MachineId) -> Result<RecoveredMachine, HistoryStoreError> {
        let _ = machine;
        Ok(RecoveredMachine::default())
    }

    /// Records a new or changed order.
    fn record_order(&self, order: &StoredOrder) -> Result<(), HistoryStoreError> {
        let _ = order;
        Ok(())
    }

    fn orders(&self) -> Result<StoredOrders, HistoryStoreError> {
        Ok(StoredOrders::default())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum MachineError {
    /// The intent names another machine than the one this service owns.
    WrongMachine,
    /// The machine is no longer in the state the user saw.
    StateChanged {
        expected: Option<ExecutionState>,
        actual: Option<ExecutionState>,
    },
    /// Upload without a prepared job, or an action without an execution.
    NothingToAct,
    /// The intent could not be recorded; nothing was sent.
    NotRecorded(HistoryStoreError),
    /// The operator role may only upload approved jobs.
    NotApproved,
    /// The current role may not perform this action.
    NotAllowed,
    /// The history already holds this action (e.g. from before a restart): nothing was sent.
    AlreadyRecorded,
    /// An outcome can only be stated for an unknown execution after a status query showed the
    /// controller is not running.
    NotReconcilable,
    Actor(ActorError),
}

impl fmt::Display for MachineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for MachineError {}

/// Result of one handled action, kept per [`ActionId`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandledAction {
    pub action: ActionId,
    /// The execution acted on (none for a reconnect without execution).
    pub result: Result<Option<ExecutionId>, MachineError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dispatched {
    /// The action was handed to the actor (it may still fail on the wire; status shows evidence).
    Sent(Option<ExecutionId>),
    /// The same action was handled before; nothing was sent again.
    AlreadyHandled(HandledAction),
}

/// One execution with the job it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionRecord {
    pub execution: Execution,
    pub job: Arc<PreparedJob>,
}

/// Snapshot of what one machine service knows, for view models.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineStatus {
    pub machine: MachineId,
    /// The execution the actor drives (possibly terminal).
    pub current: Option<ExecutionRecord>,
    /// Earlier executions of this service, oldest first.
    pub earlier: Vec<ExecutionRecord>,
    pub last_action: Option<HandledAction>,
    /// The last failure to journal observed evidence (the machine state shown stays evidence-based).
    pub journal_error: Option<HistoryStoreError>,
    handled: BTreeMap<(MachineAction, ActionSubject), u32>,
}

impl MachineStatus {
    /// The id a button for `kind` on `subject` carries now.
    pub fn action_id(&self, kind: MachineAction, subject: ActionSubject) -> ActionId {
        let generation = self.handled.get(&(kind, subject)).copied().unwrap_or(0);
        ActionId { kind, subject, generation }
    }

    /// Current and earlier executions.
    pub fn executions(&self) -> impl Iterator<Item = &ExecutionRecord> {
        self.earlier.iter().chain(self.current.as_ref())
    }
}

/// The one owner of a machine connection, shared by every window and the operator view.
pub trait MachinePort: Send + Sync {
    fn dispatch(&self, intent: MachineIntent) -> Result<Dispatched, MachineError>;
    fn status(&self) -> MachineStatus;
}

/// Machine service over one [`ExecutionActor`]. Step-driven: the runtime (or a test) calls
/// [`MachineService::poll`] to process replies and timeouts.
pub struct MachineService<P: ControllerPort> {
    inner: Mutex<Inner<P>>,
}

struct Inner<P: ControllerPort> {
    actor: ExecutionActor<P>,
    history: Arc<dyn HistoryStore>,
    chunk_bytes: usize,
    job: Option<Arc<PreparedJob>>,
    journal_error: Option<HistoryStoreError>,
    earlier: Vec<ExecutionRecord>,
    handled: BTreeMap<ActionId, HandledAction>,
    counts: BTreeMap<(MachineAction, ActionSubject), u32>,
    last: Option<HandledAction>,
}

impl<P: ControllerPort> MachineService<P> {
    /// `chunk_bytes` is the upload chunk size (the transport's payload limit).
    ///
    /// Rebuilds what the history knows about this machine first (ADR 0008 §5): the latest
    /// unfinished execution is adopted as `Unknown` — only a status query or an outcome
    /// statement can follow — and earlier ones are listed. Nothing is sent.
    pub fn new(
        mut actor: ExecutionActor<P>,
        history: Arc<dyn HistoryStore>,
        chunk_bytes: usize,
    ) -> Result<Self, HistoryStoreError> {
        let recovered = history.recover(&actor.machine().id)?;
        let mut earlier = Vec::new();
        let mut job = None;
        let unfinished = recovered.executions.iter().rposition(|r| !r.execution.state.is_terminal());
        for (i, r) in recovered.executions.into_iter().enumerate() {
            let mut execution = r.execution;
            if !execution.state.is_terminal() && !matches!(execution.state, ExecutionState::Unknown(_)) {
                execution.state = ExecutionState::Unknown(UnknownReason::InterruptedByRestart);
            }
            if Some(i) == unfinished {
                actor.resume_unknown(execution).map_err(|e| HistoryStoreError(format!("resume: {e}")))?;
                job = Some(r.job);
            } else {
                earlier.push(ExecutionRecord { execution, job: r.job });
            }
        }
        Ok(MachineService {
            inner: Mutex::new(Inner {
                actor,
                history,
                chunk_bytes: chunk_bytes.max(1),
                job,
                journal_error: None,
                earlier,
                handled: BTreeMap::new(),
                counts: recovered.handled.into_iter().collect(),
                last: None,
            }),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Inner<P>> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Processes replies and timeouts of the actor and journals what it observed.
    pub fn poll(&self) -> Result<(), ActorError> {
        let mut s = self.lock();
        let result = s.actor.poll();
        s.journal();
        result
    }

    /// Read access to the transport (tests inspect their fake).
    pub fn with_port<R>(&self, f: impl FnOnce(&P) -> R) -> R {
        f(self.lock().actor.port())
    }
}

impl<P: ControllerPort + Send> MachinePort for MachineService<P> {
    fn dispatch(&self, intent: MachineIntent) -> Result<Dispatched, MachineError> {
        let mut s = self.lock();
        if let Some(done) = s.handled.get(&intent.action) {
            return Ok(Dispatched::AlreadyHandled(done.clone()));
        }
        if intent.machine != s.actor.machine().id {
            return Err(MachineError::WrongMachine);
        }
        let kind = intent.action.kind;
        let actual = s.actor.execution().map(|e| e.state.clone());
        // Non-idempotent actions act only on the state the user saw.
        if matches!(
            kind,
            MachineAction::Upload | MachineAction::Start | MachineAction::Cancel | MachineAction::RecordOutcome(_)
        ) && intent.required_state != actual
        {
            return Err(MachineError::StateChanged { expected: intent.required_state, actual });
        }
        let current = s.actor.execution().map(|e| e.id);
        let subject_ok = match (kind, intent.action.subject) {
            (MachineAction::Upload, ActionSubject::Job(plan)) => intent.job.as_ref().is_some_and(|j| j.plan == plan),
            (MachineAction::Reconnect, ActionSubject::Machine) => true,
            (MachineAction::Upload | MachineAction::Reconnect, _) => false,
            (_, ActionSubject::Execution(id)) => current == Some(id),
            _ => false,
        };
        if !subject_ok {
            return Err(MachineError::NothingToAct);
        }
        let result = s.perform(&intent);
        s.journal();
        if let Err(MachineError::NotRecorded(_)) = &result {
            // Nothing was sent: the same click may be tried again.
            return result.map(Dispatched::Sent);
        }
        let handled = HandledAction { action: intent.action, result: result.clone() };
        s.handled.insert(intent.action, handled.clone());
        *s.counts.entry((kind, intent.action.subject)).or_insert(0) += 1;
        s.last = Some(handled);
        result.map(Dispatched::Sent)
    }

    fn status(&self) -> MachineStatus {
        let s = self.lock();
        let current = match (s.actor.execution(), s.job.as_ref()) {
            (Some(e), Some(job)) => Some(ExecutionRecord { execution: e.clone(), job: Arc::clone(job) }),
            _ => None,
        };
        MachineStatus {
            machine: s.actor.machine().id.clone(),
            current,
            earlier: s.earlier.clone(),
            last_action: s.last.clone(),
            journal_error: s.journal_error.clone(),
            handled: s.counts.clone(),
        }
    }
}

impl<P: ControllerPort> Inner<P> {
    /// Intent before action (ADR 0008 §5): returns only after the history holds the record.
    fn record(&self, action: ActionId, execution: ExecutionId, job: &PreparedJob) -> Result<(), MachineError> {
        let record = IntentRecord {
            action,
            machine: self.actor.machine().id.clone(),
            execution,
            plan: job.plan,
            rd_hash: job.rd_hash,
        };
        match self.history.record_intent(&record).map_err(MachineError::NotRecorded)? {
            Recorded::Fresh => Ok(()),
            Recorded::AlreadyRecorded => Err(MachineError::AlreadyRecorded),
        }
    }

    /// Journals the actor's view of the current execution (best effort; failures are shown).
    fn journal(&mut self) {
        if let Some(e) = self.actor.execution() {
            self.journal_error = self.history.record_observed(e).err();
        }
    }

    fn perform(&mut self, intent: &MachineIntent) -> Result<Option<ExecutionId>, MachineError> {
        let action = intent.action;
        let current = self.actor.execution().map(|e| e.id);
        let actor = |r: Result<(), ActorError>| r.map_err(MachineError::Actor);
        match action.kind {
            MachineAction::Upload => {
                let job = intent.job.clone().ok_or(MachineError::NothingToAct)?;
                if self.actor.execution().is_some_and(|e| !e.state.is_terminal()) {
                    return Err(MachineError::Actor(ActorError::Busy));
                }
                let artifact = RdArtifact::from_rd_file(job.plan, &job.rd, self.chunk_bytes)
                    .map_err(|_| MachineError::NothingToAct)?;
                let record = UploadRecord {
                    action,
                    machine: self.actor.machine().id.clone(),
                    job: Arc::clone(&job),
                    order: intent.order,
                };
                let (id, recorded) = self.history.record_upload(&record).map_err(MachineError::NotRecorded)?;
                if recorded == Recorded::AlreadyRecorded {
                    return Err(MachineError::AlreadyRecorded);
                }
                let previous = self.actor.execution().cloned().zip(self.job.clone());
                actor(self.actor.begin_upload(id, &artifact))?;
                if let Some((execution, job)) = previous {
                    self.earlier.push(ExecutionRecord { execution, job });
                }
                self.job = Some(job);
                Ok(Some(id))
            }
            MachineAction::ConfirmStored => actor(self.actor.confirm_stored()).map(|()| current),
            MachineAction::Start => {
                let (Some(id), Some(job)) = (current, self.job.clone()) else { return Err(MachineError::NothingToAct) };
                self.record(action, id, &job)?;
                actor(self.actor.start()).map(|()| current)
            }
            MachineAction::QueryStatus => actor(self.actor.query_status()).map(|()| current),
            MachineAction::Cancel => {
                let (Some(id), Some(job)) = (current, self.job.clone()) else { return Err(MachineError::NothingToAct) };
                self.record(action, id, &job)?;
                actor(self.actor.cancel()).map(|()| current)
            }
            MachineAction::Reconnect => actor(self.actor.reconnect()).map(|()| current),
            MachineAction::RecordOutcome(outcome) => {
                let Some(e) = self.actor.execution() else { return Err(MachineError::NothingToAct) };
                if !reconcilable(e) {
                    return Err(MachineError::NotReconcilable);
                }
                let record = OutcomeRecord { action, execution: e.id, outcome, role: intent.role };
                match self.history.record_outcome(&record).map_err(MachineError::NotRecorded)? {
                    Recorded::Fresh => actor(self.actor.settle_unknown(outcome.state())).map(|()| current),
                    Recorded::AlreadyRecorded => Err(MachineError::AlreadyRecorded),
                }
            }
        }
    }
}

/// An outcome may be stated only for an unknown execution whose latest status reply says the
/// controller is not running (so the statement describes a finished situation).
pub(crate) fn reconcilable(execution: &Execution) -> bool {
    matches!(execution.state, ExecutionState::Unknown(_))
        && execution
            .evidence
            .iter()
            .rev()
            .find_map(|e| match e.evidence {
                Evidence::Status(s) => Some(s),
                _ => None,
            })
            .is_some_and(|s| s != ControllerStatus::Running)
}

/// Did the controller report that a (partial) job file may remain after a cancel?
pub(crate) fn file_may_remain(execution: &Execution) -> bool {
    execution.evidence.iter().any(|e| e.evidence == Evidence::ControllerFileMayRemain)
}
