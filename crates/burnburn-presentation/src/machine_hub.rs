//! The machine hub (G5-machine-rest, M01/M02/M04/M09/W01-W03/W09): one read model for the window
//! that shows which machine is active, what it supports, how an upload stands, what was uploaded,
//! started and completed, the history with a repeat route, and the calibration measurements.
//!
//! Everything here is derived from the same evidence the machine panel uses; nothing is decided
//! by the host. Hosts send [`crate::UiIntent`]s (`ChooseMachineProfile`, `RepeatExecution`,
//! `ConfirmRepeat`, `CancelRepeat`, `RecordCalibration`, `RemoveCalibration`).

use burnburn_job::JobPlanId;
use burnburn_machine::ExecutionId;
use burnburn_types::RevisionId;

use crate::cam_dialogs::CapabilityRow;
use crate::production::OrderId;
use crate::view_model::MachineButton;

/// One known machine profile.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceRow {
    pub id: String,
    pub revision: u32,
    pub width_mm: f64,
    pub height_mm: f64,
    pub active: bool,
    /// What the user declared about the controller (M01); `None` is not declared. Never filled
    /// from what a controller replied.
    pub model: Option<String>,
    pub firmware: Option<String>,
}

/// Why an upload failed, as the evidence says. The controller reports no reason beyond a refused
/// chunk, so a full storage or a name conflict is never claimed (no evidence for either).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadFailure {
    IdentityMismatch,
    IdentityUnconfirmed,
    /// The controller refused this chunk (NAK or error).
    Rejected {
        chunk: usize,
    },
    NotStored,
    ControllerFault,
}

/// The three steps of an upload, kept apart: progress, transport ACK, confirmed storage (M04).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadStage {
    VerifyingIdentity,
    /// Chunks with a transport ACK, of total. An ACK proves receipt, not storage.
    Sending {
        acked: usize,
        total: usize,
    },
    /// Every chunk is ACKed; storage is not confirmed yet.
    AckedNotConfirmed {
        chunks: usize,
    },
    /// Read-back confirmed the stored job.
    StoredConfirmed,
    Failed(UploadFailure),
    /// The operator stopped the upload.
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadStatusView {
    pub execution: ExecutionId,
    pub job_name: String,
    pub stage: UploadStage,
    /// "Stop upload" while the upload is under way (the same cancel the machine panel offers).
    pub stop: Option<MachineButton>,
}

/// Where an execution stands. Only `Completed` counts as a produced piece.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueStage {
    Uploading,
    /// Stored on the controller, not started.
    Stored,
    StartRequested,
    Running,
    Completed,
    Failed,
    Cancelled,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueRow {
    pub execution: ExecutionId,
    pub job_name: String,
    pub plan: JobPlanId,
    pub stage: QueueStage,
    pub order: Option<(OrderId, String)>,
}

/// Executions per stage. An upload or a stored job is never a finished piece.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QueueCounts {
    pub uploading: usize,
    pub stored: usize,
    pub started: usize,
    pub completed: usize,
    pub unknown: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QueueView {
    pub rows: Vec<QueueRow>,
    pub counts: QueueCounts,
}

/// One execution in the history, newest first. Estimate and measured duration are kept apart.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryEntry {
    pub execution: ExecutionId,
    pub job_name: String,
    pub plan: JobPlanId,
    pub plan_revision: RevisionId,
    pub profile: String,
    pub profile_revision: u32,
    pub rd_bytes: usize,
    pub stage: QueueStage,
    /// Constant-velocity estimate from the frozen plan; `None` when the plan is not at hand.
    pub estimated_s: Option<f64>,
    /// Between the start acknowledgement and the controller reporting the end; `None` unless both
    /// were observed.
    pub measured_s: Option<f64>,
    pub order: Option<(OrderId, String)>,
}

/// What differs between the recipe a job was frozen with and the document's recipe now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecipeField {
    Name,
    Mode,
    Speed,
    PowerMin,
    PowerMax,
    Passes,
    AirAssist,
    Output,
    /// The operation does not exist in the document any more.
    Removed,
    /// The document has an operation the job does not have.
    Added,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipeChange {
    pub operation: String,
    pub field: RecipeField,
    pub frozen: String,
    pub current: String,
}

/// Why a repeat cannot be set up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatRefusal {
    /// The machine profile is not the one the job was frozen for.
    ProfileChanged,
    NoWindow,
}

/// A repeat in preparation: the frozen job is reused as it is; the differences with the document's
/// current recipes are listed first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepeatView {
    pub execution: ExecutionId,
    pub job_name: String,
    pub plan: JobPlanId,
    pub changes: Vec<RecipeChange>,
    pub refusal: Option<RepeatRefusal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProofKind {
    /// Kerf per material (mm).
    Kerf,
    /// Scan offset per speed (mm).
    ScanOffset,
    /// Red-dot pointer position relative to the beam (x, y mm).
    PointerOffset,
}

/// A calibration proof as the host sends it: raw text, parsed here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProofInput {
    pub kind: ProofKind,
    /// Kerf: the material; others: optional.
    pub material: String,
    /// Scan offset: the speed in mm/s.
    pub speed: String,
    /// The measured value in mm (pointer offset: x).
    pub value: String,
    /// Pointer offset: y in mm.
    pub value_y: String,
    /// How it was measured (caliper, ruler, microscope ...).
    pub method: String,
    /// The day of the measurement, as the host says it (ISO 8601).
    pub date: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProofField {
    Material,
    Speed,
    Value,
    ValueY,
    Method,
    Date,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProofProblem {
    /// No machine profile is active; a proof names its machine.
    NoMachine,
    Invalid(ProofField),
    /// A scan offset for this speed exists already (remove it first).
    DuplicateSpeed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProofRow {
    pub id: u64,
    pub kind: ProofKind,
    pub machine: String,
    pub material: String,
    pub speed_mm_s: Option<f64>,
    pub value_mm: f64,
    pub value_y_mm: Option<f64>,
    pub method: String,
    pub date: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProofView {
    pub rows: Vec<ProofRow>,
    pub problem: Option<ProofProblem>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MachineHubViewModel {
    pub devices: Vec<DeviceRow>,
    /// The matrix the compiler reads, for the active profile.
    pub support: Vec<CapabilityRow>,
    pub upload: Option<UploadStatusView>,
    pub queue: QueueView,
    pub history: Vec<HistoryEntry>,
    pub repeat: Option<RepeatView>,
    pub calibration: ProofView,
}
