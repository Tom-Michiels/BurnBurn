//! Round Corners (W1-L2a): circular fillets between straight segments.

use std::fmt;

use burnburn_types::Angle;

use crate::node::{NodeEditError, NodeRef, Parts};
use crate::{EllipticalArc, Path, Point, Segment, Vector, math};

/// Why a round-corners request failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RoundError {
    /// The radius must be finite and strictly positive.
    InvalidRadius,
    /// No straight corner could be rounded (all candidates are curves, collinear or open ends).
    NothingToRound,
}

impl fmt::Display for RoundError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RoundError::InvalidRadius => f.write_str("the radius must be a finite, positive length"),
            RoundError::NothingToRound => f.write_str("there is no straight corner to round"),
        }
    }
}

impl std::error::Error for RoundError {}

/// The rounded path and how the request went.
#[derive(Debug, Clone, PartialEq)]
pub struct RoundOutcome {
    pub path: Path,
    /// Corners replaced by an arc.
    pub rounded: usize,
    /// Of those, corners whose radius was reduced because the adjacent segments are too short.
    pub limited: usize,
    /// Requested corners left alone (open ends, tangent-continuous nodes, curves, zero-length sides).
    pub skipped: usize,
}

/// Directions shorter than this (mm) are no directions; a float-noise bound, not a user tolerance.
const NO_DIRECTION: f64 = 1e-12;
/// `|sin θ|` below this is a straight continuation or a full reversal: no fillet exists.
const NO_CORNER_SIN: f64 = 1e-9;

impl Path {
    /// Replaces straight-line corners by circular arcs of `radius`. `only` restricts the rounding
    /// to the listed nodes (closed subpaths: node `len` is node 0). When two rounded corners share
    /// a segment, both shrink proportionally so the arcs never overlap.
    pub fn round_corners(&self, radius: f64, only: Option<&[NodeRef]>) -> Result<RoundOutcome, RoundError> {
        if !(radius.is_finite() && radius > 0.0) {
            return Err(RoundError::InvalidRadius);
        }
        let mut stats = Stats::default();
        let mut subs = Vec::with_capacity(self.subpaths().len());
        for (si, sp) in self.subpaths().iter().enumerate() {
            let wanted: Option<Vec<usize>> =
                only.map(|nodes| nodes.iter().filter(|n| n.subpath == si).map(|n| n.node).collect());
            let (parts, s) = round_parts(&Parts::of(sp), radius, wanted.as_deref());
            stats.add(&s);
            subs.push(parts.build().map_err(|_| RoundError::NothingToRound)?);
        }
        // Requested nodes that name no subpath at all are skipped corners too.
        if let Some(nodes) = only {
            stats.skipped += nodes.iter().filter(|n| n.subpath >= self.subpaths().len()).count();
        }
        if stats.rounded == 0 {
            return Err(RoundError::NothingToRound);
        }
        Ok(RoundOutcome {
            path: Path::from_subpaths(subs),
            rounded: stats.rounded,
            limited: stats.limited,
            skipped: stats.skipped,
        })
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct Stats {
    rounded: usize,
    limited: usize,
    skipped: usize,
}

impl Stats {
    fn add(&mut self, o: &Stats) {
        self.rounded += o.rounded;
        self.limited += o.limited;
        self.skipped += o.skipped;
    }
}

/// One corner that can be rounded: a vertex between two straight segments.
#[derive(Clone, Copy)]
struct Corner {
    vertex: Point,
    /// Unit vectors from the vertex back along the incoming and forward along the outgoing side.
    back: Vector,
    forward: Vector,
    /// Interior angle between the two sides, in (0, π).
    theta: f64,
    /// Tangent length wanted for the requested radius.
    wanted: f64,
    /// Tangent length actually used after the shared-segment limit.
    used: f64,
}

fn unit(v: Vector) -> Option<Vector> {
    let len = v.length();
    (len > NO_DIRECTION).then(|| v * (1.0 / len))
}

fn corner_at(segs: &[Segment], start: Point, closed: bool, j: usize, radius: f64) -> Option<Corner> {
    let m = segs.len();
    let (inc, out) = if closed {
        ((j + m - 1) % m, j)
    } else if j > 0 && j < m {
        (j - 1, j)
    } else {
        return None;
    };
    if !matches!((segs[inc], segs[out]), (Segment::Line { .. }, Segment::Line { .. })) {
        return None;
    }
    let at = |k: usize| if k == 0 { start } else { segs[k - 1].end() };
    let vertex = at(j);
    let back = unit(at(inc) - vertex)?;
    let forward = unit(segs[out].end() - vertex)?;
    if back.cross(forward).abs() < NO_CORNER_SIN {
        return None;
    }
    let theta = math::acos(back.dot(forward).clamp(-1.0, 1.0));
    let wanted = radius / math::tan(theta / 2.0);
    Some(Corner { vertex, back, forward, theta, wanted, used: wanted })
}

/// Rounds the corners of one subpath; `only` lists node indices (None = every corner).
fn round_parts(p: &Parts, radius: f64, only: Option<&[usize]>) -> (Parts, Stats) {
    let segs = if p.closed { p.ring() } else { p.segs.clone() };
    let m = segs.len();
    let node_count = if p.closed { m } else { m + 1 };
    let mut stats = Stats::default();
    // Normalised, de-duplicated request (closed: node `m` is node 0).
    let requested: Vec<usize> = match only {
        None => (0..node_count).filter(|&j| p.closed || (j > 0 && j < m)).collect(),
        Some(list) => {
            let mut v: Vec<usize> = list.iter().map(|&j| if p.closed && j >= m { 0 } else { j }).collect();
            v.sort_unstable();
            v.dedup();
            v
        }
    };
    let mut corners: Vec<Option<Corner>> = vec![None; node_count];
    for &j in &requested {
        if j >= node_count {
            stats.skipped += 1;
            continue;
        }
        match corner_at(&segs, p.start, p.closed, j, radius) {
            Some(c) => corners[j] = Some(c),
            None => stats.skipped += 1,
        }
    }
    if corners.iter().all(Option::is_none) {
        return (p.clone(), stats);
    }
    // Shared-segment limit: the tangent lengths taken from both ends of a segment may not exceed it.
    let seg_start = |s: usize| if s == 0 { p.start } else { segs[s - 1].end() };
    let factors: Vec<f64> = (0..m)
        .map(|s| {
            let len = seg_start(s).distance(segs[s].end());
            let end_node = if p.closed { (s + 1) % m } else { s + 1 };
            let total: f64 = [s, end_node].iter().filter_map(|&j| corners[j].map(|c| c.wanted)).sum();
            if total > len { len / total } else { 1.0 }
        })
        .collect();
    for (j, corner) in corners.iter_mut().enumerate() {
        let Some(c) = corner.as_mut() else { continue };
        let (inc, out) = if p.closed { ((j + m - 1) % m, j) } else { (j.saturating_sub(1), j) };
        c.used = c.wanted * factors[inc].min(factors[out]);
    }
    let arcs: Vec<Option<(Point, Point, EllipticalArc)>> = corners
        .iter()
        .map(|c| {
            let c = c.as_ref()?;
            let r = c.used * math::tan(c.theta / 2.0);
            let (t1, t2) = (c.vertex + c.back * c.used, c.vertex + c.forward * c.used);
            let centre = c.vertex + unit(c.back + c.forward)? * (c.used / math::cos(c.theta / 2.0));
            let first = t1 - centre;
            let left_turn = c.back.cross(c.forward) < 0.0;
            let sweep = if left_turn { std::f64::consts::PI - c.theta } else { c.theta - std::f64::consts::PI };
            let arc = EllipticalArc::circle(
                centre,
                r,
                Angle::from_radians(math::atan2(first.y, first.x)).ok()?,
                Angle::from_radians(sweep).ok()?,
            )
            .ok()?;
            Some((t1, t2, arc))
        })
        .collect();
    for (c, a) in corners.iter().zip(&arcs) {
        match (c, a) {
            (Some(c), Some(_)) => {
                stats.rounded += 1;
                if c.used < c.wanted * (1.0 - 1e-12) {
                    stats.limited += 1;
                }
            }
            (Some(_), None) => stats.skipped += 1,
            _ => {}
        }
    }
    let rounded = |j: usize| arcs[j].is_some();
    let start = match arcs.first().and_then(Option::as_ref) {
        Some((_, t2, _)) if p.closed => *t2,
        _ => p.start,
    };
    let mut out: Vec<Segment> = Vec::with_capacity(m + node_count);
    let mut at = start;
    for (s, seg) in segs.iter().enumerate() {
        let end_node = if p.closed { (s + 1) % m } else { s + 1 };
        if rounded(end_node) {
            let Some((t1, t2, arc)) = arcs[end_node].as_ref() else { continue };
            if at.distance(*t1) > NO_DIRECTION {
                out.push(Segment::Line { to: *t1 });
            }
            out.push(Segment::Arc(*arc));
            at = *t2;
        } else {
            out.push(*seg);
            at = seg.end();
        }
    }
    let parts = if p.closed { Parts::from_ring(start, out) } else { Parts { start, segs: out, closed: false } };
    (parts, stats)
}

pub(crate) fn round_single(p: &Parts, node: NodeRef, radius: f64) -> Result<Parts, NodeEditError> {
    if !(radius.is_finite() && radius > 0.0) {
        return Err(NodeEditError::InvalidParameter);
    }
    if node.node > p.segs.len() {
        return Err(NodeEditError::NoSuchNode(node));
    }
    let (parts, stats) = round_parts(p, radius, Some(&[node.node]));
    if stats.rounded == 0 {
        return Err(NodeEditError::NotACorner(node));
    }
    Ok(parts)
}
