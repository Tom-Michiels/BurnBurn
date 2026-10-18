use crate::math;
use crate::segment::{EllipticalArc, Segment};
use crate::tolerance::FlatteningTolerance;
use crate::{Path, Point, Subpath};

/// A flattened subpath: straight chords through `points`, closed back to the first point
/// when `closed`.
#[derive(Debug, Clone, PartialEq)]
pub struct Polyline {
    pub points: Vec<Point>,
    pub closed: bool,
}

impl Path {
    /// Approximates every curve by chords whose deviation from the exact curve is at most
    /// `tolerance`. Lines are kept exact.
    pub fn flatten(&self, tolerance: &impl FlatteningTolerance) -> Vec<Polyline> {
        let tol = tolerance.max_deviation_mm();
        self.subpaths().iter().map(|sp| flatten_subpath(sp, tol)).collect()
    }
}

pub(crate) fn flatten_subpath(sp: &Subpath, tol: f64) -> Polyline {
    let mut points = vec![sp.start()];
    let mut current = sp.start();
    for seg in sp.segments() {
        match *seg {
            Segment::Line { to } => points.push(to),
            Segment::Cubic { c1, c2, to } => flatten_cubic(current, c1, c2, to, tol, &mut points),
            Segment::Arc(arc) => flatten_arc(&arc, tol, &mut points),
        }
        current = seg.end();
    }
    Polyline { points, closed: sp.is_closed() }
}

/// Subdivision depth cap: 2^24 chords per cubic bounds work for any finite input.
const MAX_CUBIC_DEPTH: u32 = 24;
/// Chord-count cap per arc; bounds output size for extreme radius/tolerance ratios.
const MAX_ARC_CHORDS: f64 = (1u32 << 20) as f64;

fn distance_to_segment(q: Point, a: Point, b: Point) -> f64 {
    let ab = b - a;
    let len2 = ab.dot(ab);
    let t = if len2 > 0.0 { ((q - a).dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
    q.distance(a + ab * t)
}

/// Recursive de Casteljau subdivision. The curve lies in the convex hull of its control
/// points, so the chord deviation is at most the control points' distance to the chord.
fn flatten_cubic(p0: Point, p1: Point, p2: Point, p3: Point, tol: f64, out: &mut Vec<Point>) {
    fn go(p0: Point, p1: Point, p2: Point, p3: Point, tol: f64, depth: u32, out: &mut Vec<Point>) {
        let flat = distance_to_segment(p1, p0, p3).max(distance_to_segment(p2, p0, p3)) <= tol;
        if flat || depth >= MAX_CUBIC_DEPTH {
            out.push(p3);
            return;
        }
        let (a, b, c) = (p0.lerp(p1, 0.5), p1.lerp(p2, 0.5), p2.lerp(p3, 0.5));
        let (d, e) = (a.lerp(b, 0.5), b.lerp(c, 0.5));
        let m = d.lerp(e, 0.5);
        go(p0, a, d, m, tol, depth + 1, out);
        go(m, e, c, p3, tol, depth + 1, out);
    }
    go(p0, p1, p2, p3, tol, 0, out);
}

/// Uniform parameter steps. The arc is the affine image of a unit circle, whose chord
/// sagitta is `1 − cos(Δt/2)`; the image scales it by at most the largest singular value.
fn flatten_arc(arc: &EllipticalArc, tol: f64, out: &mut Vec<Point>) {
    let (u, v) = arc.semi_diameters();
    // Largest singular value of the 2x2 matrix [u v].
    let (p, q, r) = (u.dot(u), u.dot(v), v.dot(v));
    let sigma = (0.5 * (p + r + ((p - r) * (p - r) + 4.0 * q * q).sqrt())).sqrt();
    let sweep = arc.sweep_parameter().abs();
    let step = if tol >= sigma { std::f64::consts::PI } else { 2.0 * math::acos(1.0 - tol / sigma) };
    let chords = (sweep / step).ceil().clamp(1.0, MAX_ARC_CHORDS);
    let n = chords as u32;
    let (t0, dt) = (arc.start_parameter(), arc.sweep_parameter() / chords);
    for i in 1..n {
        out.push(arc.point_at_parameter(t0 + dt * f64::from(i)));
    }
    out.push(arc.end_point());
}
