//! Journal records and the serde DTOs of machine evidence (machine types stay serde-free).
//! The mappings are exhaustive `match`es: a new `Evidence`/`ExecutionState` variant breaks this
//! crate's compilation instead of silently becoming "other".

use std::path::PathBuf;

use burnburn_job::ProfileRevision;
use burnburn_machine::{
    ControllerIdentity, ControllerStatus, Evidence, ExecutionId, ExecutionState, FailReason, MachineId, Request,
    UnknownReason,
};
use burnburn_types::ContentHash;
use serde::{Deserialize, Serialize};

use crate::ids::{
    ActionId, BlobKey, CompiledJobId, QueueItemId, RecordSeq, content_hash, execution_id, machine_id, opt_content_hash,
    profile_revision,
};
use crate::time::WallTime;

/// Preflight result recorded with a job. A job never has errors (else it is not a CompiledJob).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PreflightSummary {
    pub errors: u32,
    pub warnings: u32,
    /// Stable finding codes, for display and search.
    pub codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledBlobs {
    pub plan: BlobKey,
    pub rd: BlobKey,
    pub project: BlobKey,
}

/// A frozen job revision: what preview showed, what RD export wrote and what upload sends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledJob {
    pub id: CompiledJobId,
    /// `JobPlanId` hash (the job crate offers no constructor from a hash; the plan blob proves it).
    #[serde(with = "content_hash")]
    pub plan: ContentHash,
    #[serde(with = "content_hash")]
    pub rd_hash: ContentHash,
    /// Machine setup revision the job was prepared for (profile incl. calibration, IM29).
    #[serde(with = "profile_revision")]
    pub profile: ProfileRevision,
    /// `burnburn_io::project_hash` of the compiled project.
    #[serde(with = "content_hash")]
    pub project_hash: ContentHash,
    /// Display/search only; never identity.
    pub project_path: Option<PathBuf>,
    pub preflight: PreflightSummary,
    pub blobs: CompiledBlobs,
    pub created: WallTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IntentKind {
    Upload,
    ConfirmStored,
    Start,
    Stop,
    QueryStatus,
    Frame,
    WriteSettings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Role {
    Designer,
    Operator,
}

/// What the operator says they saw. Only `RanToCompletion` counts as a produced piece.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReconcileStatement {
    RanToCompletion,
    DidNotRun,
    Partial,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "basis", rename_all = "kebab-case")]
pub enum ReconcileBasis {
    StatusQuery { evidence_seq: RecordSeq },
    OperatorConfirmed { role: Role, statement: ReconcileStatement },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "request", rename_all = "kebab-case")]
pub enum RequestDto {
    QueryIdentity,
    /// The chunk bytes are in the RD blob; only index and length are journaled.
    UploadChunk {
        index: usize,
        len: usize,
    },
    QueryStored,
    Start,
    QueryStatus,
    Stop,
}

impl From<&Request> for RequestDto {
    fn from(r: &Request) -> Self {
        match r {
            Request::QueryIdentity => RequestDto::QueryIdentity,
            Request::UploadChunk { index, bytes } => RequestDto::UploadChunk { index: *index, len: bytes.len() },
            Request::QueryStored => RequestDto::QueryStored,
            Request::Start => RequestDto::Start,
            Request::QueryStatus => RequestDto::QueryStatus,
            Request::Stop => RequestDto::Stop,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StatusDto {
    Idle,
    Running,
    Finished,
    Fault,
}

impl From<ControllerStatus> for StatusDto {
    fn from(s: ControllerStatus) -> Self {
        match s {
            ControllerStatus::Idle => StatusDto::Idle,
            ControllerStatus::Running => StatusDto::Running,
            ControllerStatus::Finished => StatusDto::Finished,
            ControllerStatus::Fault => StatusDto::Fault,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentityDto {
    pub model: Option<String>,
    pub firmware: Option<String>,
}

impl From<&ControllerIdentity> for IdentityDto {
    fn from(i: &ControllerIdentity) -> Self {
        IdentityDto { model: i.model.clone(), firmware: i.firmware.clone() }
    }
}

/// One-to-one with `burnburn_machine::Evidence` (without the monotonic timestamp).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "evidence", rename_all = "kebab-case")]
pub enum EvidenceDto {
    Sent {
        request: RequestDto,
    },
    ChunkAcked {
        index: usize,
    },
    Negative {
        request: RequestDto,
    },
    IdentityConfirmed {
        identity: IdentityDto,
    },
    IdentityMismatch {
        reported: IdentityDto,
    },
    StorageConfirmed {
        #[serde(with = "content_hash")]
        rd_hash: ContentHash,
    },
    StorageDenied,
    StartAcked,
    StopAcked,
    Status {
        status: StatusDto,
    },
    Timeout {
        request: RequestDto,
        attempt: u32,
    },
    QueryUnanswered {
        request: RequestDto,
    },
    Reconnected,
    Cancelled,
    ControllerFileMayRemain,
}

impl From<&Evidence> for EvidenceDto {
    fn from(e: &Evidence) -> Self {
        match e {
            Evidence::Sent { request } => EvidenceDto::Sent { request: request.into() },
            Evidence::ChunkAcked { index } => EvidenceDto::ChunkAcked { index: *index },
            Evidence::Negative { request } => EvidenceDto::Negative { request: request.into() },
            Evidence::IdentityConfirmed(i) => EvidenceDto::IdentityConfirmed { identity: i.into() },
            Evidence::IdentityMismatch { reported } => EvidenceDto::IdentityMismatch { reported: reported.into() },
            Evidence::StorageConfirmed { rd_hash } => EvidenceDto::StorageConfirmed { rd_hash: *rd_hash },
            Evidence::StorageDenied => EvidenceDto::StorageDenied,
            Evidence::StartAcked => EvidenceDto::StartAcked,
            Evidence::StopAcked => EvidenceDto::StopAcked,
            Evidence::Status(s) => EvidenceDto::Status { status: (*s).into() },
            Evidence::Timeout { request, attempt } => {
                EvidenceDto::Timeout { request: request.into(), attempt: *attempt }
            }
            Evidence::QueryUnanswered { request } => EvidenceDto::QueryUnanswered { request: request.into() },
            Evidence::Reconnected => EvidenceDto::Reconnected,
            Evidence::Cancelled => EvidenceDto::Cancelled,
            Evidence::ControllerFileMayRemain => EvidenceDto::ControllerFileMayRemain,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "kebab-case")]
pub enum UnknownReasonDto {
    UploadChunkUnconfirmed {
        index: usize,
    },
    StartReplyLost,
    StartRefusedUnconfirmed,
    StartSendFailed,
    StopRequested,
    /// The application stopped while the execution was not terminal (derived at open).
    InterruptedByRestart,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "kebab-case")]
pub enum FailReasonDto {
    IdentityMismatch,
    IdentityUnconfirmed,
    UploadRejected {
        index: usize,
    },
    NotStored,
    ControllerFault,
    /// Operator reconciliation: the job did not run.
    OperatorDidNotRun,
    /// Operator reconciliation: the job ran partly; not a produced piece.
    OperatorPartial,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum ExecutionStateDto {
    /// Created in the history; the actor has not reported anything yet.
    Created,
    VerifyingIdentity,
    Uploading {
        acked: usize,
        total: usize,
    },
    UploadAcked,
    /// Re-verifying an earlier upload in a new session (read-back pending).
    Reverifying,
    Stored,
    StartRequested,
    Running,
    Completed,
    Unknown {
        why: UnknownReasonDto,
    },
    Failed {
        why: FailReasonDto,
    },
    Cancelled,
}

impl ExecutionStateDto {
    pub fn is_terminal(&self) -> bool {
        matches!(self, ExecutionStateDto::Completed | ExecutionStateDto::Failed { .. } | ExecutionStateDto::Cancelled)
    }

    pub fn is_unknown(&self) -> bool {
        matches!(self, ExecutionStateDto::Unknown { .. })
    }

    /// The machine state this record describes; `None` for `Created` (nothing observed yet).
    pub fn to_state(&self) -> Option<ExecutionState> {
        Some(match self {
            ExecutionStateDto::Created => return None,
            ExecutionStateDto::VerifyingIdentity => ExecutionState::VerifyingIdentity,
            ExecutionStateDto::Uploading { acked, total } => ExecutionState::Uploading { acked: *acked, total: *total },
            ExecutionStateDto::UploadAcked => ExecutionState::UploadAcked,
            ExecutionStateDto::Reverifying => ExecutionState::Reverifying,
            ExecutionStateDto::Stored => ExecutionState::Stored,
            ExecutionStateDto::StartRequested => ExecutionState::StartRequested,
            ExecutionStateDto::Running => ExecutionState::Running,
            ExecutionStateDto::Completed => ExecutionState::Completed,
            ExecutionStateDto::Unknown { why } => ExecutionState::Unknown(match why {
                UnknownReasonDto::UploadChunkUnconfirmed { index } => {
                    UnknownReason::UploadChunkUnconfirmed { index: *index }
                }
                UnknownReasonDto::StartReplyLost => UnknownReason::StartReplyLost,
                UnknownReasonDto::StartRefusedUnconfirmed => UnknownReason::StartRefusedUnconfirmed,
                UnknownReasonDto::StartSendFailed => UnknownReason::StartSendFailed,
                UnknownReasonDto::StopRequested => UnknownReason::StopRequested,
                UnknownReasonDto::InterruptedByRestart => UnknownReason::InterruptedByRestart,
            }),
            ExecutionStateDto::Failed { why } => ExecutionState::Failed(match why {
                FailReasonDto::IdentityMismatch => FailReason::IdentityMismatch,
                FailReasonDto::IdentityUnconfirmed => FailReason::IdentityUnconfirmed,
                FailReasonDto::UploadRejected { index } => FailReason::UploadRejected { index: *index },
                FailReasonDto::NotStored => FailReason::NotStored,
                FailReasonDto::ControllerFault => FailReason::ControllerFault,
                FailReasonDto::OperatorDidNotRun => FailReason::OperatorDidNotRun,
                FailReasonDto::OperatorPartial => FailReason::OperatorPartial,
            }),
            ExecutionStateDto::Cancelled => ExecutionState::Cancelled,
        })
    }
}

impl From<&ExecutionState> for ExecutionStateDto {
    fn from(s: &ExecutionState) -> Self {
        match s {
            ExecutionState::VerifyingIdentity => ExecutionStateDto::VerifyingIdentity,
            ExecutionState::Uploading { acked, total } => ExecutionStateDto::Uploading { acked: *acked, total: *total },
            ExecutionState::UploadAcked => ExecutionStateDto::UploadAcked,
            ExecutionState::Reverifying => ExecutionStateDto::Reverifying,
            ExecutionState::Stored => ExecutionStateDto::Stored,
            ExecutionState::StartRequested => ExecutionStateDto::StartRequested,
            ExecutionState::Running => ExecutionStateDto::Running,
            ExecutionState::Completed => ExecutionStateDto::Completed,
            ExecutionState::Unknown(r) => ExecutionStateDto::Unknown {
                why: match r {
                    UnknownReason::UploadChunkUnconfirmed { index } => {
                        UnknownReasonDto::UploadChunkUnconfirmed { index: *index }
                    }
                    UnknownReason::StartReplyLost => UnknownReasonDto::StartReplyLost,
                    UnknownReason::StartRefusedUnconfirmed => UnknownReasonDto::StartRefusedUnconfirmed,
                    UnknownReason::StartSendFailed => UnknownReasonDto::StartSendFailed,
                    UnknownReason::StopRequested => UnknownReasonDto::StopRequested,
                    UnknownReason::InterruptedByRestart => UnknownReasonDto::InterruptedByRestart,
                },
            },
            ExecutionState::Failed(r) => ExecutionStateDto::Failed {
                why: match r {
                    FailReason::IdentityMismatch => FailReasonDto::IdentityMismatch,
                    FailReason::IdentityUnconfirmed => FailReasonDto::IdentityUnconfirmed,
                    FailReason::UploadRejected { index } => FailReasonDto::UploadRejected { index: *index },
                    FailReason::NotStored => FailReasonDto::NotStored,
                    FailReason::ControllerFault => FailReasonDto::ControllerFault,
                    FailReason::OperatorDidNotRun => FailReasonDto::OperatorDidNotRun,
                    FailReason::OperatorPartial => FailReasonDto::OperatorPartial,
                },
            },
            ExecutionState::Cancelled => ExecutionStateDto::Cancelled,
        }
    }
}

/// A customer order (IM31): `quantity` pieces of `part`, matched by name against prepared jobs.
/// Written again (same `order`) when it changes; the latest record wins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderRecord {
    pub order: u64,
    pub name: String,
    pub part: String,
    pub quantity: u32,
    pub remnant: Option<String>,
    /// Hash of the explicitly chosen `JobPlanId` when several jobs match (never picked automatically).
    #[serde(default, with = "opt_content_hash")]
    pub chosen_plan: Option<ContentHash>,
}

/// One controller-settings backup (IM29), written before any settings write is sent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingsBackupRecord {
    #[serde(with = "machine_id")]
    pub machine: MachineId,
    /// The backup document (`burnburn_history::settings` JSON form), kept verbatim.
    pub backup: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "body", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum HistoryRecord {
    /// Blobs are durable before this record is written.
    JobCompiled(Box<CompiledJob>),
    Queued {
        item: QueueItemId,
        job: CompiledJobId,
        quantity: u32,
    },
    QueueChanged {
        item: QueueItemId,
        quantity: u32,
    },
    ExecutionCreated {
        #[serde(with = "execution_id")]
        execution: ExecutionId,
        job: CompiledJobId,
        item: Option<QueueItemId>,
        #[serde(with = "machine_id")]
        machine: MachineId,
    },
    /// Durable before the driver receives the intent.
    IntentRecorded {
        #[serde(with = "execution_id")]
        execution: ExecutionId,
        action: ActionId,
        intent: IntentKind,
    },
    Evidence {
        #[serde(with = "execution_id")]
        execution: ExecutionId,
        evidence: EvidenceDto,
    },
    StateObserved {
        #[serde(with = "execution_id")]
        execution: ExecutionId,
        state: ExecutionStateDto,
    },
    Reconciled {
        #[serde(with = "execution_id")]
        execution: ExecutionId,
        from: ExecutionStateDto,
        to: ExecutionStateDto,
        by: ReconcileBasis,
        action: ActionId,
    },
    OperatorNote {
        #[serde(with = "execution_id")]
        execution: ExecutionId,
        text: String,
    },
    SettingsBackup(SettingsBackupRecord),
    OrderRecorded(OrderRecord),
    /// This execution was made for that order; only linked executions count for it.
    OrderLinked {
        order: u64,
        #[serde(with = "execution_id")]
        execution: ExecutionId,
    },
    /// A record of a kind this build does not know (newer writer, same schema). Kept, never
    /// counted, never written by this build.
    #[serde(skip)]
    Unrecognized {
        kind: String,
        body: serde_json::Value,
    },
}

pub(crate) const KNOWN_KINDS: &[&str] = &[
    "job-compiled",
    "queued",
    "queue-changed",
    "execution-created",
    "intent-recorded",
    "evidence",
    "state-observed",
    "reconciled",
    "operator-note",
    "settings-backup",
    "order-recorded",
    "order-linked",
];

pub const RECORD_SCHEMA: u32 = 1;

#[derive(Serialize)]
pub(crate) struct EnvelopeOut<'a> {
    pub schema: u32,
    pub seq: RecordSeq,
    pub wall_time: WallTime,
    #[serde(flatten)]
    pub record: &'a HistoryRecord,
}

#[derive(Deserialize)]
pub(crate) struct EnvelopeIn {
    pub schema: u32,
    pub seq: RecordSeq,
    pub wall_time: WallTime,
    pub kind: String,
    #[serde(default)]
    pub body: serde_json::Value,
}

/// A record as read back, with its position and display time.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredRecord {
    pub seq: RecordSeq,
    pub wall_time: WallTime,
    pub record: HistoryRecord,
}
