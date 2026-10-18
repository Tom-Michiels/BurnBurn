//! Segment and path intersections (snapping, trim/extend, self-intersection checks).
//!
//! Line–line, line–cubic and line–arc are solved analytically. Curve–curve pairs use bounding-box
//! subdivision followed by a 2D Newton polish on the original segments.

use std::f64::consts::TAU;

use crate::math;
use crate::query::{SegmentRef, segments_of};
use crate::{Path, Point, Segment, Vector};

/// One crossing point; `a`/`b` give segment and parameter on each path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Intersection {
    pub point: Point,
    pub a: (SegmentRef, f64),
    pub b: (SegmentRef, f64),
}

/// Parameter slack at segment ends (float noise, not a user tolerance).
const PARAM_EPS: f64 = 1e-12;
/// Subdivision stops when both pieces are this small (mm) and Newton takes over.
const SUBDIVISION_SIZE_MM: f64 = 1e-7;
const MAX_DEPTH: u32 = 60;
/// Bounds the work for (nearly) coincident curves, which have infinitely many common points.
const MAX_ACTIVE_PAIRS: usize = 4096;
/// Two crossings closer than this (mm) are one.
const DEDUP_MM: f64 = 1e-9;

fn in_unit(t: f64) -> Option<f64> {
    (-PARAM_EPS..=1.0 + PARAM_EPS).contains(&t).then_some(t.clamp(0.0, 1.0))
}

/// All crossings of segment `a` (from `fa`) and `b` (from `fb`) as `(ta, tb, point)`.
pub fn segment_intersections(fa: Point, a: &Segment, fb: Point, b: &Segment) -> Vec<(f64, f64, Point)> {
    match (*a, *b) {
        (Segment::Line { to: ta }, Segment::Line { to: tb }) => line_line(fa, ta, fb, tb).into_iter().collect(),
        (Segment::Line { to }, other) => line_curve(fa, to, fb, &other),
        (other, Segment::Line { to }) => {
            line_curve(fb, to, fa, &other).into_iter().map(|(t, u, p)| (u, t, p)).collect()
        }
        _ => curve_curve(fa, a, fb, b),
    }
}

fn line_line(p: Point, p2: Point, q: Point, q2: Point) -> Option<(f64, f64, Point)> {
    let (r, s) = (p2 - p, q2 - q);
    let denom = r.cross(s);
    if !(denom.abs() > 1e-15 * r.length() * s.length()) {
        return None; // parallel or degenerate; overlaps are reported by diagnostics
    }
    let qp = q - p;
    let t = in_unit(qp.cross(s) / denom)?;
    let u = in_unit(qp.cross(r) / denom)?;
    Some((t, u, p.lerp(p2, t)))
}

fn line_param(p: Point, p2: Point, x: Point) -> f64 {
    let r = p2 - p;
    let len2 = r.dot(r);
    if len2 > 0.0 { (x - p).dot(r) / len2 } else { 0.0 }
}

/// Line `p → p2` against a cubic or arc starting at `f`.
fn line_curve(p: Point, p2: Point, f: Point, curve: &Segment) -> Vec<(f64, f64, Point)> {
    let r = p2 - p;
    let len = r.length();
    if !(len > 0.0) {
        return Vec::new();
    }
    let n = Vector::new(-r.y / len, r.x / len);
    let dist = |x: Point| n.dot(x - p);
    let mut params = Vec::new();
    match *curve {
        Segment::Cubic { c1, c2, to } => {
            let (d0, d1, d2, d3) = (dist(f), dist(c1), dist(c2), dist(to));
            // Bernstein → power basis.
            let a = -d0 + 3.0 * d1 - 3.0 * d2 + d3;
            let b = 3.0 * d0 - 6.0 * d1 + 3.0 * d2;
            let c = -3.0 * d0 + 3.0 * d1;
            params.extend(cubic_roots(a, b, c, d0));
        }
        Segment::Arc(arc) => {
            let (u, v) = arc.semi_diameters();
            let (ca, cb, cc) = (n.dot(u), n.dot(v), dist(arc.center()));
            let rr = math::hypot(ca, cb);
            if rr > 0.0 && (cc / rr).abs() <= 1.0 {
                let phi = math::atan2(cb, ca);
                let delta = math::acos((-cc / rr).clamp(-1.0, 1.0));
                for theta in [phi + delta, phi - delta] {
                    params.extend(arc_fraction(arc.start_parameter(), arc.sweep_parameter(), theta));
                }
            }
        }
        Segment::Line { .. } => {}
    }
    let mut out: Vec<(f64, f64, Point)> = Vec::new();
    for s in params {
        let Some(s) = in_unit(s) else { continue };
        let x = curve.point_at(f, s);
        if let Some(t) = in_unit(line_param(p, p2, x))
            && !out.iter().any(|o| o.2.distance(x) < DEDUP_MM)
        {
            out.push((t, s, x));
        }
    }
    out
}

/// Fractions of the swept range at which the arc passes parameter angle `theta`.
fn arc_fraction(start: f64, sweep: f64, theta: f64) -> Vec<f64> {
    if sweep == 0.0 {
        return Vec::new();
    }
    let base = (theta - start).rem_euclid(TAU);
    let mut out = Vec::new();
    for k in -1..=1 {
        let d = base + TAU * f64::from(k);
        let s = d / sweep;
        if (-PARAM_EPS..=1.0 + PARAM_EPS).contains(&s) {
            out.push(s);
        }
    }
    out
}

/// Real roots of `a t³ + b t² + c t + d`, polished by Newton.
pub(crate) fn cubic_roots(a: f64, b: f64, c: f64, d: f64) -> Vec<f64> {
    let scale = a.abs().max(b.abs()).max(c.abs()).max(d.abs());
    if scale == 0.0 {
        return Vec::new();
    }
    let mut roots = Vec::with_capacity(3);
    if a.abs() <= 1e-12 * scale {
        if b.abs() <= 1e-12 * scale {
            if c != 0.0 {
                roots.push(-d / c);
            }
        } else {
            let disc = c * c - 4.0 * b * d;
            if disc >= 0.0 {
                let q = -0.5 * (c + c.signum() * disc.sqrt());
                if q != 0.0 {
                    roots.push(q / b);
                    roots.push(d / q);
                } else {
                    roots.push(0.0);
                }
            }
        }
    } else {
        let (b, c, d) = (b / a, c / a, d / a);
        let q = (3.0 * c - b * b) / 9.0;
        let r = (9.0 * b * c - 27.0 * d - 2.0 * b * b * b) / 54.0;
        let disc = q * q * q + r * r;
        let shift = b / 3.0;
        if disc > 0.0 {
            let sq = disc.sqrt();
            roots.push(math::cbrt(r + sq) + math::cbrt(r - sq) - shift);
        } else if disc == 0.0 {
            let s = math::cbrt(r);
            roots.push(2.0 * s - shift);
            roots.push(-s - shift);
        } else {
            let theta = math::acos((r / (-q * q * q).sqrt()).clamp(-1.0, 1.0));
            let m = 2.0 * (-q).sqrt();
            for k in 0..3 {
                roots.push(m * math::cos((theta + TAU * f64::from(k)) / 3.0) - shift);
            }
        }
    }
    let f = |t: f64| ((a * t + b) * t + c) * t + d;
    let df = |t: f64| (3.0 * a * t + 2.0 * b) * t + c;
    roots
        .into_iter()
        .map(|mut t| {
            for _ in 0..4 {
                let g = df(t);
                if g == 0.0 {
                    break;
                }
                t -= f(t) / g;
            }
            t
        })
        .filter(|t| t.is_finite())
        .collect()
}

#[derive(Clone, Copy)]
struct Piece {
    from: Point,
    seg: Segment,
    t0: f64,
    t1: f64,
}

impl Piece {
    fn halves(&self) -> [Piece; 2] {
        let (a, b) = self.seg.split(self.from, 0.5);
        let mid = 0.5 * (self.t0 + self.t1);
        [Piece { from: self.from, seg: a, t0: self.t0, t1: mid }, Piece { from: a.end(), seg: b, t0: mid, t1: self.t1 }]
    }
}

fn curve_curve(fa: Point, a: &Segment, fb: Point, b: &Segment) -> Vec<(f64, f64, Point)> {
    let mut active =
        vec![(Piece { from: fa, seg: *a, t0: 0.0, t1: 1.0 }, Piece { from: fb, seg: *b, t0: 0.0, t1: 1.0 })];
    let mut seeds: Vec<(f64, f64)> = Vec::new();
    for _ in 0..MAX_DEPTH {
        let mut next = Vec::new();
        for (pa, pb) in active {
            let (ba, bb) = (pa.seg.bounds(pa.from), pb.seg.bounds(pb.from));
            if !ba.intersects(&bb) {
                continue;
            }
            let small = |b: crate::Bounds| b.width().max(b.height()) <= SUBDIVISION_SIZE_MM;
            if small(ba) && small(bb) {
                seeds.push((0.5 * (pa.t0 + pa.t1), 0.5 * (pb.t0 + pb.t1)));
                continue;
            }
            for ha in pa.halves() {
                for hb in pb.halves() {
                    next.push((ha, hb));
                }
            }
        }
        if next.is_empty() || next.len() > MAX_ACTIVE_PAIRS {
            for (pa, pb) in next.iter().take(MAX_ACTIVE_PAIRS) {
                seeds.push((0.5 * (pa.t0 + pa.t1), 0.5 * (pb.t0 + pb.t1)));
            }
            break;
        }
        active = next;
    }
    let mut out: Vec<(f64, f64, Point)> = Vec::new();
    for (s, t) in seeds {
        let (s, t) = newton2(fa, a, fb, b, s, t);
        let (pa, pb) = (a.point_at(fa, s), b.point_at(fb, t));
        if pa.distance(pb) > 1e-6 {
            continue;
        }
        if !out.iter().any(|o| o.2.distance(pa) < 1e-7) {
            out.push((s, t, pa));
        }
    }
    out
}

/// Solves `A(s) = B(t)` by Newton from `(s, t)`, staying in `[0, 1]²`.
fn newton2(fa: Point, a: &Segment, fb: Point, b: &Segment, mut s: f64, mut t: f64) -> (f64, f64) {
    for _ in 0..16 {
        let r = a.point_at(fa, s) - b.point_at(fb, t);
        let (da, db) = (a.derivative(fa, s), b.derivative(fb, t));
        // [da, −db] · [Δs, Δt]ᵀ = −r
        let det = da.x * (-db.y) - (-db.x) * da.y;
        if !(det.abs() > 0.0) {
            break;
        }
        let ds = (-r.x * (-db.y) - (-db.x) * (-r.y)) / det;
        let dt = (da.x * (-r.y) - da.y * (-r.x)) / det;
        s = (s + ds).clamp(0.0, 1.0);
        t = (t + dt).clamp(0.0, 1.0);
        if ds.abs() < 1e-16 && dt.abs() < 1e-16 {
            break;
        }
    }
    (s, t)
}

fn push_unique(out: &mut Vec<Intersection>, x: Intersection) {
    if !out.iter().any(|o| o.point.distance(x.point) < DEDUP_MM) {
        out.push(x);
    }
}

impl Path {
    /// Crossings between this path and `other` (touching counts). Points shared by several
    /// segments (a crossing exactly at a vertex) are reported once.
    pub fn intersections(&self, other: &Path) -> Vec<Intersection> {
        let (sa, sb) = (segments_of(self), segments_of(other));
        let mut out = Vec::new();
        for (ra, fa, a) in &sa {
            let ba = a.bounds(*fa);
            for (rb, fb, b) in &sb {
                if !ba.intersects(&b.bounds(*fb)) {
                    continue;
                }
                for (ta, tb, point) in segment_intersections(*fa, a, *fb, b) {
                    push_unique(&mut out, Intersection { point, a: (*ra, ta), b: (*rb, tb) });
                }
            }
        }
        out
    }

    /// Crossings of the path with itself, excluding the shared vertex of consecutive segments.
    pub fn self_intersections(&self) -> Vec<Intersection> {
        let segs = segments_of(self);
        let mut out = Vec::new();
        for (i, (ra, fa, a)) in segs.iter().enumerate() {
            let ba = a.bounds(*fa);
            for (rb, fb, b) in segs.iter().skip(i + 1) {
                if !ba.intersects(&b.bounds(*fb)) {
                    continue;
                }
                let same = ra.subpath == rb.subpath;
                let ring_len = segs.iter().filter(|(r, _, _)| r.subpath == ra.subpath).count();
                let consecutive = same && rb.segment == ra.segment + 1;
                let wrap =
                    same && ra.segment == 0 && rb.segment + 1 == ring_len && self.subpaths()[ra.subpath].is_closed();
                for (ta, tb, point) in segment_intersections(*fa, a, *fb, b) {
                    let at_joint =
                        (consecutive && ta > 1.0 - 1e-9 && tb < 1e-9) || (wrap && ta < 1e-9 && tb > 1.0 - 1e-9);
                    if !at_joint {
                        push_unique(&mut out, Intersection { point, a: (*ra, ta), b: (*rb, tb) });
                    }
                }
            }
        }
        out
    }
}
