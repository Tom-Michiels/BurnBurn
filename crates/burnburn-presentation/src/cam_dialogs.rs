//! The CAM dialogs of the Mac window (W3-UI-cam): Optimization Settings, Material / Interval /
//! Focus Test and Device Settings (machine profile with Rotary Setup).
//!
//! All five are *forms*: a list of fields that travel as the text the user typed (DR-05), flags
//! and pop-up choices, a list of actions, and what the controller wants said in words (notes,
//! row errors, the reason a field is greyed). A host builds one generic panel from a
//! [`DialogViewModel`] and sends three intents back: open, set a field, press an action. The
//! controller owns the draft; a dialog never changes the document or the machine profile before
//! its Apply, and a test dialog never changes the document at all (the test job is private).

use std::path::PathBuf;
use std::sync::Arc;

use burnburn_document::process::OrderKey;
use burnburn_job::{Feature, JobPlanId, PlanPreview};

use crate::cut_options::FieldState;
use crate::view_model::MessageKey;

/// One of the CAM dialogs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CamDialog {
    Optimization,
    MaterialTest,
    IntervalTest,
    FocusTest,
    Device,
    /// Arrange ▸ Array: grid, circular and along-a-path arrays with a live preview (AR02).
    Array,
    /// Laser ▸ Feeder: repeat, feed before and after, pause (FE01).
    Feeder,
    /// Laser ▸ Preflight: the findings of the plan on screen, with "show source" (C09).
    Preflight,
    /// Edit ▸ Cutting order: hard "this layer completely before that one" constraints (C06).
    Order,
    /// Edit ▸ Sub-layers: the sub-layers of a layer and the order they run in (C11).
    Sublayers,
}

impl CamDialog {
    /// The order is the order of the command ids of the hosts: new dialogs are appended.
    pub const ALL: [CamDialog; 10] = [
        CamDialog::Optimization,
        CamDialog::MaterialTest,
        CamDialog::IntervalTest,
        CamDialog::FocusTest,
        CamDialog::Device,
        CamDialog::Array,
        CamDialog::Feeder,
        CamDialog::Preflight,
        CamDialog::Order,
        CamDialog::Sublayers,
    ];

    /// The three generators; their result is a private test job.
    pub fn is_test(self) -> bool {
        matches!(self, CamDialog::MaterialTest | CamDialog::IntervalTest | CamDialog::FocusTest)
    }
}

/// A field of one of the dialogs. Which fields a dialog shows is in its [`DialogViewModel`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum DialogField {
    // Optimization Settings
    /// Whether `Order by` contains this level (the order of the levels is a separate action).
    OrderEnabled(OrderKey),
    InnerFirst,
    ReduceTravel,
    BestStart,
    BestDirection,
    HideBacklash,
    RemoveOverlapping,
    /// Overlap tolerance in mm.
    OverlapTolerance,
    // Material / Interval / Focus Test
    CellMode,
    ColumnParameter,
    ColumnMin,
    ColumnMax,
    ColumnCount,
    RowsEnabled,
    RowParameter,
    RowMin,
    RowMax,
    RowCount,
    BaseSpeed,
    BasePower,
    BasePasses,
    FillInterval,
    FillOverscan,
    CellWidth,
    CellHeight,
    CellGap,
    LabelsEnabled,
    LabelHeight,
    BorderEnabled,
    BorderMargin,
    /// Interval test: `0,1 mm` or `254 lpi`.
    IntervalMin,
    IntervalMax,
    IntervalCount,
    // Focus Test
    FocusStart,
    FocusStep,
    FocusCount,
    LineLength,
    LineSpacing,
    // Device Settings
    BedWidth,
    BedHeight,
    BedOrigin,
    MaxSpeed,
    TravelSpeed,
    MinPower,
    MaxPower,
    RotaryEnabled,
    RotaryKind,
    /// Object diameter of a chuck, roller diameter of a roller (mm).
    RotaryDiameter,
    StepsPerRotation,
    YStepsPerMm,
    RotaryMirror,
    // Device Settings: backend and the GRBL fields (G5-grbl-ui)
    /// Which controller dialect the profile compiles for (Ruida RD or GRBL G-code).
    Backend,
    /// The controller model the user says this machine has (M01). Declared, never read from a
    /// controller reply; it is what the identity check before an upload compares with.
    Model,
    /// The firmware version the user says the controller runs (M01); declared like `Model`.
    Firmware,
    /// `$30`: the spindle value that means 100 % power.
    GrblMaxSpindle,
    /// The machine runs with laser mode (`$32=1`); without it GRBL output is refused.
    GrblLaserMode,
    /// `M3` constant or `M4` dynamic power.
    GrblPowerMode,
    /// The machine has an air-assist output on `M8`/`M9`.
    GrblAirAssist,
    /// Work coordinate system selected in the program header (S70).
    GrblWorkCoordinate,
    /// Decimals of the coordinates (1 to 4).
    GrblDecimals,
    // Array
    /// Grid, circular or along a path ([`DialogChoice::ArrayGrid`] …).
    ArrayKind,
    GridColumns,
    GridRows,
    GridPitchX,
    GridPitchY,
    CircularCount,
    CircularCentreX,
    CircularCentreY,
    /// Degrees; 360 spaces the copies over the full circle.
    CircularSweep,
    CircularRotate,
    PathCount,
    /// The object to follow: a pop-up whose entries are the row's `labels`.
    PathAlong,
    PathOrient,
    // Feeder
    FeederRepeat,
    /// Material advance before each run, mm.
    FeederFeedBefore,
    FeederFeedAfter,
    /// Seconds between two runs.
    FeederPause,
    /// Feed speed, mm/s.
    FeederSpeed,
    // Preflight
    /// The finding the "show source" button works on: a pop-up whose entries are the row's labels.
    PreflightPick,
    // Cutting order
    OrderBefore,
    OrderAfter,
    /// The stored constraints, to pick one to remove.
    OrderExisting,
    // Sub-layers
    /// The layer whose sub-layers are edited.
    SubLayer,
    /// The sub-layer the fields below show.
    SubIndex,
    SubMode,
    SubSpeed,
    /// Power in percent (a sub-layer has one power, minimum = maximum).
    SubPower,
    SubPasses,
    /// Fill line interval in mm (Fill sub-layers only).
    SubInterval,
}

/// An entry of a pop-up field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum DialogChoice {
    CellLine,
    CellFill,
    ParamSpeed,
    ParamPower,
    ParamPasses,
    ParamInterval,
    ParamZOffset,
    OriginBottomLeft,
    OriginTopLeft,
    OriginTopRight,
    OriginBottomRight,
    RotaryChuck,
    RotaryRoller,
    BackendRuida,
    BackendGrbl,
    PowerConstantM3,
    PowerDynamicM4,
    /// Leave the controller's own work coordinate system.
    WorkCoordNone,
    WorkCoordG54,
    WorkCoordG55,
    WorkCoordG56,
    WorkCoordG57,
    WorkCoordG58,
    WorkCoordG59,
    ArrayGrid,
    ArrayCircular,
    ArrayPath,
    SubLine,
    SubFill,
}

const CELL_MODES: [DialogChoice; 2] = [DialogChoice::CellLine, DialogChoice::CellFill];
const PARAMETERS: [DialogChoice; 5] = [
    DialogChoice::ParamSpeed,
    DialogChoice::ParamPower,
    DialogChoice::ParamPasses,
    DialogChoice::ParamInterval,
    DialogChoice::ParamZOffset,
];
const ORIGINS: [DialogChoice; 4] = [
    DialogChoice::OriginBottomLeft,
    DialogChoice::OriginTopLeft,
    DialogChoice::OriginTopRight,
    DialogChoice::OriginBottomRight,
];
const ROTARY_KINDS: [DialogChoice; 2] = [DialogChoice::RotaryChuck, DialogChoice::RotaryRoller];
const BACKENDS: [DialogChoice; 2] = [DialogChoice::BackendRuida, DialogChoice::BackendGrbl];
const POWER_MODES: [DialogChoice; 2] = [DialogChoice::PowerConstantM3, DialogChoice::PowerDynamicM4];
const WORK_COORDINATES: [DialogChoice; 7] = [
    DialogChoice::WorkCoordNone,
    DialogChoice::WorkCoordG54,
    DialogChoice::WorkCoordG55,
    DialogChoice::WorkCoordG56,
    DialogChoice::WorkCoordG57,
    DialogChoice::WorkCoordG58,
    DialogChoice::WorkCoordG59,
];
const SUB_MODES: [DialogChoice; 2] = [DialogChoice::SubLine, DialogChoice::SubFill];
const ARRAY_KINDS: [DialogChoice; 3] = [DialogChoice::ArrayGrid, DialogChoice::ArrayCircular, DialogChoice::ArrayPath];

impl DialogField {
    /// The entries of a pop-up field (empty for the others).
    pub fn choices(self) -> &'static [DialogChoice] {
        match self {
            DialogField::CellMode => &CELL_MODES,
            DialogField::ColumnParameter | DialogField::RowParameter => &PARAMETERS,
            DialogField::BedOrigin => &ORIGINS,
            DialogField::RotaryKind => &ROTARY_KINDS,
            DialogField::Backend => &BACKENDS,
            DialogField::GrblPowerMode => &POWER_MODES,
            DialogField::GrblWorkCoordinate => &WORK_COORDINATES,
            DialogField::ArrayKind => &ARRAY_KINDS,
            DialogField::SubMode => &SUB_MODES,
            _ => &[],
        }
    }

    /// The kind of control this field needs.
    pub fn control(self) -> DialogControl {
        match self {
            DialogField::OrderEnabled(_)
            | DialogField::InnerFirst
            | DialogField::ReduceTravel
            | DialogField::BestStart
            | DialogField::BestDirection
            | DialogField::HideBacklash
            | DialogField::RemoveOverlapping
            | DialogField::RowsEnabled
            | DialogField::LabelsEnabled
            | DialogField::BorderEnabled
            | DialogField::RotaryEnabled
            | DialogField::GrblLaserMode
            | DialogField::GrblAirAssist
            | DialogField::RotaryMirror
            | DialogField::CircularRotate
            | DialogField::PathOrient => DialogControl::Flag,
            DialogField::CellMode
            | DialogField::ColumnParameter
            | DialogField::RowParameter
            | DialogField::BedOrigin
            | DialogField::Backend
            | DialogField::GrblPowerMode
            | DialogField::GrblWorkCoordinate
            | DialogField::RotaryKind
            | DialogField::ArrayKind
            | DialogField::PathAlong
            | DialogField::PreflightPick
            | DialogField::OrderBefore
            | DialogField::OrderAfter
            | DialogField::OrderExisting
            | DialogField::SubLayer
            | DialogField::SubIndex
            | DialogField::SubMode => DialogControl::Choice,
            _ => DialogControl::Text,
        }
    }
}

/// The kind of control a field needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogControl {
    Text,
    Flag,
    /// A pop-up over [`DialogField::choices`].
    Choice,
}

/// The value of one field, as the dialog holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogValue {
    Text(String),
    Flag(bool),
    /// Index into [`DialogField::choices`].
    Choice(usize),
}

impl DialogValue {
    pub fn text(&self) -> &str {
        match self {
            DialogValue::Text(t) => t,
            _ => "",
        }
    }

    pub fn flag(&self) -> bool {
        matches!(self, DialogValue::Flag(true))
    }

    /// The selected entry of `field`'s pop-up.
    pub fn choice(&self, field: DialogField) -> Option<DialogChoice> {
        match self {
            DialogValue::Choice(i) => field.choices().get(*i).copied(),
            _ => None,
        }
    }
}

/// Why a field's text cannot be used. Shown next to the field; nothing is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DialogError {
    /// Not a number (or not a length).
    Invalid,
    OutOfRange,
    /// A value that must be whole (passes, counts).
    NotWhole,
    /// More values than an axis allows, or none.
    Count,
    /// The parameter does not apply to the cell mode (interval on a line cell).
    ParameterNotForMode,
    /// Columns and rows vary the same parameter.
    SameParameter,
    /// A size is not positive.
    Size,
    /// Rows × columns is more than one test may have.
    TooManyCells,
    /// The machine profile does not support the feature the field needs.
    Unsupported(Feature),
    /// An array of zero copies, or of more than the document allows.
    TooManyCopies,
    /// Array along a path: there is no object to follow.
    NoPath,
}

/// What a dialog says in words besides its fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DialogNote {
    /// Optimization: cut priorities only reorder when `Priority` is one of the levels.
    PriorityNeedsOrderLevel,
    /// Tests need a machine profile to be compiled for.
    NoMachineProfile,
    /// Focus Test: the profile has no Z axis.
    FocusNeedsZ,
    /// A test job is not part of the project; nothing in the document changes.
    TestIsSeparate,
    /// Device: no profile yet; Apply creates the first one.
    NewProfile,
    /// Device: the Ruida dialect of this profile is not proven on a real machine.
    DialectUnverified,
    /// Device: the GRBL backend is modelled and tested against a fake controller only; nothing
    /// is proven on a real one (G5-grbl-ui).
    GrblUnverified,
    /// Rotary: the Y scale (steps per rotation) has not been confirmed on a machine.
    RotaryUnconfirmed,
    /// The CLI's profile file knows no rotary; it is not saved there.
    RotaryNotInCliProfile,
    /// Applying a profile makes compiled plans stale.
    ProfileChangeMakesPlansStale,
    /// The compile of the test failed (other than a field error).
    TestCompileFailed,
    /// The test job was lowered and written; the file is the one the preview shows.
    TestSaved,
    /// RD export of this plan is refused, with the reason.
    RdRefused(RdRefusal),
    /// Writing the file failed.
    ExportFailed,
    /// Array: the dialog works on exactly one selected object.
    ArrayNeedsOneObject,
    /// Array: what the draft makes, in whole micrometres (copies include the original).
    ArrayExtent {
        copies: u32,
        width_um: u64,
        height_um: u64,
    },
    /// Feeder: the plan has feeds or pauses, which have no proven RD form: preview only.
    FeederHasNoRdForm,
    /// Preflight: there is no plan yet; compile first.
    PreflightNoPlan,
    /// Preflight: the plan is out of date; the list describes the plan, not the document.
    PreflightStale,
    PreflightCounts {
        errors: u32,
        warnings: u32,
    },
    PreflightClean,
    /// What one finding says (the label of an entry of the list).
    Finding(FindingKind),
    /// Order: the new constraint would close a cycle through the layers listed under the form.
    OrderCycle,
    /// Order: a layer cannot come before itself.
    OrderSameLayer,
    /// Order: this constraint exists already.
    OrderDuplicate,
    /// Sub-layers: the layer has none yet.
    SublayersNone,
    /// Sub-layers: the most a layer can have are there.
    SublayersFull,
}

/// The kind of a preflight finding, without the identities (the entry's text names the object).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FindingKind {
    OutsideBed,
    TravelOutsideBed,
    OpenFillContour,
    SpeedAboveMachineMax,
    PowerOutsideMachineRange,
    DuplicateCut,
    EmptyOperation,
    UnassignedGeometry,
    StalePlan,
    UnsupportedFeature,
    InvalidTabs,
    SharedBoundaryCutOnce,
}

/// One entry of a document-dependent pop-up: an optional localised phrase and the data (names).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogLabel {
    pub note: Option<DialogNote>,
    pub text: String,
}

impl DialogLabel {
    pub fn plain(text: impl Into<String>) -> Self {
        DialogLabel { note: None, text: text.into() }
    }
}

/// Why a plan cannot be written as RD (the Ruida backend has no proven form for it). Said in
/// words next to Save RD and Send, never replaced by something else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RdRefusal {
    /// Cross-hatch scans along Y.
    CrossHatch,
    DotMode,
    RampLength,
    /// Focus (Z) offsets between passes or per operation.
    FocusOffset,
    ScanAngle,
    GrayscaleModulation,
    /// Everything else the backend cannot represent.
    Other,
    /// More settings blocks than the file format allows.
    TooManyParts,
    /// A negative machine coordinate (not confirmed for RD).
    NegativeCoordinate,
    /// Feeder feeds and pauses.
    Feeder,
}

impl RdRefusal {
    /// The refusal that the Ruida backend's `LowerError` names.
    pub fn of(what: &str) -> RdRefusal {
        match what {
            "scan axis Y" => RdRefusal::CrossHatch,
            "dot mode" => RdRefusal::DotMode,
            "ramp length" => RdRefusal::RampLength,
            "focus offset" => RdRefusal::FocusOffset,
            "scan angle" => RdRefusal::ScanAngle,
            "grayscale modulation" => RdRefusal::GrayscaleModulation,
            "feeder" => RdRefusal::Feeder,
            _ => RdRefusal::Other,
        }
    }
}

/// One field of a dialog with how to present it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogRow {
    pub field: DialogField,
    pub value: DialogValue,
    /// Hidden for another mode, greyed (with the reason if there is one) or editable.
    pub state: FieldState,
    pub error: Option<DialogError>,
    /// Pop-up entries that cannot be chosen, and why.
    pub blocked_choices: Vec<(DialogChoice, MessageKey)>,
    /// Optimization order rows: whether the level can move up / down.
    pub reorder: Option<(bool, bool)>,
    /// A pop-up whose entries depend on the document (the objects an array can follow): their
    /// titles. Empty: the entries are [`DialogField::choices`].
    pub labels: Vec<DialogLabel>,
}

/// What an action button does.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DialogAction {
    /// Optimization / Device: apply the draft (one undo step / one new profile revision).
    Apply,
    /// Close the dialog and drop the draft.
    Close,
    /// Optimization: move this `Order by` level up or down.
    MoveUp(OrderKey),
    MoveDown(OrderKey),
    /// Tests: generate and compile the test job (cancels an earlier one).
    Generate,
    /// Tests: write the generated test as RD (never into the project).
    ExportRd(PathBuf),
    /// Device: back to the flat bed (the profile as before the rotary).
    ReturnToFlatbed,
    /// Preflight: select the cause of the chosen finding and centre the view on it.
    ShowSource,
    /// Order: add the chosen constraint.
    AddRow,
    /// Order: remove the chosen constraint.
    RemoveRow,
    /// Sub-layers: move the chosen sub-layer earlier / later in the list.
    MoveRowUp,
    MoveRowDown,
}

/// A button of the dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogActionState {
    pub action: DialogActionKind,
    pub enabled: bool,
    /// Why it is greyed.
    pub reason: Option<DialogNote>,
}

/// The kind of a button (the path of Save RD comes from the host's save panel).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DialogActionKind {
    Apply,
    Close,
    Generate,
    ExportRd,
    ReturnToFlatbed,
    ShowSource,
    AddRow,
    RemoveRow,
    MoveRowUp,
    MoveRowDown,
}

/// How a machine profile supports one feature, for the capability list of Device Settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupportView {
    Supported,
    Unsupported,
    /// Not proven for this machine; the domain's reason (English) travels along.
    Unverified(String),
}

/// One row of the capability list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityRow {
    pub feature: Feature,
    pub support: SupportView,
}

/// The generated test, as the dialog shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct TestResultView {
    pub plan: JobPlanId,
    pub cells: usize,
    pub preview: Arc<PlanPreview>,
    pub errors: usize,
    pub warnings: usize,
    /// The machine profile changed since the test was compiled.
    pub stale: bool,
    /// Where it was written as RD, once it was.
    pub saved_to: Option<PathBuf>,
}

/// Everything a host needs to draw one dialog.
#[derive(Debug, Clone, PartialEq)]
pub struct DialogViewModel {
    pub dialog: CamDialog,
    pub rows: Vec<DialogRow>,
    pub notes: Vec<DialogNote>,
    pub actions: Vec<DialogActionState>,
    /// Device Settings: the feature matrix the compiler reads.
    pub capabilities: Vec<CapabilityRow>,
    pub result: Option<TestResultView>,
    /// A generate is running.
    pub busy: bool,
    /// Lines of the user's own data under the form (names of the layers on a cycle).
    pub lines: Vec<String>,
}

impl DialogViewModel {
    pub fn row(&self, field: DialogField) -> Option<&DialogRow> {
        self.rows.iter().find(|r| r.field == field)
    }

    pub fn action(&self, kind: DialogActionKind) -> Option<&DialogActionState> {
        self.actions.iter().find(|a| a.action == kind)
    }

    /// The text of a text field (empty when absent).
    pub fn text(&self, field: DialogField) -> &str {
        self.row(field).map_or("", |r| r.value.text())
    }

    pub fn flag(&self, field: DialogField) -> bool {
        self.row(field).is_some_and(|r| r.value.flag())
    }

    pub fn choice(&self, field: DialogField) -> Option<DialogChoice> {
        self.row(field).and_then(|r| r.value.choice(field))
    }

    /// The `Order by` levels in the draft's order, with whether each is in the list.
    pub fn order(&self) -> Vec<(OrderKey, bool)> {
        self.rows
            .iter()
            .filter_map(|r| match r.field {
                DialogField::OrderEnabled(key) => Some((key, r.value.flag())),
                _ => None,
            })
            .collect()
    }
}
