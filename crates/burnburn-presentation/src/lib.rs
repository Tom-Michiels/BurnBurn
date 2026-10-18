//! Shared presentation layer: one [`AppController`] with a document session per open document,
//! semantic [`UiIntent`]s, plain-data [`AppViewModel`]s and revision-checked background tasks.
//!
//! Native hosts (AppKit, Windows), the CLI and the [`HeadlessEditorHarness`] all drive the same
//! controller. No AppKit, Windows or GPU type appears here.

mod camera;
mod commit;
mod controller;
mod field;
mod harness;
mod intent;
mod production;
mod task;
mod view_model;

/// Named by [`MessageKey::MissingCapability`]; hosts translate it.
pub use burnburn_job::Feature;
pub use camera::{
    BedCameraIntent, CameraAuthorization, CameraDevice, CameraDeviceId, CameraError, CameraFrame, CameraFrameInfo,
    CameraMode, CameraPixelFormat, CameraPort, CameraViewModel,
};
pub use controller::{AppController, AppPorts, AppUpdate, HostEffect};
pub use field::{FieldError, format_mm, parse_length, parse_position};
pub use harness::{HeadlessEditorHarness, QueuedTaskPort};
pub use intent::{
    CameraIntent, CloseChoice, CommandKey, CutSettingsInput, DialogKind, FieldKey, ImportUnit, OperationField,
    OperationKind, SelectMode, SketchDimensionKind, UiIntent, ViewPointerEvent,
};
pub use production::{
    ActionId, ActionSubject, Dispatched, ExecutionRecord, HandledAction, HistoryStore, HistoryStoreError, IntentRecord,
    JobInputs, MachineAction, MachineError, MachineIntent, MachinePort, MachineService, MachineStatus, OrderDraft,
    OrderId, Outcome, OutcomeRecord, PreparedJob, Recorded, RecoveredExecution, RecoveredMachine, Role, StoredOrder,
    StoredOrders, UploadRecord,
};
pub use task::{
    DocumentMeasure, ParsedImport, TaskError, TaskInput, TaskKind, TaskOutcome, TaskOutput, TaskPort, TaskPortError,
    TaskRequest, run_task,
};
pub use view_model::{
    AppViewModel, BooleanPreview, CommandState, ConstraintHealth, ConstraintRow, CutSettingsViewModel,
    DocumentViewModel, ExportViewModel, FieldViewModel, ImportViewModel, InspectorViewModel, JobCardViewModel,
    LAYER_SLOTS, MachineButton, MachineStatusKey, MachineViewModel, MeasureViewModel, MessageKey, ModeChoice,
    ObjectRow, OperationRow, OperatorJobRow, OperatorViewModel, OrderViewModel, PlanViewModel, ProblemViewModel,
    SelectionCycle, SketchConstraintKind, SketchModeViewModel, SketchStatus, SketchViewModel, StatusMessage, TaskBadge,
    TraceSummary, TracerViewModel,
};
