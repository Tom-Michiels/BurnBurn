//! Camera-workspace fixtures (W3-UI-camera): a fake `CameraPort` that serves the frames a test
//! gives it, synthetic bed mappings with known answers, and a rendered checkerboard. The real
//! AVFoundation adapter is never used; nothing here asks for camera access.

#![allow(dead_code, clippy::expect_used, clippy::unwrap_used, clippy::panic)] // shared test helpers

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use burnburn_image::SourceImage;
use burnburn_image::calib::{
    AlignmentLens, LensModel, MarkObservation, MarkRole, WorkspaceAlignment,
};
use burnburn_presentation::{
    CameraAuthorization, CameraDevice, CameraDeviceId, CameraError, CameraFrame, CameraMode, CameraPixelFormat,
    CameraPort, config_key,
};
use burnburn_types::MonotonicTime;

pub fn dev(name: &str) -> CameraDeviceId {
    CameraDeviceId(name.to_owned())
}

pub fn mode(width: u32, height: u32) -> CameraMode {
    CameraMode { width, height, fps_milli: 30_000, pixel_format: CameraPixelFormat::Bgra8 }
}

#[derive(Default)]
pub struct FakeCam {
    pub authorization: Mutex<Option<CameraAuthorization>>,
    /// How often the OS prompt was asked for.
    pub access_requests: Mutex<u32>,
    pub started: Mutex<Vec<(CameraDeviceId, CameraMode)>>,
    pub stopped: Mutex<Vec<CameraDeviceId>>,
    devices: Mutex<Vec<CameraDevice>>,
    frames: Mutex<BTreeMap<CameraDeviceId, Arc<CameraFrame>>>,
    seq: Mutex<u64>,
}

impl FakeCam {
    pub fn new(devices: &[(&str, &[CameraMode])]) -> Arc<FakeCam> {
        let cam = FakeCam::default();
        *cam.devices.lock().unwrap() = devices
            .iter()
            .map(|(name, modes)| CameraDevice { id: dev(name), name: format!("Camera {name}"), modes: modes.to_vec() })
            .collect();
        Arc::new(cam)
    }

    pub fn authorize(&self) {
        *self.authorization.lock().unwrap() = Some(CameraAuthorization::Authorized);
    }

    /// The camera delivers `image` as its newest frame, taken in `mode`.
    pub fn show(&self, device: &str, mode: CameraMode, image: SourceImage) {
        let mut seq = self.seq.lock().unwrap();
        *seq += 1;
        let frame = CameraFrame { device: dev(device), mode, seq: *seq, image, time: MonotonicTime::ZERO };
        self.frames.lock().unwrap().insert(dev(device), Arc::new(frame));
    }
}

impl CameraPort for FakeCam {
    fn devices(&self) -> Result<Vec<CameraDevice>, CameraError> {
        Ok(self.devices.lock().unwrap().clone())
    }
    fn authorization(&self) -> CameraAuthorization {
        self.authorization.lock().unwrap().unwrap_or(CameraAuthorization::NotDetermined)
    }
    fn request_authorization(&self) {
        *self.access_requests.lock().unwrap() += 1;
    }
    fn start(&self, d: &CameraDeviceId, m: &CameraMode) -> Result<(), CameraError> {
        self.started.lock().unwrap().push((d.clone(), *m));
        Ok(())
    }
    fn stop(&self, d: &CameraDeviceId) {
        self.stopped.lock().unwrap().push(d.clone());
    }
    fn latest_frame(&self, d: &CameraDeviceId) -> Option<Arc<CameraFrame>> {
        self.frames.lock().unwrap().get(d).cloned()
    }
}

pub fn grey(width: u32, height: u32, value: u8) -> SourceImage {
    let mut rgba = vec![value; (width * height * 4) as usize];
    for px in rgba.chunks_exact_mut(4) {
        px[3] = 255;
    }
    SourceImage::from_rgba8(width, height, rgba).unwrap()
}

/// A camera that looks straight down: `bed = (ox + s·px, oy − s·py)` (millimetres, y up). A known
/// answer for every pixel, so placements can be checked without the calibration code.
#[derive(Debug, Clone, Copy)]
pub struct Mapping {
    pub mm_per_px: f64,
    pub origin_mm: [f64; 2],
}

impl Mapping {
    pub fn bed(&self, p: [f64; 2]) -> [f64; 2] {
        [self.origin_mm[0] + self.mm_per_px * p[0], self.origin_mm[1] - self.mm_per_px * p[1]]
    }

    pub fn pixel(&self, bed: [f64; 2]) -> [f64; 2] {
        [(bed[0] - self.origin_mm[0]) / self.mm_per_px, (self.origin_mm[1] - bed[1]) / self.mm_per_px]
    }
}

/// An alignment of `device` in `mode` (lens profile "standard") at `height_mm`, fitted from marks
/// that follow `map` exactly: four fit marks and two independent check marks.
pub fn aligned(device: &str, mode: CameraMode, height_mm: f64, map: Mapping) -> WorkspaceAlignment {
    let (w, h) = (f64::from(mode.width), f64::from(mode.height));
    let mark = |fx: f64, fy: f64, role| {
        let pixel = [w * fx, h * fy];
        MarkObservation { pixel, bed_mm: map.bed(pixel), role }
    };
    let marks = vec![
        mark(0.1, 0.1, MarkRole::Fit),
        mark(0.9, 0.1, MarkRole::Fit),
        mark(0.9, 0.9, MarkRole::Fit),
        mark(0.1, 0.9, MarkRole::Fit),
        mark(0.5, 0.5, MarkRole::Check),
        mark(0.3, 0.7, MarkRole::Check),
    ];
    WorkspaceAlignment::align(
        &marks,
        config_key(&dev(device), &mode, "standard"),
        AlignmentLens::Profile(LensModel::Fixed { k1: 0.0 }),
        height_mm,
        0.5,
    )
    .unwrap()
}

// ---------------------------------------------------------------------------------------------
// A rendered checkerboard (independent forward model; see burnburn-image/tests/calib_corners.rs)
// ---------------------------------------------------------------------------------------------

pub const BOARD_W: u32 = 640;
pub const BOARD_H: u32 = 360;
pub const BOARD_COLS: usize = 9;
pub const BOARD_ROWS: usize = 6;
pub const BOARD_PITCH: f64 = 30.0;

type Mat = [[f64; 3]; 3];

fn distort(u: [f64; 2], k1: f64) -> [f64; 2] {
    let c = [f64::from(BOARD_W) / 2.0, f64::from(BOARD_H) / 2.0];
    let radius = c[0].hypot(c[1]);
    let d = [u[0] - c[0], u[1] - c[1]];
    let r2 = (d[0] * d[0] + d[1] * d[1]) / (radius * radius);
    [c[0] + d[0] * (1.0 + k1 * r2), c[1] + d[1] * (1.0 + k1 * r2)]
}

fn undistort(p: [f64; 2], k1: f64) -> [f64; 2] {
    let c = [f64::from(BOARD_W) / 2.0, f64::from(BOARD_H) / 2.0];
    let radius = c[0].hypot(c[1]);
    let d = [p[0] - c[0], p[1] - c[1]];
    let mut u = d;
    for _ in 0..40 {
        let r2 = (u[0] * u[0] + u[1] * u[1]) / (radius * radius);
        u = [d[0] / (1.0 + k1 * r2), d[1] / (1.0 + k1 * r2)];
    }
    [c[0] + u[0], c[1] + u[1]]
}

fn project(g: &Mat, p: [f64; 2]) -> [f64; 2] {
    let w = g[2][0] * p[0] + g[2][1] * p[1] + g[2][2];
    [(g[0][0] * p[0] + g[0][1] * p[1] + g[0][2]) / w, (g[1][0] * p[0] + g[1][1] * p[1] + g[1][2]) / w]
}

fn inverse(g: &Mat) -> Mat {
    let det = g[0][0] * (g[1][1] * g[2][2] - g[1][2] * g[2][1]) - g[0][1] * (g[1][0] * g[2][2] - g[1][2] * g[2][0])
        + g[0][2] * (g[1][0] * g[2][1] - g[1][1] * g[2][0]);
    let c = |r: usize, s: usize| {
        let (r1, r2, s1, s2) = ((r + 1) % 3, (r + 2) % 3, (s + 1) % 3, (s + 2) % 3);
        g[r1][s1] * g[r2][s2] - g[r1][s2] * g[r2][s1]
    };
    let mut out = [[0.0; 3]; 3];
    for (r, row) in out.iter_mut().enumerate() {
        for (s, v) in row.iter_mut().enumerate() {
            *v = c(s, r) / det;
        }
    }
    out
}

fn board_grey(mm: [f64; 2]) -> f64 {
    let (a, b) = ((mm[0] / BOARD_PITCH).floor(), (mm[1] / BOARD_PITCH).floor());
    if !(a >= -1.0 && a <= BOARD_COLS as f64 - 1.0 && b >= -1.0 && b <= BOARD_ROWS as f64 - 1.0) {
        return 255.0;
    }
    if (a + b).rem_euclid(2.0) == 0.0 { 20.0 } else { 235.0 }
}

/// The three places the board is photographed from (millimetres of the board to undistorted pixels).
pub const POSES: [Mat; 3] = [
    [[1.6, 0.06667, 26.66667], [-0.03333, 1.53333, 40.0], [0.000_05, 0.000_03, 1.0]],
    [[1.53333, -0.06667, 266.66667], [0.04, 1.46667, 133.33333], [-0.000_04, 0.000_06, 1.0]],
    [[1.66667, 0.13333, 133.33333], [0.06667, 1.46667, 80.0], [0.000_03, -0.000_04, 1.0]],
];

/// A photo of the printed board through a lens with radial coefficient `k1`.
pub fn photo_of_board(pose: &Mat, k1: f64) -> SourceImage {
    let inv = inverse(pose);
    let mut rgba = Vec::with_capacity((BOARD_W * BOARD_H * 4) as usize);
    for y in 0..BOARD_H {
        for x in 0..BOARD_W {
            let sample = |sx: f64, sy: f64| {
                let u = undistort([f64::from(x) + sx, f64::from(y) + sy], k1);
                board_grey(project(&inv, u))
            };
            let corners = [sample(0.0, 0.0), sample(1.0, 0.0), sample(0.0, 1.0), sample(1.0, 1.0)];
            let grey = if corners.iter().all(|c| *c == corners[0]) && sample(0.5, 0.5) == corners[0] {
                corners[0]
            } else {
                let mut sum = 0.0;
                for i in 0..4 {
                    for j in 0..4 {
                        sum += sample((f64::from(i) + 0.5) / 4.0, (f64::from(j) + 0.5) / 4.0);
                    }
                }
                sum / 16.0
            };
            let g = grey.round().clamp(0.0, 255.0) as u8;
            rgba.extend_from_slice(&[g, g, g, 255]);
        }
    }
    SourceImage::from_rgba8(BOARD_W, BOARD_H, rgba).unwrap()
}

/// Where the lattice corners of the board are in the photo (for checking placement).
pub fn board_corner(pose: &Mat, k1: f64, col: usize, row: usize) -> [f64; 2] {
    distort(project(pose, [col as f64 * BOARD_PITCH, row as f64 * BOARD_PITCH]), k1)
}
