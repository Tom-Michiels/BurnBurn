//! Recognising products on the bed from the camera picture (G7-werk, W06): learn a template from a
//! photo, look for it in a later frame, review what was found, and put copies of the cut geometry
//! at the places the operator accepted.
//!
//! Three things stay apart on purpose. The *recognition model* is a template (grey pixels of the
//! photo) and a locator behind a port; the *cut geometry* is the selection of the window and is
//! never derived from a picture; and the *position* in millimetres comes only from the calibration
//! of the camera. The similarity a match has is a measure of likeness (0-100 %), never an accuracy
//! in millimetres, and nothing is accepted automatically: even a single match waits for the
//! operator, and with several matches every one is decided before anything is placed.

use std::fmt;
use std::sync::Arc;

use burnburn_image::GrayImage;
use burnburn_image::locate::{LocateError, LocateMatch, LocateParams, locate};
use burnburn_image::locate_pose::locate_pose;
pub use burnburn_image::locate_pose::{PoseMatch, PoseParams};
use burnburn_types::CancellationToken;

use crate::camera::CameraDeviceId;

/// The port of the localisation algorithm: where does `template` appear in `frame`? The shipped
/// implementation is [`NccLocator`] (correlation in Rust, `burnburn-image`); tests and other
/// algorithms replace it. It must honour `cancel` and return nothing partial.
pub trait TemplateLocator: Send + Sync + fmt::Debug {
    fn locate(
        &self,
        frame: &GrayImage,
        template: &GrayImage,
        params: &LocateParams,
        cancel: &CancellationToken,
    ) -> Result<Vec<LocateMatch>, LocateError>;

    /// The same with a product that may be turned and larger or smaller. The default looks without
    /// rotation or scale (every match at angle 0 and scale 1), so an algorithm that knows only
    /// translation keeps working behind the port.
    fn locate_pose(
        &self,
        frame: &GrayImage,
        template: &GrayImage,
        params: &PoseParams,
        cancel: &CancellationToken,
    ) -> Result<Vec<PoseMatch>, LocateError> {
        let plain = LocateParams {
            min_score: params.min_score,
            max_matches: params.max_matches,
            min_separation_px: params.min_separation_px,
            coarse_factor: None,
        };
        Ok(self
            .locate(frame, template, &plain, cancel)?
            .into_iter()
            .map(|m| PoseMatch { x_px: m.x_px, y_px: m.y_px, angle_deg: 0.0, scale: 1.0, score: m.score })
            .collect())
    }
}

/// The algorithm of `burnburn-image::locate`: zero-mean normalised cross-correlation.
#[derive(Debug, Clone, Copy, Default)]
pub struct NccLocator;

impl TemplateLocator for NccLocator {
    fn locate(
        &self,
        frame: &GrayImage,
        template: &GrayImage,
        params: &LocateParams,
        cancel: &CancellationToken,
    ) -> Result<Vec<LocateMatch>, LocateError> {
        locate(frame, template, params, cancel)
    }

    fn locate_pose(
        &self,
        frame: &GrayImage,
        template: &GrayImage,
        params: &PoseParams,
        cancel: &CancellationToken,
    ) -> Result<Vec<PoseMatch>, LocateError> {
        locate_pose(frame, template, params, cancel)
    }
}

/// One search handed to a worker.
#[derive(Clone)]
pub struct LocateJob {
    pub locator: Arc<dyn TemplateLocator>,
    pub frame: Arc<GrayImage>,
    pub template: Arc<GrayImage>,
    pub params: LocateParams,
    /// Look for a turned and/or scaled product (`min_score` and `max_matches` of `params` still
    /// apply); `None`: translation only.
    pub pose: Option<PoseParams>,
}

impl fmt::Debug for LocateJob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "LocateJob(frame {}x{}, template {}x{}, {:?})",
            self.frame.width, self.frame.height, self.template.width, self.template.height, self.locator
        )
    }
}

/// What the recognition panel asks for (`UiIntent::Recognition`). Numbers arrive as typed (DR-05).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecognitionIntent {
    /// Learn the template: the rectangle (pixels of the photo, `x`, `y`, `width`, `height`) of the
    /// latest frame of a running camera.
    Learn { device: CameraDeviceId, x: String, y: String, width: String, height: String, name: String },
    /// Forget the template and what was found with it.
    Forget,
    /// Look for the template in the latest frame of the camera it was learned from. `min_percent` is
    /// the least similarity of a match; empty is 70.
    Find { min_percent: String },
    /// Stop a search that is running; what it would have found is dropped.
    CancelFind,
    /// What a search also looks for: a product that is turned, one that is larger or smaller.
    SetSearch { rotation: bool, scale: bool },
    /// Learn the template by pointing at two opposite corners of its box in the camera picture on
    /// the sheet (the canvas pointer); the second corner learns it under `name`.
    PointBox { device: CameraDeviceId, name: String },
    /// Stop pointing; the corner clicked so far is dropped.
    EndPoint,
    /// Take a saved template back into use.
    Use { id: u64 },
    /// Remove a saved template.
    Delete { id: u64 },
    /// The operator's decision about one candidate.
    Decide { index: usize, decision: CandidateDecision },
    /// Put a copy of the selection (the cut geometry) at every accepted candidate.
    Place { document: burnburn_types::DocumentId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateDecision {
    /// Nothing decided yet: the start of every candidate, never changed by the program.
    Pending,
    Accepted,
    Rejected,
}

/// Why a recognition intent did nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RecognitionProblem {
    NoCameraPort,
    /// The camera is not running, or has no frame yet.
    NoFrame,
    InvalidNumber,
    /// The rectangle is empty or reaches outside the photo.
    OutsideFrame,
    /// The marked area is (almost) one grey: it has no pattern to look for.
    FlatTemplate,
    TemplateTooSmall,
    TemplateLargerThanFrame,
    NoTemplate,
    /// The camera is not in the configuration (device, resolution, format, lens) the template was learned in.
    FrameConfigChanged,
    /// A search is running.
    Busy,
    NothingFound,
    UnknownCandidate,
    /// Candidates are still waiting for a decision.
    Undecided,
    /// No candidate is accepted.
    NothingAccepted,
    /// The window has no selection: there is no cut geometry to put at the places.
    NothingSelected,
    /// The camera has no calibration for this configuration and height: pixels cannot become millimetres.
    NotCalibrated,
    /// The search itself failed.
    SearchFailed,
    /// No saved template has this id.
    UnknownTemplate,
    /// The library of templates is not read yet.
    LibraryNotReady,
    /// The library file could not be read: it is kept as it is, and nothing is saved in it.
    LibraryUnreadable,
}

/// Always shown with the results: what the score is and is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecognitionNote {
    SimilarityIsNotAccuracy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateView {
    pub name: String,
    pub width_px: u32,
    pub height_px: u32,
    pub device: CameraDeviceId,
    /// Kept in the library of templates (a template with a name is).
    pub saved: bool,
}

/// A template kept in the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedTemplateView {
    pub id: u64,
    pub name: String,
    pub width_px: u32,
    pub height_px: u32,
    pub device: CameraDeviceId,
    /// It is the template in use.
    pub in_use: bool,
}

/// What a search also looks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SearchChoice {
    pub rotation: bool,
    pub scale: bool,
}

/// Pointing at the box of a template in the camera picture on the sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct PointingView {
    /// The first corner clicked, in pixels of the camera frame.
    pub first: Option<[f64; 2]>,
    pub frame_px: (u32, u32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CandidateView {
    pub index: usize,
    /// How alike the window is to the template, in percent. Not a length and not a probability.
    pub similarity_percent: f64,
    /// The centre in the camera frame (pixel-edge coordinates).
    pub x_px: f64,
    pub y_px: f64,
    /// How far the product is turned relative to the template (degrees, clockwise in the picture;
    /// 0 for a search without rotation).
    pub angle_deg: f64,
    /// How large the product is relative to the template (1 for a search without scale).
    pub scale: f64,
    /// The place on the bed, from the camera calibration; `None` without one.
    pub bed_mm: Option<[f64; 2]>,
    pub decision: CandidateDecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RecognitionPhase {
    #[default]
    Idle,
    Searching,
    Found,
}

/// A camera that runs: the template can be learned from its picture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningCamera {
    pub device: CameraDeviceId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecognitionViewModel {
    /// The cameras that run now.
    pub cameras: Vec<RunningCamera>,
    pub template: Option<TemplateView>,
    pub phase: RecognitionPhase,
    pub candidates: Vec<CandidateView>,
    /// More than one candidate: each needs the operator's decision.
    pub needs_review: bool,
    pub accepted: usize,
    pub can_place: bool,
    pub problem: Option<RecognitionProblem>,
    pub note: RecognitionNote,
    pub search: SearchChoice,
    /// The templates in the library, oldest first.
    pub saved: Vec<SavedTemplateView>,
    pub pointing: Option<PointingView>,
    /// The file with the saved templates could not be read: it is kept as it is and nothing is saved in it.
    pub library_unreadable: bool,
}

impl Default for RecognitionViewModel {
    fn default() -> Self {
        RecognitionViewModel {
            cameras: Vec::new(),
            template: None,
            phase: RecognitionPhase::Idle,
            candidates: Vec::new(),
            needs_review: false,
            accepted: 0,
            can_place: false,
            problem: None,
            note: RecognitionNote::SimilarityIsNotAccuracy,
            search: SearchChoice::default(),
            saved: Vec::new(),
            pointing: None,
            library_unreadable: false,
        }
    }
}
