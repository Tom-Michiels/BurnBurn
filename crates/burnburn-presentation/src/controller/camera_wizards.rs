//! The lens-calibration and workspace-alignment wizards (W3-UI-camera). Corner detection and the
//! lens fit run off the UI thread through the controller's spawner, tagged with the wizard's request
//! identity: a result that belongs to a wizard that was cancelled or restarted is dropped.
//! Nothing here touches a document; camera pixels never leave the calibration.

use std::sync::Arc;

use burnburn_image::calib::{
    AlignmentLens, CameraConfigKey, CheckerGrid, CornerError, LensCalibError, LensCalibration, LensCalibrationRequest,
    LensFitParams, LensModel, LensOrder, MarkObservation, MarkRole, PatternView, WorkspaceAlignment, calibrate_lens,
    detect_checkerboard,
};
use burnburn_types::{CancellationSource, RequestId};

use crate::camera::{CameraDeviceId, CameraFrame, CameraMode};
use crate::camera_workspace::{
    AlignmentResultView, AlignmentView, CameraWhy, CameraWorkspaceIntent, LensResultView, LensWizardView, MarkView,
};

use super::camera::typed_mm;
use super::camera_marks::Proposal;
use super::{AppController, AppUpdate};

/// The printed pattern a new lens wizard starts with (inner corners, millimetres between them).
const DEFAULT_COLS: u32 = 9;
const DEFAULT_ROWS: u32 = 6;
const DEFAULT_PITCH_MM: f64 = 25.0;
/// Largest pattern the wizard accepts.
const MAX_GRID: u32 = 64;
/// A lens fit above this residual (pixels) is not a calibration.
const MAX_LENS_RMS_PX: f64 = 2.0;
const LENS_EVALUATIONS: usize = 4_000;
/// Alignment marks that contradict each other by more than this (mm) are not an alignment.
const MAX_FIT_RESIDUAL_MM: f64 = 5.0;
/// The bed assumed for the default mark layout until a machine profile names the real one.
const FALLBACK_BED_MM: (f64, f64) = (600.0, 400.0);
/// Default mark layout as fractions of the bed: four fit marks, two independent check marks.
const FIT_LAYOUT: [(f64, f64); 4] = [(0.15, 0.15), (0.85, 0.15), (0.85, 0.85), (0.15, 0.85)];
const CHECK_LAYOUT: [(f64, f64); 2] = [(0.5, 0.5), (0.35, 0.65)];

pub(super) fn default_grid() -> CheckerGrid {
    CheckerGrid { cols: DEFAULT_COLS as usize, rows: DEFAULT_ROWS as usize, pitch_mm: DEFAULT_PITCH_MM }
}

/// Finished background work, delivered to [`AppController::poll_camera_work`].
pub(super) enum Done {
    Pattern { request: RequestId, result: Result<PatternView, CornerError> },
    Lens { request: RequestId, result: Result<LensCalibration, LensCalibError> },
}

pub(super) struct LensWizard {
    request: RequestId,
    device: CameraDeviceId,
    mode: CameraMode,
    config: CameraConfigKey,
    grid: CheckerGrid,
    views: Vec<PatternView>,
    detecting: Option<CancellationSource>,
    fitting: Option<CancellationSource>,
    result: Option<LensCalibration>,
    error: Option<CameraWhy>,
}

impl LensWizard {
    fn busy(&self) -> bool {
        self.detecting.is_some() || self.fitting.is_some()
    }

    fn cancel_work(&self) {
        for source in [&self.detecting, &self.fitting].into_iter().flatten() {
            source.cancel();
        }
    }
}

pub(super) struct Mark {
    pub(super) bed_mm: [f64; 2],
    pub(super) role: MarkRole,
    pub(super) pixel: Option<[f64; 2]>,
}

pub(super) struct AlignWizard {
    pub(super) device: CameraDeviceId,
    pub(super) mode: CameraMode,
    pub(super) config: CameraConfigKey,
    pub(super) height_mm: f64,
    pub(super) marks: Vec<Mark>,
    pub(super) photo: Option<Arc<CameraFrame>>,
    pub(super) proposal: Option<Proposal>,
    pub(super) result: Option<WorkspaceAlignment>,
    pub(super) error: Option<CameraWhy>,
}

impl AppController {
    pub(super) fn camera_wizard(&mut self, intent: CameraWorkspaceIntent) -> Result<(), CameraWhy> {
        use CameraWorkspaceIntent as I;
        match intent {
            I::StartLensCalibration { device, cols, rows, pitch_mm } => {
                if cols < 2 || rows < 2 || cols > MAX_GRID || rows > MAX_GRID {
                    return Err(CameraWhy::BadGrid);
                }
                let pitch = match crate::field::parse_number(&pitch_mm, &["mm"]) {
                    Ok(p) if p.is_finite() && p > 0.0 && p <= 1_000.0 => p,
                    _ => return Err(CameraWhy::InvalidNumber),
                };
                self.start_lens_wizard(
                    device,
                    CheckerGrid { cols: cols as usize, rows: rows as usize, pitch_mm: pitch },
                )
            }
            I::AddPatternView => self.add_pattern_view(),
            I::RemovePatternView { index } => {
                let wizard = self.camera_ws.lens.as_mut().ok_or(CameraWhy::NoLensWizard)?;
                if wizard.busy() {
                    return Err(CameraWhy::Busy);
                }
                if index >= wizard.views.len() {
                    return Err(CameraWhy::MarkIndex);
                }
                wizard.views.remove(index);
                wizard.result = None;
                wizard.error = None;
                Ok(())
            }
            I::FitLens { two_coefficients } => self.fit_lens(two_coefficients),
            I::AcceptLensCalibration => {
                let wizard = self.camera_ws.lens.as_ref().ok_or(CameraWhy::NoLensWizard)?;
                let result = wizard.result.clone().ok_or(CameraWhy::NoLensResult)?;
                self.camera_ws.store.insert_lens(result);
                self.camera_ws.lens = None;
                self.rebuild_layers();
                Ok(())
            }
            I::CancelLensCalibration => {
                if let Some(wizard) = self.camera_ws.lens.take() {
                    wizard.cancel_work();
                }
                Ok(())
            }
            I::StartAlignment { device, material_height_mm } => {
                let height = typed_mm(&material_height_mm)?;
                self.start_alignment_wizard(device, height)
            }
            I::SetAlignmentMark { index, x_mm, y_mm } => {
                let parse = |t: &str| match crate::field::parse_number(t, &["mm"]) {
                    Ok(v) if v.is_finite() && v.abs() <= 100_000.0 => Ok(v),
                    _ => Err(CameraWhy::InvalidNumber),
                };
                let (x, y) = (parse(&x_mm)?, parse(&y_mm)?);
                let wizard = self.camera_ws.align.as_mut().ok_or(CameraWhy::NoAlignmentWizard)?;
                let mark = wizard.marks.get_mut(index).ok_or(CameraWhy::MarkIndex)?;
                mark.bed_mm = [x, y];
                mark.pixel = None;
                wizard.proposal = None;
                wizard.result = None;
                wizard.error = None;
                Ok(())
            }
            I::ProposeMarksJob { power_percent, speed_mm_s } => self.propose_marks_job(&power_percent, &speed_mm_s),
            I::ConfirmMarksJob { plan, action, seen } => self.confirm_marks_job(plan, action, seen),
            I::CaptureAlignmentPhoto => self.capture_alignment_photo(),
            I::PlaceAlignmentMark { index, pixel } => {
                let wizard = self.camera_ws.align.as_mut().ok_or(CameraWhy::NoAlignmentWizard)?;
                let photo = wizard.photo.as_ref().ok_or(CameraWhy::NoPhoto)?;
                let (w, h) = (f64::from(photo.image.width()), f64::from(photo.image.height()));
                if index >= wizard.marks.len() {
                    return Err(CameraWhy::MarkIndex);
                }
                if !(pixel.iter().all(|v| v.is_finite())
                    && (0.0..=w).contains(&pixel[0])
                    && (0.0..=h).contains(&pixel[1]))
                {
                    return Err(CameraWhy::OutsideImage);
                }
                wizard.marks[index].pixel = Some(pixel);
                wizard.result = None;
                wizard.error = None;
                Ok(())
            }
            I::ClearAlignmentMark { index } => {
                let wizard = self.camera_ws.align.as_mut().ok_or(CameraWhy::NoAlignmentWizard)?;
                let mark = wizard.marks.get_mut(index).ok_or(CameraWhy::MarkIndex)?;
                mark.pixel = None;
                wizard.result = None;
                Ok(())
            }
            I::FitAlignment => self.fit_alignment(),
            I::SaveAlignment => {
                let wizard = self.camera_ws.align.as_ref().ok_or(CameraWhy::NoAlignmentWizard)?;
                let result = wizard.result.clone().ok_or(CameraWhy::NoAlignmentResult)?;
                self.camera_ws.store.insert_alignment(result);
                self.camera_ws.align = None;
                self.rebuild_layers();
                Ok(())
            }
            I::CancelAlignment => {
                self.camera_ws.align = None;
                Ok(())
            }
            // Handled by the workspace itself.
            _ => Ok(()),
        }
    }

    // --- lens ------------------------------------------------------------------------------------------

    pub(super) fn start_lens_wizard(&mut self, device: CameraDeviceId, grid: CheckerGrid) -> Result<(), CameraWhy> {
        let mode = *self.running_cameras().get(&device).ok_or(CameraWhy::CameraNotRunning)?;
        let request = self.next_id::<RequestId>().ok_or(CameraWhy::Busy)?;
        if let Some(old) = self.camera_ws.lens.take() {
            old.cancel_work();
        }
        let config = self.config_of(&device, &mode);
        self.camera_ws.lens = Some(LensWizard {
            request,
            device,
            mode,
            config,
            grid,
            views: Vec::new(),
            detecting: None,
            fitting: None,
            result: None,
            error: None,
        });
        Ok(())
    }

    /// The latest frame of the wizard's camera, only if it is still in the configuration the wizard
    /// was made for.
    fn frame_for(&self, device: &CameraDeviceId, mode: &CameraMode) -> Result<Arc<CameraFrame>, CameraWhy> {
        if self.running_cameras().get(device) != Some(mode) {
            return Err(CameraWhy::FrameConfigChanged);
        }
        let frame = self.latest_frame_of(device).ok_or(CameraWhy::NoFrame)?;
        if frame.mode != *mode {
            return Err(CameraWhy::FrameConfigChanged);
        }
        Ok(frame)
    }

    fn add_pattern_view(&mut self) -> Result<(), CameraWhy> {
        let wizard = self.camera_ws.lens.as_ref().ok_or(CameraWhy::NoLensWizard)?;
        if wizard.busy() {
            return Err(CameraWhy::Busy);
        }
        let (request, grid) = (wizard.request, wizard.grid);
        let frame = self.frame_for(&wizard.device, &wizard.mode)?;
        let source = CancellationSource::new();
        let token = source.token();
        let inbox = Arc::clone(&self.camera_ws.inbox);
        let image = frame.image.clone();
        let job = Box::new(move || {
            let result = detect_checkerboard(&image, grid, &token);
            if let Ok(mut inbox) = inbox.lock() {
                inbox.push(Done::Pattern { request, result });
            }
        });
        if let Some(wizard) = self.camera_ws.lens.as_mut() {
            wizard.detecting = Some(source);
            wizard.error = None;
        }
        (self.camera_ws.spawner)(job);
        Ok(())
    }

    fn fit_lens(&mut self, two_coefficients: bool) -> Result<(), CameraWhy> {
        let wizard = self.camera_ws.lens.as_mut().ok_or(CameraWhy::NoLensWizard)?;
        if wizard.busy() {
            return Err(CameraWhy::Busy);
        }
        if wizard.views.is_empty() {
            return Err(CameraWhy::NoPatternViews);
        }
        let source = CancellationSource::new();
        let token = source.token();
        let request = LensCalibrationRequest {
            request: wizard.request,
            config: wizard.config.clone(),
            views: wizard.views.clone(),
            params: LensFitParams {
                order: if two_coefficients { LensOrder::K1K2 } else { LensOrder::K1 },
                max_rms_px: MAX_LENS_RMS_PX,
                max_evaluations: LENS_EVALUATIONS,
            },
        };
        let id = wizard.request;
        wizard.fitting = Some(source);
        wizard.error = None;
        wizard.result = None;
        let inbox = Arc::clone(&self.camera_ws.inbox);
        let job = Box::new(move || {
            let result = calibrate_lens(&request, &token);
            if let Ok(mut inbox) = inbox.lock() {
                inbox.push(Done::Lens { request: id, result });
            }
        });
        (self.camera_ws.spawner)(job);
        Ok(())
    }

    /// Delivers finished corner detections and lens fits. A result counts only for the wizard that
    /// asked for it: after a cancel or a restart its request identity no longer matches.
    pub fn poll_camera_work(&mut self) -> AppUpdate {
        let done: Vec<Done> = match self.camera_ws.inbox.lock() {
            Ok(mut inbox) => std::mem::take(&mut *inbox),
            Err(_) => Vec::new(),
        };
        let mut changed = false;
        for item in done {
            let Some(wizard) = self.camera_ws.lens.as_mut() else { continue };
            match item {
                Done::Pattern { request, result } if request == wizard.request => {
                    wizard.detecting = None;
                    changed = true;
                    match result {
                        Ok(view) => {
                            wizard.views.push(view);
                            wizard.result = None;
                            wizard.error = None;
                        }
                        Err(CornerError::Cancelled) => {}
                        Err(e) => wizard.error = Some(CameraWhy::Corners(e)),
                    }
                }
                Done::Lens { request, result } if request == wizard.request => {
                    wizard.fitting = None;
                    changed = true;
                    match result.map(|r| r.accept_for(wizard.request)) {
                        Ok(Ok(lens)) => wizard.result = Some(lens),
                        Ok(Err(_stale)) => {}
                        Err(LensCalibError::Cancelled(_)) => {}
                        Err(e) => wizard.error = Some(CameraWhy::Lens(e)),
                    }
                }
                _ => {}
            }
        }
        if changed { AppUpdate { view_changed: true, ..AppUpdate::default() } } else { AppUpdate::default() }
    }

    pub(super) fn camera_work_busy(&self) -> bool {
        self.camera_ws.lens.as_ref().is_some_and(LensWizard::busy)
    }

    pub(super) fn lens_wizard_view(&self) -> Option<LensWizardView> {
        let w = self.camera_ws.lens.as_ref()?;
        Some(LensWizardView {
            device: w.device.clone(),
            cols: w.grid.cols as u32,
            rows: w.grid.rows as u32,
            pitch_mm: w.grid.pitch_mm,
            views: w.views.iter().map(|v| v.points.len()).collect(),
            detecting: w.detecting.is_some(),
            fitting: w.fitting.is_some(),
            result: w.result.as_ref().map(|r| LensResultView {
                k1: r.intrinsics.k1,
                k2: r.intrinsics.k2,
                rms_px: r.report.rms_px,
                max_px: r.report.max_px,
                views: r.report.views,
                points: r.report.points,
                converged: r.report.converged,
            }),
            error: w.error.clone(),
            can_add_view: !w.busy(),
            can_fit: !w.busy() && !w.views.is_empty(),
            can_accept: !w.busy() && w.result.is_some(),
        })
    }

    // --- alignment ---------------------------------------------------------------------------------------

    pub(super) fn start_alignment_wizard(&mut self, device: CameraDeviceId, height_mm: f64) -> Result<(), CameraWhy> {
        let mode = *self.running_cameras().get(&device).ok_or(CameraWhy::CameraNotRunning)?;
        let (bed_w, bed_h) = self
            .machine
            .as_ref()
            .map_or(FALLBACK_BED_MM, |m| (m.profile.frame().width_mm(), m.profile.frame().height_mm()));
        let mut marks = Vec::new();
        for (fx, fy) in FIT_LAYOUT {
            marks.push(Mark { bed_mm: [bed_w * fx, bed_h * fy], role: MarkRole::Fit, pixel: None });
        }
        for (fx, fy) in CHECK_LAYOUT {
            marks.push(Mark { bed_mm: [bed_w * fx, bed_h * fy], role: MarkRole::Check, pixel: None });
        }
        let config = self.config_of(&device, &mode);
        self.camera_ws.align = Some(AlignWizard {
            device,
            mode,
            config,
            height_mm,
            marks,
            photo: None,
            proposal: None,
            result: None,
            error: None,
        });
        Ok(())
    }

    fn capture_alignment_photo(&mut self) -> Result<(), CameraWhy> {
        let wizard = self.camera_ws.align.as_ref().ok_or(CameraWhy::NoAlignmentWizard)?;
        let frame = self.frame_for(&wizard.device, &wizard.mode)?;
        if let Some(wizard) = self.camera_ws.align.as_mut() {
            wizard.photo = Some(frame);
            wizard.result = None;
            wizard.error = None;
            for mark in &mut wizard.marks {
                mark.pixel = None;
            }
        }
        Ok(())
    }

    fn fit_alignment(&mut self) -> Result<(), CameraWhy> {
        let lens = {
            let wizard = self.camera_ws.align.as_ref().ok_or(CameraWhy::NoAlignmentWizard)?;
            match self.camera_ws.store.lens_for(&wizard.config) {
                Ok(l) => AlignmentLens::Calibrated(l.clone()),
                Err(_) => AlignmentLens::Profile(LensModel::Fixed { k1: 0.0 }),
            }
        };
        let wizard = self.camera_ws.align.as_mut().ok_or(CameraWhy::NoAlignmentWizard)?;
        if wizard.photo.is_none() {
            return Err(CameraWhy::NoPhoto);
        }
        let observations: Vec<MarkObservation> = wizard
            .marks
            .iter()
            .filter_map(|m| Some(MarkObservation { pixel: m.pixel?, bed_mm: m.bed_mm, role: m.role }))
            .collect();
        let count = |role| observations.iter().filter(|o| o.role == role).count();
        if count(MarkRole::Fit) < 4 || count(MarkRole::Check) < 1 {
            return Err(CameraWhy::MarksIncomplete);
        }
        wizard.result = None;
        match WorkspaceAlignment::align(
            &observations,
            wizard.config.clone(),
            lens,
            wizard.height_mm,
            MAX_FIT_RESIDUAL_MM,
        ) {
            Ok(alignment) => {
                wizard.result = Some(alignment);
                wizard.error = None;
                Ok(())
            }
            Err(e) => {
                wizard.error = Some(CameraWhy::Align(e.clone()));
                Err(CameraWhy::Align(e))
            }
        }
    }

    /// The photo the alignment marks are clicked in (pixels for the host to show), if one was taken.
    pub fn alignment_photo(&self) -> Option<Arc<CameraFrame>> {
        self.camera_ws.align.as_ref()?.photo.clone()
    }

    pub(super) fn alignment_view(&self) -> Option<AlignmentView> {
        let w = self.camera_ws.align.as_ref()?;
        let with_pixels = |role| w.marks.iter().filter(|m| m.role == role && m.pixel.is_some()).count();
        let can_fit = w.photo.is_some() && with_pixels(MarkRole::Fit) >= 4 && with_pixels(MarkRole::Check) >= 1;
        Some(AlignmentView {
            device: w.device.clone(),
            material_height_mm: w.height_mm,
            marks: w
                .marks
                .iter()
                .enumerate()
                .map(|(index, m)| MarkView { index, bed_mm: m.bed_mm, role: m.role, pixel: m.pixel })
                .collect(),
            photo: w.photo.as_ref().map(|p| (p.image.width(), p.image.height())),
            uses_lens_calibration: self.camera_ws.store.lens_for(&w.config).is_ok(),
            proposal: self.proposal_view(w),
            result: w.result.as_ref().map(|a| AlignmentResultView {
                fit_rms_mm: a.calibration.fit_residuals_mm.rms_mm,
                fit_max_mm: a.calibration.fit_residuals_mm.max_mm,
                check_rms_mm: a.calibration.residuals_mm.rms_mm,
                check_max_mm: a.calibration.residuals_mm.max_mm,
                check_errors_mm: a.calibration.check_points.iter().map(|c| c.error_mm).collect(),
            }),
            error: w.error.clone(),
            can_propose: self.machine.is_some(),
            can_capture: true,
            can_fit,
            can_save: w.result.is_some(),
        })
    }
}
