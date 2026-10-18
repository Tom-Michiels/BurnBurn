//! Pure sketch constructions (S-draw): circles and arcs from points or tangents, three-point
//! rectangles, rounded rectangles, ellipse axes, regular polygons by their defining clicks, and
//! slots. Every function is deterministic, never panics and reports a degenerate input as a typed
//! [`ConstructError`] instead of a half result.

use std::f64::consts::{FRAC_PI_2, PI, TAU};
use std::fmt;

use burnburn_types::Angle;

use crate::math;
use crate::{EllipticalArc, GeometryError, Path, PathBuilder, Point, Rect, Vector};

/// Why a construction has no result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConstructError {
    NotFinite,
    /// Two defining points coincide.
    Coincident,
    /// The defining points lie on one line (no circle, arc or rectangle exists).
    Collinear,
    /// Defining lines are parallel where the construction needs them to intersect.
    Parallel,
    /// No circle satisfies the tangent conditions.
    NoSolution,
    RadiusNotPositive,
    /// The radius (or slot width) does not fit the shape it rounds.
    RadiusTooLarge,
    TooFewSides,
    /// An overall slot length smaller than its width.
    TooShort,
}

impl fmt::Display for ConstructError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ConstructError::NotFinite => "construction input must be finite",
            ConstructError::Coincident => "defining points coincide",
            ConstructError::Collinear => "defining points are collinear",
            ConstructError::Parallel => "defining lines are parallel",
            ConstructError::NoSolution => "no shape satisfies the conditions",
            ConstructError::RadiusNotPositive => "radius must be positive",
            ConstructError::RadiusTooLarge => "radius or width does not fit the shape",
            ConstructError::TooFewSides => "a polygon needs at least three sides",
            ConstructError::TooShort => "slot is shorter than its width",
        })
    }
}

impl std::error::Error for ConstructError {}

impl From<GeometryError> for ConstructError {
    fn from(e: GeometryError) -> Self {
        match e {
            GeometryError::NotFinite => ConstructError::NotFinite,
            _ => ConstructError::NoSolution,
        }
    }
}

/// A circle as data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Circle {
    pub center: Point,
    pub radius: f64,
}

/// An ellipse from its axes: semi-axes before rotating counter-clockwise by `rotation`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EllipseSpec {
    pub center: Point,
    pub rx: f64,
    pub ry: f64,
    pub rotation: Angle,
}

/// A regular polygon: `sides` vertices on `radius`, the first at `rotation`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PolygonSpec {
    pub center: Point,
    pub radius: f64,
    pub sides: u32,
    pub rotation: Angle,
}

/// A line through two points (infinite for tangent constructions).
pub type Line = (Point, Point);

/// Relative size below which a cross product counts as zero (collinear / parallel).
const REL_EPS: f64 = 1e-12;
/// Segments shorter than this are dropped from rounded outlines (float noise, not a tolerance).
const NOISE_MM: f64 = 1e-12;

fn finite(points: &[Point]) -> Result<(), ConstructError> {
    if points.iter().all(|p| p.is_finite()) { Ok(()) } else { Err(ConstructError::NotFinite) }
}

fn positive(v: f64) -> Result<f64, ConstructError> {
    if !v.is_finite() {
        Err(ConstructError::NotFinite)
    } else if v > 0.0 {
        Ok(v)
    } else {
        Err(ConstructError::RadiusNotPositive)
    }
}

fn angle(rad: f64) -> Result<Angle, ConstructError> {
    Angle::from_radians(rad).map_err(|_| ConstructError::NotFinite)
}

fn angle_of(v: Vector) -> f64 {
    math::atan2(v.y, v.x)
}

/// Counter-clockwise angle from direction `from` to direction `to`, in `[0, 2π)`.
fn ccw_between(from: f64, to: f64) -> f64 {
    let d = (to - from).rem_euclid(TAU);
    if d.is_finite() { d } else { 0.0 }
}

/// Circle through three points (circumcircle).
pub fn circle_through_three(a: Point, b: Point, c: Point) -> Result<Circle, ConstructError> {
    finite(&[a, b, c])?;
    let (u, v) = (b - a, c - a);
    let (lu, lv) = (u.length(), v.length());
    if lu == 0.0 || lv == 0.0 || (b - c).length() == 0.0 {
        return Err(ConstructError::Coincident);
    }
    let cross = u.cross(v);
    if cross.abs() <= REL_EPS * lu * lv {
        return Err(ConstructError::Collinear);
    }
    // Relative to `a`, which keeps the arithmetic accurate far from the origin.
    let (u2, v2) = (u.dot(u), v.dot(v));
    let d = 2.0 * cross;
    let off = Vector::new((v.y * u2 - u.y * v2) / d, (u.x * v2 - v.x * u2) / d);
    Ok(Circle { center: a + off, radius: off.length() })
}

/// A line as a unit normal `n` and offset `d` with `n · x = d`.
#[derive(Clone, Copy)]
struct Normal {
    n: Vector,
    d: f64,
}

impl Normal {
    fn of(line: Line) -> Result<Normal, ConstructError> {
        finite(&[line.0, line.1])?;
        let dir = line.1 - line.0;
        let len = dir.length();
        if len == 0.0 {
            return Err(ConstructError::Coincident);
        }
        let n = Vector::new(-dir.y / len, dir.x / len);
        Ok(Normal { n, d: n.dot(line.0.to_vector()) })
    }

    fn signed_distance(&self, p: Point) -> f64 {
        self.n.dot(p.to_vector()) - self.d
    }
}

fn side(v: f64) -> f64 {
    if v < 0.0 { -1.0 } else { 1.0 }
}

/// The circle tangent to both lines whose centre is the projection of `near` onto the bisector
/// of the sector `near` lies in; the radius follows. Parallel lines give the circle filling the
/// strip between them.
pub fn circle_tangent_two_lines(l1: Line, l2: Line, near: Point) -> Result<Circle, ConstructError> {
    let (a, b) = (Normal::of(l1)?, Normal::of(l2)?);
    finite(&[near])?;
    let s1 = side(a.signed_distance(near));
    let mut s2 = side(b.signed_distance(near));
    let mut m = a.n * s1 - b.n * s2;
    if m.length() < REL_EPS {
        // Parallel lines with the cursor outside the strip: take the strip.
        s2 = -s2;
        m = a.n * s1 - b.n * s2;
    }
    let k = s1 * a.d - s2 * b.d;
    let m2 = m.dot(m);
    if m2 < REL_EPS {
        return Err(ConstructError::NoSolution);
    }
    let t = (m.dot(near.to_vector()) - k) / m2;
    let center = near - m * t;
    let radius = s1 * a.signed_distance(center);
    if !(radius > NOISE_MM) {
        return Err(ConstructError::NoSolution);
    }
    Ok(Circle { center, radius })
}

/// The circle of the given radius tangent to both lines, in the sector `near` lies in.
pub fn circle_tangent_two_lines_radius(l1: Line, l2: Line, radius: f64, near: Point) -> Result<Circle, ConstructError> {
    let radius = positive(radius)?;
    let (a, b) = (Normal::of(l1)?, Normal::of(l2)?);
    finite(&[near])?;
    let (s1, s2) = (side(a.signed_distance(near)), side(b.signed_distance(near)));
    // s1 (n1 · c - d1) = r and s2 (n2 · c - d2) = r.
    let (r1, r2) = (a.n * s1, b.n * s2);
    let det = r1.cross(r2);
    if det.abs() < REL_EPS {
        return Err(ConstructError::Parallel);
    }
    let (e1, e2) = (radius + s1 * a.d, radius + s2 * b.d);
    let center = Point::new((e1 * r2.y - e2 * r1.y) / det, (r1.x * e2 - r2.x * e1) / det);
    Ok(Circle { center, radius })
}

/// The circle tangent to three lines (incircle or an excircle) whose centre is closest to `near`.
pub fn circle_tangent_three_lines(l1: Line, l2: Line, l3: Line, near: Point) -> Result<Circle, ConstructError> {
    let lines = [Normal::of(l1)?, Normal::of(l2)?, Normal::of(l3)?];
    finite(&[near])?;
    let mut best: Option<(f64, Circle)> = None;
    for (s2, s3) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
        let sig = [1.0, s2, s3];
        // n_i · c - sigma_i r = d_i, solved by Cramer's rule on the unknowns (cx, cy, r).
        let row = |i: usize| [lines[i].n.x, lines[i].n.y, -sig[i]];
        let (r0, r1, r2) = (row(0), row(1), row(2));
        let det3 = |m: [[f64; 3]; 3]| {
            m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
                + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
        };
        let det = det3([r0, r1, r2]);
        if det.abs() < REL_EPS {
            continue;
        }
        let d = [lines[0].d, lines[1].d, lines[2].d];
        let with = |col: usize| {
            let mut m = [r0, r1, r2];
            for (i, r) in m.iter_mut().enumerate() {
                r[col] = d[i];
            }
            det3(m) / det
        };
        let (cx, cy, r) = (with(0), with(1), with(2).abs());
        let center = Point::new(cx, cy);
        if !(r > NOISE_MM) || !center.is_finite() {
            continue;
        }
        let dist = center.distance(near);
        if best.is_none_or(|(bd, _)| dist < bd) {
            best = Some((dist, Circle { center, radius: r }));
        }
    }
    best.map(|(_, c)| c).ok_or(ConstructError::NoSolution)
}

/// The circular arc from `a` through `b` to `c`; the sweep runs the way the three clicks turn.
pub fn arc_through_three(a: Point, b: Point, c: Point) -> Result<EllipticalArc, ConstructError> {
    let circle = circle_through_three(a, b, c)?;
    let ccw = (b - a).cross(c - b) > 0.0;
    let start = angle_of(a - circle.center);
    let theta = ccw_between(start, angle_of(c - circle.center));
    let sweep = if ccw { theta } else { theta - TAU };
    EllipticalArc::circle(circle.center, circle.radius, angle(start)?, angle(sweep)?).map_err(Into::into)
}

/// The arc around `center` from `start` towards the direction of `end` (which need not lie on the
/// circle), counter-clockwise or clockwise.
pub fn arc_center_start_end(
    center: Point,
    start: Point,
    end: Point,
    ccw: bool,
) -> Result<EllipticalArc, ConstructError> {
    finite(&[center, start, end])?;
    let (rs, re) = (start - center, end - center);
    let radius = rs.length();
    if radius == 0.0 || re.length() == 0.0 {
        return Err(ConstructError::Coincident);
    }
    let a0 = angle_of(rs);
    let theta = ccw_between(a0, angle_of(re));
    if theta == 0.0 {
        return Err(ConstructError::Coincident);
    }
    let sweep = if ccw { theta } else { theta - TAU };
    EllipticalArc::circle(center, radius, angle(a0)?, angle(sweep)?).map_err(Into::into)
}

/// The arc that leaves `start` along `tangent` and ends at `end`.
pub fn arc_tangent(start: Point, tangent: Vector, end: Point) -> Result<EllipticalArc, ConstructError> {
    finite(&[start, end])?;
    if !tangent.is_finite() {
        return Err(ConstructError::NotFinite);
    }
    let len = tangent.length();
    let e = end - start;
    if len == 0.0 || e.length() == 0.0 {
        return Err(ConstructError::Coincident);
    }
    let t = tangent * (1.0 / len);
    let n = Vector::new(-t.y, t.x);
    let denom = 2.0 * e.dot(n);
    if denom.abs() <= REL_EPS * e.length() {
        return Err(ConstructError::Collinear);
    }
    // Signed radius: positive puts the centre on the left of the tangent (counter-clockwise).
    let rho = e.dot(e) / denom;
    let center = start + n * rho;
    let a0 = angle_of(start - center);
    let theta = ccw_between(a0, angle_of(end - center));
    let sweep = if rho > 0.0 { theta } else { theta - TAU };
    EllipticalArc::circle(center, rho.abs(), angle(a0)?, angle(sweep)?).map_err(Into::into)
}

/// Corners (counter-clockwise from `a` when `c` lies to the left of `a → b`) of the rectangle with
/// one side `a → b` whose opposite side passes through `c`.
pub fn rectangle_three_point(a: Point, b: Point, c: Point) -> Result<[Point; 4], ConstructError> {
    finite(&[a, b, c])?;
    let ab = b - a;
    let len = ab.length();
    if len == 0.0 {
        return Err(ConstructError::Coincident);
    }
    let n = Vector::new(-ab.y / len, ab.x / len);
    let h = (c - a).dot(n);
    if h.abs() <= REL_EPS * len {
        return Err(ConstructError::Collinear);
    }
    Ok([a, b, b + n * h, a + n * h])
}

/// An axis-aligned rectangle with all four corners rounded by `radius`.
pub fn rounded_rectangle(rect: Rect, radius: f64) -> Result<Path, ConstructError> {
    let r = positive(radius)?;
    let (w, h) = (rect.width().mm(), rect.height().mm());
    if r > w.min(h) / 2.0 {
        return Err(ConstructError::RadiusTooLarge);
    }
    let o = rect.origin();
    let (x0, y0, x1, y1) = (o.x, o.y, o.x + w, o.y + h);
    let corner = |cx: f64, cy: f64, start: f64| -> Result<EllipticalArc, ConstructError> {
        Ok(EllipticalArc::circle(Point::new(cx, cy), r, angle(start)?, angle(FRAC_PI_2)?)?)
    };
    let mut b = PathBuilder::new().move_to(Point::new(x0 + r, y0));
    let line = |b: PathBuilder, to: Point, from: Point| {
        if from.distance(to) > NOISE_MM { b.line_to(to) } else { b }
    };
    b = line(b, Point::new(x1 - r, y0), Point::new(x0 + r, y0));
    b = b.arc(corner(x1 - r, y0 + r, -FRAC_PI_2)?);
    b = line(b, Point::new(x1, y1 - r), Point::new(x1, y0 + r));
    b = b.arc(corner(x1 - r, y1 - r, 0.0)?);
    b = line(b, Point::new(x0 + r, y1), Point::new(x1 - r, y1));
    b = b.arc(corner(x0 + r, y1 - r, FRAC_PI_2)?);
    b = line(b, Point::new(x0, y0 + r), Point::new(x0, y1 - r));
    b = b.arc(corner(x0 + r, y0 + r, PI)?);
    b.close().build().map_err(Into::into)
}

/// An ellipse from its centre, the end of the major axis and any point on the ellipse's outline
/// that is off the major axis (its distance from the axis is the minor semi-axis).
pub fn ellipse_from_axes(center: Point, major_end: Point, minor: Point) -> Result<EllipseSpec, ConstructError> {
    finite(&[center, major_end, minor])?;
    let m = major_end - center;
    let rx = m.length();
    if rx == 0.0 {
        return Err(ConstructError::Coincident);
    }
    let n = Vector::new(-m.y / rx, m.x / rx);
    let ry = (minor - center).dot(n).abs();
    if ry <= REL_EPS * rx {
        return Err(ConstructError::Collinear);
    }
    Ok(EllipseSpec { center, rx, ry, rotation: angle(angle_of(m))? })
}

fn check_sides(sides: u32) -> Result<(), ConstructError> {
    if sides < 3 { Err(ConstructError::TooFewSides) } else { Ok(()) }
}

/// A regular polygon with a vertex at `vertex` (inscribed in the circle around `center`).
pub fn polygon_inscribed(center: Point, vertex: Point, sides: u32) -> Result<PolygonSpec, ConstructError> {
    finite(&[center, vertex])?;
    check_sides(sides)?;
    let v = vertex - center;
    let radius = v.length();
    if radius == 0.0 {
        return Err(ConstructError::Coincident);
    }
    Ok(PolygonSpec { center, radius, sides, rotation: angle(angle_of(v))? })
}

/// A regular polygon whose edge midpoint is `edge_mid` (circumscribed about the circle around `center`).
pub fn polygon_circumscribed(center: Point, edge_mid: Point, sides: u32) -> Result<PolygonSpec, ConstructError> {
    finite(&[center, edge_mid])?;
    check_sides(sides)?;
    let v = edge_mid - center;
    let apothem = v.length();
    if apothem == 0.0 {
        return Err(ConstructError::Coincident);
    }
    let half = PI / f64::from(sides);
    Ok(PolygonSpec { center, radius: apothem / math::cos(half), sides, rotation: angle(angle_of(v) - half)? })
}

/// A regular polygon with `p1 → p2` as one side, on the left of that direction; the first vertex is `p1`.
pub fn polygon_from_edge(p1: Point, p2: Point, sides: u32) -> Result<PolygonSpec, ConstructError> {
    finite(&[p1, p2])?;
    check_sides(sides)?;
    let e = p2 - p1;
    let len = e.length();
    if len == 0.0 {
        return Err(ConstructError::Coincident);
    }
    let half = PI / f64::from(sides);
    let left = Vector::new(-e.y / len, e.x / len);
    let center = p1.lerp(p2, 0.5) + left * (len / (2.0 * math::tan(half)));
    let radius = len / (2.0 * math::sin(half));
    Ok(PolygonSpec { center, radius, sides, rotation: angle(angle_of(p1 - center))? })
}

/// A straight slot: two semicircular ends of diameter `width` centred on `c1` and `c2`.
pub fn slot_center_to_center(c1: Point, c2: Point, width: f64) -> Result<Path, ConstructError> {
    finite(&[c1, c2])?;
    let h = positive(width)? / 2.0;
    let d = c2 - c1;
    let len = d.length();
    if len == 0.0 {
        return Err(ConstructError::Coincident);
    }
    let n = Vector::new(-d.y / len, d.x / len);
    let a_n = angle_of(n);
    let cap = |center: Point, start: f64| -> Result<EllipticalArc, ConstructError> {
        Ok(EllipticalArc::circle(center, h, angle(start)?, angle(PI)?)?)
    };
    PathBuilder::new()
        .move_to(c1 - n * h)
        .line_to(c2 - n * h)
        .arc(cap(c2, a_n - PI)?)
        .line_to(c1 + n * h)
        .arc(cap(c1, a_n)?)
        .close()
        .build()
        .map_err(Into::into)
}

/// A straight slot whose outline spans exactly `p1` to `p2` end to end.
pub fn slot_overall(p1: Point, p2: Point, width: f64) -> Result<Path, ConstructError> {
    finite(&[p1, p2])?;
    let w = positive(width)?;
    let d = p2 - p1;
    let len = d.length();
    if len == 0.0 {
        return Err(ConstructError::Coincident);
    }
    if len <= w {
        return Err(ConstructError::TooShort);
    }
    let u = d * (1.0 / len);
    slot_center_to_center(p1 + u * (w / 2.0), p2 - u * (w / 2.0), w)
}

/// A slot along a circular centreline (`radius` around `center`, from `start` through `sweep`,
/// either direction) with semicircular ends; `width` must be smaller than twice the radius.
pub fn slot_along_arc(
    center: Point,
    radius: f64,
    start: Angle,
    sweep: Angle,
    width: f64,
) -> Result<Path, ConstructError> {
    finite(&[center])?;
    let r = positive(radius)?;
    let h = positive(width)? / 2.0;
    if sweep.radians().abs() == 0.0 {
        return Err(ConstructError::Coincident);
    }
    if h >= r {
        return Err(ConstructError::RadiusTooLarge);
    }
    // Always walk counter-clockwise; a clockwise slot is the same outline.
    let (sa, sw) = if sweep.radians() < 0.0 {
        (start.radians() + sweep.radians(), -sweep.radians())
    } else {
        (start.radians(), sweep.radians())
    };
    let ea = sa + sw;
    let at = |radius: f64, a: f64| {
        let (s, c) = math::sin_cos(a);
        center + Vector::new(radius * c, radius * s)
    };
    let arc = |c: Point, rad: f64, from: f64, by: f64| -> Result<EllipticalArc, ConstructError> {
        Ok(EllipticalArc::circle(c, rad, angle(from)?, angle(by)?)?)
    };
    PathBuilder::new()
        .move_to(at(r + h, sa))
        .arc(arc(center, r + h, sa, sw)?)
        .arc(arc(at(r, ea), h, ea, PI)?)
        .arc(arc(center, r - h, ea, -sw)?)
        .arc(arc(at(r, sa), h, sa + PI, PI)?)
        .close()
        .build()
        .map_err(Into::into)
}
