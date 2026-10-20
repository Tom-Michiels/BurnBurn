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
}

impl CamDialog {
    pub const ALL: [CamDialog; 5] = [
        CamDialog::Optimization,
        CamDialog::MaterialTest,
        CamDialog::IntervalTest,
        CamDialog::FocusTest,
        CamDialog::Device,
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

impl DialogField {
    /// The entries of a pop-up field (empty for the others).
    pub fn choices(self) -> &'static [DialogChoice] {
        match self {
            DialogField::CellMode => &CELL_MODES,
            DialogField::ColumnParameter | DialogField::RowParameter => &PARAMETERS,
            DialogField::BedOrigin => &ORIGINS,
            DialogField::RotaryKind => &ROTARY_KINDS,
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
            | DialogField::RotaryMirror => DialogControl::Flag,
            DialogField::CellMode
            | DialogField::ColumnParameter
            | DialogField::RowParameter
            | DialogField::BedOrigin
            | DialogField::RotaryKind => DialogControl::Choice,
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
