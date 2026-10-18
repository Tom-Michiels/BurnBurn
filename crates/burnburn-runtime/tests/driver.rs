//! History-before-driver ordering (ADR 0008 §5) with a scripted fake controller and io's
//! MemoryFileSystem. No socket, no laser.
#![allow(clippy::unwrap_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use burnburn_document::DocumentSession;
use burnburn_history::{
    ActionId, CompileRecord, CompiledJobId, ExecutionStateDto, History, IntentKind, JournalFs, JournalLimits,
    PreflightSummary, UnknownReasonDto, WallTime,
};
use burnburn_io::fs::{FileSystem, FsError, FsErrorKind, FsOp};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{Project, ProjectUuid};
use burnburn_job::{
    JobPlan, JobPlanBuilder, MachineFrame, MachinePoint, OperationSettings, OriginCorner, PlanEvent, PlanMode,
    PlanSource, PowerRatio, ProfileRevision,
};
use burnburn_machine::{
    ControllerIdentity, ControllerPort, ControllerStatus, Envelope, ExecutionActor, ExecutionId, ExecutionState,
    Incoming, MachineId, MachineIdentity, Policy, PortError, Reply, Request, RequestTag,
};
use burnburn_runtime::{DriverError, MachineDriver, WallClock};
use burnburn_test_support::{FakeClock, ScriptedPeer, Step};
use burnburn_types::{ContentHash, DocumentId, HashDomain, OperationId, RevisionId, StableId};

type Peer = ScriptedPeer<Request, Reply>;

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

/// io's MemoryFileSystem with `append` layered on top (C-7 pending).
#[derive(Default)]
struct Fs {
    mem: MemoryFileSystem,
    appends: Mutex<usize>,
}

impl FileSystem for Fs {
    fn read(&self, p: &Path, l: u64) -> Result<Vec<u8>, FsError> {
        self.mem.read(p, l)
    }
    fn exists(&self, p: &Path) -> Result<bool, FsError> {
        self.mem.exists(p)
    }
    fn create_dir_all(&self, d: &Path) -> Result<(), FsError> {
        self.mem.create_dir_all(d)
    }
    fn create_temp_in(&self, d: &Path, s: &str) -> Result<PathBuf, FsError> {
        self.mem.create_temp_in(d, s)
    }
    fn write_all(&self, p: &Path, b: &[u8]) -> Result<(), FsError> {
        self.mem.write_all(p, b)
    }
    fn sync_file(&self, p: &Path) -> Result<(), FsError> {
        self.mem.sync_file(p)
    }
    fn replace(&self, f: &Path, t: &Path) -> Result<(), FsError> {
        self.mem.replace(f, t)
    }
    fn sync_dir(&self, d: &Path) -> Result<(), FsError> {
        self.mem.sync_dir(d)
    }
    fn remove_file(&self, p: &Path) -> Result<(), FsError> {
        self.mem.remove_file(p)
    }
    fn append(&self, path: &Path, bytes: &[u8]) -> Result<u64, FsError> {
        *self.appends.lock().unwrap() += 1;
        let mut old = match self.mem.read(path, u64::MAX) {
            Ok(b) => b,
            Err(e) if e.kind == FsErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };
        old.extend_from_slice(bytes);
        self.mem.write_all(path, &old)?;
        Ok(old.len() as u64)
    }
    fn list_dir(&self, dir: &Path) -> Result<Vec<PathBuf>, FsError> {
        self.mem.list_dir(dir)
    }
}

struct FixedWall;
impl WallClock for FixedWall {
    fn now(&self) -> WallTime {
        WallTime::from_unix_millis(1_792_000_000_000)
    }
}

const TIMEOUT: Duration = Duration::from_millis(500);

fn dir() -> PathBuf {
    PathBuf::from("/app-data/history")
}

fn setup() -> ProfileRevision {
    ProfileRevision { id: "bench".into(), revision: 1, content: ContentHash::of(HashDomain::Profile, b"bench") }
}

fn plan() -> JobPlan {
    let source = PlanSource {
        document_revision: RevisionId::INITIAL,
        source_hash: ContentHash::of(HashDomain::Plan, b"driver-test"),
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

const RD: [u8; 3] = [0xd8, 0x10, 0x89];

/// A history holding one frozen job.
fn history_with_job(fs: &Fs) -> (History, CompiledJobId) {
    let (mut h, _) = History::open(fs, &dir(), &JournalLimits::default()).unwrap();
    let snapshot = (*DocumentSession::new(DocumentId::from_raw(1)).snapshot()).clone();
    let project = Project::new(ProjectUuid::from_bytes([1; 16]), snapshot);
    let job = h
        .record_compiled(
            fs,
            CompileRecord {
                plan: &plan(),
                rd_file: &RD,
                project: &project,
                project_path: None,
                preflight: PreflightSummary::default(),
                profile: &setup(),
            },
            FixedWall.now(),
        )
        .unwrap();
    (h, job)
}

struct Rig {
    fs: Arc<Fs>,
    clock: FakeClock,
    peer: Peer,
    driver: MachineDriver<FakePort>,
    job: CompiledJobId,
}

fn rig_with(fs: Arc<Fs>, history: History, job: CompiledJobId) -> Rig {
    let clock = FakeClock::new();
    let peer = Peer::new(clock.clone());
    let machine = MachineId::new("bench-ruida-1");
    let port = FakePort { peer: peer.clone(), machine: machine.clone(), last: None };
    let identity = MachineIdentity { id: machine, model: Some("RDC6445G".into()), firmware: Some("fake-1".into()) };
    let actor = ExecutionActor::new(
        identity,
        port,
        Arc::new(clock.clone()),
        Policy { reply_timeout: TIMEOUT, max_query_attempts: 2 },
    );
    let fs_dyn: Arc<dyn JournalFs> = Arc::clone(&fs) as Arc<dyn JournalFs>;
    Rig { fs, clock, peer, driver: MachineDriver::new(actor, history, fs_dyn, Arc::new(FixedWall)), job }
}

fn rig() -> Rig {
    let fs = Arc::new(Fs::default());
    let (h, job) = history_with_job(&fs);
    rig_with(fs, h, job)
}

fn identity() -> Reply {
    Reply::Identity(ControllerIdentity { model: Some("RDC6445G".into()), firmware: Some("fake-1".into()) })
}

impl Rig {
    fn upload_and_store(&mut self) -> ExecutionId {
        self.peer.script(Step::expect(Request::QueryIdentity).reply(identity()));
        self.peer.script(Step::expect(Request::UploadChunk { index: 0, bytes: RD.to_vec() }).reply(Reply::Ack));
        self.peer.script(Step::expect(Request::QueryStored).reply(Reply::Stored(true)));
        let e = self.driver.upload(self.job, None, &setup(), ActionId::from_raw(1)).unwrap();
        self.driver.poll().unwrap();
        self.driver.poll().unwrap();
        self.driver.confirm_stored(ActionId::from_raw(2)).unwrap();
        self.driver.poll().unwrap();
        assert_eq!(self.driver.actor().execution().unwrap().state, ExecutionState::Stored);
        e
    }

    fn starts_sent(&self) -> usize {
        self.peer.sent().iter().filter(|r| **r == Request::Start).count()
    }
}

#[test]
fn start_intent_is_durable_before_driver_receives_it() {
    let mut r = rig();
    let e = r.upload_and_store();
    // The disk refuses to make the intent durable: the start must not leave.
    r.fs.mem.fail_on(FsOp::SyncFile, 0);
    assert!(matches!(r.driver.start(e, ActionId::from_raw(3)), Err(DriverError::History(_))));
    assert_eq!(r.starts_sent(), 0, "start sent although its intent was not durable");
    assert_eq!(r.driver.actor().execution().unwrap().state, ExecutionState::Stored);

    // With a working disk: intent synced first, then exactly one start.
    r.peer.script(Step::expect(Request::Start).reply(Reply::Ack));
    let syncs_before = r.fs.mem.log().iter().filter(|(op, _)| *op == FsOp::SyncFile).count();
    r.driver.start(e, ActionId::from_raw(4)).unwrap();
    assert_eq!(r.starts_sent(), 1);
    let syncs_after = r.fs.mem.log().iter().filter(|(op, _)| *op == FsOp::SyncFile).count();
    assert!(syncs_after > syncs_before);
    assert!(r.driver.history().execution(e).unwrap().intent_recorded(IntentKind::Start));

    // The same click again is a no-op; a new click never starts twice.
    r.driver.start(e, ActionId::from_raw(4)).unwrap();
    assert_eq!(r.driver.start(e, ActionId::from_raw(5)), Err(DriverError::StartAlreadyRecorded));
    r.driver.poll().unwrap();
    assert_eq!(r.starts_sent(), 1);
    r.peer.verify_complete().unwrap();
}

#[test]
fn restart_after_start_never_resends_start_or_upload() {
    let mut r = rig();
    let e = r.upload_and_store();
    r.peer.script(Step::expect(Request::Start).reply(Reply::Ack));
    r.driver.start(e, ActionId::from_raw(3)).unwrap();
    r.driver.poll().unwrap();
    r.peer.script(Step::expect(Request::QueryStatus).reply(Reply::Status(ControllerStatus::Running)));
    r.driver.query_status(ActionId::from_raw(4)).unwrap();
    r.driver.poll().unwrap();
    assert_eq!(r.driver.history().execution(e).unwrap().state, ExecutionStateDto::Running);

    // Crash: new process, new actor, same disk.
    let fs = Arc::clone(&r.fs);
    drop(r);
    let (h, report) = History::open(fs.as_ref(), &dir(), &JournalLimits::default()).unwrap();
    assert_eq!(report.interrupted, vec![e]);
    let job = h.execution(e).unwrap().job;
    let mut r = rig_with(fs, h, job);
    assert_eq!(
        r.driver.history().execution(e).unwrap().state,
        ExecutionStateDto::Unknown { why: UnknownReasonDto::InterruptedByRestart }
    );
    assert_eq!(r.driver.start(e, ActionId::from_raw(9)), Err(DriverError::StartAlreadyRecorded));
    assert_eq!(r.driver.reverify(e, ActionId::from_raw(10)), Err(DriverError::StartAlreadyRecorded));
    r.clock.advance(TIMEOUT * 3);
    r.driver.poll().unwrap();
    assert!(r.peer.sent().is_empty(), "nothing is sent after a restart: {:?}", r.peer.sent());
}

#[test]
fn stored_upload_is_started_in_a_later_session_only_after_reverification() {
    let mut r = rig();
    let e = r.upload_and_store();
    let fs = Arc::clone(&r.fs);
    drop(r);
    let (h, _) = History::open(fs.as_ref(), &dir(), &JournalLimits::default()).unwrap();
    let job = h.execution(e).unwrap().job;
    let mut r = rig_with(fs, h, job);

    // Without fresh proof in this session: refused.
    assert!(matches!(r.driver.start(e, ActionId::from_raw(20)), Err(DriverError::NotStartable(_))));
    r.peer.script(Step::expect(Request::QueryIdentity).reply(identity()));
    r.peer.script(Step::expect(Request::QueryStored).reply(Reply::Stored(true)));
    r.driver.reverify(e, ActionId::from_raw(21)).unwrap();
    r.driver.poll().unwrap();
    r.driver.poll().unwrap();
    r.peer.script(Step::expect(Request::Start).reply(Reply::Ack));
    r.driver.start(e, ActionId::from_raw(22)).unwrap();
    assert_eq!(r.peer.sent(), vec![Request::QueryIdentity, Request::QueryStored, Request::Start]);
    assert_eq!(r.driver.history().execution(e).unwrap().state, ExecutionStateDto::StartRequested);
}

#[test]
fn repeated_upload_action_does_not_upload_twice() {
    let mut r = rig();
    r.peer.script(Step::expect(Request::QueryIdentity).reply(identity()));
    let a = r.driver.upload(r.job, None, &setup(), ActionId::from_raw(1)).unwrap();
    let b = r.driver.upload(r.job, None, &setup(), ActionId::from_raw(1)).unwrap();
    assert_eq!(a, b);
    assert_eq!(r.driver.history().executions().count(), 1);
    assert_eq!(r.peer.sent(), vec![Request::QueryIdentity]);
}
