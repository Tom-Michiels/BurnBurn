mod live;

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use burnburn_types::Clock;

use crate::artifact::RdArtifact;
use crate::ids::{ExecutionId, MachineIdentity};
use crate::port::{ControllerPort, ControllerStatus, Envelope, Incoming, PortError, Reply, Request, RequestTag};
use crate::state::{Evidence, EvidenceEntry, Execution, ExecutionState, FailReason, UnknownReason};

/// Upper bound on replies processed per poll/drain, so a flooding peer cannot stall the actor.
const MAX_REPLIES_PER_POLL: usize = 64;

/// Timeout and retry policy. Values must come from measurement on the target machine; the
/// defaults are conservative placeholders, not LightBurn values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    pub reply_timeout: Duration,
    /// Total attempts for an idempotent query (1 = no retry).
    pub max_query_attempts: u32,
}

impl Default for Policy {
    fn default() -> Self {
        Policy { reply_timeout: Duration::from_secs(2), max_query_attempts: 3 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActorError {
    /// The profile has no model/firmware to compare with the controller; uploading would skip
    /// the identity check.
    IdentityNotConfigured,
    /// Another execution is still active on this actor.
    Busy,
    NoExecution,
    EmptyJob,
    /// The requested action is not allowed in the current state (e.g. start before Stored).
    InvalidState(ExecutionState),
    /// A start was already sent for this execution; it is never sent twice.
    StartAlreadySent,
    /// A request is still waiting for its reply.
    RequestInFlight,
    Port(PortError),
}

impl fmt::Display for ActorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ActorError {}

#[derive(Debug, Clone)]
struct InFlight {
    tag: RequestTag,
    request: Request,
    sent_at: burnburn_types::MonotonicTime,
    attempt: u32,
}

/// Drives one execution at a time on one machine. Single-threaded, step-driven: call
/// [`ExecutionActor::poll`] to process replies and timeouts.
pub struct ExecutionActor<P: ControllerPort> {
    machine: MachineIdentity,
    port: P,
    clock: Arc<dyn Clock>,
    policy: Policy,
    execution: Option<Execution>,
    chunks: Vec<Vec<u8>>,
    in_flight: Option<InFlight>,
    epoch: u32,
    next_seq: u32,
    ignored: Vec<Incoming>,
    /// The current execution re-verifies an earlier upload instead of uploading.
    reverify: bool,
    /// Laser-window state (live requests); independent of the execution state.
    pub(crate) live: crate::live::LiveState,
}

impl<P: ControllerPort> ExecutionActor<P> {
    pub fn new(machine: MachineIdentity, port: P, clock: Arc<dyn Clock>, policy: Policy) -> Self {
        ExecutionActor {
            machine,
            port,
            clock,
            policy,
            execution: None,
            chunks: Vec::new(),
            in_flight: None,
            epoch: 0,
            next_seq: 0,
            ignored: Vec::new(),
            reverify: false,
            live: crate::live::LiveState::default(),
        }
    }

    pub fn machine(&self) -> &MachineIdentity {
        &self.machine
    }

    /// Replaces the model and firmware the user declared for this machine (M01). A controller's
    /// reply is compared with it and never written into it. Refused while an execution is not
    /// terminal or a request waits for its reply, so one execution is checked against one identity.
    pub fn declare_identity(&mut self, model: Option<String>, firmware: Option<String>) -> Result<(), ActorError> {
        if self.execution.as_ref().is_some_and(|e| !e.state.is_terminal()) {
            return Err(ActorError::Busy);
        }
        if self.in_flight.is_some() {
            return Err(ActorError::RequestInFlight);
        }
        self.machine.model = model;
        self.machine.firmware = firmware;
        Ok(())
    }

    pub fn port(&self) -> &P {
        &self.port
    }

    pub fn execution(&self) -> Option<&Execution> {
        self.execution.as_ref()
    }

    /// Replies that were received but not attributed to the current request.
    pub fn ignored(&self) -> &[Incoming] {
        &self.ignored
    }

    /// Tag of the request currently waiting for a reply (diagnostics).
    pub fn in_flight_tag(&self) -> Option<RequestTag> {
        self.in_flight.as_ref().map(|f| f.tag)
    }

    /// Start a new execution: first the identity query, and only when the controller reports
    /// the profile's identity, the first chunk. Refused while another execution is not
    /// terminal; an `Unknown` execution needs explicit reconciliation first.
    pub fn begin_upload(&mut self, id: ExecutionId, artifact: &RdArtifact) -> Result<(), ActorError> {
        if self.execution.as_ref().is_some_and(|e| !e.state.is_terminal()) {
            return Err(ActorError::Busy);
        }
        if !self.machine.is_verifiable() {
            return Err(ActorError::IdentityNotConfigured);
        }
        if artifact.chunks().is_empty() {
            return Err(ActorError::EmptyJob);
        }
        self.execution = Some(Execution {
            id,
            machine: self.machine.id.clone(),
            plan: artifact.plan(),
            rd_hash: artifact.rd_hash(),
            state: ExecutionState::VerifyingIdentity,
            evidence: Vec::new(),
            start_sent: false,
        });
        self.chunks = artifact.chunks().to_vec();
        self.in_flight = None;
        self.reverify = false;
        // A failed send of a query is handled like a lost reply: bounded retry on timeout.
        let _ = self.send(Request::QueryIdentity, 1);
        Ok(())
    }

    /// Re-verify an execution uploaded in an earlier session (e.g. after a restart): identity
    /// query, then the stored read-back. Never sends job bytes and never starts anything; only
    /// a fresh `Stored(true)` makes [`ExecutionActor::start`] possible.
    pub fn begin_reverify(&mut self, id: ExecutionId, artifact: &RdArtifact) -> Result<(), ActorError> {
        self.begin_upload(id, artifact)?;
        self.reverify = true;
        Ok(())
    }

    /// Adopt an execution from an earlier session whose outcome is unknown (after a restart,
    /// rebuilt from the production history). Only a status query or an explicit settlement can
    /// follow; it never sends job bytes and never a start.
    pub fn resume_unknown(&mut self, execution: Execution) -> Result<(), ActorError> {
        if !matches!(execution.state, ExecutionState::Unknown(_)) {
            return Err(ActorError::InvalidState(execution.state));
        }
        if self.execution.as_ref().is_some_and(|e| !e.state.is_terminal()) {
            return Err(ActorError::Busy);
        }
        self.execution = Some(execution);
        self.chunks = Vec::new();
        self.in_flight = None;
        self.reverify = false;
        Ok(())
    }

    /// Settle an execution whose outcome is unknown into a terminal state, on an explicit
    /// statement (operator reconciliation). Sends nothing.
    pub fn settle_unknown(&mut self, to: ExecutionState) -> Result<(), ActorError> {
        let exec = self.execution.as_ref().ok_or(ActorError::NoExecution)?;
        if !matches!(exec.state, ExecutionState::Unknown(_)) {
            return Err(ActorError::InvalidState(exec.state.clone()));
        }
        if !to.is_terminal() {
            return Err(ActorError::InvalidState(to));
        }
        self.in_flight = None;
        self.set_state(to);
        Ok(())
    }

    /// Ask the controller to confirm the stored job (idempotent; retried within the policy).
    pub fn confirm_stored(&mut self) -> Result<(), ActorError> {
        self.ready_for_request(|s| matches!(s, ExecutionState::UploadAcked))?;
        // A failed send of a query is handled like a lost reply: bounded retry on timeout.
        let _ = self.send(Request::QueryStored, 1);
        Ok(())
    }

    /// Send the start request once. Requires `Stored` evidence. Never repeated, also not after
    /// a reconnect.
    pub fn start(&mut self) -> Result<(), ActorError> {
        if self.execution.as_ref().is_some_and(|e| e.start_sent) {
            return Err(ActorError::StartAlreadySent);
        }
        self.ready_for_request(|s| matches!(s, ExecutionState::Stored))?;
        if let Some(exec) = self.execution.as_mut() {
            exec.start_sent = true;
            exec.state = ExecutionState::StartRequested;
        }
        if let Err(e) = self.send(Request::Start, 1) {
            self.fail_in_flight_send(ExecutionState::Unknown(UnknownReason::StartSendFailed));
            return Err(ActorError::Port(e));
        }
        Ok(())
    }

    /// Read controller status (idempotent; retried within the policy).
    pub fn query_status(&mut self) -> Result<(), ActorError> {
        self.ready_for_request(|_| true)?;
        let _ = self.send(Request::QueryStatus, 1);
        Ok(())
    }

    /// Cancel the execution. Before a start this is purely local: nothing is sent and no
    /// controller file is deleted (deleting is a separate, explicit user action). After a start
    /// a single stop is sent and the outcome stays `Unknown` until confirmed.
    pub fn cancel(&mut self) -> Result<(), ActorError> {
        let exec = self.execution.as_ref().ok_or(ActorError::NoExecution)?;
        if exec.state.is_terminal() {
            return Err(ActorError::InvalidState(exec.state.clone()));
        }
        let started = exec.start_sent;
        let chunks_sent =
            exec.evidence.iter().any(|e| matches!(e.evidence, Evidence::Sent { request: Request::UploadChunk { .. } }));
        self.in_flight = None;
        self.record(Evidence::Cancelled);
        if !started {
            if chunks_sent {
                self.record(Evidence::ControllerFileMayRemain);
            }
            self.set_state(ExecutionState::Cancelled);
            return Ok(());
        }
        self.set_state(ExecutionState::Unknown(UnknownReason::StopRequested));
        self.send(Request::Stop, 1).map_err(ActorError::Port)
    }

    /// Re-establish the transport. Starts a new epoch so late replies never match, and turns
    /// an interrupted non-idempotent request into `Unknown`. Never resends anything.
    pub fn reconnect(&mut self) -> Result<(), ActorError> {
        self.port.reconnect().map_err(ActorError::Port)?;
        self.epoch = self.epoch.wrapping_add(1);
        if self.execution.is_some() {
            self.record(Evidence::Reconnected);
        }
        if let Some(f) = self.in_flight.take() {
            if let Request::Live(live) = &f.request {
                self.live_reply_lost(live, crate::live::LiveUnknown::Reconnected);
            } else {
                self.uncertain_after_lost_reply(&f.request);
            }
        }
        Ok(())
    }

    /// Process received replies, then timeouts.
    pub fn poll(&mut self) -> Result<(), ActorError> {
        for _ in 0..MAX_REPLIES_PER_POLL {
            match self.port.try_recv() {
                Ok(Some(incoming)) => self.handle(incoming),
                // A transport error is not a reply; the timeout path decides what it means.
                Ok(None) | Err(_) => break,
            }
        }
        self.check_timeout();
        Ok(())
    }

    fn ready_for_request(&self, allowed: impl Fn(&ExecutionState) -> bool) -> Result<(), ActorError> {
        let exec = self.execution.as_ref().ok_or(ActorError::NoExecution)?;
        if !allowed(&exec.state) {
            return Err(ActorError::InvalidState(exec.state.clone()));
        }
        if self.in_flight.is_some() {
            return Err(ActorError::RequestInFlight);
        }
        Ok(())
    }

    fn record(&mut self, evidence: Evidence) {
        let at = self.clock.now();
        if let Some(exec) = self.execution.as_mut() {
            exec.evidence.push(EvidenceEntry { at, evidence });
        }
    }

    fn state(&self) -> Option<&ExecutionState> {
        self.execution.as_ref().map(|e| &e.state)
    }

    fn set_state(&mut self, state: ExecutionState) {
        if let Some(exec) = self.execution.as_mut() {
            exec.state = state;
        }
    }

    fn fail_in_flight_send(&mut self, state: ExecutionState) {
        self.in_flight = None;
        self.set_state(state);
    }

    /// Send one request as the single in-flight request. Replies that are already queued
    /// belong to earlier requests (e.g. a duplicated ACK) and are discarded first.
    fn send(&mut self, request: Request, attempt: u32) -> Result<(), PortError> {
        for _ in 0..MAX_REPLIES_PER_POLL {
            match self.port.try_recv() {
                Ok(Some(stale)) => self.ignored.push(stale),
                Ok(None) | Err(_) => break,
            }
        }
        let Some(execution) = self.tag_owner(&request) else {
            return Err(PortError::Other("no execution".into()));
        };
        let tag = RequestTag { execution, epoch: self.epoch, seq: self.next_seq };
        self.next_seq = self.next_seq.wrapping_add(1);
        if !matches!(request, Request::Live(_)) {
            self.record(Evidence::Sent { request: request.clone() });
        }
        self.in_flight = Some(InFlight { tag, request: request.clone(), sent_at: self.clock.now(), attempt });
        self.port.send(Envelope { to: self.machine.id.clone(), tag, request })
    }

    fn handle(&mut self, incoming: Incoming) {
        let attributed =
            incoming.from == self.machine.id && self.in_flight.as_ref().is_some_and(|f| f.tag == incoming.tag);
        let Some(flight) = self.in_flight.clone().filter(|_| attributed) else {
            self.ignored.push(incoming);
            return;
        };
        if let Request::Live(request) = &flight.request {
            self.handle_live(request, incoming);
            return;
        }
        match (&flight.request, &incoming.reply) {
            (Request::UploadChunk { index, .. }, Reply::Ack) => self.chunk_acked(*index),
            (Request::UploadChunk { index, .. }, Reply::Nak | Reply::Err) => {
                self.finish_flight(Evidence::Negative { request: flight.request.clone() });
                self.set_state(ExecutionState::Failed(FailReason::UploadRejected { index: *index }));
            }
            // An ACK to a query only means "received"; the answer is still pending.
            (Request::QueryIdentity | Request::QueryStored | Request::QueryStatus, Reply::Ack) => {}
            (Request::QueryIdentity, Reply::Identity(reported)) => {
                if self.state() != Some(&ExecutionState::VerifyingIdentity) {
                    self.finish_flight(Evidence::IdentityConfirmed(reported.clone()));
                    return;
                }
                if self.machine.matches(reported) {
                    self.finish_flight(Evidence::IdentityConfirmed(reported.clone()));
                    if self.reverify {
                        self.set_state(ExecutionState::Reverifying);
                        // A failed send of a query is handled like a lost reply.
                        let _ = self.send(Request::QueryStored, 1);
                    } else {
                        self.send_chunk(0);
                    }
                } else {
                    self.finish_flight(Evidence::IdentityMismatch { reported: reported.clone() });
                    self.set_state(ExecutionState::Failed(FailReason::IdentityMismatch));
                }
            }
            (Request::QueryStored, Reply::Stored(stored)) => {
                let stored = *stored;
                let Some(rd_hash) = self.execution.as_ref().map(|e| e.rd_hash) else { return };
                self.finish_flight(if stored {
                    Evidence::StorageConfirmed { rd_hash }
                } else {
                    Evidence::StorageDenied
                });
                if matches!(self.state(), Some(ExecutionState::UploadAcked | ExecutionState::Reverifying)) {
                    self.set_state(if stored {
                        ExecutionState::Stored
                    } else {
                        ExecutionState::Failed(FailReason::NotStored)
                    });
                }
            }
            (Request::QueryStatus, Reply::Status(status)) => {
                let status = *status;
                self.finish_flight(Evidence::Status(status));
                let next = match (self.state(), status) {
                    (Some(ExecutionState::StartRequested), ControllerStatus::Running) => Some(ExecutionState::Running),
                    (Some(ExecutionState::Running), ControllerStatus::Finished) => Some(ExecutionState::Completed),
                    (Some(ExecutionState::StartRequested | ExecutionState::Running), ControllerStatus::Fault) => {
                        Some(ExecutionState::Failed(FailReason::ControllerFault))
                    }
                    _ => None,
                };
                if let Some(next) = next {
                    self.set_state(next);
                }
            }
            (Request::Start, Reply::Ack) => self.finish_flight(Evidence::StartAcked),
            (Request::Start, Reply::Nak | Reply::Err) => {
                self.finish_flight(Evidence::Negative { request: Request::Start });
                self.set_state(ExecutionState::Unknown(UnknownReason::StartRefusedUnconfirmed));
            }
            (Request::Stop, Reply::Ack) => self.finish_flight(Evidence::StopAcked),
            (
                Request::QueryIdentity | Request::QueryStored | Request::QueryStatus | Request::Stop,
                Reply::Nak | Reply::Err,
            ) => {
                self.finish_flight(Evidence::Negative { request: flight.request.clone() });
            }
            // A reply of the wrong kind for the request in flight is not evidence.
            _ => self.ignored.push(incoming),
        }
    }

    fn finish_flight(&mut self, evidence: Evidence) {
        self.in_flight = None;
        self.record(evidence);
    }

    fn chunk_acked(&mut self, index: usize) {
        self.finish_flight(Evidence::ChunkAcked { index });
        let next = index + 1;
        if next >= self.chunks.len() {
            self.set_state(ExecutionState::UploadAcked);
            return;
        }
        self.send_chunk(next);
    }

    fn send_chunk(&mut self, index: usize) {
        let Some(bytes) = self.chunks.get(index).cloned() else { return };
        self.set_state(ExecutionState::Uploading { acked: index, total: self.chunks.len() });
        if self.send(Request::UploadChunk { index, bytes }, 1).is_err() {
            self.fail_in_flight_send(ExecutionState::Unknown(UnknownReason::UploadChunkUnconfirmed { index }));
        }
    }

    fn check_timeout(&mut self) {
        let Some(flight) = self.in_flight.clone() else { return };
        if self.clock.now().saturating_since(flight.sent_at) < self.policy.reply_timeout {
            return;
        }
        self.in_flight = None;
        if let Request::Live(live) = &flight.request {
            if live.is_idempotent() && flight.attempt < self.policy.max_query_attempts {
                let _ = self.send(flight.request.clone(), flight.attempt + 1);
            } else {
                self.live_reply_lost(live, crate::live::LiveUnknown::ReplyLost);
            }
            return;
        }
        self.record(Evidence::Timeout { request: flight.request.clone(), attempt: flight.attempt });
        if flight.request.is_idempotent() {
            if flight.attempt < self.policy.max_query_attempts {
                let _ = self.send(flight.request, flight.attempt + 1);
            } else {
                let identity = flight.request == Request::QueryIdentity;
                self.record(Evidence::QueryUnanswered { request: flight.request });
                if identity && self.state() == Some(&ExecutionState::VerifyingIdentity) {
                    self.set_state(ExecutionState::Failed(FailReason::IdentityUnconfirmed));
                }
            }
            return;
        }
        self.uncertain_after_lost_reply(&flight.request);
    }

    /// A non-idempotent request lost its reply: its effect is unknown and it is not resent.
    fn uncertain_after_lost_reply(&mut self, request: &Request) {
        if self.state().is_some_and(ExecutionState::is_terminal) {
            return;
        }
        match request {
            Request::UploadChunk { index, .. } => {
                self.set_state(ExecutionState::Unknown(UnknownReason::UploadChunkUnconfirmed { index: *index }));
            }
            Request::Start => self.set_state(ExecutionState::Unknown(UnknownReason::StartReplyLost)),
            // Nothing of the job was sent: interrupted identity verification simply fails.
            Request::QueryIdentity if self.state() == Some(&ExecutionState::VerifyingIdentity) => {
                self.set_state(ExecutionState::Failed(FailReason::IdentityUnconfirmed));
            }
            Request::Stop | Request::QueryIdentity | Request::QueryStored | Request::QueryStatus | Request::Live(_) => {
            }
        }
    }
}
