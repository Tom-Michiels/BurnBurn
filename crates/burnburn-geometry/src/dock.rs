//! Dock (E04): slide objects along an axis until their geometry touches a reference.
//!
//! The contact is the first moment a vertex of one flattened outline meets an edge of the other,
//! found by casting axis-parallel rays; no polygon is built. Outlines that already overlap are
//! refused instead of guessed at.

use std::fmt;

use crate::{Bounds, FlatteningTolerance, Path, Point, Vector};

/// The way objects travel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DockDirection {
    Left,
    Right,
    Up,
    Down,
}

/// What counts as the boundary of the objects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DockContact {
    /// Bounding boxes touch.
    Bounds,
    /// The (flattened) outlines touch: a concave reference can be entered.
    Geometry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DockError {
    NotFinite,
    /// The clearance must not be negative.
    InvalidGap,
    /// The moving objects or the reference have no geometry.
    EmptyGeometry,
    /// Nothing in the reference lies ahead of the moving objects in this direction.
    NoContact,
    /// The moving objects already overlap the reference; docking would be ambiguous.
    Overlapping,
}

impl fmt::Display for DockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot dock: {self:?}")
    }
}

impl std::error::Error for DockError {}

/// Distances below this count as touching (float noise of coordinates up to ~1e5 mm).
const TOUCH_EPS_MM: f64 = 1e-9;

/// The translation that docks `moving` (one rigid body) to `reference` (their union). `gap` is the
/// clearance that remains along the dock direction; an object closer than `gap` moves back.
pub fn dock_offset(
    moving: &[&Path],
    reference: &[&Path],
    direction: DockDirection,
    contact: DockContact,
    gap: f64,
    tolerance: &impl FlatteningTolerance,
) -> Result<Vector, DockError> {
    if !gap.is_finite() {
        return Err(DockError::NotFinite);
    }
    if gap < 0.0 {
        return Err(DockError::InvalidGap);
    }
    let (m_bounds, r_bounds) = (union_bounds(moving), union_bounds(reference));
    let (Some(mb), Some(rb)) = (m_bounds, r_bounds) else { return Err(DockError::EmptyGeometry) };
    let travel = match contact {
        DockContact::Bounds => bounds_travel(&mb, &rb, direction)?,
        DockContact::Geometry => geometry_travel(moving, reference, direction, tolerance)?,
    };
    let along = travel - gap;
    Ok(match direction {
        DockDirection::Right => Vector::new(along, 0.0),
        DockDirection::Left => Vector::new(-along, 0.0),
        DockDirection::Up => Vector::new(0.0, along),
        DockDirection::Down => Vector::new(0.0, -along),
    })
}

fn union_bounds(paths: &[&Path]) -> Option<Bounds> {
    paths.iter().filter_map(|p| p.bounds()).reduce(Bounds::union)
}

/// The same geometry seen with the dock direction as +x.
fn to_frame(p: Point, direction: DockDirection) -> Point {
    match direction {
        DockDirection::Right => p,
        DockDirection::Left => Point::new(-p.x, p.y),
        DockDirection::Up => Point::new(p.y, p.x),
        DockDirection::Down => Point::new(-p.y, p.x),
    }
}

fn bounds_travel(mb: &Bounds, rb: &Bounds, direction: DockDirection) -> Result<f64, DockError> {
    let corners = [to_frame(mb.min(), direction), to_frame(mb.max(), direction)];
    let m = Bounds::from_corners(corners[0], corners[1]);
    let corners = [to_frame(rb.min(), direction), to_frame(rb.max(), direction)];
    let r = Bounds::from_corners(corners[0], corners[1]);
    // The corridor swept by the moving box must meet the reference.
    if m.max().y < r.min().y - TOUCH_EPS_MM || m.min().y > r.max().y + TOUCH_EPS_MM {
        return Err(DockError::NoContact);
    }
    let travel = r.min().x - m.max().x;
    if travel < -TOUCH_EPS_MM {
        // Behind the moving box entirely, or overlapping it.
        return if r.max().x < m.min().x + TOUCH_EPS_MM {
            Err(DockError::NoContact)
        } else {
            Err(DockError::Overlapping)
        };
    }
    Ok(travel.max(0.0))
}

/// A flattened contour in the frame of the dock direction.
struct Ring {
    points: Vec<Point>,
    closed: bool,
}

impl Ring {
    fn edges(&self) -> impl Iterator<Item = (Point, Point)> + '_ {
        let n = self.points.len();
        let count = if self.closed { n } else { n.saturating_sub(1) };
        (0..count).map(move |i| (self.points[i], self.points[(i + 1) % n]))
    }
}

fn rings(paths: &[&Path], direction: DockDirection, tolerance: &impl FlatteningTolerance) -> Vec<Ring> {
    paths
        .iter()
        .flat_map(|p| p.flatten(tolerance))
        .map(|pl| Ring { points: pl.points.iter().map(|&q| to_frame(q, direction)).collect(), closed: pl.closed })
        .filter(|r| !r.points.is_empty())
        .collect()
}

fn geometry_travel(
    moving: &[&Path],
    reference: &[&Path],
    direction: DockDirection,
    tolerance: &impl FlatteningTolerance,
) -> Result<f64, DockError> {
    let (m, r) = (rings(moving, direction, tolerance), rings(reference, direction, tolerance));
    if m.is_empty() || r.is_empty() {
        return Err(DockError::EmptyGeometry);
    }
    if overlaps(&m, &r) {
        return Err(DockError::Overlapping);
    }
    let mut best: Option<f64> = None;
    let mut consider = |t: f64| best = Some(best.map_or(t, |b: f64| b.min(t)));
    // Vertices of the moving body hit edges of the reference ahead of them…
    for v in m.iter().flat_map(|ring| ring.points.iter()) {
        for (a, b) in r.iter().flat_map(Ring::edges) {
            if let Some(t) = ray_hit(*v, a, b) {
                consider(t);
            }
        }
    }
    // …and vertices of the reference hit the moving body when it moves forward (a ray backwards).
    for v in r.iter().flat_map(|ring| ring.points.iter()) {
        let mirrored = Point::new(-v.x, v.y);
        for (a, b) in m.iter().flat_map(Ring::edges) {
            if let Some(t) = ray_hit(mirrored, Point::new(-a.x, a.y), Point::new(-b.x, b.y)) {
                consider(t);
            }
        }
    }
    best.ok_or(DockError::NoContact)
}

/// Distance along +x from `o` to segment `a`–`b`, when it lies ahead (within touching noise).
fn ray_hit(o: Point, a: Point, b: Point) -> Option<f64> {
    let dy = b.y - a.y;
    if dy.abs() <= f64::EPSILON * a.y.abs().max(b.y.abs()).max(1.0) {
        if (a.y - o.y).abs() > TOUCH_EPS_MM {
            return None;
        }
        // The ray runs along a horizontal edge: it meets the nearer end ahead.
        return [a.x - o.x, b.x - o.x].into_iter().filter(|t| *t >= -TOUCH_EPS_MM).reduce(f64::min).map(|t| t.max(0.0));
    }
    let s = (o.y - a.y) / dy;
    if !(-1e-12..=1.0 + 1e-12).contains(&s) {
        return None;
    }
    let t = a.x + s * (b.x - a.x) - o.x;
    (t >= -TOUCH_EPS_MM).then_some(t.max(0.0))
}

/// True when the outlines cross properly or one lies strictly inside the other.
fn overlaps(m: &[Ring], r: &[Ring]) -> bool {
    for (a, b) in m.iter().flat_map(Ring::edges) {
        for (c, d) in r.iter().flat_map(Ring::edges) {
            if properly_cross(a, b, c, d) {
                return true;
            }
        }
    }
    let inside = |p: Point, rings: &[Ring]| {
        let mut odd = false;
        for ring in rings.iter().filter(|g| g.closed) {
            if boundary_distance(p, rings) <= TOUCH_EPS_MM {
                return false;
            }
            odd ^= ring_contains(ring, p);
        }
        odd
    };
    m.iter().flat_map(|g| g.points.iter()).any(|&p| inside(p, r))
        || r.iter().flat_map(|g| g.points.iter()).any(|&p| inside(p, m))
}

fn properly_cross(a: Point, b: Point, c: Point, d: Point) -> bool {
    let side = |p: Point, q: Point, s: Point| (q - p).cross(s - p);
    let scale = (b - a).length().max((d - c).length());
    let eps = TOUCH_EPS_MM * scale;
    let (d1, d2, d3, d4) = (side(a, b, c), side(a, b, d), side(c, d, a), side(c, d, b));
    (d1 > eps && d2 < -eps || d1 < -eps && d2 > eps) && (d3 > eps && d4 < -eps || d3 < -eps && d4 > eps)
}

fn ring_contains(ring: &Ring, p: Point) -> bool {
    let mut inside = false;
    for (a, b) in ring.edges() {
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            inside = !inside;
        }
    }
    inside
}

fn boundary_distance(p: Point, rings: &[Ring]) -> f64 {
    rings
        .iter()
        .flat_map(Ring::edges)
        .map(|(a, b)| {
            let ab = b - a;
            let len2 = ab.dot(ab);
            let t = if len2 > 0.0 { ((p - a).dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
            p.distance(a + ab * t)
        })
        .fold(f64::INFINITY, f64::min)
}
