//! Template localisation (G7-werk, W06): where in a camera frame does a learned template appear?
//!
//! The measure is the zero-mean normalised cross-correlation of grey values. It ignores brightness
//! and contrast, so a lamp that dims does not lose the product, and it is a *similarity* between
//! -1 and 1: it says how alike a window is to the template, never how accurate a position is in
//! millimetres (that is up to the calibration of the camera, and nothing in this module feeds
//! production geometry). Every window that is alike enough is a candidate; a copy next to a copy is
//! two matches, not one pick, and the caller decides which count.
//!
//! The search is translation only (no rotation or scale), exhaustive at a reduced size to find the
//! candidates and exact at full size around each of them. Positions are the centre of the template
//! in pixel-edge coordinates (pixel `(x, y)` covers `[x, x+1) × [y, y+1)`), refined to a fraction of
//! a pixel by a parabola through the scores around the peak. Work is cooperative: a cancelled
//! search returns nothing, never a partial list.

use burnburn_types::{CancellationToken, Cancelled};

use crate::adjust::PixelRect;
use crate::gray::{AlphaPolicy, GrayImage, to_gray};
use crate::source::SourceImage;

/// Smallest template side (pixels) that still holds a pattern.
const MIN_TEMPLATE_SIDE: u32 = 4;
/// A template whose grey values hardly vary has no pattern to look for.
const MIN_TEMPLATE_DEVIATION: f64 = 1.0;
/// Most matches a caller can ask for.
const MAX_MATCHES: usize = 10_000;
/// Largest reduction factor of the coarse search.
const MAX_COARSE_FACTOR: u32 = 16;
/// Aim of the automatic reduction: the template is about this many pixels across when searched coarsely.
const COARSE_TEMPLATE_SIDE: u32 = 12;

/// How strict and how many.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocateParams {
    /// The least similarity a window must have to be a match, above 0 and at most 1.
    pub min_score: f64,
    /// At most this many matches are returned, the most alike first.
    pub max_matches: usize,
    /// Two matches closer than this (pixels, centre to centre) are one: the better stays. `None`:
    /// half the longest side of the template.
    pub min_separation_px: Option<f64>,
    /// Reduction of the coarse search. `None` chooses one from the template size; `Some(1)` searches
    /// every position at full size (exact, slow).
    pub coarse_factor: Option<u32>,
}

impl Default for LocateParams {
    fn default() -> Self {
        LocateParams { min_score: 0.6, max_matches: 16, min_separation_px: None, coarse_factor: None }
    }
}

/// One place where the template appears.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocateMatch {
    /// The centre of the template in the frame, pixel-edge coordinates.
    pub x_px: f64,
    pub y_px: f64,
    /// The similarity at this place, at most 1. Not a length, not a probability.
    pub score: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LocateError {
    Cancelled,
    /// The template is smaller than 4 x 4 pixels.
    TemplateTooSmall,
    TemplateLargerThanFrame,
    /// The template is (almost) one grey value: there is no pattern to find.
    FlatTemplate,
    /// A parameter that means nothing (a score outside 0..1, no matches asked for, ...).
    BadParams,
    /// A crop that is empty or reaches outside the frame.
    BadCrop,
    /// An image whose number of values does not match its size.
    BadImage,
}

impl From<Cancelled> for LocateError {
    fn from(_: Cancelled) -> Self {
        LocateError::Cancelled
    }
}

/// A camera frame as grey values, converted the way the tracer converts (Rec. 709 luma, a
/// transparent pixel read as white paper).
pub fn gray_of_source(source: &SourceImage) -> GrayImage {
    let (width, height) = (source.width(), source.height());
    let mut values = Vec::with_capacity(width as usize * height as usize);
    for y in 0..height {
        for x in 0..width {
            values.push(to_gray(source.rgba_at(x, y), AlphaPolicy::default()));
        }
    }
    GrayImage { width, height, values }
}

fn well_formed(image: &GrayImage) -> bool {
    image.width > 0 && image.height > 0 && image.values.len() == image.width as usize * image.height as usize
}

/// The sub-image of `frame` at `rect`: what the operator marks in a photo to learn a template.
pub fn crop_gray(frame: &GrayImage, rect: PixelRect) -> Result<GrayImage, LocateError> {
    let fits_x = rect.x.checked_add(rect.width).is_some_and(|r| r <= frame.width);
    let fits_y = rect.y.checked_add(rect.height).is_some_and(|b| b <= frame.height);
    if rect.width == 0 || rect.height == 0 || !fits_x || !fits_y || !well_formed(frame) {
        return Err(LocateError::BadCrop);
    }
    let mut values = Vec::with_capacity(rect.width as usize * rect.height as usize);
    for y in rect.y..rect.y + rect.height {
        let row = y as usize * frame.width as usize;
        values.extend_from_slice(&frame.values[row + rect.x as usize..row + (rect.x + rect.width) as usize]);
    }
    Ok(GrayImage { width: rect.width, height: rect.height, values })
}

/// A grey image with the sums that make a window's mean and spread cheap.
struct Integral {
    width: usize,
    /// `(width + 1) * (height + 1)` running sums of the values and of their squares.
    sum: Vec<f64>,
    sum_sq: Vec<f64>,
}

impl Integral {
    fn new(image: &GrayImage) -> Integral {
        let (w, h) = (image.width as usize, image.height as usize);
        let stride = w + 1;
        let mut sum = vec![0.0; stride * (h + 1)];
        let mut sum_sq = vec![0.0; stride * (h + 1)];
        for y in 0..h {
            let (mut row, mut row_sq) = (0.0, 0.0);
            for x in 0..w {
                let v = f64::from(image.values[y * w + x]);
                row += v;
                row_sq += v * v;
                sum[(y + 1) * stride + x + 1] = sum[y * stride + x + 1] + row;
                sum_sq[(y + 1) * stride + x + 1] = sum_sq[y * stride + x + 1] + row_sq;
            }
        }
        Integral { width: stride, sum, sum_sq }
    }

    /// Sum and sum of squares of the window at `(x, y)` of `w x h`.
    fn window(&self, x: usize, y: usize, w: usize, h: usize) -> (f64, f64) {
        let at = |v: &Vec<f64>, px: usize, py: usize| v[py * self.width + px];
        let take = |v: &Vec<f64>| at(v, x + w, y + h) - at(v, x, y + h) - at(v, x + w, y) + at(v, x, y);
        (take(&self.sum), take(&self.sum_sq))
    }
}

/// The template with its mean taken out, ready to be slid over a frame.
struct Kernel {
    width: usize,
    height: usize,
    zero_mean: Vec<f64>,
    /// Sum of squared deviations from the mean.
    energy: f64,
}

impl Kernel {
    fn new(template: &GrayImage) -> Kernel {
        let n = template.values.len() as f64;
        let mean = template.values.iter().map(|v| f64::from(*v)).sum::<f64>() / n;
        let zero_mean: Vec<f64> = template.values.iter().map(|v| f64::from(*v) - mean).collect();
        let energy = zero_mean.iter().map(|v| v * v).sum();
        Kernel { width: template.width as usize, height: template.height as usize, zero_mean, energy }
    }
}

/// The similarity of the window at `(x, y)` with the template; 0 for a window with no variation.
fn similarity(frame: &GrayImage, integral: &Integral, kernel: &Kernel, x: usize, y: usize) -> f64 {
    let (w, h) = (kernel.width, kernel.height);
    let n = (w * h) as f64;
    let (sum, sum_sq) = integral.window(x, y, w, h);
    let spread = sum_sq - sum * sum / n;
    if spread <= 1e-9 {
        return 0.0;
    }
    let stride = frame.width as usize;
    let mut dot = 0.0;
    for ky in 0..h {
        let row = (y + ky) * stride + x;
        let k = ky * w;
        for kx in 0..w {
            dot += f64::from(frame.values[row + kx]) * kernel.zero_mean[k + kx];
        }
    }
    (dot / (spread * kernel.energy).sqrt()).clamp(-1.0, 1.0)
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

fn check_params(params: &LocateParams) -> Result<(), LocateError> {
    let score_ok = params.min_score.is_finite() && params.min_score > 0.0 && params.min_score <= 1.0;
    let count_ok = (1..=MAX_MATCHES).contains(&params.max_matches);
    let separation_ok = params.min_separation_px.is_none_or(|s| s.is_finite() && s >= 0.0);
    let factor_ok = params.coarse_factor.is_none_or(|f| (1..=MAX_COARSE_FACTOR).contains(&f));
    if score_ok && count_ok && separation_ok && factor_ok { Ok(()) } else { Err(LocateError::BadParams) }
}

/// Every place where `template` appears in `frame` with a similarity of at least `min_score`, the
/// most alike first (ties by position, so the order is fixed), at most `max_matches`, none closer
/// than the separation to a better one.
pub fn locate(
    frame: &GrayImage,
    template: &GrayImage,
    params: &LocateParams,
    cancel: &CancellationToken,
) -> Result<Vec<LocateMatch>, LocateError> {
    check_params(params)?;
    if !well_formed(frame) || !well_formed(template) {
        return Err(LocateError::BadImage);
    }
    cancel.check()?;
    if template.width < MIN_TEMPLATE_SIDE || template.height < MIN_TEMPLATE_SIDE {
        return Err(LocateError::TemplateTooSmall);
    }
    if template.width > frame.width || template.height > frame.height {
        return Err(LocateError::TemplateLargerThanFrame);
    }
    let kernel = Kernel::new(template);
    if (kernel.energy / template.values.len() as f64).sqrt() < MIN_TEMPLATE_DEVIATION {
        return Err(LocateError::FlatTemplate);
    }
    let integral = Integral::new(frame);
    let (tw, th) = (template.width as usize, template.height as usize);
    let (max_x, max_y) = (frame.width as usize - tw, frame.height as usize - th);

    let side = template.width.min(template.height);
    let mut factor = params.coarse_factor.unwrap_or_else(|| (side / COARSE_TEMPLATE_SIDE).clamp(1, 8));
    while factor > 1 && side / factor < MIN_TEMPLATE_SIDE {
        factor -= 1;
    }

    // Candidate places: every local maximum of the similarity that is alike enough.
    let mut candidates: Vec<(usize, usize)> = Vec::new();
    if factor <= 1 {
        let mut scores = vec![0.0_f64; (max_x + 1) * (max_y + 1)];
        for y in 0..=max_y {
            cancel.check()?;
            for x in 0..=max_x {
                scores[y * (max_x + 1) + x] = similarity(frame, &integral, &kernel, x, y);
            }
        }
        collect_peaks(&scores, max_x + 1, max_y + 1, params.min_score, &mut candidates);
    } else {
        let (small_frame, small_template) = (shrink(frame, factor), shrink(template, factor));
        let small_kernel = Kernel::new(&small_template);
        let small_integral = Integral::new(&small_frame);
        let (sw, sh) = (small_template.width as usize, small_template.height as usize);
        if small_frame.width as usize >= sw && small_frame.height as usize >= sh && small_kernel.energy > 0.0 {
            let (smax_x, smax_y) = (small_frame.width as usize - sw, small_frame.height as usize - sh);
            let mut scores = vec![0.0_f64; (smax_x + 1) * (smax_y + 1)];
            for y in 0..=smax_y {
                cancel.check()?;
                for x in 0..=smax_x {
                    scores[y * (smax_x + 1) + x] = similarity(&small_frame, &small_integral, &small_kernel, x, y);
                }
            }
            // A copy between two coarse positions scores lower there: look a little wider.
            let mut coarse = Vec::new();
            collect_peaks(&scores, smax_x + 1, smax_y + 1, (params.min_score * 0.5).max(0.05), &mut coarse);
            candidates = coarse.into_iter().map(|(x, y)| (x * factor as usize, y * factor as usize)).collect();
        }
    }

    // Exact scores around each candidate, then the best of them.
    let reach = if factor <= 1 { 0 } else { factor as usize };
    let mut refined: Vec<(usize, usize, f64)> = Vec::new();
    for (cx, cy) in candidates {
        cancel.check()?;
        let (x0, x1) = (cx.saturating_sub(reach), (cx + reach).min(max_x));
        let (y0, y1) = (cy.saturating_sub(reach), (cy + reach).min(max_y));
        let mut best: Option<(usize, usize, f64)> = None;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let s = similarity(frame, &integral, &kernel, x, y);
                if best.is_none_or(|b| s > b.2) {
                    best = Some((x, y, s));
                }
            }
        }
        if let Some(b) = best.filter(|b| b.2 >= params.min_score) {
            refined.push(b);
        }
    }
    // Most alike first; equal scores by position, so the order is the same every time.
    refined.sort_by(|a, b| b.2.total_cmp(&a.2).then(a.1.cmp(&b.1)).then(a.0.cmp(&b.0)));
    refined.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);

    let separation = params.min_separation_px.unwrap_or(f64::from(template.width.max(template.height)) / 2.0);
    let mut kept: Vec<(usize, usize, f64)> = Vec::new();
    for c in refined {
        let apart = kept.iter().all(|k| {
            let (dx, dy) = (k.0 as f64 - c.0 as f64, k.1 as f64 - c.1 as f64);
            dx.hypot(dy) >= separation
        });
        if apart {
            kept.push(c);
            if kept.len() == params.max_matches {
                break;
            }
        }
    }

    Ok(kept
        .into_iter()
        .map(|(x, y, score)| {
            let at = |dx: isize, dy: isize| -> Option<f64> {
                let (nx, ny) = (x.checked_add_signed(dx)?, y.checked_add_signed(dy)?);
                (nx <= max_x && ny <= max_y).then(|| similarity(frame, &integral, &kernel, nx, ny))
            };
            let along = |before: Option<f64>, after: Option<f64>| match (before, after) {
                (Some(b), Some(a)) if score >= b && score >= a && (b - 2.0 * score + a) < 0.0 => {
                    (0.5 * (b - a) / (b - 2.0 * score + a)).clamp(-0.5, 0.5)
                }
                _ => 0.0,
            };
            let (fx, fy) = (along(at(-1, 0), at(1, 0)), along(at(0, -1), at(0, 1)));
            LocateMatch {
                x_px: x as f64 + fx + tw as f64 / 2.0,
                y_px: y as f64 + fy + th as f64 / 2.0,
                score: score.min(1.0),
            }
        })
        .collect())
}

/// The places of a similarity map that are at least `threshold` and no smaller than any neighbour.
fn collect_peaks(scores: &[f64], width: usize, height: usize, threshold: f64, out: &mut Vec<(usize, usize)>) {
    for y in 0..height {
        for x in 0..width {
            let s = scores[y * width + x];
            if s < threshold {
                continue;
            }
            let mut is_peak = true;
            'around: for dy in -1_isize..=1 {
                for dx in -1_isize..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let (Some(nx), Some(ny)) = (x.checked_add_signed(dx), y.checked_add_signed(dy)) else { continue };
                    if nx < width && ny < height && scores[ny * width + nx] > s {
                        is_peak = false;
                        break 'around;
                    }
                }
            }
            if is_peak {
                out.push((x, y));
            }
        }
    }
}
