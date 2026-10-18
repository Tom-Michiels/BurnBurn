//! Laser-window requests on the [`ExecutionActor`]: jog, home, move, pause/resume, position and
//! DSP-file requests. They share the actor's single in-flight request and clock, but not its
//! execution: no live request creates an execution, changes an execution state or starts a job.
//!
//! Rules, each covered by `tests/live.rs`:
//! - every request is gated by its own proven capability (default: unverified = refused);
//! - requests that move the head or touch the file store are refused while a job may be active;
//! - only reads are retried; a lost reply to anything else is `Unknown` and never resent;
//! - a jog release is never blocked, so a lost UI focus can always stop the head;
//! - a known position is dropped the moment a motion command is sent (unknown is not zero).

use super::*;
use crate::controller_files::{ControllerFileName, FileEntry};
use crate::live::{
    Capabilities, JogKey, LiveAttempt, LiveError, LiveFeature, LiveOutcome, LiveRequest, LiveUnknown,
};
use crate::position::{AxisPosition, PositionReading};

/// Tag owner of live requests: they belong to no execution.
const LIVE_CHANNEL: ExecutionId = ExecutionId::from_raw_const(u64::MAX);

impl<P: ControllerPort> ExecutionActor<P> {
    pub fn set_capabilities(&mut self, capabilities: Capabilities) {
        self.live.capabilities = capabilities;
    }

    /// The last position the controller reported; unknown axes are `Unknown`, never 0.
    pub fn position(&self) -> &PositionReading {
        &self.live.position
    }

    /// Every live request that reached the transport (or tried to), oldest first.
    pub fn live_attempts(&self) -> &[LiveAttempt] {
        &self.live.attempts
    }

    /// The jog key that is held, or whose release is unconfirmed.
    pub fn jog_held(&self) -> Option<JogKey> {
        self.live.jog_held
    }

    /// The last answered file list.
    pub fn controller_files(&self) -> Option<&[FileEntry]> {
        self.live.files.as_deref()
    }

    pub fn file_metadata(&self, name: &ControllerFileName) -> Option<&FileEntry> {
        self.live.metadata.iter().rev().find(|e| &e.name == name)
    }

    fn gate(&self, feature: LiveFeature) -> Result<(), LiveError> {
        match self.live.capabilities.get(feature) {
            crate::controller_files::Capability::Supported => Ok(()),
            crate::controller_files::Capability::Unsupported => Err(LiveError::Unsupported { feature }),
            crate::controller_files::Capability::Unverified { reason } => {
                Err(LiveError::NotVerified { feature, reason: reason.clone(), needs: feature.required_evidence() })
            }
        }
    }

    /// Sends one live request. The outcome arrives through [`ExecutionActor::poll`] and is
    /// readable in [`ExecutionActor::live_attempts`].
    pub fn live(&mut self, request: LiveRequest) -> Result<(), LiveError> {
        let release = matches!(request, LiveRequest::JogStop(_));
        if !release {
            self.gate(request.feature())?;
        }
        if request.needs_idle_machine() {
            if let Some(active) = self.execution.as_ref().filter(|e| !e.state.is_terminal()) {
                return Err(LiveError::JobMayBeActive(active.state.clone()));
            }
        }
        if self.in_flight.is_some() {
            return Err(LiveError::RequestInFlight);
        }
        if let (LiveRequest::JogStart(_), Some(held)) = (&request, self.live.jog_held) {
            return Err(LiveError::JogKeyHeld(held));
        }
        let seq = u32::try_from(self.live.attempts.len()).unwrap_or(u32::MAX);
        self.live.attempts.push(LiveAttempt { seq, request: request.clone(), outcome: LiveOutcome::Pending });
        if request.moves_head() {
            self.live.position = PositionReading::UNKNOWN;
        }
        if let LiveRequest::JogStart(key) = &request {
            self.live.jog_held = Some(*key);
        }
        match self.send(Request::Live(request.clone()), 1) {
            Ok(()) => Ok(()),
            Err(e) => {
                self.in_flight = None;
                // These could not have left the machine; anything else may have.
                let left = !matches!(e, PortError::NotSupported(_) | PortError::QueueFull | PortError::Disconnected);
                if left {
                    self.set_live_outcome(LiveOutcome::Unknown(LiveUnknown::SendFailed));
                } else {
                    self.set_live_outcome(LiveOutcome::NotSent);
                    if matches!(request, LiveRequest::JogStart(_)) {
                        self.live.jog_held = None;
                    }
                }
                Err(LiveError::Port(e))
            }
        }
    }

    /// Releases the held jog key (focus loss, window close). `Ok(false)`: nothing was held.
    pub fn release_jog(&mut self) -> Result<bool, LiveError> {
        match self.live.jog_held {
            Some(key) => self.live(LiveRequest::JogStop(key)).map(|()| true),
            None => Ok(false),
        }
    }

    /// Save a compiled RD file under a name on the controller. Gated like the other file
    /// operations; even when the capability is claimed this build has no named-upload wire form,
    /// so nothing is ever sent.
    pub fn save_rd_to_controller(
        &mut self,
        _name: &ControllerFileName,
        _artifact: &crate::artifact::RdArtifact,
    ) -> Result<(), LiveError> {
        self.gate(LiveFeature::FileUpload)?;
        Err(LiveError::NoWireForm { feature: LiveFeature::FileUpload })
    }

    fn set_live_outcome(&mut self, outcome: LiveOutcome) {
        if let Some(a) = self.live.attempts.last_mut() {
            a.outcome = outcome;
        }
    }

    /// The tag owner for a request: live requests never use an execution id.
    pub(super) fn tag_owner(&self, request: &Request) -> Option<ExecutionId> {
        match request {
            Request::Live(_) => Some(LIVE_CHANNEL),
            _ => self.execution.as_ref().map(|e| e.id),
        }
    }

    /// A reply attributed to the in-flight live request.
    pub(super) fn handle_live(&mut self, request: &LiveRequest, incoming: Incoming) {
        let now = self.clock.now();
        let held = self.live.jog_held;
        match (request, &incoming.reply) {
            // Received; the data answer of a read is still pending.
            (r, Reply::Ack) if r.is_idempotent() => {}
            (LiveRequest::JogStop(_), Reply::Ack) => {
                self.in_flight = None;
                self.live.jog_held = None;
                self.set_live_outcome(LiveOutcome::Acked);
            }
            (_, Reply::Ack) => {
                self.in_flight = None;
                self.set_live_outcome(LiveOutcome::Acked);
            }
            (r, Reply::Nak | Reply::Err) => {
                self.in_flight = None;
                // A refused press never held the key; a refused release leaves it held.
                if matches!(r, LiveRequest::JogStart(_)) {
                    self.live.jog_held = None;
                } else {
                    self.live.jog_held = held;
                }
                self.set_live_outcome(LiveOutcome::Refused);
            }
            (LiveRequest::ReadPosition, Reply::Position { x_um, y_um }) => {
                self.in_flight = None;
                self.live.position = PositionReading {
                    x: AxisPosition::from_reading(*x_um),
                    y: AxisPosition::from_reading(*y_um),
                    read_at: (x_um.is_some() || y_um.is_some()).then_some(now),
                };
                self.set_live_outcome(LiveOutcome::Answered);
            }
            (LiveRequest::ListFiles, Reply::Files(files)) => {
                self.in_flight = None;
                self.live.files = Some(files.clone());
                self.set_live_outcome(LiveOutcome::Answered);
            }
            (LiveRequest::FileMetadata(name), Reply::FileMetadata(entry)) if &entry.name == name => {
                self.in_flight = None;
                self.live.metadata.push(entry.clone());
                self.set_live_outcome(LiveOutcome::Answered);
            }
            // A reply of the wrong kind for the request in flight is not evidence.
            _ => self.ignored.push(incoming),
        }
    }

    /// The reply to a live request did not arrive (timeout or reconnect).
    pub(super) fn live_reply_lost(&mut self, request: &LiveRequest, why: LiveUnknown) {
        let _ = request;
        self.set_live_outcome(LiveOutcome::Unknown(why));
    }
}
