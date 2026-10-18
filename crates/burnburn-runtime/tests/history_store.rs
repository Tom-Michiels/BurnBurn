//! `JournalHistoryStore`: presentation's machine service over the real append-only history on
//! io's MemoryFileSystem, with a scripted fake controller and a fake clock. A "restart" drops
//! service and store and reopens the journal from the same file system. No socket, no laser.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use burnburn_document::DocumentSession;
use burnburn_history::{
    ExecutionStateDto, FailReasonDto, History, JournalFs, JournalLimits, ReconcileBasis, ReconcileStatement,
    Role as HRole, WallTime,
};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{Project, ProjectUuid};
use burnburn_job::{
    JobPlan, JobPlanBuilder, MachineFrame, MachinePoint, OperationSettings, OriginCorner, PlanEvent, PlanMode,
    PlanSource, PowerRatio, ProfileRevision,
};
use burnburn_machine::{
    ControllerIdentity, ControllerPort, ControllerStatus, Envelope, ExecutionActor, ExecutionState, Incoming,
    MachineId, MachineIdentity, Policy, PortError, Reply, Request, RequestTag, UnknownReason,
};
use burnburn_presentation::{
    ActionSubject, Dispatched, HistoryStore, JobInputs, MachineAction, MachineError, MachineIntent, MachinePort,
    MachineService, OrderDraft, OrderId, Outcome, PreparedJob, Role, StoredOrder,
};
use burnburn_runtime::{JournalHistoryStore, WallClock};
use burnburn_test_support::{FakeClock, ScriptedPeer, Step};
use burnburn_types::{ContentHash, DocumentId, HashDomain, OperationId, RevisionId, StableId};

type Peer = ScriptedPeer<Request, Reply>;

const MACHINE: &str = "bench-ruida-1";
const MODEL: &str = "RDC6445G";
const TIMEOUT: Duration = Duration::from_millis(500);
const RD: [u8; 5] = [0xd8, 0x10, 0x89, 0x41, 0x42];
const CHUNK: usize = 2;

struct FakePort {
    peer: Peer,
    machine: MachineId,
    last: Option<RequestTag>,
}

impl ControllerPort for FakePort {
    fn send(&mut self, e: Envelope) -> Result<(), PortError> {
        self.last = Some(e.tag);
        self.peer.send(e.request).map_err(|e| PortError::Other(e.to_string()))
    }
    fn try_recv(&mut self) -> Result<Option<Incoming>, PortError> {
        let r = self.peer.recv().map_err(|e| PortError::Other(e.to_string()))?;
        Ok(r.and_then(|reply| self.last.map(|tag| Incoming { from: self.machine.clone(), tag, reply })))
    }
    fn reconnect(&mut self) -> Result<(), PortError> {
        self.peer.reconnect();
        Ok(())
    }
}

struct FixedWall;
impl WallClock for FixedWall {
    fn now(&self) -> WallTime {
        WallTime::from_unix_millis(1_792_000_000_000)
    }
}

fn dir() -> PathBuf {
    PathBuf::from("/app-data/history")
}

fn setup() -> ProfileRevision {
    ProfileRevision { id: "bench".into(), revision: 1, content: ContentHash::of(HashDomain::Profile, b"bench") }
}

fn plan() -> JobPlan {
    let source = PlanSource {
        document_revision: RevisionId::INITIAL,
        source_hash: ContentHash::of(HashDomain::Plan, b"history-store-test"),
        machine_profile: setup(),
        compiler_version: "test".into(),
    };
    let mut b = JobPlanBuilder::new(
        source,
        MachineFrame::new(100.0, 100.0, OriginCorner::BottomLeft).unwrap(),
        MachinePoint::new(0.0, 0.0).unwrap(),
    );
    let s = b
        .add_settings(OperationSettings {
            mode: PlanMode::Line,
            speed_mm_s: 10.0,
            min_power: PowerRatio::new(0.5).unwrap(),
            max_power: PowerRatio::new(0.5).unwrap(),
            air_assist: false,
        })
        .unwrap();
    b.push(PlanEvent::OperationBoundary { operation: OperationId::from_raw(1), settings: s, pass: 0 }).unwrap();
    b.push(PlanEvent::Cut { to: MachinePoint::new(10.0, 0.0).unwrap(), origin: None }).unwrap();
    b.finish().unwrap()
}

fn job() -> Arc<PreparedJob> {
    let plan = Arc::new(plan());
    let snapshot = (*DocumentSession::new(DocumentId::from_raw(1)).snapshot()).clone();
    let project = Project::new(ProjectUuid::from_bytes([1; 16]), snapshot);
    Arc::new(PreparedJob {
        plan: plan.id(),
        plan_revision: plan.source().document_revision,
        profile: setup(),
        rd_hash: burnburn_machine::rd_hash(&RD),
        rd: Arc::from(&RD[..]),
        name: "plaat".into(),
        inputs: Some(Arc::new(JobInputs {
            plan,
            project: Arc::new(project),
            project_path: Some(PathBuf::from("/work/plaat.burnburn")),
        })),
    })
}

/// One application run: the journal reopened from `fs`, a store and a service over it.
struct Run {
    store: Arc<JournalHistoryStore>,
    service: MachineService<FakePort>,
}

fn start_app(fs: &Arc<MemoryFileSystem>, clock: &FakeClock, peer: &Peer) -> Run {
    let (history, _) = History::open(fs.as_ref(), &dir(), &JournalLimits::default()).unwrap();
    let journal: Arc<dyn JournalFs> = Arc::clone(fs) as Arc<dyn JournalFs>;
    let store = Arc::new(JournalHistoryStore::new(history, journal, Arc::new(FixedWall)));
    let machine = MachineId::new(MACHINE);
    let port = FakePort { peer: peer.clone(), machine: machine.clone(), last: None };
    let identity = MachineIdentity { id: machine, model: Some(MODEL.into()), firmware: None };
    let policy = Policy { reply_timeout: TIMEOUT, max_query_attempts: 2 };
    let actor = ExecutionActor::new(identity, port, Arc::new(clock.clone()), policy);
    let service = MachineService::new(actor, store.clone(), CHUNK).unwrap();
    Run { store, service }
}

struct Rig {
    fs: Arc<MemoryFileSystem>,
    clock: FakeClock,
    peer: Peer,
}

fn rig() -> Rig {
    let clock = FakeClock::new();
    Rig { fs: Arc::new(MemoryFileSystem::new()), peer: Peer::new(clock.clone()), clock }
}

fn act(
    run: &Run,
    kind: MachineAction,
    job: Option<Arc<PreparedJob>>,
    order: Option<OrderId>,
) -> Result<Dispatched, MachineError> {
    let st = run.service.status();
    let subject = match (kind, &job, st.current.as_ref()) {
        (MachineAction::Upload, Some(j), _) => ActionSubject::Job(j.plan),
        (MachineAction::Reconnect, _, _) => ActionSubject::Machine,
        (_, _, Some(c)) => ActionSubject::Execution(c.execution.id),
        _ => ActionSubject::Machine,
    };
    let intent = MachineIntent {
        action: st.action_id(kind, subject),
        machine: MachineId::new(MACHINE),
        job,
        required_state: st.current.as_ref().map(|c| c.execution.state.clone()),
        order,
        role: Role::Operator,
    };
    let out = run.service.dispatch(intent);
    for _ in 0..16 {
        run.service.poll().unwrap();
    }
    out
}

fn upload_store_start(r: &Rig, run: &Run, order: Option<OrderId>) {
    let identity = ControllerIdentity { model: Some(MODEL.into()), firmware: None };
    r.peer.script(Step::expect(Request::QueryIdentity).reply(Reply::Identity(identity)));
    for i in 0..RD.len().div_ceil(CHUNK) {
        r.peer.script(
            Step::expect_matching(
                format!("chunk {i}"),
                move |q| matches!(q, Request::UploadChunk { index, .. } if *index == i),
            )
            .reply(Reply::Ack),
        );
    }
    r.peer.script(Step::expect(Request::QueryStored).reply(Reply::Stored(true)));
    r.peer.script(Step::expect(Request::Start).reply(Reply::Ack));
    act(run, MachineAction::Upload, Some(job()), order).unwrap();
    act(run, MachineAction::ConfirmStored, None, None).unwrap();
    act(run, MachineAction::Start, None, None).unwrap();
    assert_eq!(run.service.status().current.unwrap().execution.state, ExecutionState::StartRequested);
}

fn starts(peer: &Peer) -> usize {
    peer.sent().iter().filter(|q| **q == Request::Start).count()
}

/// ADR 0008 §5 over the real journal: after a restart the started execution is Unknown, names
/// the frozen job (RD from the blob), offers no start, and nothing is resent.
#[test]
fn restart_marks_unfinished_execution_unknown_without_resending_start() {
    let r = rig();
    let first = start_app(&r.fs, &r.clock, &r.peer);
    upload_store_start(&r, &first, None);
    let sent = r.peer.sent().len();
    drop(first);

    let second = start_app(&r.fs, &r.clock, &r.peer);
    let current = second.service.status().current.expect("execution from the journal");
    assert_eq!(current.execution.state, ExecutionState::Unknown(UnknownReason::InterruptedByRestart));
    assert!(current.execution.start_sent);
    assert_eq!((current.job.plan, current.job.rd.as_ref()), (job().plan, &RD[..]));
    assert!(act(&second, MachineAction::Start, None, None).is_err(), "never a start for an unknown execution");
    assert!(act(&second, MachineAction::Upload, Some(job()), None).is_err(), "machine busy until reconciled");
    assert_eq!(r.peer.sent().len(), sent, "a restart sends nothing");
    assert_eq!(starts(&r.peer), 1);
}

/// The status query after a restart is journaled (evidence continues after the old entries);
/// the operator's statement settles the execution durably and is never counted as produced
/// unless it says "ran to completion".
#[test]
fn status_query_after_restart_is_journaled_and_the_operator_statement_settles_it() {
    let r = rig();
    let first = start_app(&r.fs, &r.clock, &r.peer);
    upload_store_start(&r, &first, None);
    drop(first);

    let second = start_app(&r.fs, &r.clock, &r.peer);
    let id = second.service.status().current.unwrap().execution.id;
    let old = second.store.with_history(|h| h.execution(id).unwrap().evidence.len());
    r.peer.script(Step::expect(Request::QueryStatus).reply(Reply::Status(ControllerStatus::Idle)));
    act(&second, MachineAction::QueryStatus, None, None).unwrap();
    let evidence = second.store.with_history(|h| h.execution(id).unwrap().evidence.clone());
    assert!(evidence.len() > old, "evidence after the restart is journaled");
    assert!(
        evidence[old..].iter().any(|(_, e)| matches!(e, burnburn_history::EvidenceDto::Status { .. })),
        "the status reply is in the history"
    );
    act(&second, MachineAction::RecordOutcome(Outcome::DidNotRun), None, None).unwrap();
    drop(second);

    let third = start_app(&r.fs, &r.clock, &r.peer);
    let view = third.store.with_history(|h| h.execution(id).cloned()).unwrap();
    assert_eq!(view.state, ExecutionStateDto::Failed { why: FailReasonDto::OperatorDidNotRun });
    assert_eq!(
        view.reconciled,
        Some(ReconcileBasis::OperatorConfirmed { role: HRole::Operator, statement: ReconcileStatement::DidNotRun })
    );
    assert!(third.service.status().current.is_none_or(|c| c.execution.state.is_terminal()));
    assert_eq!(starts(&r.peer), 1);
}

/// The same upload button (same action id) after a restart is not a fresh intent; the next
/// button carries a new id, so a second piece can be made.
#[test]
fn an_action_from_before_the_restart_is_never_fresh_again() {
    let r = rig();
    let first = start_app(&r.fs, &r.clock, &r.peer);
    let old_upload = first.service.status().action_id(MachineAction::Upload, ActionSubject::Job(job().plan));
    upload_store_start(&r, &first, None);
    for status in [ControllerStatus::Running, ControllerStatus::Finished] {
        r.peer.script(Step::expect(Request::QueryStatus).reply(Reply::Status(status)));
        act(&first, MachineAction::QueryStatus, None, None).unwrap();
    }
    assert_eq!(first.service.status().current.unwrap().execution.state, ExecutionState::Completed);
    drop(first);

    let second = start_app(&r.fs, &r.clock, &r.peer);
    let next = second.service.status().action_id(MachineAction::Upload, ActionSubject::Job(job().plan));
    assert_ne!(next, old_upload, "the next upload button has a new id");
    let before = r.peer.sent().len();
    let replay = MachineIntent {
        action: old_upload,
        machine: MachineId::new(MACHINE),
        job: Some(job()),
        required_state: second.service.status().current.map(|c| c.execution.state),
        order: None,
        role: Role::Operator,
    };
    assert_eq!(second.service.dispatch(replay), Err(MachineError::AlreadyRecorded));
    assert_eq!(r.peer.sent().len(), before, "nothing sent for a replayed action");
}

/// IM31: orders and the execution made for an order are in the journal after a restart.
#[test]
fn orders_and_their_executions_are_durable() {
    let r = rig();
    let first = start_app(&r.fs, &r.clock, &r.peer);
    let order = StoredOrder {
        id: OrderId(1),
        draft: OrderDraft { name: "Klant A".into(), part: "plaat".into(), quantity: 2, remnant: Some("rest".into()) },
        chosen: Some(job().plan.hash()),
    };
    first.store.record_order(&order).unwrap();
    upload_store_start(&r, &first, Some(OrderId(1)));
    let id = first.service.status().current.unwrap().execution.id;
    drop(first);

    let second = start_app(&r.fs, &r.clock, &r.peer);
    let stored = second.store.orders().unwrap();
    assert_eq!(stored.orders, vec![order]);
    assert_eq!(stored.executions, vec![(id, OrderId(1))]);
}
