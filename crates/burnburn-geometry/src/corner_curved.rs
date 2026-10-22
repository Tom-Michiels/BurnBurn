//! Fillet and chamfer of a corner where at least one side is a curve (arc or Bézier).
//!
//! A fillet is the circle of the given radius that touches both sides. Its centre `c` lies on
//! the inner side of both curves, so for the points `P(s)` of the first side and `Q(t)` of the
//! second, with unit normals `N` towards that side, `P + rN = Q + rN' = c`. That is two
//! equations in the two parameters `(s, t)`; they are solved by damped Newton iterations from
//! a grid of starting points and the solution nearest to the corner is kept. Chamfer lengths
//! are arc lengths measured from the corner.

use std::f64::consts::{PI, TAU};

use burnburn_types::Angle;

use crate::corner::{CornerCut, CornerError};
use crate::intersect::segment_intersections;
use crate::ring::Ring;
use crate::{EllipticalArc, Point, Segment, Vector, math};

const EPS: f64 = 1e-9;
/// Newton stops when the centre of both offsets agrees to this (mm).
const SOLVE_TOL: f64 = 1e-11;
const MAX_ITER: usize = 60;
/// Simpson intervals for an arc length; the integrand is smooth on one segment.
const LENGTH_STEPS: usize = 512;

/// A side of the corner with the point it starts at.
#[derive(Clone, Copy)]
struct Side {
    from: Point,
    seg: Segment,
}

impl Side {
    fn at(&self, t: f64) -> Point {
        self.seg.point_at(self.from, t)
    }

    fn velocity(&self, t: f64) -> Vector {
        self.seg.derivative(self.from, t)
    }

    /// The unit normal to the left of the direction of travel.
    fn left_normal(&self, t: f64) -> Option<Vector> {
        let v = self.velocity(t);
        let l = v.length();
        (l.is_finite() && l > EPS).then(|| Vector::new(-v.y / l, v.x / l))
    }

    fn unit_tangent(&self, t: f64) -> Option<Vector> {
        let v = self.velocity(t);
        let l = v.length();
        (l.is_finite() && l > EPS).then(|| v * (1.0 / l))
    }

    /// Arc length between the parameters `t0 <= t1`.
    fn length(&self, t0: f64, t1: f64) -> f64 {
        if t1 <= t0 {
            return 0.0;
        }
        if let Segment::Line { .. } = self.seg {
            return self.velocity(0.0).length() * (t1 - t0);
        }
        let h = (t1 - t0) / LENGTH_STEPS as f64;
        let f = |i: usize| self.velocity(t0 + h * i as f64).length();
        let mut sum = f(0) + f(LENGTH_STEPS);
        for i in 1..LENGTH_STEPS {
            sum += f(i) * if i % 2 == 1 { 4.0 } else { 2.0 };
        }
        sum * h / 3.0
    }

    /// The parameter whose distance along the side from the corner is `d`. The corner is the
    /// end (`t = 1`) of the incoming side and the start (`t = 0`) of the outgoing one.
    fn param_at_distance(&self, d: f64, from_end: bool) -> f64 {
        let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
        for _ in 0..80 {
            let mid = 0.5 * (lo + hi);
            let walked = if from_end { self.length(mid, 1.0) } else { self.length(0.0, mid) };
            // `walked` grows towards the far end of the side.
            if (walked < d) == from_end {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        0.5 * (lo + hi)
    }
}

fn angle(r: f64) -> Result<Angle, CornerError> {
    Angle::from_radians(r).map_err(|_| CornerError::InvalidValue)
}

/// `(f, g)` where the offsets of both sides meet: `f` is the vector between them.
fn offset_gap(a: &Side, b: &Side, r: f64, sigma: f64, s: f64, t: f64) -> Option<Vector> {
    let (na, nb) = (a.left_normal(s)?, b.left_normal(t)?);
    let g = (a.at(s) + na * (sigma * r)) - (b.at(t) + nb * (sigma * r));
    (g.x.is_finite() && g.y.is_finite()).then_some(g)
}

/// Solves for the tangent circle: parameters `(s, t)` on the incoming and outgoing side.
fn solve_fillet(a: &Side, b: &Side, r: f64, sigma: f64) -> Option<(f64, f64)> {
    let (la, lb) = (a.length(0.0, 1.0), b.length(0.0, 1.0));
    let mut best: Option<(f64, f64, f64)> = None;
    for s0 in [0.95, 0.8, 0.6, 0.4, 0.2, 0.05] {
        for t0 in [0.05, 0.2, 0.4, 0.6, 0.8, 0.95] {
            let Some((s, t)) = newton(a, b, r, sigma, s0, t0) else { continue };
            if !((-EPS..=1.0 + EPS).contains(&s) && (-EPS..=1.0 + EPS).contains(&t)) {
                continue;
            }
            let (s, t) = (s.clamp(0.0, 1.0), t.clamp(0.0, 1.0));
            // How much of both sides the fillet takes away: the nearest solution wins.
            let taken = (1.0 - s) * la + t * lb;
            if best.is_none_or(|b| taken < b.2 - 1e-9) {
                best = Some((s, t, taken));
            }
        }
    }
    best.map(|(s, t, _)| (s, t))
}

fn newton(a: &Side, b: &Side, r: f64, sigma: f64, mut s: f64, mut t: f64) -> Option<(f64, f64)> {
    const H: f64 = 1e-6;
    for _ in 0..MAX_ITER {
        let g = offset_gap(a, b, r, sigma, s, t)?;
        if g.length() < SOLVE_TOL {
            return Some((s, t));
        }
        let ds = (offset_gap(a, b, r, sigma, s + H, t)? - offset_gap(a, b, r, sigma, s - H, t)?) * (0.5 / H);
        let dt = (offset_gap(a, b, r, sigma, s, t + H)? - offset_gap(a, b, r, sigma, s, t - H)?) * (0.5 / H);
        let det = ds.cross(dt);
        if !(det.abs() > 1e-14) {
            return None;
        }
        // Solve [ds dt] [x y]^T = -g.
        let x = (-g).cross(dt) / det;
        let y = ds.cross(-g) / det;
        // Damp the step so one iteration never leaves the neighbourhood of the side.
        let scale = (0.5 / x.abs().max(y.abs()).max(1e-300)).min(1.0);
        s += x * scale;
        t += y * scale;
        if !(s.is_finite() && t.is_finite()) || s.abs() > 3.0 || t.abs() > 3.0 {
            return None;
        }
    }
    None
}

/// Cuts the corner between segment `ring.segs[prev]` and `ring.segs[i]` when either is
/// curved. Returns the start point and the segments of the rebuilt ring.
pub(crate) fn cut_curved(
    ring: &Ring,
    i: usize,
    prev_is_first: bool,
    cut: CornerCut,
    subpath_start: Point,
) -> Result<(Point, Vec<Segment>), CornerError> {
    let m = ring.len();
    let prev = (i + m - 1) % m;
    let a = Side { from: ring.starts[prev], seg: ring.segs[prev] };
    let b = Side { from: ring.starts[i], seg: ring.segs[i] };
    let (ta, tb) = (a.unit_tangent(1.0).ok_or(CornerError::Collinear)?, b.unit_tangent(0.0).ok_or(CornerError::Collinear)?);
    let sin_turn = ta.cross(tb);
    if sin_turn.abs() < 1e-9 {
        return Err(CornerError::Collinear);
    }
    // Left turn: the cut lies on the left of both sides.
    let sigma = sin_turn.signum();
    let (la, lb) = (a.length(0.0, 1.0), b.length(0.0, 1.0));
    let order = |first: f64, second: f64| if prev_is_first { (first, second) } else { (second, first) };
    let (s, t, join) = match cut {
        CornerCut::Fillet { radius } => {
            let (s, t) = solve_fillet(&a, &b, radius, sigma).ok_or(CornerError::TooLarge)?;
            let na = a.left_normal(s).ok_or(CornerError::TooLarge)?;
            let (p, q) = (a.at(s), b.at(t));
            let centre = p + na * (sigma * radius);
            let start = math::atan2(p.y - centre.y, p.x - centre.x);
            let end = math::atan2(q.y - centre.y, q.x - centre.x);
            let sweep = if sigma > 0.0 { (end - start).rem_euclid(TAU) } else { -(start - end).rem_euclid(TAU) };
            if sweep.abs() < 1e-12 {
                return Err(CornerError::TooLarge);
            }
            let arc = EllipticalArc::circle(centre, radius, angle(start)?, angle(sweep)?)
                .map_err(|_| CornerError::InvalidValue)?;
            (s, t, Segment::Arc(arc))
        }
        CornerCut::ChamferEqual { distance } => chamfer_by_distance(&a, &b, la, lb, distance, distance)?,
        CornerCut::ChamferTwo { first, second } => {
            let (d1, d2) = order(first, second);
            chamfer_by_distance(&a, &b, la, lb, d1, d2)?
        }
        CornerCut::ChamferAngle { distance, angle_deg } => {
            chamfer_by_angle(&a, &b, la, lb, prev_is_first, distance, angle_deg)?
        }
    };
    rebuild_curved(ring, i, s, t, join, subpath_start)
}

fn chamfer_by_distance(a: &Side, b: &Side, la: f64, lb: f64, d1: f64, d2: f64) -> Result<(f64, f64, Segment), CornerError> {
    if d1 > la + EPS || d2 > lb + EPS {
        return Err(CornerError::TooLarge);
    }
    let s = a.param_at_distance(d1.min(la), true);
    let t = b.param_at_distance(d2.min(lb), false);
    Ok((s, t, Segment::Line { to: b.at(t) }))
}

fn chamfer_by_angle(
    a: &Side,
    b: &Side,
    la: f64,
    lb: f64,
    prev_is_first: bool,
    distance: f64,
    angle_deg: f64,
) -> Result<(f64, f64, Segment), CornerError> {
    let (first, other, first_len) = if prev_is_first { (a, b, la) } else { (b, a, lb) };
    if distance > first_len + EPS {
        return Err(CornerError::TooLarge);
    }
    let p1 = first.param_at_distance(distance.min(first_len), prev_is_first);
    let origin = first.at(p1);
    // The direction from the cut point back to the corner, turned by the angle towards the
    // inside of the corner (the side on which the other curve starts).
    let tangent = first.unit_tangent(p1).ok_or(CornerError::InvalidValue)?;
    let towards_corner = if prev_is_first { tangent } else { tangent * -1.0 };
    let probe = if prev_is_first { other.at(0.1) } else { other.at(0.9) };
    let side = towards_corner.cross(probe - origin);
    if !(side.abs() > EPS) {
        return Err(CornerError::InvalidValue);
    }
    let alpha = angle_deg.to_radians() * side.signum();
    if !(angle_deg.to_radians() > 1e-6 && angle_deg.to_radians() < PI - 1e-6) {
        return Err(CornerError::InvalidValue);
    }
    let (sn, cs) = (math::sin(alpha), math::cos(alpha));
    let dir = Vector::new(towards_corner.x * cs - towards_corner.y * sn, towards_corner.x * sn + towards_corner.y * cs);
    let reach = 4.0 * (la + lb) + 1.0;
    let ray = Segment::Line { to: origin + dir * reach };
    let mut hit: Option<(f64, f64)> = None; // (parameter on the other side, distance from the corner)
    for (ta, tb, _) in segment_intersections(origin, &ray, other.from, &other.seg) {
        if ta * reach <= EPS {
            continue;
        }
        let nearer = if prev_is_first { hit.is_none_or(|h| tb < h.0) } else { hit.is_none_or(|h| tb > h.0) };
        if nearer {
            hit = Some((tb, ta));
        }
    }
    let (tb, _) = hit.ok_or(CornerError::InvalidValue)?;
    let (s, t) = if prev_is_first { (p1, tb) } else { (tb, p1) };
    // The end of the chamfer on the other side must be a point of that side, not its far end.
    if (prev_is_first && tb >= 1.0 - EPS) || (!prev_is_first && tb <= EPS) {
        return Err(CornerError::TooLarge);
    }
    Ok((s, t, Segment::Line { to: b.at(t) }))
}

fn rebuild_curved(
    ring: &Ring,
    i: usize,
    s: f64,
    t: f64,
    join: Segment,
    subpath_start: Point,
) -> Result<(Point, Vec<Segment>), CornerError> {
    let m = ring.len();
    let prev = (i + m - 1) % m;
    let to_a = prev as f64 + s;
    let from_b = i as f64 + t;
    let mut out: Vec<Segment> = Vec::with_capacity(m + 2);
    if i == 0 {
        // The corner is the ring start: the ring now starts at the cut on the outgoing side.
        let start = ring.segs[0].point_at(ring.starts[0], t);
        if let Some((_, segs)) = ring.slice(from_b, to_a) {
            out.extend(segs);
        }
        out.push(join);
        return Ok((start, out));
    }
    if let Some((_, segs)) = ring.slice(0.0, to_a) {
        out.extend(segs);
    }
    out.push(join);
    if let Some((_, segs)) = ring.slice(from_b, m as f64) {
        out.extend(segs);
    }
    if ring.closed && matches!(out.last(), Some(Segment::Line { to }) if to.distance(subpath_start) <= EPS) {
        out.pop();
    }
    Ok((subpath_start, out))
}
