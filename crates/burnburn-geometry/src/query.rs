//! Nearest-point and hit-test queries on paths (E03).

use crate::flatten::flatten_subpath;
use crate::{HitTestTolerance, Path, Point, Segment, Subpath};

/// One segment of one subpath. `segment == segments.len()` is the implicit closing line of a
/// closed subpath.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SegmentRef {
    pub subpath: usize,
    pub segment: usize,
}

/// The closest point of a path to a query point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Nearest {
    pub at: SegmentRef,
    /// Segment parameter in `[0, 1]`.
    pub t: f64,
    pub point: Point,
    pub distance: f64,
}

/// How a query point hits a path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HitKind {
    /// Within the hit tolerance of the outline.
    Edge(Nearest),
    /// Inside a closed area (even-odd rule over the closed subpaths), away from the outline.
    Interior,
}

/// Structural float-noise bound for an implicit closing line (same as the arc continuity bound).
pub(crate) const CLOSING_NOISE_MM: f64 = 1e-9;

/// Every drawn segment with its start point, including a non-degenerate implicit closing line.
pub(crate) fn segments_of(path: &Path) -> Vec<(SegmentRef, Point, Segment)> {
    let mut out = Vec::new();
    for (si, sp) in path.subpaths().iter().enumerate() {
        out.extend(subpath_segments(si, sp));
    }
    out
}

pub(crate) fn subpath_segments(si: usize, sp: &Subpath) -> Vec<(SegmentRef, Point, Segment)> {
    let mut out = Vec::with_capacity(sp.segments().len() + 1);
    let mut from = sp.start();
    for (gi, seg) in sp.segments().iter().enumerate() {
        out.push((SegmentRef { subpath: si, segment: gi }, from, *seg));
        from = seg.end();
    }
    // A closing gap at float-noise level (e.g. after four quarter arcs) is not a segment.
    if sp.is_closed() && from.distance(sp.start()) > CLOSING_NOISE_MM {
        out.push((SegmentRef { subpath: si, segment: sp.segments().len() }, from, Segment::Line { to: sp.start() }));
    }
    out
}

/// Closest point on one segment: analytic for lines, sampling plus Newton on
/// `(B(t) − q)·B'(t) = 0` for curves.
pub(crate) fn nearest_on_segment(from: Point, seg: &Segment, q: Point) -> (f64, Point, f64) {
    if let Segment::Line { to } = *seg {
        let d = to - from;
        let len2 = d.dot(d);
        let t = if len2 > 0.0 { ((q - from).dot(d) / len2).clamp(0.0, 1.0) } else { 0.0 };
        let p = from.lerp(to, t);
        return (t, p, p.distance(q));
    }
    const SAMPLES: u32 = 32;
    let mut best = (0.0, from, from.distance(q));
    for i in 0..=SAMPLES {
        let t = f64::from(i) / f64::from(SAMPLES);
        let p = seg.point_at(from, t);
        let d = p.distance(q);
        if d < best.2 {
            best = (t, p, d);
        }
    }
    let mut t = best.0;
    for _ in 0..32 {
        let p = seg.point_at(from, t);
        let d1 = seg.derivative(from, t);
        let d2 = seg.second_derivative(from, t);
        let r = p - q;
        let g = r.dot(d1);
        let h = d1.dot(d1) + r.dot(d2);
        if !(h.abs() > 0.0) {
            break;
        }
        let next = (t - g / h).clamp(0.0, 1.0);
        let done = (next - t).abs() < 1e-15;
        t = next;
        if done {
            break;
        }
    }
    let p = seg.point_at(from, t);
    let d = p.distance(q);
    if d <= best.2 { (t, p, d) } else { best }
}

/// Even-odd containment on a flattening with chord deviation `dev`.
pub(crate) fn contains_even_odd(path: &Path, q: Point, dev: f64) -> bool {
    let mut inside = false;
    for sp in path.subpaths().iter().filter(|s| s.is_closed()) {
        let poly = flatten_subpath(sp, dev);
        let pts = &poly.points;
        let n = pts.len();
        if n < 3 {
            continue;
        }
        let mut j = n - 1;
        for i in 0..n {
            let (a, b) = (pts[i], pts[j]);
            if (a.y > q.y) != (b.y > q.y) {
                let x = a.x + (q.y - a.y) * (b.x - a.x) / (b.y - a.y);
                if q.x < x {
                    inside = !inside;
                }
            }
            j = i;
        }
    }
    inside
}

impl Path {
    /// The closest point of the outline (all subpaths, including closing lines).
    pub fn nearest(&self, q: Point) -> Option<Nearest> {
        let mut best: Option<Nearest> = None;
        for (at, from, seg) in segments_of(self) {
            if let Some(b) = &best
                && seg.bounds(from).distance_to(q) > b.distance
            {
                continue;
            }
            let (t, point, distance) = nearest_on_segment(from, &seg, q);
            if best.as_ref().is_none_or(|b| distance < b.distance) {
                best = Some(Nearest { at, t, point, distance });
            }
        }
        best
    }

    /// Edge hit within `tol`, else interior of a closed area (even-odd), else `None`.
    pub fn hit(&self, q: Point, tol: &HitTestTolerance) -> Option<HitKind> {
        if let Some(n) = self.nearest(q)
            && n.distance <= tol.mm()
        {
            return Some(HitKind::Edge(n));
        }
        // Points farther than `tol` from the outline cannot change side within a tenth of it.
        contains_even_odd(self, q, tol.mm() * 0.1).then_some(HitKind::Interior)
    }
}
