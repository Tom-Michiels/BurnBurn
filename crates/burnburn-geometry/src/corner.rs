//! Fillet and chamfer of a corner between two straight segments (SolidCraft `sketch.fillet`,
//! `sketch.chamfer.*`).

use std::f64::consts::PI;

use burnburn_types::Angle;

use crate::corner_curved;
use crate::query::nearest_on_segment;
use crate::ring::{Ring, make_subpath};
use crate::{EllipticalArc, Path, Point, Segment, math};

/// Lengths below this are a vertex, not a piece of a side (float noise).
const EPS: f64 = 1e-9;

/// How a corner is cut. Lengths are millimetres along the two sides measured from the corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CornerCut {
    /// A tangent arc of `radius`.
    Fillet { radius: f64 },
    /// The same distance on both sides.
    ChamferEqual { distance: f64 },
    /// `first` on the side nearer to the pick, `second` on the other side.
    ChamferTwo { first: f64, second: f64 },
    /// `distance` on the side nearer to the pick; the chamfer line makes `angle_deg` with that side.
    ChamferAngle { distance: f64, angle_deg: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CornerError {
    /// No corner between two segments lies within reach of the pick.
    NoCorner,
    /// Reserved: corners between curves are cut since golf 8, so the geometry no longer returns this.
    CurvedSide,
    /// A distance, radius or angle is not a positive finite number, or the angle misses the other side.
    InvalidValue,
    /// The cut is longer than one of the two sides.
    TooLarge,
    /// The two sides are collinear or fold back on themselves: there is no corner to cut.
    Collinear,
}

impl std::fmt::Display for CornerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            CornerError::NoCorner => "no corner near the pick",
            CornerError::CurvedSide => "this kind of side cannot be cut",
            CornerError::InvalidValue => "the radius, distance or angle is not valid",
            CornerError::TooLarge => "the cut is longer than a side of the corner",
            CornerError::Collinear => "the sides do not form a corner",
        })
    }
}

impl std::error::Error for CornerError {}

/// Where the two sides of a corner are cut and what joins the cut points.
struct Cut {
    t1: Point,
    t2: Point,
    join: Segment,
}

fn positive(v: f64) -> Result<f64, CornerError> {
    if v.is_finite() && v > 0.0 { Ok(v) } else { Err(CornerError::InvalidValue) }
}

impl Path {
    /// Cuts the corner nearest to `pick` (within `reach` mm): a vertex joining two straight
    /// segments. For [`CornerCut::ChamferTwo`] and [`CornerCut::ChamferAngle`] the first side
    /// is the one nearer to `pick`. The input is never changed; on error nothing is returned.
    pub fn cut_corner(&self, pick: Point, reach: f64, cut: CornerCut) -> Result<Path, CornerError> {
        let values_ok = match cut {
            CornerCut::Fillet { radius } => positive(radius).map(|_| ()),
            CornerCut::ChamferEqual { distance } => positive(distance).map(|_| ()),
            CornerCut::ChamferTwo { first, second } => positive(first).and(positive(second)).map(|_| ()),
            CornerCut::ChamferAngle { distance, angle_deg } => positive(distance).and(positive(angle_deg)).map(|_| ()),
        };
        values_ok?;
        let (si, ring, i) = self.nearest_vertex(pick, reach).ok_or(CornerError::NoCorner)?;
        let m = ring.len();
        let prev = (i + m - 1) % m;
        let (Segment::Line { .. }, Segment::Line { to: b }) = (ring.segs[prev], ring.segs[i]) else {
            let pick_first = |ring: &Ring| {
                nearest_on_segment(ring.starts[prev], &ring.segs[prev], pick).2
                    <= nearest_on_segment(ring.starts[i], &ring.segs[i], pick).2
            };
            let (start, segs) =
                corner_curved::cut_curved(&ring, i, pick_first(&ring), cut, self.subpaths()[si].start())?;
            let mut subs = self.subpaths().to_vec();
            subs[si] = make_subpath(start, segs, ring.closed).ok_or(CornerError::InvalidValue)?;
            return Ok(Path::from_subpaths(subs));
        };
        let (a, corner) = (ring.starts[prev], ring.starts[i]);
        let (la, lb) = (a.distance(corner), corner.distance(b));
        if !(la > EPS && lb > EPS) {
            return Err(CornerError::Collinear);
        }
        let (u, w) = ((a - corner) * (1.0 / la), (b - corner) * (1.0 / lb));
        let (cos, sin) = (u.dot(w), u.cross(w).abs());
        if sin < 1e-9 {
            return Err(CornerError::Collinear);
        }
        let theta = math::atan2(sin, cos);
        let prev_is_first =
            nearest_on_segment(a, &ring.segs[prev], pick).2 <= nearest_on_segment(corner, &ring.segs[i], pick).2;
        let order = |first: f64, second: f64| if prev_is_first { (first, second) } else { (second, first) };
        let (d1, d2) = match cut {
            CornerCut::Fillet { radius } => {
                let d = radius * (1.0 + cos) / sin;
                (d, d)
            }
            CornerCut::ChamferEqual { distance } => (distance, distance),
            CornerCut::ChamferTwo { first, second } => order(first, second),
            CornerCut::ChamferAngle { distance, angle_deg } => {
                let alpha = angle_deg.to_radians();
                let opposite = PI - alpha - theta;
                if !(alpha > 1e-6 && opposite > 1e-6) {
                    return Err(CornerError::InvalidValue);
                }
                order(distance, distance * math::sin(alpha) / math::sin(opposite))
            }
        };
        if d1 > la + EPS || d2 > lb + EPS {
            return Err(CornerError::TooLarge);
        }
        let (t1, t2) = (corner + u * d1, corner + w * d2);
        let join = match cut {
            CornerCut::Fillet { radius } => {
                let bisector = u + w;
                let half_sin = ((1.0 - cos) / 2.0).sqrt();
                let centre = corner + bisector * (radius / half_sin / bisector.length());
                let start = math::atan2(t1.y - centre.y, t1.x - centre.x);
                // Travelling a → corner → b turns left when u × w < 0.
                let sweep = if u.cross(w) < 0.0 { PI - theta } else { theta - PI };
                let angle = |r: f64| Angle::from_radians(r).map_err(|_| CornerError::InvalidValue);
                Segment::Arc(
                    EllipticalArc::circle(centre, radius, angle(start)?, angle(sweep)?)
                        .map_err(|_| CornerError::InvalidValue)?,
                )
            }
            _ => Segment::Line { to: t2 },
        };
        let pieces = rebuild(&ring, i, a, b, Cut { t1, t2, join }, self.subpaths()[si].start());
        let mut subs = self.subpaths().to_vec();
        subs[si] = make_subpath(pieces.0, pieces.1, ring.closed).ok_or(CornerError::InvalidValue)?;
        Ok(Path::from_subpaths(subs))
    }

    /// The nearest vertex joining two segments of one subpath within `reach` of `q`.
    fn nearest_vertex(&self, q: Point, reach: f64) -> Option<(usize, Ring, usize)> {
        let mut best: Option<(f64, usize, Ring, usize)> = None;
        for (si, sp) in self.subpaths().iter().enumerate() {
            let ring = Ring::new(sp);
            let m = ring.len();
            let first = if ring.closed { 0 } else { 1 };
            for i in first..m {
                if m < 2 {
                    continue;
                }
                let d = ring.starts[i].distance(q);
                if d <= reach && best.as_ref().is_none_or(|b| d < b.0) {
                    best = Some((d, si, ring.clone(), i));
                }
            }
        }
        best.map(|(_, si, ring, i)| (si, ring, i))
    }
}

/// The ring with the corner at vertex `i` replaced by `cut`; returns the start point and segments.
/// `a` is the far end of the incoming side and `b` the far end of the outgoing side.
fn rebuild(ring: &Ring, i: usize, a: Point, b: Point, cut: Cut, start: Point) -> (Point, Vec<Segment>) {
    let m = ring.len();
    let line = |to: Point| Segment::Line { to };
    let mut out: Vec<Segment> = Vec::with_capacity(m + 2);
    if i == 0 {
        // The corner is the ring start: it moves to the cut point on the outgoing side.
        if cut.t2.distance(b) > EPS {
            out.push(line(b));
        }
        out.extend_from_slice(&ring.segs[1..m - 1]);
        if cut.t1.distance(a) > EPS {
            out.push(line(cut.t1));
        }
        out.push(cut.join);
        if ring.closed && matches!(out.last(), Some(Segment::Line { .. })) {
            out.pop();
        }
        return (cut.t2, out);
    }
    out.extend_from_slice(&ring.segs[..i - 1]);
    if cut.t1.distance(a) > EPS {
        out.push(line(cut.t1));
    }
    out.push(cut.join);
    if cut.t2.distance(b) > EPS {
        out.push(line(b));
    }
    out.extend_from_slice(&ring.segs[i + 1..]);
    if ring.closed && matches!(out.last(), Some(Segment::Line { to }) if to.distance(start) <= EPS) {
        out.pop();
    }
    (start, out)
}
