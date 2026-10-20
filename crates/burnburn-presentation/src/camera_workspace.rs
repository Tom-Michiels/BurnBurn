//! The camera workspace (W3-UI-camera): plain-data intents, typed refusals and view models for the
//! camera picture on the bed, the lens-calibration wizard, the workspace-alignment wizard and the
//! management of stored calibrations. Nothing here touches a camera, a machine or a document; the
//! controller (`controller/camera*.rs`) interprets the intents.
//!
//! Camera pixels are a visual placement aid. Nothing in this module is production geometry, and
//! no intent turns a picture into a job; the only job that leaves from here burns alignment marks
//! after an explicit confirmation of exactly that job.

use burnburn_image::calib::{
    AlignError, CameraConfigKey, CornerError, Invalidity, LensCalibError, MarkRole, StoreMiss,
};
use burnburn_job::JobPlanId;
use burnburn_machine::ExecutionState;

use crate::camera::{CameraDeviceId, CameraMode, CameraPixelFormat};
use crate::production::ActionId;
use crate::view_model::MessageKey;

/// Which calibration of the store a list row stands for.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CalibrationRef {
    pub config: CameraConfigKey,
    pub kind: CalibrationKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CalibrationKind {
    Lens,
    /// An alignment at one material height, in whole micrometres (the store's key).
    Alignment {
        height_um: i64,
    },
}

/// What the camera panels ask for (`UiIntent::CameraWorkspace`). Numbers arrive as typed (DR-05).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum CameraWorkspaceIntent {
    // --- the picture on the bed ---------------------------------------------------------------
    SetOverlayVisible(bool),
    /// 0 (invisible) to 100 (opaque).
    SetOverlayOpacity {
        percent: f64,
    },
    /// "Update overlay": freeze the latest frame of every running camera as the picture on the bed.
    UpdateOverlay,
    /// Height of the material surface above the bed that the picture is placed for.
    SetMaterialHeight {
        mm: String,
    },
    /// Pixels at the edge of a camera's image that are not trusted where cameras overlap.
    SetEdgeMargin {
        px: String,
    },
    // --- several cameras, one bed ---------------------------------------------------------------
    /// Start `device` in `mode` next to the cameras already running. `lens` names the lens/mounting
    /// profile (a different one is a different configuration).
    UseCamera {
        device: CameraDeviceId,
        mode: CameraMode,
        lens: String,
    },
    DropCamera {
        device: CameraDeviceId,
    },
    SetLensProfile {
        device: CameraDeviceId,
        lens: String,
    },
    // --- lens calibration -------------------------------------------------------------------------
    StartLensCalibration {
        device: CameraDeviceId,
        cols: u32,
        rows: u32,
        pitch_mm: String,
    },
    /// Photograph the pattern now: the latest frame goes to the corner detector.
    AddPatternView,
    RemovePatternView {
        index: usize,
    },
    FitLens {
        two_coefficients: bool,
    },
    AcceptLensCalibration,
    CancelLensCalibration,
    // --- workspace alignment -----------------------------------------------------------------------
    StartAlignment {
        device: CameraDeviceId,
        material_height_mm: String,
    },
    SetAlignmentMark {
        index: usize,
        x_mm: String,
        y_mm: String,
    },
    /// Prepare the job that burns the marks. Nothing is sent: the proposal shows what would burn.
    ProposeMarksJob {
        power_percent: String,
        speed_mm_s: String,
    },
    /// Send exactly the proposed job to the machine (upload only; starting stays in the machine
    /// panel). `action` and `seen` are copied from the proposal.
    ConfirmMarksJob {
        plan: JobPlanId,
        action: ActionId,
        seen: Option<ExecutionState>,
    },
    /// Freeze the latest frame: the photo the marks are clicked in.
    CaptureAlignmentPhoto,
    PlaceAlignmentMark {
        index: usize,
        pixel: [f64; 2],
    },
    ClearAlignmentMark {
        index: usize,
    },
    FitAlignment,
    SaveAlignment,
    CancelAlignment,
    // --- stored calibrations -------------------------------------------------------------------------
    DeleteCalibration {
        calibration: CalibrationRef,
    },
    /// Start the wizard that makes this calibration again (the camera must run in its configuration).
    Recalibrate {
        calibration: CalibrationRef,
    },
}

/// Why a camera-workspace control is off or an action was refused. Typed, so every host can say it
/// in words; nothing is shown as a success that was not proven.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum CameraWhy {
    NoCameraPort,
    /// The OS has not granted camera access; the prompt appears only on the access button.
    CameraNotAuthorized,
    /// The camera refused to start in that mode.
    CameraRefused,
    /// The camera is not running (or was never started in this configuration).
    CameraNotRunning,
    /// The running camera has not delivered a frame yet.
    NoFrame,
    /// The frame is not in the configuration the wizard was started with.
    FrameConfigChanged,
    InvalidNumber,
    /// Update overlay with no running camera.
    NothingToUpdate,
    /// The store has nothing usable for this camera in its current configuration.
    NotCalibrated(StoreMiss),
    /// A pixel the stored alignments cannot project onto the bed.
    Unmappable,
    NoLensWizard,
    NoAlignmentWizard,
    /// A detection or a fit is still running.
    Busy,
    Corners(CornerError),
    Lens(LensCalibError),
    Align(AlignError),
    NoPatternViews,
    /// The lens fit is not finished (or failed).
    NoLensResult,
    BadGrid,
    MarkIndex,
    /// A clicked pixel outside the photo.
    OutsideImage,
    NoPhoto,
    /// Fewer fit or check marks with a clicked position than the fit needs.
    MarksIncomplete,
    NoAlignmentResult,
    // --- the job that burns the marks ---
    NoMachine,
    NoMachineProfile,
    /// The marks job could not be built or lowered to RD.
    MarksJobFailed,
    /// Marks (or their arms) would leave the bed.
    MarksOutsideBed,
    SpeedTooHigh,
    /// Confirming a job that is not the proposal on screen.
    NotProposed,
    NotAllowedForRole,
    /// The machine service refused (not verified, busy, state changed, not recorded).
    MachineRefused(MachineWhy),
}

/// What the machine service said when it refused to take the marks job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MachineWhy {
    /// The controller identity is not configured, so a reply could never be checked.
    IdentityNotConfigured,
    Busy,
    StateChanged,
    NotRecorded,
    WrongMachine,
    Other,
}

// ---------------------------------------------------------------------------------------------
// View models
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct CameraWorkspaceView {
    pub overlay: OverlayView,
    pub material_height_mm: f64,
    /// Cameras running in the bed system.
    pub cameras: Vec<CameraInUse>,
    /// Every stored calibration, with whether it still applies.
    pub calibrations: Vec<CalibrationRow>,
    pub lens: Option<LensWizardView>,
    pub alignment: Option<AlignmentView>,
    /// A corner detection or lens fit is running.
    pub busy: bool,
    pub last_refusal: Option<CameraWhy>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OverlayView {
    pub visible: bool,
    pub opacity_percent: f64,
    pub edge_margin_px: f64,
    pub layers: Vec<OverlayLayerView>,
    pub can_update: bool,
    pub update_why_not: Option<CameraWhy>,
    /// Always "visuele plaatsing": the picture never feeds a job.
    pub note: MessageKey,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OverlayLayerView {
    pub device: CameraDeviceId,
    /// Sequence number of the frame the layer shows.
    pub captured_seq: u64,
    /// Mesh cells that land on the bed (cells where another camera sees better are not counted).
    pub cells: usize,
    /// `None`: placed on the bed; otherwise why this camera's picture is not drawn.
    pub not_placed: Option<CameraWhy>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CameraInUse {
    pub device: CameraDeviceId,
    pub name: String,
    pub mode: CameraMode,
    pub lens: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CalibrationRow {
    pub reference: CalibrationRef,
    pub device: CameraDeviceId,
    pub width: u32,
    pub height: u32,
    pub frame_rate_mhz: u32,
    pub lens: String,
    pub validity: CalibrationValidity,
    pub summary: CalibrationSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalibrationValidity {
    /// The camera runs in exactly this configuration.
    Valid,
    /// The camera runs in another configuration; the first difference.
    Invalid(Invalidity),
    /// The camera is not running, so there is nothing to compare with.
    CameraNotInUse,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CalibrationSummary {
    Lens { k1: f64, k2: f64, rms_px: f64 },
    Alignment { material_height_mm: f64, check_rms_mm: f64, check_max_mm: f64 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct LensWizardView {
    pub device: CameraDeviceId,
    pub cols: u32,
    pub rows: u32,
    pub pitch_mm: f64,
    /// Corners found in each photo.
    pub views: Vec<usize>,
    pub detecting: bool,
    pub fitting: bool,
    pub result: Option<LensResultView>,
    pub error: Option<CameraWhy>,
    pub can_add_view: bool,
    pub can_fit: bool,
    pub can_accept: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LensResultView {
    pub k1: f64,
    pub k2: f64,
    pub rms_px: f64,
    pub max_px: f64,
    pub views: usize,
    pub points: usize,
    pub converged: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlignmentView {
    pub device: CameraDeviceId,
    pub material_height_mm: f64,
    pub marks: Vec<MarkView>,
    /// Size of the photo the marks are clicked in.
    pub photo: Option<(u32, u32)>,
    /// The fit uses the stored lens calibration of this configuration.
    pub uses_lens_calibration: bool,
    pub proposal: Option<MarksProposalView>,
    pub result: Option<AlignmentResultView>,
    pub error: Option<CameraWhy>,
    pub can_propose: bool,
    pub can_capture: bool,
    pub can_fit: bool,
    pub can_save: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkView {
    pub index: usize,
    pub bed_mm: [f64; 2],
    pub role: MarkRole,
    /// Where it was clicked in the photo.
    pub pixel: Option<[f64; 2]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarksProposalView {
    pub plan: JobPlanId,
    pub name: String,
    pub marks_mm: Vec<[f64; 2]>,
    pub power_percent: f64,
    pub speed_mm_s: f64,
    /// Copy into `ConfirmMarksJob`; `None` while the machine cannot take a job right now.
    pub confirm: Option<MarksConfirm>,
    pub confirm_why_not: Option<CameraWhy>,
    /// The job was handed to the machine service (upload only).
    pub sent: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarksConfirm {
    pub action: ActionId,
    pub seen: Option<ExecutionState>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlignmentResultView {
    pub fit_rms_mm: f64,
    pub fit_max_mm: f64,
    /// Residuals on the independent check marks.
    pub check_rms_mm: f64,
    pub check_max_mm: f64,
    pub check_errors_mm: Vec<f64>,
}

/// The calibration key of a presentation camera mode (formats the calibration does not know keep
/// their identity under a fixed code, so two formats never share a calibration).
pub fn config_key(device: &CameraDeviceId, mode: &CameraMode, lens: &str) -> CameraConfigKey {
    use burnburn_image::calib as c;
    let pixel_format = match mode.pixel_format {
        CameraPixelFormat::Bgra8 => c::CameraPixelFormat::Bgra8,
        CameraPixelFormat::Yuv420Video => c::CameraPixelFormat::Yuv420Video,
        CameraPixelFormat::Yuy2 => c::CameraPixelFormat::Other(u32::from_be_bytes(*b"yuvs")),
        CameraPixelFormat::Rgb24 => c::CameraPixelFormat::Other(u32::from_be_bytes(*b"RGB3")),
        CameraPixelFormat::Other(code) => c::CameraPixelFormat::Other(code),
    };
    CameraConfigKey {
        device: c::CameraDeviceId(device.0.clone()),
        mode: c::CameraMode { width: mode.width, height: mode.height, pixel_format, frame_rate_mhz: mode.fps_milli },
        lens: lens.to_owned(),
    }
}
