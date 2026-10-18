use std::f64::consts::{PI, TAU};

use burnburn_types::Angle;

use crate::math;
use crate::{Affine, Bounds, GeometryError, Point, Vector};

/// An elliptical arc `p(t) = center + u·cos t + v·sin t` for `t` from `start` to `start + sweep`.
///
/// `u` and `v` are conjugate semi-diameters. A circle has `u ⟂ v` and `|u| = |v|`. This form is
/// closed under every affine transform, so transforming an arc never approximates it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EllipticalArc {
    center: Point,
    u: Vector,
    v: Vector,
    start: f64,
    sweep: f64,
}

impl EllipticalArc {
    /// `sweep` is signed: positive runs counter-clockwise when `u × v > 0`. `|sweep| <= 2π`.
    pub fn new(center: Point, u: Vector, v: Vector, start: Angle, sweep: Angle) -> Result<Self, GeometryError> {
        if !(center.is_finite() && u.is_finite() && v.is_finite()) {
            return Err(GeometryError::NotFinite);
        }
        if u.cross(v) == 0.0 {
            return Err(GeometryError::DegenerateArc);
        }
        if sweep.radians().abs() > TAU {
            return Err(GeometryError::DegenerateArc);
        }
        Ok(EllipticalArc { center, u, v, start: start.radians(), sweep: sweep.radians() })
    }

    /// A circular arc of `radius` around `center`, angles measured counter-clockwise from +x.
    pub fn circle(center: Point, radius: f64, start: Angle, sweep: Angle) -> Result<Self, GeometryError> {
        if !(radius > 0.0) {
            return Err(GeometryError::DegenerateArc);
        }
        EllipticalArc::new(center, Vector::new(radius, 0.0), Vector::new(0.0, radius), start, sweep)
    }

    pub fn center(&self) -> Point {
        self.center
    }

    pub fn semi_diameters(&self) -> (Vector, Vector) {
        (self.u, self.v)
    }

    pub fn start_parameter(&self) -> f64 {
        self.start
    }

    pub fn sweep_parameter(&self) -> f64 {
        self.sweep
    }

    pub fn point_at_parameter(&self, t: f64) -> Point {
        let (s, c) = math::sin_cos(t);
        self.center + self.u * c + self.v * s
    }

    pub fn start_point(&self) -> Point {
        self.point_at_parameter(self.start)
    }

    pub fn end_point(&self) -> Point {
        self.point_at_parameter(self.start + self.sweep)
    }

    pub fn transform(&self, m: &Affine) -> EllipticalArc {
        EllipticalArc { center: m.apply(self.center), u: m.apply_vector(self.u), v: m.apply_vector(self.v), ..*self }
    }

    /// The same curve traversed backwards.
    pub fn reversed(&self) -> EllipticalArc {
        EllipticalArc { start: self.start + self.sweep, sweep: -self.sweep, ..*self }
    }

    /// Whether parameter `t` (any branch) lies within the swept interval.
    fn sweeps_through(&self, t: f64) -> bool {
        let (lo, span) =
            if self.sweep >= 0.0 { (self.start, self.sweep) } else { (self.start + self.sweep, -self.sweep) };
        (t - lo).rem_euclid(TAU) <= span
    }

    fn include_in(&self, b: &mut Bounds) {
        b.include(self.start_point());
        b.include(self.end_point());
        // x(t) = cx + ux cos t + vx sin t is extremal where tan t = vx/ux (and that + π).
        for (cu, cv) in [(self.u.x, self.v.x), (self.u.y, self.v.y)] {
            let t0 = math::atan2(cv, cu);
            for t in [t0, t0 + PI] {
                if self.sweeps_through(t) {
                    b.include(self.point_at_parameter(t));
                }
            }
        }
    }

    /// ∮ x dy − y dx over the arc (twice the signed area contribution).
    fn double_area(&self) -> f64 {
        // cross(p, p') = cross(c, p') + cross(u, v); integrating gives the closed form below.
        let chord = self.end_point() - self.start_point();
        self.center.to_vector().cross(chord) + self.u.cross(self.v) * self.sweep
    }
}

impl EllipticalArc {
    /// The point at fraction `s ∈ [0, 1]` of the swept parameter range.
    pub fn point_at_fraction(&self, s: f64) -> Point {
        self.point_at_parameter(self.start + self.sweep * s)
    }

    /// The two arcs on either side of fraction `s ∈ (0, 1)`; exact (same ellipse, split parameter).
    pub fn split_at_fraction(&self, s: f64) -> (EllipticalArc, EllipticalArc) {
        let mid = self.start + self.sweep * s;
        (
            EllipticalArc { sweep: self.sweep * s, ..*self },
            EllipticalArc { start: mid, sweep: self.sweep * (1.0 - s), ..*self },
        )
    }

    /// Cubic approximation, one cubic per at most a quarter turn of parameter.
    ///
    /// On the unit circle each piece uses `k = 4/3 · tan(Δ/4)`; the radial error of a quarter
    /// turn is at most 2.7e-4 of the radius, and an affine image scales it by the largest
    /// semi-axis. The pieces' endpoints lie exactly on the arc.
    pub fn to_cubics(&self) -> Vec<Segment> {
        let pieces = (self.sweep.abs() / (PI / 2.0)).ceil().max(1.0);
        let n = pieces as u32;
        let delta = self.sweep / pieces;
        let k = 4.0 / 3.0 * math::tan(delta / 4.0);
        let map = |x: f64, y: f64| self.center + self.u * x + self.v * y;
        (0..n)
            .map(|i| {
                let t0 = self.start + delta * f64::from(i);
                let t1 = if i + 1 == n { self.start + self.sweep } else { t0 + delta };
                let (s0, c0) = math::sin_cos(t0);
                let (s1, c1) = math::sin_cos(t1);
                Segment::Cubic { c1: map(c0 - k * s0, s0 + k * c0), c2: map(c1 + k * s1, s1 - k * c1), to: map(c1, s1) }
            })
            .collect()
    }
}

/// One drawing step from the current point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segment {
    Line {
        to: Point,
    },
    /// Cubic Bézier from the current point with control points `c1`, `c2`.
    Cubic {
        c1: Point,
        c2: Point,
        to: Point,
    },
    /// An arc starting (within the builder's continuity rule) at the current point.
    Arc(EllipticalArc),
}

impl Segment {
    pub fn end(&self) -> Point {
        match *self {
            Segment::Line { to } | Segment::Cubic { to, .. } => to,
            Segment::Arc(arc) => arc.end_point(),
        }
    }

    pub(crate) fn is_finite(&self) -> bool {
        match *self {
            Segment::Line { to } => to.is_finite(),
            Segment::Cubic { c1, c2, to } => c1.is_finite() && c2.is_finite() && to.is_finite(),
            Segment::Arc(_) => true,
        }
    }

    pub(crate) fn transform(&self, m: &Affine) -> Segment {
        match *self {
            Segment::Line { to } => Segment::Line { to: m.apply(to) },
            Segment::Cubic { c1, c2, to } => Segment::Cubic { c1: m.apply(c1), c2: m.apply(c2), to: m.apply(to) },
            Segment::Arc(arc) => Segment::Arc(arc.transform(m)),
        }
    }

    /// Extends `b` with the exact extent of this segment starting at `from`.
    pub(crate) fn include_in(&self, from: Point, b: &mut Bounds) {
        match *self {
            Segment::Line { to } => b.include(to),
            Segment::Cubic { c1, c2, to } => {
                b.include(to);
                for t in
                    cubic_extrema(from.x, c1.x, c2.x, to.x).into_iter().chain(cubic_extrema(from.y, c1.y, c2.y, to.y))
                {
                    b.include(cubic_point(from, c1, c2, to, t));
                }
            }
            Segment::Arc(arc) => arc.include_in(b),
        }
    }

    /// ∫ x dy − y dx along the segment from `from`.
    pub(crate) fn double_area(&self, from: Point) -> f64 {
        match *self {
            Segment::Line { to } => from.to_vector().cross(to.to_vector()),
            Segment::Cubic { c1, c2, to } => {
                // cross(B, B') is a degree-5 polynomial: 3-point Gauss-Legendre is exact.
                const NODES: [(f64, f64); 3] =
                    [(0.112_701_665_379_258_31, 5.0 / 18.0), (0.5, 8.0 / 18.0), (0.887_298_334_620_741_7, 5.0 / 18.0)];
                NODES
                    .iter()
                    .map(|&(t, w)| {
                        let p = cubic_point(from, c1, c2, to, t).to_vector();
                        w * p.cross(cubic_derivative(from, c1, c2, to, t))
                    })
                    .sum()
            }
            Segment::Arc(arc) => arc.double_area(),
        }
    }
}

impl Segment {
    /// The point at parameter `t ∈ [0, 1]` of this segment starting at `from`.
    pub fn point_at(&self, from: Point, t: f64) -> Point {
        match *self {
            Segment::Line { to } => from.lerp(to, t),
            Segment::Cubic { c1, c2, to } => cubic_point(from, c1, c2, to, t),
            Segment::Arc(arc) => arc.point_at_fraction(t),
        }
    }

    /// Splits at `t ∈ (0, 1)` without changing the curve (De Casteljau for cubics).
    pub fn split(&self, from: Point, t: f64) -> (Segment, Segment) {
        match *self {
            Segment::Line { to } => (Segment::Line { to: from.lerp(to, t) }, Segment::Line { to }),
            Segment::Cubic { c1, c2, to } => {
                let (a, b, c) = (from.lerp(c1, t), c1.lerp(c2, t), c2.lerp(to, t));
                let (d, e) = (a.lerp(b, t), b.lerp(c, t));
                let m = d.lerp(e, t);
                (Segment::Cubic { c1: a, c2: d, to: m }, Segment::Cubic { c1: e, c2: c, to })
            }
            Segment::Arc(arc) => {
                let (a, b) = arc.split_at_fraction(t);
                (Segment::Arc(a), Segment::Arc(b))
            }
        }
    }

    /// The same curve traversed backwards, ending at `from`.
    pub fn reversed(&self, from: Point) -> Segment {
        match *self {
            Segment::Line { .. } => Segment::Line { to: from },
            Segment::Cubic { c1, c2, .. } => Segment::Cubic { c1: c2, c2: c1, to: from },
            Segment::Arc(arc) => Segment::Arc(arc.reversed()),
        }
    }

    /// The parameter-space derivative at `t` (not normalised).
    pub fn derivative(&self, from: Point, t: f64) -> Vector {
        match *self {
            Segment::Line { to } => to - from,
            Segment::Cubic { c1, c2, to } => cubic_derivative(from, c1, c2, to, t),
            Segment::Arc(arc) => {
                let p = arc.start + arc.sweep * t;
                let (s, c) = math::sin_cos(p);
                (arc.v * c - arc.u * s) * arc.sweep
            }
        }
    }

    /// The second parameter-space derivative at `t`.
    pub(crate) fn second_derivative(&self, from: Point, t: f64) -> Vector {
        match *self {
            Segment::Line { .. } => Vector::ZERO,
            Segment::Cubic { c1, c2, to } => {
                let a = (from - c1) + (c2 - c1);
                let b = (c1 - c2) + (to - c2);
                (a * (1.0 - t) + b * t) * 6.0
            }
            Segment::Arc(arc) => {
                let p = arc.start + arc.sweep * t;
                let (s, c) = math::sin_cos(p);
                (arc.u * c + arc.v * s) * (-arc.sweep * arc.sweep)
            }
        }
    }

    /// Exact bounds of the segment starting at `from`.
    pub fn bounds(&self, from: Point) -> Bounds {
        let mut b = Bounds::from_point(from);
        self.include_in(from, &mut b);
        b
    }
}

pub(crate) fn cubic_point(p0: Point, p1: Point, p2: Point, p3: Point, t: f64) -> Point {
    let mt = 1.0 - t;
    let (w0, w1, w2, w3) = (mt * mt * mt, 3.0 * mt * mt * t, 3.0 * mt * t * t, t * t * t);
    Point::new(w0 * p0.x + w1 * p1.x + w2 * p2.x + w3 * p3.x, w0 * p0.y + w1 * p1.y + w2 * p2.y + w3 * p3.y)
}

fn cubic_derivative(p0: Point, p1: Point, p2: Point, p3: Point, t: f64) -> Vector {
    let mt = 1.0 - t;
    ((p1 - p0) * (3.0 * mt * mt)) + ((p2 - p1) * (6.0 * mt * t)) + ((p3 - p2) * (3.0 * t * t))
}

/// Parameters in (0, 1) where one coordinate of a cubic has zero derivative.
fn cubic_extrema(p0: f64, p1: f64, p2: f64, p3: f64) -> Vec<f64> {
    // B'(t)/3 = a t² + b t + c with the coefficients below.
    let a = -p0 + 3.0 * p1 - 3.0 * p2 + p3;
    let b = 2.0 * (p0 - 2.0 * p1 + p2);
    let c = p1 - p0;
    let mut roots = Vec::with_capacity(2);
    let scale = a.abs().max(b.abs()).max(c.abs());
    if scale == 0.0 {
        return roots;
    }
    if a.abs() <= 1e-12 * scale {
        if b != 0.0 {
            roots.push(-c / b);
        }
    } else {
        let disc = b * b - 4.0 * a * c;
        if disc >= 0.0 {
            // Numerically stable quadratic roots.
            let q = -0.5 * (b + b.signum() * disc.sqrt());
            roots.push(q / a);
            if q != 0.0 {
                roots.push(c / q);
            }
        }
    }
    roots.retain(|t| *t > 0.0 && *t < 1.0);
    roots
}
