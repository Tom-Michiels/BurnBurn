//! Execution state machine against a scripted fake controller and a fake clock.
//! No real laser, socket or wall clock is involved.
#![allow(clippy::unwrap_used, clippy::panic)] // test harness outside #[test] fns

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use burnburn_job::{JobPlanBuilder, JobPlanId, MachineFrame, MachinePoint, OriginCorner, PlanSource, ProfileRevision};
use burnburn_machine::{
    ActorError, ControllerIdentity, ControllerPort, ControllerStatus, Envelope, Evidence, ExecutionActor, ExecutionId,
    ExecutionState, FailReason, Incoming, MachineId, MachineIdentity, Policy, PortError, RdArtifact, Reply, Request,
    RequestTag, UnknownReason,
};
use burnburn_test_support::{FakeClock, PeerError, ScriptedPeer, Step};
use burnburn_types::{ContentHash, HashDomain, RevisionId};

type Peer = ScriptedPeer<Request, Reply>;
type Injected = Arc<Mutex<VecDeque<Incoming>>>;

/// Test adapter: like a UDP adapter, it attributes each controller reply to the request that is
/// in flight (the last one sent). `injected` lets a test deliver stray datagrams.
struct FakePort {
    peer: Peer,
    machine: MachineId,
    last_tag: Option<RequestTag>,
    injected: Injected,
}

impl ControllerPort for FakePort {
    fn send(&mut self, envelope: Envelope) -> Result<(), PortError> {
        assert_eq!(envelope.to, self.machine, "actor addressed another machine");
        self.last_tag = Some(envelope.tag);
        self.peer.send(envelope.request).map_err(port_error)
    }

    fn try_recv(&mut self) -> Result<Option<Incoming>, PortError> {
        if let Some(stray) = self.injected.lock().unwrap().pop_front() {
            return Ok(Some(stray));
        }
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
    injected: Injected,
    actor: ExecutionActor<FakePort>,
}

fn rig() -> Rig {
    let clock = FakeClock::new();
    let peer = Peer::new(clock.clone());
    let machine = MachineId::new("bench-ruida-1");
    let injected: Injected = Arc::default();
    let port = FakePort { peer: peer.clone(), machine: machine.clone(), last_tag: None, injected: injected.clone() };
    let policy = Policy { reply_timeout: TIMEOUT, max_query_attempts: 3 };
    let actor = ExecutionActor::new(profile_identity(machine), port, Arc::new(clock.clone()), policy);
    Rig { clock, peer, injected, actor }
}

const JOB: u64 = 7;

fn profile_identity(id: MachineId) -> MachineIdentity {
    MachineIdentity { id, model: Some("RDC6445G".into()), firmware: Some("fake-1".into()) }
}

fn reported(model: &str) -> Reply {
    Reply::Identity(ControllerIdentity { model: Some(model.into()), firmware: Some("fake-1".into()) })
}

/// A tiny frozen plan, only to have a real `JobPlanId`.
fn plan_id() -> JobPlanId {
    let source = PlanSource {
        document_revision: RevisionId::INITIAL,
        source_hash: ContentHash::of(HashDomain::Plan, b"machine-test"),
        machine_profile: ProfileRevision {
            id: "bench".into(),
            revision: 1,
            content: ContentHash::of(HashDomain::Profile, b"bench"),
        },
        compiler_version: "test".into(),
    };
    let frame = MachineFrame::new(100.0, 100.0, OriginCorner::BottomLeft).unwrap();
    JobPlanBuilder::new(source, frame, MachinePoint::new(0.0, 0.0).unwrap()).finish().unwrap().id()
}

const RD_FILE: [u8; 3] = [0xd8, 0x10, 0x89];

fn artifact() -> RdArtifact {
    RdArtifact::from_rd_file(plan_id(), &RD_FILE, 2).unwrap()
}

fn chunks() -> Vec<Vec<u8>> {
    artifact().chunks().to_vec()
}

fn chunk(index: usize) -> Request {
    Request::UploadChunk { index, bytes: chunks()[index].clone() }
}

impl Rig {
    fn state(&self) -> ExecutionState {
        self.actor.execution().unwrap().state.clone()
    }

    fn evidence(&self) -> Vec<Evidence> {
        self.actor.execution().unwrap().evidence.iter().map(|e| e.evidence.clone()).collect()
    }

    fn sent_count(&self, request: &Request) -> usize {
        self.peer.sent().iter().filter(|r| *r == request).count()
    }

    /// The controller confirms the profile's identity (precedes every upload).
    fn expect_identity(&mut self) {
        self.peer.script(Step::expect(Request::QueryIdentity).reply(Reply::Ack).reply(reported("RDC6445G")));
    }

    fn time_out(&mut self) {
        self.clock.advance(TIMEOUT + Duration::from_millis(1));
        self.actor.poll().unwrap();
    }

    /// Scripted happy path up to a confirmed stored job.
    fn upload_and_store(&mut self) {
        self.expect_identity();
        self.peer.script(Step::expect(chunk(0)).reply(Reply::Ack));
        self.peer.script(Step::expect(chunk(1)).reply(Reply::Ack));
        self.peer.script(Step::expect(Request::QueryStored).reply(Reply::Ack).reply(Reply::Stored(true)));
        self.actor.begin_upload(ExecutionId::from_raw(JOB), &artifact()).unwrap();
        self.actor.poll().unwrap();
        self.actor.confirm_stored().unwrap();
        self.actor.poll().unwrap();
        assert_eq!(self.state(), ExecutionState::Stored);
    }
}

#[test]
fn ack_is_not_stored() {
    let mut r = rig();
    r.expect_identity();
    r.peer.script(Step::expect(chunk(0)).reply(Reply::Ack));
    r.peer.script(Step::expect(chunk(1)).reply(Reply::Ack));

    r.actor.begin_upload(ExecutionId::from_raw(JOB), &artifact()).unwrap();
    r.actor.poll().unwrap();

    assert_eq!(r.state(), ExecutionState::UploadAcked);
    let ev = r.evidence();
    assert!(ev.contains(&Evidence::ChunkAcked { index: 0 }) && ev.contains(&Evidence::ChunkAcked { index: 1 }));
    assert!(!ev.iter().any(|e| matches!(e, Evidence::StorageConfirmed { .. })));
    r.peer.verify_complete().unwrap();
}

#[test]
fn stored_is_not_completed() {
    let mut r = rig();
    r.upload_and_store();

    r.peer.script(Step::expect(Request::Start).reply(Reply::Ack));
    r.actor.start().unwrap();
    r.actor.poll().unwrap();
    assert_eq!(r.state(), ExecutionState::StartRequested, "a start ACK is not proof of running");

    r.peer.script(Step::expect(Request::QueryStatus).reply(Reply::Status(ControllerStatus::Running)));
    r.actor.query_status().unwrap();
    r.actor.poll().unwrap();
    assert_eq!(r.state(), ExecutionState::Running);

    // Idle without a finished report proves nothing about completion.
    r.peer.script(Step::expect(Request::QueryStatus).reply(Reply::Status(ControllerStatus::Idle)));
    r.actor.query_status().unwrap();
    r.actor.poll().unwrap();
    assert_ne!(r.state(), ExecutionState::Completed);

    r.peer.script(Step::expect(Request::QueryStatus).reply(Reply::Status(ControllerStatus::Finished)));
    r.actor.query_status().unwrap();
    r.actor.poll().unwrap();
    assert_eq!(r.state(), ExecutionState::Completed);
    r.peer.verify_complete().unwrap();
}

#[test]
fn lost_start_reply_does_not_cause_second_start_after_reconnect() {
    let mut r = rig();
    r.upload_and_store();

    r.peer.script(Step::expect(Request::Start).lose_reply(Reply::Ack));
    r.actor.start().unwrap();
    r.actor.poll().unwrap();
    assert_eq!(r.state(), ExecutionState::StartRequested);

    r.time_out();
    assert_eq!(r.state(), ExecutionState::Unknown(UnknownReason::StartReplyLost));

    // Connection drops and comes back; the actor must not "recover" by starting again.
    r.peer.disconnect();
    r.actor.reconnect().unwrap();
    r.actor.poll().unwrap();
    r.time_out();
    r.time_out();
    assert_eq!(r.actor.start(), Err(ActorError::StartAlreadySent));

    // An idempotent status query is still allowed, and does not resolve Unknown by itself.
    r.peer.script(Step::expect(Request::QueryStatus).reply(Reply::Status(ControllerStatus::Running)));
    r.actor.query_status().unwrap();
    r.actor.poll().unwrap();

    assert_eq!(r.sent_count(&Request::Start), 1, "exactly one start ever sent");
    assert_eq!(r.state(), ExecutionState::Unknown(UnknownReason::StartReplyLost));
    assert!(r.evidence().contains(&Evidence::Status(ControllerStatus::Running)));
    r.peer.verify_complete().unwrap();
}

#[test]
fn cancel_does_not_delete_implicit_controller_file() {
    // After a confirmed store: cancel is local; nothing is sent (no delete, no start).
    let mut r = rig();
    r.upload_and_store();
    let sent_before = r.peer.sent().len();
    r.actor.cancel().unwrap();
    r.actor.poll().unwrap();
    r.time_out();
    assert_eq!(r.state(), ExecutionState::Cancelled);
    assert_eq!(r.peer.sent().len(), sent_before, "cancel sent a request to the controller");
    r.peer.verify_complete().unwrap();

    // Mid-upload: no further chunks, nothing deleted, and the possible remainder is recorded.
    let mut r = rig();
    r.expect_identity();
    r.peer.script(Step::expect(chunk(0)).reply_after(Duration::from_millis(100), Reply::Ack));
    r.actor.begin_upload(ExecutionId::from_raw(JOB), &artifact()).unwrap();
    r.actor.poll().unwrap(); // identity confirmed, chunk 0 in flight
    r.actor.cancel().unwrap();
    r.clock.advance(Duration::from_millis(200));
    r.actor.poll().unwrap();
    assert_eq!(r.state(), ExecutionState::Cancelled);
    assert_eq!(r.peer.sent(), vec![Request::QueryIdentity, chunk(0)]);
    assert!(r.evidence().contains(&Evidence::ControllerFileMayRemain));
}

#[test]
fn reply_for_other_machine_or_old_execution_is_ignored() {
    let mut r = rig();
    r.expect_identity();
    r.peer.script(Step::expect(chunk(0)).reply(Reply::Ack));
    r.peer.script(Step::expect(chunk(1)).reply(Reply::Ack));
    r.peer.script(Step::expect(Request::QueryStored).reply_after(Duration::from_millis(100), Reply::Stored(true)));
    r.actor.begin_upload(ExecutionId::from_raw(JOB), &artifact()).unwrap();
    r.actor.poll().unwrap();
    r.actor.confirm_stored().unwrap();

    // Find the tag of the in-flight QueryStored from what the fake recorded.
    let current = r.actor.in_flight_tag().unwrap();
    let other_machine = Incoming { from: MachineId::new("other-ruida"), tag: current, reply: Reply::Stored(true) };
    let old_execution = Incoming {
        from: MachineId::new("bench-ruida-1"),
        tag: RequestTag { execution: ExecutionId::from_raw(6), ..current },
        reply: Reply::Stored(true),
    };
    let old_epoch = Incoming {
        from: MachineId::new("bench-ruida-1"),
        tag: RequestTag { epoch: current.epoch.wrapping_add(1), ..current },
        reply: Reply::Stored(true),
    };
    r.injected.lock().unwrap().extend([other_machine.clone(), old_execution.clone(), old_epoch.clone()]);
    r.actor.poll().unwrap();

    assert_eq!(r.state(), ExecutionState::UploadAcked, "stray reply changed the state");
    assert_eq!(r.actor.ignored(), &[other_machine, old_execution, old_epoch]);

    // The genuine reply still counts.
    r.clock.advance(Duration::from_millis(100));
    r.actor.poll().unwrap();
    assert_eq!(r.state(), ExecutionState::Stored);
}

#[test]
fn idempotent_query_is_retried_only_up_to_the_bound() {
    let mut r = rig();
    r.expect_identity();
    r.peer.script(Step::expect(chunk(0)).reply(Reply::Ack));
    r.peer.script(Step::expect(chunk(1)).reply(Reply::Ack));
    for _ in 0..3 {
        r.peer.script(Step::expect(Request::QueryStored)); // silently ignored
    }
    r.actor.begin_upload(ExecutionId::from_raw(JOB), &artifact()).unwrap();
    r.actor.poll().unwrap();
    r.actor.confirm_stored().unwrap();
    for _ in 0..5 {
        r.time_out();
    }
    assert_eq!(r.sent_count(&Request::QueryStored), 3);
    // Not verified is reported as such: still only UploadAcked.
    assert_eq!(r.state(), ExecutionState::UploadAcked);
    assert!(r.evidence().contains(&Evidence::QueryUnanswered { request: Request::QueryStored }));
    r.peer.verify_complete().unwrap();
}

#[test]
fn lost_chunk_ack_is_unknown_and_chunk_is_not_resent() {
    let mut r = rig();
    r.expect_identity();
    r.peer.script(Step::expect(chunk(0)).lose_reply(Reply::Ack));
    r.actor.begin_upload(ExecutionId::from_raw(JOB), &artifact()).unwrap();
    r.time_out();
    r.time_out();
    assert_eq!(r.state(), ExecutionState::Unknown(UnknownReason::UploadChunkUnconfirmed { index: 0 }));
    assert_eq!(r.peer.sent(), vec![Request::QueryIdentity, chunk(0)]);
    assert!(matches!(r.actor.start(), Err(ActorError::InvalidState(_))));
}

#[test]
fn start_requires_stored_evidence() {
    let mut r = rig();
    r.expect_identity();
    r.peer.script(Step::expect(chunk(0)).reply(Reply::Ack));
    r.peer.script(Step::expect(chunk(1)).reply(Reply::Ack));
    r.actor.begin_upload(ExecutionId::from_raw(JOB), &artifact()).unwrap();
    r.actor.poll().unwrap();
    assert_eq!(r.actor.start(), Err(ActorError::InvalidState(ExecutionState::UploadAcked)));
    assert_eq!(r.sent_count(&Request::Start), 0);
}

#[test]
fn duplicated_ack_is_not_counted_for_the_next_chunk() {
    let mut r = rig();
    r.expect_identity();
    r.peer.script(Step::expect(chunk(0)).reply_duplicated(Reply::Ack, 2));
    r.peer.script(Step::expect(chunk(1)).lose_reply(Reply::Ack));
    r.actor.begin_upload(ExecutionId::from_raw(JOB), &artifact()).unwrap();
    r.actor.poll().unwrap();
    assert_eq!(r.state(), ExecutionState::Uploading { acked: 1, total: 2 });
    r.time_out();
    assert_eq!(r.state(), ExecutionState::Unknown(UnknownReason::UploadChunkUnconfirmed { index: 1 }));
}

#[test]
fn stored_evidence_names_the_uploaded_rd_hash() {
    let mut r = rig();
    r.upload_and_store();
    let exec = r.actor.execution().unwrap().clone();
    let expected = ContentHash::of(HashDomain::Rd, &RD_FILE);
    assert_eq!((exec.plan, exec.rd_hash), (plan_id(), expected), "execution names plan and RD bytes");
    assert_eq!(
        r.peer.sent()[1..3]
            .iter()
            .flat_map(|q| match q {
                Request::UploadChunk { bytes, .. } => bytes.clone(),
                _ => Vec::new(),
            })
            .collect::<Vec<_>>(),
        RD_FILE.to_vec(),
        "uploaded bytes are exactly the hashed file"
    );
    assert!(
        r.evidence().contains(&Evidence::StorageConfirmed { rd_hash: expected }),
        "Stored evidence must name the uploaded RD hash: {:?}",
        r.evidence()
    );
}

#[test]
fn identity_mismatch_prevents_upload() {
    let mut r = rig();
    r.peer.script(Step::expect(Request::QueryIdentity).reply(reported("RDC6442S")));
    r.actor.begin_upload(ExecutionId::from_raw(JOB), &artifact()).unwrap();
    r.actor.poll().unwrap();
    r.time_out();
    assert_eq!(r.state(), ExecutionState::Failed(FailReason::IdentityMismatch));
    assert_eq!(r.peer.sent(), vec![Request::QueryIdentity], "no job byte may leave before identity matches");
    let ev = r.evidence();
    assert!(ev.iter().any(
        |e| matches!(e, Evidence::IdentityMismatch { reported } if reported.model.as_deref() == Some("RDC6442S"))
    ));
    r.peer.verify_complete().unwrap();
}

#[test]
fn unanswered_identity_query_fails_without_uploading() {
    let mut r = rig();
    for _ in 0..3 {
        r.peer.script(Step::expect(Request::QueryIdentity));
    }
    r.actor.begin_upload(ExecutionId::from_raw(JOB), &artifact()).unwrap();
    for _ in 0..5 {
        r.time_out();
    }
    assert_eq!(r.state(), ExecutionState::Failed(FailReason::IdentityUnconfirmed));
    assert_eq!(r.sent_count(&Request::QueryIdentity), 3, "bounded idempotent retries");
    assert!(r.peer.sent().iter().all(|q| *q == Request::QueryIdentity));
    r.peer.verify_complete().unwrap();
}

#[test]
fn unverifiable_profile_identity_refuses_upload() {
    let clock = FakeClock::new();
    let peer = Peer::new(clock.clone());
    let machine = MachineId::new("bench-ruida-1");
    let port = FakePort { peer: peer.clone(), machine: machine.clone(), last_tag: None, injected: Arc::default() };
    let mut actor = ExecutionActor::new(MachineIdentity::unprobed(machine), port, Arc::new(clock), Policy::default());
    assert_eq!(actor.begin_upload(ExecutionId::from_raw(JOB), &artifact()), Err(ActorError::IdentityNotConfigured));
    assert!(peer.sent().is_empty());
    assert!(actor.execution().is_none());
}

#[test]
fn reverify_reads_back_without_resending_job_bytes() {
    let mut r = rig();
    r.expect_identity();
    r.peer.script(Step::expect(Request::QueryStored).reply(Reply::Stored(true)));
    r.actor.begin_reverify(ExecutionId::from_raw(JOB), &artifact()).unwrap();
    r.actor.poll().unwrap();
    r.actor.poll().unwrap();
    assert_eq!(r.state(), ExecutionState::Stored);
    assert_eq!(r.peer.sent(), vec![Request::QueryIdentity, Request::QueryStored], "no chunk is sent again");
    let expected = ContentHash::of(HashDomain::Rd, &RD_FILE);
    assert!(r.evidence().contains(&Evidence::StorageConfirmed { rd_hash: expected }));

    // Denied read-back fails; start stays impossible.
    let mut r = rig();
    r.expect_identity();
    r.peer.script(Step::expect(Request::QueryStored).reply(Reply::Stored(false)));
    r.actor.begin_reverify(ExecutionId::from_raw(JOB), &artifact()).unwrap();
    r.actor.poll().unwrap();
    r.actor.poll().unwrap();
    assert_eq!(r.state(), ExecutionState::Failed(FailReason::NotStored));
    assert!(matches!(r.actor.start(), Err(ActorError::InvalidState(_))));
}

/// An execution rebuilt from the history after a restart, as the history recorded it.
fn interrupted(start_sent: bool) -> burnburn_machine::Execution {
    burnburn_machine::Execution {
        id: ExecutionId::from_raw(JOB),
        machine: MachineId::new("bench-ruida-1"),
        plan: plan_id(),
        rd_hash: artifact().rd_hash(),
        state: ExecutionState::Unknown(UnknownReason::InterruptedByRestart),
        evidence: Vec::new(),
        start_sent,
    }
}

/// ADR 0008 §5: after a restart only a status query may follow an unknown execution; a status
/// reply carries no job identity and leaves it unknown; nothing starts, nothing uploads.
#[test]
fn resumed_unknown_execution_allows_only_a_status_query() {
    let mut r = rig();
    r.actor.resume_unknown(interrupted(true)).unwrap();
    assert_eq!(r.state(), ExecutionState::Unknown(UnknownReason::InterruptedByRestart));
    assert!(r.actor.start().is_err(), "never a start for an unknown execution");
    assert_eq!(r.actor.begin_upload(ExecutionId::from_raw(JOB + 1), &artifact()), Err(ActorError::Busy));

    r.peer.script(Step::expect(Request::QueryStatus).reply(Reply::Status(ControllerStatus::Idle)));
    r.actor.query_status().unwrap();
    r.actor.poll().unwrap();
    assert_eq!(
        r.evidence(),
        vec![Evidence::Sent { request: Request::QueryStatus }, Evidence::Status(ControllerStatus::Idle)]
    );
    assert_eq!(r.state(), ExecutionState::Unknown(UnknownReason::InterruptedByRestart));
    assert_eq!(r.peer.sent(), vec![Request::QueryStatus]);
}

/// Only an unknown execution may be adopted, and only on an idle actor.
#[test]
fn resume_refuses_known_states_and_a_busy_actor() {
    let mut r = rig();
    let mut stored = interrupted(false);
    stored.state = ExecutionState::Stored;
    assert_eq!(r.actor.resume_unknown(stored), Err(ActorError::InvalidState(ExecutionState::Stored)));
    assert!(r.actor.execution().is_none(), "a refused resume adopts nothing");

    r.upload_and_store();
    assert_eq!(r.actor.resume_unknown(interrupted(true)), Err(ActorError::Busy));
    assert_eq!(r.state(), ExecutionState::Stored);
}

/// Settling: only from Unknown, only into a terminal state; afterwards the machine is free.
#[test]
fn settling_an_unknown_execution_is_explicit_and_terminal() {
    let mut r = rig();
    r.actor.resume_unknown(interrupted(true)).unwrap();
    assert_eq!(r.actor.settle_unknown(ExecutionState::Running), Err(ActorError::InvalidState(ExecutionState::Running)));
    assert_eq!(r.state(), ExecutionState::Unknown(UnknownReason::InterruptedByRestart));
    r.actor.settle_unknown(ExecutionState::Failed(FailReason::OperatorDidNotRun)).unwrap();
    assert_eq!(r.state(), ExecutionState::Failed(FailReason::OperatorDidNotRun));
    assert!(r.peer.sent().is_empty(), "settling sends nothing");
    assert_eq!(
        r.actor.settle_unknown(ExecutionState::Completed),
        Err(ActorError::InvalidState(ExecutionState::Failed(FailReason::OperatorDidNotRun)))
    );

    let mut s = rig();
    s.upload_and_store();
    assert_eq!(
        s.actor.settle_unknown(ExecutionState::Completed),
        Err(ActorError::InvalidState(ExecutionState::Stored))
    );
}
