//! `burnburn_presentation::HistoryStore` over the append-only production history
//! (`burnburn-history`, ADR 0008). Presentation never names history types; this adapter maps
//! presentation's records and read models onto journal records.
//!
//! - An upload freezes the job (plan, RD, project blobs) once, creates the execution, links its
//!   order and records the upload intent — all durable before the service sends a byte.
//! - Presentation action ids are mapped to history action ids by a stable hash, so the same
//!   button pressed again (also after a restart) is never a fresh intent.
//! - Evidence the actor observes is journaled from the position this session last wrote, also
//!   for an execution adopted after a restart.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use burnburn_history::{
    ActionId as HActionId, CompileRecord, CompiledJobId, EvidenceDto, ExecutionStateDto, History, HistoryRecord,
    IntentKind, JournalFs, OrderRecord, PreflightSummary, ReconcileStatement, Role as HRole, WallTime,
};
use burnburn_machine::{Execution, ExecutionId, ExecutionState, MachineId, UnknownReason};
use burnburn_presentation::{
    ActionId, ActionSubject, HistoryStore, HistoryStoreError, IntentRecord, MachineAction, OrderDraft, OrderId,
    Outcome, OutcomeRecord, PreparedJob, Recorded, RecoveredExecution, RecoveredMachine, Role, StoredOrder,
    StoredOrders, UploadRecord,
};
use burnburn_types::{ContentHash, HashDomain};

use crate::driver::WallClock;

/// The production history as presentation's [`HistoryStore`].
pub struct JournalHistoryStore {
    inner: Mutex<Inner>,
}

struct Inner {
    history: History,
    fs: Arc<dyn JournalFs>,
    wall: Arc<dyn WallClock>,
    /// Actor evidence entries journaled in this session, per execution.
    journaled: BTreeMap<ExecutionId, usize>,
}

impl JournalHistoryStore {
    /// `history` as returned by `History::open` over `fs` (unfinished executions are `Unknown`).
    pub fn new(history: History, fs: Arc<dyn JournalFs>, wall: Arc<dyn WallClock>) -> Self {
        JournalHistoryStore { inner: Mutex::new(Inner { history, fs, wall, journaled: BTreeMap::new() }) }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Read access to the history (diagnostics, tests).
    pub fn with_history<R>(&self, f: impl FnOnce(&History) -> R) -> R {
        f(&self.lock().history)
    }
}

fn store_error(e: impl std::fmt::Display) -> HistoryStoreError {
    HistoryStoreError(e.to_string())
}

/// Stable history action id of a presentation action: hash of kind, subject and generation.
fn history_action(action: &ActionId) -> HActionId {
    let mut bytes = b"burnburn-presentation-action-1".to_vec();
    let (code, outcome) = match action.kind {
        MachineAction::Upload => (1u8, 0u8),
        MachineAction::ConfirmStored => (2, 0),
        MachineAction::Start => (3, 0),
        MachineAction::QueryStatus => (4, 0),
        MachineAction::Cancel => (5, 0),
        MachineAction::Reconnect => (6, 0),
        MachineAction::RecordOutcome(o) => (7, outcome_code(o)),
        _ => (255, 0),
    };
    bytes.extend_from_slice(&[code, outcome]);
    match action.subject {
        ActionSubject::Job(plan) => {
            bytes.push(1);
            bytes.extend_from_slice(plan.hash().as_bytes());
        }
        ActionSubject::Execution(id) => {
            bytes.push(2);
            bytes.extend_from_slice(&id.raw().to_be_bytes());
        }
        ActionSubject::Machine => bytes.push(3),
    }
    bytes.extend_from_slice(&action.generation.to_be_bytes());
    let hash = ContentHash::of(HashDomain::HistoryRecord, &bytes);
    let mut raw = [0u8; 16];
    raw.copy_from_slice(&hash.as_bytes()[..16]);
    HActionId::from_raw(u128::from_be_bytes(raw))
}

fn outcome_code(o: Outcome) -> u8 {
    match o {
        Outcome::RanToCompletion => 1,
        Outcome::DidNotRun => 2,
        Outcome::Partial => 3,
    }
}

fn statement(o: Outcome) -> ReconcileStatement {
    match o {
        Outcome::RanToCompletion => ReconcileStatement::RanToCompletion,
        Outcome::DidNotRun => ReconcileStatement::DidNotRun,
        Outcome::Partial => ReconcileStatement::Partial,
    }
}

impl Inner {
    fn now(&self) -> WallTime {
        self.wall.now()
    }

    fn recorded(&self, action: &ActionId) -> Option<ExecutionId> {
        self.history.action_outcome(history_action(action)).map(|o| o.execution)
    }

    /// The frozen job of an execution as presentation's `PreparedJob` (RD and plan from blobs).
    fn prepared(&self, job: CompiledJobId) -> Result<PreparedJob, HistoryStoreError> {
        let fs = self.fs.as_ref();
        let compiled = self.history.job(job).ok_or_else(|| store_error("unknown job"))?;
        let plan = self.history.load_plan(fs, job).map_err(store_error)?;
        let rd = self.history.load_rd(fs, job).map_err(store_error)?;
        let name = compiled
            .project_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map_or_else(|| "job".to_owned(), |n| n.to_string_lossy().into_owned());
        Ok(PreparedJob {
            plan: plan.id(),
            plan_revision: plan.source().document_revision,
            profile: compiled.profile.clone(),
            rd_hash: compiled.rd_hash,
            rd: Arc::from(rd),
            name,
            inputs: None,
        })
    }
}

impl HistoryStore for JournalHistoryStore {
    fn record_upload(&self, record: &UploadRecord) -> Result<(ExecutionId, Recorded), HistoryStoreError> {
        let mut s = self.lock();
        if let Some(execution) = s.recorded(&record.action) {
            return Ok((execution, Recorded::AlreadyRecorded));
        }
        let inputs = record.job.inputs.as_ref().ok_or_else(|| store_error("job has no frozen inputs"))?;
        let (at, fs) = (s.now(), Arc::clone(&s.fs));
        let compile = CompileRecord {
            plan: &inputs.plan,
            rd_file: &record.job.rd,
            project: &inputs.project,
            project_path: inputs.project_path.clone(),
            // A prepared job never has preflight errors (presentation refuses to prepare it).
            preflight: PreflightSummary::default(),
            profile: &record.job.profile,
        };
        let job = s.history.record_compiled(fs.as_ref(), compile, at).map_err(store_error)?;
        // The job is prepared for its own setup revision; a changed machine setup is a new compile.
        let execution = s
            .history
            .create_execution(fs.as_ref(), job, record.machine.clone(), None, &record.job.profile, at)
            .map_err(store_error)?;
        if let Some(order) = record.order {
            s.history.link_order(fs.as_ref(), order.0, execution, at).map_err(store_error)?;
        }
        s.history
            .record_intent(fs.as_ref(), execution, history_action(&record.action), IntentKind::Upload, at)
            .map_err(store_error)?;
        s.journaled.insert(execution, 0);
        Ok((execution, Recorded::Fresh))
    }

    fn record_intent(&self, record: &IntentRecord) -> Result<Recorded, HistoryStoreError> {
        let mut s = self.lock();
        let intent = match record.action.kind {
            MachineAction::Start => IntentKind::Start,
            MachineAction::Cancel => IntentKind::Stop,
            MachineAction::QueryStatus => IntentKind::QueryStatus,
            MachineAction::ConfirmStored => IntentKind::ConfirmStored,
            _ => return Err(store_error("not an execution intent")),
        };
        let (at, fs) = (s.now(), Arc::clone(&s.fs));
        let receipt = s
            .history
            .record_intent(fs.as_ref(), record.execution, history_action(&record.action), intent, at)
            .map_err(store_error)?;
        Ok(if receipt.fresh { Recorded::Fresh } else { Recorded::AlreadyRecorded })
    }

    fn record_outcome(&self, record: &OutcomeRecord) -> Result<Recorded, HistoryStoreError> {
        let mut s = self.lock();
        if s.recorded(&record.action).is_some() {
            return Ok(Recorded::AlreadyRecorded);
        }
        let role = match record.role {
            Role::Designer => HRole::Designer,
            Role::Operator => HRole::Operator,
        };
        let (at, fs) = (s.now(), Arc::clone(&s.fs));
        s.history
            .reconcile_by_operator(
                fs.as_ref(),
                record.execution,
                role,
                statement(record.outcome),
                history_action(&record.action),
                at,
            )
            .map_err(store_error)?;
        Ok(Recorded::Fresh)
    }

    fn record_observed(&self, execution: &Execution) -> Result<(), HistoryStoreError> {
        let mut s = self.lock();
        let Some(view) = s.history.execution(execution.id) else { return Err(store_error("unknown execution")) };
        let recorded_state = view.state.clone();
        let written = s.journaled.get(&execution.id).copied().unwrap_or(0);
        let (at, fs) = (s.now(), Arc::clone(&s.fs));
        for e in execution.evidence.iter().skip(written) {
            let record = HistoryRecord::Evidence { execution: execution.id, evidence: EvidenceDto::from(&e.evidence) };
            s.history.append(fs.as_ref(), record, at).map_err(store_error)?;
            *s.journaled.entry(execution.id).or_insert(0) += 1;
        }
        let state = ExecutionStateDto::from(&execution.state);
        if state != recorded_state {
            let record = HistoryRecord::StateObserved { execution: execution.id, state };
            s.history.append(fs.as_ref(), record, at).map_err(store_error)?;
        }
        Ok(())
    }

    fn recover(&self, machine: &MachineId) -> Result<RecoveredMachine, HistoryStoreError> {
        let mut s = self.lock();
        let views: Vec<_> = s.history.executions_for_machine(machine).cloned().collect();
        let mut out = RecoveredMachine::default();
        let mut handled: BTreeMap<(MachineAction, ActionSubject), u32> = BTreeMap::new();
        for v in views {
            let job = Arc::new(s.prepared(v.job)?);
            let state = v.state.to_state().unwrap_or(ExecutionState::Unknown(UnknownReason::InterruptedByRestart));
            for i in &v.intents {
                let key = match i.intent {
                    IntentKind::Upload => (MachineAction::Upload, ActionSubject::Job(job.plan)),
                    IntentKind::Start => (MachineAction::Start, ActionSubject::Execution(v.id)),
                    IntentKind::Stop => (MachineAction::Cancel, ActionSubject::Execution(v.id)),
                    _ => continue,
                };
                *handled.entry(key).or_insert(0) += 1;
            }
            // The adopted execution starts with no actor evidence: journal from there on.
            s.journaled.insert(v.id, 0);
            out.executions.push(RecoveredExecution {
                execution: Execution {
                    id: v.id,
                    machine: v.machine.clone(),
                    plan: job.plan,
                    rd_hash: job.rd_hash,
                    state,
                    evidence: Vec::new(),
                    start_sent: v.intent_recorded(IntentKind::Start),
                },
                order: s.history.order_of(v.id).map(OrderId),
                job,
            });
        }
        out.handled = handled.into_iter().collect();
        Ok(out)
    }

    fn record_order(&self, order: &StoredOrder) -> Result<(), HistoryStoreError> {
        let mut s = self.lock();
        let record = OrderRecord {
            order: order.id.0,
            name: order.draft.name.clone(),
            part: order.draft.part.clone(),
            quantity: order.draft.quantity,
            remnant: order.draft.remnant.clone(),
            chosen_plan: order.chosen,
        };
        let (at, fs) = (s.now(), Arc::clone(&s.fs));
        s.history.record_order(fs.as_ref(), record, at).map(|_| ()).map_err(store_error)
    }

    fn orders(&self) -> Result<StoredOrders, HistoryStoreError> {
        let s = self.lock();
        let orders = s
            .history
            .orders()
            .map(|o| StoredOrder {
                id: OrderId(o.order),
                draft: OrderDraft {
                    name: o.name.clone(),
                    part: o.part.clone(),
                    quantity: o.quantity,
                    remnant: o.remnant.clone(),
                },
                chosen: o.chosen_plan,
            })
            .collect();
        let executions =
            s.history.executions().filter_map(|e| s.history.order_of(e.id).map(|o| (e.id, OrderId(o)))).collect();
        Ok(StoredOrders { orders, executions })
    }
}
