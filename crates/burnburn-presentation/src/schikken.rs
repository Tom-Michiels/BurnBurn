//! Schikken (W3-UI-schikken): the plain-data types of Dock, Move Together, Warp/Deform, Resize
//! Slots, Tangent Circle, Print and Cut, Center Finder and Nesting. One intent
//! ([`SchikkenIntent`]), one view-model section ([`SchikkenViewModel`]) and one message family
//! ([`SchikkenMessage`]), so the shared files only get one additive line each.

use burnburn_geometry::DockContact;
use burnburn_types::GeometryId;

/// Which selected object a dock slides against (it stays; the others move).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DockReference {
    /// The first selected object (LightBurn's rule, shown as the align reference).
    #[default]
    FirstSelected,
    LastSelected,
    /// A chosen member of the selection.
    Object(GeometryId),
}

/// Which side Move Together closes towards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TogetherAnchorChoice {
    /// The lowest / leftmost object stays.
    #[default]
    Start,
    /// The highest / rightmost object stays.
    End,
    Object(GeometryId),
}

/// The options of Dock and Move Together, as the panel shows them. The gap is raw text (DR-05).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceOptionsInput {
    pub reference: DockReference,
    pub contact: DockContactChoice,
    /// Clearance left between the objects, mm as typed (`0`, `2`, `1,5 mm`).
    pub gap: String,
    /// Objects of one group move as one body.
    pub by_group: bool,
    pub anchor: TogetherAnchorChoice,
}

impl Default for PlaceOptionsInput {
    fn default() -> Self {
        PlaceOptionsInput {
            reference: DockReference::default(),
            contact: DockContactChoice::default(),
            gap: "0".to_owned(),
            by_group: false,
            anchor: TogetherAnchorChoice::default(),
        }
    }
}

/// What counts as the boundary of the objects when docking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DockContactChoice {
    /// Bounding boxes touch.
    #[default]
    Bounds,
    /// The outlines touch: a notch can be entered.
    Outline,
}

impl DockContactChoice {
    pub(crate) fn contact(self) -> DockContact {
        match self {
            DockContactChoice::Bounds => DockContact::Bounds,
            DockContactChoice::Outline => DockContact::Geometry,
        }
    }
}

/// Why Dock or Move Together did nothing. The document is unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlaceFailure {
    /// Nothing of the reference lies ahead in that direction.
    NoContact,
    /// A moving object already overlaps the reference.
    Overlapping,
    /// The reference chosen is not part of the selection.
    ReferenceNotSelected,
    /// Fewer objects than the command needs.
    TooFewObjects,
    /// An object has no geometry.
    EmptyGeometry,
    /// Everything stays where it is already.
    NothingToMove,
    /// Any other refusal of the command.
    Refused,
}

/// Which features Resize Slots touches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SlotChoice {
    /// Notches cut into the material and rectangular holes.
    #[default]
    Slots,
    /// Protrusions of the material.
    Tabs,
    Both,
}

/// The fields of the Resize Slots dialog, as typed (DR-05).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotsInput {
    /// The width of the features to find (mm).
    pub from: String,
    /// The width they get (mm).
    pub to: String,
    /// A feature matches when its width is this close to `from` (mm).
    pub tolerance: String,
    pub select: SlotChoice,
}

impl Default for SlotsInput {
    fn default() -> Self {
        SlotsInput {
            from: String::new(),
            to: String::new(),
            tolerance: "0,01".to_owned(),
            select: SlotChoice::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlotField {
    From,
    To,
    Tolerance,
}

/// Why Resize Slots did nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlotsFailure {
    /// The text of this field is not a valid value.
    InvalidField(SlotField),
    /// No feature of the chosen kind has the width to find.
    NoSlotsFound,
    /// The new width would make a neighbouring edge collapse or reverse.
    Collision,
    /// Nothing would change.
    NothingChanged,
    Refused,
}

/// The open Resize Slots dialog: what was typed and why the last OK did nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotsDialogViewModel {
    pub input: SlotsInput,
    pub error: Option<SchikkenMessage>,
}

/// What a nest works on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum NestScope {
    /// Every object of the drawing that can move.
    #[default]
    Quick,
    /// The selected objects only; the others stay and are avoided.
    Selected,
}

/// How many turns a part may take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum NestRotations {
    /// Parts keep their orientation.
    Fixed,
    /// 0° and 180°.
    Half,
    /// Quarter turns.
    #[default]
    Quarter,
    /// Steps of 45°.
    Eighth,
}

impl NestRotations {
    pub(crate) fn degrees(self) -> Vec<f64> {
        match self {
            NestRotations::Fixed => vec![0.0],
            NestRotations::Half => vec![0.0, 180.0],
            NestRotations::Quarter => vec![0.0, 90.0, 180.0, 270.0],
            NestRotations::Eighth => (0..8).map(|k| f64::from(k) * 45.0).collect(),
        }
    }
}

/// The fields of the nest panel, as typed (DR-05).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NestInput {
    /// Minimum distance between parts (mm).
    pub padding: String,
    /// Minimum distance to the edge of the sheet and to what stays (mm).
    pub edge_buffer: String,
    /// Grid step of the search (mm); finer packs tighter and takes longer.
    pub step: String,
    pub rotations: NestRotations,
    /// Whether a part may be placed as its mirror image (C12). Forbidden unless chosen.
    pub mirror: NestMirror,
    /// A closed object of the drawing that is the sheet (a remnant): the parts go inside its contour
    /// and the other objects that stay are its exclusion zones. `None`: the machine bed (W07).
    pub sheet_object: Option<u64>,
    /// How many identical sheets the parts may go on (whole number, at least 1).
    pub sheets: String,
    /// How many copies of every part (whole number, 1 to 1000) unless the part has its own (C12).
    pub copies: String,
    /// The parts that have their own number of copies: the first object of the part and the text.
    pub part_copies: Vec<(u64, String)>,
    /// Whether an object inside another one is a hole of that part (C10).
    pub holes: NestHoles,
    /// Slide every placement against its neighbours with exact geometry (slower, packs tighter).
    pub tight: bool,
}

/// What an object that lies inside another object is for the nest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum NestHoles {
    /// A hole of that part: it moves with it, and the space is free for other parts.
    #[default]
    Join,
    /// A part of its own.
    Separate,
}

/// Whether the nest may mirror a part. Mirroring changes what a cut part is, so it is a choice that
/// is off by default and never used to get a better score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum NestMirror {
    #[default]
    Forbidden,
    Allowed,
}

impl Default for NestInput {
    fn default() -> Self {
        NestInput {
            padding: "1".to_owned(),
            edge_buffer: "1".to_owned(),
            step: "1".to_owned(),
            rotations: NestRotations::default(),
            mirror: NestMirror::default(),
            sheet_object: None,
            sheets: "1".to_owned(),
            copies: "1".to_owned(),
            part_copies: Vec::new(),
            holes: NestHoles::default(),
            tight: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NestField {
    Padding,
    EdgeBuffer,
    Step,
    Sheets,
    /// The number of copies of a part.
    Copies,
}

/// Why a nest did not run or gave nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NestFailure {
    InvalidField(NestField),
    /// No object can be nested (none selected, all open or locked).
    NothingToNest,
    /// The raster of the sheet would be too large for this step: use a coarser step.
    GridTooLarge,
    /// A part or the sheet could not be processed.
    Geometry,
    /// The result no longer fits the document: it changed while or after nesting.
    Stale,
    /// The chosen sheet is one of the objects being nested.
    SheetIsPart,
    /// The chosen sheet is not in the drawing any more, or is not a closed contour.
    SheetGone,
    Refused,
}

/// Where the nest stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NestRun {
    Idle,
    /// A cancellable task is running.
    Running,
    /// A result for the current document is ready to apply.
    Done,
    /// The document changed after the result was computed: nest again.
    Stale,
    /// The result was applied.
    Applied,
}

/// The sheet the parts go on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NestSheetView {
    pub width_mm: f64,
    pub height_mm: f64,
    /// The machine's bed (otherwise the bounds of what is being nested).
    pub is_bed: bool,
    /// The object whose contour is the sheet (a remnant), when one is chosen (W07).
    pub contour: Option<u64>,
}

/// A closed object that may be the sheet of a nest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NestSheetChoice {
    pub object: u64,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NestUnplacedReason {
    /// No free position on the sheet.
    NoFit,
    /// No allowed rotation fits the grain rule.
    NoAllowedRotation,
    /// The search budget ran out.
    Budget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NestSkipReason {
    /// The outline is not closed: nothing to pack.
    OpenContour,
    /// The object has no geometry.
    NoGeometry,
    /// Generated geometry (sketch, text, array output) moves with its definition.
    Derived,
}

/// A part that could not be placed (one row per part; a group is one part).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NestUnplacedRow {
    pub objects: Vec<burnburn_types::GeometryId>,
    pub reason: NestUnplacedReason,
    /// How many copies of the part were not placed.
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NestSkippedRow {
    pub object: burnburn_types::GeometryId,
    pub reason: NestSkipReason,
}

/// What the finished nest says (C10): what was placed, what was not and why.
#[derive(Debug, Clone, PartialEq)]
pub struct NestReportViewModel {
    pub placed: usize,
    pub unplaced: Vec<NestUnplacedRow>,
    pub skipped: Vec<NestSkippedRow>,
    /// Share of the sheet the placed parts cover, 0…1.
    pub utilisation: f64,
    /// Parts placed as their mirror image (always 0 unless mirroring was allowed).
    pub mirrored: usize,
    /// Per sheet: how much is used and how much is left (C12).
    pub sheets: Vec<NestSheetUseRow>,
    /// Per part: ordered and placed (C12). One row per part; a group is one part.
    pub parts: Vec<NestPartStatusRow>,
    /// Complete sets placed: a set is one of every part.
    pub complete_sets: u32,
}

/// What is used and left of one sheet, in mm².
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NestSheetUseRow {
    pub net_mm2: f64,
    pub placed_mm2: f64,
    pub free_mm2: f64,
}

/// How much of one part's order was placed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NestPartStatusRow {
    pub objects: Vec<burnburn_types::GeometryId>,
    pub ordered: u32,
    pub placed: u32,
}

/// One part of the nest as the panel lists it, with the number of copies it will get (C12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NestPartRow {
    /// The first object of the part (its key in `NestInput::part_copies`).
    pub object: u64,
    pub name: String,
    /// The number of copies as text: its own, or the one of the panel.
    pub copies: String,
    /// The part has its own number.
    pub own: bool,
}

/// The nest panel (Quick Nest, Nest Selected) while it is open.
#[derive(Debug, Clone, PartialEq)]
pub struct NestPanelViewModel {
    pub scope: NestScope,
    pub input: NestInput,
    /// Why the last Start did nothing.
    pub error: Option<SchikkenMessage>,
    pub run: NestRun,
    /// Objects that would be nested as parts, and the sheet they would go on.
    pub parts: usize,
    pub sheet: Option<NestSheetView>,
    /// The closed objects that may be the sheet instead of the bed (W07).
    pub sheet_choices: Vec<NestSheetChoice>,
    /// The parts with the number of copies each will get (C12).
    pub part_rows: Vec<NestPartRow>,
    pub report: Option<NestReportViewModel>,
    /// Apply is available: a fresh result with at least one placement.
    pub can_apply: bool,
}

/// Whether Print and Cut may change the size of the design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PrintCutScale {
    /// No scaling: the scale is exactly 1; disagreement shows up as residuals.
    #[default]
    Fixed,
    /// A uniform scale within `1 ± max deviation`; anything else is refused.
    Uniform,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegistrationField {
    X,
    Y,
    MaxDeviation,
}

/// Why Print and Cut or the Center Finder did nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegistrationFailure {
    InvalidField(RegistrationField),
    /// Not both design targets are set.
    TargetNotSet,
    /// Not both targets have been measured.
    NotMeasured,
    /// The head position is not (fully) known; it is never taken as 0.
    PositionUnknown,
    NoMachine,
    NoMachineProfile,
    /// The design targets lie too close together to determine a rotation.
    DesignTargetsTooClose,
    /// The measured targets lie too close together.
    MeasuredTargetsTooClose,
    /// The best uniform scale is outside the allowed deviation.
    ScaleOutOfRange,
    /// Print and Cut works with Absolute Coordinates: another start would move the registered job.
    NeedsAbsoluteCoordinates,
    /// Any other refusal of the registration.
    Refused,
}

/// A position in mm, document (bed view) coordinates.
pub type PointMm = (f64, f64);

/// What a finished registration says.
#[derive(Debug, Clone, PartialEq)]
pub struct RegistrationResultView {
    /// Counter-clockwise rotation of the design on the bed.
    pub rotation_deg: f64,
    pub scale: f64,
    /// Distance between each transformed design target and where it was measured (mm).
    pub residuals_mm: Vec<f64>,
    pub rms_mm: f64,
}

/// The Print and Cut panel while it is open.
#[derive(Debug, Clone, PartialEq)]
pub struct PrintAndCutViewModel {
    /// The two design targets (document coordinates).
    pub targets: [Option<PointMm>; 2],
    /// Where the targets were found on the bed.
    pub measured: [Option<PointMm>; 2],
    /// A canvas click sets this design target.
    pub picking: Option<usize>,
    pub scale: PrintCutScale,
    /// The allowed deviation of the scale (`0,05` = 5 %), shown with `Uniform`.
    pub max_deviation: String,
    /// Present while the registration is in force: the next compile uses it.
    pub result: Option<RegistrationResultView>,
    pub error: Option<SchikkenMessage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CenterProblem {
    /// Fewer than three distinct points.
    Coincident,
    /// The points lie on one line: there is no circle.
    Collinear,
}

/// A fitted circle with its misfit.
#[derive(Debug, Clone, PartialEq)]
pub struct CenterFitView {
    pub center: PointMm,
    /// The centre in the machine frame, for "go to centre".
    pub center_machine: Option<PointMm>,
    pub radius_mm: f64,
    /// Per point: distance from the centre minus the radius (outside is positive).
    pub residuals_mm: Vec<f64>,
    pub rms_mm: f64,
    pub max_residual_mm: f64,
    /// The part of the circle the points cover (radians).
    pub arc_span_rad: f64,
    /// The points cover too little of the circle: the centre is badly determined.
    pub poorly_determined: bool,
}

/// The Center Finder panel while it is open.
#[derive(Debug, Clone, PartialEq)]
pub struct CenterFinderViewModel {
    pub points: Vec<PointMm>,
    /// How many more points a circle needs.
    pub needed: usize,
    /// A canvas click adds a point.
    pub picking: bool,
    pub fit: Option<CenterFitView>,
    pub problem: Option<CenterProblem>,
    pub error: Option<SchikkenMessage>,
}

/// The arc a Center Finder fit must span (radians) to count as well determined.
pub const POOR_ARC_SPAN_RAD: f64 = 0.5;

/// Print and Cut actions. `index` is the design target, 0 or 1.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum PrintAndCutIntent {
    /// The next canvas click (snapped to the geometry) sets this design target.
    PickTarget {
        document: burnburn_types::DocumentId,
        index: usize,
    },
    /// A design target as typed coordinates (mm).
    SetTarget {
        document: burnburn_types::DocumentId,
        index: usize,
        x: String,
        y: String,
    },
    /// Where target `index` was found: the laser's position. Refused while it is unknown.
    MeasureTarget {
        document: burnburn_types::DocumentId,
        index: usize,
    },
    /// Where target `index` was found, typed (bed view, mm).
    SetMeasured {
        document: burnburn_types::DocumentId,
        index: usize,
        x: String,
        y: String,
    },
    SetScale {
        document: burnburn_types::DocumentId,
        scale: PrintCutScale,
        max_deviation: String,
    },
    /// Compute the registration and put it in force.
    Register {
        document: burnburn_types::DocumentId,
    },
    /// Forget targets, measurements and the registration; the next compile is unregistered.
    Reset {
        document: burnburn_types::DocumentId,
    },
    Close {
        document: burnburn_types::DocumentId,
    },
}

/// Center Finder actions.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum CenterFinderIntent {
    /// Canvas clicks add points until the finder is closed or Escape is pressed.
    PickPoints {
        document: burnburn_types::DocumentId,
    },
    AddPoint {
        document: burnburn_types::DocumentId,
        x: String,
        y: String,
    },
    /// The laser's position as an edge point. Refused while it is unknown.
    MeasurePoint {
        document: burnburn_types::DocumentId,
    },
    UndoLast {
        document: burnburn_types::DocumentId,
    },
    Clear {
        document: burnburn_types::DocumentId,
    },
    Close {
        document: burnburn_types::DocumentId,
    },
}

/// Why Tangent Circle has no solution to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TangentProblem {
    /// Two targets are the same point, line or circle.
    Coincident,
    /// Infinitely many circles qualify (for example the diameter equals the width of a strip).
    Degenerate,
    /// No circle is tangent to all targets.
    NoSolution,
    Other,
}

/// The Tangent Circle tool while it has picked targets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TangentViewModel {
    /// Targets picked so far, of at most three.
    pub picked: usize,
    /// Solutions drawn.
    pub candidates: usize,
    /// A click would create the solution nearest to the pointer.
    pub chosen: bool,
    /// The typed diameter of a two-target circle, as a radius.
    pub radius_mm: Option<f64>,
    pub problem: Option<TangentProblem>,
}

/// Messages of this module's flows (localised by the host).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SchikkenMessage {
    /// Print and Cut or the Center Finder did nothing.
    Registration(RegistrationFailure),
    /// A nest did not run or gave nothing.
    Nest(NestFailure),
    /// Resize Slots refused.
    Slots(SlotsFailure),
    /// Dock refused; says why.
    Dock(PlaceFailure),
    /// Move Together refused.
    MoveTogether(PlaceFailure),
    /// The gap field holds no valid, non-negative length.
    InvalidGap,
}

/// The state of the Dock / Move Together options for the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceOptionsViewModel {
    pub input: PlaceOptionsInput,
    /// Why the last options input was refused (the previous options stay in force).
    pub error: Option<SchikkenMessage>,
}

/// Why the handles of Warp or Deform cannot be applied as they are.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReshapeProblem {
    /// The quadrilateral is not strictly convex.
    NotConvex,
    /// The corners enclose no area.
    Degenerate,
    /// The control net folds the plane over itself.
    Folded,
    /// Any other refusal of the mapping.
    Other,
}

/// The Warp / Deform session of a window, while one of those tools is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReshapeViewModel {
    /// Warp only: bilinear instead of projective interpolation.
    pub bilinear: bool,
    /// Some handle is off its start position: Apply and Reset are available.
    pub modified: bool,
    pub problem: Option<ReshapeProblem>,
    /// The selection is too large for a live preview: only the frame is drawn.
    pub simplified: bool,
}

/// Everything this module adds to a document window's view model.
#[derive(Debug, Clone, PartialEq)]
pub struct SchikkenViewModel {
    pub place: PlaceOptionsViewModel,
    /// `Some` while the Warp or Deform tool is active on a usable selection.
    pub reshape: Option<ReshapeViewModel>,
    /// The Resize Slots dialog while it is open.
    pub slots: Option<SlotsDialogViewModel>,
    /// The nest panel while it is open.
    pub nest: Option<NestPanelViewModel>,
    /// The Print and Cut panel while it is open.
    pub print_and_cut: Option<PrintAndCutViewModel>,
    /// The Center Finder panel while it is open.
    pub center_finder: Option<CenterFinderViewModel>,
    /// The Tangent Circle tool while it has picked targets.
    pub tangent: Option<TangentViewModel>,
}

/// A user action of this module (everything but the command keys, which carry no payload).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum SchikkenIntent {
    /// The Dock / Move Together options panel. Invalid text keeps the old options and says why.
    SetPlaceOptions {
        document: burnburn_types::DocumentId,
        options: PlaceOptionsInput,
    },
    /// Warp: bilinear (`true`) or projective interpolation of the four corners.
    SetWarpKind {
        document: burnburn_types::DocumentId,
        bilinear: bool,
    },
    /// OK in the Resize Slots dialog. Valid input is one document command and closes the dialog;
    /// anything else keeps it open with the reason.
    ApplySlots {
        document: burnburn_types::DocumentId,
        input: SlotsInput,
    },
    CancelSlots {
        document: burnburn_types::DocumentId,
    },
    /// The options of the nest panel as typed; invalid text keeps the old options and says why.
    /// A result of other options is dropped.
    SetNestOptions {
        document: burnburn_types::DocumentId,
        input: NestInput,
    },
    /// Start the nest as a cancellable task (an older one is cancelled).
    StartNest {
        document: burnburn_types::DocumentId,
    },
    /// Stop the running nest; no result is delivered.
    CancelNest {
        document: burnburn_types::DocumentId,
    },
    /// Move the parts to the placements of the finished nest (one undo entry).
    ApplyNest {
        document: burnburn_types::DocumentId,
    },
    /// Select the objects the finished nest could not place.
    SelectNestUnplaced {
        document: burnburn_types::DocumentId,
    },
    CloseNest {
        document: burnburn_types::DocumentId,
    },
    PrintAndCut(PrintAndCutIntent),
    CenterFinder(CenterFinderIntent),
}

impl SchikkenIntent {
    /// The window the intent is for.
    pub fn document(&self) -> burnburn_types::DocumentId {
        match self {
            SchikkenIntent::SetPlaceOptions { document, .. }
            | SchikkenIntent::SetWarpKind { document, .. }
            | SchikkenIntent::ApplySlots { document, .. }
            | SchikkenIntent::CancelSlots { document }
            | SchikkenIntent::SetNestOptions { document, .. }
            | SchikkenIntent::StartNest { document }
            | SchikkenIntent::CancelNest { document }
            | SchikkenIntent::ApplyNest { document }
            | SchikkenIntent::SelectNestUnplaced { document }
            | SchikkenIntent::CloseNest { document } => *document,
            SchikkenIntent::PrintAndCut(i) => match i {
                PrintAndCutIntent::PickTarget { document, .. }
                | PrintAndCutIntent::SetTarget { document, .. }
                | PrintAndCutIntent::MeasureTarget { document, .. }
                | PrintAndCutIntent::SetMeasured { document, .. }
                | PrintAndCutIntent::SetScale { document, .. }
                | PrintAndCutIntent::Register { document }
                | PrintAndCutIntent::Reset { document }
                | PrintAndCutIntent::Close { document } => *document,
            },
            SchikkenIntent::CenterFinder(i) => match i {
                CenterFinderIntent::PickPoints { document }
                | CenterFinderIntent::AddPoint { document, .. }
                | CenterFinderIntent::MeasurePoint { document }
                | CenterFinderIntent::UndoLast { document }
                | CenterFinderIntent::Clear { document }
                | CenterFinderIntent::Close { document } => *document,
            },
        }
    }
}
