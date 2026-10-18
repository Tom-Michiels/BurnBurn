//! Laser-window actions through the machine driver: history first, then the actor; outcomes
//! that may stay unknown; replay without a machine. Scripted fake controller, in-memory journal.
#![allow(clippy::unwrap_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use burnburn_document::DocumentSession;
use burnburn_history::{
    ActionId, CompileRecord, CompiledJobId, History, IntentKind, JournalFs, JournalLimits, PreflightSummary, WallTime,
};
use burnburn_history::{DiagnosticOptions, LiveOutcomeDto, LiveUnknownDto};
use burnburn_io::fs::{FileSystem, FsError, FsErrorKind, FsOp};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{Project, ProjectUuid};
use burnburn_job::{
    JobPlan, JobPlanBuilder, MachineFrame, MachinePoint, OperationSettings, OriginCorner, PlanEvent, PlanMode,
    PlanSource, PowerRatio, ProfileRevision,
};
use burnburn_machine::{
    Capabilities, Capability, ControllerFileName, FileCapabilities, HomeTarget, JogKey, LiveCapabilities, LiveError,
    LiveRequest,
};
use burnburn_machine::{
    ControllerIdentity, ControllerPort, Envelope, ExecutionActor, Incoming, MachineId, MachineIdentity, Policy,
    PortError, Reply, Request, RequestTag,
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

fn all_supported() -> Capabilities {
    let s = || Capability::Supported;
    Capabilities {
        live: LiveCapabilities { jog: s(), home: s(), move_absolute: s(), pause_resume: s(), read_position: s() },
        files: FileCapabilities { list: s(), metadata: s(), upload_named: s(), run: s(), delete: s() },
    }
}

fn live_rig() -> Rig {
    let mut r = rig();
    r.driver.set_capabilities(all_supported());
    r
}

fn home() -> LiveRequest {
    LiveRequest::Home(HomeTarget::Xy)
}

impl Rig {
    fn starts_sent(&self) -> usize {
        self.peer.sent().iter().filter(|r| **r == Request::Start).count()
    }
}

fn live_sent(r: &Rig) -> usize {
    r.peer.sent().iter().filter(|q| matches!(q, Request::Live(_))).count()
}

#[test]
fn a_live_intent_is_durable_before_the_driver_sends_it() {
    let mut r = live_rig();
    r.fs.mem.fail_on(FsOp::SyncFile, 0);
    assert!(matches!(r.driver.live(ActionId::from_raw(1), home()), Err(DriverError::History(_))));
    assert_eq!(live_sent(&r), 0, "sent although its intent was not durable");
}

#[test]
fn a_refused_action_leaves_no_journal_record_and_no_wire_traffic() {
    let mut r = rig(); // default capabilities: everything unverified
    let before = r.driver.history().records().len();
    let err = r.driver.live(ActionId::from_raw(1), home()).unwrap_err();
    assert!(matches!(err, DriverError::Live(LiveError::NotVerified { .. })), "{err:?}");
    assert_eq!(r.driver.history().records().len(), before);
    assert_eq!(live_sent(&r), 0);
}

#[test]
fn the_same_action_id_twice_sends_one_request() {
    let mut r = live_rig();
    r.peer.script(Step::expect(Request::Live(home())).reply(Reply::Ack));
    r.driver.live(ActionId::from_raw(5), home()).unwrap();
    r.driver.poll().unwrap();
    // A double click, or the same click re-sent after reconnecting.
    r.driver.live(ActionId::from_raw(5), home()).unwrap();
    assert_eq!(live_sent(&r), 1);
    assert_eq!(r.driver.history().live_actions().count(), 1);
}

#[test]
fn the_outcome_is_journaled_after_the_reply_and_unknown_stays_unknown() {
    let mut r = live_rig();
    r.peer.script(Step::expect(Request::Live(home())).reply(Reply::Ack));
    r.driver.live(ActionId::from_raw(1), home()).unwrap();
    assert_eq!(r.driver.history().live_action(ActionId::from_raw(1)).unwrap().outcome, None, "not before the reply");
    r.driver.poll().unwrap();
    assert_eq!(r.driver.history().live_action(ActionId::from_raw(1)).unwrap().outcome, Some(LiveOutcomeDto::Acked));

    // A motion whose reply is lost stays unknown, is not resent, and the journal says so.
    r.peer.script(Step::expect(Request::Live(LiveRequest::MoveAbsolute { x_um: 1, y_um: 2 })));
    r.driver.live(ActionId::from_raw(2), LiveRequest::MoveAbsolute { x_um: 1, y_um: 2 }).unwrap();
    r.clock.advance(TIMEOUT + Duration::from_millis(1));
    r.driver.poll().unwrap();
    r.clock.advance(TIMEOUT + Duration::from_millis(1));
    r.driver.poll().unwrap();
    assert_eq!(
        r.driver.history().live_action(ActionId::from_raw(2)).unwrap().outcome,
        Some(LiveOutcomeDto::Unknown { why: LiveUnknownDto::ReplyLost })
    );
    assert_eq!(live_sent(&r), 2);
}

#[test]
fn a_crash_between_intent_and_outcome_leaves_the_action_unknown_and_resends_nothing() {
    let mut r = live_rig();
    r.peer.script(Step::expect(Request::Live(LiveRequest::JogStart(JogKey::XLeft))));
    r.driver.live(ActionId::from_raw(1), LiveRequest::JogStart(JogKey::XLeft)).unwrap();
    let fs = Arc::clone(&r.fs);
    drop(r.driver); // the application dies here

    let (history, _) = History::open(fs.as_ref(), &dir(), &JournalLimits::default()).unwrap();
    let v = history.live_action(ActionId::from_raw(1)).unwrap();
    assert_eq!(v.outcome, Some(LiveOutcomeDto::Unknown { why: LiveUnknownDto::InterruptedByRestart }));
    let job = history.jobs().next().unwrap().id;
    let again = rig_with(Arc::clone(&fs), history, job);
    assert!(again.peer.sent().is_empty(), "restart must not replay the jog");
    assert!(again.driver.actor().live_attempts().is_empty());
}

#[test]
fn a_transport_without_a_verified_wire_form_journals_not_sent() {
    struct Refusing;
    impl ControllerPort for Refusing {
        fn send(&mut self, _: Envelope) -> Result<(), PortError> {
            Err(PortError::NotSupported("no verified wire form".into()))
        }
        fn try_recv(&mut self) -> Result<Option<Incoming>, PortError> {
            Ok(None)
        }
        fn reconnect(&mut self) -> Result<(), PortError> {
            Ok(())
        }
    }
    let fs = Arc::new(Fs::default());
    let (h, _) = history_with_job(&fs);
    let clock = FakeClock::new();
    let identity = MachineIdentity { id: MachineId::new("m"), model: Some("x".into()), firmware: None };
    let actor = ExecutionActor::new(identity, Refusing, Arc::new(clock), Policy::default());
    let fs_dyn: Arc<dyn JournalFs> = Arc::clone(&fs) as Arc<dyn JournalFs>;
    let mut driver = MachineDriver::new(actor, h, fs_dyn, Arc::new(FixedWall));
    driver.set_capabilities(all_supported());
    assert!(matches!(driver.live(ActionId::from_raw(1), home()), Err(DriverError::Live(LiveError::Port(_)))));
    assert_eq!(driver.history().live_action(ActionId::from_raw(1)).unwrap().outcome, Some(LiveOutcomeDto::NotSent));
}

#[test]
fn releasing_a_held_jog_key_is_journaled_like_any_other_action() {
    let mut r = live_rig();
    r.peer.script(Step::expect(Request::Live(LiveRequest::JogStart(JogKey::YTop))).reply(Reply::Ack));
    r.peer.script(Step::expect(Request::Live(LiveRequest::JogStop(JogKey::YTop))).reply(Reply::Ack));
    r.driver.live(ActionId::from_raw(1), LiveRequest::JogStart(JogKey::YTop)).unwrap();
    r.driver.poll().unwrap();
    assert!(r.driver.release_jog(ActionId::from_raw(2)).unwrap());
    r.driver.poll().unwrap();
    assert_eq!(r.driver.history().live_action(ActionId::from_raw(2)).unwrap().outcome, Some(LiveOutcomeDto::Acked));
    assert!(!r.driver.release_jog(ActionId::from_raw(3)).unwrap(), "nothing held: no action, no record");
    assert!(r.driver.history().live_action(ActionId::from_raw(3)).is_none());
}

#[test]
fn running_a_controller_file_never_creates_an_execution_or_a_start_intent() {
    let mut r = live_rig();
    let name = ControllerFileName::new("PLAAT01").unwrap();
    r.peer.script(Step::expect(Request::Live(LiveRequest::RunFile(name.clone()))).reply(Reply::Ack));
    r.driver.live(ActionId::from_raw(1), LiveRequest::RunFile(name)).unwrap();
    r.driver.poll().unwrap();
    assert_eq!(r.driver.history().executions().count(), 0);
    assert_eq!(r.starts_sent(), 0);
}

#[test]
fn diagnostic_replay_needs_no_machine_contact() {
    let mut r = live_rig();
    r.peer.script(Step::expect(Request::Live(home())).reply(Reply::Ack));
    r.driver.live(ActionId::from_raw(1), home()).unwrap();
    r.driver.poll().unwrap();

    // The controller is gone and unscripted: replay must not need it.
    r.peer.disconnect();
    let sent_before = r.peer.sent_log().len();
    let report = r.driver.diagnostic_report(&DiagnosticOptions::default());
    assert_eq!(report.live_actions.len(), 1);
    assert_eq!(report.live_actions[0].outcome, Some(LiveOutcomeDto::Acked));
    assert_eq!(report.jobs.len(), 1);
    assert_eq!(r.peer.sent_log().len(), sent_before, "replay produced traffic");
    assert!(r.driver.actor().in_flight_tag().is_none());
}

#[test]
fn a_frame_upload_records_the_frame_intent_and_never_a_production_upload() {
    let mut r = live_rig();
    r.peer.script(Step::expect(Request::QueryIdentity).reply(identity()));
    r.peer.script(Step::expect(Request::UploadChunk { index: 0, bytes: RD.to_vec() }).reply(Reply::Ack));
    let e = r.driver.upload_frame(r.job, &setup(), ActionId::from_raw(1)).unwrap();
    r.driver.poll().unwrap();
    let v = r.driver.history().execution(e).unwrap();
    assert!(v.intent_recorded(IntentKind::Frame));
    assert!(!v.intent_recorded(IntentKind::Upload));
    assert_eq!(v.item, None, "a frame run belongs to no queue item and counts no piece");
}
