//! Template recognition in the camera picture (W06): `Learn` takes a rectangle of the latest frame
//! as the template, `Find` hands the newest frame to the locator on a worker, `Decide` is the
//! operator's word about each candidate and `Place` puts a copy of the selection at every accepted
//! place. A search that was replaced, cancelled or forgotten is dropped by its task identity; a
//! candidate is never accepted by the program.

use std::sync::Arc;

use burnburn_document::{ArrangeCommand, CommandPayload, PasteItem};
use burnburn_editor::{BedPicture, BedQuad, Overlay, PointerButton, PointerPhase, SnapKind};
use burnburn_geometry::{Affine, Path, Point, Vector};
use burnburn_image::GrayImage;
use burnburn_image::PixelRect;
use burnburn_image::calib::CameraConfigKey;
use burnburn_image::locate::{LocateError, LocateParams, check_template, crop_gray, gray_of_source};
use burnburn_types::{
    Angle, CancellationSource, ContentHash, DocumentId, HashDomain, RequestId, RevisionId, TaskId,
};

use super::clipboard::{bounds_of, items_of};
use super::{AppController, AppUpdate, apply_edit};
use crate::camera::CameraDeviceId;
use crate::field::parse_number;
use crate::intent::ViewPointerEvent;
use crate::recognition::{
    CandidateDecision, CandidateView, LocateJob, NccLocator, PointingView, PoseParams, RecognitionIntent,
    RecognitionNote, RecognitionPhase, RecognitionProblem, RecognitionViewModel, RunningCamera, SavedTemplateView,
    SearchChoice, TemplateLocator, TemplateView,
};
use crate::task::{TaskError, TaskInput, TaskKind, TaskOutcome, TaskOutput, TaskRequest, revision_key};

/// The least similarity of a match when the operator typed nothing (percent).
const DEFAULT_MIN_PERCENT: f64 = 70.0;
/// Angles tried by a search with rotation (degrees); the refinement finds the angle in between.
const ANGLE_STEP_DEG: f64 = 10.0;
/// Scales tried by a search with scale; the refinement finds the scale in between.
const SCALES: [f64; 3] = [0.8, 1.0, 1.25];
/// The camera picture lies on the sheet at this size while the box of a template is pointed at.
const POINT_MM_PER_PX: f64 = 0.1;
/// A drag shorter than this (pixels) is a click: the second corner is the next click.
const MIN_DRAG_PX: f64 = 4.0;
/// The picture shown to point at is at most this many pixels on its long side.
const PICTURE_SIDE: u32 = 512;
/// How far along the turned axis the angle on the bed is measured (pixels).
const ANGLE_PROBE_PX: f64 = 10.0;

struct Learned {
    name: String,
    /// The id in the library of templates, when it is kept there.
    saved_id: Option<u64>,
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
    angle_deg: f64,
    scale: f64,
    score: f64,
    decision: CandidateDecision,
}

struct Found {
    config: CameraConfigKey,
    candidates: Vec<Candidate>,
}

/// Pointing at the box of a template in the camera picture on the sheet.
struct BoxPick {
    device: CameraDeviceId,
    name: String,
    frame_px: (u32, u32),
    picture: Arc<BedPicture>,
    first: Option<[f64; 2]>,
    /// The press of this gesture was the first corner: a drag away from it ends on the second.
    pressed: bool,
    hover: Option<[f64; 2]>,
}

pub(super) struct RecognitionState {
    mode: SearchChoice,
    point: Option<BoxPick>,
    locator: Arc<dyn TemplateLocator>,
    template: Option<Learned>,
    search: Option<Search>,
    found: Option<Found>,
    problem: Option<RecognitionProblem>,
}

impl Default for RecognitionState {
    fn default() -> Self {
        RecognitionState {
            mode: SearchChoice::default(),
            point: None,
            locator: Arc::new(NccLocator),
            template: None,
            search: None,
            found: None,
            problem: None,
        }
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
                angle_deg: c.angle_deg,
                scale: c.scale,
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
                saved: t.saved_id.is_some(),
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
            search: r.mode,
            saved: self
                .libs
                .recognition
                .iter()
                .flat_map(|l| l.templates.iter())
                .map(|t| SavedTemplateView {
                    id: t.id,
                    name: t.name.clone(),
                    width_px: t.width,
                    height_px: t.height,
                    device: CameraDeviceId(t.config.device.0.clone()),
                    in_use: r.template.as_ref().is_some_and(|u| u.saved_id == Some(t.id)),
                })
                .collect(),
            pointing: r.point.as_ref().map(|p| PointingView { first: p.first, frame_px: p.frame_px }),
            library_unreadable: self.libs.recognition_unreadable,
        }
    }

    pub(super) fn recognition_intent(&mut self, intent: RecognitionIntent) -> AppUpdate {
        self.recognition.problem = None;
        // The saved templates are read with the other libraries, at the first use.
        self.ensure_libraries();
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
            RecognitionIntent::SetSearch { rotation, scale } => {
                self.recognition.mode = SearchChoice { rotation, scale };
                Ok(())
            }
            RecognitionIntent::PointBox { device, name } => {
                if let Err(problem) = self.begin_point(&device, &name) {
                    self.recognition.problem = Some(problem);
                }
                return self.pointing_update();
            }
            RecognitionIntent::EndPoint => {
                self.recognition.point = None;
                return self.pointing_update();
            }
            RecognitionIntent::Use { id } => self.use_saved(id),
            RecognitionIntent::Delete { id } => self.delete_saved(id),
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
        let mut template = crop_gray(&gray, rect).map_err(problem_of)?;
        // The template is kept as 8-bit grey, so a template restored from the library is the one in use now.
        for v in &mut template.values {
            *v = v.round().clamp(0.0, 255.0);
        }
        check_template(&template).map_err(problem_of)?;
        let config = self.config_of(device, &frame.mode);
        // A template with a name is kept in the library (teaching a name again replaces that template).
        let name = name.trim().to_owned();
        let saved_id = self.keep_template(&name, &config, &template);
        // The old template and everything found with it go only now: a refused learn changes nothing.
        self.recognition.stop_search();
        self.recognition.found = None;
        self.recognition.template =
            Some(Learned { name, saved_id, gray: Arc::new(template), device: device.clone(), config });
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
            pose: self.pose_params(min_percent / 100.0),
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
            Ok(TaskOutput::LocatedPoses(matches)) if matches.is_empty() => {
                self.recognition.problem = Some(RecognitionProblem::NothingFound);
            }
            Ok(TaskOutput::LocatedPoses(matches)) => {
                let candidates = matches
                    .into_iter()
                    .map(|m| Candidate {
                        x_px: m.x_px,
                        y_px: m.y_px,
                        angle_deg: m.angle_deg,
                        scale: m.scale,
                        score: m.score,
                        decision: CandidateDecision::Pending,
                    })
                    .collect();
                self.recognition.found = Some(Found { config, candidates });
            }
            Ok(TaskOutput::Located(matches)) => {
                let candidates = matches
                    .into_iter()
                    .map(|m| Candidate {
                        x_px: m.x_px,
                        y_px: m.y_px,
                        angle_deg: 0.0,
                        scale: 1.0,
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

    /// A copy of the selection at every accepted place, turned and scaled like the product found
    /// there; the list is spent afterwards.
    fn place_copies(&mut self, document: DocumentId) -> Result<(), RecognitionProblem> {
        if self.recognition.search.is_some() {
            return Err(RecognitionProblem::Busy);
        }
        let found = self.recognition.found.as_ref().ok_or(RecognitionProblem::NothingFound)?;
        if found.candidates.iter().any(|c| c.decision == CandidateDecision::Pending) {
            return Err(RecognitionProblem::Undecided);
        }
        let accepted: Vec<(&Candidate, [f64; 2])> = found
            .candidates
            .iter()
            .filter(|c| c.decision == CandidateDecision::Accepted)
            .map(|c| (c, [c.x_px, c.y_px]))
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
        for (c, pixel) in &accepted {
            let at = self.bed_of(&config, *pixel).ok_or(RecognitionProblem::NotCalibrated)?;
            let turn =
                if c.angle_deg == 0.0 { 0.0 } else { self.bed_angle(&config, *pixel, c.angle_deg).unwrap_or(0.0) };
            targets.push((at, turn, c.scale));
        }
        let requests: Vec<Option<RequestId>> = targets.iter().map(|_| self.next_id::<RequestId>()).collect();
        if let Some(s) = self.sessions.get_mut(&document) {
            for ((target, turn, scale), request) in targets.iter().zip(requests) {
                let command = if *turn == 0.0 && *scale == 1.0 {
                    let offset = Vector { x: target[0] - centre[0], y: target[1] - centre[1] };
                    ArrangeCommand::Paste { items: items.clone(), offset }
                } else {
                    // Turned about the centre of the selection, scaled, and put on the place found.
                    let moved = placed(&items, centre, *target, *turn, *scale);
                    ArrangeCommand::Paste { items: moved, offset: Vector { x: 0.0, y: 0.0 } }
                };
                apply_edit(s, document, CommandPayload::Arrange(command), request);
            }
        }
        self.recognition.found = None;
        Ok(())
    }

    /// The turn on the bed (degrees, counter-clockwise, y up) of a product found `angle_deg` turned
    /// in the picture (clockwise, y down) at `pixel`: the picture's axes go to the bed by the
    /// calibration, mirrored or not.
    fn bed_angle(&self, config: &CameraConfigKey, pixel: [f64; 2], angle_deg: f64) -> Option<f64> {
        let a = angle_deg.to_radians();
        let origin = self.bed_of(config, pixel)?;
        let along = self.bed_of(config, [pixel[0] + ANGLE_PROBE_PX, pixel[1]])?;
        let turned =
            self.bed_of(config, [pixel[0] + ANGLE_PROBE_PX * a.cos(), pixel[1] + ANGLE_PROBE_PX * a.sin()])?;
        let heading = |p: [f64; 2]| (p[1] - origin[1]).atan2(p[0] - origin[0]);
        Some((heading(turned) - heading(along)).to_degrees())
    }

    /// The search a panel choice asks for; `None` is translation only.
    fn pose_params(&self, min_score: f64) -> Option<PoseParams> {
        let mode = self.recognition.mode;
        (mode.rotation || mode.scale).then(|| PoseParams {
            min_score,
            angle_step_deg: mode.rotation.then_some(ANGLE_STEP_DEG),
            scales: if mode.scale { SCALES.to_vec() } else { vec![1.0] },
            ..PoseParams::default()
        })
    }

    // ---- saved templates -------------------------------------------------------------------------

    /// Keeps a named template in the library; its id, or `None` when it is not kept (no name, the
    /// library not read yet or unreadable).
    fn keep_template(&mut self, name: &str, config: &CameraConfigKey, template: &GrayImage) -> Option<u64> {
        if name.is_empty() || self.libs.recognition_unreadable {
            return None;
        }
        let gray8: Vec<u8> = template.values.iter().map(|v| v.round().clamp(0.0, 255.0) as u8).collect();
        let library = self.libs.recognition.as_mut()?;
        let id = library.upsert(name, config.clone(), template.width, template.height, gray8).ok()?;
        self.flush_recognition();
        Some(id)
    }

    fn use_saved(&mut self, id: u64) -> Result<(), RecognitionProblem> {
        let library = self.libs.recognition.as_ref().ok_or(RecognitionProblem::LibraryNotReady)?;
        let record = library.find(id).cloned().ok_or(RecognitionProblem::UnknownTemplate)?;
        self.recognition.stop_search();
        self.recognition.found = None;
        let values = record.gray8.iter().map(|b| f32::from(*b)).collect();
        self.recognition.template = Some(Learned {
            name: record.name,
            saved_id: Some(record.id),
            gray: Arc::new(GrayImage { width: record.width, height: record.height, values }),
            device: CameraDeviceId(record.config.device.0.clone()),
            config: record.config,
        });
        Ok(())
    }

    fn delete_saved(&mut self, id: u64) -> Result<(), RecognitionProblem> {
        if self.libs.recognition_unreadable {
            return Err(RecognitionProblem::LibraryUnreadable);
        }
        let library = self.libs.recognition.as_mut().ok_or(RecognitionProblem::LibraryNotReady)?;
        if !library.remove(id) {
            return Err(RecognitionProblem::UnknownTemplate);
        }
        // A template in use stays usable; it is just no longer kept.
        if let Some(t) = self.recognition.template.as_mut().filter(|t| t.saved_id == Some(id)) {
            t.saved_id = None;
        }
        self.flush_recognition();
        Ok(())
    }

    // ---- pointing at the box of a template ---------------------------------------------------------

    fn pointing_update(&self) -> AppUpdate {
        self.workspace_update()
    }

    fn begin_point(&mut self, device: &CameraDeviceId, name: &str) -> Result<(), RecognitionProblem> {
        self.recognition.point = None;
        if self.camera.is_none() {
            return Err(RecognitionProblem::NoCameraPort);
        }
        if !self.running_cameras().contains_key(device) {
            return Err(RecognitionProblem::NoFrame);
        }
        let frame = self.latest_frame_of(device).ok_or(RecognitionProblem::NoFrame)?;
        let gray = gray_of_source(&frame.image);
        let picture = picture_of(&gray).ok_or(RecognitionProblem::SearchFailed)?;
        self.recognition.point = Some(BoxPick {
            device: device.clone(),
            name: name.to_owned(),
            frame_px: (gray.width, gray.height),
            picture: Arc::new(picture),
            first: None,
            pressed: false,
            hover: None,
        });
        // Pointing starts with the whole picture in view.
        let (w, h) = (f64::from(gray.width) * POINT_MM_PER_PX, f64::from(gray.height) * POINT_MM_PER_PX);
        let bounds = burnburn_geometry::Bounds::from_corners(Point::ORIGIN, Point::new(w, h));
        if let Some(document) = self.active.or_else(|| self.sessions.keys().next().copied())
            && let Some(s) = self.sessions.get_mut(&document)
        {
            super::arrange::fit_to(s, document, bounds);
        }
        Ok(())
    }

    /// Escape puts the pointing down. Whether there was any.
    pub(super) fn end_recognition_point(&mut self) -> bool {
        self.recognition.point.take().is_some()
    }

    /// A pointer event of the canvas while the box of a template is pointed at: it is the pointer in
    /// the picture. `None` when nothing is being pointed at (the event is an ordinary one).
    pub(super) fn recognition_pointer(&mut self, document: DocumentId, event: &ViewPointerEvent) -> Option<AppUpdate> {
        let frame_px = self.recognition.point.as_ref()?.frame_px;
        let s = self.sessions.get(&document)?;
        let mm = s.camera.points_to_world(&s.viewport, event.position_pt);
        let px = [
            (mm.x / POINT_MM_PER_PX).clamp(0.0, f64::from(frame_px.0)),
            (f64::from(frame_px.1) - mm.y / POINT_MM_PER_PX).clamp(0.0, f64::from(frame_px.1)),
        ];
        let primary = event.button.is_none_or(|b| b == PointerButton::Primary);
        let mut finished: Option<([f64; 2], [f64; 2])> = None;
        if let Some(pick) = self.recognition.point.as_mut() {
            match event.phase {
                PointerPhase::Down if primary => match pick.first {
                    None => {
                        pick.first = Some(px);
                        pick.pressed = true;
                    }
                    Some(first) => finished = Some((first, px)),
                },
                PointerPhase::Move => pick.hover = Some(px),
                PointerPhase::Up if primary => {
                    if pick.pressed
                        && let Some(first) = pick.first
                        && (first[0] - px[0]).hypot(first[1] - px[1]) >= MIN_DRAG_PX
                    {
                        finished = Some((first, px));
                    }
                    pick.pressed = false;
                }
                PointerPhase::Cancel => pick.pressed = false,
                // Everything else (a press with another button) is eaten while pointing.
                _ => {}
            }
        }
        if let Some((a, b)) = finished {
            self.finish_point(a, b);
        }
        Some(self.pointing_update())
    }

    /// The second corner is in: the box is the template.
    fn finish_point(&mut self, a: [f64; 2], b: [f64; 2]) {
        let Some(pick) = self.recognition.point.take() else { return };
        self.recognition.problem = None;
        let (x0, x1) = (a[0].min(b[0]).round(), a[0].max(b[0]).round());
        let (y0, y1) = (a[1].min(b[1]).round(), a[1].max(b[1]).round());
        let number = |v: f64| format!("{}", v as u32);
        let result =
            self.learn_template(&pick.device, &number(x0), &number(y0), &number(x1 - x0), &number(y1 - y0), &pick.name);
        if let Err(problem) = result {
            self.recognition.problem = Some(problem);
        }
    }

    /// The camera picture to point at, and the corner and the box marked so far, on the sheet.
    pub(super) fn recognition_overlays(&self) -> Vec<Overlay> {
        let Some(pick) = self.recognition.point.as_ref() else { return Vec::new() };
        let height = f64::from(pick.frame_px.1);
        let to_mm = |p: [f64; 2]| Point::new(p[0] * POINT_MM_PER_PX, (height - p[1]) * POINT_MM_PER_PX);
        let (w, h) = (f64::from(pick.frame_px.0), height);
        let quad = BedQuad {
            bed_mm: [to_mm([0.0, 0.0]), to_mm([w, 0.0]), to_mm([w, h]), to_mm([0.0, h])],
            uv: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        };
        let mut out =
            vec![Overlay::BedImage { picture: Arc::clone(&pick.picture), quads: Arc::from(vec![quad]), opacity: 1.0 }];
        if let Some(first) = pick.first {
            out.push(Overlay::SnapCandidate { at: to_mm(first), kind: SnapKind::Endpoint });
            if let Some(other) = pick.hover {
                let corners = [first, [other[0], first[1]], other, [first[0], other[1]]].map(to_mm);
                if let Ok(rect) = Path::polyline(&corners, true) {
                    out.push(Overlay::ResultPreview { path: Arc::new(rect) });
                }
            }
        }
        out
    }
}

/// A small grey picture of the camera frame for the sheet (at most [`PICTURE_SIDE`] pixels on the long side).
fn picture_of(gray: &GrayImage) -> Option<BedPicture> {
    let (w, h) = (gray.width, gray.height);
    let step = w.max(h).div_ceil(PICTURE_SIDE).max(1);
    let (nw, nh) = (w.div_ceil(step), h.div_ceil(step));
    let mut rgba = Vec::with_capacity(nw as usize * nh as usize * 4);
    for by in 0..nh {
        for bx in 0..nw {
            let (mut sum, mut n) = (0.0_f32, 0.0_f32);
            for y in by * step..((by + 1) * step).min(h) {
                for x in bx * step..((bx + 1) * step).min(w) {
                    sum += gray.values[(y * w + x) as usize];
                    n += 1.0;
                }
            }
            let g = (sum / n.max(1.0)).round().clamp(0.0, 255.0) as u8;
            rgba.extend_from_slice(&[g, g, g, 255]);
        }
    }
    let hash = ContentHash::of(HashDomain::SourcePixels, &rgba);
    let mut id = [0u8; 8];
    id.copy_from_slice(&hash.as_bytes()[..8]);
    BedPicture::new(u64::from_be_bytes(id), nw, nh, Arc::from(rgba))
}

/// The items turned by `turn_deg` (counter-clockwise), `scale` times as large, about `centre`, and
/// put on `target`. Editable definitions of basic shapes are dropped: the shape is no longer the basic one.
fn placed(items: &[PasteItem], centre: [f64; 2], target: [f64; 2], turn_deg: f64, scale: f64) -> Vec<PasteItem> {
    let matrix = (|| {
        let to_origin = Affine::translate(Vector { x: -centre[0], y: -centre[1] }).ok()?;
        let scaled = to_origin.then(&Affine::scale(scale, scale).ok()?);
        let turned = scaled.then(&Affine::rotate(Angle::from_degrees(turn_deg).ok()?));
        Some(turned.then(&Affine::translate(Vector { x: target[0], y: target[1] }).ok()?))
    })();
    items
        .iter()
        .map(|item| {
            let mut item = item.clone();
            if let Some(m) = &matrix {
                item.path = item.path.transform(m);
                item.parametric = None;
            }
            item
        })
        .collect()
}
