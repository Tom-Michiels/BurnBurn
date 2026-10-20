//! Print and Cut registration and the Center Finder (E27): fit a similarity transform to measured
//! target positions, and a circle centre to measured edge points.

use std::f64::consts::TAU;
use std::fmt;

use burnburn_types::Angle;

use crate::math;
use crate::{Affine, Circle, Point, Vector};

/// Whether the registration may change the size of the design.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScaleMode {
    /// No scaling: the scale is exactly 1; disagreement shows up as residuals.
    Fixed,
    /// A uniform scale within `1 ± max_deviation`; anything else is refused.
    Uniform { max_deviation: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegistrationOptions {
    pub scale: ScaleMode,
    /// The targets must span at least this distance (mm) in the design and on the machine.
    pub min_separation: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum RegistrationError {
    NotFinite,
    /// Print and Cut needs at least two targets.
    TooFewTargets,
    CountMismatch,
    /// All design targets lie (almost) on one point.
    DesignTargetsTooClose,
    /// All measured targets lie (almost) on one point.
    MeasuredTargetsTooClose,
    /// The best uniform scale is outside the allowed range.
    ScaleOutOfRange {
        scale: f64,
    },
    /// The targets do not determine a rotation.
    Degenerate,
    InvalidOptions,
}

impl fmt::Display for RegistrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot register: {self:?}")
    }
}

impl std::error::Error for RegistrationError {}

/// The fitted placement of a design on the machine.
#[derive(Debug, Clone, PartialEq)]
pub struct Registration {
    pub transform: Affine,
    pub rotation: Angle,
    pub scale: f64,
    /// Distance (mm) between each transformed design target and its measured position.
    pub residuals: Vec<f64>,
}

impl Registration {
    pub fn rms(&self) -> f64 {
        rms(&self.residuals)
    }

    pub fn max_residual(&self) -> f64 {
        self.residuals.iter().copied().fold(0.0, f64::max)
    }
}

fn rms(values: &[f64]) -> f64 {
    if values.is_empty() { 0.0 } else { (values.iter().map(|v| v * v).sum::<f64>() / values.len() as f64).sqrt() }
}

fn centroid(points: &[Point]) -> Point {
    let n = points.len() as f64;
    Point::new(points.iter().map(|p| p.x).sum::<f64>() / n, points.iter().map(|p| p.y).sum::<f64>() / n)
}

fn max_pairwise_distance(points: &[Point]) -> f64 {
    let mut best = 0.0f64;
    for (i, a) in points.iter().enumerate() {
        for b in &points[i + 1..] {
            best = best.max(a.distance(*b));
        }
    }
    best
}

/// Least-squares similarity (rotation, translation, optional uniform scale) from `design` onto
/// `measured`. Two targets determine it exactly; more targets average the measurement noise.
pub fn register(
    design: &[Point],
    measured: &[Point],
    options: &RegistrationOptions,
) -> Result<Registration, RegistrationError> {
    let valid_scale = match options.scale {
        ScaleMode::Fixed => true,
        ScaleMode::Uniform { max_deviation } => max_deviation.is_finite() && max_deviation >= 0.0,
    };
    if !valid_scale || !options.min_separation.is_finite() || options.min_separation < 0.0 {
        return Err(RegistrationError::InvalidOptions);
    }
    if design.len() != measured.len() {
        return Err(RegistrationError::CountMismatch);
    }
    if design.len() < 2 {
        return Err(RegistrationError::TooFewTargets);
    }
    if !design.iter().chain(measured).all(|p| p.is_finite()) {
        return Err(RegistrationError::NotFinite);
    }
    if max_pairwise_distance(design) < options.min_separation {
        return Err(RegistrationError::DesignTargetsTooClose);
    }
    if max_pairwise_distance(measured) < options.min_separation {
        return Err(RegistrationError::MeasuredTargetsTooClose);
    }
    let (dc, mc) = (centroid(design), centroid(measured));
    // Least squares over rotation θ and scale s: maximise Σ (s R d) · m, so tan θ = Σ d×m / Σ d·m.
    let (mut dot, mut cross, mut dd, mut mm) = (0.0, 0.0, 0.0, 0.0);
    for (d, m) in design.iter().zip(measured) {
        let (d, m) = (*d - dc, *m - mc);
        dot += d.dot(m);
        cross += d.cross(m);
        dd += d.dot(d);
        mm += m.dot(m);
    }
    let strength = math::hypot(dot, cross);
    if !(strength > 1e-12 * (dd * mm).sqrt()) {
        return Err(RegistrationError::Degenerate);
    }
    let rotation = Angle::from_radians(math::atan2(cross, dot)).map_err(|_| RegistrationError::NotFinite)?;
    let scale = match options.scale {
        ScaleMode::Fixed => 1.0,
        ScaleMode::Uniform { max_deviation } => {
            let s = strength / dd;
            if (s - 1.0).abs() > max_deviation {
                return Err(RegistrationError::ScaleOutOfRange { scale: s });
            }
            s
        }
    };
    let transform = Affine::translate(-dc.to_vector())
        .and_then(|t| Affine::scale(scale, scale).map(|s| t.then(&s)))
        .and_then(|t| Affine::translate(mc.to_vector()).map(|m| t.then(&Affine::rotate(rotation)).then(&m)))
        .map_err(|_| RegistrationError::NotFinite)?;
    let residuals = design.iter().zip(measured).map(|(d, m)| transform.apply(*d).distance(*m)).collect();
    Ok(Registration { transform, rotation, scale, residuals })
}

/// Why no circle was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CircleFitError {
    NotFinite,
    /// A circle needs three points.
    TooFew,
    /// Fewer than three distinct points.
    Coincident,
    /// The points lie on one line: no circle (and no centre) exists.
    Collinear,
}

impl fmt::Display for CircleFitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "no circle: {self:?}")
    }
}

impl std::error::Error for CircleFitError {}

/// A fitted circle with its misfit.
#[derive(Debug, Clone, PartialEq)]
pub struct CircleFit {
    pub circle: Circle,
    /// Per input point: distance from the centre minus the radius (outside is positive).
    pub residuals: Vec<f64>,
    /// The angle (radians) of the circle the points cover; small spans fit badly.
    pub arc_span: f64,
}

impl CircleFit {
    pub fn rms(&self) -> f64 {
        rms(&self.residuals)
    }

    pub fn max_abs_residual(&self) -> f64 {
        self.residuals.iter().map(|r| r.abs()).fold(0.0, f64::max)
    }
}

/// The circle that fits `points` best (least squares on the distance), for the Center Finder.
pub fn fit_circle(points: &[Point]) -> Result<CircleFit, CircleFitError> {
    if !points.iter().all(|p| p.is_finite()) {
        return Err(CircleFitError::NotFinite);
    }
    if points.len() < 3 {
        return Err(CircleFitError::TooFew);
    }
    let mean = centroid(points);
    let extent = points.iter().map(|p| (*p - mean).length()).fold(0.0, f64::max);
    if extent == 0.0 || distinct_count(points, 1e-9 * extent) < 3 {
        return Err(CircleFitError::Coincident);
    }
    // Work in units of the extent, about the centroid: keeps the arithmetic well conditioned.
    let q: Vec<Vector> = points.iter().map(|p| (*p - mean) * (1.0 / extent)).collect();
    if is_collinear(&q) {
        return Err(CircleFitError::Collinear);
    }
    let start = algebraic_centre(&q).ok_or(CircleFitError::Collinear)?;
    let (centre, radius) = refine_geometric(&q, start);
    if !(radius.is_finite() && radius > 0.0 && centre.is_finite()) {
        return Err(CircleFitError::Collinear);
    }
    let circle = Circle { center: mean + centre.to_vector() * extent, radius: radius * extent };
    let residuals: Vec<f64> = points.iter().map(|p| p.distance(circle.center) - circle.radius).collect();
    let arc_span = arc_span(points, circle.center);
    Ok(CircleFit { circle, residuals, arc_span })
}

fn distinct_count(points: &[Point], eps: f64) -> usize {
    let mut seen: Vec<Point> = Vec::new();
    for p in points {
        if seen.iter().all(|s| s.distance(*p) > eps) {
            seen.push(*p);
        }
    }
    seen.len()
}

/// True when all points lie on one line (the spread across the principal axis is negligible).
fn is_collinear(q: &[Vector]) -> bool {
    let (sxx, syy, sxy) = q.iter().fold((0.0, 0.0, 0.0), |a, v| (a.0 + v.x * v.x, a.1 + v.y * v.y, a.2 + v.x * v.y));
    let phi = 0.5 * math::atan2(2.0 * sxy, sxx - syy);
    let (s, c) = math::sin_cos(phi);
    let (along, across) = q
        .iter()
        .fold((0.0f64, 0.0f64), |a, v| (a.0.max((c * v.x + s * v.y).abs()), a.1.max((-s * v.x + c * v.y).abs())));
    across <= 1e-9 * along
}

/// The Kåsa algebraic fit: the centre that minimises Σ (|p|² + D x + E y + F)².
fn algebraic_centre(q: &[Vector]) -> Option<Point> {
    let (mut sxx, mut syy, mut sxy, mut sxz, mut syz) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for v in q {
        let z = v.x * v.x + v.y * v.y;
        sxx += v.x * v.x;
        syy += v.y * v.y;
        sxy += v.x * v.y;
        sxz += v.x * z;
        syz += v.y * z;
    }
    let det = sxx * syy - sxy * sxy;
    if !(det.abs() > 0.0) {
        return None;
    }
    let (d, e) = ((-sxz * syy + syz * sxy) / det, (-syz * sxx + sxz * sxy) / det);
    let c = Point::new(-d / 2.0, -e / 2.0);
    c.is_finite().then_some(c)
}

/// Gauss–Newton on the geometric distance, damped so the misfit never grows.
fn refine_geometric(q: &[Vector], start: Point) -> (Point, f64) {
    let radius_at = |c: Point| q.iter().map(|v| (c - Point::new(v.x, v.y)).length()).sum::<f64>() / q.len() as f64;
    let misfit = |c: Point| {
        let r = radius_at(c);
        q.iter().map(|v| ((c - Point::new(v.x, v.y)).length() - r).powi(2)).sum::<f64>()
    };
    let mut centre = start;
    for _ in 0..100 {
        // With r eliminated (its optimum is the mean distance), the residual is d_i − mean(d).
        let r = radius_at(centre);
        let mut jtj = [[0.0f64; 2]; 2];
        let mut jtr = [0.0f64; 2];
        let units: Vec<Vector> = q
            .iter()
            .map(|v| {
                let w = centre - Point::new(v.x, v.y);
                let l = w.length();
                if l > 0.0 { w * (1.0 / l) } else { Vector::ZERO }
            })
            .collect();
        let mean_u = Vector::new(
            units.iter().map(|u| u.x).sum::<f64>() / units.len() as f64,
            units.iter().map(|u| u.y).sum::<f64>() / units.len() as f64,
        );
        for (v, u) in q.iter().zip(&units) {
            let di = (centre - Point::new(v.x, v.y)).length();
            let (jx, jy) = (u.x - mean_u.x, u.y - mean_u.y);
            jtj[0][0] += jx * jx;
            jtj[0][1] += jx * jy;
            jtj[1][1] += jy * jy;
            jtr[0] += jx * (di - r);
            jtr[1] += jy * (di - r);
        }
        jtj[1][0] = jtj[0][1];
        let det = jtj[0][0] * jtj[1][1] - jtj[0][1] * jtj[1][0];
        if !(det.abs() > 1e-300) {
            break;
        }
        let step = Vector::new(
            -(jtj[1][1] * jtr[0] - jtj[0][1] * jtr[1]) / det,
            -(jtj[0][0] * jtr[1] - jtj[1][0] * jtr[0]) / det,
        );
        let before = misfit(centre);
        let mut damping = 1.0;
        let mut next = centre + step;
        while misfit(next) > before && damping > 1e-6 {
            damping *= 0.5;
            next = centre + step * damping;
        }
        if misfit(next) > before {
            break;
        }
        let moved = (next - centre).length();
        centre = next;
        if moved < 1e-15 {
            break;
        }
    }
    (centre, radius_at(centre))
}

/// The angle of the circle covered by the points: a full turn minus the widest gap between them.
fn arc_span(points: &[Point], centre: Point) -> f64 {
    let mut angles: Vec<f64> = points.iter().map(|p| math::atan2(p.y - centre.y, p.x - centre.x)).collect();
    angles.sort_by(f64::total_cmp);
    let mut widest = angles[0] + TAU - angles[angles.len() - 1];
    for w in angles.windows(2) {
        widest = widest.max(w[1] - w[0]);
    }
    (TAU - widest).max(0.0)
}
