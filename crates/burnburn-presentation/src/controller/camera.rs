//! The bed camera (IM27): the port, its device and mode, and the camera panel.

use std::sync::Arc;

use crate::camera::{
    BedCameraIntent, CameraAuthorization, CameraDeviceId, CameraFrameInfo, CameraMode, CameraPort, CameraViewModel,
};
use crate::view_model::MessageKey;

use super::{AppController, AppUpdate, error};

impl AppController {
    /// The bed camera (IM27); hosts implement the port.
    pub fn attach_camera(&mut self, camera: Arc<dyn CameraPort>) {
        self.camera = Some(camera);
        self.camera_active = None;
    }
}

impl AppController {
    /// Camera panel, when a camera port is attached.
    pub fn camera_view(&self) -> Option<CameraViewModel> {
        let camera = self.camera.as_ref()?;
        let (devices, devices_error) = match camera.devices() {
            Ok(d) => (d, None),
            Err(e) => (Vec::new(), Some(e)),
        };
        let frame = self
            .camera_active
            .as_ref()
            .and_then(|(device, _)| camera.latest_frame(device))
            .map(|f| CameraFrameInfo { seq: f.seq, time: f.time, width: f.image.width(), height: f.image.height() });
        Some(CameraViewModel {
            authorization: camera.authorization(),
            devices,
            devices_error,
            active: self.camera_active.clone(),
            frame,
            note: MessageKey::VisualPlacementOnly,
        })
    }

    pub(super) fn bed_camera(&mut self, intent: BedCameraIntent) -> AppUpdate {
        let Some(camera) = self.camera.clone() else { return error(None, MessageKey::CommandRejected) };
        let changed = AppUpdate { view_changed: true, ..AppUpdate::default() };
        match intent {
            BedCameraIntent::RequestAccess => {
                camera.request_authorization();
                changed
            }
            BedCameraIntent::Start { device, mode } => {
                if camera.authorization() != CameraAuthorization::Authorized {
                    return error(None, MessageKey::CameraNotAuthorized);
                }
                if let Some((old, _)) = self.camera_active.take() {
                    camera.stop(&old);
                }
                match camera.start(&device, &mode) {
                    Ok(()) => {
                        self.camera_active = Some((device, mode));
                        changed
                    }
                    Err(_) => error(None, MessageKey::CommandRejected),
                }
            }
            BedCameraIntent::Stop => {
                if let Some((device, _)) = self.camera_active.take() {
                    camera.stop(&device);
                }
                changed
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The camera workspace (W3-UI-camera): the picture on the bed, the cameras in one bed system and
// the stored calibrations. The wizards live in `camera_wizards.rs`, the marks job in
// `camera_marks.rs`. Camera pixels are a visual placement aid; nothing here feeds CAM.
// ---------------------------------------------------------------------------------------------

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

use burnburn_editor::{BedPicture, BedQuad, Overlay};
use burnburn_geometry::Point;
use burnburn_image::PixelFormat;
use burnburn_image::SourceImage;
use burnburn_image::calib::{
    self as calib, CalibrationStore, CameraConfigKey, CameraView, HeightLookupError, StoreMiss, cameras_seeing,
    config_validity,
};

use crate::camera::CameraFrame;
use crate::camera_workspace::{
    CalibrationKind, CalibrationRef, CalibrationRow, CalibrationSummary, CalibrationValidity, CameraInUse, CameraWhy,
    CameraWorkspaceIntent, CameraWorkspaceView, OverlayLayerView, OverlayView, config_key,
};
use crate::field::parse_number;

use super::camera_wizards::{self, Done};

/// Runs a job off the UI thread. The default spawns a thread; tests inject their own.
pub type CameraSpawner = Arc<dyn Fn(Box<dyn FnOnce() + Send>) + Send + Sync>;

/// Mesh cells per picture: the lens bends straight lines, so the picture is placed cell by cell.
const MESH_COLS: usize = 32;
const MESH_ROWS: usize = 18;
/// Largest typed material height, edge margin and the like: anything above is a typing mistake.
const MAX_TYPED: f64 = 1_000.0;
/// The lens profile a camera has until the user names another.
pub(super) const DEFAULT_LENS_PROFILE: &str = "standard";
/// Opacity a new overlay starts with (percent).
const DEFAULT_OPACITY_PERCENT: f64 = 50.0;

/// A frozen frame placed (or not) on the bed.
pub(super) struct Layer {
    pub(super) device: CameraDeviceId,
    /// The configuration the frame was taken in; its calibration is looked up with this key.
    pub(super) config: CameraConfigKey,
    pub(super) seq: u64,
    pub(super) picture: Arc<BedPicture>,
    pub(super) quads: Arc<[BedQuad]>,
    pub(super) not_placed: Option<CameraWhy>,
}

pub(super) struct Workspace {
    pub(super) store: CalibrationStore,
    /// Cameras started by the workspace (the strip's own camera is `camera_active`).
    pub(super) sources: BTreeMap<CameraDeviceId, CameraMode>,
    /// Lens/mounting profile per camera; part of a calibration's configuration.
    pub(super) lens_profiles: BTreeMap<CameraDeviceId, String>,
    pub(super) visible: bool,
    pub(super) opacity_percent: f64,
    pub(super) edge_margin_px: f64,
    pub(super) height_mm: f64,
    pub(super) layers: Vec<Layer>,
    pub(super) lens: Option<camera_wizards::LensWizard>,
    pub(super) align: Option<camera_wizards::AlignWizard>,
    pub(super) refusal: Option<CameraWhy>,
    pub(super) spawner: CameraSpawner,
    /// Finished background work, delivered by [`AppController::poll_camera_work`].
    pub(super) inbox: Arc<Mutex<Vec<Done>>>,
}

impl Default for Workspace {
    fn default() -> Self {
        Workspace {
            store: CalibrationStore::default(),
            sources: BTreeMap::new(),
            lens_profiles: BTreeMap::new(),
            visible: true,
            opacity_percent: DEFAULT_OPACITY_PERCENT,
            edge_margin_px: 0.0,
            height_mm: 0.0,
            layers: Vec::new(),
            lens: None,
            align: None,
            refusal: None,
            spawner: Arc::new(|job| {
                // A thread that cannot start leaves the wizard waiting; the user can cancel it.
                let _ = std::thread::Builder::new().name("burnburn-camera".into()).spawn(job);
            }),
            inbox: Arc::default(),
        }
    }
}

impl Workspace {
    pub(super) fn lens_profile(&self, device: &CameraDeviceId) -> String {
        self.lens_profiles.get(device).cloned().unwrap_or_else(|| DEFAULT_LENS_PROFILE.to_owned())
    }
}

impl AppController {
    /// The cameras that run: the strip's camera and every camera the workspace started.
    pub(super) fn running_cameras(&self) -> BTreeMap<CameraDeviceId, CameraMode> {
        let mut running = self.camera_ws.sources.clone();
        if let Some((device, mode)) = &self.camera_active {
            running.insert(device.clone(), *mode);
        }
        running
    }

    /// The configuration a running camera is in right now.
    pub(super) fn config_of(&self, device: &CameraDeviceId, mode: &CameraMode) -> CameraConfigKey {
        config_key(device, mode, &self.camera_ws.lens_profile(device))
    }

    pub(super) fn latest_frame_of(&self, device: &CameraDeviceId) -> Option<Arc<CameraFrame>> {
        self.camera.as_ref()?.latest_frame(device)
    }

    pub(super) fn workspace_update(&self) -> AppUpdate {
        let documents: Vec<_> = self.sessions.keys().copied().collect();
        AppUpdate {
            view_changed: true,
            effects: documents.iter().map(|d| super::HostEffect::Redraw { document: *d }).collect(),
            scenes_changed: documents,
        }
    }

    /// The pictures on the bed for the scene of every window (beneath the grid and the design).
    pub(super) fn camera_overlays(&self) -> Vec<Overlay> {
        let ws = &self.camera_ws;
        if !ws.visible || ws.opacity_percent <= 0.0 {
            return Vec::new();
        }
        ws.layers
            .iter()
            .filter(|l| l.not_placed.is_none() && !l.quads.is_empty())
            .map(|l| Overlay::BedImage {
                picture: Arc::clone(&l.picture),
                quads: Arc::clone(&l.quads),
                opacity: (ws.opacity_percent / 100.0) as f32,
            })
            .collect()
    }

    pub fn attach_calibrations(&mut self, store: CalibrationStore) {
        self.camera_ws.store = store;
        self.rebuild_layers();
    }

    /// The stored lens calibrations and workspace alignments (the host persists them).
    pub fn calibrations(&self) -> &CalibrationStore {
        &self.camera_ws.store
    }

    pub fn set_camera_spawner(&mut self, spawner: CameraSpawner) {
        self.camera_ws.spawner = spawner;
    }

    pub(super) fn camera_workspace(&mut self, intent: CameraWorkspaceIntent) -> AppUpdate {
        self.camera_ws.refusal = None;
        let result = match intent {
            CameraWorkspaceIntent::SetOverlayVisible(on) => {
                self.camera_ws.visible = on;
                Ok(())
            }
            CameraWorkspaceIntent::SetOverlayOpacity { percent } => {
                if percent.is_finite() && (0.0..=100.0).contains(&percent) {
                    self.camera_ws.opacity_percent = percent;
                    Ok(())
                } else {
                    Err(CameraWhy::InvalidNumber)
                }
            }
            CameraWorkspaceIntent::UpdateOverlay => self.update_overlay(),
            CameraWorkspaceIntent::SetMaterialHeight { mm } => typed_mm(&mm).map(|v| {
                self.camera_ws.height_mm = v;
                self.rebuild_layers();
            }),
            CameraWorkspaceIntent::SetEdgeMargin { px } => typed_mm(&px).map(|v| {
                self.camera_ws.edge_margin_px = v;
                self.rebuild_layers();
            }),
            CameraWorkspaceIntent::UseCamera { device, mode, lens } => self.use_camera(device, mode, lens),
            CameraWorkspaceIntent::DropCamera { device } => self.drop_camera(&device),
            CameraWorkspaceIntent::SetLensProfile { device, lens } => {
                self.camera_ws.lens_profiles.insert(device, lens);
                Ok(())
            }
            CameraWorkspaceIntent::DeleteCalibration { calibration } => self.delete_calibration(&calibration),
            CameraWorkspaceIntent::Recalibrate { calibration } => self.recalibrate(&calibration),
            other => self.camera_wizard(other),
        };
        if let Err(why) = result {
            self.camera_ws.refusal = Some(why);
        }
        self.workspace_update()
    }

    // --- cameras -----------------------------------------------------------------------------------

    fn use_camera(&mut self, device: CameraDeviceId, mode: CameraMode, lens: String) -> Result<(), CameraWhy> {
        let camera = self.camera.clone().ok_or(CameraWhy::NoCameraPort)?;
        if camera.authorization() != CameraAuthorization::Authorized {
            return Err(CameraWhy::CameraNotAuthorized);
        }
        if self.running_cameras().contains_key(&device) {
            camera.stop(&device);
        }
        if self.camera_active.as_ref().is_some_and(|(d, _)| *d == device) {
            self.camera_active = None;
        }
        camera.start(&device, &mode).map_err(|_| CameraWhy::CameraRefused)?;
        self.camera_ws.sources.insert(device.clone(), mode);
        self.camera_ws.lens_profiles.insert(device, lens);
        Ok(())
    }

    fn drop_camera(&mut self, device: &CameraDeviceId) -> Result<(), CameraWhy> {
        if !self.running_cameras().contains_key(device) {
            return Err(CameraWhy::CameraNotRunning);
        }
        if let Some(camera) = &self.camera {
            camera.stop(device);
        }
        self.camera_ws.sources.remove(device);
        if self.camera_active.as_ref().is_some_and(|(d, _)| d == device) {
            self.camera_active = None;
        }
        self.camera_ws.layers.retain(|l| l.device != *device);
        self.rebuild_layers();
        Ok(())
    }

    // --- the picture ------------------------------------------------------------------------------------

    /// "Update overlay": the latest frame of every running camera becomes its picture.
    fn update_overlay(&mut self) -> Result<(), CameraWhy> {
        let running = self.running_cameras();
        if running.is_empty() {
            return Err(CameraWhy::NothingToUpdate);
        }
        let mut layers = Vec::new();
        for device in running.keys() {
            let Some(frame) = self.latest_frame_of(device) else { continue };
            let Some(picture) = picture_of(&frame) else { continue };
            layers.push(Layer {
                device: device.clone(),
                config: self.config_of(device, &frame.mode),
                seq: frame.seq,
                picture: Arc::new(picture),
                quads: Arc::from(Vec::new()),
                not_placed: None,
            });
        }
        if layers.is_empty() {
            return Err(CameraWhy::NoFrame);
        }
        self.camera_ws.layers = layers;
        self.rebuild_layers();
        Ok(())
    }

    /// Places every picture again: after an update, a changed store, height or edge margin.
    pub(super) fn rebuild_layers(&mut self) {
        let ws = &mut self.camera_ws;
        let (store, height, margin) = (&ws.store, ws.height_mm, ws.edge_margin_px);
        let mut meshes: Vec<Result<Mesh, CameraWhy>> =
            ws.layers.iter().map(|l| mesh_of(store, &l.config, l.picture.width, l.picture.height, height)).collect();
        // Where cameras overlap, or an edge margin applies, each cell goes to the camera that sees it best.
        let placed: Vec<usize> = (0..ws.layers.len()).filter(|i| meshes[*i].is_ok()).collect();
        if placed.len() > 1 || margin > 0.0 {
            let alignments: Vec<Option<&calib::WorkspaceAlignment>> =
                ws.layers.iter().map(|l| nearest_alignment(store, &l.config, height)).collect();
            let views: Vec<(usize, CameraView<'_>)> = placed
                .iter()
                .filter_map(|&i| Some((i, CameraView { current: &ws.layers[i].config, alignment: alignments[i]? })))
                .collect();
            let all: Vec<CameraView<'_>> = views.iter().map(|(_, v)| v.clone()).collect();
            for &i in &placed {
                let device = &ws.layers[i].config.device;
                if let Ok(mesh) = &mut meshes[i] {
                    mesh.cells.retain(|q| {
                        let c = centre(q);
                        cameras_seeing([c.x, c.y], &all, margin).chosen().is_some_and(|s| s.camera == *device)
                    });
                }
            }
        }
        for (layer, mesh) in ws.layers.iter_mut().zip(meshes) {
            match mesh {
                Ok(m) => {
                    layer.quads = Arc::from(m.cells);
                    layer.not_placed = None;
                }
                Err(why) => {
                    layer.quads = Arc::from(Vec::new());
                    layer.not_placed = Some(why);
                }
            }
        }
    }

    // --- calibrations ---------------------------------------------------------------------------------------

    fn delete_calibration(&mut self, which: &CalibrationRef) -> Result<(), CameraWhy> {
        let store = &mut self.camera_ws.store;
        let removed = match which.kind {
            CalibrationKind::Lens => store.remove_lens(&which.config).is_some(),
            CalibrationKind::Alignment { height_um } => {
                store.remove_alignment(&which.config, height_um as f64 / 1000.0).is_some()
            }
        };
        if !removed {
            return Err(CameraWhy::NotCalibrated(StoreMiss::NotCalibrated));
        }
        self.rebuild_layers();
        Ok(())
    }

    /// Starts the wizard that makes `which` again; the camera must run in exactly its configuration.
    fn recalibrate(&mut self, which: &CalibrationRef) -> Result<(), CameraWhy> {
        let device = CameraDeviceId(which.config.device.0.clone());
        let running = self.running_cameras();
        let mode = *running.get(&device).ok_or(CameraWhy::CameraNotRunning)?;
        let current = self.config_of(&device, &mode);
        if config_validity(&which.config, &current).is_err() {
            return Err(CameraWhy::FrameConfigChanged);
        }
        match which.kind {
            CalibrationKind::Lens => self.start_lens_wizard(device, camera_wizards::default_grid()),
            CalibrationKind::Alignment { height_um } => self.start_alignment_wizard(device, height_um as f64 / 1000.0),
        }
    }

    // --- the view ---------------------------------------------------------------------------------------------

    /// The camera workspace; `None` without a camera port.
    pub fn camera_workspace_view(&self) -> Option<CameraWorkspaceView> {
        let camera = self.camera.as_ref()?;
        let ws = &self.camera_ws;
        let running = self.running_cameras();
        let names: BTreeMap<CameraDeviceId, String> =
            camera.devices().unwrap_or_default().into_iter().map(|d| (d.id, d.name)).collect();
        let cameras: Vec<CameraInUse> = running
            .iter()
            .map(|(device, mode)| CameraInUse {
                device: device.clone(),
                name: names.get(device).cloned().unwrap_or_else(|| device.0.clone()),
                mode: *mode,
                lens: ws.lens_profile(device),
            })
            .collect();
        let validity = |made: &CameraConfigKey| {
            let device = CameraDeviceId(made.device.0.clone());
            match running.get(&device) {
                None => CalibrationValidity::CameraNotInUse,
                Some(mode) => match config_validity(made, &self.config_of(&device, mode)) {
                    Ok(()) => CalibrationValidity::Valid,
                    Err(why) => CalibrationValidity::Invalid(why),
                },
            }
        };
        let row = |config: &CameraConfigKey, kind, summary| CalibrationRow {
            reference: CalibrationRef { config: config.clone(), kind },
            device: CameraDeviceId(config.device.0.clone()),
            width: config.mode.width,
            height: config.mode.height,
            frame_rate_mhz: config.mode.frame_rate_mhz,
            lens: config.lens.clone(),
            validity: validity(config),
            summary,
        };
        let mut calibrations: Vec<CalibrationRow> = ws
            .store
            .lenses()
            .map(|l| {
                let summary =
                    CalibrationSummary::Lens { k1: l.intrinsics.k1, k2: l.intrinsics.k2, rms_px: l.report.rms_px };
                row(&l.valid_for, CalibrationKind::Lens, summary)
            })
            .collect();
        calibrations.extend(ws.store.alignments().map(|a| {
            let summary = CalibrationSummary::Alignment {
                material_height_mm: a.material_height_mm,
                check_rms_mm: a.calibration.residuals_mm.rms_mm,
                check_max_mm: a.calibration.residuals_mm.max_mm,
            };
            let height_um = (a.material_height_mm * 1000.0).round() as i64;
            row(a.config(), CalibrationKind::Alignment { height_um }, summary)
        }));
        let update_why_not = running.is_empty().then_some(CameraWhy::NothingToUpdate);
        Some(CameraWorkspaceView {
            overlay: OverlayView {
                visible: ws.visible,
                opacity_percent: ws.opacity_percent,
                edge_margin_px: ws.edge_margin_px,
                layers: ws
                    .layers
                    .iter()
                    .map(|l| OverlayLayerView {
                        device: l.device.clone(),
                        captured_seq: l.seq,
                        cells: l.quads.len(),
                        not_placed: l.not_placed.clone(),
                    })
                    .collect(),
                can_update: update_why_not.is_none(),
                update_why_not,
                note: MessageKey::VisualPlacementOnly,
            },
            material_height_mm: ws.height_mm,
            cameras,
            calibrations,
            lens: self.lens_wizard_view(),
            alignment: self.alignment_view(),
            busy: self.camera_work_busy(),
            last_refusal: ws.refusal.clone(),
        })
    }
}

/// A typed number of millimetres/pixels: finite, from 0 to a sane maximum.
pub(super) fn typed_mm(text: &str) -> Result<f64, CameraWhy> {
    match parse_number(text, &["mm", "px"]) {
        Ok(v) if v.is_finite() && (0.0..=MAX_TYPED).contains(&v) => Ok(v),
        _ => Err(CameraWhy::InvalidNumber),
    }
}

/// The RGBA picture of a frame; its id changes with the frame so a renderer keeps unchanged textures.
fn picture_of(frame: &CameraFrame) -> Option<BedPicture> {
    let image: &SourceImage = &frame.image;
    let (w, h) = (image.width(), image.height());
    let rgba: Arc<[u8]> = if image.format() == PixelFormat::Rgba8 {
        Arc::from(image.samples())
    } else {
        let mut out = Vec::with_capacity(w as usize * h as usize * 4);
        for y in 0..h {
            for x in 0..w {
                let p = image.rgba_at(x, y);
                out.extend(p.map(|v| v.round().clamp(0.0, 255.0) as u8));
            }
        }
        Arc::from(out)
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (&frame.device, frame.seq).hash(&mut hasher);
    BedPicture::new(hasher.finish(), w, h, rgba)
}

struct Mesh {
    cells: Vec<BedQuad>,
}

fn centre(q: &BedQuad) -> Point {
    let sum = q.bed_mm.iter().fold((0.0, 0.0), |a, p| (a.0 + p.x, a.1 + p.y));
    Point::new(sum.0 / 4.0, sum.1 / 4.0)
}

/// The alignment to use for sighting tests: the one at exactly `height_mm`, else the nearest height.
fn nearest_alignment<'a>(
    store: &'a CalibrationStore,
    cfg: &CameraConfigKey,
    height_mm: f64,
) -> Option<&'a calib::WorkspaceAlignment> {
    store
        .alignments_for_config(cfg)
        .into_iter()
        .min_by(|a, b| (a.material_height_mm - height_mm).abs().total_cmp(&(b.material_height_mm - height_mm).abs()))
}

/// The mesh that lays a `width × height` picture on the bed: every cell corner is projected with the
/// stored alignment(s) for the material height. A cell whose corners do not all land on the bed, or
/// whose corners fold over each other (a view beyond the horizon), is left out.
fn mesh_of(
    store: &CalibrationStore,
    cfg: &CameraConfigKey,
    width: u32,
    height: u32,
    material_mm: f64,
) -> Result<Mesh, CameraWhy> {
    let (w, h) = (f64::from(width), f64::from(height));
    let at = |col: usize, row: usize| -> Result<Option<Point>, CameraWhy> {
        let pixel = [w * col as f64 / MESH_COLS as f64, h * row as f64 / MESH_ROWS as f64];
        match store.pixel_to_bed_at_height(cfg, pixel, material_mm) {
            Ok(p) if p.iter().all(|v| v.is_finite()) => Ok(Some(Point::new(p[0], p[1]))),
            Ok(_) | Err(HeightLookupError::Unmappable) => Ok(None),
            Err(HeightLookupError::Miss(StoreMiss::NoAlignmentForHeight { available_mm })) => {
                Err(CameraWhy::NotCalibrated(StoreMiss::NoAlignmentForHeight { available_mm }))
            }
            Err(HeightLookupError::Miss(miss)) => Err(CameraWhy::NotCalibrated(miss)),
            Err(HeightLookupError::OutsideCalibratedRange { .. }) => {
                let available_mm = store.alignments_for_config(cfg).iter().map(|a| a.material_height_mm).collect();
                Err(CameraWhy::NotCalibrated(StoreMiss::NoAlignmentForHeight { available_mm }))
            }
        }
    };
    let mut grid: Vec<Vec<Option<Point>>> = Vec::with_capacity(MESH_ROWS + 1);
    for row in 0..=MESH_ROWS {
        let mut line = Vec::with_capacity(MESH_COLS + 1);
        for col in 0..=MESH_COLS {
            line.push(at(col, row)?);
        }
        grid.push(line);
    }
    let mut cells = Vec::new();
    for row in 0..MESH_ROWS {
        for col in 0..MESH_COLS {
            let corners = [grid[row][col], grid[row][col + 1], grid[row + 1][col + 1], grid[row + 1][col]];
            let [Some(a), Some(b), Some(c), Some(d)] = corners else { continue };
            if !is_convex(&[a, b, c, d]) {
                continue;
            }
            let (u0, u1) = (col as f32 / MESH_COLS as f32, (col + 1) as f32 / MESH_COLS as f32);
            let (v0, v1) = (row as f32 / MESH_ROWS as f32, (row + 1) as f32 / MESH_ROWS as f32);
            cells.push(BedQuad { bed_mm: [a, b, c, d], uv: [[u0, v0], [u1, v0], [u1, v1], [u0, v1]] });
        }
    }
    Ok(Mesh { cells })
}

/// All four turns of the polygon go the same way (and none is flat): no fold, no twist.
fn is_convex(p: &[Point; 4]) -> bool {
    let turn = |a: Point, b: Point, c: Point| (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x);
    let turns = [turn(p[0], p[1], p[2]), turn(p[1], p[2], p[3]), turn(p[2], p[3], p[0]), turn(p[3], p[0], p[1])];
    turns.iter().all(|t| *t > 0.0) || turns.iter().all(|t| *t < 0.0)
}
