//! Line trim/extend (E05), geometry diagnostics and gap repair (E09).

use std::fmt;

use crate::intersect::segment_intersections;
use crate::query::{nearest_on_segment, segments_of, subpath_segments};
use crate::{
    Bounds, DiagnosticTolerance, EndpointRef, NodeEdit, NodeRef, Path, Point, Segment, SegmentRef, SpatialIndex,
    Subpath, Vector,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TrimError {
    /// The segment nearest to the pick point is not a straight line.
    NotALine,
    /// No cutter crosses the picked line.
    NoIntersection,
    EmptyPath,
}

impl fmt::Display for TrimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrimError::NotALine => f.write_str("only straight lines can be trimmed"),
            TrimError::NoIntersection => f.write_str("no boundary crosses the picked line"),
            TrimError::EmptyPath => f.write_str("the path is empty"),
        }
    }
}

impl std::error::Error for TrimError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExtendError {
    NoSuchEnd,
    /// The end's segment is not a straight line.
    NotALine,
    /// The end belongs to a closed subpath.
    Closed,
    /// No boundary lies ahead of the end.
    NoBoundaryAhead,
}

impl fmt::Display for ExtendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot extend: {self:?}")
    }
}

impl std::error::Error for ExtendError {}

/// One located geometry problem. `K` identifies the object (for example a `GeometryId`).
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum IssueKind<K> {
    /// An open subpath; `gap` is the distance between its ends (mm).
    OpenContour {
        gap: f64,
    },
    SelfIntersection,
    /// The segment coincides with a segment of `other` over `overlap` mm (cut twice).
    DuplicateSegment {
        other: K,
        overlap: f64,
    },
    /// A segment shorter than the tolerance; `length` in mm.
    ZeroLength {
        length: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeometryIssue<K> {
    pub kind: IssueKind<K>,
    pub object: K,
    pub at: Option<SegmentRef>,
    pub location: Point,
}

/// Parameters below this distance from a segment end are the end itself (float noise).
const END_EPS: f64 = 1e-12;

fn line_cuts(from: Point, to: Point, cutters: &[&Path]) -> Vec<f64> {
    let seg = Segment::Line { to };
    let mut ts = Vec::new();
    for cutter in cutters {
        for (_, cf, cs) in segments_of(cutter) {
            for (t, _, _) in segment_intersections(from, &seg, cf, &cs) {
                if t > END_EPS && t < 1.0 - END_EPS {
                    ts.push(t);
                }
            }
        }
    }
    ts.sort_by(f64::total_cmp);
    ts
}

fn subpath(start: Point, segments: Vec<Segment>, closed: bool) -> Option<Subpath> {
    if segments.is_empty() {
        return None;
    }
    Subpath::new(start, segments, closed).ok()
}

impl Path {
    /// Removes the piece of the picked line between the nearest crossings with `cutters` (or the
    /// line's own end when no crossing lies on that side). A cut open subpath splits in two; a
    /// cut closed subpath becomes one open subpath.
    pub fn trim_line(&self, pick: Point, cutters: &[&Path]) -> Result<Path, TrimError> {
        let near = self.nearest(pick).ok_or(TrimError::EmptyPath)?;
        let si = near.at.subpath;
        let sp = self.subpaths().get(si).ok_or(TrimError::EmptyPath)?;
        let segs = subpath_segments(si, sp);
        let g = segs.iter().position(|(r, _, _)| *r == near.at).ok_or(TrimError::EmptyPath)?;
        let (_, a, seg) = segs[g];
        let Segment::Line { to: b } = seg else { return Err(TrimError::NotALine) };
        let cuts = line_cuts(a, b, cutters);
        if cuts.is_empty() {
            return Err(TrimError::NoIntersection);
        }
        let lo = cuts.iter().copied().filter(|t| *t < near.t).fold(0.0, f64::max);
        let hi = cuts.iter().copied().filter(|t| *t > near.t).fold(1.0, f64::min);
        let (p_lo, p_hi) = (a.lerp(b, lo), a.lerp(b, hi));
        let seg_list: Vec<Segment> = segs.iter().map(|(_, _, s)| *s).collect();
        let mut out: Vec<Subpath> = self.subpaths()[..si].to_vec();
        if sp.is_closed() {
            // Remaining: from p_hi through B, around the ring, back to A, then to p_lo.
            let mut rest = Vec::new();
            if hi < 1.0 {
                rest.push(Segment::Line { to: b });
            }
            rest.extend_from_slice(&seg_list[g + 1..]);
            rest.extend_from_slice(&seg_list[..g]);
            if lo > 0.0 {
                rest.push(Segment::Line { to: p_lo });
            }
            out.extend(subpath(p_hi, rest, false));
        } else {
            let mut before = seg_list[..g].to_vec();
            if lo > 0.0 {
                before.push(Segment::Line { to: p_lo });
            }
            let mut after = Vec::new();
            if hi < 1.0 {
                after.push(Segment::Line { to: b });
            }
            after.extend_from_slice(&seg_list[g + 1..]);
            out.extend(subpath(sp.start(), before, false));
            out.extend(subpath(p_hi, after, false));
        }
        out.extend_from_slice(&self.subpaths()[si + 1..]);
        Ok(Path::from_subpaths(out))
    }

    /// Moves a line end along its direction to the nearest boundary crossing ahead.
    pub fn extend_line(&self, end: EndpointRef, boundaries: &[&Path]) -> Result<Path, ExtendError> {
        let (si, at_end) = match end {
            EndpointRef::Start(s) => (s, false),
            EndpointRef::End(s) => (s, true),
        };
        let sp = self.subpaths().get(si).ok_or(ExtendError::NoSuchEnd)?;
        if sp.is_closed() {
            return Err(ExtendError::Closed);
        }
        let segs = sp.segments();
        let (anchor, tip) = if at_end {
            match segs.last() {
                Some(Segment::Line { to }) => {
                    (segs.len().checked_sub(2).and_then(|i| segs.get(i)).map_or(sp.start(), Segment::end), *to)
                }
                _ => return Err(ExtendError::NotALine),
            }
        } else {
            match segs.first() {
                Some(Segment::Line { to }) => (*to, sp.start()),
                _ => return Err(ExtendError::NotALine),
            }
        };
        let dir = tip - anchor;
        let len = dir.length();
        if !(len > 0.0) {
            return Err(ExtendError::NotALine);
        }
        let mut reach = Bounds::from_point(tip);
        for b in boundaries.iter().filter_map(|b| b.bounds()) {
            reach = reach.union(b);
        }
        let far = tip + dir * ((2.0 * (reach.width() + reach.height()) + 1.0) / len);
        let ray = Segment::Line { to: far };
        let mut best: Option<f64> = None;
        for boundary in boundaries {
            for (_, bf, bs) in segments_of(boundary) {
                for (t, _, _) in segment_intersections(tip, &ray, bf, &bs) {
                    if t > END_EPS && best.is_none_or(|b| t < b) {
                        best = Some(t);
                    }
                }
            }
        }
        let t = best.ok_or(ExtendError::NoBoundaryAhead)?;
        let hit = tip.lerp(far, t);
        let mut new_segs = segs.to_vec();
        let mut start = sp.start();
        if at_end {
            if let Some(last) = new_segs.last_mut() {
                *last = Segment::Line { to: hit };
            }
        } else {
            start = hit;
        }
        let mut subs = self.subpaths().to_vec();
        subs[si] = Subpath::new(start, new_segs, false).map_err(|_| ExtendError::NoBoundaryAhead)?;
        Ok(Path::from_subpaths(subs))
    }
}

/// A size measure that is zero exactly for a degenerate segment.
fn segment_extent(from: Point, seg: &Segment) -> f64 {
    match *seg {
        Segment::Line { to } => from.distance(to),
        Segment::Cubic { c1, c2, to } => from.distance(c1) + c1.distance(c2) + c2.distance(to),
        Segment::Arc(arc) => {
            let (u, v) = arc.semi_diameters();
            arc.sweep_parameter().abs() * u.length().max(v.length())
        }
    }
}

/// Overlap length when two segments coincide within `tol`, else `None`.
fn coincident(fa: Point, a: &Segment, fb: Point, b: &Segment, tol: f64) -> Option<f64> {
    match (*a, *b) {
        (Segment::Line { to: ta }, Segment::Line { to: tb }) => {
            let d = ta - fa;
            let len = d.length();
            if !(len > tol) {
                return None;
            }
            let n = Vector::new(-d.y / len, d.x / len);
            if n.dot(fb - fa).abs() > tol || n.dot(tb - fa).abs() > tol {
                return None;
            }
            let (u0, u1) = ((fb - fa).dot(d) / len, (tb - fa).dot(d) / len);
            let overlap = u0.max(u1).min(len) - u0.min(u1).max(0.0);
            (overlap > tol).then_some(overlap)
        }
        _ => {
            let same_ends = (fa.distance(fb) <= tol && a.end().distance(b.end()) <= tol)
                || (fa.distance(b.end()) <= tol && a.end().distance(fb) <= tol);
            if !same_ends || segment_extent(fa, a) <= tol {
                return None;
            }
            let all_close = [0.25, 0.5, 0.75].iter().all(|&t| nearest_on_segment(fb, b, a.point_at(fa, t)).2 <= tol);
            all_close.then(|| {
                (1..=16)
                    .map(|i| a.point_at(fa, f64::from(i - 1) / 16.0).distance(a.point_at(fa, f64::from(i) / 16.0)))
                    .sum()
            })
        }
    }
}

/// All issues of `objects`, ordered by object, then location.
pub fn diagnose<K: Copy + Ord>(objects: &[(K, &Path)], tol: &DiagnosticTolerance) -> Vec<GeometryIssue<K>> {
    let t = tol.mm();
    let mut issues = Vec::new();
    let mut all: Vec<(K, SegmentRef, Point, Segment)> = Vec::new();
    for &(key, path) in objects {
        for (si, sp) in path.subpaths().iter().enumerate() {
            let segs = subpath_segments(si, sp);
            if !sp.is_closed() {
                let last = SegmentRef { subpath: si, segment: sp.segments().len().saturating_sub(1) };
                issues.push(GeometryIssue {
                    kind: IssueKind::OpenContour { gap: sp.start().distance(sp.end()) },
                    object: key,
                    at: Some(last),
                    location: sp.end(),
                });
            }
            for (at, from, seg) in segs {
                let length = segment_extent(from, &seg);
                if length <= t {
                    issues.push(GeometryIssue {
                        kind: IssueKind::ZeroLength { length },
                        object: key,
                        at: Some(at),
                        location: from,
                    });
                } else {
                    all.push((key, at, from, seg));
                }
            }
        }
        for x in path.self_intersections() {
            issues.push(GeometryIssue {
                kind: IssueKind::SelfIntersection,
                object: key,
                at: Some(x.a.0),
                location: x.point,
            });
        }
    }
    let index = SpatialIndex::build(all.iter().enumerate().map(|(i, (_, _, f, s))| (i, s.bounds(*f).expanded(t))));
    for (i, (ka, ra, fa, a)) in all.iter().enumerate() {
        let mut others = index.query_rect(&a.bounds(*fa).expanded(t));
        others.sort_unstable();
        for j in others.into_iter().filter(|&j| j > i) {
            let (kb, _, fb, b) = &all[j];
            if let Some(overlap) = coincident(*fa, a, *fb, b, t) {
                issues.push(GeometryIssue {
                    kind: IssueKind::DuplicateSegment { other: *kb, overlap },
                    object: *ka,
                    at: Some(*ra),
                    location: *fa,
                });
            }
        }
    }
    issues.sort_by(|a, b| {
        a.object.cmp(&b.object).then(a.location.x.total_cmp(&b.location.x)).then(a.location.y.total_cmp(&b.location.y))
    });
    issues
}

/// Closes open contours whose ends meet within `tol` (chaining pieces of one path), snapping the
/// moved end exactly onto its partner. Returns the repaired path and the indices of the
/// subpaths the repair closed. Gaps above `tol` are left alone (an intended slot stays a slot).
pub fn close_gaps(path: &Path, tol: &DiagnosticTolerance) -> (Path, Vec<usize>) {
    let t = tol.mm();
    let mut current = path.clone();
    let mut closed_by_repair = vec![false; current.subpaths().len()];
    loop {
        let subs = current.subpaths();
        let mut best: Option<(f64, EndpointRef, EndpointRef)> = None;
        let mut consider = |d: f64, a: EndpointRef, b: EndpointRef| {
            if d <= t && best.is_none_or(|(bd, _, _)| d < bd) {
                best = Some((d, a, b));
            }
        };
        for (i, a) in subs.iter().enumerate().filter(|(_, s)| !s.is_closed()) {
            if a.segments().len() >= 2 {
                consider(a.end().distance(a.start()), EndpointRef::Start(i), EndpointRef::End(i));
            }
            for (j, b) in subs.iter().enumerate().skip(i + 1).filter(|(_, s)| !s.is_closed()) {
                consider(a.end().distance(b.start()), EndpointRef::End(i), EndpointRef::Start(j));
                consider(a.end().distance(b.end()), EndpointRef::End(i), EndpointRef::End(j));
                consider(a.start().distance(b.start()), EndpointRef::Start(i), EndpointRef::Start(j));
                consider(a.start().distance(b.end()), EndpointRef::Start(i), EndpointRef::End(j));
            }
        }
        let Some((_, a, b)) = best else { break };
        let point = |e: EndpointRef| match e {
            EndpointRef::Start(s) => subs[s].start(),
            EndpointRef::End(s) => subs[s].end(),
        };
        let node = |e: EndpointRef| match e {
            EndpointRef::Start(s) => NodeRef { subpath: s, node: 0 },
            EndpointRef::End(s) => NodeRef { subpath: s, node: subs[s].segments().len() },
        };
        // Snap b onto a where possible; an arc end cannot move, then a short line bridges.
        let target = point(a);
        let snapped = current.edit_nodes(&NodeEdit::MoveNode { node: node(b), to: target }).unwrap_or(current.clone());
        let (sa, sb) = (endpoint_index(a), endpoint_index(b));
        let joined = snapped.edit_nodes(&NodeEdit::JoinEndpoints { a, b });
        match joined {
            Ok(p) => {
                if sa == sb {
                    closed_by_repair[sa] = true;
                } else if sb < closed_by_repair.len() {
                    closed_by_repair.remove(sb);
                }
                current = p;
            }
            Err(_) => break,
        }
    }
    let closed = closed_by_repair.iter().enumerate().filter(|(_, c)| **c).map(|(i, _)| i).collect();
    (current, closed)
}

fn endpoint_index(e: EndpointRef) -> usize {
    match e {
        EndpointRef::Start(s) | EndpointRef::End(s) => s,
    }
}
