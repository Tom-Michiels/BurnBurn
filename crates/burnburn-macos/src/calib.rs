//! Camera-to-bed calibration (IM27), pure and deterministic: no camera, no clock, no threads.
//!
//! Model: an observed pixel is first undistorted with a one-parameter radial lens model around
//! the image centre, `p_d = c + (p_u − c)(1 + k1·r²)` with `r = |p_u − c| / half-diagonal`, then a
//! planar homography maps the undistorted pixel to bed millimetres. The homography is fitted with
//! the normalised DLT (Hartley) on the *fit* marks only; `k1` is either given by the lens profile
//! or estimated by a bounded 1-D search on the fit residual. Residuals are reported separately
//! for the independent *check* marks, which never influence the fit (W05).
//!
//! A calibration is valid only for exactly the camera configuration it was made with (device,
//! resolution, pixel format, lens profile): `calibration_is_invalid_after_camera_resolution_changes`.
//! Camera pixels are a placement aid, never CAM input without an explicit user placement.
//!
//! Interim home: contracts-phase-c §IM27 places this in `burnburn-image::calib` (owner Trace); it
//! has no native dependency so it can move there unchanged with its tests.

/// Stable identity of a camera as the OS reports it (AVFoundation `uniqueID`, MF symbolic link).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CameraDeviceId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CameraPixelFormat {
    Bgra8,
    /// 4:2:0 bi-planar video range (`420v`), the usual webcam default.
    Yuv420Video,
    /// Any other FourCC.
    Other(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CameraMode {
    pub width: u32,
    pub height: u32,
    pub pixel_format: CameraPixelFormat,
}

/// What a calibration is bound to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CameraConfigKey {
    pub device: CameraDeviceId,
    pub mode: CameraMode,
    /// Lens / mounting profile chosen by the user (a changed lens invalidates the calibration).
    pub lens: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkRole {
    /// Used to fit the model.
    Fit,
    /// Independent: only measured against the fitted model.
    Check,
}

/// A mark the user placed: where it is in the camera image and where it is on the bed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkObservation {
    pub pixel: [f64; 2],
    pub bed_mm: [f64; 2],
    pub role: MarkRole,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LensModel {
    /// Radial coefficient from a lens profile (0 = ideal lens).
    Fixed { k1: f64 },
    /// Estimate `k1` from the fit marks (needs one mark more than the homography).
    EstimateRadial,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CalibParams {
    pub lens: LensModel,
    /// A fit mark further off than this means the marks contradict each other: no calibration.
    pub max_fit_residual_mm: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Intrinsics {
    pub center_px: [f64; 2],
    /// Normalising radius (half the image diagonal).
    pub radius_px: f64,
    pub k1: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResidualStats {
    pub count: usize,
    pub rms_mm: f64,
    pub max_mm: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CheckPoint {
    pub pixel: [f64; 2],
    pub bed_mm: [f64; 2],
    /// Distance between the mapped pixel and the measured bed position.
    pub error_mm: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CameraCalibration {
    pub valid_for: CameraConfigKey,
    pub intrinsics: Intrinsics,
    /// Undistorted pixel (homogeneous) → bed mm, `[2][2] = 1`.
    pub bed_homography: [[f64; 3]; 3],
    pub fit_residuals_mm: ResidualStats,
    /// Residuals on the independent check marks.
    pub residuals_mm: ResidualStats,
    pub check_points: Vec<CheckPoint>,
}

/// Why a calibration does not apply to the current camera configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invalidity {
    DeviceChanged,
    ResolutionChanged,
    PixelFormatChanged,
    LensChanged,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CalibError {
    TooFewFitPoints {
        need: usize,
        got: usize,
    },
    /// Residuals must be measured on marks that were not fitted.
    NoCheckPoints,
    NonFinite,
    /// The fit marks lie (almost) on one line, in the image or on the bed.
    Collinear,
    /// The equations have no unique solution (e.g. three of four marks on a line).
    Degenerate,
    /// The same image position was given two different bed positions.
    ConflictingMeasurements,
    /// The model cannot explain the fit marks within the allowed residual.
    Inconsistent {
        max_fit_residual_mm: f64,
    },
    OutsideImage,
    /// The image size in the configuration is zero.
    BadMode,
}

/// Search interval and budget for the radial coefficient (deterministic, iteration-bounded).
const K1_RANGE: (f64, f64) = (-0.5, 0.5);
const K1_GRID: usize = 40;
const K1_GOLDEN_ITERATIONS: usize = 80;
const UNDISTORT_ITERATIONS: usize = 30;
/// Two marks closer than this (px) are the same image position.
const SAME_PIXEL_PX: f64 = 0.5;

pub fn calibrate(
    observations: &[MarkObservation],
    cfg: CameraConfigKey,
    params: CalibParams,
) -> Result<CameraCalibration, CalibError> {
    let (w, h) = (f64::from(cfg.mode.width), f64::from(cfg.mode.height));
    if w <= 0.0 || h <= 0.0 {
        return Err(CalibError::BadMode);
    }
    if !(params.max_fit_residual_mm.is_finite() && params.max_fit_residual_mm >= 0.0) {
        return Err(CalibError::NonFinite);
    }
    if let LensModel::Fixed { k1 } = params.lens
        && !k1.is_finite()
    {
        return Err(CalibError::NonFinite);
    }
    if observations.iter().any(|o| !(finite(o.pixel) && finite(o.bed_mm))) {
        return Err(CalibError::NonFinite);
    }
    if observations.iter().any(|o| o.pixel[0] < 0.0 || o.pixel[1] < 0.0 || o.pixel[0] > w || o.pixel[1] > h) {
        return Err(CalibError::OutsideImage);
    }
    for (i, a) in observations.iter().enumerate() {
        for b in &observations[i + 1..] {
            if dist(a.pixel, b.pixel) < SAME_PIXEL_PX && dist(a.bed_mm, b.bed_mm) > params.max_fit_residual_mm {
                return Err(CalibError::ConflictingMeasurements);
            }
        }
    }
    let fit: Vec<MarkObservation> = observations.iter().filter(|o| o.role == MarkRole::Fit).copied().collect();
    let check: Vec<MarkObservation> = observations.iter().filter(|o| o.role == MarkRole::Check).copied().collect();
    let need = match params.lens {
        LensModel::Fixed { .. } => 4,
        LensModel::EstimateRadial => 5,
    };
    if fit.len() < need {
        return Err(CalibError::TooFewFitPoints { need, got: fit.len() });
    }
    if check.is_empty() {
        return Err(CalibError::NoCheckPoints);
    }
    let pixels: Vec<[f64; 2]> = fit.iter().map(|o| o.pixel).collect();
    let beds: Vec<[f64; 2]> = fit.iter().map(|o| o.bed_mm).collect();
    if collinear(&pixels) || collinear(&beds) {
        return Err(CalibError::Collinear);
    }

    let center = [w / 2.0, h / 2.0];
    let radius = (center[0] * center[0] + center[1] * center[1]).sqrt();
    let lens = Lens { center, radius };
    let k1 = match params.lens {
        LensModel::Fixed { k1 } => k1,
        LensModel::EstimateRadial => estimate_k1(&lens, &fit)?,
    };
    let homography = fit_homography(&lens, k1, &fit).ok_or(CalibError::Degenerate)?;
    let fit_errors: Vec<f64> = fit.iter().map(|o| error_mm(&lens, k1, &homography, o)).collect();
    let fit_stats = stats(&fit_errors).ok_or(CalibError::Degenerate)?;
    if fit_stats.max_mm > params.max_fit_residual_mm {
        return Err(CalibError::Inconsistent { max_fit_residual_mm: fit_stats.max_mm });
    }
    let check_points: Vec<CheckPoint> = check
        .iter()
        .map(|o| CheckPoint { pixel: o.pixel, bed_mm: o.bed_mm, error_mm: error_mm(&lens, k1, &homography, o) })
        .collect();
    let check_errors: Vec<f64> = check_points.iter().map(|c| c.error_mm).collect();
    let residuals = stats(&check_errors).ok_or(CalibError::Degenerate)?;
    Ok(CameraCalibration {
        valid_for: cfg,
        intrinsics: Intrinsics { center_px: center, radius_px: radius, k1 },
        bed_homography: homography,
        fit_residuals_mm: fit_stats,
        residuals_mm: residuals,
        check_points,
    })
}

impl CameraCalibration {
    /// `Ok` only for exactly the configuration the calibration was made with.
    pub fn validity(&self, current: &CameraConfigKey) -> Result<(), Invalidity> {
        let made = &self.valid_for;
        if made.device != current.device {
            return Err(Invalidity::DeviceChanged);
        }
        if (made.mode.width, made.mode.height) != (current.mode.width, current.mode.height) {
            return Err(Invalidity::ResolutionChanged);
        }
        if made.mode.pixel_format != current.mode.pixel_format {
            return Err(Invalidity::PixelFormatChanged);
        }
        if made.lens != current.lens {
            return Err(Invalidity::LensChanged);
        }
        Ok(())
    }

    /// Bed position (mm) of an image pixel, refused when the camera configuration changed.
    pub fn pixel_to_bed(&self, current: &CameraConfigKey, pixel: [f64; 2]) -> Result<[f64; 2], Invalidity> {
        self.validity(current)?;
        let lens = Lens { center: self.intrinsics.center_px, radius: self.intrinsics.radius_px };
        let u = lens.undistort(pixel, self.intrinsics.k1);
        Ok(apply(&self.bed_homography, u).unwrap_or([f64::NAN, f64::NAN]))
    }
}

struct Lens {
    center: [f64; 2],
    radius: f64,
}

impl Lens {
    /// Inverse of the radial model by fixed-point iteration (converges for |k1·r²| < 1).
    fn undistort(&self, p: [f64; 2], k1: f64) -> [f64; 2] {
        let d = [p[0] - self.center[0], p[1] - self.center[1]];
        let mut u = d;
        for _ in 0..UNDISTORT_ITERATIONS {
            let r2 = (u[0] * u[0] + u[1] * u[1]) / (self.radius * self.radius);
            let f = 1.0 + k1 * r2;
            if f.abs() < 1e-9 {
                break;
            }
            u = [d[0] / f, d[1] / f];
        }
        [self.center[0] + u[0], self.center[1] + u[1]]
    }
}

fn finite(p: [f64; 2]) -> bool {
    p[0].is_finite() && p[1].is_finite()
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// Points on one line: the smaller principal axis of their spread is negligible.
fn collinear(points: &[[f64; 2]]) -> bool {
    let n = points.len() as f64;
    let mean = points.iter().fold([0.0, 0.0], |a, p| [a[0] + p[0] / n, a[1] + p[1] / n]);
    let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
    for p in points {
        let (dx, dy) = (p[0] - mean[0], p[1] - mean[1]);
        sxx += dx * dx;
        syy += dy * dy;
        sxy += dx * dy;
    }
    let trace = sxx + syy;
    if trace <= 0.0 {
        return true;
    }
    let det = sxx * syy - sxy * sxy;
    let disc = ((trace * trace) / 4.0 - det).max(0.0).sqrt();
    let small = trace / 2.0 - disc;
    small / trace < 1e-10
}

fn apply(h: &[[f64; 3]; 3], p: [f64; 2]) -> Option<[f64; 2]> {
    let w = h[2][0] * p[0] + h[2][1] * p[1] + h[2][2];
    if w.abs() < 1e-15 {
        return None;
    }
    Some([(h[0][0] * p[0] + h[0][1] * p[1] + h[0][2]) / w, (h[1][0] * p[0] + h[1][1] * p[1] + h[1][2]) / w])
}

fn error_mm(lens: &Lens, k1: f64, h: &[[f64; 3]; 3], o: &MarkObservation) -> f64 {
    apply(h, lens.undistort(o.pixel, k1)).map_or(f64::INFINITY, |b| dist(b, o.bed_mm))
}

fn stats(errors: &[f64]) -> Option<ResidualStats> {
    if errors.iter().any(|e| !e.is_finite()) {
        return None;
    }
    let n = errors.len();
    let rms = if n == 0 { 0.0 } else { (errors.iter().map(|e| e * e).sum::<f64>() / n as f64).sqrt() };
    Some(ResidualStats { count: n, rms_mm: rms, max_mm: errors.iter().copied().fold(0.0, f64::max) })
}

/// Hartley normalisation: centroid to the origin, mean distance √2. Returns `T` and `T⁻¹`.
fn normaliser(points: &[[f64; 2]]) -> Option<([[f64; 3]; 3], [[f64; 3]; 3])> {
    let n = points.len() as f64;
    let mean = points.iter().fold([0.0, 0.0], |a, p| [a[0] + p[0] / n, a[1] + p[1] / n]);
    let spread = points.iter().map(|p| dist(*p, mean)).sum::<f64>() / n;
    if !(spread.is_finite() && spread > 0.0) {
        return None;
    }
    let s = std::f64::consts::SQRT_2 / spread;
    let t = [[s, 0.0, -s * mean[0]], [0.0, s, -s * mean[1]], [0.0, 0.0, 1.0]];
    let inv = [[1.0 / s, 0.0, mean[0]], [0.0, 1.0 / s, mean[1]], [0.0, 0.0, 1.0]];
    Some((t, inv))
}

fn mul(a: &[[f64; 3]; 3], b: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut m = [[0.0; 3]; 3];
    for (i, row) in m.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    m
}

/// Normalised DLT with `h33 = 1`, least squares through the normal equations.
fn fit_homography(lens: &Lens, k1: f64, fit: &[MarkObservation]) -> Option<[[f64; 3]; 3]> {
    let src: Vec<[f64; 2]> = fit.iter().map(|o| lens.undistort(o.pixel, k1)).collect();
    let dst: Vec<[f64; 2]> = fit.iter().map(|o| o.bed_mm).collect();
    let (ts, _) = normaliser(&src)?;
    let (td, td_inv) = normaliser(&dst)?;
    let mut ata = [[0.0f64; 8]; 8];
    let mut atb = [0.0f64; 8];
    for (s, d) in src.iter().zip(&dst) {
        let x = apply(&ts, *s)?;
        let u = apply(&td, *d)?;
        let rows = [
            ([x[0], x[1], 1.0, 0.0, 0.0, 0.0, -x[0] * u[0], -x[1] * u[0]], u[0]),
            ([0.0, 0.0, 0.0, x[0], x[1], 1.0, -x[0] * u[1], -x[1] * u[1]], u[1]),
        ];
        for (row, rhs) in rows {
            for i in 0..8 {
                for j in 0..8 {
                    ata[i][j] += row[i] * row[j];
                }
                atb[i] += row[i] * rhs;
            }
        }
    }
    let hv = solve8(ata, atb)?;
    let hn = [[hv[0], hv[1], hv[2]], [hv[3], hv[4], hv[5]], [hv[6], hv[7], 1.0]];
    let h = mul(&td_inv, &mul(&hn, &ts));
    let scale = h[2][2];
    if !(scale.is_finite() && scale.abs() > 1e-15) {
        return None;
    }
    let h = h.map(|row| row.map(|v| v / scale));
    h.iter().flatten().all(|v| v.is_finite()).then_some(h)
}

/// Gaussian elimination with partial pivoting; `None` when (numerically) singular.
fn solve8(mut a: [[f64; 8]; 8], mut b: [f64; 8]) -> Option<[f64; 8]> {
    let scale = a.iter().flatten().fold(0.0f64, |m, v| m.max(v.abs()));
    if scale <= 0.0 {
        return None;
    }
    for col in 0..8 {
        let pivot = (col..8).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[pivot][col].abs() < 1e-12 * scale {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        for row in col + 1..8 {
            let f = a[row][col] / a[col][col];
            let pivot_row = a[col];
            for (v, p) in a[row].iter_mut().zip(pivot_row).skip(col) {
                *v -= f * p;
            }
            b[row] -= f * b[col];
        }
    }
    let mut x = [0.0f64; 8];
    for row in (0..8).rev() {
        let sum: f64 = (row + 1..8).map(|k| a[row][k] * x[k]).sum();
        x[row] = (b[row] - sum) / a[row][row];
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}

/// RMS fit residual for a candidate `k1` (infinite when the fit is degenerate).
fn fit_cost(lens: &Lens, k1: f64, fit: &[MarkObservation]) -> f64 {
    let Some(h) = fit_homography(lens, k1, fit) else { return f64::INFINITY };
    let errors: Vec<f64> = fit.iter().map(|o| error_mm(lens, k1, &h, o)).collect();
    stats(&errors).map_or(f64::INFINITY, |s| s.rms_mm)
}

/// Grid search over [`K1_RANGE`], then golden-section refinement around the best grid cell.
fn estimate_k1(lens: &Lens, fit: &[MarkObservation]) -> Result<f64, CalibError> {
    let (lo, hi) = K1_RANGE;
    let step = (hi - lo) / K1_GRID as f64;
    let mut best = (f64::INFINITY, 0.0);
    for i in 0..=K1_GRID {
        let k = lo + step * i as f64;
        let cost = fit_cost(lens, k, fit);
        if cost < best.0 {
            best = (cost, k);
        }
    }
    if !best.0.is_finite() {
        return Err(CalibError::Degenerate);
    }
    let (mut a, mut b) = ((best.1 - step).max(lo), (best.1 + step).min(hi));
    let ratio = (5.0f64.sqrt() - 1.0) / 2.0;
    let mut c = b - ratio * (b - a);
    let mut d = a + ratio * (b - a);
    let (mut fc, mut fd) = (fit_cost(lens, c, fit), fit_cost(lens, d, fit));
    for _ in 0..K1_GOLDEN_ITERATIONS {
        if fc < fd {
            b = d;
            d = c;
            fd = fc;
            c = b - ratio * (b - a);
            fc = fit_cost(lens, c, fit);
        } else {
            a = c;
            c = d;
            fc = fd;
            d = a + ratio * (b - a);
            fd = fit_cost(lens, d, fit);
        }
    }
    Ok((a + b) / 2.0)
}
