//! Line, arc and circle candidates for a chain, with residuals against its skeleton points.

use std::f64::consts::TAU;

use burnburn_geometry::{EllipticalArc, Point, Segment, Subpath};
use burnburn_types::Angle;

use super::{Primitive, PrimitiveCandidate};

const GAUSS_NEWTON_ITERATIONS: usize = 20;
/// Arcs sweeping less than this are left to the line candidate.
const MIN_ARC_SWEEP_RAD: f64 = 0.17;

fn residuals(pts: &[Point], f: impl Fn(Point) -> f64) -> (f64, f64) {
    let mut max = 0.0f64;
    let mut sq = 0.0;
    for &p in pts {
        let d = f(p).abs();
        max = max.max(d);
        sq += d * d;
    }
    (max, (sq / pts.len().max(1) as f64).sqrt())
}

/// Total least-squares line; residual = perpendicular distance.
fn line(pts: &[Point]) -> Option<(Primitive, f64, f64)> {
    let (first, last) = (*pts.first()?, *pts.last()?);
    let n = pts.len() as f64;
    let c = Point::new(pts.iter().map(|p| p.x).sum::<f64>() / n, pts.iter().map(|p| p.y).sum::<f64>() / n);
    let (mut sxx, mut sxy, mut syy) = (0.0, 0.0, 0.0);
    for p in pts {
        let (dx, dy) = (p.x - c.x, p.y - c.y);
        sxx += dx * dx;
        sxy += dx * dy;
        syy += dy * dy;
    }
    let a = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    let (ux, uy) = (a.cos(), a.sin());
    let project = |p: Point| {
        let t = (p.x - c.x) * ux + (p.y - c.y) * uy;
        Point::new(c.x + t * ux, c.y + t * uy)
    };
    let (from, to) = (project(first), project(last));
    if from.distance(to) <= 1e-9 {
        return None;
    }
    let (max, rms) = residuals(pts, |p| (p.x - c.x) * -uy + (p.y - c.y) * ux);
    Some((Primitive::Line { from, to }, max, rms))
}

/// Kåsa algebraic fit, refined by Gauss–Newton on the geometric distance.
fn circle_fit(pts: &[Point]) -> Option<(Point, f64)> {
    if pts.len() < 3 {
        return None;
    }
    // Normal equations of x² + y² + D x + E y + F = 0, centred for conditioning.
    let n = pts.len() as f64;
    let (mx, my) = (pts.iter().map(|p| p.x).sum::<f64>() / n, pts.iter().map(|p| p.y).sum::<f64>() / n);
    let mut m = [[0.0f64; 4]; 3];
    for p in pts {
        let (x, y) = (p.x - mx, p.y - my);
        let row = [x, y, 1.0];
        let rhs = -(x * x + y * y);
        for i in 0..3 {
            for j in 0..3 {
                m[i][j] += row[i] * row[j];
            }
            m[i][3] += row[i] * rhs;
        }
    }
    let sol = solve3(m)?;
    let (cx, cy) = (-sol[0] / 2.0, -sol[1] / 2.0);
    let r2 = cx * cx + cy * cy - sol[2];
    if !(r2 > 0.0) {
        return None;
    }
    let (mut cx, mut cy, mut r) = (cx + mx, cy + my, r2.sqrt());
    for _ in 0..GAUSS_NEWTON_ITERATIONS {
        let mut jtj = [[0.0f64; 4]; 3];
        for p in pts {
            let (dx, dy) = (p.x - cx, p.y - cy);
            let d = dx.hypot(dy);
            if d <= 1e-12 {
                continue;
            }
            let res = d - r;
            let j = [-dx / d, -dy / d, -1.0];
            for a in 0..3 {
                for b in 0..3 {
                    jtj[a][b] += j[a] * j[b];
                }
                jtj[a][3] -= j[a] * res;
            }
        }
        let Some(step) = solve3(jtj) else { break };
        cx += step[0];
        cy += step[1];
        r += step[2];
        if step.iter().map(|s| s.abs()).fold(0.0, f64::max) < 1e-12 {
            break;
        }
    }
    (r > 0.0 && cx.is_finite() && cy.is_finite() && r.is_finite()).then_some((Point::new(cx, cy), r))
}

/// Gaussian elimination with partial pivoting on an augmented 3×4 matrix.
fn solve3(mut m: [[f64; 4]; 3]) -> Option<[f64; 3]> {
    for col in 0..3 {
        let pivot = (col..3).max_by(|&a, &b| m[a][col].abs().total_cmp(&m[b][col].abs()))?;
        if m[pivot][col].abs() < 1e-14 {
            return None;
        }
        m.swap(col, pivot);
        for row in 0..3 {
            if row != col {
                let f = m[row][col] / m[col][col];
                let pivot_row = m[col];
                for (v, p) in m[row].iter_mut().zip(pivot_row).skip(col) {
                    *v -= f * p;
                }
            }
        }
    }
    Some([m[0][3] / m[0][0], m[1][3] / m[1][1], m[2][3] / m[2][2]])
}

/// Candidates sorted by max residual.
pub(crate) fn candidates(pts: &[Point], closed: bool, tolerance: f64) -> Vec<PrimitiveCandidate> {
    let mut out = Vec::new();
    let mut push = |primitive: Primitive, max: f64, rms: f64| {
        if max.is_finite() && rms.is_finite() {
            out.push(PrimitiveCandidate {
                primitive,
                max_residual_mm: max,
                rms_residual_mm: rms,
                within_tolerance: max <= tolerance,
            });
        }
    };
    if !closed && let Some((l, max, rms)) = line(pts) {
        push(l, max, rms);
    }
    if let Some((center, radius)) = circle_fit(pts) {
        let (max, rms) = residuals(pts, |p| p.distance(center) - radius);
        // Unwrapped angle swept along the chain.
        let angle = |p: Point| (p.y - center.y).atan2(p.x - center.x);
        let mut sweep = 0.0;
        for w in pts.windows(2) {
            let mut d = angle(w[1]) - angle(w[0]);
            if d > std::f64::consts::PI {
                d -= TAU;
            } else if d < -std::f64::consts::PI {
                d += TAU;
            }
            sweep += d;
        }
        if closed || sweep.abs() >= TAU * 0.97 {
            push(Primitive::Circle { center, radius }, max, rms);
        } else if sweep.abs() >= MIN_ARC_SWEEP_RAD
            && let Some(&first) = pts.first()
        {
            push(Primitive::Arc { center, radius, start_rad: angle(first), sweep_rad: sweep }, max, rms);
        }
    }
    out.sort_by(|a, b| a.max_residual_mm.total_cmp(&b.max_residual_mm));
    out
}

/// Exact geometry of a primitive.
pub(crate) fn path_of(p: &Primitive) -> Option<Subpath> {
    match *p {
        Primitive::Line { from, to } => Subpath::new(from, vec![Segment::Line { to }], false).ok(),
        Primitive::Arc { center, radius, start_rad, sweep_rad } => {
            let arc = EllipticalArc::circle(
                center,
                radius,
                Angle::from_radians(start_rad).ok()?,
                Angle::from_radians(sweep_rad).ok()?,
            )
            .ok()?;
            Subpath::new(arc.start_point(), vec![Segment::Arc(arc)], false).ok()
        }
        Primitive::Circle { center, radius } => {
            let arc =
                EllipticalArc::circle(center, radius, Angle::from_radians(0.0).ok()?, Angle::from_radians(TAU).ok()?)
                    .ok()?;
            Subpath::new(arc.start_point(), vec![Segment::Arc(arc)], true).ok()
        }
    }
}
