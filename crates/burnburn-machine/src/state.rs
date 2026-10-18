use burnburn_job::JobPlanId;
use burnburn_types::{ContentHash, MonotonicTime};

use crate::ids::{ControllerIdentity, ExecutionId, MachineId};
use crate::port::{ControllerStatus, Request};

/// Proven state of one execution. Each level needs its own evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionState {
    /// The identity query is in flight; nothing of the job has been sent.
    VerifyingIdentity,
    /// Chunks are being sent; `acked` of `total` chunks have a transport ACK.
    Uploading {
        acked: usize,
        total: usize,
    },
    /// Every chunk was ACKed. Not proof that the controller stored the job.
    UploadAcked,
    /// Re-verification of an earlier upload (new session): identity confirmed, read-back of the
    /// stored job pending. No job bytes are sent in this path.
    Reverifying,
    /// The controller confirmed the stored job (read-back).
    Stored,
    /// Start was sent (possibly ACKed). Not proof that the job runs.
    StartRequested,
    /// Status reported running after our start.
    Running,
    /// Status reported the job finished after it was observed running.
    Completed,
    /// The outcome is uncertain. Automation never sends a start in this state.
    Unknown(UnknownReason),
    Failed(FailReason),
    Cancelled,
}

impl ExecutionState {
    pub fn is_terminal(&self) -> bool {
        matches!(self, ExecutionState::Completed | ExecutionState::Failed(_) | ExecutionState::Cancelled)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnknownReason {
    /// A chunk was sent but its ACK never arrived: the controller may hold partial data.
    UploadChunkUnconfirmed { index: usize },
    /// Start was sent but no reply arrived (or the connection dropped).
    StartReplyLost,
    /// Start got a negative reply; its firmware meaning is unconfirmed.
    StartRefusedUnconfirmed,
    /// The start request could not be handed to the transport; whether it left is unknown.
    StartSendFailed,
    /// Stop was sent for a started execution; its effect is not confirmed.
    StopRequested,
    /// The application restarted while this execution was not finished (ADR 0008 §5).
    InterruptedByRestart,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailReason {
    /// The controller reported another model/firmware than the profile. Nothing was uploaded.
    IdentityMismatch,
    /// The identity query stayed unanswered. Nothing was uploaded.
    IdentityUnconfirmed,
    UploadRejected {
        index: usize,
    },
    NotStored,
    ControllerFault,
    /// The operator stated, after an unknown outcome, that the job did not run.
    OperatorDidNotRun,
    /// The operator stated, after an unknown outcome, that the job ran only partly.
    OperatorPartial,
}

/// Facts observed by the actor, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Evidence {
    Sent {
        request: Request,
    },
    ChunkAcked {
        index: usize,
    },
    Negative {
        request: Request,
    },
    IdentityConfirmed(ControllerIdentity),
    IdentityMismatch {
        reported: ControllerIdentity,
    },
    /// Read-back confirmed the stored job; names the RD bytes that were uploaded (DR-10).
    StorageConfirmed {
        rd_hash: ContentHash,
    },
    StorageDenied,
    StartAcked,
    StopAcked,
    Status(ControllerStatus),
    Timeout {
        request: Request,
        attempt: u32,
    },
    /// A query was given up after the bounded number of attempts.
    QueryUnanswered {
        request: Request,
    },
    Reconnected,
    Cancelled,
    /// Cancel happened after chunks were sent; a (partial) controller file may remain and is
    /// deliberately not deleted.
    ControllerFileMayRemain,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceEntry {
    pub at: MonotonicTime,
    pub evidence: Evidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Execution {
    pub id: ExecutionId,
    pub machine: MachineId,
    /// The plan and the exact RD bytes this execution uploads (DR-10).
    pub plan: JobPlanId,
    pub rd_hash: ContentHash,
    pub state: ExecutionState,
    pub evidence: Vec<EvidenceEntry>,
    /// True once a start request was handed to the transport. Stays true forever.
    pub start_sent: bool,
}
