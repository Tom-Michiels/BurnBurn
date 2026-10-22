//! Template localisation with rotation and scale (G8-cam-werk, W06): where in a camera frame does a
//! learned template appear, turned by how many degrees and how much larger or smaller?
//!
//! The measure is the same zero-mean normalised cross-correlation as [`crate::locate`], a similarity
//! between -1 and 1 and never a length. A candidate pose is `(centre, angle, scale)`: every pixel of
//! the template is looked up in the frame at the place it would have if the product were turned by
//! `angle` and `scale` times as large (bilinear), so the template is never resampled and a pose
//! between two grid angles is as exact as one on it.
//!
//! The search is coarse to fine. The frame and the template are reduced by a whole factor, every
//! pose of the grid (`angle_step_deg`, `scales`) is scored at every position there, and the
//! candidates (the best pose around each place, at least half the wanted similarity) are refined at
//! full size by a pattern search over position (down to a quarter pixel), angle (down to half a
//! degree) and scale. Work is cooperative: a cancelled search returns nothing, never a partial list.
//!
//! Conventions: pixel-edge coordinates (pixel `(x, y)` covers `[x, x + 1) x [y, y + 1)`), `y` down.
//! The angle turns the template to the product in the frame by the usual rotation matrix, so with `y`
//! down a positive angle looks clockwise on the screen. The scale is the size of the product in the
//! frame relative to the template (1.25: a quarter larger). Positions are centres of the template.
//!
//! G9 widened the pose. A product may be the *mirror image* of the template (reflected in the
//! template's y axis before it is turned; `mirror`), may be *seen at an angle* (a homography: the
//! offset of a template pixel is divided by `w = 1 + (tx * m.x + ty * m.y) / R`, with `R` the half
//! diagonal of the template and `(tx, ty)` the tilt; `max_tilt`), and may lie *partly outside the
//! frame* (`min_visible`): such a pose is scored on the template pixels that are inside the frame
//! only, and the match says how much that was (`visible`). Without these options a pose whose
//! template would reach outside the frame is not looked at and nothing is mirrored or tilted.

use burnburn_types::{CancellationToken, Cancelled};

use crate::gray::GrayImage;
use crate::locate::{LocateError, check_template};

/// Largest number of poses in the grid (angles times scales): more is a typing mistake, not a search.
const MAX_POSES: usize = 2_000;
/// Most candidates that are refined, per match asked for.
const CANDIDATES_PER_MATCH: usize = 6;
/// Aim of the automatic reduction: the template is about this many pixels across when searched coarsely.
const COARSE_TEMPLATE_SIDE: u32 = 14;
/// Smallest template side after the reduction.
const MIN_COARSE_SIDE: u32 = 6;
/// Most matches a caller can ask for.
const MAX_MATCHES: usize = 1_000;
/// The pattern search stops when the steps are below these.
const FINAL_POSITION_STEP_PX: f64 = 0.25;
const FINAL_ANGLE_STEP_DEG: f64 = 0.5;
const MAX_REFINE_ROUNDS: usize = 120;
/// The largest tilt that can be asked for (each of the two components).
const MAX_TILT: f64 = 0.6;
/// `min_visible` must be above this: less than a quarter of a template is not a pattern.
const MIN_VISIBLE_FLOOR: f64 = 0.25;
/// The refinement of the tilt stops below this step.
const FINAL_TILT_STEP: f64 = 0.005;
/// Fewest template pixels a partly visible pose may be scored on.
const MIN_VISIBLE_PIXELS: usize = 16;

/// How strict, how many, and which poses.
#[derive(Debug, Clone, PartialEq)]
pub struct PoseParams {
    /// The least similarity a pose must have to be a match, above 0 and at most 1.
    pub min_score: f64,
    /// At most this many matches are returned, the most alike first.
    pub max_matches: usize,
    /// Two matches closer than this (pixels, centre to centre) are one: the better stays. `None`:
    /// half the longest side of the template.
    pub min_separation_px: Option<f64>,
    /// Angles tried: `None` only 0 degrees (translation), `Some(step)` the whole circle in steps of
    /// `step` degrees (between 1 and 90).
    pub angle_step_deg: Option<f64>,
    /// Scales tried (each between 0.25 and 4). Empty is not allowed; `[1.0]` is no scale search.
    pub scales: Vec<f64>,
    /// Also look for the product as its mirror image (reflected in the template's own y axis, then
    /// turned and scaled), as a product printed or cut the other way round looks.
    pub mirror: bool,
    /// Also look for the product seen at an angle (a homography): the largest tilt, between 0 and
    /// 0.6, where a tilt of 0.2 makes the far edge of the template 20 % smaller than the near
    /// edge. `None`: a flat view only.
    pub max_tilt: Option<f64>,
    /// The smallest part of the template that must lie inside the frame, above 0.25 and at most 1.
    /// 1: only products wholly in the frame. Less: a product partly outside the frame is found from
    /// the part that is inside (see [`PoseMatch::visible`]).
    pub min_visible: f64,
}

impl Default for PoseParams {
    fn default() -> Self {
        PoseParams {
            min_score: 0.6,
            max_matches: 16,
            min_separation_px: None,
            angle_step_deg: Some(10.0),
            scales: vec![1.0],
            mirror: false,
            max_tilt: None,
            min_visible: 1.0,
        }
    }
}

/// One place where the template appears.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PoseMatch {
    /// The centre of the template in the frame.
    pub x_px: f64,
    pub y_px: f64,
    /// How far the product is turned relative to the template, degrees in `(-180, 180]`.
    pub angle_deg: f64,
    /// How large the product is relative to the template.
    pub scale: f64,
    /// The similarity at this pose, at most 1. Not a length, not a probability.
    pub score: f64,
    /// The product is the mirror image of the template (reflected in its y axis before the turn).
    pub mirrored: bool,
    /// The tilt of the view in the template's axes as seen after mirroring: `(0, 0)` is a flat view.
    pub tilt: (f64, f64),
    /// The part of the template that lies inside the frame, in `(0, 1]`; below 1 the match rests on
    /// that part only and a person should look at it before using it.
    pub visible: f64,
}

fn check_params(p: &PoseParams) -> Result<(), LocateError> {
    let score_ok = p.min_score.is_finite() && p.min_score > 0.0 && p.min_score <= 1.0;
    let count_ok = (1..=MAX_MATCHES).contains(&p.max_matches);
    let separation_ok = p.min_separation_px.is_none_or(|s| s.is_finite() && s >= 0.0);
    let angle_ok = p.angle_step_deg.is_none_or(|a| a.is_finite() && (1.0..=90.0).contains(&a));
    let scales_ok = !p.scales.is_empty() && p.scales.iter().all(|s| s.is_finite() && (0.25..=4.0).contains(s));
    let tilt_ok = p.max_tilt.is_none_or(|t| t.is_finite() && t > 0.0 && t <= MAX_TILT);
    let visible_ok = p.min_visible.is_finite() && p.min_visible > MIN_VISIBLE_FLOOR && p.min_visible <= 1.0;
    if score_ok
        && count_ok
        && separation_ok
        && angle_ok
        && scales_ok
        && tilt_ok
        && visible_ok
        && grid_size(p) <= MAX_POSES
    {
        Ok(())
    } else {
        Err(LocateError::BadParams)
    }
}

fn angles(p: &PoseParams) -> Vec<f64> {
    match p.angle_step_deg {
        None => vec![0.0],
        Some(step) => {
            let n = (360.0 / step).round().max(1.0) as usize;
            (0..n).map(|k| k as f64 * 360.0 / n as f64).collect()
        }
    }
}

fn grid_size(p: &PoseParams) -> usize {
    match p.angle_step_deg {
        Some(step) if step.is_finite() && step >= 1.0 => (360.0 / step).round().max(1.0) as usize,
        _ => 1,
    }
    .saturating_mul(p.scales.len())
    .saturating_mul(mirrors(p).len())
    .saturating_mul(tilts(p).len())
}

fn mirrors(p: &PoseParams) -> &'static [bool] {
    if p.mirror { &[false, true] } else { &[false] }
}

/// The tilts of the coarse grid: flat, and a half of the largest one in each direction.
fn tilts(p: &PoseParams) -> Vec<(f64, f64)> {
    match p.max_tilt {
        None => vec![(0.0, 0.0)],
        Some(m) => vec![(0.0, 0.0), (m / 2.0, 0.0), (-m / 2.0, 0.0), (0.0, m / 2.0), (0.0, -m / 2.0)],
    }
}

/// Average of `factor x factor` blocks; the remainder at the right and bottom edge is dropped.
fn shrink(image: &GrayImage, factor: u32) -> GrayImage {
    if factor <= 1 {
        return image.clone();
    }
    let f = factor as usize;
    let (w, h) = (image.width as usize / f, image.height as usize / f);
    let mut values = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0.0;
            for dy in 0..f {
                for dx in 0..f {
                    sum += image.values[(y * f + dy) * image.width as usize + x * f + dx];
                }
            }
            values.push(sum / (f * f) as f32);
        }
    }
    GrayImage { width: w as u32, height: h as u32, values }
}

/// The frame sampled between its pixel centres.
struct Sampler<'a> {
    image: &'a GrayImage,
}

impl Sampler<'_> {
    /// The value at `(x, y)` in pixel-edge coordinates, linear between the four nearest centres;
    /// positions up to half a pixel outside the frame read the edge pixel.
    fn at(&self, x: f64, y: f64) -> f64 {
        let (w, h) = (self.image.width as usize, self.image.height as usize);
        let fx = (x - 0.5).clamp(0.0, (w - 1) as f64);
        let fy = (y - 0.5).clamp(0.0, (h - 1) as f64);
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
        let (tx, ty) = (fx - x0 as f64, fy - y0 as f64);
        let v = |xx: usize, yy: usize| f64::from(self.image.values[yy * w + xx]);
        let top = v(x0, y0) * (1.0 - tx) + v(x1, y0) * tx;
        let bottom = v(x0, y1) * (1.0 - tx) + v(x1, y1) * tx;
        top * (1.0 - ty) + bottom * ty
    }
}

/// The template with its mean taken out and the offsets of its pixels from its centre.
struct Kernel {
    width: usize,
    height: usize,
    /// The half diagonal of the template: the scale of the tilt.
    radius: f64,
    /// The template values as they are (a partly visible pose is scored on a part of them).
    values: Vec<f64>,
    /// Offset of each pixel centre from the centre of the template, row by row.
    offsets: Vec<(f64, f64)>,
    zero_mean: Vec<f64>,
    energy: f64,
}

impl Kernel {
    fn new(template: &GrayImage) -> Kernel {
        let (w, h) = (template.width as usize, template.height as usize);
        let n = (w * h) as f64;
        let mean = template.values.iter().map(|v| f64::from(*v)).sum::<f64>() / n;
        let zero_mean: Vec<f64> = template.values.iter().map(|v| f64::from(*v) - mean).collect();
        let energy = zero_mean.iter().map(|v| v * v).sum();
        let mut offsets = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                offsets.push((x as f64 + 0.5 - w as f64 / 2.0, y as f64 + 0.5 - h as f64 / 2.0));
            }
        }
        let values = template.values.iter().map(|v| f64::from(*v)).collect();
        Kernel { width: w, height: h, radius: 0.5 * (w as f64).hypot(h as f64), values, offsets, zero_mean, energy }
    }

    /// Half the width and height: where the corners of the template are from its centre.
    fn half(&self) -> (f64, f64) {
        (self.width as f64 / 2.0, self.height as f64 / 2.0)
    }
}

/// A pose: the centre of the template, its turn, its scale, whether it is mirrored and its tilt.
#[derive(Clone, Copy)]
struct Pose {
    x: f64,
    y: f64,
    angle_deg: f64,
    scale: f64,
    mirrored: bool,
    tx: f64,
    ty: f64,
}

impl Pose {
    fn matrix(&self) -> (f64, f64) {
        let a = self.angle_deg.to_radians();
        (a.cos() * self.scale, a.sin() * self.scale)
    }

    /// Where the template pixel at offset `(ox, oy)` from the template's centre lies in the frame.
    fn point(&self, matrix: (f64, f64), radius: f64, ox: f64, oy: f64) -> (f64, f64) {
        let (c, s) = matrix;
        let mx = if self.mirrored { -ox } else { ox };
        let w = 1.0 + (self.tx * mx + self.ty * oy) / radius;
        (self.x + (c * mx - s * oy) / w, self.y + (s * mx + c * oy) / w)
    }
}

/// Does the turned, scaled and tilted template lie wholly inside the frame? (A projective map keeps
/// straight lines straight, and the tilt is limited so that it never flips, so the corners decide.)
fn fits(frame: &GrayImage, kernel: &Kernel, pose: &Pose) -> bool {
    let m = pose.matrix();
    let (hw, hh) = kernel.half();
    [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)].iter().all(|(ox, oy)| {
        let (px, py) = pose.point(m, kernel.radius, *ox, *oy);
        px >= 0.0 && py >= 0.0 && px <= f64::from(frame.width) && py <= f64::from(frame.height)
    })
}

/// The similarity of the frame at `pose` with the template; 0 where the frame has no variation.
fn similarity(sampler: &Sampler<'_>, kernel: &Kernel, pose: &Pose) -> f64 {
    let m = pose.matrix();
    let n = kernel.offsets.len() as f64;
    let (mut sum, mut sum_sq, mut dot) = (0.0, 0.0, 0.0);
    for (k, (ox, oy)) in kernel.offsets.iter().enumerate() {
        let (px, py) = pose.point(m, kernel.radius, *ox, *oy);
        let v = sampler.at(px, py);
        sum += v;
        sum_sq += v * v;
        dot += v * kernel.zero_mean[k];
    }
    let spread = sum_sq - sum * sum / n;
    if spread <= 1e-9 {
        return 0.0;
    }
    (dot / (spread * kernel.energy).sqrt()).clamp(-1.0, 1.0)
}

/// The similarity of the part of the template that lies inside the frame, and how much of the
/// template that is; `None` when it is less than `min_visible` (or too few pixels to mean anything).
/// The correlation is taken over those pixels alone, with their own mean.
fn partial_similarity(sampler: &Sampler<'_>, kernel: &Kernel, pose: &Pose, min_visible: f64) -> Option<(f64, f64)> {
    let frame = sampler.image;
    let (fw, fh) = (f64::from(frame.width), f64::from(frame.height));
    let m = pose.matrix();
    // Cheap exit: the mapped box of the template hardly touches the frame.
    let (hw, hh) = kernel.half();
    let corners: Vec<(f64, f64)> = [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)]
        .iter()
        .map(|(ox, oy)| pose.point(m, kernel.radius, *ox, *oy))
        .collect();
    let (x0, x1) = corners.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, c| (a.0.min(c.0), a.1.max(c.0)));
    let (y0, y1) = corners.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, c| (a.0.min(c.1), a.1.max(c.1)));
    let area = (x1 - x0) * (y1 - y0);
    let inside = (x1.min(fw) - x0.max(0.0)).max(0.0) * (y1.min(fh) - y0.max(0.0)).max(0.0);
    if area <= 0.0 || inside < 0.5 * min_visible * area {
        return None;
    }
    let (mut n, mut st, mut stt, mut sv, mut svv, mut stv) = (0usize, 0.0, 0.0, 0.0, 0.0, 0.0);
    for (k, (ox, oy)) in kernel.offsets.iter().enumerate() {
        let (px, py) = pose.point(m, kernel.radius, *ox, *oy);
        if px < 0.0 || py < 0.0 || px > fw || py > fh {
            continue;
        }
        let (t, v) = (kernel.values[k], sampler.at(px, py));
        n += 1;
        st += t;
        stt += t * t;
        sv += v;
        svv += v * v;
        stv += t * v;
    }
    let visible = n as f64 / kernel.offsets.len() as f64;
    if n < MIN_VISIBLE_PIXELS || visible < min_visible {
        return None;
    }
    let nf = n as f64;
    let (spread_t, spread_v) = (stt - st * st / nf, svv - sv * sv / nf);
    if spread_t <= 1e-9 || spread_v <= 1e-9 {
        return Some((0.0, visible));
    }
    Some((((stv - st * sv / nf) / (spread_t * spread_v).sqrt()).clamp(-1.0, 1.0), visible))
}

/// The similarity and the visible part of the template at `pose`: the whole template when it lies in
/// the frame, a part of it when `min_visible` allows that, otherwise nothing.
fn score(
    frame: &GrayImage,
    sampler: &Sampler<'_>,
    kernel: &Kernel,
    pose: &Pose,
    min_visible: f64,
) -> Option<(f64, f64)> {
    if fits(frame, kernel, pose) {
        Some((similarity(sampler, kernel, pose), 1.0))
    } else if min_visible < 1.0 {
        partial_similarity(sampler, kernel, pose, min_visible)
    } else {
        None
    }
}

fn wrap_degrees(a: f64) -> f64 {
    let r = a.rem_euclid(360.0);
    if r > 180.0 { r - 360.0 } else { r }
}

fn separated(kept: &[PoseMatch], x: f64, y: f64, separation: f64) -> bool {
    kept.iter().all(|k| (k.x_px - x).hypot(k.y_px - y) >= separation)
}

/// Every pose at which `template` appears in `frame` with a similarity of at least `min_score`,
/// the most alike first (ties by position, so the order is fixed), at most `max_matches`, none whose
/// centre is closer than the separation to a better one.
pub fn locate_pose(
    frame: &GrayImage,
    template: &GrayImage,
    params: &PoseParams,
    cancel: &CancellationToken,
) -> Result<Vec<PoseMatch>, LocateError> {
    check_params(params)?;
    let well_formed =
        |i: &GrayImage| i.width > 0 && i.height > 0 && i.values.len() == i.width as usize * i.height as usize;
    if !well_formed(frame) || !well_formed(template) {
        return Err(LocateError::BadImage);
    }
    cancel.check().map_err(|Cancelled| LocateError::Cancelled)?;
    check_template(template)?;
    let diagonal = f64::from(template.width).hypot(f64::from(template.height));
    let rotates = params.angle_step_deg.is_some();
    let max_scale = params.scales.iter().copied().fold(1.0_f64, f64::max);
    // A template that turns needs room for its diagonal; one that stays only for itself.
    let needs = if rotates { diagonal * max_scale } else { f64::from(template.width.max(template.height)) * max_scale };
    if needs > f64::from(frame.width.min(frame.height))
        && (template.width > frame.width || template.height > frame.height)
    {
        return Err(LocateError::TemplateLargerThanFrame);
    }

    // Coarse search over the grid of poses.
    let side = template.width.min(template.height);
    let mut factor = (side / COARSE_TEMPLATE_SIDE).clamp(1, 8);
    while factor > 1 && side / factor < MIN_COARSE_SIDE {
        factor -= 1;
    }
    let (small_frame, small_template) = (shrink(frame, factor), shrink(template, factor));
    let small_kernel = Kernel::new(&small_template);
    let small = Sampler { image: &small_frame };
    let coarse_floor = (params.min_score * 0.5).max(0.05);
    let step = f64::from(factor);
    let partial = params.min_visible < 1.0;
    // A partly visible product has its centre up to about a half diagonal outside the frame.
    let margin = if partial { (small_kernel.radius * max_scale).ceil() as usize } else { 0 };
    let tilt_grid = tilts(params);
    let mut found: Vec<(f64, Pose)> = Vec::new();
    if small_kernel.energy > 0.0 {
        let grid_angles = angles(params);
        let (fw, fh) = (small_frame.width as usize + 2 * margin, small_frame.height as usize + 2 * margin);
        for &mirrored in mirrors(params) {
            for &(tx, ty) in &tilt_grid {
                for &scale in &params.scales {
                    for &angle_deg in &grid_angles {
                        cancel.check().map_err(|Cancelled| LocateError::Cancelled)?;
                        // Scores of this pose over the positions, on the pixel grid of the small frame
                        // (extended by the margin on every side).
                        let mut scores = vec![f64::NEG_INFINITY; fw * fh];
                        for y in 0..fh {
                            for x in 0..fw {
                                let pose = Pose {
                                    x: x as f64 - margin as f64 + 0.5,
                                    y: y as f64 - margin as f64 + 0.5,
                                    angle_deg,
                                    scale,
                                    mirrored,
                                    tx,
                                    ty,
                                };
                                if let Some((s, _)) =
                                    score(&small_frame, &small, &small_kernel, &pose, params.min_visible)
                                {
                                    scores[y * fw + x] = s;
                                }
                            }
                        }
                        for y in 0..fh {
                            for x in 0..fw {
                                let s = scores[y * fw + x];
                                if s < coarse_floor || !is_peak(&scores, fw, fh, x, y) {
                                    continue;
                                }
                                let pose = Pose {
                                    x: (x as f64 - margin as f64 + 0.5) * step,
                                    y: (y as f64 - margin as f64 + 0.5) * step,
                                    angle_deg,
                                    scale,
                                    mirrored,
                                    tx,
                                    ty,
                                };
                                found.push((s, pose));
                            }
                        }
                    }
                }
            }
        }
    }
    // The best pose around a place stands for it: the same product at the next angle is not another one.
    found.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then(a.1.y.total_cmp(&b.1.y))
            .then(a.1.x.total_cmp(&b.1.x))
            .then(a.1.angle_deg.total_cmp(&b.1.angle_deg))
            .then(a.1.scale.total_cmp(&b.1.scale))
            .then(a.1.mirrored.cmp(&b.1.mirrored))
            .then(a.1.tx.total_cmp(&b.1.tx))
            .then(a.1.ty.total_cmp(&b.1.ty))
    });
    let separation = params.min_separation_px.unwrap_or(f64::from(template.width.max(template.height)) / 2.0);
    let coarse_separation = (separation / 2.0).max(step);
    let limit = params.max_matches.saturating_mul(CANDIDATES_PER_MATCH);
    let mut candidates: Vec<Pose> = Vec::new();
    for (_, pose) in found {
        if candidates.iter().all(|c| (c.x - pose.x).hypot(c.y - pose.y) >= coarse_separation) {
            candidates.push(pose);
            if candidates.len() >= limit {
                break;
            }
        }
    }

    // Refine each candidate at full size.
    let kernel = Kernel::new(template);
    let full = Sampler { image: frame };
    let angle_step0 = params.angle_step_deg.unwrap_or(0.0) / 2.0;
    let scale_lo = params.scales.iter().copied().fold(f64::INFINITY, f64::min) * 0.9;
    let scale_hi = max_scale.max(params.scales.iter().copied().fold(0.0, f64::max)) * 1.1;
    let scale_step0 = scale_step(&params.scales);
    let tilt_limit = params.max_tilt.unwrap_or(0.0);
    let tilt_step0 = tilt_limit / 4.0;
    let mut refined: Vec<PoseMatch> = Vec::new();
    for start in candidates {
        cancel.check().map_err(|Cancelled| LocateError::Cancelled)?;
        let mut pose = start;
        let at_start = score(frame, &full, &kernel, &pose, params.min_visible);
        let (mut best, mut visible) = at_start.unwrap_or((f64::NEG_INFINITY, 0.0));
        let (mut dp, mut da, mut ds, mut dt) = (step, angle_step0, scale_step0, tilt_step0);
        for _ in 0..MAX_REFINE_ROUNDS {
            let mut moves: Vec<Pose> = Vec::with_capacity(12);
            for (mx, my) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                moves.push(Pose { x: pose.x + mx * dp, y: pose.y + my * dp, ..pose });
            }
            if da >= FINAL_ANGLE_STEP_DEG / 2.0 {
                for sign in [1.0, -1.0] {
                    moves.push(Pose { angle_deg: pose.angle_deg + sign * da, ..pose });
                }
            }
            if ds > 0.0 {
                for sign in [1.0, -1.0] {
                    let scale = (pose.scale * (1.0 + sign * ds)).clamp(scale_lo, scale_hi);
                    moves.push(Pose { scale, ..pose });
                }
            }
            if dt >= FINAL_TILT_STEP / 2.0 && tilt_limit > 0.0 {
                for sign in [1.0, -1.0] {
                    moves.push(Pose { tx: (pose.tx + sign * dt).clamp(-tilt_limit, tilt_limit), ..pose });
                    moves.push(Pose { ty: (pose.ty + sign * dt).clamp(-tilt_limit, tilt_limit), ..pose });
                }
            }
            let mut improved: Option<(f64, f64, Pose)> = None;
            for m in moves {
                let Some((s, v)) = score(frame, &full, &kernel, &m, params.min_visible) else { continue };
                if s > best + 1e-12 && improved.is_none_or(|(b, _, _)| s > b) {
                    improved = Some((s, v, m));
                }
            }
            match improved {
                Some((s, v, m)) => {
                    best = s;
                    visible = v;
                    pose = m;
                }
                None => {
                    dp /= 2.0;
                    da /= 2.0;
                    ds /= 2.0;
                    dt /= 2.0;
                    if dp < FINAL_POSITION_STEP_PX && da < FINAL_ANGLE_STEP_DEG && ds < 0.002 && dt < FINAL_TILT_STEP {
                        break;
                    }
                }
            }
        }
        if best >= params.min_score {
            refined.push(PoseMatch {
                x_px: pose.x,
                y_px: pose.y,
                angle_deg: wrap_degrees(pose.angle_deg),
                scale: pose.scale,
                score: best.min(1.0),
                mirrored: pose.mirrored,
                tilt: (pose.tx, pose.ty),
                visible,
            });
        }
    }
    refined.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.y_px.total_cmp(&b.y_px)).then(a.x_px.total_cmp(&b.x_px)));
    let mut kept: Vec<PoseMatch> = Vec::new();
    for m in refined {
        if separated(&kept, m.x_px, m.y_px, separation) {
            kept.push(m);
            if kept.len() == params.max_matches {
                break;
            }
        }
    }
    Ok(kept)
}

/// The relative scale step to start the refinement with: half the gap between the grid scales, or a
/// few percent when there is only one (so a scale between two grid scales is still found).
fn scale_step(scales: &[f64]) -> f64 {
    if scales.len() < 2 {
        return 0.0;
    }
    let mut sorted = scales.to_vec();
    sorted.sort_by(f64::total_cmp);
    let gap = sorted.windows(2).map(|w| w[1] / w[0] - 1.0).fold(f64::INFINITY, f64::min);
    (gap / 2.0).clamp(0.01, 0.25)
}

/// Is `(x, y)` at least as large as its eight neighbours in `scores`?
fn is_peak(scores: &[f64], width: usize, height: usize, x: usize, y: usize) -> bool {
    let s = scores[y * width + x];
    for dy in -1_isize..=1 {
        for dx in -1_isize..=1 {
            if dx == 0 && dy == 0 {
                continue;
            }
            let (Some(nx), Some(ny)) = (x.checked_add_signed(dx), y.checked_add_signed(dy)) else { continue };
            if nx < width && ny < height && scores[ny * width + nx] > s {
                return false;
            }
        }
    }
    true
}
