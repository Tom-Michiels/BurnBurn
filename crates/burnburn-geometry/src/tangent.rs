//! Tangent Circle (S-draw, W2-L2): every circle tangent to three chosen points, lines and circles
//! (the problem of Apollonius), or to two of them with a given radius.
//!
//! A circle `(c, r)` is tangent to a point `P` when `|c − P|² = r²`, to a line `n·x = d` when
//! `n·c − d = ±r`, and to a circle `(C, ρ)` when `|c − C|² = (r ± ρ)²`. Subtracting the quadratic
//! equations pairwise leaves linear ones, so each of the (at most eight) sign combinations is a
//! line of solutions in `(x, y, r)` cut by one quadric. Everything is solved in a frame centred on
//! the targets and scaled to their size, so thresholds are relative.

use std::fmt;

use crate::{Circle, Line, Point, Segment, Vector};

/// Geometry a circle can be tangent to. Lines are infinite.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TangentTarget {
    Point(Point),
    Line(Line),
    Circle(Circle),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum TangentError {
    NotFinite,
    /// Two targets are the same point, line or circle: the answer would not be a finite set.
    Coincident,
    /// Infinitely many circles qualify (for example the radius of half the width of a strip).
    Degenerate,
    /// No circle is tangent to all targets.
    NoSolution,
    RadiusNotPositive,
}

impl fmt::Display for TangentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "no tangent circle: {self:?}")
    }
}

impl std::error::Error for TangentError {}

/// Relative size below which two quantities count as equal (coincident targets, parallel lines).
const REL_EPS: f64 = 1e-12;
/// Relative tangency error accepted for a solution (squaring loses about half the digits at a double root).
const TANGENT_EPS: f64 = 1e-6;

impl TangentTarget {
    /// The target for one straight or circular segment starting at `from`; Béziers and elliptical
    /// arcs have no tangent-circle form.
    pub fn from_segment(from: Point, segment: &Segment) -> Option<TangentTarget> {
        match segment {
            Segment::Line { to } => {
                (from.is_finite() && to.is_finite() && from != *to).then_some(TangentTarget::Line((from, *to)))
            }
            Segment::Arc(arc) => {
                let (u, v) = arc.semi_diameters();
                let (lu, lv) = (u.length(), v.length());
                let circular = lu > 0.0 && (lu - lv).abs() <= 1e-9 * lu && u.dot(v).abs() <= 1e-9 * lu * lv;
                circular.then(|| TangentTarget::Circle(Circle { center: arc.center(), radius: lu }))
            }
            _ => None,
        }
    }

    /// Where `circle`, which is tangent to this target, touches it.
    pub fn contact(&self, circle: &Circle) -> Point {
        match *self {
            TangentTarget::Point(p) => p,
            TangentTarget::Line((a, b)) => {
                let d = b - a;
                let len2 = d.dot(d);
                if len2 == 0.0 { a } else { a + d * ((circle.center - a).dot(d) / len2) }
            }
            TangentTarget::Circle(other) => {
                let between = other.center - circle.center;
                let dist = between.length();
                if dist == 0.0 {
                    return other.center;
                }
                let toward = between * (1.0 / dist);
                // Ours lies inside the other circle: the touch is on the far side, away from it.
                let inside_other = other.radius > circle.radius
                    && (dist - (other.radius - circle.radius)).abs() < (dist - (other.radius + circle.radius)).abs();
                if inside_other {
                    circle.center - toward * circle.radius
                } else {
                    circle.center + toward * circle.radius
                }
            }
        }
    }
}

/// A target in the solver's frame (origin at the targets, unit scale of their size).
#[derive(Clone, Copy)]
enum Norm {
    Pt(Vector),
    Ln { n: Vector, d: f64 },
    Ci { c: Vector, rho: f64 },
}

struct Frame {
    origin: Point,
    scale: f64,
}

impl Frame {
    fn of(targets: &[TangentTarget]) -> Result<Frame, TangentError> {
        let mut refs = Vec::new();
        let mut size = 0.0f64;
        for t in targets {
            match *t {
                TangentTarget::Point(p) => refs.push(p),
                TangentTarget::Line((a, b)) => refs.extend([a, b]),
                TangentTarget::Circle(c) => {
                    if !c.radius.is_finite() || c.radius <= 0.0 {
                        return Err(if c.radius.is_finite() {
                            TangentError::Coincident
                        } else {
                            TangentError::NotFinite
                        });
                    }
                    refs.push(c.center);
                    size = size.max(c.radius);
                }
            }
        }
        if !refs.iter().all(|p| p.is_finite()) {
            return Err(TangentError::NotFinite);
        }
        let n = refs.len() as f64;
        let origin = Point::new(refs.iter().map(|p| p.x).sum::<f64>() / n, refs.iter().map(|p| p.y).sum::<f64>() / n);
        let spread = refs.iter().map(|p| p.distance(origin)).fold(0.0, f64::max);
        let scale = (spread + size).max(f64::MIN_POSITIVE);
        Ok(Frame { origin, scale })
    }

    fn vector(&self, p: Point) -> Vector {
        (p - self.origin) * (1.0 / self.scale)
    }

    fn norm(&self, t: &TangentTarget) -> Result<Norm, TangentError> {
        Ok(match *t {
            TangentTarget::Point(p) => Norm::Pt(self.vector(p)),
            TangentTarget::Line((a, b)) => {
                let (a, b) = (self.vector(a), self.vector(b));
                let dir = b - a;
                let len = dir.length();
                if len <= REL_EPS {
                    return Err(TangentError::Coincident);
                }
                let n = Vector::new(-dir.y / len, dir.x / len);
                Norm::Ln { n, d: n.dot(a) }
            }
            TangentTarget::Circle(c) => Norm::Ci { c: self.vector(c.center), rho: c.radius / self.scale },
        })
    }
}

fn same(a: &Norm, b: &Norm) -> bool {
    match (*a, *b) {
        (Norm::Pt(p), Norm::Pt(q)) => (p - q).length() <= REL_EPS,
        (Norm::Ln { n: n1, d: d1 }, Norm::Ln { n: n2, d: d2 }) => {
            let s = if n1.dot(n2) < 0.0 { -1.0 } else { 1.0 };
            n1.cross(n2).abs() <= REL_EPS && (d1 - s * d2).abs() <= REL_EPS
        }
        (Norm::Ci { c: c1, rho: r1 }, Norm::Ci { c: c2, rho: r2 }) => {
            (c1 - c2).length() <= REL_EPS && (r1 - r2).abs() <= REL_EPS
        }
        _ => false,
    }
}

/// How far a circle `(c, r)` is from touching the target.
fn gap(t: &Norm, c: Vector, r: f64) -> f64 {
    match *t {
        Norm::Pt(p) => ((c - p).length() - r).abs(),
        Norm::Ln { n, d } => ((n.dot(c) - d).abs() - r).abs(),
        Norm::Ci { c: o, rho } => {
            let dist = (c - o).length();
            (dist - (r + rho)).abs().min((dist - (r - rho).abs()).abs())
        }
    }
}

/// The circle is the target circle itself (touching "everywhere"), which is no answer.
fn is_the_target(t: &Norm, c: Vector, r: f64) -> bool {
    matches!(*t, Norm::Ci { c: o, rho } if (c - o).length() <= 1e-9 && (r - rho).abs() <= 1e-9)
}

fn dedupe(mut found: Vec<(Vector, f64)>) -> Vec<(Vector, f64)> {
    found.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.x.total_cmp(&b.0.x)).then(a.0.y.total_cmp(&b.0.y)));
    let mut out: Vec<(Vector, f64)> = Vec::new();
    for f in found {
        if !out.iter().any(|o| (o.0 - f.0).length() <= 1e-9 && (o.1 - f.1).abs() <= 1e-9) {
            out.push(f);
        }
    }
    out
}

fn finish(frame: &Frame, found: Vec<(Vector, f64)>) -> Vec<Circle> {
    found.into_iter().map(|(c, r)| Circle { center: frame.origin + c * frame.scale, radius: r * frame.scale }).collect()
}

/// A quadratic condition `x² + y² − r² − 2 C·(x, y) + kr·r + c0 = 0`.
#[derive(Clone, Copy)]
struct Quad {
    c: Vector,
    kr: f64,
    c0: f64,
}

type Row = ([f64; 3], f64);

fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn solve_three(norms: &[Norm; 3]) -> Vec<(Vector, f64)> {
    let mut found = Vec::new();
    for mask in 0u8..8 {
        let mut lin: Vec<Row> = Vec::new();
        let mut quad: Vec<Quad> = Vec::new();
        let mut redundant = false;
        for (i, t) in norms.iter().enumerate() {
            let s = if mask & (1 << i) != 0 { -1.0 } else { 1.0 };
            match *t {
                Norm::Pt(p) => {
                    redundant |= s < 0.0; // a point has no sign: skip the mirrored combination
                    quad.push(Quad { c: p, kr: 0.0, c0: p.dot(p) });
                }
                Norm::Ln { n, d } => lin.push(([n.x, n.y, -s], d)),
                Norm::Ci { c, rho } => quad.push(Quad { c, kr: -2.0 * s * rho, c0: c.dot(c) - rho * rho }),
            }
        }
        if redundant {
            continue;
        }
        for q1 in quad.iter().skip(1) {
            let q0 = quad[0];
            lin.push(([-2.0 * (q1.c.x - q0.c.x), -2.0 * (q1.c.y - q0.c.y), q1.kr - q0.kr], -(q1.c0 - q0.c0)));
        }
        if quad.is_empty() {
            if let [(a1, b1), (a2, b2), (a3, b3)] = lin[..] {
                let det = dot3(a1, cross3(a2, a3));
                if det.abs() > 1e-12 {
                    let c23 = cross3(a2, a3);
                    let c31 = cross3(a3, a1);
                    let c12 = cross3(a1, a2);
                    let y = [
                        (b1 * c23[0] + b2 * c31[0] + b3 * c12[0]) / det,
                        (b1 * c23[1] + b2 * c31[1] + b3 * c12[1]) / det,
                        (b1 * c23[2] + b2 * c31[2] + b3 * c12[2]) / det,
                    ];
                    found.push((Vector::new(y[0], y[1]), y[2]));
                }
            }
            continue;
        }
        let [(a1, b1), (a2, b2)] = lin[..] else { continue };
        let v = cross3(a1, a2);
        let v2 = dot3(v, v);
        let (n1, n2) = (dot3(a1, a1).sqrt(), dot3(a2, a2).sqrt());
        if v2.sqrt() <= REL_EPS * n1 * n2 {
            continue;
        }
        let (c1, c2) = (cross3(a2, v), cross3(v, a1));
        let p0 = [0, 1, 2].map(|k| (b1 * c1[k] + b2 * c2[k]) / v2);
        let q = quad[0];
        let big_a = v[0] * v[0] + v[1] * v[1] - v[2] * v[2];
        let big_b =
            2.0 * (p0[0] * v[0] + p0[1] * v[1] - p0[2] * v[2]) - 2.0 * (q.c.x * v[0] + q.c.y * v[1]) + q.kr * v[2];
        let big_c =
            p0[0] * p0[0] + p0[1] * p0[1] - p0[2] * p0[2] - 2.0 * (q.c.x * p0[0] + q.c.y * p0[1]) + q.kr * p0[2] + q.c0;
        for t in quadratic_roots(big_a, big_b, big_c, v2) {
            let y = [0, 1, 2].map(|k| p0[k] + t * v[k]);
            found.push((Vector::new(y[0], y[1]), y[2]));
        }
    }
    found.retain(|(c, r)| c.is_finite() && r.is_finite() && *r > 1e-12);
    found.retain(|(c, r)| norms.iter().all(|t| gap(t, *c, *r) <= TANGENT_EPS && !is_the_target(t, *c, *r)));
    dedupe(found)
}

/// Real roots of `a t² + b t + c`; the linear case when `a` is negligible against the direction length.
fn quadratic_roots(a: f64, b: f64, c: f64, v2: f64) -> Vec<f64> {
    if a.abs() <= REL_EPS * v2.max(1.0) {
        return if b.abs() > REL_EPS * v2.sqrt() { vec![-c / b] } else { Vec::new() };
    }
    let mut disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        if -disc <= 1e-10 * (b * b + (4.0 * a * c).abs()) {
            disc = 0.0;
        } else {
            return Vec::new();
        }
    }
    let q = -0.5 * (b + if b < 0.0 { -disc.sqrt() } else { disc.sqrt() });
    let mut roots = vec![q / a];
    if q != 0.0 {
        roots.push(c / q);
    }
    roots
}

/// All circles tangent to the three targets, smallest radius first.
pub fn tangent_circles(targets: [TangentTarget; 3]) -> Result<Vec<Circle>, TangentError> {
    let frame = Frame::of(&targets)?;
    let norms = [frame.norm(&targets[0])?, frame.norm(&targets[1])?, frame.norm(&targets[2])?];
    if same(&norms[0], &norms[1]) || same(&norms[0], &norms[2]) || same(&norms[1], &norms[2]) {
        return Err(TangentError::Coincident);
    }
    let found = solve_three(&norms);
    if found.is_empty() {
        return Err(TangentError::NoSolution);
    }
    Ok(finish(&frame, found))
}

enum Locus {
    Line { n: Vector, d: f64 },
    Circle { c: Vector, r: f64 },
}

fn loci(t: &Norm, radius: f64) -> Vec<Locus> {
    match *t {
        Norm::Pt(p) => vec![Locus::Circle { c: p, r: radius }],
        Norm::Ln { n, d } => vec![Locus::Line { n, d: d + radius }, Locus::Line { n, d: d - radius }],
        Norm::Ci { c, rho } => {
            let mut v = vec![Locus::Circle { c, r: rho + radius }];
            if (rho - radius).abs() > 1e-9 {
                v.push(Locus::Circle { c, r: (rho - radius).abs() });
            }
            v
        }
    }
}

/// Intersections of two loci; `None` for an infinite overlap.
fn meet(a: &Locus, b: &Locus) -> Option<Vec<Vector>> {
    match (a, b) {
        (Locus::Line { n: n1, d: d1 }, Locus::Line { n: n2, d: d2 }) => {
            let det = n1.cross(*n2);
            if det.abs() <= REL_EPS {
                let s = if n1.dot(*n2) < 0.0 { -1.0 } else { 1.0 };
                return if (d1 - s * d2).abs() <= REL_EPS { None } else { Some(Vec::new()) };
            }
            Some(vec![Vector::new((d1 * n2.y - d2 * n1.y) / det, (n1.x * d2 - n2.x * d1) / det)])
        }
        (Locus::Line { n, d }, Locus::Circle { c, r }) | (Locus::Circle { c, r }, Locus::Line { n, d }) => {
            let h = n.dot(*c) - d;
            if h.abs() > r + REL_EPS {
                return Some(Vec::new());
            }
            let foot = *c - *n * h;
            let half = (r * r - h * h).max(0.0).sqrt();
            let along = Vector::new(-n.y, n.x);
            Some(vec![foot + along * half, foot - along * half])
        }
        (Locus::Circle { c: c1, r: r1 }, Locus::Circle { c: c2, r: r2 }) => {
            let between = *c2 - *c1;
            let d = between.length();
            if d <= REL_EPS {
                return if (r1 - r2).abs() <= REL_EPS { None } else { Some(Vec::new()) };
            }
            if d > r1 + r2 + REL_EPS || d < (r1 - r2).abs() - REL_EPS {
                return Some(Vec::new());
            }
            let along = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
            let half = (r1 * r1 - along * along).max(0.0).sqrt();
            let u = between * (1.0 / d);
            let mid = *c1 + u * along;
            let perp = Vector::new(-u.y, u.x);
            Some(vec![mid + perp * half, mid - perp * half])
        }
    }
}

/// All circles of `radius` tangent to both targets, smallest centre (x, then y) first.
pub fn tangent_circles_radius(targets: [TangentTarget; 2], radius: f64) -> Result<Vec<Circle>, TangentError> {
    if !radius.is_finite() {
        return Err(TangentError::NotFinite);
    }
    if radius <= 0.0 {
        return Err(TangentError::RadiusNotPositive);
    }
    let frame = Frame::of(&targets)?;
    let norms = [frame.norm(&targets[0])?, frame.norm(&targets[1])?];
    if same(&norms[0], &norms[1]) {
        return Err(TangentError::Coincident);
    }
    let r = radius / frame.scale;
    let mut found = Vec::new();
    for a in loci(&norms[0], r) {
        for b in loci(&norms[1], r) {
            match meet(&a, &b) {
                None => return Err(TangentError::Degenerate),
                Some(points) => found.extend(points.into_iter().map(|c| (c, r))),
            }
        }
    }
    found.retain(|(c, r)| {
        c.is_finite() && norms.iter().all(|t| gap(t, *c, *r) <= TANGENT_EPS && !is_the_target(t, *c, *r))
    });
    let mut found = dedupe(found);
    found.sort_by(|a, b| a.0.x.total_cmp(&b.0.x).then(a.0.y.total_cmp(&b.0.y)));
    if found.is_empty() {
        return Err(TangentError::NoSolution);
    }
    Ok(finish(&frame, found))
}

/// The circle whose centre is nearest to `near`.
pub fn nearest_circle(circles: &[Circle], near: Point) -> Option<Circle> {
    circles.iter().copied().min_by(|a, b| a.center.distance(near).total_cmp(&b.center.distance(near)))
}
