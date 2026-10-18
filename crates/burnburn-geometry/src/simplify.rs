//! Optimize Shapes (W1-L2a): fewer nodes within an explicit geometric error budget.

use std::fmt;

use burnburn_types::CancellationToken;

use crate::node::Parts;
use crate::{Path, Point, ProductionTolerance, Segment, Subpath};

/// Why an optimisation produced no path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum OptimizeError {
    Cancelled,
}

impl fmt::Display for OptimizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("optimise cancelled")
    }
}

impl std::error::Error for OptimizeError {}

/// The simplified path and what changed.
#[derive(Debug, Clone, PartialEq)]
pub struct OptimizeOutcome {
    pub path: Path,
    /// Nodes (segment ends) saved over the input.
    pub removed_nodes: usize,
    /// Cubics that were flat within the budget and became lines.
    pub flattened_curves: usize,
}

/// A line shorter than this (mm) is a duplicate point, dropped without spending budget.
const DUPLICATE_MM: f64 = 1e-12;
/// Cancellation is polled this often (points) inside a long run.
const CANCEL_STRIDE: usize = 4096;

impl Path {
    /// Simplifies the path so that every point of the input lies within `tolerance` of the result.
    ///
    /// Straight runs are reduced with Ramer–Douglas–Peucker (kept vertices are input vertices, so
    /// every dropped vertex is within the budget of the chord replacing it, and with it the
    /// segments between them). Cubics whose control points are within half the budget of their
    /// chord become lines; a run containing such a line then spends the other half. Arcs, curved
    /// spans, the start node and subpath ends are kept; a closed subpath never drops below three
    /// nodes. Curve fitting (merging smooth cubics) is not part of this operation.
    pub fn optimize(
        &self,
        tolerance: &ProductionTolerance,
        cancel: &CancellationToken,
    ) -> Result<OptimizeOutcome, OptimizeError> {
        let tol = tolerance.mm();
        let mut subs = Vec::with_capacity(self.subpaths().len());
        let (mut removed, mut flattened) = (0, 0);
        for sp in self.subpaths() {
            cancel.check().map_err(|_| OptimizeError::Cancelled)?;
            let (new, r, f) = optimize_subpath(sp, tol, cancel)?;
            removed += r;
            flattened += f;
            subs.push(new);
        }
        Ok(OptimizeOutcome { path: Path::from_subpaths(subs), removed_nodes: removed, flattened_curves: flattened })
    }
}

fn distance_to_segment(q: Point, a: Point, b: Point) -> f64 {
    let ab = b - a;
    let len2 = ab.dot(ab);
    let t = if len2 > 0.0 { ((q - a).dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
    q.distance(a + ab * t)
}

/// Marks the vertices Ramer–Douglas–Peucker keeps (both ends always); iterative, no recursion.
fn rdp(points: &[Point], eps: f64, cancel: &CancellationToken) -> Result<Vec<bool>, OptimizeError> {
    let n = points.len();
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n - 1] = true;
    let mut stack = vec![(0usize, n - 1)];
    let mut work = 0usize;
    while let Some((lo, hi)) = stack.pop() {
        let mut worst = (0.0, 0usize);
        for (i, &q) in points.iter().enumerate().take(hi).skip(lo + 1) {
            let d = distance_to_segment(q, points[lo], points[hi]);
            if d > worst.0 {
                worst = (d, i);
            }
        }
        work += hi - lo;
        if work >= CANCEL_STRIDE {
            work = 0;
            cancel.check().map_err(|_| OptimizeError::Cancelled)?;
        }
        if worst.0 > eps {
            keep[worst.1] = true;
            stack.push((lo, worst.1));
            stack.push((worst.1, hi));
        }
    }
    Ok(keep)
}

fn optimize_subpath(
    sp: &Subpath,
    tol: f64,
    cancel: &CancellationToken,
) -> Result<(Subpath, usize, usize), OptimizeError> {
    let unchanged = || Ok((sp.clone(), 0, 0));
    let parts = Parts::of(sp);
    let mut segs = if parts.closed { parts.ring() } else { parts.segs.clone() };
    // Duplicate points cost nothing; flat cubics become lines at half the budget.
    let mut from = parts.start;
    let mut kept: Vec<(Segment, bool)> = Vec::with_capacity(segs.len());
    let mut flattened = 0;
    for seg in segs.drain(..) {
        let end = seg.end();
        match seg {
            Segment::Line { .. } if from.distance(end) <= DUPLICATE_MM => {}
            Segment::Cubic { c1, c2, to }
                if distance_to_segment(c1, from, to) <= tol / 2.0 && distance_to_segment(c2, from, to) <= tol / 2.0 =>
            {
                flattened += 1;
                kept.push((Segment::Line { to }, true));
            }
            other => kept.push((other, false)),
        }
        from = end;
    }
    if kept.is_empty() {
        return unchanged();
    }
    let m = kept.len();
    let point = |k: usize| if k == 0 { parts.start } else { kept[k - 1].0.end() };
    let is_line = |k: usize| matches!(kept[k].0, Segment::Line { .. });
    // Anchors never move: the start node, both subpath ends and every line/curve boundary.
    let mut anchor = vec![false; m + 1];
    anchor[0] = true;
    anchor[m] = true;
    for (k, a) in anchor.iter_mut().enumerate().take(m).skip(1) {
        *a = is_line(k - 1) != is_line(k);
    }
    if parts.closed && (0..m).all(is_line) {
        // A loop of lines needs a second anchor, otherwise its only chord degenerates to a point.
        let far = (1..m).fold((0.0, 0usize), |best, k| {
            let d = point(k).distance(parts.start);
            if d > best.0 { (d, k) } else { best }
        });
        if far.1 > 0 {
            anchor[far.1] = true;
        }
    }
    let anchors: Vec<usize> = (0..=m).filter(|&k| anchor[k]).collect();
    let mut out: Vec<Segment> = Vec::with_capacity(m);
    for pair in anchors.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if !is_line(a) {
            out.extend(kept[a..b].iter().map(|(s, _)| *s));
            continue;
        }
        let points: Vec<Point> = (a..=b).map(point).collect();
        let eps = if kept[a..b].iter().any(|(_, converted)| *converted) { tol / 2.0 } else { tol };
        let keep = rdp(&points, eps, cancel)?;
        out.extend(points.iter().zip(&keep).skip(1).filter(|(_, k)| **k).map(|(q, _)| Segment::Line { to: *q }));
    }
    let new_parts = if parts.closed {
        Parts::from_ring(parts.start, out)
    } else {
        Parts { start: parts.start, segs: out, closed: false }
    };
    let too_few = new_parts.segs.is_empty() || (new_parts.closed && new_parts.segs.len() < 2);
    if too_few {
        return unchanged();
    }
    let removed = (sp.segments().len() + 1).saturating_sub(new_parts.segs.len() + 1);
    match new_parts.build() {
        Ok(new) if new != *sp => Ok((new, removed, flattened)),
        Ok(_) | Err(_) => unchanged(),
    }
}
