//! Line planning: contour order (inner before outer, then nearest neighbour), entry points and
//! the cut-path options of IM25 (tabs, leads, overcut, perforation) plus the shared-edge policy
//! of IM32.

use std::collections::BTreeMap;

use burnburn_document::process::{LineOptions, SharedEdgePolicy};
use burnburn_geometry::{Bounds, Point, Vector};
use burnburn_types::{CancellationToken, GeometryId, OperationId};

use crate::CompileError;
use crate::compile::Step;

/// One flattened subpath of a referenced geometry, in bed-view mm.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Contour {
    pub geometry: GeometryId,
    /// Vertices; for a closed contour without the repeated start point.
    pub points: Vec<Point>,
    pub closed: bool,
}

impl Contour {
    pub fn new(geometry: GeometryId, mut points: Vec<Point>, closed: bool) -> Self {
        if closed && points.len() > 1 && points.first() == points.last() {
            points.pop();
        }
        Contour { geometry, points, closed }
    }

    pub fn translate(&mut self, v: Vector) {
        for p in &mut self.points {
            *p = *p + v;
        }
    }
}

/// How contours are ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Order {
    /// Inner before outer, then nearest neighbour (cutting).
    Containment,
    /// Exactly the given order; only the entry point is chosen (offset-fill rings).
    AsGiven,
}

/// Plans all contours of one operation pass, starting at `head`; updates `head`.
///
/// With [`Order::Containment`] a contour that lies inside a closed contour is cut before it
/// (inner before outer, so parts do not drop out early). Among the contours whose inner
/// contours are done, the one whose entry point is nearest to the head goes next; equal
/// distances keep input order (operation target order, then subpath order).
pub(crate) fn plan(
    contours: &[Contour],
    order: Order,
    options: &LineOptions,
    shared: SharedEdgePolicy,
    head: &mut Point,
    operation: OperationId,
    cancel: &CancellationToken,
) -> Result<Vec<Step>, CompileError> {
    let usable: Vec<&Contour> = contours.iter().filter(|c| c.points.len() >= 2).collect();
    let inner = match order {
        Order::Containment => containment(&usable, cancel)?,
        Order::AsGiven => vec![Vec::new(); usable.len()],
    };
    let depth = hole_depth(&usable);
    let mut done = vec![false; usable.len()];
    let mut steps = Vec::new();
    for k in 0..usable.len() {
        cancel.check()?;
        let chosen = match order {
            Order::AsGiven => Some((k, entry(usable[k], *head))),
            Order::Containment => {
                let ready = |i: usize| !done[i] && inner[i].iter().all(|&k| done[k]);
                // If containment were ever cyclic (degenerate input), fall back to any remaining contour.
                let any_ready = (0..usable.len()).any(ready);
                let mut best: Option<(usize, Entry)> = None;
                for (i, c) in usable.iter().enumerate() {
                    if done[i] || (any_ready && !ready(i)) {
                        continue;
                    }
                    let e = entry(c, *head);
                    if best.as_ref().is_none_or(|(_, b)| e.distance < b.distance) {
                        best = Some((i, e));
                    }
                }
                best
            }
        };
        let Some((i, e)) = chosen else { break };
        done[i] = true;
        let waste_inside = depth[i] % 2 == 1;
        emit(usable[i], &e, options, waste_inside, &mut steps, head, operation)?;
    }
    if shared == SharedEdgePolicy::CutOnce {
        let start = start_of(&steps);
        steps = cut_shared_edges_once(steps, start);
    }
    Ok(steps)
}

fn start_of(steps: &[Step]) -> Option<Point> {
    match steps.first() {
        Some(Step::Travel(p)) => Some(*p),
        _ => None,
    }
}

/// Where and how to enter a contour.
pub(crate) struct Entry {
    distance: f64,
    /// Closed: index of the entry vertex. Open: 0 = forward, otherwise reversed.
    index: usize,
}

fn entry(c: &Contour, head: Point) -> Entry {
    if c.closed {
        let mut best = Entry { distance: f64::INFINITY, index: 0 };
        for (i, p) in c.points.iter().enumerate() {
            let d = head.distance(*p);
            if d < best.distance {
                best = Entry { distance: d, index: i };
            }
        }
        best
    } else {
        let first = c.points.first().map_or(f64::INFINITY, |p| head.distance(*p));
        let last = c.points.last().map_or(f64::INFINITY, |p| head.distance(*p));
        if last < first { Entry { distance: last, index: 1 } } else { Entry { distance: first, index: 0 } }
    }
}

/// Twice the signed area (positive = counter-clockwise).
fn signed_area2(points: &[Point]) -> f64 {
    let n = points.len();
    (0..n).map(|k| points[k].x * points[(k + 1) % n].y - points[(k + 1) % n].x * points[k].y).sum()
}

fn unit(v: Vector) -> Vector {
    let len = v.x.hypot(v.y);
    if len > 0.0 { Vector::new(v.x / len, v.y / len) } else { Vector::ZERO }
}

/// Emits one contour: lead-in, path (with overcut), tabs/perforation gaps as laser-off
/// travel, lead-out.
fn emit(
    c: &Contour,
    e: &Entry,
    options: &LineOptions,
    waste_inside: bool,
    steps: &mut Vec<Step>,
    head: &mut Point,
    operation: OperationId,
) -> Result<(), CompileError> {
    let n = c.points.len();
    let mut path: Vec<Point> = if c.closed {
        // Keep the drawn direction; start at the entry vertex and return to it.
        (0..=n).map(|k| c.points[(e.index + k) % n]).collect()
    } else if e.index == 0 {
        c.points.clone()
    } else {
        c.points.iter().rev().copied().collect()
    };
    let perimeter = length(&path);

    // Side of the waste for leads: right of the travel direction for a counter-clockwise outer
    // contour, flipped for clockwise contours and for holes.
    let ccw = c.closed && signed_area2(&c.points) > 0.0;
    let waste = |d: Vector| -> Vector {
        let right = Vector::new(d.y, -d.x);
        if c.closed == (ccw != waste_inside) { right } else { right * -1.0 }
    };

    if c.closed
        && let Some(o) = options.overcut
    {
        let mut extra = walk(&path, o.length_mm());
        path.append(&mut extra);
    }

    let mut gaps: Vec<(f64, f64)> = Vec::new();
    if let Some(t) = options.tabs
        && c.closed
    {
        let count = f64::from(t.count());
        if t.width_mm() * count >= perimeter {
            return Err(CompileError::InvalidTabs { operation, geometry: c.geometry });
        }
        for i in 0..t.count() {
            let centre = (f64::from(i) + 0.5) * perimeter / count;
            gaps.push((centre - t.width_mm() / 2.0, centre + t.width_mm() / 2.0));
        }
    }
    if let Some(p) = options.perforation {
        let total = length(&path);
        let period = p.on_mm() + p.off_mm();
        let mut k = 0.0;
        while k * period < total {
            gaps.push((k * period + p.on_mm(), (k + 1.0) * period));
            k += 1.0;
        }
    }

    let (Some(&first), Some(&last)) = (path.first(), path.last()) else { return Ok(()) };
    let lead_dir_in = path.get(1).map_or(Vector::ZERO, |p| unit(*p - first));
    match options.lead_in {
        Some(l) => {
            // Open paths: the lead continues the path backwards; closed: perpendicular, waste side.
            let d = if c.closed { waste(lead_dir_in) } else { lead_dir_in * -1.0 };
            steps.push(Step::Travel(first + d * l.length_mm()));
            steps.push(Step::Cut { to: first, source: c.geometry });
        }
        None => steps.push(Step::Travel(first)),
    }
    *head = first;
    emit_with_gaps(&path, &gaps, c.geometry, steps);
    *head = last;
    if let Some(l) = options.lead_out {
        let k = path.len();
        let d_last = if k >= 2 { unit(path[k - 1] - path[k - 2]) } else { Vector::ZERO };
        let d = if c.closed { waste(d_last) } else { d_last };
        let end = last + d * l.length_mm();
        steps.push(Step::Cut { to: end, source: c.geometry });
        *head = end;
    }
    Ok(())
}

fn length(path: &[Point]) -> f64 {
    path.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// The points after `path[0]` along `path` for `distance` (the last one interpolated).
fn walk(path: &[Point], distance: f64) -> Vec<Point> {
    let mut out = Vec::new();
    let mut left = distance;
    for w in path.windows(2) {
        let d = w[0].distance(w[1]);
        if d >= left {
            out.push(w[0].lerp(w[1], if d > 0.0 { left / d } else { 0.0 }));
            return out;
        }
        out.push(w[1]);
        left -= d;
    }
    out
}

/// Cuts along `path` except inside `gaps` (arc-length intervals), which become travel.
fn emit_with_gaps(path: &[Point], gaps: &[(f64, f64)], source: GeometryId, steps: &mut Vec<Step>) {
    let in_gap = |s: f64| gaps.iter().any(|&(a, b)| s >= a && s < b);
    let mut s0 = 0.0;
    for w in path.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = a.distance(b);
        // Split points of this segment at gap boundaries.
        let mut cuts: Vec<f64> = gaps.iter().flat_map(|&(g0, g1)| [g0, g1]).filter(|&t| t > s0 && t < s0 + d).collect();
        cuts.sort_by(f64::total_cmp);
        cuts.dedup();
        cuts.push(s0 + d);
        let mut from = s0;
        for t in cuts {
            let to = if t >= s0 + d { b } else { a.lerp(b, (t - s0) / d) };
            let mid = 0.5 * (from + t);
            if in_gap(mid) {
                steps.push(Step::Travel(to));
            } else {
                steps.push(Step::Cut { to, source });
            }
            from = t;
        }
        s0 += d;
    }
}

/// Exact identity of an undirected segment.
type SegmentKey = ((u64, u64), (u64, u64));

fn key(a: Point, b: Point) -> SegmentKey {
    let (p, q) = ((a.x.to_bits(), a.y.to_bits()), (b.x.to_bits(), b.y.to_bits()));
    if p <= q { (p, q) } else { (q, p) }
}

/// IM32 `CutOnce`: a segment that another geometry already cut in this pass (exactly the same
/// endpoints, either direction) becomes laser-off travel. No tolerance: shared boundaries from
/// one region graph are bit-identical; near-coincident edges are not merged.
fn cut_shared_edges_once(steps: Vec<Step>, start: Option<Point>) -> Vec<Step> {
    let mut cut: BTreeMap<SegmentKey, GeometryId> = BTreeMap::new();
    let mut out = Vec::with_capacity(steps.len());
    let mut at = start;
    for step in steps {
        match step {
            Step::Cut { to, source } => {
                let Some(from) = at else {
                    out.push(step);
                    at = Some(to);
                    continue;
                };
                let k = key(from, to);
                match cut.get(&k) {
                    Some(g) if *g != source => out.push(Step::Travel(to)),
                    _ => {
                        cut.insert(k, source);
                        out.push(step);
                    }
                }
                at = Some(to);
            }
            Step::Travel(p) => {
                out.push(step);
                at = Some(p);
            }
            Step::Scan { to, .. } | Step::PowerScan { to, .. } => {
                out.push(step);
                at = Some(to);
            }
        }
    }
    // Merge consecutive travels: only the last target matters.
    let mut merged: Vec<Step> = Vec::with_capacity(out.len());
    for s in out {
        if let (Step::Travel(_), Some(Step::Travel(_))) = (&s, merged.last()) {
            merged.pop();
        }
        merged.push(s);
    }
    merged
}

/// Nesting depth of each closed contour among the closed contours of the same geometry
/// (odd = hole).
fn hole_depth(cs: &[&Contour]) -> Vec<usize> {
    cs.iter()
        .enumerate()
        .map(|(i, c)| {
            let Some(p) = c.points.first() else { return 0 };
            cs.iter()
                .enumerate()
                .filter(|(j, o)| {
                    *j != i
                        && o.geometry == c.geometry
                        && o.closed
                        && o.points.len() >= 3
                        && point_in_polygon(*p, &o.points)
                })
                .count()
        })
        .collect()
}

/// For each contour, the contours that lie inside it (and must be cut first).
fn containment(cs: &[&Contour], cancel: &CancellationToken) -> Result<Vec<Vec<usize>>, CompileError> {
    let bounds: Vec<Option<Bounds>> = cs.iter().map(|c| bounds_of(&c.points)).collect();
    let mut inside = vec![vec![false; cs.len()]; cs.len()]; // inside[i][j]: i lies inside j
    for (j, outer) in cs.iter().enumerate() {
        cancel.check()?;
        if !outer.closed || outer.points.len() < 3 {
            continue;
        }
        for (i, c) in cs.iter().enumerate() {
            let within = match (bounds[i], bounds[j]) {
                (Some(bi), Some(bj)) => bj.contains(bi.min()) && bj.contains(bi.max()) && (bi != bj),
                _ => false,
            };
            if i != j && within && c.points.iter().all(|p| point_in_polygon(*p, &outer.points)) {
                inside[i][j] = true;
            }
        }
    }
    Ok((0..cs.len()).map(|j| (0..cs.len()).filter(|&i| inside[i][j] && !inside[j][i]).collect()).collect())
}

pub(crate) fn bounds_of(points: &[Point]) -> Option<Bounds> {
    let (first, rest) = points.split_first()?;
    let mut b = Bounds::from_point(*first);
    for p in rest {
        b.include(*p);
    }
    Some(b)
}

/// Even-odd point-in-polygon (crossing number). Points exactly on an edge may go either way;
/// callers only use this for contours that do not touch.
pub(crate) fn point_in_polygon(p: Point, poly: &[Point]) -> bool {
    let mut inside = false;
    let n = poly.len();
    for k in 0..n {
        let (a, b) = (poly[k], poly[(k + 1) % n]);
        if (a.y > p.y) != (b.y > p.y) {
            let x = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y);
            if p.x < x {
                inside = !inside;
            }
        }
    }
    inside
}
