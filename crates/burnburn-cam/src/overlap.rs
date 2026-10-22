//! Remove Overlapping Lines (W2-L3): works on the *planned* cut steps of one layer and pass, not
//! on the source geometry. A cut stretch that lies within `tolerance` of a stretch that was
//! already kept becomes laser-off travel. Only the kept stretches are remembered, so the
//! tolerance cannot drift along a chain of nearly coincident lines. A stretch is only compared
//! with stretches of *other* shapes: the joints of one contour, an overcut or a path that crosses
//! itself are meant to touch and are never removed.

use std::collections::HashMap;

use burnburn_geometry::Point;
use burnburn_types::{CancellationToken, GeometryId};

use crate::CompileError;
use crate::compile::Step;

/// Stretches shorter than this (mm) are rounding noise of the interval search, not cuts.
const MIN_PIECE_MM: f64 = 1e-9;

/// The segments kept so far in one layer pass, indexed on a coarse grid.
pub(crate) struct Kept {
    cell_mm: f64,
    segments: Vec<(Point, Point, GeometryId)>,
    grid: HashMap<(i64, i64), Vec<usize>>,
    /// Query stamp per segment, to visit a candidate once per query.
    stamp: Vec<u64>,
    query: u64,
}

impl Kept {
    pub fn new(tolerance_mm: f64) -> Self {
        Kept {
            // Samples are at most half a cell apart, so every pair within `tolerance` (at most
            // half a cell) shares a cell or neighbours.
            cell_mm: (2.0 * tolerance_mm).max(1.0),
            segments: Vec::new(),
            grid: HashMap::new(),
            stamp: Vec::new(),
            query: 0,
        }
    }

    fn cell(&self, p: Point) -> (i64, i64) {
        ((p.x / self.cell_mm).floor() as i64, (p.y / self.cell_mm).floor() as i64)
    }

    /// Cells touched by sample points along `a..b`, spaced at most half a cell.
    fn cells_along(&self, a: Point, b: Point) -> Vec<(i64, i64)> {
        let n = ((a.distance(b) / (self.cell_mm / 2.0)).ceil() as usize).clamp(1, 1_000_000);
        let mut cells: Vec<(i64, i64)> = (0..=n).map(|k| self.cell(a.lerp(b, k as f64 / n as f64))).collect();
        cells.dedup();
        cells
    }

    fn add(&mut self, a: Point, b: Point, source: GeometryId) {
        let id = self.segments.len();
        self.segments.push((a, b, source));
        self.stamp.push(0);
        for c in self.cells_along(a, b) {
            self.grid.entry(c).or_default().push(id);
        }
    }

    fn candidates(&mut self, a: Point, b: Point) -> Vec<usize> {
        self.query += 1;
        let mut out = Vec::new();
        for (cx, cy) in self.cells_along(a, b) {
            for dx in -1..=1 {
                for dy in -1..=1 {
                    if let Some(ids) = self.grid.get(&(cx + dx, cy + dy)) {
                        for &id in ids {
                            if self.stamp[id] != self.query {
                                self.stamp[id] = self.query;
                                out.push(id);
                            }
                        }
                    }
                }
            }
        }
        out.sort_unstable();
        out
    }
}

/// Squared distance from `p` to the segment `c..d`.
fn dist2_to_segment(p: Point, c: Point, d: Point) -> f64 {
    let (vx, vy) = (d.x - c.x, d.y - c.y);
    let len2 = vx * vx + vy * vy;
    let t = if len2 > 0.0 { (((p.x - c.x) * vx + (p.y - c.y) * vy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    let (qx, qy) = (c.x + t * vx, c.y + t * vy);
    (p.x - qx).powi(2) + (p.y - qy).powi(2)
}

/// The interval of `t` in `[0, 1]` for which `a + t (b − a)` is within `tol` of the segment
/// `c..d`. That set is convex (the distance to a convex set is convex along a line), so it is
/// one interval: found with a ternary search for the nearest point and a bisection per side.
fn covered_interval(a: Point, b: Point, c: Point, d: Point, tol: f64) -> Option<(f64, f64)> {
    let f = |t: f64| dist2_to_segment(a.lerp(b, t), c, d);
    let limit = tol * tol;
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..80 {
        let (m1, m2) = (lo + (hi - lo) / 3.0, hi - (hi - lo) / 3.0);
        if f(m1) <= f(m2) {
            hi = m2;
        } else {
            lo = m1;
        }
    }
    let nearest = 0.5 * (lo + hi);
    if f(nearest) > limit {
        return None;
    }
    let edge = |inside: f64, outside: f64| {
        let (mut inside, mut outside) = (inside, outside);
        for _ in 0..80 {
            let mid = 0.5 * (inside + outside);
            if f(mid) <= limit {
                inside = mid;
            } else {
                outside = mid;
            }
        }
        inside
    };
    let start = if f(0.0) <= limit { 0.0 } else { edge(nearest, 0.0) };
    let end = if f(1.0) <= limit { 1.0 } else { edge(nearest, 1.0) };
    Some((start, end))
}

/// Which stretch of a shared line is kept.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Keep {
    /// The first cut that reaches the line keeps it (Remove Overlapping Lines).
    First,
    /// The last cut that reaches the line keeps it: common-line cutting (contract IM32) cuts a
    /// shared stretch as part of the shape that is cut last, so after the inner features of both.
    Last,
}

/// The parameter intervals of the cut `from..to` of `source` that stay cut (not covered by a kept
/// stretch of another shape for at least `min_overlap_mm`); the kept pieces are remembered.
fn uncovered(
    from: Point,
    to: Point,
    source: GeometryId,
    tolerance_mm: f64,
    min_overlap_mm: f64,
    kept: &mut Kept,
) -> Vec<(f64, f64)> {
    let mut covered: Vec<(f64, f64)> = Vec::new();
    for id in kept.candidates(from, to) {
        let (c, d, other) = kept.segments[id];
        if other == source {
            continue;
        }
        if let Some(i) = covered_interval(from, to, c, d, tolerance_mm) {
            covered.push(i);
        }
    }
    let length = from.distance(to);
    covered.retain(|(s, e)| (e - s) * length >= min_overlap_mm);
    covered.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut pieces: Vec<(f64, f64)> = Vec::new();
    let mut cursor = 0.0;
    for (s, e) in covered {
        if s > cursor {
            pieces.push((cursor, s));
        }
        cursor = cursor.max(e);
    }
    if cursor < 1.0 {
        pieces.push((cursor, 1.0));
    }
    pieces.retain(|(s, e)| (e - s) * length >= MIN_PIECE_MM);
    for (s, e) in &pieces {
        let (p, q) = (from.lerp(to, *s), if *e >= 1.0 { to } else { from.lerp(to, *e) });
        kept.add(p, q, source);
    }
    pieces
}

/// `steps` with every cut stretch that overlaps a kept one (within `tolerance_mm`) turned into
/// travel. A shared stretch shorter than `min_overlap_mm` stays a cut: two shapes that merely cross
/// or touch at a corner share no line (common-line cutting, C12). `start` is where the head is
/// before the first step. `keep` says whether the first or the last cut of a shared line stays.
pub(crate) fn remove(
    steps: Vec<Step>,
    start: Point,
    tolerance_mm: f64,
    min_overlap_mm: f64,
    keep: Keep,
    kept: &mut Kept,
    cancel: &CancellationToken,
) -> Result<Vec<Step>, CompileError> {
    // Where the head is before each step.
    let mut from_of = Vec::with_capacity(steps.len());
    let mut at = start;
    for step in &steps {
        from_of.push(at);
        match step {
            Step::Travel(p) => at = *p,
            Step::Cut { to, .. } | Step::Scan { to, .. } | Step::PowerScan { to, .. } | Step::RampScan { to, .. } => {
                at = *to;
            }
            Step::Dot { .. } => {}
        }
    }
    // The pieces that stay cut, per step: decided first to last or last to first.
    let mut pieces: Vec<Option<Vec<(f64, f64)>>> = vec![None; steps.len()];
    let order: Vec<usize> = match keep {
        Keep::First => (0..steps.len()).collect(),
        Keep::Last => (0..steps.len()).rev().collect(),
    };
    for i in order {
        cancel.check()?;
        if let Step::Cut { to, source, .. } = steps[i] {
            pieces[i] = Some(uncovered(from_of[i], to, source, tolerance_mm, min_overlap_mm, kept));
        }
    }
    let mut out = Vec::with_capacity(steps.len());
    for (i, step) in steps.into_iter().enumerate() {
        let (to, source, tab_percent) = match step {
            Step::Cut { to, source, tab_percent } => (to, source, tab_percent),
            other => {
                out.push(other);
                continue;
            }
        };
        let from = from_of[i];
        let mut head = from;
        for (s, e) in pieces[i].take().unwrap_or_default() {
            let (p, q) = (from.lerp(to, s), if e >= 1.0 { to } else { from.lerp(to, e) });
            if head != p {
                out.push(Step::Travel(p));
            }
            out.push(Step::Cut { to: q, source, tab_percent });
            head = q;
        }
        // The head ends where the original cut ended; a removed tail is travelled over.
        if head != to {
            out.push(Step::Travel(to));
        }
    }
    Ok(out)
}
