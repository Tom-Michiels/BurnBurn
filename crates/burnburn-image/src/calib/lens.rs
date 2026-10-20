//! Lens calibration from a printed pattern (W2-L5e).

use burnburn_types::{CancellationToken, Cancelled, RequestId};

use super::{CameraConfigKey, Intrinsics, Lens, Radial, collinear, dist, finite, spread_order};
use crate::Homography;

/// One detected pattern corner: where it was found in the camera image and where it is on the flat
/// pattern (millimetres, pattern plane).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PatternPoint {
    pub pixel: [f64; 2],
    pub grid_mm: [f64; 2],
}

/// All corners detected in one photo of the (flat) pattern.
#[derive(Debug, Clone, PartialEq)]
pub struct PatternView {
    pub points: Vec<PatternPoint>,
}

impl PatternView {
    /// Corners in row-major order (`cols` per row, as a checkerboard detector reports them) on a
    /// pattern whose neighbouring corners are `pitch_mm` apart.
    pub fn from_grid(
        cols: usize,
        rows: usize,
        pitch_mm: f64,
        corners_px: &[[f64; 2]],
    ) -> Result<PatternView, LensCalibError> {
        if cols < 2 || rows < 2 || !(pitch_mm.is_finite() && pitch_mm > 0.0) || corners_px.len() != cols * rows {
            return Err(LensCalibError::BadGrid);
        }
        let points = corners_px
            .iter()
            .enumerate()
            .map(|(i, &pixel)| PatternPoint {
                pixel,
                grid_mm: [(i % cols) as f64 * pitch_mm, (i / cols) as f64 * pitch_mm],
            })
            .collect();
        Ok(PatternView { points })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LensOrder {
    K1,
    K1K2,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LensFitParams {
    pub order: LensOrder,
    pub max_rms_px: f64,
    pub max_evaluations: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LensCalibrationRequest {
    pub request: RequestId,
    pub config: CameraConfigKey,
    pub views: Vec<PatternView>,
    pub params: LensFitParams,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LensFitReport {
    pub views: usize,
    pub points: usize,
    pub rms_px: f64,
    pub max_px: f64,
    pub rms_plane_mm: f64,
    pub evaluations: usize,
    pub converged: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LensCalibration {
    pub request: RequestId,
    pub valid_for: CameraConfigKey,
    pub intrinsics: Intrinsics,
    pub report: LensFitReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaleResult {
    pub expected: RequestId,
    pub got: RequestId,
}

impl LensCalibration {
    /// The result only if it answers the request that is still current; a result computed for a
    /// superseded request (new photos, other settings) must not replace the newer state.
    pub fn accept_for(self, current: RequestId) -> Result<LensCalibration, StaleResult> {
        if self.request == current { Ok(self) } else { Err(StaleResult { expected: current, got: self.request }) }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum LensCalibError {
    TooFewViews { need: usize, got: usize },
    TooFewPoints { view: usize, need: usize, got: usize },
    NonFinite,
    OutsideImage { view: usize },
    BadMode,
    BadParams,
    BadGrid,
    Collinear { view: usize },
    InsufficientCoverage { reach: f64, need: f64 },
    Degenerate { view: usize },
    BudgetExhausted { need: usize, budget: usize },
    ResidualTooHigh { rms_px: f64, max_rms_px: f64 },
    Cancelled(Cancelled),
}

/// Corners needed in every view: a homography takes four, the rest must expose the curvature.
const MIN_POINTS_PER_VIEW: usize = 8;
/// The farthest corner must reach this normalised radius (half diagonal = 1): distortion is a
/// function of radius and invisible near the centre.
const MIN_REACH: f64 = 0.5;
const COEFF_RANGE: (f64, f64) = (-0.5, 0.5);
const K1_GRID: usize = 40;
const K1K2_GRID: usize = 14;
const GOLDEN_ITERATIONS: usize = 100;
const NELDER_MEAD_ITERATIONS: usize = 400;
const SIMPLEX_TOLERANCE: f64 = 1e-11;

/// Fits the radial coefficients (shared by all views) and one plane homography per view.
///
/// For a candidate `(k1, k2)` every corner is undistorted, a homography (shared DLT,
/// [`crate::Homography::from_points`]) maps each view to the pattern plane, and the cost is the mean
/// squared plane error. A coarse grid over `[-0.5, 0.5]` picks the basin, then golden-section (first
/// order) or Nelder–Mead (two coefficients) refines it. The search is deterministic and bounded by
/// `max_evaluations`; the token is polled before every evaluation.
pub fn calibrate_lens(
    request: &LensCalibrationRequest,
    cancel: &CancellationToken,
) -> Result<LensCalibration, LensCalibError> {
    cancel.check().map_err(LensCalibError::Cancelled)?;
    let cfg = &request.config;
    if cfg.mode.width == 0 || cfg.mode.height == 0 {
        return Err(LensCalibError::BadMode);
    }
    let params = request.params;
    if !(params.max_rms_px.is_finite() && params.max_rms_px > 0.0) || params.max_evaluations == 0 {
        return Err(LensCalibError::BadParams);
    }
    if request.views.is_empty() {
        return Err(LensCalibError::TooFewViews { need: 1, got: 0 });
    }
    let lens = Lens::for_mode(&cfg.mode);
    let (w, h) = (f64::from(cfg.mode.width), f64::from(cfg.mode.height));
    let mut reach = 0.0f64;
    for (index, view) in request.views.iter().enumerate() {
        if view.points.len() < MIN_POINTS_PER_VIEW {
            return Err(LensCalibError::TooFewPoints {
                view: index,
                need: MIN_POINTS_PER_VIEW,
                got: view.points.len(),
            });
        }
        if view.points.iter().any(|p| !(finite(p.pixel) && finite(p.grid_mm))) {
            return Err(LensCalibError::NonFinite);
        }
        if view.points.iter().any(|p| p.pixel[0] < 0.0 || p.pixel[1] < 0.0 || p.pixel[0] > w || p.pixel[1] > h) {
            return Err(LensCalibError::OutsideImage { view: index });
        }
        let pixels: Vec<[f64; 2]> = view.points.iter().map(|p| p.pixel).collect();
        let grid: Vec<[f64; 2]> = view.points.iter().map(|p| p.grid_mm).collect();
        if collinear(&pixels) || collinear(&grid) {
            return Err(LensCalibError::Collinear { view: index });
        }
        for p in &pixels {
            reach = reach.max(dist(*p, lens.center) / lens.radius);
        }
    }
    if reach < MIN_REACH {
        return Err(LensCalibError::InsufficientCoverage { reach, need: MIN_REACH });
    }

    let mut eval = Evaluator::new(&request.views, lens, params.max_evaluations, cancel);
    let (best, converged) = match params.order {
        LensOrder::K1 => search_k1(&mut eval)?,
        LensOrder::K1K2 => search_k1k2(&mut eval)?,
    };
    let radial = Radial { k1: best.0, k2: if params.order == LensOrder::K1K2 { best.1 } else { 0.0 } };
    let report = eval.report(radial, converged).ok_or(LensCalibError::Degenerate { view: 0 })?;
    if report.rms_px > params.max_rms_px {
        return Err(LensCalibError::ResidualTooHigh { rms_px: report.rms_px, max_rms_px: params.max_rms_px });
    }
    Ok(LensCalibration {
        request: request.request,
        valid_for: cfg.clone(),
        intrinsics: Intrinsics {
            center_px: eval.lens.center,
            radius_px: eval.lens.radius,
            k1: radial.k1,
            k2: radial.k2,
        },
        report,
    })
}

struct ViewData {
    /// Pixels and pattern positions, the four best-spread points first (see [`spread_order`]).
    pixels: Vec<[f64; 2]>,
    grid: Vec<[f64; 2]>,
}

struct Evaluator<'a> {
    views: Vec<ViewData>,
    lens: Lens,
    used: usize,
    budget: usize,
    cancel: &'a CancellationToken,
}

impl<'a> Evaluator<'a> {
    fn new(views: &[PatternView], lens: Lens, budget: usize, cancel: &'a CancellationToken) -> Self {
        let views = views
            .iter()
            .map(|v| {
                let grid: Vec<[f64; 2]> = v.points.iter().map(|p| p.grid_mm).collect();
                let order = spread_order(&grid);
                ViewData {
                    pixels: order.iter().map(|&i| v.points[i].pixel).collect(),
                    grid: order.iter().map(|&i| grid[i]).collect(),
                }
            })
            .collect();
        Evaluator { views, lens, used: 0, budget, cancel }
    }

    /// Per view: the plane map of the undistorted corners and the squared plane errors, or `None`
    /// when the candidate cannot be inverted or the fit is degenerate.
    fn fit_view(&self, view: &ViewData, k: Radial) -> Option<(Homography, Vec<f64>)> {
        let mut pairs = Vec::with_capacity(view.pixels.len());
        for (p, g) in view.pixels.iter().zip(&view.grid) {
            let (u, settled) = self.lens.undistort_checked(*p, k);
            if !settled {
                return None;
            }
            pairs.push((u, *g));
        }
        let (h, _) = Homography::from_points(&pairs).ok()?;
        let errors = pairs.iter().map(|(u, g)| h.apply(*u).map(|m| dist(m, *g))).collect::<Option<Vec<f64>>>()?;
        Some((h, errors))
    }

    /// Mean squared plane error of a candidate (infinite when it is not usable).
    fn cost(&mut self, k1: f64, k2: f64) -> Result<f64, LensCalibError> {
        self.cancel.check().map_err(LensCalibError::Cancelled)?;
        if self.used >= self.budget {
            return Err(LensCalibError::BudgetExhausted { need: self.used + 1, budget: self.budget });
        }
        self.used += 1;
        if !(COEFF_RANGE.0..=COEFF_RANGE.1).contains(&k1) || !(COEFF_RANGE.0..=COEFF_RANGE.1).contains(&k2) {
            return Ok(f64::INFINITY);
        }
        let k = Radial { k1, k2 };
        let (mut sum, mut n) = (0.0, 0usize);
        for view in &self.views {
            let Some((_, errors)) = self.fit_view(view, k) else { return Ok(f64::INFINITY) };
            sum += errors.iter().map(|e| e * e).sum::<f64>();
            n += errors.len();
        }
        Ok(if n == 0 { f64::INFINITY } else { sum / n as f64 })
    }

    fn remaining(&self) -> usize {
        self.budget - self.used
    }

    /// Residuals of the final model: plane error and the reprojection error in pixels
    /// (pattern point → inverse plane map → lens → compared with the detected corner).
    fn report(&self, k: Radial, converged: bool) -> Option<LensFitReport> {
        let (mut sq_mm, mut sq_px, mut max_px, mut n) = (0.0, 0.0, 0.0f64, 0usize);
        for view in &self.views {
            let (h, errors) = self.fit_view(view, k)?;
            let back = h.inverse()?;
            sq_mm += errors.iter().map(|e| e * e).sum::<f64>();
            for (p, g) in view.pixels.iter().zip(&view.grid) {
                let e = dist(self.lens.distort(back.apply(*g)?, k), *p);
                sq_px += e * e;
                max_px = max_px.max(e);
                n += 1;
            }
        }
        let nf = n as f64;
        let report = LensFitReport {
            views: self.views.len(),
            points: n,
            rms_px: (sq_px / nf).sqrt(),
            max_px,
            rms_plane_mm: (sq_mm / nf).sqrt(),
            evaluations: self.used,
            converged,
        };
        (report.rms_px.is_finite() && report.rms_plane_mm.is_finite()).then_some(report)
    }
}

/// Runs a refinement stage; a budget that runs out *during* refinement ends it early and is reported
/// as `converged = false` instead of an error (the grid stage already produced a valid candidate).
fn refinement_budget<T>(r: Result<T, LensCalibError>) -> Result<Option<T>, LensCalibError> {
    match r {
        Ok(v) => Ok(Some(v)),
        Err(LensCalibError::BudgetExhausted { .. }) => Ok(None),
        Err(e) => Err(e),
    }
}

fn search_k1(eval: &mut Evaluator<'_>) -> Result<((f64, f64), bool), LensCalibError> {
    let (lo, hi) = COEFF_RANGE;
    let step = (hi - lo) / K1_GRID as f64;
    let need = K1_GRID + 1 + 2;
    if eval.remaining() < need {
        return Err(LensCalibError::BudgetExhausted { need, budget: eval.budget });
    }
    let mut best = (f64::INFINITY, 0.0);
    for i in 0..=K1_GRID {
        let k = lo + step * i as f64;
        let c = eval.cost(k, 0.0)?;
        if c < best.0 {
            best = (c, k);
        }
    }
    if !best.0.is_finite() {
        return Err(LensCalibError::Degenerate { view: 0 });
    }
    let (mut a, mut b) = ((best.1 - step).max(lo), (best.1 + step).min(hi));
    let ratio = (5.0f64.sqrt() - 1.0) / 2.0;
    let mut c = b - ratio * (b - a);
    let mut d = a + ratio * (b - a);
    let (mut fc, mut fd) = (eval.cost(c, 0.0)?, eval.cost(d, 0.0)?);
    let mut converged = false;
    for _ in 0..GOLDEN_ITERATIONS {
        if b - a < SIMPLEX_TOLERANCE {
            converged = true;
            break;
        }
        let step_result = if fc < fd {
            b = d;
            d = c;
            fd = fc;
            c = b - ratio * (b - a);
            refinement_budget(eval.cost(c, 0.0)).map(|v| v.map(|v| fc = v))
        } else {
            a = c;
            c = d;
            fc = fd;
            d = a + ratio * (b - a);
            refinement_budget(eval.cost(d, 0.0)).map(|v| v.map(|v| fd = v))
        };
        if step_result?.is_none() {
            break;
        }
    }
    Ok((((a + b) / 2.0, 0.0), converged))
}

fn search_k1k2(eval: &mut Evaluator<'_>) -> Result<((f64, f64), bool), LensCalibError> {
    let (lo, hi) = COEFF_RANGE;
    let step = (hi - lo) / K1K2_GRID as f64;
    let need = (K1K2_GRID + 1) * (K1K2_GRID + 1) + 2;
    if eval.remaining() < need {
        return Err(LensCalibError::BudgetExhausted { need, budget: eval.budget });
    }
    let mut best = (f64::INFINITY, (0.0, 0.0));
    for i in 0..=K1K2_GRID {
        for j in 0..=K1K2_GRID {
            let p = (lo + step * i as f64, lo + step * j as f64);
            let c = eval.cost(p.0, p.1)?;
            if c < best.0 {
                best = (c, p);
            }
        }
    }
    if !best.0.is_finite() {
        return Err(LensCalibError::Degenerate { view: 0 });
    }
    // Nelder–Mead from the best grid node; ties are broken by vertex order (stable sort), so the
    // path is the same on every run.
    let mut simplex = [
        (best.0, best.1),
        (eval.cost(best.1.0 + step * 0.5, best.1.1)?, (best.1.0 + step * 0.5, best.1.1)),
        (eval.cost(best.1.0, best.1.1 + step * 0.5)?, (best.1.0, best.1.1 + step * 0.5)),
    ];
    let mut converged = false;
    'iterations: for _ in 0..NELDER_MEAD_ITERATIONS {
        simplex.sort_by(|a, b| a.0.total_cmp(&b.0));
        let size = simplex
            .iter()
            .map(|v| (v.1.0 - simplex[0].1.0).abs().max((v.1.1 - simplex[0].1.1).abs()))
            .fold(0.0, f64::max);
        if size < SIMPLEX_TOLERANCE {
            converged = true;
            break;
        }
        let centroid = ((simplex[0].1.0 + simplex[1].1.0) / 2.0, (simplex[0].1.1 + simplex[1].1.1) / 2.0);
        let worst = simplex[2];
        let along = |t: f64| (centroid.0 + t * (centroid.0 - worst.1.0), centroid.1 + t * (centroid.1 - worst.1.1));
        macro_rules! try_cost {
            ($p:expr) => {
                match refinement_budget(eval.cost($p.0, $p.1))? {
                    Some(c) => c,
                    None => break 'iterations,
                }
            };
        }
        let reflected = along(1.0);
        let fr = try_cost!(reflected);
        if fr < simplex[0].0 {
            let expanded = along(2.0);
            let fe = try_cost!(expanded);
            simplex[2] = if fe < fr { (fe, expanded) } else { (fr, reflected) };
        } else if fr < simplex[1].0 {
            simplex[2] = (fr, reflected);
        } else {
            let contracted = if fr < worst.0 { along(0.5) } else { along(-0.5) };
            let fc = try_cost!(contracted);
            if fc < worst.0.min(fr) {
                simplex[2] = (fc, contracted);
            } else {
                for v in 1..3 {
                    let p = ((simplex[0].1.0 + simplex[v].1.0) / 2.0, (simplex[0].1.1 + simplex[v].1.1) / 2.0);
                    simplex[v] = (try_cost!(p), p);
                }
            }
        }
    }
    simplex.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok((simplex[0].1, converged))
}
