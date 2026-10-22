//! The laser window (W2-L1-laser): plain-data types shared by the controller, the tasks and the hosts.
//!
//! The window is a view over the machine service: the position (unknown is never 0), the controls
//! with their enablement and the typed reason a control is off, the job setup that feeds the
//! compile (start from, 3×3 job origin, cut selected, selection origin) and the controller files.
//! Nothing here talks to a machine; every action is a [`crate::UiIntent::Laser`] that the
//! controller turns into a [`crate::LaserIntent`] for the machine service.

use std::fmt;
use std::sync::Arc;

use burnburn_cam::{CompileError, CompileOptions, PreflightReport, RasterSource, compile_with, preflight_with};
use burnburn_document::DocumentSnapshot;
use burnburn_job::{JobPlan, MachineFrame, MachinePoint, MachineProfile};
use burnburn_machine::{
    ControllerFileName, FrameShape, JobAnchor, JogKey, LaserAction, LaserRefusal, LiveError, LiveFeature,
    PlacementError, PlacementSpec, Rect, ScopeError, StartFrom, place_plan,
};
use burnburn_types::{CancellationToken, GeometryId};

use crate::production::ActionId;
use crate::task::TaskError;

/// A view direction for a bounded jog step (right and up positive on the bed as the user sees it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StepDirection {
    Left,
    Right,
    Up,
    Down,
}

/// What a laser-window control asks for. Numbers arrive as typed (DR-05), parsed here.
#[derive(Debug, Clone, PartialEq)]
pub enum LaserCommand {
    /// A domain action (jog press/release, home, pause, resume, position, go to origin, files).
    Action(LaserAction),
    /// "Ga naar": numeric absolute move, X and Y in millimetres of the machine frame.
    MoveToText { x: String, y: String },
    /// A bounded step in a view direction, as long as the step size in the setup.
    Step(StepDirection),
    /// A laser-off frame run of the reviewed plan.
    Frame(FrameShape),
    /// "Save RD naar controller" under this name. Not a start; refused until proven.
    SaveRd { name: String },
}

/// The laser window's setup: what the next compile and the next actions use.
#[derive(Debug, Clone, PartialEq)]
pub enum LaserSetup {
    StartFrom(StartFrom),
    /// The 3×3 job origin: a point on the bounds of the job as the user sees it.
    Anchor(JobAnchor),
    CutSelected(bool),
    UseSelectionOrigin(bool),
    /// Step size of the jog buttons, millimetres as typed.
    StepMm(String),
    /// Speed of the laser-off frame run, mm/s as typed.
    FrameSpeed(String),
    /// While on, a canvas click moves the laser there instead of selecting.
    PositionLaserMode(bool),
    /// "Oorsprong hier instellen": only from a fully known position.
    SetOriginHere,
    ClearOrigin,
}

/// Why a laser-window control is off or an action was refused. Typed, so every host can say it in
/// words; nothing is ever shown as a success that was not proven.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LaserWhy {
    /// No machine service is attached (offline editor).
    NoMachine,
    NoMachineProfile,
    /// Not proven on this controller; `needs` names the hardware evidence that is missing.
    NotVerified {
        feature: LiveFeature,
        needs: &'static str,
    },
    Unsupported(LiveFeature),
    /// The capability is claimed but this build has no wire form for it.
    NoWireForm(LiveFeature),
    /// A job may be running: the request would fight it.
    JobMayBeActive,
    RequestInFlight,
    JogKeyHeld(JogKey),
    /// The transport could not carry the request.
    PortFailed,
    /// The journal could not record the intent: nothing was sent.
    NotRecorded,
    /// The head position is not (fully) known; it is never taken as 0.
    PositionUnknown,
    UserOriginNotSet,
    OutsideBed,
    InvalidCoordinate,
    InvalidNumber,
    InvalidFileName,
    /// There is no reviewed plan for this revision to frame or place.
    NoReviewedPlan,
    /// "Cut selected" or "use selection origin" with an empty selection.
    NothingSelected,
    /// Nothing to burn in the document.
    NoObjects,
    /// The placed job would leave the bed.
    PlacementOutsideBed,
    /// The plan was compiled with another setup, or on a head position that has moved since:
    /// compile again.
    PlacementStale,
    /// The action is not allowed for this role or on this port.
    NotAllowed,
    Refused,
}

impl From<&LiveError> for LaserWhy {
    fn from(e: &LiveError) -> Self {
        match e {
            LiveError::Unsupported { feature } => LaserWhy::Unsupported(*feature),
            LiveError::NotVerified { feature, needs, .. } => LaserWhy::NotVerified { feature: *feature, needs },
            LiveError::NoWireForm { feature } => LaserWhy::NoWireForm(*feature),
            LiveError::JobMayBeActive(_) => LaserWhy::JobMayBeActive,
            LiveError::RequestInFlight => LaserWhy::RequestInFlight,
            LiveError::JogKeyHeld(k) => LaserWhy::JogKeyHeld(*k),
            LiveError::Port(_) => LaserWhy::PortFailed,
        }
    }
}

impl From<&LaserRefusal> for LaserWhy {
    fn from(r: &LaserRefusal) -> Self {
        match r {
            LaserRefusal::PositionUnknown => LaserWhy::PositionUnknown,
            LaserRefusal::UserOriginNotSet => LaserWhy::UserOriginNotSet,
            LaserRefusal::OutsideBed { .. } => LaserWhy::OutsideBed,
            LaserRefusal::InvalidCoordinate => LaserWhy::InvalidCoordinate,
        }
    }
}

impl From<&PlacementError> for LaserWhy {
    fn from(e: &PlacementError) -> Self {
        match e {
            PlacementError::PositionUnknown => LaserWhy::PositionUnknown,
            PlacementError::UserOriginNotSet => LaserWhy::UserOriginNotSet,
            PlacementError::OutsideBed { .. } => LaserWhy::PlacementOutsideBed,
            _ => LaserWhy::Refused,
        }
    }
}

impl From<&ScopeError> for LaserWhy {
    fn from(e: &ScopeError) -> Self {
        match e {
            ScopeError::NothingSelected | ScopeError::UnknownObject(_) => LaserWhy::NothingSelected,
            ScopeError::NoObjects => LaserWhy::NoObjects,
        }
    }
}

/// Which control of the laser window. Hosts show them in their own layout.
#[derive(Debug, Clone, PartialEq)]
pub enum LaserControlKind {
    /// Press and hold: the head moves until the release. Never from a key repeat.
    Jog(JogKey),
    Step(StepDirection),
    MoveTo,
    /// Toggle: the next canvas click moves the laser there.
    PositionLaser,
    Home,
    GoToMachineOrigin,
    GoToUserOrigin,
    /// Local: stores the known position as the user origin. No machine contact.
    SetOrigin,
    Pause,
    Resume,
    ShowPosition,
    Frame(FrameShape),
    SaveRd,
    ListFiles,
    FileMetadata(ControllerFileName),
    RunFile(ControllerFileName),
    DeleteFile(ControllerFileName),
}

/// One control with its idempotency key and whether it can be used right now.
#[derive(Debug, Clone, PartialEq)]
pub struct LaserControl {
    pub kind: LaserControlKind,
    /// Copy into `UiIntent::Laser`. `None` for local controls (set origin, position-laser mode).
    pub action: Option<ActionId>,
    pub enabled: bool,
    /// Why the control is off.
    pub why_not: Option<LaserWhy>,
}

/// The head position as the user may read it: unknown axes are `None`, never 0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LaserPosition {
    pub x_mm: Option<f64>,
    pub y_mm: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LaserSetupView {
    pub start_from: StartFrom,
    pub anchor: JobAnchor,
    pub cut_selected: bool,
    pub use_selection_origin: bool,
    /// Machine-frame millimetres; `None` is "not set", never (0, 0).
    pub user_origin_mm: Option<(f64, f64)>,
    pub step_mm: f64,
    pub frame_speed_mm_s: f64,
    pub position_laser_mode: bool,
}

/// A file on the controller (DSP storage), apart from local projects. `None` size is "onbekend".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControllerFileRow {
    pub name: ControllerFileName,
    pub size_bytes: Option<u64>,
}

/// Where the compiled plan of the active window was put by the setup (start from / job origin).
#[derive(Debug, Clone, PartialEq)]
pub struct PlacementView {
    pub start_from: StartFrom,
    pub anchor: JobAnchor,
    /// The shift applied to the job, bed-view axes are the machine frame's (millimetres).
    pub delta_mm: (f64, f64),
    /// The head position the plan was placed on, if it was placed on the position.
    pub from_position_mm: Option<(f64, f64)>,
    /// `true` when the setup changed since this plan was compiled: compile again.
    pub setup_changed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LaserViewModel {
    pub position: LaserPosition,
    /// The held jog key (or the one whose release is unconfirmed).
    pub jog_held: Option<JogKey>,
    /// The key-up's key and idempotency key; `Some` only while a key is held.
    pub jog_release: Option<(JogKey, ActionId)>,
    pub setup: LaserSetupView,
    pub controls: Vec<LaserControl>,
    /// The last answered file list.
    pub files: Option<Vec<ControllerFileRow>>,
    pub placement: Option<PlacementView>,
    /// Why the last laser-window action was refused.
    pub last_refusal: Option<LaserWhy>,
    /// The execution state these controls were offered in; copy into `UiIntent::Laser`.
    pub seen: Option<burnburn_machine::ExecutionState>,
}

impl LaserViewModel {
    pub fn control(&self, kind: &LaserControlKind) -> Option<&LaserControl> {
        self.controls.iter().find(|c| &c.kind == kind)
    }

    /// True when an enabled control of this kind would act.
    pub fn offers(&self, kind: &LaserControlKind) -> bool {
        self.control(kind).is_some_and(|c| c.enabled)
    }
}

/// Compile-time input of the laser window: filters and origin are composed before any bounds are
/// used, then the plan is put where the setup says (`burnburn_machine::{resolve_scope, place_plan}`).
#[derive(Debug, Clone)]
pub struct LaserCompile {
    /// Objects to compile ("Cut Selected Graphics"); `None` compiles everything.
    pub included: Option<Vec<GeometryId>>,
    /// Bed-view bounds the 3×3 job origin refers to (the selection's with "Use Selection Origin").
    pub origin_bounds: Rect,
    pub spec: PlacementSpec,
    pub frame: MachineFrame,
    /// The head position in machine-frame micrometres when it is fully known.
    pub current_position_um: Option<(i64, i64)>,
    pub user_origin_um: Option<(i64, i64)>,
}

/// What the placement did to a compiled plan.
#[derive(Debug, Clone, PartialEq)]
pub struct AppliedPlacement {
    pub spec: PlacementSpec,
    pub delta_mm: (f64, f64),
    pub from_position_um: Option<(i64, i64)>,
    /// Identifies the setup the plan was compiled with (compared with the current setup).
    pub setup_key: u64,
}

impl fmt::Display for LaserWhy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

fn um_to_point((x, y): (i64, i64)) -> Result<MachinePoint, TaskError> {
    MachinePoint::new(x as f64 / 1000.0, y as f64 / 1000.0)
        .map_err(|_| TaskError::Placement(LaserWhy::InvalidCoordinate))
}

/// The compile of the laser window: restrict to the included objects, compile, put the plan where
/// the setup says, and derive preview input from the placed plan. Runs on a worker.
pub(crate) fn compile_placed(
    snapshot: &Arc<DocumentSnapshot>,
    profile: &MachineProfile,
    options: &CompileOptions,
    rasters: &dyn RasterSource,
    laser: &LaserCompile,
    setup_key: u64,
    cancel: &CancellationToken,
) -> Result<(JobPlan, PreflightReport, AppliedPlacement), TaskError> {
    let scoped = match &laser.included {
        Some(ids) => Arc::new(snapshot.restricted_to(ids)),
        None => Arc::clone(snapshot),
    };
    let plan = compile_with(&scoped, profile, options, rasters, cancel).map_err(|e| match e {
        CompileError::Cancelled => TaskError::Cancelled,
        e => TaskError::Compile(e),
    })?;
    cancel.check().map_err(|_| TaskError::Cancelled)?;
    let current_position = laser.current_position_um.map(um_to_point).transpose()?;
    let user_origin = laser.user_origin_um.map(um_to_point).transpose()?;
    let input = burnburn_machine::PlacementInput { frame: &laser.frame, current_position, user_origin };
    let placed = place_plan(&plan, &laser.spec, laser.origin_bounds, &input)
        .map_err(|e| TaskError::Placement(LaserWhy::from(&e)))?;
    cancel.check().map_err(|_| TaskError::Cancelled)?;
    let report = preflight_with(&scoped, &placed.plan, profile, rasters);
    let applied = AppliedPlacement {
        spec: laser.spec,
        delta_mm: placed.delta_mm,
        from_position_um: match laser.spec.start_from {
            StartFrom::CurrentPosition => laser.current_position_um,
            _ => None,
        },
        setup_key,
    };
    Ok((placed.plan, report, applied))
}
