//! Warp 4 points and Deform 16 points (W2-L2): continuous, non-affine maps of a path.
//!
//! Own contract (the exact LightBurn mapping is unverified): the source is the bounding box of the
//! selection; Warp maps its four corners onto a convex quadrilateral (projectively, so straight
//! lines stay straight, or bilinearly), Deform maps it through a bicubic Bézier tensor patch whose
//! 4 × 4 control net starts out as the regular grid.
//!
//! A mapped path is rebuilt from lines and cubic Béziers: each segment is fitted by a Hermite cubic
//! through the mapped end points with the mapped end tangents, and split in halves until seven
//! interior samples of the piece lie within the tolerance of the exact image.

use std::fmt;

use burnburn_types::CancellationToken;

use crate::{Bounds, Path, Point, ProductionTolerance, Segment, Subpath, Vector};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WarpError {
    NotFinite,
    /// The source box has no area.
    DegenerateSource,
    /// The target corners enclose no area (collinear or coincident).
    DegenerateQuad,
    /// The target quadrilateral is not strictly convex.
    NotConvex,
    /// The control net folds the plane over itself (checked on a 17 × 17 grid of the patch).
    Folded,
    /// A control point index above 3.
    NoSuchControlPoint,
    /// The path leaves the source box the map is defined on.
    OutsideDomain,
    /// The error budget cannot be met (or the output would be unreasonably large).
    BudgetExceeded,
    Cancelled,
}

impl fmt::Display for WarpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot warp: {self:?}")
    }
}

impl std::error::Error for WarpError {}

/// A map of the plane defined on a source box.
pub trait PointMap {
    /// The box the map is defined on.
    fn domain(&self) -> Bounds;
    /// The image of `p`; `None` where the map is not finite.
    fn eval(&self, p: Point) -> Option<Point>;
}

/// How a quadrilateral warp interpolates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WarpKind {
    /// A homography: straight lines stay straight, perspective is possible.
    Projective,
    /// Bilinear interpolation of the corners: lines parallel to the box edges stay straight.
    Bilinear,
}

/// Source box corners → target quadrilateral `[bottom-left, bottom-right, top-right, top-left]`.
#[derive(Debug, Clone, PartialEq)]
pub struct QuadWarp {
    source: Bounds,
    target: [Point; 4],
    kind: WarpKind,
    /// Homography coefficients `(a, b, c, d, e, f, g, h)` of the unit square onto the target.
    homography: [f64; 8],
}

/// Relative size below which an area or a cross product counts as zero.
const REL_EPS: f64 = 1e-12;

fn source_is_valid(source: &Bounds) -> Result<(), WarpError> {
    let (min, max) = (source.min(), source.max());
    if !(min.is_finite() && max.is_finite()) {
        return Err(WarpError::NotFinite);
    }
    if !(source.width() > 0.0 && source.height() > 0.0) {
        return Err(WarpError::DegenerateSource);
    }
    Ok(())
}

fn to_unit(source: &Bounds, p: Point) -> (f64, f64) {
    ((p.x - source.min().x) / source.width(), (p.y - source.min().y) / source.height())
}

impl QuadWarp {
    pub fn new(source: Bounds, target: [Point; 4], kind: WarpKind) -> Result<Self, WarpError> {
        if !target.iter().all(|p| p.is_finite()) {
            return Err(WarpError::NotFinite);
        }
        source_is_valid(&source)?;
        let edges: [Vector; 4] = std::array::from_fn(|k| target[(k + 1) % 4] - target[k]);
        let scale = edges.iter().map(|e| e.dot(*e)).fold(0.0, f64::max);
        let eps = REL_EPS * scale;
        let turns: [f64; 4] = std::array::from_fn(|k| edges[k].cross(edges[(k + 1) % 4]));
        if turns.iter().any(|c| *c > eps) && turns.iter().any(|c| *c < -eps) {
            return Err(WarpError::NotConvex);
        }
        if turns.iter().all(|c| c.abs() <= eps) {
            return Err(WarpError::DegenerateQuad);
        }
        if turns.iter().any(|c| c.abs() <= eps) {
            return Err(WarpError::NotConvex);
        }
        Ok(QuadWarp { source, target, kind, homography: square_to_quad(&target) })
    }
}

/// Heckbert's projective map of the unit square onto a quadrilateral.
fn square_to_quad(q: &[Point; 4]) -> [f64; 8] {
    let (sx, sy) = (q[0].x - q[1].x + q[2].x - q[3].x, q[0].y - q[1].y + q[2].y - q[3].y);
    let (g, h);
    if sx.abs() <= REL_EPS && sy.abs() <= REL_EPS {
        (g, h) = (0.0, 0.0);
    } else {
        let (dx1, dx2, dy1, dy2) = (q[1].x - q[2].x, q[3].x - q[2].x, q[1].y - q[2].y, q[3].y - q[2].y);
        let det = dx1 * dy2 - dx2 * dy1;
        g = (sx * dy2 - dx2 * sy) / det;
        h = (dx1 * sy - sx * dy1) / det;
    }
    [
        q[1].x - q[0].x + g * q[1].x,
        q[3].x - q[0].x + h * q[3].x,
        q[0].x,
        q[1].y - q[0].y + g * q[1].y,
        q[3].y - q[0].y + h * q[3].y,
        q[0].y,
        g,
        h,
    ]
}

impl PointMap for QuadWarp {
    fn domain(&self) -> Bounds {
        self.source
    }

    fn eval(&self, p: Point) -> Option<Point> {
        let (u, v) = to_unit(&self.source, p);
        let image = match self.kind {
            WarpKind::Bilinear => {
                let q = &self.target;
                let (a, b, c, d) = ((1.0 - u) * (1.0 - v), u * (1.0 - v), u * v, (1.0 - u) * v);
                Point::new(
                    a * q[0].x + b * q[1].x + c * q[2].x + d * q[3].x,
                    a * q[0].y + b * q[1].y + c * q[2].y + d * q[3].y,
                )
            }
            WarpKind::Projective => {
                let [a, b, c, d, e, f, g, h] = self.homography;
                let w = g * u + h * v + 1.0;
                if !(w > 0.0) {
                    return None;
                }
                Point::new((a * u + b * v + c) / w, (d * u + e * v + f) / w)
            }
        };
        image.is_finite().then_some(image)
    }
}

/// A 4 × 4 control net over a source box; `net[row][column]`, row 0 at the bottom, column 0 on the left.
#[derive(Debug, Clone, PartialEq)]
pub struct DeformGrid {
    source: Bounds,
    net: [[Point; 4]; 4],
}

fn bernstein(t: f64) -> [f64; 4] {
    let s = 1.0 - t;
    [s * s * s, 3.0 * t * s * s, 3.0 * t * t * s, t * t * t]
}

fn bernstein_derivative(t: f64) -> [f64; 4] {
    let s = 1.0 - t;
    [-3.0 * s * s, 3.0 * s * s - 6.0 * t * s, 6.0 * t * s - 3.0 * t * t, 3.0 * t * t]
}

impl DeformGrid {
    /// The regular grid: maps every point to itself.
    pub fn identity(source: Bounds) -> Self {
        let mut net = [[Point::ORIGIN; 4]; 4];
        for (j, row) in net.iter_mut().enumerate() {
            for (i, c) in row.iter_mut().enumerate() {
                *c = Point::new(
                    source.min().x + source.width() * i as f64 / 3.0,
                    source.min().y + source.height() * j as f64 / 3.0,
                );
            }
        }
        DeformGrid { source, net }
    }

    pub fn new(source: Bounds, net: [[Point; 4]; 4]) -> Result<Self, WarpError> {
        if !net.iter().flatten().all(|p| p.is_finite()) {
            return Err(WarpError::NotFinite);
        }
        source_is_valid(&source)?;
        let grid = DeformGrid { source, net };
        grid.check_not_folded()?;
        Ok(grid)
    }

    pub fn net(&self) -> &[[Point; 4]; 4] {
        &self.net
    }

    /// The same grid with one control point moved.
    pub fn with_point(&self, row: usize, column: usize, to: Point) -> Result<Self, WarpError> {
        if row > 3 || column > 3 {
            return Err(WarpError::NoSuchControlPoint);
        }
        let mut net = self.net;
        net[row][column] = to;
        DeformGrid::new(self.source, net)
    }

    /// The patch's Jacobian determinant must keep one sign on a 17 × 17 grid of the patch.
    fn check_not_folded(&self) -> Result<(), WarpError> {
        let floor = 1e-9 * self.source.width() * self.source.height();
        let mut sign = 0.0f64;
        for a in 0..=16 {
            for b in 0..=16 {
                let (u, v) = (f64::from(a) / 16.0, f64::from(b) / 16.0);
                let (bu, bv, du, dv) = (bernstein(u), bernstein(v), bernstein_derivative(u), bernstein_derivative(v));
                let (mut su, mut sv) = (Vector::ZERO, Vector::ZERO);
                for j in 0..4 {
                    for i in 0..4 {
                        let c = self.net[j][i].to_vector();
                        su = su + c * (du[i] * bv[j]);
                        sv = sv + c * (bu[i] * dv[j]);
                    }
                }
                let det = su.cross(sv);
                if !det.is_finite() || det.abs() <= floor {
                    return Err(WarpError::Folded);
                }
                if sign == 0.0 {
                    sign = det.signum();
                } else if det.signum() != sign {
                    return Err(WarpError::Folded);
                }
            }
        }
        Ok(())
    }
}

impl PointMap for DeformGrid {
    fn domain(&self) -> Bounds {
        self.source
    }

    fn eval(&self, p: Point) -> Option<Point> {
        let (u, v) = to_unit(&self.source, p);
        let (bu, bv) = (bernstein(u), bernstein(v));
        let mut s = Vector::ZERO;
        for (row, wv) in self.net.iter().zip(bv) {
            for (c, wu) in row.iter().zip(bu) {
                s = s + c.to_vector() * (wu * wv);
            }
        }
        let image = Point::new(s.x, s.y);
        image.is_finite().then_some(image)
    }
}

/// Depth of the halving per segment: 2^14 pieces at most.
const MAX_DEPTH: u32 = 14;
/// Pieces for the whole path; bounds the output for absurd budgets.
const MAX_PIECES: usize = 200_000;
/// Interior samples per piece (at k/8 of its parameter range).
const SAMPLES: usize = 7;

struct Fitter<'a, M: PointMap> {
    map: &'a M,
    tolerance: f64,
    step: f64,
    cancel: &'a CancellationToken,
    pieces: usize,
}

impl<M: PointMap> Fitter<'_, M> {
    fn image(&self, p: Point) -> Result<Point, WarpError> {
        self.map.eval(p).ok_or(WarpError::OutsideDomain)
    }

    /// The image of the segment's tangent at `t`, by central differences of the map.
    fn image_tangent(&self, from: Point, seg: &Segment, t: f64) -> Result<Vector, WarpError> {
        let q = seg.point_at(from, t);
        let d = seg.derivative(from, t);
        let h = self.step;
        let dx = self.image(q + Vector::new(h, 0.0))? - self.image(q - Vector::new(h, 0.0))?;
        let dy = self.image(q + Vector::new(0.0, h))? - self.image(q - Vector::new(0.0, h))?;
        Ok((dx * d.x + dy * d.y) * (1.0 / (2.0 * h)))
    }

    fn point_at(&self, from: Point, seg: &Segment, t: f64) -> Result<Point, WarpError> {
        let q = if t <= 0.0 {
            from
        } else if t >= 1.0 {
            seg.end()
        } else {
            seg.point_at(from, t)
        };
        self.image(q)
    }

    fn fit(
        &mut self,
        from: Point,
        seg: &Segment,
        range: (f64, f64, Point, Point),
        depth: u32,
        out: &mut Vec<Segment>,
    ) -> Result<(), WarpError> {
        self.cancel.check().map_err(|_| WarpError::Cancelled)?;
        let (ta, tb, p0, p3) = range;
        let mut samples = [(0.0, Point::ORIGIN); SAMPLES];
        for (k, s) in samples.iter_mut().enumerate() {
            let f = (k + 1) as f64 / (SAMPLES + 1) as f64;
            *s = (f, self.point_at(from, seg, ta + f * (tb - ta))?);
        }
        if matches!(seg, Segment::Line { .. }) {
            let chord = p3 - p0;
            let len2 = chord.dot(chord);
            let flat = samples.iter().all(|(_, q)| {
                let t = if len2 > 0.0 { ((*q - p0).dot(chord) / len2).clamp(0.0, 1.0) } else { 0.0 };
                q.distance(p0 + chord * t) <= 0.5 * self.tolerance
            });
            if flat {
                self.piece(out)?;
                out.push(Segment::Line { to: p3 });
                return Ok(());
            }
        }
        let span = tb - ta;
        let c1 = p0 + self.image_tangent(from, seg, ta)? * (span / 3.0);
        let c2 = p3 - self.image_tangent(from, seg, tb)? * (span / 3.0);
        let error = samples
            .iter()
            .map(|(f, q)| {
                let b = bernstein(*f);
                let fit = p0.to_vector() * b[0] + c1.to_vector() * b[1] + c2.to_vector() * b[2] + p3.to_vector() * b[3];
                q.distance(Point::new(fit.x, fit.y))
            })
            .fold(0.0, f64::max);
        if error <= self.tolerance {
            self.piece(out)?;
            out.push(Segment::Cubic { c1, c2, to: p3 });
            return Ok(());
        }
        if depth >= MAX_DEPTH {
            return Err(WarpError::BudgetExceeded);
        }
        let mid = 0.5 * (ta + tb);
        let pm = self.point_at(from, seg, mid)?;
        self.fit(from, seg, (ta, mid, p0, pm), depth + 1, out)?;
        self.fit(from, seg, (mid, tb, pm, p3), depth + 1, out)
    }

    fn piece(&mut self, out: &[Segment]) -> Result<(), WarpError> {
        self.pieces = self.pieces.max(out.len() + 1);
        if self.pieces > MAX_PIECES { Err(WarpError::BudgetExceeded) } else { Ok(()) }
    }

    fn segment(&mut self, from: Point, seg: &Segment, out: &mut Vec<Segment>) -> Result<(), WarpError> {
        let (p0, p3) = (self.image(from)?, self.image(seg.end())?);
        self.fit(from, seg, (0.0, 1.0, p0, p3), 0, out)
    }
}

/// The image of `path` under `map`, as lines and cubic Béziers that stay within `tolerance` of the
/// exact image (checked at several points per piece). Closed subpaths stay closed.
pub fn warp_path(
    path: &Path,
    map: &impl PointMap,
    tolerance: &ProductionTolerance,
    cancel: &CancellationToken,
) -> Result<Path, WarpError> {
    let Some(bounds) = path.bounds() else { return Ok(Path::default()) };
    let domain = map.domain();
    let slack = 1e-9 * domain.width().max(domain.height()).max(1.0);
    let allowed = domain.expanded(slack);
    if !(allowed.contains(bounds.min()) && allowed.contains(bounds.max())) {
        return Err(WarpError::OutsideDomain);
    }
    let mut fitter =
        Fitter { map, tolerance: tolerance.mm(), step: 1e-6 * domain.width().max(domain.height()), cancel, pieces: 0 };
    let mut subpaths = Vec::with_capacity(path.subpaths().len());
    for sp in path.subpaths() {
        let mut out = Vec::with_capacity(sp.segments().len());
        let mut current = sp.start();
        for seg in sp.segments() {
            fitter.segment(current, seg, &mut out)?;
            current = seg.end();
        }
        let start_image = fitter.image(sp.start())?;
        if sp.is_closed() && current != sp.start() {
            // The closing line is part of the outline and bends like any other edge.
            let closing = Segment::Line { to: sp.start() };
            let mut tail = Vec::new();
            fitter.segment(current, &closing, &mut tail)?;
            if !matches!(tail[..], [Segment::Line { to }] if to == start_image) {
                out.extend(tail);
            }
        }
        subpaths.push(Subpath::new(start_image, out, sp.is_closed()).map_err(|_| WarpError::NotFinite)?);
    }
    Ok(Path::from_subpaths(subpaths))
}
