//! Template recognition in the camera picture (W06): `Learn` takes a rectangle of the latest frame
//! as the template, `Find` hands the newest frame to the locator on a worker, `Decide` is the
//! operator's word about each candidate and `Place` puts a copy of the selection at every accepted
//! place. A search that was replaced, cancelled or forgotten is dropped by its task identity; a
//! candidate is never accepted by the program.

use std::sync::Arc;

use burnburn_document::{ArrangeCommand, CommandPayload};
use burnburn_geometry::Vector;
use burnburn_image::GrayImage;
use burnburn_image::PixelRect;
use burnburn_image::calib::CameraConfigKey;
use burnburn_image::locate::{LocateError, LocateParams, check_template, crop_gray, gray_of_source};
use burnburn_types::{CancellationSource, DocumentId, RequestId, RevisionId, TaskId};

use super::clipboard::{bounds_of, items_of};
use super::{AppController, AppUpdate, apply_edit};
use crate::camera::CameraDeviceId;
use crate::field::parse_number;
use crate::recognition::{
    CandidateDecision, CandidateView, LocateJob, NccLocator, RecognitionIntent, RecognitionNote, RecognitionPhase,
    RecognitionProblem, RecognitionViewModel, RunningCamera, TemplateLocator, TemplateView,
};
use crate::task::{TaskError, TaskInput, TaskKind, TaskOutcome, TaskOutput, TaskRequest, revision_key};

/// The least similarity of a match when the operator typed nothing (percent).
const DEFAULT_MIN_PERCENT: f64 = 70.0;

struct Learned {
    name: String,
    gray: Arc<GrayImage>,
    device: CameraDeviceId,
    /// The camera configuration (device, resolution, format, frame rate, lens) the photo was taken in.
    config: CameraConfigKey,
}

struct Search {
    task: TaskId,
    cancel: CancellationSource,
}

struct Candidate {
    x_px: f64,
    y_px: f64,
    score: f64,
    decision: CandidateDecision,
}

struct Found {
    config: CameraConfigKey,
    candidates: Vec<Candidate>,
}

pub(super) struct RecognitionState {
    locator: Arc<dyn TemplateLocator>,
    template: Option<Learned>,
    search: Option<Search>,
    found: Option<Found>,
    problem: Option<RecognitionProblem>,
}

impl Default for RecognitionState {
    fn default() -> Self {
        RecognitionState { locator: Arc::new(NccLocator), template: None, search: None, found: None, problem: None }
    }
}

impl RecognitionState {
    /// Is `task` the search that is still wanted? A replaced or cancelled one is not.
    pub(super) fn owns(&self, task: TaskId) -> bool {
        self.search.as_ref().is_some_and(|s| s.task == task)
    }

    /// Stops the running search, if any: its result is dropped when it arrives.
    fn stop_search(&mut self) {
        if let Some(search) = self.search.take() {
            search.cancel.cancel();
        }
    }
}

fn changed() -> AppUpdate {
    AppUpdate { view_changed: true, ..AppUpdate::default() }
}

/// A pixel number as typed: a whole number of pixels, not negative.
fn pixels(text: &str) -> Result<u32, RecognitionProblem> {
    match parse_number(text, &["px"]) {
        Ok(v) if v.is_finite() && v >= 0.0 && v.fract() == 0.0 && v <= f64::from(u32::MAX) => Ok(v as u32),
        _ => Err(RecognitionProblem::InvalidNumber),
    }
}

fn problem_of(e: LocateError) -> RecognitionProblem {
    match e {
        LocateError::FlatTemplate => RecognitionProblem::FlatTemplate,
        LocateError::TemplateTooSmall => RecognitionProblem::TemplateTooSmall,
        LocateError::TemplateLargerThanFrame => RecognitionProblem::TemplateLargerThanFrame,
        LocateError::BadCrop => RecognitionProblem::OutsideFrame,
        _ => RecognitionProblem::SearchFailed,
    }
}

impl AppController {
    /// Replaces the algorithm behind the recognition port.
    pub fn attach_locator(&mut self, locator: Arc<dyn TemplateLocator>) {
        self.recognition.locator = locator;
    }

    /// The bed position of a camera pixel for the material height of the camera workspace, if the
    /// camera has a calibration for exactly this configuration.
    fn bed_of(&self, config: &CameraConfigKey, pixel: [f64; 2]) -> Option<[f64; 2]> {
        self.camera_ws.store.pixel_to_bed_at_height(config, pixel, self.camera_ws.height_mm).ok()
    }

    pub(super) fn recognition_view(&self) -> RecognitionViewModel {
        let r = &self.recognition;
        let candidates: Vec<CandidateView> = r
            .found
            .iter()
            .flat_map(|f| f.candidates.iter().map(move |c| (f, c)))
            .enumerate()
            .map(|(index, (f, c))| CandidateView {
                index,
                similarity_percent: c.score * 100.0,
                x_px: c.x_px,
                y_px: c.y_px,
                bed_mm: self.bed_of(&f.config, [c.x_px, c.y_px]),
                decision: c.decision,
            })
            .collect();
        let accepted = candidates.iter().filter(|c| c.decision == CandidateDecision::Accepted).count();
        let undecided = candidates.iter().any(|c| c.decision == CandidateDecision::Pending);
        let names: std::collections::BTreeMap<CameraDeviceId, String> = self
            .camera
            .as_ref()
            .and_then(|c| c.devices().ok())
            .unwrap_or_default()
            .into_iter()
            .map(|d| (d.id, d.name))
            .collect();
        let cameras = self
            .running_cameras()
            .into_keys()
            .map(|device| RunningCamera {
                name: names.get(&device).cloned().unwrap_or_else(|| device.0.clone()),
                device,
            })
            .collect();
        RecognitionViewModel {
            cameras,
            template: r.template.as_ref().map(|t| TemplateView {
                name: t.name.clone(),
                width_px: t.gray.width,
                height_px: t.gray.height,
                device: t.device.clone(),
            }),
            phase: if r.search.is_some() {
                RecognitionPhase::Searching
            } else if r.found.is_some() {
                RecognitionPhase::Found
            } else {
                RecognitionPhase::Idle
            },
            needs_review: candidates.len() > 1,
            can_place: accepted > 0 && !undecided,
            accepted,
            candidates,
            problem: r.problem,
            note: RecognitionNote::SimilarityIsNotAccuracy,
        }
    }

    pub(super) fn recognition_intent(&mut self, intent: RecognitionIntent) -> AppUpdate {
        self.recognition.problem = None;
        let result = match intent {
            RecognitionIntent::Learn { device, x, y, width, height, name } => {
                self.learn_template(&device, &x, &y, &width, &height, &name)
            }
            RecognitionIntent::Forget => {
                self.recognition.stop_search();
                self.recognition.template = None;
                self.recognition.found = None;
                Ok(())
            }
            RecognitionIntent::Find { min_percent } => self.find_template(&min_percent),
            RecognitionIntent::CancelFind => {
                self.recognition.stop_search();
                Ok(())
            }
            RecognitionIntent::Decide { index, decision } => self.decide(index, decision),
            RecognitionIntent::Place { document } => self.place_copies(document),
        };
        if let Err(problem) = result {
            self.recognition.problem = Some(problem);
        }
        changed()
    }

    fn learn_template(
        &mut self,
        device: &CameraDeviceId,
        x: &str,
        y: &str,
        width: &str,
        height: &str,
        name: &str,
    ) -> Result<(), RecognitionProblem> {
        let rect = PixelRect { x: pixels(x)?, y: pixels(y)?, width: pixels(width)?, height: pixels(height)? };
        if self.camera.is_none() {
            return Err(RecognitionProblem::NoCameraPort);
        }
        if !self.running_cameras().contains_key(device) {
            return Err(RecognitionProblem::NoFrame);
        }
        let frame = self.latest_frame_of(device).ok_or(RecognitionProblem::NoFrame)?;
        let gray = gray_of_source(&frame.image);
        let template = crop_gray(&gray, rect).map_err(problem_of)?;
        check_template(&template).map_err(problem_of)?;
        // The old template and everything found with it go only now: a refused learn changes nothing.
        self.recognition.stop_search();
        self.recognition.found = None;
        self.recognition.template = Some(Learned {
            name: name.trim().to_owned(),
            gray: Arc::new(template),
            device: device.clone(),
            config: self.config_of(device, &frame.mode),
        });
        Ok(())
    }

    fn find_template(&mut self, min_percent: &str) -> Result<(), RecognitionProblem> {
        let min_percent = if min_percent.trim().is_empty() {
            DEFAULT_MIN_PERCENT
        } else {
            match parse_number(min_percent, &["%"]) {
                Ok(v) if v.is_finite() && v > 0.0 && v <= 100.0 => v,
                _ => return Err(RecognitionProblem::InvalidNumber),
            }
        };
        let (device, config, template) = {
            let t = self.recognition.template.as_ref().ok_or(RecognitionProblem::NoTemplate)?;
            (t.device.clone(), t.config.clone(), Arc::clone(&t.gray))
        };
        if self.camera.is_none() {
            return Err(RecognitionProblem::NoCameraPort);
        }
        if !self.running_cameras().contains_key(&device) {
            return Err(RecognitionProblem::NoFrame);
        }
        let frame = self.latest_frame_of(&device).ok_or(RecognitionProblem::NoFrame)?;
        if self.config_of(&device, &frame.mode) != config {
            return Err(RecognitionProblem::FrameConfigChanged);
        }
        let document =
            self.active.or_else(|| self.sessions.keys().next().copied()).ok_or(RecognitionProblem::NoFrame)?;
        let task = self.next_id::<TaskId>().ok_or(RecognitionProblem::SearchFailed)?;
        let cancel = CancellationSource::new();
        let job = LocateJob {
            locator: Arc::clone(&self.recognition.locator),
            frame: Arc::new(gray_of_source(&frame.image)),
            template,
            params: LocateParams { min_score: min_percent / 100.0, ..LocateParams::default() },
        };
        let request = TaskRequest {
            id: task,
            document,
            source: RevisionId::INITIAL,
            key: revision_key(TaskKind::Locate, document, RevisionId::INITIAL),
            cancel: cancel.token(),
            input: TaskInput::Locate(Box::new(job)),
        };
        // A newer search replaces the one still waiting: only the newest counts.
        self.recognition.stop_search();
        self.recognition.found = None;
        if self.ports.tasks.submit(request).is_err() {
            return Err(RecognitionProblem::SearchFailed);
        }
        self.recognition.search = Some(Search { task, cancel });
        Ok(())
    }

    /// The search finished: its candidates, every one still waiting for the operator.
    pub(super) fn located(&mut self, outcome: TaskOutcome) -> AppUpdate {
        let Some(search) = self.recognition.search.take() else { return AppUpdate::default() };
        if search.task != outcome.id {
            self.recognition.search = Some(search);
            return AppUpdate::default();
        }
        let Some(config) = self.recognition.template.as_ref().map(|t| t.config.clone()) else {
            return changed();
        };
        match outcome.result {
            Ok(TaskOutput::Located(matches)) if matches.is_empty() => {
                self.recognition.problem = Some(RecognitionProblem::NothingFound);
            }
            Ok(TaskOutput::Located(matches)) => {
                let candidates = matches
                    .into_iter()
                    .map(|m| Candidate {
                        x_px: m.x_px,
                        y_px: m.y_px,
                        score: m.score,
                        decision: CandidateDecision::Pending,
                    })
                    .collect();
                self.recognition.found = Some(Found { config, candidates });
            }
            Err(TaskError::Cancelled) => {}
            _ => self.recognition.problem = Some(RecognitionProblem::SearchFailed),
        }
        changed()
    }

    fn decide(&mut self, index: usize, decision: CandidateDecision) -> Result<(), RecognitionProblem> {
        let found = self.recognition.found.as_mut().ok_or(RecognitionProblem::NothingFound)?;
        let candidate = found.candidates.get_mut(index).ok_or(RecognitionProblem::UnknownCandidate)?;
        candidate.decision = decision;
        Ok(())
    }

    /// A copy of the selection at every accepted place; the list is spent afterwards.
    fn place_copies(&mut self, document: DocumentId) -> Result<(), RecognitionProblem> {
        if self.recognition.search.is_some() {
            return Err(RecognitionProblem::Busy);
        }
        let found = self.recognition.found.as_ref().ok_or(RecognitionProblem::NothingFound)?;
        if found.candidates.iter().any(|c| c.decision == CandidateDecision::Pending) {
            return Err(RecognitionProblem::Undecided);
        }
        let accepted: Vec<[f64; 2]> = found
            .candidates
            .iter()
            .filter(|c| c.decision == CandidateDecision::Accepted)
            .map(|c| [c.x_px, c.y_px])
            .collect();
        if accepted.is_empty() {
            return Err(RecognitionProblem::NothingAccepted);
        }
        let config = found.config.clone();
        let session = self.sessions.get(&document).ok_or(RecognitionProblem::NothingSelected)?;
        let items = items_of(session, session.selection.ids());
        let centre = bounds_of(&items).ok_or(RecognitionProblem::NothingSelected)?;
        let centre = [(centre.min().x + centre.max().x) / 2.0, (centre.min().y + centre.max().y) / 2.0];
        // Every accepted place needs millimetres before anything is placed.
        let mut targets = Vec::with_capacity(accepted.len());
        for pixel in accepted {
            targets.push(self.bed_of(&config, pixel).ok_or(RecognitionProblem::NotCalibrated)?);
        }
        let requests: Vec<Option<RequestId>> = targets.iter().map(|_| self.next_id::<RequestId>()).collect();
        if let Some(s) = self.sessions.get_mut(&document) {
            for (target, request) in targets.iter().zip(requests) {
                let offset = Vector { x: target[0] - centre[0], y: target[1] - centre[1] };
                let payload = CommandPayload::Arrange(ArrangeCommand::Paste { items: items.clone(), offset });
                apply_edit(s, document, payload, request);
            }
        }
        self.recognition.found = None;
        Ok(())
    }
}
