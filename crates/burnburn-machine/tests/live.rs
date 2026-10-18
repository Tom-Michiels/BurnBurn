//! Live (non-job) laser-window actions on the actor: jog, home, move, pause/resume, position and
//! DSP-file requests against a scripted fake controller. No real laser, socket or wall clock.
#![allow(clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use burnburn_job::{JobPlanBuilder, JobPlanId, MachineFrame, MachinePoint, OriginCorner, PlanSource, ProfileRevision};
use burnburn_machine::{
    Capabilities, Capability, ControllerFileName, ControllerPort, Envelope, Execution, ExecutionActor, ExecutionId,
    ExecutionState, FileCapabilities, FileEntry, HomeTarget, Incoming, JogKey, LiveCapabilities, LiveError,
    LiveFeature, LiveOutcome, LiveRequest, MachineId, MachineIdentity, Policy, PortError, RdArtifact, Reply, Request,
    RequestTag, UnknownReason,
};
use burnburn_test_support::{FakeClock, PeerError, ScriptedPeer, Step};
use burnburn_types::{ContentHash, HashDomain, RevisionId};

type Peer = ScriptedPeer<Request, Reply>;

struct FakePort {
    peer: Peer,
    machine: MachineId,
    last_tag: Option<RequestTag>,
}

impl ControllerPort for FakePort {
    fn send(&mut self, envelope: Envelope) -> Result<(), PortError> {
        self.last_tag = Some(envelope.tag);
        self.peer.send(envelope.request).map_err(port_error)
    }

    fn try_recv(&mut self) -> Result<Option<Incoming>, PortError> {
        let reply = self.peer.recv().map_err(port_error)?;
        Ok(reply.map(|reply| Incoming { from: self.machine.clone(), tag: self.last_tag.unwrap(), reply }))
    }

    fn reconnect(&mut self) -> Result<(), PortError> {
        self.peer.reconnect();
        Ok(())
    }
}

fn port_error(e: PeerError) -> PortError {
    match e {
        PeerError::Disconnected => PortError::Disconnected,
        other => PortError::Other(other.to_string()),
    }
}

const TIMEOUT: Duration = Duration::from_millis(500);

struct Rig {
    clock: FakeClock,
    peer: Peer,
    actor: ExecutionActor<FakePort>,
}

fn rig_with(caps: Capabilities) -> Rig {
    let clock = FakeClock::new();
    let peer = Peer::new(clock.clone());
    let machine = MachineId::new("bench-ruida-1");
    let port = FakePort { peer: peer.clone(), machine: machine.clone(), last_tag: None };
    let identity = MachineIdentity { id: machine, model: Some("RDC6445G".into()), firmware: Some("fake-1".into()) };
    let mut actor = ExecutionActor::new(identity, port, Arc::new(clock.clone()), Policy { reply_timeout: TIMEOUT, max_query_attempts: 3 });
    actor.set_capabilities(caps);
    Rig { clock, peer, actor }
}

fn all_supported() -> Capabilities {
    let s = || Capability::Supported;
    Capabilities {
        live: LiveCapabilities { jog: s(), home: s(), move_absolute: s(), pause_resume: s(), read_position: s() },
        files: FileCapabilities { list: s(), metadata: s(), upload_named: s(), run: s(), delete: s() },
    }
}

fn rig() -> Rig {
    rig_with(all_supported())
}

impl Rig {
    fn sent(&self) -> Vec<Request> {
        self.peer.sent()
    }

    fn time_out(&mut self) {
        self.clock.advance(TIMEOUT + Duration::from_millis(1));
        self.actor.poll().unwrap();
    }

    fn last_outcome(&self) -> LiveOutcome {
        self.actor.live_attempts().last().unwrap().outcome.clone()
    }

    /// An execution from an earlier session whose outcome is unknown: the job may still run.
    fn adopt_unknown_job(&mut self) {
        let source = PlanSource {
            document_revision: RevisionId::INITIAL,
            source_hash: ContentHash::of(HashDomain::Plan, b"live-test"),
            machine_profile: ProfileRevision {
                id: "bench".into(),
                revision: 1,
                content: ContentHash::of(HashDomain::Profile, b"bench"),
            },
            compiler_version: "test".into(),
        };
        let frame = MachineFrame::new(100.0, 100.0, OriginCorner::BottomLeft).unwrap();
        let plan: JobPlanId =
            JobPlanBuilder::new(source, frame, MachinePoint::new(0.0, 0.0).unwrap()).finish().unwrap().id();
        let artifact = RdArtifact::from_rd_file(plan, &[1, 2, 3], 2).unwrap();
        self.actor
            .resume_unknown(Execution {
                id: ExecutionId::from_raw(3),
                machine: MachineId::new("bench-ruida-1"),
                plan,
                rd_hash: artifact.rd_hash(),
                state: ExecutionState::Unknown(UnknownReason::StartReplyLost),
                evidence: Vec::new(),
                start_sent: true,
            })
            .unwrap();
    }
}

fn live(r: LiveRequest) -> Request {
    Request::Live(r)
}

fn name(s: &str) -> ControllerFileName {
    ControllerFileName::new(s).unwrap()
}

// ---- position (M05) ----

#[test]
fn unknown_axis_position_is_not_shown_as_zero() {
    let mut r = rig();
    // Before any reading: nothing to show, in particular not (0, 0).
    let p = r.actor.position();
    assert_eq!((p.x.mm(), p.y.mm()), (None, None));
    assert_eq!(p.machine_point(), None);

    // The controller answers for X only: Y stays unknown, X is shown as read.
    r.peer.script(Step::expect(live(LiveRequest::ReadPosition)).reply(Reply::Ack).reply(Reply::Position {
        x_um: Some(12_500),
        y_um: None,
    }));
    r.actor.live(LiveRequest::ReadPosition).unwrap();
    r.actor.poll().unwrap();
    let p = r.actor.position();
    assert_eq!((p.x.mm(), p.y.mm()), (Some(12.5), None));
    assert_eq!(p.machine_point(), None, "a point needs both axes");
    assert!(p.read_at.is_some());
}

#[test]
fn a_known_position_of_exactly_zero_is_known_not_unknown() {
    let mut r = rig();
    r.peer.script(Step::expect(live(LiveRequest::ReadPosition)).reply(Reply::Position { x_um: Some(0), y_um: Some(0) }));
    r.actor.live(LiveRequest::ReadPosition).unwrap();
    r.actor.poll().unwrap();
    assert_eq!(r.actor.position().machine_point().map(|p| (p.x_mm(), p.y_mm())), Some((0.0, 0.0)));
}

#[test]
fn a_motion_command_makes_the_known_position_unknown_until_it_is_read_again() {
    let mut r = rig();
    r.peer.script(Step::expect(live(LiveRequest::ReadPosition)).reply(Reply::Position { x_um: Some(1_000), y_um: Some(2_000) }));
    r.actor.live(LiveRequest::ReadPosition).unwrap();
    r.actor.poll().unwrap();
    assert!(r.actor.position().machine_point().is_some());

    r.peer.script(Step::expect(live(LiveRequest::Home(HomeTarget::Xy))).reply(Reply::Ack));
    r.actor.live(LiveRequest::Home(HomeTarget::Xy)).unwrap();
    assert_eq!(r.actor.position().x.mm(), None, "stale coordinates must not look current");
    r.actor.poll().unwrap();
    assert_eq!(r.actor.position().machine_point(), None, "an ACK to home is not a position");
}

#[test]
fn read_position_is_an_idempotent_query_retried_within_the_bound() {
    let mut r = rig();
    // First attempt: no reply at all. Second attempt: answered.
    r.peer.script(Step::expect(live(LiveRequest::ReadPosition)));
    r.peer.script(Step::expect(live(LiveRequest::ReadPosition)).reply(Reply::Position { x_um: Some(5), y_um: Some(6) }));
    r.actor.live(LiveRequest::ReadPosition).unwrap();
    r.time_out();
    r.actor.poll().unwrap();
    assert_eq!(r.sent().len(), 2);
    assert_eq!(r.actor.position().x.mm(), Some(0.005));
    assert_eq!(r.last_outcome(), LiveOutcome::Answered);
    r.peer.verify_complete().unwrap();
}

// ---- gating ----

#[test]
fn unverified_live_features_are_refused_with_the_needed_evidence_and_nothing_is_sent() {
    let mut r = rig_with(Capabilities::default());
    let all = [
        (LiveRequest::JogStart(JogKey::XLeft), LiveFeature::Jog),
        (LiveRequest::Home(HomeTarget::Xy), LiveFeature::Home),
        (LiveRequest::MoveAbsolute { x_um: 1, y_um: 2 }, LiveFeature::MoveAbsolute),
        (LiveRequest::Pause, LiveFeature::PauseResume),
        (LiveRequest::Resume, LiveFeature::PauseResume),
        (LiveRequest::ReadPosition, LiveFeature::ReadPosition),
        (LiveRequest::ListFiles, LiveFeature::FileList),
        (LiveRequest::FileMetadata(name("A")), LiveFeature::FileMetadata),
        (LiveRequest::RunFile(name("A")), LiveFeature::FileRun),
        (LiveRequest::DeleteFile(name("A")), LiveFeature::FileDelete),
    ];
    for (request, feature) in all {
        match r.actor.live(request.clone()) {
            Err(LiveError::NotVerified { feature: got, needs, .. }) => {
                assert_eq!(got, feature, "{request:?}");
                assert!(needs.len() > 20, "{request:?}: the refusal must say which hardware evidence is missing");
            }
            other => panic!("{request:?}: {other:?}"),
        }
    }
    assert!(r.sent().is_empty(), "a refused action must not touch the machine: {:?}", r.sent());
    assert!(r.actor.live_attempts().is_empty(), "refusals are not machine attempts");
}

#[test]
fn an_unsupported_feature_is_refused_as_unsupported_not_as_unverified() {
    let mut caps = all_supported();
    caps.live.home = Capability::Unsupported;
    let mut r = rig_with(caps);
    assert_eq!(
        r.actor.live(LiveRequest::Home(HomeTarget::Xy)),
        Err(LiveError::Unsupported { feature: LiveFeature::Home })
    );
    assert!(r.sent().is_empty());
}

#[test]
fn motion_is_refused_while_a_job_may_be_running_but_pause_and_reads_are_not() {
    let mut r = rig();
    r.adopt_unknown_job();
    for request in [
        LiveRequest::JogStart(JogKey::YTop),
        LiveRequest::Home(HomeTarget::Xy),
        LiveRequest::MoveAbsolute { x_um: 0, y_um: 0 },
        LiveRequest::RunFile(name("A")),
        LiveRequest::DeleteFile(name("A")),
    ] {
        assert!(matches!(r.actor.live(request.clone()), Err(LiveError::JobMayBeActive(_))), "{request:?}");
    }
    assert!(r.sent().is_empty());

    r.peer.script(Step::expect(live(LiveRequest::Pause)).reply(Reply::Ack));
    r.actor.live(LiveRequest::Pause).unwrap();
    r.actor.poll().unwrap();
    r.peer.script(Step::expect(live(LiveRequest::ReadPosition)).reply(Reply::Position { x_um: Some(1), y_um: Some(1) }));
    r.actor.live(LiveRequest::ReadPosition).unwrap();
}

#[test]
fn pause_and_resume_are_separate_requests_and_never_change_the_execution() {
    let mut r = rig();
    r.adopt_unknown_job();
    let before = r.actor.execution().unwrap().clone();
    r.peer.script(Step::expect(live(LiveRequest::Pause)).reply(Reply::Ack));
    r.peer.script(Step::expect(live(LiveRequest::Resume)).reply(Reply::Ack));
    r.actor.live(LiveRequest::Pause).unwrap();
    r.actor.poll().unwrap();
    assert_eq!(r.last_outcome(), LiveOutcome::Acked, "ACK = received, nothing more");
    r.actor.live(LiveRequest::Resume).unwrap();
    r.actor.poll().unwrap();
    assert_eq!(r.actor.execution().unwrap(), &before, "pause/resume ACKs prove neither running nor completion");
    assert_eq!(r.sent(), vec![live(LiveRequest::Pause), live(LiveRequest::Resume)]);
    assert!(!r.sent().contains(&Request::Start));
}

#[test]
fn a_second_request_while_one_is_in_flight_is_refused() {
    let mut r = rig();
    r.peer.script(Step::expect(live(LiveRequest::Home(HomeTarget::Xy))));
    r.actor.live(LiveRequest::Home(HomeTarget::Xy)).unwrap();
    assert_eq!(r.actor.live(LiveRequest::Pause), Err(LiveError::RequestInFlight));
    assert_eq!(r.sent().len(), 1);
}

// ---- failure behaviour ----

#[test]
fn a_lost_reply_to_a_motion_command_is_unknown_and_the_command_is_never_resent() {
    let mut r = rig();
    r.peer.script(Step::expect(live(LiveRequest::MoveAbsolute { x_um: 10_000, y_um: 20_000 })));
    r.actor.live(LiveRequest::MoveAbsolute { x_um: 10_000, y_um: 20_000 }).unwrap();
    r.time_out();
    r.time_out();
    assert!(matches!(r.last_outcome(), LiveOutcome::Unknown(_)), "{:?}", r.last_outcome());
    assert_eq!(r.sent().len(), 1, "non-idempotent: sent exactly once");
    assert_eq!(r.actor.position().machine_point(), None);
}

#[test]
fn a_nak_or_err_to_home_is_a_refusal_not_a_success() {
    for reply in [Reply::Nak, Reply::Err] {
        let mut r = rig();
        r.peer.script(Step::expect(live(LiveRequest::Home(HomeTarget::Xy))).reply(reply.clone()));
        r.actor.live(LiveRequest::Home(HomeTarget::Xy)).unwrap();
        r.actor.poll().unwrap();
        assert_eq!(r.last_outcome(), LiveOutcome::Refused, "{reply:?}");
        assert_eq!(r.sent().len(), 1);
    }
}

#[test]
fn a_transport_without_a_verified_wire_form_marks_the_attempt_not_sent() {
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
    let clock = FakeClock::new();
    let identity = MachineIdentity { id: MachineId::new("m"), model: Some("x".into()), firmware: None };
    let mut actor = ExecutionActor::new(identity, Refusing, Arc::new(clock), Policy::default());
    actor.set_capabilities(all_supported());
    let err = actor.live(LiveRequest::Home(HomeTarget::Xy)).unwrap_err();
    assert!(matches!(err, LiveError::Port(PortError::NotSupported(_))), "{err:?}");
    assert_eq!(actor.live_attempts().last().unwrap().outcome, LiveOutcome::NotSent);
    // And the actor is usable again: nothing stays in flight.
    assert_eq!(actor.in_flight_tag(), None);
}

#[test]
fn reconnect_resends_nothing_and_turns_an_in_flight_motion_into_unknown() {
    let mut r = rig();
    r.peer.script(Step::expect(live(LiveRequest::Home(HomeTarget::Xy))));
    r.actor.live(LiveRequest::Home(HomeTarget::Xy)).unwrap();
    r.actor.reconnect().unwrap();
    r.actor.poll().unwrap();
    assert!(matches!(r.last_outcome(), LiveOutcome::Unknown(_)));
    assert_eq!(r.sent().len(), 1);
}

// ---- jog keys ----

#[test]
fn jog_press_and_release_are_two_requests_and_a_second_key_is_refused_while_one_is_held() {
    let mut r = rig();
    r.peer.script(Step::expect(live(LiveRequest::JogStart(JogKey::XLeft))).reply(Reply::Ack));
    r.peer.script(Step::expect(live(LiveRequest::JogStop(JogKey::XLeft))).reply(Reply::Ack));
    r.actor.live(LiveRequest::JogStart(JogKey::XLeft)).unwrap();
    r.actor.poll().unwrap();
    assert_eq!(r.actor.jog_held(), Some(JogKey::XLeft));
    assert_eq!(r.actor.live(LiveRequest::JogStart(JogKey::YTop)), Err(LiveError::JogKeyHeld(JogKey::XLeft)));
    r.actor.live(LiveRequest::JogStop(JogKey::XLeft)).unwrap();
    r.actor.poll().unwrap();
    assert_eq!(r.actor.jog_held(), None);
    r.peer.verify_complete().unwrap();
}

#[test]
fn a_lost_jog_start_reply_keeps_the_key_held_so_the_release_stays_possible() {
    let mut r = rig();
    r.peer.script(Step::expect(live(LiveRequest::JogStart(JogKey::YBottom))));
    r.actor.live(LiveRequest::JogStart(JogKey::YBottom)).unwrap();
    r.time_out();
    assert!(matches!(r.last_outcome(), LiveOutcome::Unknown(_)));
    assert_eq!(r.actor.jog_held(), Some(JogKey::YBottom), "the head may still be moving");
    // Releasing is always allowed, also with motion otherwise blocked.
    r.peer.script(Step::expect(live(LiveRequest::JogStop(JogKey::YBottom))).reply(Reply::Ack));
    assert!(r.actor.release_jog().unwrap());
    r.actor.poll().unwrap();
    assert_eq!(r.actor.jog_held(), None);
    assert!(!r.actor.release_jog().unwrap(), "nothing held, nothing sent");
}

#[test]
fn jog_release_is_allowed_even_when_the_feature_is_not_verified() {
    // A held key can only come from an earlier verified press, but the release must never be
    // blocked by a capability change (focus loss must always be able to stop the head).
    let mut r = rig();
    r.peer.script(Step::expect(live(LiveRequest::JogStart(JogKey::ZUp))).reply(Reply::Ack));
    r.actor.live(LiveRequest::JogStart(JogKey::ZUp)).unwrap();
    r.actor.poll().unwrap();
    r.actor.set_capabilities(Capabilities::default());
    r.peer.script(Step::expect(live(LiveRequest::JogStop(JogKey::ZUp))).reply(Reply::Ack));
    assert!(r.actor.release_jog().unwrap());
}

// ---- DSP files ----

#[test]
fn file_list_and_metadata_replies_are_kept_as_data() {
    let mut r = rig();
    let entries = vec![
        FileEntry { name: name("PLAAT01"), size_bytes: Some(1234) },
        FileEntry { name: name("LOGO"), size_bytes: None },
    ];
    r.peer.script(Step::expect(live(LiveRequest::ListFiles)).reply(Reply::Ack).reply(Reply::Files(entries.clone())));
    r.actor.live(LiveRequest::ListFiles).unwrap();
    r.actor.poll().unwrap();
    assert_eq!(r.actor.controller_files(), Some(entries.as_slice()));
    assert_eq!(r.last_outcome(), LiveOutcome::Answered);

    let meta = FileEntry { name: name("PLAAT01"), size_bytes: Some(1234) };
    r.peer.script(Step::expect(live(LiveRequest::FileMetadata(name("PLAAT01")))).reply(Reply::FileMetadata(meta.clone())));
    r.actor.live(LiveRequest::FileMetadata(name("PLAAT01"))).unwrap();
    r.actor.poll().unwrap();
    assert_eq!(r.actor.file_metadata(&name("PLAAT01")), Some(&meta));
}

#[test]
fn a_file_list_for_the_wrong_request_is_not_taken_as_the_list() {
    let mut r = rig();
    r.peer.script(Step::expect(live(LiveRequest::Pause)).reply(Reply::Files(vec![])));
    r.actor.live(LiveRequest::Pause).unwrap();
    r.actor.poll().unwrap();
    assert_eq!(r.actor.controller_files(), None);
    assert_eq!(r.actor.ignored().len(), 1);
}

#[test]
fn running_a_controller_file_is_its_own_request_and_never_the_job_start() {
    let mut r = rig();
    r.peer.script(Step::expect(live(LiveRequest::RunFile(name("PLAAT01")))).reply(Reply::Ack));
    r.actor.live(LiveRequest::RunFile(name("PLAAT01"))).unwrap();
    r.actor.poll().unwrap();
    assert_eq!(r.sent(), vec![live(LiveRequest::RunFile(name("PLAAT01")))]);
    assert!(r.actor.execution().is_none(), "running a stored file creates no execution and no start");
    assert_eq!(r.last_outcome(), LiveOutcome::Acked, "an ACK does not say the file ran");
}

#[test]
fn saving_an_rd_file_to_the_controller_has_no_wire_form_even_when_the_capability_is_supported() {
    let mut r = rig();
    let plan = {
        let source = PlanSource {
            document_revision: RevisionId::INITIAL,
            source_hash: ContentHash::of(HashDomain::Plan, b"save"),
            machine_profile: ProfileRevision {
                id: "bench".into(),
                revision: 1,
                content: ContentHash::of(HashDomain::Profile, b"bench"),
            },
            compiler_version: "test".into(),
        };
        let frame = MachineFrame::new(100.0, 100.0, OriginCorner::BottomLeft).unwrap();
        JobPlanBuilder::new(source, frame, MachinePoint::new(0.0, 0.0).unwrap()).finish().unwrap().id()
    };
    let artifact = RdArtifact::from_rd_file(plan, &[1, 2, 3], 2).unwrap();
    let err = r.actor.save_rd_to_controller(&name("PLAAT01"), &artifact).unwrap_err();
    assert_eq!(err, LiveError::NoWireForm { feature: LiveFeature::FileUpload });
    assert!(r.sent().is_empty());

    let mut unverified = rig_with(Capabilities::default());
    assert!(matches!(
        unverified.actor.save_rd_to_controller(&name("PLAAT01"), &artifact),
        Err(LiveError::NotVerified { feature: LiveFeature::FileUpload, .. })
    ));
}
