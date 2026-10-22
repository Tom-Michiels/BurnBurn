use std::path::PathBuf;

use burnburn_document::process::OperationDraft;
use burnburn_document::sketch::{ConstraintId, DimensionRole, EntityId, SketchConstraint, SketchId};
use burnburn_editor::{CanvasKey, Modifiers, PointerButton, PointerId, PointerPhase, Viewport};
use burnburn_job::{JobPlanId, MachineProfile};
use burnburn_machine::ExecutionState;
use burnburn_ruida::RuidaProfile;
use burnburn_trace::TraceRecipe;
use burnburn_types::{DocumentId, GeometryId, MonotonicTime, OperationId};

use crate::barcode_panel::BarcodeEdit;
use crate::camera::BedCameraIntent;
use crate::controller::AppIntent;
use crate::cut_options::CutExtras;
use crate::image_panel::{ImageAdjustInput, TraceFormInput, TracePreset};
use crate::laser::{LaserCommand, LaserSetup};
use crate::production::{ActionId, OrderDraft, OrderId};
use crate::text_panel::{TextEdit, TextPathEdit};
use crate::variable_panel::VariableEdit;

/// A semantic user action. Native hosts translate OS events into these; they never change
/// document fields and never parse field text themselves. Menu, toolbar, shortcut and command
/// palette all use `Invoke`.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum UiIntent {
    NewDocument,
    /// Path chosen in NSOpenPanel / IFileOpenDialog.
    Open {
        path: PathBuf,
    },
    /// A document window became key/active.
    Focus {
        document: DocumentId,
    },
    Save {
        document: DocumentId,
        path: Option<PathBuf>,
    },
    Close {
        document: DocumentId,
    },
    /// Answer to `HostEffect::ConfirmClose`.
    ConfirmClose {
        document: DocumentId,
        choice: CloseChoice,
    },
    DialogCancelled {
        document: DocumentId,
        dialog: DialogKind,
    },
    /// Selection from the object tree (canvas clicks arrive as `Pointer`).
    Select {
        document: DocumentId,
        ids: Vec<GeometryId>,
        mode: SelectMode,
    },
    /// Raw text from an inspector field; parsing with units happens here, not in the host (DR-05).
    CommitField {
        document: DocumentId,
        field: FieldKey,
        text: String,
    },
    /// Exact size typed in the drawing HUD while a shape is being dragged (E01): `width` is the
    /// diameter for the circle tool; an empty `height` means "same as width".
    CommitToolSize {
        document: DocumentId,
        width: String,
        height: String,
    },
    /// Choose a tool by its catalog id (`ToolCatalog`): the toolbar's one way to pick a tool.
    SelectTool {
        document: Option<DocumentId>,
        id: String,
    },
    /// Choose a variant from a group's drop-down: the group remembers it and the tool starts.
    SelectVariant {
        document: Option<DocumentId>,
        group: String,
        id: String,
    },
    Invoke {
        document: Option<DocumentId>,
        command: CommandKey,
    },
    /// Machine preferences (B11): the active machine profile for compiling and RD export.
    SetMachineProfile {
        profile: Box<MachineProfile>,
        ruida: RuidaProfile,
    },
    /// The active machine becomes the known profile with this id (G5-machine-rest, P07): the one
    /// it replaces is kept in the profile library. Unknown ids change nothing.
    ChooseMachineProfile {
        id: String,
    },
    /// A machine profile file (Ruida or GRBL JSON) chosen in the host's open panel (G6-machine):
    /// it is read off the UI thread and, when usable, becomes the active machine. An unusable file
    /// changes nothing and is reported.
    OpenMachineProfile {
        path: PathBuf,
    },
    /// Machine hub (W03): prepare a repeat of an earlier execution from its frozen job. Lists the
    /// differences with the document's recipes; nothing is sent or changed yet.
    RepeatExecution {
        execution: burnburn_machine::ExecutionId,
    },
    /// Machine hub: make the repeat's frozen job the window's prepared job (upload is a separate,
    /// explicit step and creates a new execution).
    ConfirmRepeat {
        document: DocumentId,
    },
    CancelRepeat,
    /// The Start control was used (M06): opens the confirmation of the stored job. Nothing is sent.
    RequestStart {
        document: Option<DocumentId>,
    },
    /// The user confirmed exactly the confirmation on screen. `action` and `seen` are copied from
    /// `StartConfirmViewModel`; a confirmation that is not current any more sends nothing.
    ConfirmStart {
        document: Option<DocumentId>,
        action: crate::production::ActionId,
        seen: Option<burnburn_machine::ExecutionState>,
    },
    /// Closes the confirmation without starting.
    CancelStart,
    /// Recognising products in the camera picture (W06): learn, find, decide, place.
    Recognition(crate::recognition::RecognitionIntent),
    /// The remnant library (W07): keep the selection, remove, place.
    Remnant(crate::remnants::RemnantIntent),
    /// Names the remnant an order is made from, or none. The name must be in the remnant library.
    /// Linking changes no count.
    SetOrderRemnant {
        order: crate::production::OrderId,
        remnant: Option<String>,
    },
    /// Machine hub (W09): record one calibration proof.
    RecordCalibration {
        input: crate::machine_hub::ProofInput,
    },
    RemoveCalibration {
        id: u64,
    },
    /// The same for a GRBL machine (G5-grbl-ui): the profile carries the GRBL capability matrix
    /// and `grbl` the dialect settings. It replaces the active machine, Ruida or GRBL.
    SetGrblMachineProfile {
        profile: Box<MachineProfile>,
        grbl: Box<burnburn_gcode::GrblProfile>,
    },
    /// Process-layer panel (B10, C01): a new operation for the selected objects.
    AddOperation {
        document: DocumentId,
        draft: OperationDraft,
    },
    /// Location chosen in the export save panel (B12, M03).
    ExportRd {
        document: DocumentId,
        path: PathBuf,
    },
    /// A control of the GRBL laser window (G5-grbl-ui): console, macros, jog, home, unlock.
    Grbl(crate::grbl_panel::GrblIntent),
    /// Location chosen in the G-code save panel (G5-grbl-ui).
    ExportGcode {
        document: DocumentId,
        path: PathBuf,
    },
    /// Tracer (B9): an image file to trace; it is read and decoded off the UI thread.
    OpenTraceSource {
        document: DocumentId,
        path: PathBuf,
    },
    /// Tracer panel settings (scale, threshold, fitting); every change starts a new trace task.
    SetTraceRecipe {
        document: DocumentId,
        recipe: Box<TraceRecipe>,
    },
    /// Parameter panel of a sketch (B8, S02): raw text as typed, e.g. `150 mm` or
    /// `materiaaldikte + speling`; the sketch evaluator parses and solves it.
    SetSketchParameter {
        document: DocumentId,
        sketch: SketchId,
        name: String,
        text: String,
    },
    /// Remove a parameter of the sketch (S02, one history entry). A parameter that a dimension or
    /// another parameter still uses is not removed; the panel says so.
    RemoveSketchParameter {
        document: DocumentId,
        sketch: SketchId,
        name: String,
    },
    /// Constraint panel (S01/S03): add a relation or dimension (one history entry).
    AddSketchConstraint {
        document: DocumentId,
        sketch: SketchId,
        constraint: SketchConstraint,
    },
    RemoveSketchConstraint {
        document: DocumentId,
        sketch: SketchId,
        constraint: ConstraintId,
    },
    /// Dimension field: expression text as typed (`breedte + 20 mm`); the evaluator parses it.
    SetSketchDimension {
        document: DocumentId,
        sketch: SketchId,
        constraint: ConstraintId,
        text: String,
    },
    /// Constraint panel and canvas (sketch mode): select sketch entities and/or constraints of the
    /// open sketch. Window state, never a document command.
    SketchSelect {
        document: DocumentId,
        entities: Vec<EntityId>,
        constraints: Vec<ConstraintId>,
        mode: SelectMode,
    },
    /// "Maat": a dimension on the selected sketch entities. `text` is raw (DR-05); empty = the
    /// value as drawn. The kind is inferred from the selection unless `kind` names one.
    AddSketchDimension {
        document: DocumentId,
        kind: SketchDimensionKind,
        text: String,
    },
    /// Measure ("(120)") or drive a dimension. Turning it into a driving one that over-constrains
    /// the sketch is accepted and shown as a conflict (S03).
    SetSketchDimensionRole {
        document: DocumentId,
        sketch: SketchId,
        constraint: ConstraintId,
        role: DimensionRole,
    },
    /// The conflict message was clicked: select the constraints involved (S04).
    SelectSketchProblem {
        document: DocumentId,
    },
    /// "Herstel laatste geldige" (S04): undo the failed edits of this sketch until it solves
    /// again. They stay available as redo. Refused (with a message) when no earlier state solved.
    RestoreLastValidSketch {
        document: DocumentId,
        sketch: SketchId,
    },
    /// Type filter (E03): select every visible, unlocked object of `kind` (Replace), or add them
    /// to the selection (Toggle).
    SelectByKind {
        document: DocumentId,
        kind: burnburn_document::ObjectKind,
        mode: SelectMode,
    },
    /// Bestand > Notities tonen (P04): stores the project notes as one undo step. The same text is
    /// no new step; a text beyond the limit is refused and the panel stays open.
    SetNotes {
        document: DocumentId,
        text: String,
    },
    /// Closes the notes panel without a change.
    CloseNotes {
        document: DocumentId,
    },
    /// Canvas pointer input in logical canvas points (DR-04); presentation applies the camera.
    Pointer {
        document: DocumentId,
        event: ViewPointerEvent,
    },
    /// Canvas key (Esc, Return, Backspace, Delete).
    Key {
        document: DocumentId,
        key: CanvasKey,
        modifiers: Modifiers,
    },
    /// Pan, zoom and resize: window state, never a document command.
    Camera {
        document: DocumentId,
        intent: CameraIntent,
    },
    /// A machine control (job card, machine panel, operator view). `action` and `seen` are copied
    /// unchanged from the `MachineButton` the user clicked; this never becomes a document command.
    Machine {
        document: Option<DocumentId>,
        action: ActionId,
        seen: Option<ExecutionState>,
    },
    /// A laser-window control (jog, move, home, frame, pause, files, ...). `action` and `seen` are
    /// copied unchanged from the `LaserControl` the user used; `None` action is refused. Never a
    /// document command.
    Laser {
        document: Option<DocumentId>,
        action: Option<ActionId>,
        seen: Option<ExecutionState>,
        command: LaserCommand,
    },
    /// Laser window setup: start from, job origin, cut selected, selection origin, step, frame speed,
    /// position-laser mode, user origin. Window state; the next compile uses it.
    LaserSetup {
        setup: LaserSetup,
    },
    /// Focus lost, window closed or panel hidden: releases the held jog key. Always allowed.
    LaserReleaseJog,
    /// Help ▸ Diagnose exporteren: the replay of the journal as text, written to `path` (chosen in a
    /// save panel). Private data is in only when ticked; there is no machine contact.
    ExportDiagnostics {
        options: crate::production::DiagnosticOptions,
        path: PathBuf,
    },
    /// Archief ▸ Importeer (P03): the file chosen in the open panel; parsed off the UI thread.
    Import {
        document: DocumentId,
        path: PathBuf,
    },
    /// Unit choice in the import preview (required for a unitless DXF, an override otherwise).
    SetImportUnit {
        document: DocumentId,
        unit: ImportUnit,
    },
    /// The file features: import pages, templates, libraries, bundles and the export dialog.
    Files(crate::files::FilesIntent),
    /// Offset panel (E05): signed distance as typed (`2 mm` outward, `-1,5 mm` inward) for the
    /// selected objects; sources are kept.
    Offset {
        document: DocumentId,
        distance: String,
    },
    /// Offset preview (E05): computes the result on a worker for the selected objects and shows its
    /// size; the document does not change. A newer preview or any edit replaces/clears it.
    PreviewOffset {
        document: DocumentId,
        distance: String,
    },
    /// Corner shape and keep-sources choice of the next offset; an open preview is recomputed.
    SetOffsetOptions {
        document: DocumentId,
        options: crate::offset::OffsetOptions,
    },
    /// Repair preview (E09): what closing the gaps of problem `issue` at this tolerance would do,
    /// computed on a worker; the document does not change.
    PreviewRepair {
        document: DocumentId,
        issue: usize,
        tolerance: String,
    },
    /// A preparation of the open tracer's image: local threshold, four-point correction, masks (T03).
    TracePrep {
        document: DocumentId,
        intent: crate::trace_prep::TracePrepIntent,
    },
    /// Own recipes, A/B comparison and batch tracing of the open tracer (G7-trace, T10).
    TraceRecipe {
        document: DocumentId,
        intent: crate::trace_recipes::TraceRecipeIntent,
    },
    /// Pointing in the image of the open tracer: corners clicked, masks painted (G7-trace, T03).
    TracePick {
        document: DocumentId,
        pick: crate::trace_pick::TracePick,
    },
    /// A decision on the open centre-line result (T04, T08).
    CenterlineEdit {
        document: DocumentId,
        edit: crate::centerline_panel::CenterlineEdit,
    },
    /// The plate Compile uses (P06); `None` compiles the whole design. A plate that does not exist
    /// is refused. The plan shown so far is dropped: it belongs to the other choice.
    ChoosePlate {
        document: DocumentId,
        plate: Option<u64>,
    },
    /// An edit of a part instance in the structure panel (S07).
    Structure {
        document: DocumentId,
        edit: crate::structure_panel::StructureEdit,
    },
    /// Problem list (E09): close the gaps of problem `issue` (index into `DocumentViewModel.issues`)
    /// with the tolerance as typed. Gaps above the tolerance stay open.
    RepairIssue {
        document: DocumentId,
        issue: usize,
        tolerance: String,
    },
    /// Order list (IM31).
    AddOrder {
        order: OrderDraft,
    },
    /// Explicit choice between candidate jobs of an order.
    ChooseOrderJob {
        order: OrderId,
        plan: JobPlanId,
    },
    /// Upload of the order's job; the execution counts for this order.
    UploadForOrder {
        order: OrderId,
        action: ActionId,
        seen: Option<ExecutionState>,
    },
    /// Definitive text from the host's text input (IME commit). Composition stays in the host;
    /// one commit is one text command: it extends the selected text object or creates one.
    InsertText {
        document: DocumentId,
        text: String,
    },
    /// Operations panel: a new operation for the selection, with raw field text (DR-05).
    AddOperationText {
        document: DocumentId,
        name: String,
        kind: OperationKind,
        speed: String,
        power: String,
    },
    /// Operations panel: one field of an existing operation, raw text.
    SetOperationField {
        document: DocumentId,
        operation: OperationId,
        field: OperationField,
        text: String,
    },
    /// Colour palette (C01, LightBurn-like layers): the selection goes to the operation of
    /// colour `slot`, created as a Line layer when that colour has none yet. `add` (Shift-click)
    /// keeps the objects' other operations (a second treatment, C02); otherwise they move.
    AssignLayer {
        document: DocumentId,
        slot: u8,
        add: bool,
    },
    /// Layer list: show or hide the objects of an operation on the canvas. Window state only:
    /// no command, no undo, no effect on output (C01).
    SetOperationVisible {
        document: DocumentId,
        operation: OperationId,
        visible: bool,
    },
    /// Layer list: include the operation in compiled output or not (one `UpdateOperation`).
    SetOperationOutput {
        document: DocumentId,
        operation: OperationId,
        enabled: bool,
    },
    /// Double-click on a layer: open its Cut Settings sheet (`DocumentViewModel::cut_settings`).
    OpenCutSettings {
        document: DocumentId,
        operation: OperationId,
    },
    /// OK in the Cut Settings sheet: raw field text (DR-05). Valid input is one `UpdateOperation`
    /// (visibility is window state) and closes the sheet; invalid input keeps it open with the error.
    ApplyCutSettings {
        document: DocumentId,
        operation: OperationId,
        settings: CutSettingsInput,
    },
    CancelCutSettings {
        document: DocumentId,
    },
    /// Bitmap import sheet (P03): the resolution as typed (`254`, `300 dpi`). Window state until
    /// the import is committed.
    SetImageDpi {
        document: DocumentId,
        dpi: String,
    },
    /// Adjust Image panel: all field text as typed. Valid input starts a preview task (the older
    /// one is cancelled and its result dropped); nothing reaches the document before
    /// `CommandKey::ApplyImageAdjust`.
    SetImageAdjust {
        document: DocumentId,
        input: Box<ImageAdjustInput>,
    },
    /// File chosen for Replace Image: the pixels of the selected image operation are replaced,
    /// frame and placement stay (one history entry).
    ReplaceImage {
        document: DocumentId,
        path: PathBuf,
    },
    /// File chosen for Replace to Fit (G7-trace, T09): like `ReplaceImage`, and the frame is
    /// resized to hold the new picture in its own aspect ratio, as large as the old frame allows
    /// and in the middle of it (one history entry).
    ReplaceImageToFit {
        document: DocumentId,
        path: PathBuf,
    },
    /// Location chosen for Save Processed Bitmap of the selected image operation. Writes exactly
    /// the pixels the compiler scans; the document is untouched.
    SaveProcessedBitmap {
        document: DocumentId,
        path: PathBuf,
    },
    /// Tracer panel (T01, T02, T04): all field text as typed. Valid input starts a new trace task.
    SetTraceForm {
        document: DocumentId,
        input: Box<TraceFormInput>,
    },
    /// Tracer recipe choice (T10): replaces every recipe value but the declared resolution.
    ApplyTracePreset {
        document: DocumentId,
        preset: TracePreset,
    },
    /// Bed camera panel (IM27).
    BedCamera(BedCameraIntent),
    /// Camera picture on the bed, lens and alignment wizards, stored calibrations (W3-UI-camera).
    CameraWorkspace(crate::camera_workspace::CameraWorkspaceIntent),
    /// Preferences, shortcuts, undo history, preview window, support (W3-UI-app).
    App(AppIntent),
    /// Host timer (and crash guard): write recovery files for documents changed since the
    /// last autosave. Never writes the project file, never marks a document saved.
    Autosave,
    /// At start: look for recovery files; answered with `HostEffect::OfferRecovery`.
    CheckRecovery,
    /// Open a recovery file as a new, unsaved window.
    RestoreRecovery {
        path: PathBuf,
    },
    /// The user declined the offer: remove the recovery file.
    DiscardRecovery {
        path: PathBuf,
    },
    /// OK in the geometry dialog (join, round corners, optimize): the value as typed, with its
    /// unit (DR-05). Valid input is one document command and closes the dialog; invalid input
    /// keeps it open with the reason (W2-L1-tekenen).
    ApplyGeometryDialog {
        document: DocumentId,
        text: String,
    },
    CancelGeometryDialog {
        document: DocumentId,
    },
    /// Tool options (sides, points, ratio, radius, pattern counts, mirror copies): one option of
    /// the active tool, raw text (`1`/`0` for a toggle). The tool uses it from the next gesture.
    SetToolOption {
        document: DocumentId,
        key: ParamKey,
        text: String,
    },
    /// The shape section of the inspector: one parameter of the selected basic shape, raw text.
    /// Valid input is one `ShapeCommand::SetParams` (one undo entry); invalid input keeps the
    /// shape and shows the reason in the row.
    CommitShapeParam {
        document: DocumentId,
        key: ParamKey,
        text: String,
    },
    /// Laser Tools / Edit ▸ one of the CAM dialogs (Optimization Settings, Material / Interval /
    /// Focus Test, Device Settings): opens it with a fresh draft from the document / profile.
    OpenCamDialog {
        document: DocumentId,
        dialog: crate::CamDialog,
    },
    /// One field of an open CAM dialog, as the user typed it. Nothing is applied before Apply.
    SetCamField {
        document: DocumentId,
        dialog: crate::CamDialog,
        field: crate::DialogField,
        value: crate::DialogValue,
    },
    /// A button of an open CAM dialog (Apply, Generate, Save RD, order up/down, flat bed, Close).
    CamDialogAction {
        document: DocumentId,
        dialog: crate::CamDialog,
        action: crate::DialogAction,
    },
    /// Edit ▸ Plak with an object payload read from the OS pasteboard by the host (the
    /// controller's own copy of the last Copy is used when the host has none: `Invoke`).
    /// Start: the list the host stored under Archief ▸ Recente projecten. Not a change to persist.
    SetRecentProjects {
        paths: Vec<PathBuf>,
    },
    /// Archief ▸ Recente projecten ▸ Wis menu.
    ClearRecentProjects,
    PasteData {
        document: Option<DocumentId>,
        bytes: Vec<u8>,
        in_place: bool,
    },
    /// The 3×3 reference point of the inspector: the point of the selection's bounds that X and
    /// Y show and that stays in place when the size changes. Window state.
    SetReferencePoint {
        document: DocumentId,
        point: RefPoint,
    },
    /// Aspect lock of the size fields: a width or height commit scales both axes. Window state.
    SetAspectLock {
        document: DocumentId,
        locked: bool,
    },
    /// Schikken (W3-UI-schikken): dock, move together, reshape, slots, nesting and registration
    /// options and answers; see [`crate::SchikkenIntent`].
    Schikken(crate::schikken::SchikkenIntent),
    /// The text toolbar (W3-UI-tekst): one edit of the selected text object, one undo step.
    Text {
        document: DocumentId,
        edit: TextEdit,
    },
    /// The text-on-path dialog: choose Distort, start, offset, direction; apply or cancel.
    TextPath {
        document: DocumentId,
        edit: TextPathEdit,
    },
    /// The variable-text panel: type, serial counter, auto-advance, data rows, Test and Bake.
    Variable {
        document: DocumentId,
        edit: VariableEdit,
    },
    /// The barcode dialog: type, payload, parameters, width; Create makes the symbol.
    Barcode {
        document: DocumentId,
        edit: BarcodeEdit,
    },
    /// The generator dialog (box, living hinge): kind, fields, lid; Insert makes the generator.
    Generator {
        document: DocumentId,
        edit: crate::generator_panel::GeneratorEdit,
    },
}

/// A numeric (or on/off) setting of a basic shape or of a tool; the host shows one row per key
/// the view model lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ParamKey {
    /// Sides of a regular polygon.
    Sides,
    /// Tips of a star or double star.
    Points,
    /// Inner over outer radius of a star (0 < ratio < 1).
    Ratio,
    /// Corner radius of a rounded rectangle (mm), also the Round Corners tool's radius.
    CornerRadius,
    /// Rectangular pattern columns and rows.
    Columns,
    Rows,
    /// Circular pattern copies (including the original) and sweep in degrees.
    Count,
    Sweep,
    /// Mirror keeps the originals (`1`) or mirrors them in place (`0`).
    KeepOriginal,
    /// The dimension tool gives an ellipse its minor axis instead of its major axis (`1` / `0`).
    MinorAxis,
    /// Radius of the Fillet tool in mm (G7-editor, E05).
    FilletRadius,
    /// Distance of the Chamfer tools in mm (the first distance of the two-distance chamfer).
    ChamferDistance,
    /// Second distance of the two-distance chamfer in mm.
    ChamferDistance2,
    /// Angle of the distance-angle chamfer in degrees.
    ChamferAngle,
}

/// One of the nine reference points of a selection's bounds (the inspector's 3×3 grid).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RefPoint {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    #[default]
    BottomLeft,
    Bottom,
    BottomRight,
}

impl RefPoint {
    pub const ALL: [RefPoint; 9] = [
        RefPoint::TopLeft,
        RefPoint::Top,
        RefPoint::TopRight,
        RefPoint::Left,
        RefPoint::Center,
        RefPoint::Right,
        RefPoint::BottomLeft,
        RefPoint::Bottom,
        RefPoint::BottomRight,
    ];

    /// Horizontal position: 0 left edge, 1 centre, 2 right edge.
    pub fn column(self) -> u8 {
        match self {
            RefPoint::TopLeft | RefPoint::Left | RefPoint::BottomLeft => 0,
            RefPoint::Top | RefPoint::Center | RefPoint::Bottom => 1,
            RefPoint::TopRight | RefPoint::Right | RefPoint::BottomRight => 2,
        }
    }

    /// Vertical position: 0 top edge, 1 middle, 2 bottom edge.
    pub fn row(self) -> u8 {
        match self {
            RefPoint::TopLeft | RefPoint::Top | RefPoint::TopRight => 0,
            RefPoint::Left | RefPoint::Center | RefPoint::Right => 1,
            RefPoint::BottomLeft | RefPoint::Bottom | RefPoint::BottomRight => 2,
        }
    }
}

/// Which dimension "Maat" adds. `Auto` follows the selection: a line → its length, two points →
/// their distance, a circle → its radius, two lines → their angle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SketchDimensionKind {
    Auto,
    Length,
    Distance,
    DistanceX,
    DistanceY,
    Radius,
    Diameter,
    Angle,
    /// The full minor axis of the selected ellipse (G7-editor, S01).
    MinorDiameter,
}

/// What the Cut Settings sheet sends: field text as typed, mode and the two toggles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CutSettingsInput {
    pub mode: OperationKind,
    /// mm/s.
    pub speed: String,
    /// Percent.
    pub power_min: String,
    pub power_max: String,
    pub passes: String,
    /// Line kerf width (`0,15 mm`); ignored for other modes, empty = no kerf.
    pub kerf: String,
    pub output_enabled: bool,
    pub visible: bool,
    /// The option fields the sheet shows (tabs, leads, fill and image options); fields that are
    /// absent keep their stored value.
    pub extras: CutExtras,
}

/// Kind of operation offered by the operations panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum OperationKind {
    Line,
    Fill,
    /// Concentric offset rings (needs `Feature::OffsetFill` in the machine profile).
    OffsetFill,
    /// Image engraving (modes, dithering, screen); an image operation keeps this mode.
    Image,
    /// Shown for operations the panel cannot edit.
    Other,
}

/// Editable fields of an operation row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum OperationField {
    Name,
    /// mm/s.
    Speed,
    /// Percent of maximum power (min = max).
    Power,
    /// Cut Settings: minimum power (percent).
    PowerMin,
    /// Cut Settings: maximum power (percent).
    PowerMax,
    /// Cut Settings: whole number of passes (1–1000).
    Passes,
    /// Cut Settings: Line kerf width.
    Kerf,
}

/// Host-normalised pointer event. Position in **logical canvas points** (origin top-left, y down);
/// hosts only convert backing pixels to points (AppKit already delivers points).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewPointerEvent {
    pub pointer: PointerId,
    pub phase: PointerPhase,
    /// `None` for a hover move.
    pub button: Option<PointerButton>,
    pub position_pt: [f64; 2],
    pub modifiers: Modifiers,
    pub click_count: u8,
    pub time: MonotonicTime,
}

impl ViewPointerEvent {
    /// A primary-button mouse event without modifiers.
    pub fn mouse(phase: PointerPhase, position_pt: [f64; 2], time: MonotonicTime) -> Self {
        ViewPointerEvent {
            pointer: PointerId(0),
            phase,
            button: Some(PointerButton::Primary),
            position_pt,
            modifiers: Modifiers::default(),
            click_count: 1,
            time,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum CameraIntent {
    /// Trackpad scroll / wheel: the content follows the fingers by `delta_pt`.
    Scroll {
        delta_pt: [f64; 2],
    },
    /// Pinch (AppKit `magnification`) or Ctrl+wheel: zoom by `1 + magnification` around the anchor.
    Magnify {
        magnification: f64,
        anchor_pt: [f64; 2],
    },
    Resized {
        viewport: Viewport,
    },
}

/// Drawing unit chosen for an import without (or with a wrong) declared unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportUnit {
    Millimeter,
    Centimeter,
    Inch,
}

impl ImportUnit {
    pub fn mm_per_unit(self) -> f64 {
        match self {
            ImportUnit::Millimeter => 1.0,
            ImportUnit::Centimeter => 10.0,
            ImportUnit::Inch => 25.4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SelectMode {
    Replace,
    Toggle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum FieldKey {
    /// Left edge of the selection bounds (mm).
    X,
    /// Bottom edge of the selection bounds (mm, y up).
    Y,
    Width,
    Height,
    /// Rotation of the selection about its bounds centre, degrees counter-clockwise. The field
    /// always shows 0: a commit turns the selection by the typed angle (objects carry no angle).
    Rotation,
    /// Scale of the selection in percent of its current width (shows 100; a commit scales by it).
    ScaleX,
    /// Scale in percent of the current height; with the aspect lock on it scales both axes.
    ScaleY,
    /// Shear along x in degrees (shows 0; a commit shears by it about the reference point).
    ShearX,
    /// Shear along y in degrees.
    ShearY,
    /// Cut order priority of the selected shapes, a whole number in ±10 000 (0 = default, lower is
    /// cut first). Only reorders when `Priority` is a level of the optimization `Order by`.
    Priority,
}

/// Stable command ids shared by menu, toolbar, shortcuts and command palette (X02).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum CommandKey {
    New,
    Save,
    SaveAs,
    Close,
    Undo,
    Redo,
    Delete,
    SelectAll,
    ToolSelect,
    ToolNode,
    ToolRectangle,
    ToolEllipse,
    ToolCircle,
    ToolLine,
    ToolPolyline,
    ToolPen,
    /// Toggle object snapping (endpoint, midpoint, centre, intersection).
    SnapToObjects,
    SnapToGrid,
    AlignLeft,
    AlignRight,
    AlignTop,
    AlignBottom,
    AlignCenterX,
    AlignCenterY,
    DistributeHorizontal,
    DistributeVertical,
    BooleanUnion,
    BooleanDifference,
    BooleanIntersection,
    /// Apply the current boolean preview (one history entry).
    ApplyPreview,
    /// Compute document extents off the UI thread (first revision-checked task).
    Measure,
    /// Compile a JobPlan for the current revision and show its preview (B12).
    Compile,
    /// Write the current plan as an RD file (asks for a location first).
    ExportRd,
    /// Write the current plan as G-code for a GRBL machine (asks for a location first).
    ExportGcode,
    /// Apply the current trace result (one history entry).
    ApplyTrace,
    /// Apply the current contour trace as a *trace* of the document (T02): source, recipe and
    /// engine version are kept, the result objects stay linked to it (one history entry). When the
    /// tracer was opened with `TraceEdit`, the outputs of that trace are replaced in place.
    ApplyTraceLinked,
    /// Bitmap ▸ Bewerk trace: open the tracer on the stored source and recipe of the trace the
    /// selected object belongs to.
    TraceEdit,
    CloseTracer,
    /// Freeze the current (fresh, error-free) plan with its RD bytes as the job of this document (C1).
    PrepareJob,
    /// Commit the import preview (one history entry); disabled until the unit (or, for a
    /// bitmap, the resolution) is known. A bitmap also gets its image operation.
    CommitImport,
    /// Close the import preview; the document is unchanged.
    CancelImport,
    /// Bitmap ▸ Adjust Image: opens the panel for the selected image.
    AdjustImage,
    /// Apply the adjusted values of the open Adjust Image panel (one history entry).
    ApplyImageAdjust,
    /// Close the Adjust Image panel; the image operation is unchanged.
    CancelImageAdjust,
    /// Bitmap ▸ Mask Image: the selected closed shape(s) clip the selected image.
    MaskImage,
    /// Bitmap ▸ Replace Image… (the host asks for a file, then sends `UiIntent::ReplaceImage`);
    /// this key carries the enablement.
    ReplaceImage,
    /// Bitmap ▸ Replace to Fit… (the host asks for a file, then sends `UiIntent::ReplaceImageToFit`);
    /// this key carries the enablement.
    ReplaceImageToFit,
    /// Bitmap ▸ Refresh Image: re-read the file the image came from (one history entry when the
    /// pixels changed).
    RefreshImage,
    /// Bitmap ▸ Save Processed Bitmap… (the host asks for a file, then sends
    /// `UiIntent::SaveProcessedBitmap`); this key carries the enablement.
    SaveProcessedBitmap,
    /// Trim mode (E05): a click on a line removes the piece up to the nearest crossings.
    ToolTrim,
    /// Release the prepared job for the operator view (designer only, IM31).
    ApproveJob,
    /// Schets ▸ Nieuwe schets: an empty sketch, and sketch mode on it.
    SketchNew,
    /// Schets ▸ Maak schets van selectie: drawn lines, polylines, rectangles and circles become a sketch.
    SketchFromSelection,
    /// Schets ▸ Bewerk schets: back into sketch mode on the sketch of the selected object.
    SketchEdit,
    /// Schets ▸ Sluit schets: leave sketch mode.
    SketchFinish,
    SketchCoincident,
    SketchHorizontal,
    SketchVertical,
    SketchParallel,
    SketchPerpendicular,
    SketchEqual,
    SketchTangent,
    SketchFix,
    /// Schets ▸ Maat: a dimension on the selection with the value as drawn (then edit it).
    SketchDimension,
    /// Remove the selected constraints of the open sketch (one history entry).
    SketchRemoveConstraint,
    /// Vormen ▸ Verbinden/sluiten…: join or close the selected paths within a tolerance (dialog).
    ShapesJoin,
    /// Vormen ▸ Snijden: the last selected shape cuts the others.
    ShapesCut,
    /// Vormen ▸ Hoeken afronden…: every straight corner of the selection (dialog with the radius).
    ShapesRoundCorners,
    /// Vormen ▸ Optimaliseren…: fewer nodes within an error budget (dialog).
    ShapesOptimize,
    /// Vormen ▸ Richting omkeren, met de klok mee, tegen de klok in.
    ShapesReverse,
    /// Shapes ▸ Lock Start Point: the cut-path optimization keeps the start point of the selected
    /// shapes (a toggle: checked when every selected shape has it).
    LockPathStart,
    /// Shapes ▸ Lock Direction: the optimization keeps the direction of the selected shapes.
    LockPathDirection,
    ShapesClockwise,
    ShapesCounterClockwise,
    /// Bewerken ▸ Omzetten naar pad: a basic shape loses its parameters, the outline stays.
    ShapesConvertToPath,
    /// Knopen (Node tool): act on the selected nodes of the selected path.
    NodeDelete,
    NodeBreak,
    NodeSmooth,
    NodeSymmetric,
    NodeCorner,
    NodeToLine,
    NodeToCurve,
    NodeClosePath,
    NodeJoinEnds,
    NodeStartPoint,
    /// Edit ▸ Knip: copy the selection to the clipboard and delete it (one undo entry).
    Cut,
    /// Edit ▸ Kopieer: the selection goes to the clipboard; the document does not change.
    Copy,
    /// Edit ▸ Plak: the clipboard's objects, centred on the view (one undo entry).
    Paste,
    /// Edit ▸ Plak op plaats: the clipboard's objects at their original position.
    PasteInPlace,
    /// Edit ▸ Dupliceer: a copy of the selection at the same position; the clipboard stays as it is.
    Duplicate,
    /// Edit ▸ Verwijder dubbelen: of identical stacked shapes only the first stays.
    DeleteDuplicates,
    /// Arrange ▸ Een stap naar voren in de tekenvolgorde (render order; not the cut order).
    PushForward,
    /// Arrange ▸ Een stap naar achteren in de tekenvolgorde.
    PushBackward,
    /// Arrange ▸ Voorop in de tekenvolgorde.
    PushToFront,
    /// Arrange ▸ Achteraan in de tekenvolgorde.
    PushToBack,
    /// Arrange ▸ Groepeer de selectie.
    Group,
    /// Arrange ▸ Splits de buitenste groep van de selectie (één niveau).
    Ungroup,
    /// Arrange ▸ Groepeer vormen die in elkaar liggen (buitenomtrek met gaten).
    AutoGroup,
    /// Arrange ▸ Haal samengestelde vormen uit elkaar: één object per deelpad.
    BreakApart,
    /// Arrange ▸ Spiegel horizontaal om het midden van de selectie.
    FlipHorizontal,
    /// Arrange ▸ Spiegel verticaal om het midden van de selectie.
    FlipVertical,
    /// Arrange ▸ Draai 90° met de klok mee om het midden van de selectie.
    Rotate90Cw,
    /// Arrange ▸ Draai 90° tegen de klok in.
    Rotate90Ccw,
    /// Arrange ▸ Vergrendel de selectie: niet te kiezen op het canvas.
    LockSelected,
    /// Arrange ▸ Ontgrendel de geselecteerde vormen (kies ze in de objectenlijst).
    UnlockSelected,
    /// Arrange ▸ Ontgrendel alle vormen.
    UnlockAll,
    /// Arrange ▸ Verberg de selectie op het canvas; de vormen blijven deel van de job.
    HideSelected,
    /// Arrange ▸ Toon alle verborgen vormen.
    ShowAll,
    /// Edit ▸ Keer de selectie om (alleen kiesbare vormen).
    InvertSelection,
    /// Edit ▸ Kies open paden.
    SelectOpenShapes,
    /// Edit ▸ Kies alle vormen in de laag van de selectie.
    SelectInLayer,
    /// Edit ▸ Kies de vormen die in de selectie liggen.
    SelectContained,
    /// Edit ▸ Kies vormen kleiner dan de geselecteerde vorm.
    SelectSmaller,
    /// Edit ▸ Kies alle cirkels.
    SelectCircles,
    /// Edit ▸ Kies alle vormen van hetzelfde type als de selectie (typefilter, E03).
    SelectSameType,
    /// Arrange ▸ Isoleer selectie: verberg al het andere (één stap; Toon alles herstelt, E03).
    IsolateSelection,
    /// Weergave ▸ Zoom naar het werkblad (bed van de machine, anders alle objecten).
    ZoomPage,
    /// Weergave ▸ Zoom naar de selectie.
    ZoomSelection,
    /// Weergave ▸ Zoom in.
    ZoomIn,
    /// Weergave ▸ Zoom uit.
    ZoomOut,
    /// Weergave ▸ Draadmodel: alleen omtrekken, geen vulling (aangevinkt = draadmodel).
    ViewWireframe,
    /// Schikken ▸ Docken: the other selected objects slide against the reference (W3-UI-schikken).
    DockLeft,
    DockRight,
    DockUp,
    DockDown,
    /// Schikken ▸ Samenschuiven: close the gaps between the selected objects along one axis.
    MoveTogetherHorizontal,
    MoveTogetherVertical,
    /// Gereedschap ▸ Vervorm selectie (4 punten): the Warp tool with four corner handles.
    ToolWarp,
    /// Gereedschap ▸ Vervorm selectie (16 punten): the Deform tool with a 4 × 4 net.
    ToolDeform,
    /// Apply the handles of the Warp or Deform tool (Enter does the same).
    ReshapeApply,
    /// Put the handles of the Warp or Deform tool back on their start positions.
    ReshapeReset,
    /// Gereedschap ▸ Sleuven aanpassen…: opens the Resize Slots dialog for the selection.
    ResizeSlots,
    /// Schikken ▸ Quick Nest: pack every object that can move on the sheet (panel).
    QuickNest,
    /// Schikken ▸ Nest Selected: pack the selection, avoiding what stays (panel).
    NestSelected,
    /// Laser ▸ Print and Cut: register the design on a printed sheet (panel).
    PrintAndCut,
    /// Laser ▸ Center Finder: find the centre of a round object from points on its edge (panel).
    CenterFinder,
    /// Bewerken ▸ Tekst op pad…: the selected text follows the selected guide (dialog with Distort).
    TextOnPath,
    /// Tekenen ▸ Streepjescode…: the barcode dialog (type, payload, size, readability).
    BarcodeNew,
    /// Archief ▸ Zoek projecten…: the project search panel (P08).
    SearchProjects,
    /// Tekst ▸ Doos of scharnier…: the generator dialog (finger-jointed box, living hinge).
    GeneratorNew,
    /// Tekst ▸ Bewerk generator…: the selected generated object (box, hinge or barcode) opens
    /// again with its parameters.
    GeneratorEdit,
    /// Archief ▸ Bewaar als template…: the host asks for a file and what the template holds, then
    /// sends `FilesIntent::SaveAsTemplate`.
    SaveAsTemplate,
    /// Archief ▸ Nieuw van template…: the host asks for a template, then sends
    /// `FilesIntent::NewFromTemplate`.
    NewFromTemplate,
    /// Archief ▸ Machineprofiel openen…: the host asks for a profile file, then sends
    /// `UiIntent::OpenMachineProfile`.
    OpenMachineProfile,
    /// Archief ▸ Exporteren ▸ SVG: opens the export dialog.
    ExportSvg,
    /// Archief ▸ Exporteren ▸ DXF: opens the export dialog.
    ExportDxf,
    /// Archief ▸ Bundels ▸ Exporteren: opens the bundle dialog.
    ExportBundle,
    /// Archief ▸ Bundels ▸ Importeren: the host asks for a bundle file, then sends
    /// `FilesIntent::OpenBundleImport`.
    ImportBundle,
    /// Venster ▸ Materiaalbibliotheek: shows or hides the panel.
    ShowMaterialLibrary,
    /// Venster ▸ Kunstbibliotheek: shows or hides the panel.
    ShowArtLibrary,
    /// Bestand > Notities tonen: shows or hides the project notes (P04).
    ShowNotes,
}

impl CommandKey {
    pub const ALL: &'static [CommandKey] = &[
        CommandKey::New,
        CommandKey::Save,
        CommandKey::SaveAs,
        CommandKey::Close,
        CommandKey::Undo,
        CommandKey::Redo,
        CommandKey::Delete,
        CommandKey::SelectAll,
        CommandKey::ToolSelect,
        CommandKey::ToolNode,
        CommandKey::ToolRectangle,
        CommandKey::ToolEllipse,
        CommandKey::ToolCircle,
        CommandKey::ToolLine,
        CommandKey::ToolPolyline,
        CommandKey::ToolPen,
        CommandKey::SnapToObjects,
        CommandKey::SnapToGrid,
        CommandKey::AlignLeft,
        CommandKey::AlignRight,
        CommandKey::AlignTop,
        CommandKey::AlignBottom,
        CommandKey::AlignCenterX,
        CommandKey::AlignCenterY,
        CommandKey::DistributeHorizontal,
        CommandKey::DistributeVertical,
        CommandKey::BooleanUnion,
        CommandKey::BooleanDifference,
        CommandKey::BooleanIntersection,
        CommandKey::ApplyPreview,
        CommandKey::Measure,
        CommandKey::Compile,
        CommandKey::ExportRd,
        CommandKey::ExportGcode,
        CommandKey::ApplyTrace,
        CommandKey::ApplyTraceLinked,
        CommandKey::TraceEdit,
        CommandKey::CloseTracer,
        CommandKey::PrepareJob,
        CommandKey::ApproveJob,
        CommandKey::ToolTrim,
        CommandKey::CommitImport,
        CommandKey::CancelImport,
        CommandKey::AdjustImage,
        CommandKey::ApplyImageAdjust,
        CommandKey::CancelImageAdjust,
        CommandKey::MaskImage,
        CommandKey::ReplaceImage,
        CommandKey::ReplaceImageToFit,
        CommandKey::RefreshImage,
        CommandKey::SaveProcessedBitmap,
        CommandKey::SketchNew,
        CommandKey::SketchFromSelection,
        CommandKey::SketchEdit,
        CommandKey::SketchFinish,
        CommandKey::SketchCoincident,
        CommandKey::SketchHorizontal,
        CommandKey::SketchVertical,
        CommandKey::SketchParallel,
        CommandKey::SketchPerpendicular,
        CommandKey::SketchEqual,
        CommandKey::SketchTangent,
        CommandKey::SketchFix,
        CommandKey::SketchDimension,
        CommandKey::SketchRemoveConstraint,
        CommandKey::ShapesJoin,
        CommandKey::ShapesCut,
        CommandKey::ShapesRoundCorners,
        CommandKey::ShapesOptimize,
        CommandKey::ShapesReverse,
        CommandKey::LockPathStart,
        CommandKey::LockPathDirection,
        CommandKey::ShapesClockwise,
        CommandKey::ShapesCounterClockwise,
        CommandKey::ShapesConvertToPath,
        CommandKey::NodeDelete,
        CommandKey::NodeBreak,
        CommandKey::NodeSmooth,
        CommandKey::NodeSymmetric,
        CommandKey::NodeCorner,
        CommandKey::NodeToLine,
        CommandKey::NodeToCurve,
        CommandKey::NodeClosePath,
        CommandKey::NodeJoinEnds,
        CommandKey::NodeStartPoint,
        CommandKey::Cut,
        CommandKey::Copy,
        CommandKey::Paste,
        CommandKey::PasteInPlace,
        CommandKey::Duplicate,
        CommandKey::DeleteDuplicates,
        CommandKey::PushForward,
        CommandKey::PushBackward,
        CommandKey::PushToFront,
        CommandKey::PushToBack,
        CommandKey::Group,
        CommandKey::Ungroup,
        CommandKey::AutoGroup,
        CommandKey::BreakApart,
        CommandKey::FlipHorizontal,
        CommandKey::FlipVertical,
        CommandKey::Rotate90Cw,
        CommandKey::Rotate90Ccw,
        CommandKey::LockSelected,
        CommandKey::UnlockSelected,
        CommandKey::UnlockAll,
        CommandKey::HideSelected,
        CommandKey::ShowAll,
        CommandKey::InvertSelection,
        CommandKey::SelectOpenShapes,
        CommandKey::SelectInLayer,
        CommandKey::SelectContained,
        CommandKey::SelectSmaller,
        CommandKey::SelectCircles,
        CommandKey::SelectSameType,
        CommandKey::IsolateSelection,
        CommandKey::ZoomPage,
        CommandKey::ZoomSelection,
        CommandKey::ZoomIn,
        CommandKey::ZoomOut,
        CommandKey::ViewWireframe,
        CommandKey::DockLeft,
        CommandKey::DockRight,
        CommandKey::DockUp,
        CommandKey::DockDown,
        CommandKey::MoveTogetherHorizontal,
        CommandKey::MoveTogetherVertical,
        CommandKey::ToolWarp,
        CommandKey::ToolDeform,
        CommandKey::ReshapeApply,
        CommandKey::ReshapeReset,
        CommandKey::ResizeSlots,
        CommandKey::QuickNest,
        CommandKey::NestSelected,
        CommandKey::PrintAndCut,
        CommandKey::CenterFinder,
        CommandKey::TextOnPath,
        CommandKey::BarcodeNew,
        CommandKey::SearchProjects,
        CommandKey::GeneratorNew,
        CommandKey::GeneratorEdit,
        CommandKey::SaveAsTemplate,
        CommandKey::NewFromTemplate,
        CommandKey::OpenMachineProfile,
        CommandKey::ExportSvg,
        CommandKey::ExportDxf,
        CommandKey::ExportBundle,
        CommandKey::ImportBundle,
        CommandKey::ShowMaterialLibrary,
        CommandKey::ShowArtLibrary,
        CommandKey::ShowNotes,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DialogKind {
    SavePanel,
    CloseConfirmation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CloseChoice {
    Save,
    Discard,
    Cancel,
}
