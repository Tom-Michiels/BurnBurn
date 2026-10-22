//! G6-machine (M01): the identity the user declares for a machine is what a controller's reply is
//! compared with. It changes only while nothing runs, and a reply never writes it.
#![allow(clippy::unwrap_used, clippy::panic)] // test harness outside #[test] fns

use std::sync::Arc;
use std::time::Duration;

use burnburn_job::{JobPlanBuilder, JobPlanId, MachineFrame, MachinePoint, OriginCorner, PlanSource, ProfileRevision};
use burnburn_machine::{
    ActorError, ControllerIdentity, ControllerPort, Envelope, ExecutionActor, ExecutionId, ExecutionState, FailReason,
    Incoming, MachineId, MachineIdentity, Policy, PortError, RdArtifact, Reply, Request, RequestTag,
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
    PortError::Other(e.to_string())
}

fn rig(identity: impl FnOnce(MachineId) -> MachineIdentity) -> (ExecutionActor<FakePort>, Peer) {
    let clock = FakeClock::new();
    let peer = Peer::new(clock.clone());
    let machine = MachineId::new("bench-ruida-1");
    let port = FakePort { peer: peer.clone(), machine: machine.clone(), last_tag: None };
    let policy = Policy { reply_timeout: Duration::from_millis(500), max_query_attempts: 2 };
    (ExecutionActor::new(identity(machine), port, Arc::new(clock), policy), peer)
}

fn artifact() -> RdArtifact {
    let source = PlanSource {
        document_revision: RevisionId::INITIAL,
        source_hash: ContentHash::of(HashDomain::Plan, b"declared-identity"),
        machine_profile: ProfileRevision {
            id: "bench".into(),
            revision: 1,
            content: ContentHash::of(HashDomain::Profile, b"bench"),
        },
        compiler_version: "test".into(),
    };
    let frame = MachineFrame::new(100.0, 100.0, OriginCorner::BottomLeft).unwrap();
    let id: JobPlanId = JobPlanBuilder::new(source, frame, MachinePoint::new(0.0, 0.0).unwrap()).finish().unwrap().id();
    RdArtifact::from_rd_file(id, &[0xd8, 0x10, 0x89], 2).unwrap()
}

fn reported(model: &str) -> Reply {
    Reply::Identity(ControllerIdentity { model: Some(model.into()), firmware: None })
}

#[test]
fn a_machine_without_a_declared_identity_refuses_uploads_until_one_is_declared() {
    let (mut actor, peer) = rig(MachineIdentity::unprobed);
    assert_eq!(actor.begin_upload(ExecutionId::from_raw(1), &artifact()), Err(ActorError::IdentityNotConfigured));
    actor.declare_identity(Some("RDC6445G".into()), None).unwrap();
    peer.script(Step::expect(Request::QueryIdentity).reply(reported("RDC6445G")));
    actor.begin_upload(ExecutionId::from_raw(1), &artifact()).unwrap();
    actor.poll().unwrap();
    assert!(!matches!(actor.execution().unwrap().state, ExecutionState::Failed(_)));
}

#[test]
fn the_declared_model_is_what_the_reply_is_compared_with() {
    let (mut actor, peer) = rig(|id| MachineIdentity { id, model: Some("RDC6445G".into()), firmware: None });
    actor.declare_identity(Some("RDC6442S".into()), None).unwrap();
    assert_eq!(actor.machine().model.as_deref(), Some("RDC6442S"));
    peer.script(Step::expect(Request::QueryIdentity).reply(reported("RDC6445G")));
    actor.begin_upload(ExecutionId::from_raw(1), &artifact()).unwrap();
    actor.poll().unwrap();
    assert_eq!(actor.execution().unwrap().state, ExecutionState::Failed(FailReason::IdentityMismatch));
    peer.verify_complete().unwrap();
}

#[test]
fn clearing_the_declaration_makes_uploads_refuse_again() {
    let (mut actor, _peer) = rig(|id| MachineIdentity { id, model: Some("RDC6445G".into()), firmware: None });
    actor.declare_identity(None, None).unwrap();
    assert_eq!(actor.machine().model, None);
    assert_eq!(actor.begin_upload(ExecutionId::from_raw(1), &artifact()), Err(ActorError::IdentityNotConfigured));
}

#[test]
fn the_identity_does_not_change_while_an_execution_is_active() {
    let (mut actor, peer) = rig(|id| MachineIdentity { id, model: Some("RDC6445G".into()), firmware: None });
    peer.script(Step::expect(Request::QueryIdentity).reply(reported("RDC6445G")));
    actor.begin_upload(ExecutionId::from_raw(1), &artifact()).unwrap();
    assert_eq!(actor.declare_identity(Some("ander".into()), None), Err(ActorError::Busy));
    assert_eq!(actor.machine().model.as_deref(), Some("RDC6445G"), "refused: nothing changed");
}

#[test]
fn a_controller_reply_never_writes_the_declared_identity() {
    let (mut actor, peer) = rig(MachineIdentity::unprobed);
    actor.declare_identity(Some("RDC6445G".into()), None).unwrap();
    peer.script(
        Step::expect(Request::QueryIdentity).reply(Reply::Identity(ControllerIdentity {
            model: Some("RDC6445G".into()),
            firmware: Some("v9.99".into()),
        })),
    );
    actor.begin_upload(ExecutionId::from_raw(1), &artifact()).unwrap();
    actor.poll().unwrap();
    assert_eq!(actor.machine().firmware, None, "what the controller says about itself is evidence, not configuration");
}
