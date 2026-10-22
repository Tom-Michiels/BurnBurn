//! Shared presentation layer: one [`AppController`] with a document session per open document,
//! semantic [`UiIntent`]s, plain-data [`AppViewModel`]s and revision-checked background tasks.
//!
//! Native hosts (AppKit, Windows), the CLI and the [`HeadlessEditorHarness`] all drive the same
//! controller. No AppKit, Windows or GPU type appears here.

mod barcode_panel;
mod cam_dialogs;
mod camera;
mod camera_workspace;
mod catalog;
mod centerline_panel;
mod commit;
mod controller;
mod cut_options;
mod field;
mod files;
mod files_task;
mod grbl;
mod grbl_panel;
mod harness;
mod image_panel;
mod image_task;
mod intent;
mod laser;
mod machine_hub;
mod nest_task;
mod notes;
mod offset;
mod production;
pub mod profile_file;
mod schikken;
mod structure_panel;
mod task;
mod text_panel;
mod thumbnail;
mod variable_panel;
mod view_model;

pub use barcode_panel::{
    BarcodeDialogViewModel, BarcodeEdit, BarcodeField, BarcodeIssue, BarcodeProblem, BarcodeScaleWarning,
    BarcodeTypeRow, EccStepViewModel, EccViewModel, Readability, SymbolInputMode, SymbolKind, Symbology,
};
/// The job clock `attach_clock` takes (UTC seconds and a fixed zone offset).
pub use burnburn_generators::JobClock;
/// Named by [`MessageKey::MissingCapability`]; hosts translate it.
pub use burnburn_job::Feature;
/// What the preview window draws and picks (hosts need no job dependency of their own).
pub use burnburn_job::{MachinePoint, MoveKind, PlanPreview, PreviewMove, SourceRef};
/// The machine vocabulary the laser window speaks, so hosts need no machine dependency of their own.
pub use burnburn_machine::{
    ControllerFileName, FrameShape, HomeTarget, JobAnchor, JogKey, LaserAction, LiveFeature, OriginTarget, StartFrom,
};
pub use cam_dialogs::{
    CamDialog, CapabilityRow, DialogAction, DialogActionKind, DialogActionState, DialogChoice, DialogControl,
    DialogError, DialogField, DialogLabel, DialogNote, DialogRow, DialogValue, DialogViewModel, FindingKind, RdRefusal,
    SupportView, TestResultView,
};
pub use camera::{
    BedCameraIntent, CameraAuthorization, CameraDevice, CameraDeviceId, CameraError, CameraFrame, CameraFrameInfo,
    CameraMode, CameraPixelFormat, CameraPort, CameraViewModel,
};
pub use camera_workspace::{
    AlignmentResultView, AlignmentView, CalibrationKind, CalibrationRef, CalibrationRow, CalibrationSummary,
    CalibrationValidity, CameraInUse, CameraWhy, CameraWorkspaceIntent, CameraWorkspaceView, LensResultView,
    LensWizardView, MachineWhy, MarkView, MarksConfirm, MarksProposalView, OverlayLayerView, OverlayView, config_key,
};
pub use catalog::{
    Action, ActiveTool, DisabledReason, Enablement, GroupViewModel, PanelId, PanelViewModel, ToolButton, ToolCatalog,
    ToolContext, ToolSpec, ToolState, ToolbarViewModel, VariantMemory, enablement,
};
pub use controller::{AppController, AppPorts, AppUpdate, CameraSpawner, HostEffect, MachineActivity};
// App panels (W3-UI-app): preferences, shortcuts, undo history, preview, support.
pub use burnburn_gcode::SerialError as GrblSerialError;
pub use burnburn_gcode::SerialPort as GrblSerialPort;
/// The GRBL backend's vocabulary, so hosts need no gcode dependency of their own.
pub use burnburn_gcode::console::{ConsoleError, MacroError};
pub use burnburn_gcode::grbl::CommandRefusal as GrblCommandRefusal;
pub use burnburn_gcode::{
    GrblProfile, PowerMode as GrblPowerMode, WorkCoordinate as GrblWorkCoordinate, grbl_capabilities,
};
pub use centerline_panel::{
    CenterlineEdit, CenterlineEditViewModel, ChainRow, JunctionRow, JunctionState, PrimitiveKind, PrimitiveRow,
};
pub use controller::{
    AppIntent, DEBUG_LOG_LINES, DebugLogViewModel, DisplayQuality, HelpInfo, HistoryRow, HistoryViewModel, KeyBinding,
    LanguageChoice, LengthUnit, NudgeDirection, NudgeStep, PartialMove, PlaybackSpeed, PrefError, PrefKey, PrefProblem,
    PrefRow, PrefUnit, PrefValue, Preferences, PrefsViewModel, PreviewIntent, PreviewSelection, PreviewViewModel,
    RdInspectViewModel, RdRecordRow, ShortcutError, ShortcutProblem, ShortcutRow, ShortcutTarget, ShortcutsViewModel,
};
pub use cut_options::{
    CutChoice, CutControl, CutExtras, CutField, CutGates, CutSection, CutValue, FieldState, IMAGE_MODES,
};
pub use field::{FieldError, format_mm, parse_length, parse_position};
pub use files::{
    ArtGroupRow, ArtItemRow, ArtPanelViewModel, ArtThumbnail, BundleChange, BundleChoice, BundleConflict,
    BundleDialogViewModel, BundleExportView, BundleImportView, BundleItem, BundleKind, BundleMode, BundleProfileChange,
    BundleProfileRow, BundleReport, BundleRow, ExportCurves, ExportDialogViewModel, ExportField, ExportFormat,
    ExportLayers, ExportLossRow, ExportOptionsInput, ExportUnit, FilesIntent, FilesMessage, FilesViewModel, ImportKind,
    ImportLayerRow, ImportPage, ImportPages, Lbrn2ImageRow, Lbrn2ImageState, Lbrn2LayerRow, Lbrn2ReportViewModel,
    Lbrn2Warning, MaterialFilter, MaterialLinkStatus, MaterialOperationRow, MaterialOrigin, MaterialPanelViewModel,
    MaterialRow, TemplateScope,
};
pub use grbl::{GcodeExportViewModel, GrblFinding};
pub use grbl_panel::{
    GRBL_START_NEEDS, GrblConsoleView, GrblControl, GrblControlKind, GrblEntryKind, GrblIntent, GrblJobFailure,
    GrblJobState, GrblJobView, GrblMachineState, GrblMacroRow, GrblPort, GrblRefusalKind, GrblTranscriptRow,
    GrblUnknownReason, GrblViewModel, GrblWhy,
};
pub use harness::{HeadlessEditorHarness, QueuedTaskPort};
pub use image_panel::{
    ImageAdjustInput, ImageAdjustViewModel, ImageField, ImageImportViewModel, ImageIssue, ImagePreview, ImageViewModel,
    ThresholdChoice, TraceFormInput, TraceFormViewModel, TraceMode, TracePreset,
};
pub use image_task::{BitmapInput, BitmapJob, BitmapSource};
pub use intent::{
    CameraIntent, CloseChoice, CommandKey, CutSettingsInput, DialogKind, FieldKey, ImportUnit, OperationField,
    OperationKind, ParamKey, RefPoint, SelectMode, SketchDimensionKind, UiIntent, ViewPointerEvent,
};
pub use laser::{
    AppliedPlacement, ControllerFileRow, LaserCommand, LaserCompile, LaserControl, LaserControlKind, LaserPosition,
    LaserSetup, LaserSetupView, LaserViewModel, LaserWhy, PlacementView, StepDirection,
};
pub use machine_hub::{
    DeviceRow, HistoryEntry, MachineHubViewModel, ProofField, ProofInput, ProofKind, ProofProblem, ProofRow, ProofView,
    QueueCounts, QueueRow, QueueStage, QueueView, RecipeChange, RecipeField, RepeatRefusal, RepeatView, UploadFailure,
    UploadStage, UploadStatusView,
};
pub use notes::NotesViewModel;
pub use offset::{OffsetJoin, OffsetOptions, OffsetPreviewViewModel, OffsetViewModel, RepairPreviewViewModel};
pub use production::{
    ActionId, ActionSubject, DiagnosticOptions, Dispatched, ExecutionRecord, HandledAction, HistoryStore,
    HistoryStoreError, IntentRecord, JobInputs, LaserCall, LaserIntent, LiveIntentRecord, LiveKind, LiveOutcomeRecord,
    LiveStatus, MachineAction, MachineError, MachineIntent, MachinePort, MachineService, MachineStatus, OrderDraft,
    OrderId, Outcome, OutcomeRecord, PreparedJob, Recorded, RecoveredExecution, RecoveredMachine, Role, StoredOrder,
    StoredOrders, UploadRecord,
};
pub use schikken::*;
pub use structure_panel::{
    BrokenReason, InstanceRow, InstanceStatus, PartRow, PartValue, PlateIssueRow, PlateRow, ReferenceRow,
    ReferenceStatus, StructureEdit, StructureViewModel,
};
pub use task::{
    DocumentMeasure, ParsedImport, TaskError, TaskInput, TaskKind, TaskOutcome, TaskOutput, TaskPort, TaskPortError,
    TaskRequest, run_task,
};
pub use text_panel::{
    Align, Distort, MissingFontChoice, TextEdit, TextField, TextIssue, TextOnPathViewModel, TextPathEdit, TextToggle,
    TextToolbarViewModel, VAlign,
};
pub use variable_panel::{
    AdvancePolicy, CsvViewModel, SerialField, SerialViewModel, TestLine, VariableEdit, VariableIssue,
    VariablePanelViewModel, VariableProblem,
};
pub use view_model::{
    AppViewModel, BooleanPreview, CommandState, ConstraintHealth, ConstraintRow, CutSettingsViewModel,
    DocumentViewModel, ExportViewModel, FieldViewModel, GeometryDialogKind, GeometryDialogViewModel, ImportViewModel,
    InspectorViewModel, JobCardViewModel, LAYER_SLOTS, MachineButton, MachineStatusKey, MachineViewModel,
    MeasureViewModel, MessageKey, ModeChoice, ObjectRow, OperationRow, OperatorJobRow, OperatorViewModel,
    OrderViewModel, PALETTE_SLOTS, ParamRow, PlanViewModel, ProblemViewModel, SelectionCycle, ShapePanelViewModel,
    SketchConstraintKind, SketchModeViewModel, SketchStatus, SketchViewModel, StatusMessage, TOOL_SLOT_T1,
    TOOL_SLOT_T2, TaskBadge, ToolMeasurement, ToolRejection, TraceSummary, TracerViewModel,
};
