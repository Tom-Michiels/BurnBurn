//! Node-type, segment-type, start-point and direction conversions (W1-L2a). All of them work on
//! a single subpath and leave every span they do not name bit-identical.

use crate::node::{NodeEditError, NodeRef, Parts};
use crate::{NodeKind, Point, Segment, SegmentKind, Subpath, Vector, Winding};

/// A direction below this length (mm) is "no direction"; a float-noise bound, not a user tolerance.
const NO_DIRECTION: f64 = 1e-12;

/// The subpath as a cycle (closed: explicit closing line) or plain segment list (open).
fn working(p: &Parts) -> Vec<Segment> {
    if p.closed { p.ring() } else { p.segs.clone() }
}

/// Rebuilds the subpath from a working list (the inverse of [`working`]).
fn finish(p: &Parts, segs: Vec<Segment>) -> Parts {
    if p.closed { Parts::from_ring(p.start, segs) } else { Parts { start: p.start, segs, closed: false } }
}

fn point_of(start: Point, segs: &[Segment], j: usize) -> Point {
    if j == 0 { start } else { segs[j - 1].end() }
}

/// The two segment indices that meet at node `j` of the working list.
fn sides(closed: bool, m: usize, j: usize) -> (Option<usize>, Option<usize>) {
    if closed { (Some((j + m - 1) % m), Some(j)) } else { ((j > 0).then(|| j - 1), (j < m).then_some(j)) }
}

/// Unit vector, or `None` for (near) zero length.
fn unit(v: Vector) -> Option<Vector> {
    let len = v.length();
    (len > NO_DIRECTION).then(|| v * (1.0 / len))
}

/// Tangent direction (pointing along travel) and handle length of a segment's side at node `at`.
/// A missing or zero-length handle falls back to the chord, with a third of its length.
fn side_tangent(seg: &Segment, from: Point, at_start: bool) -> Option<(Vector, f64)> {
    let chord = seg.end() - from;
    match *seg {
        Segment::Line { .. } => unit(chord).map(|d| (d, chord.length() / 3.0)),
        Segment::Cubic { c1, c2, to } => {
            let handle = if at_start { c1 - from } else { to - c2 };
            let fallback = if at_start { c2 - from } else { to - c1 };
            let v = if handle.length() > NO_DIRECTION { handle } else { fallback };
            match unit(v) {
                Some(d) if handle.length() > NO_DIRECTION => Some((d, handle.length())),
                Some(d) => Some((d, chord.length() / 3.0)),
                None => unit(chord).map(|d| (d, chord.length() / 3.0)),
            }
        }
        Segment::Arc(_) => None,
    }
}

fn arc_error(subpath: usize, segment: usize) -> NodeEditError {
    NodeEditError::ArcSegment { subpath, segment }
}

fn sets_equal(a: &[Segment], b: &[Segment]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| match (x, y) {
            (Segment::Cubic { c1: a1, c2: a2, to: at }, Segment::Cubic { c1: b1, c2: b2, to: bt }) => {
                a1.distance(*b1) < 1e-12 && a2.distance(*b2) < 1e-12 && at.distance(*bt) < 1e-12
            }
            _ => x == y,
        })
}

pub(crate) fn set_node_kind(p: &Parts, node: NodeRef, kind: NodeKind) -> Result<Parts, NodeEditError> {
    let mut segs = working(p);
    let m = segs.len();
    if node.node > p.segs.len() {
        return Err(NodeEditError::NoSuchNode(node));
    }
    let j = if p.closed && node.node >= m { 0 } else { node.node };
    let at = point_of(p.start, &segs, j);
    let (inc, out) = sides(p.closed, m, j);
    for s in [inc, out].into_iter().flatten() {
        if matches!(segs[s], Segment::Arc(_)) {
            return Err(arc_error(node.subpath, s));
        }
    }
    let before = segs.clone();
    match kind {
        NodeKind::Corner => {
            if let Some(i) = inc
                && let Segment::Cubic { c2, .. } = &mut segs[i]
            {
                *c2 = at;
            }
            if let Some(o) = out
                && let Segment::Cubic { c1, .. } = &mut segs[o]
            {
                *c1 = at;
            }
        }
        NodeKind::Smooth | NodeKind::Symmetric => {
            let (Some(i), Some(o)) = (inc, out) else { return Err(NodeEditError::NoChange) };
            let in_from = point_of(p.start, &segs, i);
            let rev = segs[i].reversed(in_from);
            let (Some((t_in, l_in)), Some((t_out, l_out))) =
                (side_tangent(&rev, at, true), side_tangent(&segs[o], at, true))
            else {
                return Err(NodeEditError::NoChange);
            };
            // `t_in` points back along the incoming span; travel direction is its negation.
            let Some(dir) = unit((-t_in) + t_out) else { return Err(NodeEditError::NoChange) };
            let (len_in, len_out) = match kind {
                NodeKind::Symmetric => ((l_in + l_out) / 2.0, (l_in + l_out) / 2.0),
                _ => (l_in, l_out),
            };
            let new_in = at - dir * len_in;
            let new_out = at + dir * len_out;
            let next = segs[o].end();
            segs[i] = match segs[i] {
                Segment::Cubic { c1, to, .. } => Segment::Cubic { c1, c2: new_in, to },
                _ => Segment::Cubic { c1: in_from.lerp(at, 1.0 / 3.0), c2: new_in, to: at },
            };
            segs[o] = match segs[o] {
                Segment::Cubic { c2, to, .. } => Segment::Cubic { c1: new_out, c2, to },
                _ => Segment::Cubic { c1: new_out, c2: next.lerp(at, 1.0 / 3.0), to: next },
            };
        }
    }
    if sets_equal(&before, &segs) {
        return Err(NodeEditError::NoChange);
    }
    Ok(finish(p, segs))
}

pub(crate) fn set_segment_kind(
    p: &Parts,
    subpath: usize,
    segment: usize,
    kind: SegmentKind,
) -> Result<Parts, NodeEditError> {
    let mut segs = working(p);
    if segment >= segs.len() {
        return Err(NodeEditError::NoSuchSegment { subpath, segment });
    }
    let from = point_of(p.start, &segs, segment);
    let new = match (kind, segs[segment]) {
        (SegmentKind::Line, Segment::Line { .. }) | (SegmentKind::Curve, Segment::Cubic { .. } | Segment::Arc(_)) => {
            return Err(NodeEditError::NoChange);
        }
        (SegmentKind::Line, other) => Segment::Line { to: other.end() },
        (SegmentKind::Curve, Segment::Line { to }) => {
            Segment::Cubic { c1: from.lerp(to, 1.0 / 3.0), c2: from.lerp(to, 2.0 / 3.0), to }
        }
    };
    segs[segment] = new;
    Ok(finish(p, segs))
}

pub(crate) fn set_start_point(p: &Parts, node: NodeRef) -> Result<Parts, NodeEditError> {
    if !p.closed {
        return Err(NodeEditError::SubpathOpen(node.subpath));
    }
    if node.node > p.segs.len() {
        return Err(NodeEditError::NoSuchNode(node));
    }
    let ring = p.ring();
    let k = if node.node >= ring.len() { 0 } else { node.node };
    if k == 0 {
        return Err(NodeEditError::NoChange);
    }
    let (start, segs) = Parts::rotated(p.start, &ring, k);
    Ok(Parts::from_ring(start, segs))
}

pub(crate) fn set_winding(p: &Parts, sp: &Subpath, subpath: usize, winding: Winding) -> Result<Parts, NodeEditError> {
    if winding == Winding::Degenerate {
        return Err(NodeEditError::InvalidParameter);
    }
    let Some(current) = sp.winding() else { return Err(NodeEditError::SubpathOpen(subpath)) };
    if current == winding || current == Winding::Degenerate {
        return Err(NodeEditError::NoChange);
    }
    // Reverse the cycle from the same start node (a plain reversal would start at the last node).
    let ring = Parts { start: p.start, segs: p.ring(), closed: true };
    let rev = ring.reversed();
    Ok(Parts::from_ring(rev.start, rev.segs))
}
