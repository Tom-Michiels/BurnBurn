//! Array generators (IM26): placements of copies as transforms. Moved here from `burnburn-cam`
//! so that document modifiers (IM21) can use them; `burnburn-cam` re-exports them unchanged.

use std::fmt;

use crate::{Affine, GeometryError, Point, Vector, math};
use burnburn_types::Length;

/// `columns × rows` copies; copy `(i, j)` is translated by `(i · pitch_x, j · pitch_y)`, row by row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridArray {
    pub columns: u32,
    pub rows: u32,
    pub pitch_x: Length,
    pub pitch_y: Length,
}

/// `count` copies around `center` over `sweep_deg` (counter-clockwise). A full circle
/// (`|sweep| = 360°`) spaces copies by `sweep / count`; a partial sweep includes both ends
/// (`sweep / (count − 1)`). With `rotate_items` each copy turns with its position; otherwise it
/// is only translated so that `reference` moves along the circle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CircularArray {
    pub count: u32,
    pub center: Point,
    pub sweep_deg: f64,
    pub rotate_items: bool,
    pub reference: Point,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ArrayError {
    /// Zero copies, more than [`MAX_COPIES`], or a non-finite/zero sweep.
    OutOfRange { field: &'static str },
}

impl fmt::Display for ArrayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ArrayError::OutOfRange { field } => write!(f, "array {field} is out of range"),
        }
    }
}

impl std::error::Error for ArrayError {}

/// Upper bound on copies of one array.
pub const MAX_COPIES: u64 = 100_000;

/// Transforms of a grid array, row-major from the original (identity first).
pub fn grid_array(g: &GridArray) -> Result<Vec<Affine>, ArrayError> {
    let n = u64::from(g.columns) * u64::from(g.rows);
    if n == 0 || n > MAX_COPIES {
        return Err(ArrayError::OutOfRange { field: "count" });
    }
    let mut out = Vec::with_capacity(n as usize);
    for j in 0..g.rows {
        for i in 0..g.columns {
            let v = Vector::new(f64::from(i) * g.pitch_x.mm(), f64::from(j) * g.pitch_y.mm());
            out.push(Affine::translate(v).map_err(|_| ArrayError::OutOfRange { field: "pitch" })?);
        }
    }
    Ok(out)
}

/// `(sin, cos)` of `deg`, exact for multiples of 90°.
fn sin_cos_deg(deg: f64) -> (f64, f64) {
    let q = deg.rem_euclid(360.0);
    match q {
        0.0 => (0.0, 1.0),
        90.0 => (1.0, 0.0),
        180.0 => (0.0, -1.0),
        270.0 => (-1.0, 0.0),
        _ => math::sin_cos(q.to_radians()),
    }
}

/// Rotation by `deg` degrees (counter-clockwise) about `center`; exact for multiples of 90°.
pub fn rotation_about(center: Point, deg: f64) -> Result<Affine, GeometryError> {
    let (s, c) = sin_cos_deg(deg);
    // p' = R (p − c) + c
    let e = center.x - (c * center.x - s * center.y);
    let f = center.y - (s * center.x + c * center.y);
    Affine::new([c, s, -s, c, e, f])
}

/// Transforms of a circular array (identity first).
pub fn circular_array(a: &CircularArray) -> Result<Vec<Affine>, ArrayError> {
    if a.count == 0 || u64::from(a.count) > MAX_COPIES {
        return Err(ArrayError::OutOfRange { field: "count" });
    }
    if !(a.sweep_deg.is_finite() && a.sweep_deg != 0.0 && a.sweep_deg.abs() <= 360.0) {
        return Err(ArrayError::OutOfRange { field: "sweep" });
    }
    let full = a.sweep_deg.abs() == 360.0;
    let divisions = if full || a.count == 1 { f64::from(a.count) } else { f64::from(a.count - 1) };
    let bad = |_| ArrayError::OutOfRange { field: "center" };
    let mut out = Vec::with_capacity(a.count as usize);
    for k in 0..a.count {
        let deg = a.sweep_deg * f64::from(k) / divisions;
        let r = rotation_about(a.center, deg).map_err(bad)?;
        if a.rotate_items {
            out.push(r);
        } else {
            out.push(Affine::translate(r.apply(a.reference) - a.reference).map_err(bad)?);
        }
    }
    Ok(out)
}

/// Copies along a path: copy `k` carries the source from the start of `along` to the `k`-th of
/// `count` points spaced evenly by arc length (a closed path divides its whole length by `count`,
/// an open one includes both ends). With `orient` each copy also turns with the path's direction
/// there, relative to the direction at the start. Copy 0 is the source itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PathArray {
    pub count: u32,
    pub orient: bool,
}

/// Transforms of a path array (identity first). The source is expected to sit at the start of
/// `along`; the copies move it rigidly along the path, so a source elsewhere keeps its offset.
pub fn path_array(
    along: &crate::Path,
    a: &PathArray,
    tolerance: &crate::ProductionTolerance,
) -> Result<Vec<Affine>, ArrayError> {
    if a.count == 0 || u64::from(a.count) > MAX_COPIES {
        return Err(ArrayError::OutOfRange { field: "count" });
    }
    let bad = ArrayError::OutOfRange { field: "path" };
    // The first subpath that has a length; a closed one gets its closing chord.
    let (points, closed) = along
        .flatten(tolerance)
        .into_iter()
        .filter_map(|pl| {
            let mut pts = pl.points;
            if pl.closed && pts.first() != pts.last() {
                pts.extend(pts.first().copied());
            }
            let len: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
            (pts.len() >= 2 && len > 0.0 && len.is_finite()).then_some((pts, pl.closed))
        })
        .next()
        .ok_or(bad)?;
    let mut cumulative = vec![0.0];
    for w in points.windows(2) {
        cumulative.push(cumulative.last().copied().unwrap_or(0.0) + w[0].distance(w[1]));
    }
    let total = cumulative.last().copied().unwrap_or(0.0);
    let divisions = if closed { f64::from(a.count) } else { f64::from(a.count.saturating_sub(1).max(1)) };

    // Position and unit direction at arc length `s`: the segment that starts there, or the last
    // segment at the very end.
    let at = |s: f64| -> (Point, Vector) {
        let last = points.len() - 2;
        let i = cumulative.partition_point(|c| *c <= s).saturating_sub(1).min(last);
        let (p, q) = (points[i], points[i + 1]);
        let d = q - p;
        let len = math::hypot(d.x, d.y);
        let span = cumulative[i + 1] - cumulative[i];
        let t = if span > 0.0 { ((s - cumulative[i]) / span).clamp(0.0, 1.0) } else { 0.0 };
        let dir = if len > 0.0 { Vector::new(d.x / len, d.y / len) } else { Vector::new(1.0, 0.0) };
        (p.lerp(q, t), dir)
    };

    let (s0, d0) = at(0.0);
    let bad_transform = |_| ArrayError::OutOfRange { field: "path" };
    let mut out = Vec::with_capacity(a.count as usize);
    for k in 0..a.count {
        let (sk, dk) = at(total * f64::from(k) / divisions);
        if !a.orient {
            out.push(Affine::translate(sk - s0).map_err(bad_transform)?);
            continue;
        }
        // Rotation taking the start direction to this one: cos = d0·dk, sin = d0×dk.
        let (c, s) = (d0.x * dk.x + d0.y * dk.y, d0.x * dk.y - d0.y * dk.x);
        // p' = R (p − s0) + sk
        let e = sk.x - (c * s0.x - s * s0.y);
        let f = sk.y - (s * s0.x + c * s0.y);
        out.push(Affine::new([c, s, -s, c, e, f]).map_err(bad_transform)?);
    }
    Ok(out)
}
