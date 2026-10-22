use std::collections::BTreeMap;

use std::sync::Arc;

use burnburn_cam::Finding;
use burnburn_document::UndoLabel;
use burnburn_document::sketch::{ConstraintId, DimensionRole, EntityId, SketchId};
use burnburn_editor::{
    Camera, GeometryIssue, IssueKind, SnapCandidate, SnapKind, SnapSettings, ToolKind, ToolPreview, Viewport,
};
use burnburn_geometry::{BooleanOp, NodeRef, Path};
use burnburn_job::{JobPlanId, PlanPreview};
use burnburn_types::{DocumentId, GeometryId, OperationId, RevisionId, TaskId, Unit};

use burnburn_machine::{ExecutionId, ExecutionState, MachineId};

use crate::catalog::ToolbarViewModel;
use crate::cut_options::{CutExtras, CutField, CutGates};
use crate::field::FieldError;
use crate::intent::{CommandKey, CutSettingsInput, FieldKey, ImportUnit, OperationField, OperationKind, ParamKey};
use crate::production::{ActionId, MachineAction, OrderId};
use crate::task::TaskKind;

/// Presentation data only: no native objects, no closures, user-visible text as localisation
/// keys (NL/EN) except user data (names, numbers). Hosts diff it to update widgets.
#[derive(Debug, Clone, PartialEq)]
pub struct AppViewModel {
    pub documents: Vec<DocumentViewModel>,
    pub active: Option<DocumentId>,
    /// Enablement for the active document; drives `validateMenuItem:` and the toolbar.
    pub commands: BTreeMap<CommandKey, CommandState>,
    /// The tool register as the toolbar draws it: panels, groups, selection and enablement.
    pub toolbar: ToolbarViewModel,
}

impl AppViewModel {
    pub fn document(&self, id: DocumentId) -> Option<&DocumentViewModel> {
        self.documents.iter().find(|d| d.id == id)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DocumentViewModel {
    pub id: DocumentId,
    pub revision: RevisionId,
    /// File name, or the localised "Untitled N" when never saved.
    pub title: String,
    /// macOS: `setDocumentEdited` (dot in close button); Windows: `*` in the title.
    pub dirty: bool,
    pub tool: ToolKind,
    /// Temporary gesture preview (never document data).
    pub preview: Option<ToolPreview>,
    pub objects: Vec<ObjectRow>,
    pub selection: Vec<GeometryId>,
    pub inspector: Option<InspectorViewModel>,
    pub undo: Option<UndoLabel>,
    pub redo: Option<UndoLabel>,
    pub messages: Vec<StatusMessage>,
    pub tasks: Vec<TaskBadge>,
    /// Result of the latest Measure task that matched the then-current revision.
    pub measure: Option<MeasureViewModel>,
    /// Readout of the measuring tool, cleared by the next tool choice or edit.
    pub tool_measurement: Option<ToolMeasurement>,
    pub camera: Camera,
    pub viewport: Viewport,
    /// Named snap target under the pointer (cursor label "Eindpunt", E04).
    pub snap: Option<SnapCandidate>,
    pub snap_settings: SnapSettings,
    /// After an Alt-click: which of the overlapping candidates is selected ("2 van 3", E03).
    pub selection_cycle: Option<SelectionCycle>,
    /// Align/distribute reference (first selected) when two or more objects are selected (E04).
    pub align_reference: Option<GeometryId>,
    /// The 3×3 point X and Y show and the size fields keep in place (window state).
    pub reference_point: crate::intent::RefPoint,
    /// A width or height commit scales both axes (window state).
    pub aspect_locked: bool,
    /// Draw outlines only (window state; hosts draw with `Theme::wireframe`).
    pub wireframe: bool,
    /// Located geometry problems of this revision (E09), also drawn as scene overlays.
    pub issues: Vec<GeometryIssue>,
    /// Error of the last typed drawing size; the gesture stays in preview (E01).
    pub tool_size_error: Option<MessageKey>,
    /// Latest boolean result computed for the current revision (live preview, E05).
    pub boolean_preview: Option<BooleanPreview>,
    /// Parametric sketches with their evaluation status and parameters (B8).
    pub sketches: Vec<SketchViewModel>,
    /// The last compiled plan of this document (kept after edits, then marked stale).
    pub plan: Option<PlanViewModel>,
    /// The last RD export of this document.
    pub export: Option<ExportViewModel>,
    /// The last G-code export (GRBL machines).
    pub gcode_export: Option<crate::GcodeExportViewModel>,
    /// The open tracer panel, if any (B9).
    pub tracer: Option<TracerViewModel>,
    /// The prepared (frozen) job of this document, if any (C1, W01).
    pub job: Option<JobCardViewModel>,
    /// The open import preview (P03), if any.
    pub import: Option<ImportViewModel>,
    /// The last refused edit with its place (offset, trim, repair); cleared by the next change.
    pub problem: Option<ProblemViewModel>,
    /// Operations panel rows, in execution order.
    pub operations: Vec<OperationRow>,
    /// Why the last operations-panel input sent no command.
    pub operation_error: Option<MessageKey>,
    /// The open Cut Settings sheet, if any.
    pub cut_settings: Option<CutSettingsViewModel>,
    /// Sketch mode (B8): the open sketch's constraint list, freedom and diagnosis.
    pub sketch_mode: Option<SketchModeViewModel>,
    /// Bitmap import sheet and Adjust Image panel (W2-L1-beeld).
    pub image: crate::image_panel::ImageViewModel,
    /// The geometry dialog (join, round corners, optimize) while it is open.
    pub geometry_dialog: Option<GeometryDialogViewModel>,
    /// The parameters of the single selected basic shape (the inspector's shape section); `None`
    /// for an ordinary path, which has no definition.
    pub shape_panel: Option<ShapePanelViewModel>,
    /// The options of the active tool (sides, ratio, radius, pattern counts, mirror copies).
    pub tool_options: Vec<ParamRow>,
    /// The selected nodes in node-edit mode; empty for every other tool.
    pub node_selection: Vec<NodeRef>,
    /// Dock, Move Together, reshape, slots, nesting and registration (W3-UI-schikken).
    pub schikken: crate::schikken::SchikkenViewModel,
    /// Offset options and the preview of the open offset (E05).
    pub offset: crate::offset::OffsetViewModel,
    /// The before/after of the repair being considered (E09).
    pub repair_preview: Option<crate::offset::RepairPreviewViewModel>,
    /// Junctions and primitive candidates of the open centre-line result, in centre-line mode (T04, T08).
    pub centerline: Option<crate::centerline_panel::CenterlineEditViewModel>,
    /// The compact operator view (W08), on every window while there is a machine service, an order
    /// or an approved job.
    pub operator: Option<OperatorViewModel>,
    /// The confirmation before a start while one is open (M06).
    pub start_confirm: Option<crate::start_confirm::StartConfirmViewModel>,
    /// The remnant library (W07).
    pub remnants: crate::remnants::RemnantsViewModel,
    /// Template recognition in the camera picture (W06).
    pub recognition: crate::recognition::RecognitionViewModel,
    /// Preparing the image of the open tracer: local threshold, plane correction, masks (T03).
    pub trace_prep: Option<crate::trace_prep::TracePrepViewModel>,
    /// Own recipes, the A/B comparison and the batch report of the open tracer (G7-trace, T10).
    pub trace_recipes: Option<crate::trace_recipes::TraceRecipesViewModel>,
    /// Parts, variants, plates and references (S06, S07, P06).
    pub structure: crate::structure_panel::StructureViewModel,
    /// The project notes panel while it is open (P04).
    pub notes: Option<crate::notes::NotesViewModel>,
    /// The text toolbar while exactly one text object is selected (W3-UI-tekst).
    pub text_toolbar: Option<crate::text_panel::TextToolbarViewModel>,
    /// The text-on-path dialog while it is open.
    pub text_on_path: Option<crate::text_panel::TextOnPathViewModel>,
    /// The variable-text panel while exactly one text object is selected.
    pub variable_panel: Option<crate::variable_panel::VariablePanelViewModel>,
    /// The barcode dialog while it is open.
    pub barcode_dialog: Option<crate::barcode_panel::BarcodeDialogViewModel>,
    /// The generator dialog (box, living hinge) while it is open.
    pub generator_dialog: Option<crate::generator_panel::GeneratorDialogViewModel>,
    /// The file panels: Material Library, Art Library, bundles and the export dialog.
    pub files: crate::files::FilesViewModel,
}

/// What the geometry dialog asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GeometryDialogKind {
    /// Gap tolerance of Join/Close.
    Join,
    /// Radius of Round Corners for the whole selection.
    RoundCorners,
    /// Error budget of Optimize Shapes.
    Optimize,
}

/// The value sheet of a selection-wide geometry command: the text is the default or what the
/// user typed last; `error` is why the last OK did nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeometryDialogViewModel {
    pub kind: GeometryDialogKind,
    pub text: String,
    pub error: Option<MessageKey>,
}

/// One setting row: a number as text, or an on/off choice (`toggle`, text `1` or `0`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamRow {
    pub key: ParamKey,
    pub text: String,
    pub toggle: bool,
    pub error: Option<MessageKey>,
}

/// The inspector section of one parametric basic shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShapePanelViewModel {
    pub object: GeometryId,
    pub rows: Vec<ParamRow>,
}

/// Number of colours in the layer palette (slots `0..LAYER_SLOTS`). Hosts draw slot `n` in the
/// theme's process-layer colour `n`, on the palette and on the canvas.
pub const LAYER_SLOTS: u8 = 30;
/// The tool layers T1 and T2 (LY01) follow the colours in the palette: guides and registration,
/// never output.
pub const TOOL_SLOT_T1: u8 = LAYER_SLOTS;
pub const TOOL_SLOT_T2: u8 = LAYER_SLOTS + 1;
/// Buttons in the palette: the colours and the two tool layers.
pub const PALETTE_SLOTS: u8 = LAYER_SLOTS + 2;

/// The Cut Settings sheet of one operation: current values as editable text, the modes the
/// machine profile allows, the two toggles and why the last OK was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CutSettingsViewModel {
    pub operation: OperationId,
    pub name: String,
    pub slot: u8,
    pub mode: OperationKind,
    /// Line, Fill, Offset fill; a blocked mode stays visible with its reason (C05, M02).
    pub modes: Vec<ModeChoice>,
    pub speed: String,
    pub power_min: String,
    pub power_max: String,
    pub passes: String,
    pub kerf: String,
    pub output_enabled: bool,
    pub visible: bool,
    pub error: Option<MessageKey>,
    /// Current values of the option fields of this operation's mode (empty for other modes).
    pub extras: CutExtras,
    /// What the machine profile blocks; blocked fields stay visible with their reason.
    pub gates: CutGates,
}

impl CutSettingsViewModel {
    /// What pressing OK without touching anything would send.
    pub fn input(&self) -> CutSettingsInput {
        CutSettingsInput {
            mode: self.mode,
            speed: self.speed.clone(),
            power_min: self.power_min.clone(),
            power_max: self.power_max.clone(),
            passes: self.passes.clone(),
            kerf: self.kerf.clone(),
            output_enabled: self.output_enabled,
            visible: self.visible,
            extras: self.extras.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeChoice {
    pub kind: OperationKind,
    /// `Some` when the active machine profile does not support this mode.
    pub blocked: Option<MessageKey>,
}

/// One operation in the operations panel (a layer). Speed (mm/s) and power (%) as display text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationRow {
    pub id: OperationId,
    pub name: String,
    pub kind: OperationKind,
    /// Palette colour of this layer (`0..PALETTE_SLOTS`; the last two are the tool layers).
    pub slot: u8,
    /// A tool layer (T1/T2): never output.
    pub tool: bool,
    pub speed: String,
    /// Maximum power.
    pub power: String,
    pub power_min: String,
    pub passes: String,
    /// Line kerf width in mm ("0" without kerf or for other modes).
    pub kerf: String,
    /// Its objects are drawn on the canvas (window state, C01).
    pub visible: bool,
    /// Number of geometry objects the operation uses.
    pub targets: usize,
    pub output_enabled: bool,
}

/// Import preview and report (B6): physical size for the current unit decision, what was not
/// imported (with source reference) and open contours.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportViewModel {
    pub file: String,
    /// No declared unit and no choice yet: Importeer stays disabled.
    pub needs_unit: bool,
    pub unit: Option<ImportUnit>,
    pub units: Option<burnburn_io::import::UnitDecision>,
    /// Overall size in mm with the current decision (`None` while the unit is unknown).
    pub size_mm: Option<(f64, f64)>,
    pub items: usize,
    pub skipped: Vec<burnburn_io::import::Skipped>,
    pub open_contours: usize,
    /// The format of the file (decides which sections the preview shows).
    pub kind: crate::files::ImportKind,
    /// PDF/AI: the pages of the file; empty for every other format.
    pub pages: Vec<crate::files::ImportPage>,
    /// PDF/AI: the pages the preview and the commit use.
    pub page_choice: Option<crate::files::ImportPages>,
    /// PDF/AI: the objects of the chosen pages are still being read.
    pub busy: bool,
    /// Layers the import proposes (stroke/fill colours of a PDF, pens of a plotter file).
    pub layers: Vec<crate::files::ImportLayerRow>,
    /// The user accepted the proposed layers: the import also creates their operations (P04).
    pub layers_accepted: bool,
    /// LightBurn project: what was and was not taken over, per layer.
    pub lbrn2: Option<crate::files::Lbrn2ReportViewModel>,
}

/// A refused edit, located: the host zooms to `at` and highlights `object`.
#[derive(Debug, Clone, PartialEq)]
pub struct ProblemViewModel {
    pub message: MessageKey,
    pub object: Option<GeometryId>,
    pub at: burnburn_geometry::Point,
}

/// Job card (C1): the frozen job revision. It never follows later edits; `design_changed` shows
/// the banner "ontwerp gewijzigd sinds job".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobCardViewModel {
    pub plan: JobPlanId,
    pub plan_revision: RevisionId,
    pub rd_hash: burnburn_types::ContentHash,
    pub bytes: usize,
    /// Released for the operator view (IM31).
    pub approved: bool,
    pub design_changed: bool,
    /// Upload of exactly this job; `None` without a machine service.
    pub upload: Option<MachineButton>,
}

/// Compact operator view (W08, IM31): machine status, approved jobs and orders. It has no
/// tools, fields or document commands by construction.
#[derive(Debug, Clone, PartialEq)]
pub struct OperatorViewModel {
    pub machine: Option<MachineViewModel>,
    /// Approved prepared jobs only.
    pub jobs: Vec<OperatorJobRow>,
    pub orders: Vec<OrderViewModel>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OperatorJobRow {
    /// The window the job was prepared in; `None` for a released job that comes from the history
    /// (after a restart) and has no window.
    pub document: Option<DocumentId>,
    pub name: String,
    /// The document revision the job was compiled from (frozen with the job).
    pub revision: RevisionId,
    /// The size and place the job takes on the bed, from the frozen plan; `None` for a job rebuilt
    /// from the history without its plan.
    pub bounds: Option<burnburn_geometry::Bounds>,
    /// The material of the plates in the project the job was made from, when they agree.
    pub material: Option<String>,
    pub plan: JobPlanId,
    pub rd_hash: burnburn_types::ContentHash,
    pub upload: Option<MachineButton>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderViewModel {
    pub id: OrderId,
    pub name: String,
    pub part: String,
    pub quantity: u32,
    /// Executions of this order with Completed evidence. Never from uploads, stored or unknown.
    pub completed: u32,
    pub remnant: Option<String>,
    /// How the named remnant compares with the order's job; `None` without a remnant.
    pub remnant_link: Option<crate::remnants::RemnantLinkStatus>,
    /// The job that produces this order: the only candidate, or the explicit choice.
    pub job: Option<JobPlanId>,
    /// Approved jobs whose name matches the order's part.
    pub candidates: Vec<JobPlanId>,
    /// More than one candidate and no choice yet: nothing is picked automatically.
    pub needs_choice: bool,
    /// Upload of the order's job (send as `UiIntent::UploadForOrder`).
    pub upload: Option<MachineButton>,
}

/// One machine control. Hosts send `UiIntent::Machine` with `action` and `seen` unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineButton {
    pub action: ActionId,
    pub enabled: bool,
    /// The execution state this button was offered in.
    pub seen: Option<ExecutionState>,
    pub label: MessageKey,
}

/// Status shown large in the machine panel and the operator view; follows evidence only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MachineStatusKey {
    Idle,
    /// The controller's identity is checked against the profile before any job byte is sent.
    VerifyingIdentity,
    /// An earlier upload is checked again (identity, then read-back); no job bytes are sent.
    Reverifying,
    Uploading,
    /// Every chunk has a transport ACK; storage not confirmed ("geüpload, nog niet opgeslagen bevestigd").
    UploadedNotConfirmed,
    Stored,
    StartRequested,
    Running,
    Completed,
    Failed,
    Cancelled,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineViewModel {
    pub machine: MachineId,
    pub status: MachineStatusKey,
    /// Explanation with the status (e.g. "status onbekend — controleer machine").
    pub message: Option<MessageKey>,
    pub execution: Option<ExecutionId>,
    pub job: Option<JobPlanId>,
    pub job_name: Option<String>,
    /// Chunks with a transport ACK, of total.
    pub progress: Option<(usize, usize)>,
    /// Controls for the current execution (never an upload; that is on the job card).
    pub buttons: Vec<MachineButton>,
    /// The last handled action and whether it was accepted (visible after a reconnect).
    pub last_action: Option<(ActionId, bool)>,
}

impl MachineViewModel {
    pub fn button(&self, kind: MachineAction) -> Option<&MachineButton> {
        self.buttons.iter().find(|b| b.action.kind == kind)
    }

    /// True when an enabled control would send `kind`.
    pub fn offers(&self, kind: MachineAction) -> bool {
        self.button(kind).is_some_and(|b| b.enabled)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TracerViewModel {
    pub width_px: u32,
    pub height_px: u32,
    /// No physical scale declared yet: the user must give one before tracing (T01).
    pub needs_scale: bool,
    /// A trace task is running ("berekenen…"); the last valid result stays visible.
    pub computing: bool,
    pub result: Option<TraceSummary>,
    /// The panel's fields, the recipe in force and the last refused input.
    pub form: crate::image_panel::TraceFormViewModel,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraceSummary {
    pub components: usize,
    pub holes: usize,
    pub width_mm: f64,
    pub height_mm: f64,
    /// Declared source resolution, shown apart from the fitting tolerance.
    pub mm_per_pixel: (f64, f64),
    pub fit_tolerance_mm: f64,
    pub preview: Arc<Path>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlanViewModel {
    pub id: JobPlanId,
    /// Document revision the plan was compiled from.
    pub revision: RevisionId,
    /// The document or the machine profile changed since compiling.
    pub stale: bool,
    pub preview: Arc<PlanPreview>,
    pub findings: Vec<Finding>,
    /// What the GRBL backend refuses of this plan (empty on a Ruida machine); each one is also
    /// counted in `errors`.
    pub grbl: Vec<crate::GrblFinding>,
    /// The measured values the compiler used for this plan, with the proofs they came from (W09).
    pub calibration: Vec<burnburn_job::AppliedCalibration>,
    pub errors: usize,
    pub warnings: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportViewModel {
    pub plan: JobPlanId,
    pub plan_revision: RevisionId,
    pub path: std::path::PathBuf,
    pub rd_hash: burnburn_types::ContentHash,
    pub bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SketchViewModel {
    pub id: SketchId,
    pub name: String,
    pub status: SketchStatus,
    /// Parameters as typed (name, text).
    pub parameters: Vec<(String, String)>,
    /// Number of constraints (relations and dimensions).
    pub constraints: usize,
    /// Remaining degrees of freedom of the last valid solution (design aid, S03).
    pub dof: Option<u32>,
    /// Constraints the solver names in its diagnosis (conflict, unmet, invalid value): the
    /// constraint panel selects these (S04).
    pub problem_constraints: Vec<ConstraintId>,
    /// The most recently added constraint (highest id), e.g. to undo a suggestion.
    pub last_constraint: Option<ConstraintId>,
}

/// What a constraint is, for the list's first column (hosts translate it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SketchConstraintKind {
    Coincident,
    PointOn,
    Horizontal,
    Vertical,
    Parallel,
    Perpendicular,
    Tangent,
    Equal,
    Midpoint,
    Symmetric,
    Concentric,
    Fixed,
    Length,
    Distance,
    DistanceX,
    DistanceY,
    Radius,
    Diameter,
    Angle,
    MinorDiameter,
}

/// What the solver says about one constraint (symbol and words, not colour only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ConstraintHealth {
    Satisfied,
    /// Holds, but another constraint already says the same.
    Redundant,
    Conflicting,
    /// Could not be met (the sketch did not converge).
    Unmet,
    /// A dimension whose text or parameters are invalid.
    Invalid,
    /// A driven dimension: measured, never constraining.
    Measured,
    /// Not evaluated.
    Unknown,
}

/// One row of the constraint list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstraintRow {
    pub id: ConstraintId,
    pub kind: SketchConstraintKind,
    /// The entities it relates, in argument order.
    pub entities: Vec<EntityId>,
    /// A dimension's text as typed (`breedte + 20 mm`); editable.
    pub text: Option<String>,
    /// A dimension's current value in mm or degrees, from the solved geometry.
    pub value: Option<String>,
    pub role: Option<DimensionRole>,
    pub health: ConstraintHealth,
    pub selected: bool,
}

/// Sketch mode of one window: the open sketch, what is selected and what the solver says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SketchModeViewModel {
    pub sketch: SketchId,
    pub name: String,
    pub status: SketchStatus,
    /// Why the sketch failed, in words (`SketchConflict`, `SketchNotConverged`, …).
    pub problem: Option<MessageKey>,
    /// The constraints the diagnosis names: `SelectSketchProblem` selects them.
    pub problem_constraints: Vec<ConstraintId>,
    /// Remaining degrees of freedom of the last valid solution (0 = fully constrained).
    pub dof: Option<u32>,
    pub rows: Vec<ConstraintRow>,
    pub parameters: Vec<(String, String)>,
    pub selected_entities: Vec<EntityId>,
    pub selected_constraints: Vec<ConstraintId>,
    /// Why the last sketch input did nothing (selection does not fit, shape unsupported, …).
    pub refused: Option<MessageKey>,
    /// "Herstel laatste geldige" applies: the sketch failed and there is history to go back in.
    pub can_restore: bool,
    /// How many curves ended on another geometric branch than the one drawn (the other side of a
    /// tangent, the other sweep of an arc): reported, never silent (S05).
    pub branch_flips: usize,
}

/// Evaluation state shown with symbol and text (not colour only, X03).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SketchStatus {
    Valid,
    /// Not evaluated since the last definition change.
    Pending,
    /// Evaluation failed; the geometry shown is the last valid one.
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionCycle {
    /// 1-based position among the candidates under the pointer.
    pub position: usize,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BooleanPreview {
    pub revision: RevisionId,
    pub op: BooleanOp,
    pub operands: Vec<GeometryId>,
    pub result: Arc<Path>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectRow {
    pub id: GeometryId,
    pub label: String,
    pub depth: u16,
    pub selected: bool,
    /// Not hidden (Arrange ▸ Verberg).
    pub visible: bool,
    /// Locked objects are not picked on the canvas.
    pub locked: bool,
    /// Group chain, outermost first (empty: ungrouped).
    pub groups: Vec<burnburn_types::GroupId>,
    /// How many layers use this object (C02): more than one is "linked", the lists say how many.
    pub linked: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorViewModel {
    pub object: GeometryId,
    pub fields: Vec<FieldViewModel>,
}

impl InspectorViewModel {
    pub fn field(&self, key: FieldKey) -> Option<&FieldViewModel> {
        self.fields.iter().find(|f| f.key == key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldViewModel {
    pub key: FieldKey,
    pub label: MessageKey,
    pub text: String,
    pub unit: Unit,
    pub editable: bool,
    pub error: Option<MessageKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandState {
    pub enabled: bool,
    pub checked: bool,
    pub label: MessageKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusMessage {
    pub key: MessageKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskBadge {
    pub id: TaskId,
    pub kind: TaskKind,
}

/// Why the active tool refused its gesture (the document is unchanged); hosts say it in words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ToolRejection {
    NothingPicked,
    DerivedGeometry,
    NeedsSelection,
    InvalidSelection,
    NothingToConstrain,
    /// Auto-constrain found only unclear relations and applied none (S08).
    AmbiguousRelations,
    InvalidValue,
    Corner,
    Trim,
    Extend,
    Break,
    Offset,
    /// Construction geometry exists only inside a sketch.
    ConstructionOutsideSketch,
    /// Reserved: fillet and chamfer also cut corners between curves since golf 8.
    CurvedCorner,
    /// Only lines and arcs can be extended.
    BezierExtend,
    /// The gesture needs one selected object: the document has no atomic command for several.
    SingleObjectOnly,
    /// Not available for another reason that the tool names itself.
    Unavailable,
}

impl ToolRejection {
    pub const ALL: [ToolRejection; 17] = [
        ToolRejection::NothingPicked,
        ToolRejection::DerivedGeometry,
        ToolRejection::NeedsSelection,
        ToolRejection::InvalidSelection,
        ToolRejection::NothingToConstrain,
        ToolRejection::AmbiguousRelations,
        ToolRejection::InvalidValue,
        ToolRejection::Corner,
        ToolRejection::Trim,
        ToolRejection::Extend,
        ToolRejection::Break,
        ToolRejection::Offset,
        ToolRejection::ConstructionOutsideSketch,
        ToolRejection::CurvedCorner,
        ToolRejection::BezierExtend,
        ToolRejection::SingleObjectOnly,
        ToolRejection::Unavailable,
    ];
}

/// The readout of a measuring tool (Dimension outside a sketch); a readout, never document data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolMeasurement {
    pub distance_text: String,
    pub dx_text: String,
    pub dy_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeasureViewModel {
    pub revision: RevisionId,
    pub objects: usize,
    /// `None` for an empty document.
    pub width_text: Option<String>,
    pub height_text: Option<String>,
}

/// Localisation keys. Hosts map them to NL/EN strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MessageKey {
    /// Dock, Move Together and the other Schikken flows refused something, or report (W3-UI-schikken).
    Schikken(crate::schikken::SchikkenMessage),
    /// The active tool refused its gesture; nothing changed.
    ToolRejected(ToolRejection),
    /// Text input without any font to draw it with.
    NoFontAvailable,
    /// Operations-panel text that is not a valid number for that field.
    InvalidNumber(OperationField),
    /// Cut Settings: this option field holds no valid value (number, range or relation to a
    /// sibling field); nothing was changed.
    InvalidCutField(CutField),
    /// Cut Settings: Dot mode excludes tabs, leads, overcut and perforation.
    CutOptionsConflict,
    /// Cut Settings: this field does not apply to the chosen image mode.
    CutOptionNotForMode(CutField),
    /// The camera may only start after the OS granted access.
    CameraNotAuthorized,
    /// Camera image: a visual placement aid, not production geometry.
    VisualPlacementOnly,
    Command(CommandKey),
    UndoAction(UndoLabel),
    RedoAction(UndoLabel),
    Field(FieldKey),
    InvalidLength(FieldError),
    CommandRejected,
    /// An extension of a Bézier end went straight along its end tangent, for this reason (G9-editor).
    ExtendedStraight(burnburn_geometry::ExtendNote),
    /// A text that follows a guide cannot be turned, scaled or sheared on its own (G9-editor, E08):
    /// it follows its guide; move it, change its start and distance, or turn the guide.
    BoundTextFollowsGuide,
    /// A structure edit (part, variant, reference) was refused for this reason (G6-werk).
    Structure(crate::structure_panel::StructureProblem),
    SaveFailed,
    /// The file on disk was written by a newer BurnBurn: not overwritten, nothing was saved (X07).
    SaveNewerFileOnDisk,
    /// Saved; the previous version of the file was kept next to it (X07).
    PreviousVersionKept,
    /// The undo history hit its memory budget: this many of the oldest steps can no longer be undone (X04).
    HistoryTrimmed(usize),
    /// A machine profile was chosen that this app does not know; nothing changed.
    UnknownMachineProfile,
    /// The chosen machine profile file cannot be used (unreadable, damaged or not a profile);
    /// nothing changed.
    MachineProfileFileUnusable,
    /// A repeat cannot be set up: the machine profile is not the one the job was frozen for.
    RepeatRefused,
    /// Machine hub: the control that stops an upload that is under way.
    StopUpload,
    OpenFailed,
    /// The project was written by a newer BurnBurn: refused, the file is left untouched (P01).
    OpenNewerSchema,
    /// Cursor label of a snap target.
    Snap(SnapKind),
    Issue(IssueKind),
    BooleanFailed,
    /// No machine profile selected (preferences).
    NoMachineProfile,
    CompileFailed,
    ExportFailed,
    /// The GRBL backend refuses the plan: no G-code was written (the preflight names why).
    GcodeRefused,
    /// RD files are for Ruida machines; the active machine is a GRBL machine.
    RdNotForGrbl,
    /// G-code files are for GRBL machines; the active machine is a Ruida machine.
    GcodeNotForRuida,
    ImageFailed,
    TraceFailed,
    /// Saved, but the OS did not confirm durability (directory sync failed). Not an error dialog.
    SaveNotConfirmedDurable,
    /// Label of a machine control ("Software-stop" for `Cancel` after a start).
    MachineAction(MachineAction),
    /// A laser-window action or compile was refused; the reason says why, in words.
    LaserRefused(crate::laser::LaserWhy),
    /// The diagnostic report could not be rendered or written.
    DiagnosticsFailed,
    PrepareFailed,
    /// "Ontwerp gewijzigd sinds job – nieuwe conceptversie".
    DesignChangedSinceJob,
    /// "Geüpload, nog niet opgeslagen bevestigd".
    UploadedNotStoredConfirmed,
    /// "Start verzonden; wacht op status".
    StartRequestedAwaitingStatus,
    /// "Status onbekend — controleer machine" (lost start reply; never retried).
    StatusUnknownCheckMachine,
    /// "Software-stop verzonden; uitkomst onbekend — controleer machine. Hardware-noodstop blijft leidend."
    StopSentOutcomeUnknown,
    /// "Upload onbevestigd — controleer controller" (no automatic second upload).
    UploadUnconfirmedCheckController,
    /// A cancelled upload may have left a (partial) file on the controller; it is not deleted.
    ControllerFileMayRemain,
    UploadRejected,
    /// The controller reported another model/firmware than the profile; nothing was uploaded.
    IdentityMismatch,
    /// The identity query stayed unanswered; nothing was uploaded.
    IdentityUnconfirmed,
    NotStored,
    ControllerFault,
    /// The machine moved on since the control was shown, or the action is not possible now.
    MachineActionRefused,
    /// The intent could not be recorded in the history; nothing was sent.
    IntentNotRecorded,
    /// The file could not be read or parsed; nothing was imported.
    ImportFailed,
    /// A file feature (import, template, library, bundle, export) was refused for this reason.
    Files(crate::files::FilesMessage),
    /// An offset would remove a whole object (or needs closed contours); nothing changed.
    OffsetImpossible,
    /// The trim found no crossing on the picked line, or the pick is not on a line.
    TrimImpossible,
    /// The chosen gap is larger than the repair tolerance; nothing changed.
    NothingToRepair,
    /// The current role may not do this (operator: no editing, approved jobs only).
    NotAllowedForRole,
    /// A palette colour was clicked without a selection.
    SelectObjectsFirst,
    /// A tool layer (T1/T2) is never output; its output cannot be switched on.
    ToolLayerNoOutput,
    /// The tabs of a Line layer no longer fit a shape of the layer (said at the tab field).
    TabsDoNotFit,
    /// The machine profile does not support (or has not validated) this feature (C05, M02).
    MissingCapability(burnburn_job::Feature),
    /// The kind of a constraint in the constraint list.
    SketchConstraint(SketchConstraintKind),
    /// The solver found contradicting constraints; the last valid geometry stays.
    SketchConflict,
    /// The solver did not reach a solution; the last valid geometry stays.
    SketchNotConverged,
    /// A dimension's text is not a valid length or angle.
    SketchInvalidDimensions,
    /// A parameter is invalid (unit, name or cycle).
    SketchInvalidParameters,
    SketchInvalidDefinition,
    /// The selected entities do not fit this relation or dimension.
    SketchSelectionDoesNotFit,
    /// Nothing is selected to relate.
    SketchNothingSelected,
    /// "Herstel laatste geldige": no earlier state of this sketch solved.
    SketchNothingToRestore,
    /// The source image of a trace is not in the project any more.
    TraceSourceMissing,
    /// A parameter that is still used by a dimension or another parameter is not removed.
    SketchParameterInUse,
    /// Ellipses, free curves and the like cannot be drawn into a sketch yet.
    SketchShapeUnsupported,
    /// The selected objects contain curves other than full circles.
    SketchConversionImpossible,
    /// The sketch edit was refused by the document.
    SketchEditRefused,
    /// The operation would leave these shapes as they are (already smooth, already optimal, …).
    NothingToChange,
    /// A shape parameter outside what the shape allows (sides, tips, ratio, radius).
    ShapeParamInvalid,
    /// The pasteboard payload is not a readable BurnBurn clipboard (damaged, foreign or too new).
    ClipboardUnreadable,
    /// Delete Duplicates found no identical stacked shapes.
    NoDuplicatesFound,
    /// A selection command matched nothing.
    NothingFound,
    /// The change would move or delete locked objects (unlock them first).
    ObjectsLocked,
    /// RD output is refused for a feature the Ruida backend has no proven form for (W3-UI-cam).
    RdRefused(crate::RdRefusal),
}
