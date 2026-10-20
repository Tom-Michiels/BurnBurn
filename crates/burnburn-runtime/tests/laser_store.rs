//! W2-L1-laser: the laser-window actions of presentation's machine service over the real
//! append-only history (`JournalHistoryStore`): intents and outcomes in the journal, ids that do not
//! collide after a restart, frame uploads recorded as frames, and the diagnostic report.
//! Fake controller and fake clock; no socket, no laser.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use burnburn_document::DocumentSession;
use burnburn_history::{History, IntentKind, JournalFs, JournalLimits, LiveIntentDto, LiveOutcomeDto, WallTime};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{Project, ProjectUuid};
use burnburn_job::{
    JobPlan, JobPlanBuilder, MachineFrame, MachinePoint, OperationSettings, OriginCorner, PlanEvent, PlanMode,
    PlanSource, PowerRatio, ProfileRevision,
};
use burnburn_machine::{
    Capabilities, Capability, ControllerIdentity, ControllerPort, Envelope, ExecutionActor, HomeTarget, Incoming,
    LaserAction, LiveRequest, MachineId, MachineIdentity, Policy, PortError, Reply, Request, RequestTag,
};
use burnburn_presentation::{
    ActionSubject, DiagnosticOptions, Dispatched, HistoryStore, JobInputs, LaserCall, LaserIntent, LiveKind,
    MachineAction, MachineError, MachineIntent, MachinePort, MachineService, PreparedJob, Role,
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

fn supported() -> Capabilities {
    let mut c = Capabilities::default();
    c.live.home = Capability::Supported;
    c.live.pause_resume = Capability::Supported;
    c.live.move_absolute = Capability::Supported;
    c.live.read_position = Capability::Supported;
    c
}

fn laser(run: &Run, action: LaserAction) -> Result<Dispatched, MachineError> {
    let kind = MachineAction::Live(LiveKind::of(&action));
    let intent = LaserIntent {
        action: run.service.status().action_id(kind, ActionSubject::Machine),
        machine: MachineId::new(MACHINE),
        call: LaserCall::Action(action),
        frame: MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap(),
        user_origin: None,
    };
    let out = run.service.laser(intent);
    for _ in 0..8 {
        run.service.poll().unwrap();
    }
    out
}

fn home_ack(r: &Rig) {
    r.peer.script(Step::expect(Request::Live(LiveRequest::Home(HomeTarget::Xy))).reply(Reply::Ack));
}

#[test]
fn laser_actions_reach_the_journal_with_their_proven_outcome() {
    let r = rig();
    let run = start_app(&r.fs, &r.clock, &r.peer);
    run.service.set_capabilities(supported());
    home_ack(&r);
    assert!(laser(&run, LaserAction::Home(HomeTarget::Xy)).is_ok());

    let live: Vec<_> = run.store.with_history(|h| h.live_actions().cloned().collect());
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].intent, LiveIntentDto::Home { target: "xy".into() });
    assert_eq!(live[0].outcome, Some(LiveOutcomeDto::Acked));
}

#[test]
fn a_restart_does_not_make_the_next_click_a_duplicate_of_an_old_one() {
    let r = rig();
    let run = start_app(&r.fs, &r.clock, &r.peer);
    run.service.set_capabilities(supported());
    home_ack(&r);
    laser(&run, LaserAction::Home(HomeTarget::Xy)).unwrap();
    drop(run);

    let run = start_app(&r.fs, &r.clock, &r.peer);
    run.service.set_capabilities(supported());
    let next = run.service.status().action_id(MachineAction::Live(LiveKind::Home), ActionSubject::Machine);
    assert_eq!(next.generation, 1, "the button's id continues after the journaled home");
    home_ack(&r);
    laser(&run, LaserAction::Home(HomeTarget::Xy)).unwrap();
    let sent = r.peer.sent().iter().filter(|q| matches!(q, Request::Live(LiveRequest::Home(_)))).count();
    assert_eq!(sent, 2, "the second home after the restart was sent, not swallowed as a duplicate");
    assert_eq!(run.store.with_history(|h| h.live_actions().count()), 2);
}

#[test]
fn moves_with_different_origins_and_other_kinds_never_share_an_id() {
    let r = rig();
    let run = start_app(&r.fs, &r.clock, &r.peer);
    run.service.set_capabilities(supported());
    r.peer.script(Step::expect(Request::Live(LiveRequest::Pause)).reply(Reply::Ack));
    r.peer.script(Step::expect(Request::Live(LiveRequest::Home(HomeTarget::Xy))).reply(Reply::Ack));
    r.peer.script(Step::expect(Request::Live(LiveRequest::MoveAbsolute { x_um: 1000, y_um: 2000 })).reply(Reply::Ack));
    laser(&run, LaserAction::Pause).unwrap();
    laser(&run, LaserAction::Home(HomeTarget::Xy)).unwrap();
    laser(&run, LaserAction::MoveTo { x_um: 1000, y_um: 2000 }).unwrap();
    assert_eq!(run.store.with_history(|h| h.live_actions().count()), 3, "three kinds, three journal records");
    drop(run);

    // After a restart every move-like button continues above the journaled moves.
    let run = start_app(&r.fs, &r.clock, &r.peer);
    for kind in [LiveKind::MoveTo, LiveKind::JogStep, LiveKind::PositionLaser, LiveKind::GoToOrigin] {
        let id = run.service.status().action_id(MachineAction::Live(kind), ActionSubject::Machine);
        assert!(id.generation >= 1, "{kind:?} would reuse the id of the earlier move");
    }
}

#[test]
fn a_frame_upload_is_journaled_as_a_frame_and_never_as_an_upload() {
    let r = rig();
    let run = start_app(&r.fs, &r.clock, &r.peer);
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
    let status = run.service.status();
    let intent = MachineIntent {
        action: status.action_id(MachineAction::Frame, ActionSubject::Machine),
        machine: MachineId::new(MACHINE),
        job: Some(job()),
        required_state: None,
        order: None,
        role: Role::Designer,
    };
    run.service.dispatch(intent).unwrap();
    for _ in 0..16 {
        run.service.poll().unwrap();
    }
    run.store.with_history(|h| {
        let e = h.executions().next().expect("one execution");
        assert!(e.intent_recorded(IntentKind::Frame));
        assert!(!e.intent_recorded(IntentKind::Upload));
    });
    drop(run);

    // After a restart the next frame button continues above the journaled frame.
    let run = start_app(&r.fs, &r.clock, &r.peer);
    let next = run.service.status().action_id(MachineAction::Frame, ActionSubject::Machine);
    assert_eq!(next.generation, 1);
}

#[test]
fn the_diagnostic_report_needs_no_machine_and_omits_private_data_unless_ticked() {
    let r = rig();
    let run = start_app(&r.fs, &r.clock, &r.peer);
    run.service.set_capabilities(supported());
    home_ack(&r);
    laser(&run, LaserAction::Home(HomeTarget::Xy)).unwrap();
    let sent_before = r.peer.sent().len();

    let plain = run.store.diagnostic_report(&DiagnosticOptions::default()).unwrap();
    assert!(plain.contains("diagnostic replay"), "{plain}");
    assert!(!plain.contains(MACHINE), "machine ids stay out unless ticked: {plain}");
    let ticked = run
        .store
        .diagnostic_report(&DiagnosticOptions { include_project_paths: true, include_machine_ids: true })
        .unwrap();
    assert!(ticked.contains(MACHINE), "{ticked}");
    assert_eq!(r.peer.sent().len(), sent_before, "no machine contact");
}
