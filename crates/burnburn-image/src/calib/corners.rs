//! Checkerboard corner detection for the lens-calibration wizard (W3-UI-camera).
//!
//! Pure and deterministic, no camera and no threads; heavy enough (a whole frame) to poll a
//! [`CancellationToken`]. Three steps:
//!
//! 1. *Candidates.* The ChESS response (Bennett & Lasenby, "ChESS – quick and robust detection of
//!    chess-board features") on a ring of radius 5 peaks at X-junctions and is negative on plain
//!    edges; local maxima above a fraction of the strongest peak are kept.
//! 2. *Sub-pixel position.* Each candidate is refined to the point whose distance to the image
//!    gradient lines in its neighbourhood is least (the intersection of the two edges).
//! 3. *Order.* The `cols × rows` strongest candidates are labelled by trying the eight ways the four
//!    extreme points can be the corners of the grid; each labelling fits a homography grid → image
//!    and re-assigns the nearest candidate to every lattice point. A labelling is valid when every
//!    point has its own candidate close to the fitted lattice; of the valid, equally good ones
//!    (the lattice is symmetric) the one that is not mirrored and starts at the top-left wins.
//!
//! Pixel coordinates are continuous, the image spanning `0..width × 0..height`, so the centre of
//! the first pixel is `(0.5, 0.5)`; this is the convention of the calibration model.

use burnburn_types::CancellationToken;

use super::fit_homography_pairs;
use super::lens::{PatternPoint, PatternView};
use crate::SourceImage;

/// The printed pattern: inner corners per row and column, and the distance between neighbours.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CheckerGrid {
    pub cols: usize,
    pub rows: usize,
    pub pitch_mm: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CornerError {
    /// The grid description is not a pattern (fewer than 2 × 2 corners, no positive pitch).
    BadGrid,
    /// The board was not found: `found` corners looked like corners of a board.
    NotFound {
        found: usize,
    },
    Cancelled,
}

/// Ring radius of the ChESS response, and so the margin around the image that is not searched.
const RING: usize = 5;
/// Ring sample offsets (x, y), clockwise from the bottom, 16 samples at radius 5.
const RING_OFFSETS: [(i32, i32); 16] = [
    (0, 5),
    (-2, 4),
    (-3, 3),
    (-4, 2),
    (-5, 0),
    (-4, -2),
    (-3, -3),
    (-2, -4),
    (0, -5),
    (2, -4),
    (3, -3),
    (4, -2),
    (5, 0),
    (4, 2),
    (3, 3),
    (2, 4),
];
/// A candidate must reach this fraction of the strongest response.
const RELATIVE_THRESHOLD: f32 = 0.25;
/// Half the side of the window the sub-pixel refinement looks at, in pixels.
const REFINE_HALF: i32 = 5;
const REFINE_ITERATIONS: usize = 12;
/// A refined corner that moved further than this from its candidate was not a corner.
const MAX_REFINE_MOVE_PX: f64 = 3.0;
/// A lattice point and its candidate may be this far apart, as a fraction of the local spacing.
const MAX_LATTICE_MISFIT: f64 = 0.35;
/// Labellings whose fit is within this factor (plus a hundredth of a pixel) of the best are equals.
const EQUAL_FIT_FACTOR: f64 = 1.5;

struct Grey {
    w: usize,
    h: usize,
    v: Vec<f32>,
}

impl Grey {
    fn of(image: &SourceImage) -> Grey {
        let (w, h) = (image.width() as usize, image.height() as usize);
        let mut v = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let [r, g, b, _] = image.rgba_at(x as u32, y as u32);
                v.push(0.299 * r + 0.587 * g + 0.114 * b);
            }
        }
        Grey { w, h, v }
    }

    fn at(&self, x: i32, y: i32) -> f32 {
        self.v[y as usize * self.w + x as usize]
    }
}

pub fn detect_checkerboard(
    image: &SourceImage,
    grid: CheckerGrid,
    cancel: &CancellationToken,
) -> Result<PatternView, CornerError> {
    if grid.cols < 2 || grid.rows < 2 || !(grid.pitch_mm.is_finite() && grid.pitch_mm > 0.0) {
        return Err(CornerError::BadGrid);
    }
    cancel.check().map_err(|_| CornerError::Cancelled)?;
    let grey = Grey::of(image);
    let wanted = grid.cols * grid.rows;
    let peaks = chess_peaks(&grey, cancel)?;
    if peaks.len() < wanted {
        return Err(CornerError::NotFound { found: peaks.len() });
    }
    let mut corners = Vec::with_capacity(wanted);
    for &(x, y, _) in peaks.iter().take(wanted) {
        cancel.check().map_err(|_| CornerError::Cancelled)?;
        match refine(&grey, x, y) {
            Some(p) => corners.push(p),
            None => return Err(CornerError::NotFound { found: peaks.len() - 1 }),
        }
    }
    let ordered = order_grid(&corners, grid.cols, grid.rows, cancel)?
        .ok_or(CornerError::NotFound { found: peaks.len().min(wanted - 1) })?;
    let points = ordered
        .into_iter()
        .enumerate()
        .map(|(i, pixel)| PatternPoint {
            // Index space (pixel centres at integers) to continuous coordinates.
            pixel: [pixel[0] + 0.5, pixel[1] + 0.5],
            grid_mm: [(i % grid.cols) as f64 * grid.pitch_mm, (i / grid.cols) as f64 * grid.pitch_mm],
        })
        .collect();
    Ok(PatternView { points })
}

/// Local maxima of the ChESS response above the relative threshold, strongest first, as
/// `(x, y, response)` in pixel indices.
fn chess_peaks(g: &Grey, cancel: &CancellationToken) -> Result<Vec<(i32, i32, f32)>, CornerError> {
    let r = RING;
    if g.w <= 2 * r + 2 || g.h <= 2 * r + 2 {
        return Ok(Vec::new());
    }
    let mut response = vec![0.0f32; g.w * g.h];
    let mut best = 0.0f32;
    for y in r..g.h - r {
        if y % 16 == 0 {
            cancel.check().map_err(|_| CornerError::Cancelled)?;
        }
        for x in r..g.w - r {
            let (xi, yi) = (x as i32, y as i32);
            let mut ring = [0.0f32; 16];
            for (i, (dx, dy)) in RING_OFFSETS.iter().enumerate() {
                ring[i] = g.at(xi + dx, yi + dy);
            }
            let sr: f32 = (0..4).map(|n| ((ring[n] + ring[n + 8]) - (ring[n + 4] + ring[n + 12])).abs()).sum();
            let dr: f32 = (0..8).map(|n| (ring[n] - ring[n + 8]).abs()).sum();
            let local =
                (g.at(xi, yi) + g.at(xi - 1, yi) + g.at(xi + 1, yi) + g.at(xi, yi - 1) + g.at(xi, yi + 1)) / 5.0;
            let mr = (ring.iter().sum::<f32>() / 16.0 - local).abs();
            let value = sr - dr - 16.0 * mr;
            response[y * g.w + x] = value;
            best = best.max(value);
        }
    }
    if best <= 0.0 {
        return Ok(Vec::new());
    }
    let threshold = best * RELATIVE_THRESHOLD;
    let mut peaks = Vec::new();
    let reach = r as i32;
    for y in r..g.h - r {
        for x in r..g.w - r {
            let v = response[y * g.w + x];
            if v < threshold {
                continue;
            }
            // Strict maximum in the ring-sized neighbourhood, ties broken towards the first in scan order.
            let mut is_peak = true;
            'window: for dy in -reach..=reach {
                for dx in -reach..=reach {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if (dx, dy) == (0, 0)
                        || nx < r as i32
                        || ny < r as i32
                        || nx >= (g.w - r) as i32
                        || ny >= (g.h - r) as i32
                    {
                        continue;
                    }
                    let n = response[ny as usize * g.w + nx as usize];
                    if n > v || (n == v && (dy < 0 || (dy == 0 && dx < 0))) {
                        is_peak = false;
                        break 'window;
                    }
                }
            }
            if is_peak {
                peaks.push((x as i32, y as i32, v));
            }
        }
    }
    peaks.sort_by(|a, b| b.2.total_cmp(&a.2).then(a.1.cmp(&b.1)).then(a.0.cmp(&b.0)));
    Ok(peaks)
}

/// The point nearest (in the least-squares sense) to all gradient lines in the window around
/// `(x, y)`: solve `Σ w g gᵀ (q − p) = 0` with `g` the image gradient at `p`.
fn refine(g: &Grey, x: i32, y: i32) -> Option<[f64; 2]> {
    let mut q = [f64::from(x), f64::from(y)];
    let sigma2 = f64::from(REFINE_HALF * REFINE_HALF) / 2.0;
    for _ in 0..REFINE_ITERATIONS {
        let (cx, cy) = (q[0].round() as i32, q[1].round() as i32);
        let (mut a, mut b, mut c) = (0.0f64, 0.0f64, 0.0f64);
        let (mut r1, mut r2) = (0.0f64, 0.0f64);
        for dy in -REFINE_HALF..=REFINE_HALF {
            for dx in -REFINE_HALF..=REFINE_HALF {
                let (px, py) = (cx + dx, cy + dy);
                if px < 1 || py < 1 || px >= g.w as i32 - 1 || py >= g.h as i32 - 1 {
                    continue;
                }
                let gx = f64::from(g.at(px + 1, py) - g.at(px - 1, py)) / 2.0;
                let gy = f64::from(g.at(px, py + 1) - g.at(px, py - 1)) / 2.0;
                let d2 = (f64::from(px) - q[0]).powi(2) + (f64::from(py) - q[1]).powi(2);
                let w = (-d2 / (2.0 * sigma2)).exp();
                let (xx, xy, yy) = (w * gx * gx, w * gx * gy, w * gy * gy);
                a += xx;
                b += xy;
                c += yy;
                r1 += xx * f64::from(px) + xy * f64::from(py);
                r2 += xy * f64::from(px) + yy * f64::from(py);
            }
        }
        let det = a * c - b * b;
        if det.abs() < 1e-9 * (a + c).max(1.0).powi(2) {
            return None;
        }
        let next = [(c * r1 - b * r2) / det, (a * r2 - b * r1) / det];
        let moved = (next[0] - q[0]).hypot(next[1] - q[1]);
        q = next;
        if !(q[0].is_finite() && q[1].is_finite())
            || (q[0] - f64::from(x)).hypot(q[1] - f64::from(y)) > MAX_REFINE_MOVE_PX
        {
            return None;
        }
        if moved < 1e-3 {
            break;
        }
    }
    Some(q)
}

fn cross(o: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

/// Convex hull (Andrew's monotone chain), counter-clockwise in a y-up frame.
fn hull(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut p = points.to_vec();
    p.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    p.dedup();
    if p.len() < 3 {
        return p;
    }
    let mut lower: Vec<[f64; 2]> = Vec::new();
    for &q in &p {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], q) <= 0.0 {
            lower.pop();
        }
        lower.push(q);
    }
    let mut upper: Vec<[f64; 2]> = Vec::new();
    for &q in p.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], q) <= 0.0 {
            upper.pop();
        }
        upper.push(q);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// The four hull vertices spanning the largest quadrilateral, in hull order.
fn extreme_corners(h: &[[f64; 2]]) -> Option<[[f64; 2]; 4]> {
    let n = h.len();
    if n < 4 {
        return None;
    }
    let area =
        |i: usize, j: usize, k: usize, l: usize| (cross(h[i], h[j], h[k]).abs() + cross(h[i], h[k], h[l]).abs()) / 2.0;
    let mut best = (0.0f64, [0, 1, 2, 3]);
    for i in 0..n {
        for j in i + 1..n {
            for k in j + 1..n {
                for l in k + 1..n {
                    let a = area(i, j, k, l);
                    if a > best.0 {
                        best = (a, [i, j, k, l]);
                    }
                }
            }
        }
    }
    (best.0 > 0.0).then(|| best.1.map(|i| h[i]))
}

/// A labelling that explains all points: the points by grid index, how well, and which way round.
struct Labelling {
    by_index: Vec<[f64; 2]>,
    rms: f64,
    mirrored: bool,
}

fn order_grid(
    points: &[[f64; 2]],
    cols: usize,
    rows: usize,
    cancel: &CancellationToken,
) -> Result<Option<Vec<[f64; 2]>>, CornerError> {
    let Some(extremes) = extreme_corners(&hull(points)) else { return Ok(None) };
    let (c1, r1) = ((cols - 1) as f64, (rows - 1) as f64);
    let ideal = [[0.0, 0.0], [c1, 0.0], [c1, r1], [0.0, r1]];
    let mut valid: Vec<Labelling> = Vec::new();
    for reverse in [false, true] {
        for turn in 0..4 {
            cancel.check().map_err(|_| CornerError::Cancelled)?;
            let pairs: Vec<([f64; 2], [f64; 2])> = (0..4)
                .map(|t| {
                    let at = if reverse { (turn + 4 - t) % 4 } else { (turn + t) % 4 };
                    (ideal[t], extremes[at])
                })
                .collect();
            if let Some(l) = label(points, cols, rows, pairs) {
                valid.push(l);
            }
        }
    }
    let Some(best) = valid.iter().map(|l| l.rms).reduce(f64::min) else { return Ok(None) };
    let limit = best * EQUAL_FIT_FACTOR + 0.01;
    // Not mirrored first, then the origin nearest the top left of the picture.
    let pick = valid.into_iter().filter(|l| l.rms <= limit).min_by(|a, b| {
        a.mirrored
            .cmp(&b.mirrored)
            .then_with(|| (a.by_index[0][0] + a.by_index[0][1]).total_cmp(&(b.by_index[0][0] + b.by_index[0][1])))
    });
    Ok(pick.map(|l| l.by_index))
}

fn apply(h: &[[f64; 3]; 3], p: [f64; 2]) -> Option<[f64; 2]> {
    let w = h[2][0] * p[0] + h[2][1] * p[1] + h[2][2];
    (w.abs() > 1e-12)
        .then(|| [(h[0][0] * p[0] + h[0][1] * p[1] + h[0][2]) / w, (h[1][0] * p[0] + h[1][1] * p[1] + h[1][2]) / w])
}

/// Fit a lattice homography starting from four corner pairs, re-assigning the nearest point to
/// every lattice position; `None` unless every lattice position gets its own close point.
fn label(points: &[[f64; 2]], cols: usize, rows: usize, corners: Vec<([f64; 2], [f64; 2])>) -> Option<Labelling> {
    let n = cols * rows;
    let mut pairs = corners;
    let mut assigned: Vec<usize> = Vec::new();
    for _ in 0..4 {
        let h = fit_homography_pairs(&pairs)?;
        let mut taken = vec![false; points.len()];
        assigned.clear();
        for idx in 0..n {
            let lattice = [(idx % cols) as f64, (idx / cols) as f64];
            let projected = apply(&h, lattice)?;
            let (best, _) = points
                .iter()
                .enumerate()
                .map(|(i, p)| (i, (p[0] - projected[0]).hypot(p[1] - projected[1])))
                .min_by(|a, b| a.1.total_cmp(&b.1))?;
            if taken[best] {
                return None;
            }
            taken[best] = true;
            assigned.push(best);
        }
        pairs = (0..n).map(|idx| ([(idx % cols) as f64, (idx / cols) as f64], points[assigned[idx]])).collect();
    }
    let h = fit_homography_pairs(&pairs)?;
    let mut sum_sq = 0.0;
    for idx in 0..n {
        let (col, row) = (idx % cols, idx / cols);
        let projected = apply(&h, [col as f64, row as f64])?;
        let actual = points[assigned[idx]];
        let err = (actual[0] - projected[0]).hypot(actual[1] - projected[1]);
        // The local spacing: to the neighbour on the other side of the lattice step.
        let spacing = [
            (col + 1 < cols).then_some([col as f64 + 1.0, row as f64]),
            (col > 0).then_some([col as f64 - 1.0, row as f64]),
            (row + 1 < rows).then_some([col as f64, row as f64 + 1.0]),
            (row > 0).then_some([col as f64, row as f64 - 1.0]),
        ]
        .into_iter()
        .flatten()
        .filter_map(|q| apply(&h, q))
        .map(|q| (q[0] - projected[0]).hypot(q[1] - projected[1]))
        .fold(f64::MAX, f64::min);
        if err > MAX_LATTICE_MISFIT * spacing {
            return None;
        }
        sum_sq += err * err;
    }
    // Orientation of the lattice in the picture: the sign of the cross product of the two lattice
    // steps at the middle (positive: the grid's x towards the right and y downwards, unmirrored).
    let mid = [(cols as f64 - 1.0) / 2.0, (rows as f64 - 1.0) / 2.0];
    let (o, ex, ey) = (apply(&h, mid)?, apply(&h, [mid[0] + 1.0, mid[1]])?, apply(&h, [mid[0], mid[1] + 1.0])?);
    let mirrored = cross(o, ex, ey) < 0.0;
    Some(Labelling {
        by_index: assigned.iter().map(|&i| points[i]).collect(),
        rms: (sum_sq / n as f64).sqrt(),
        mirrored,
    })
}
