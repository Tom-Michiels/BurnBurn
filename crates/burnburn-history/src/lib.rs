//! Append-only production history (IM17, ADR 0008).
//!
//! - A [`CompiledJob`] freezes one compiled revision: the plan (blob, re-verified against its
//!   id), the exact RD bytes (blob + RD hash), the project snapshot and the machine setup
//!   revision. Editing or undoing the project never touches it; re-running a job reads the
//!   blobs, it never recompiles.
//! - Executions are separate records with their evidence. Intents are durable before a driver
//!   receives them; after a restart every non-terminal execution is `Unknown` until a status
//!   query or an operator statement settles it. Nothing is ever marked completed automatically.
//! - The journal is segments of checksummed records written only through the injected
//!   [`JournalFs`]; a torn tail is reported and left in place, mid-segment damage opens the
//!   history read-only. Nothing is rewritten or truncated.

mod hex;
mod history;
mod ids;
mod journal;
mod plan_blob;
mod record;
mod settings;
mod storage;
mod time;

pub use history::{
    ActionOutcome, ActionWhat, CompileRecord, Corruption, ExecutionView, History, HistoryError, IntentReceipt,
    JournalLimits, MAX_BLOB_BYTES, QueueCounts, QueueItem, RecordedIntent, RecoveryReport, TornTail, compiled_job_id,
    segment_path,
};
pub use ids::{ActionId, BlobKey, CompiledJobId, QueueItemId, RecordSeq};
pub use journal::{FORMAT_VERSION, RECORD_MAGIC, SEGMENT_MAGIC};
pub use plan_blob::{decode_plan, encode_plan};
pub use record::{
    CompiledBlobs, CompiledJob, EvidenceDto, ExecutionStateDto, FailReasonDto, HistoryRecord, IdentityDto, IntentKind,
    OrderRecord, PreflightSummary, RECORD_SCHEMA, ReconcileBasis, ReconcileStatement, RequestDto, Role,
    SettingsBackupRecord, StatusDto, StoredRecord, UnknownReasonDto,
};
pub use settings::{BackupValue, SETTINGS_BACKUP_SCHEMA, SettingsBackup};
pub use storage::JournalFs;
pub use time::WallTime;
