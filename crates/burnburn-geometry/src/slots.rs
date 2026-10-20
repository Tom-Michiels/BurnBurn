//! Resize Slots (W2-L2): change the width of box-joint slots and tabs without scaling the shape.
//!
//! A slot is a rectangular notch in a straight-edged outline, a tab its mirror image, and a
//! rectangular hole counts as a slot too. Only the two walls of a matching feature move.

use std::fmt;

use burnburn_types::Length;

use crate::{Path, Point, ProductionTolerance, Segment, Subpath, Vector};

/// Which features are resized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlotSelect {
    /// Notches cut into the material and rectangular holes.
    Slots,
    /// Protrusions of the material.
    Tabs,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResizeSlots {
    /// The width of the features to find (mm).
    pub from_width: f64,
    /// The width they get (mm).
    pub to_width: f64,
    /// A feature matches when its width differs from `from_width` by at most this (mm).
    pub match_tolerance: f64,
    pub select: SlotSelect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SlotsError {
    /// A width or tolerance is not finite, not positive (tolerance: negative).
    InvalidParameter,
    /// No feature of the selected kind has the width to match.
    NoSlotsFound,
    /// The new width makes a neighbouring edge collapse or reverse.
    Collision,
}

impl fmt::Display for SlotsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot resize slots: {self:?}")
    }
}

impl std::error::Error for SlotsError {}

#[derive(Debug, Clone, PartialEq)]
pub struct SlotsOutcome {
    pub path: Path,
    /// Features that were resized.
    pub resized: usize,
    /// Matching features left alone because they share a wall with one that was resized.
    pub skipped: usize,
}

/// Largest sine of the angle between edges that still counts as perpendicular or antiparallel.
const ANGLE_EPS: f64 = 1e-6;
/// An adjacent edge must be at least this far (sine) from the wall direction to be a proper corner.
const CORNER_MIN: f64 = 1e-3;

/// A straight-edged closed contour as its vertices (the closing vertex is not repeated).
struct Ring {
    vertices: Vec<Point>,
    /// The subpath repeated its start explicitly as the last segment.
    explicit_close: bool,
}

fn ring_of(sp: &Subpath) -> Option<Ring> {
    if !sp.is_closed() || !sp.segments().iter().all(|s| matches!(s, Segment::Line { .. })) {
        return None;
    }
    let mut vertices = vec![sp.start()];
    vertices.extend(sp.segments().iter().map(Segment::end));
    let explicit_close = vertices.len() > 1 && vertices.last() == vertices.first();
    if explicit_close {
        vertices.pop();
    }
    (vertices.len() >= 4).then_some(Ring { vertices, explicit_close })
}

fn shoelace(v: &[Point]) -> f64 {
    (0..v.len()).map(|i| v[i].to_vector().cross(v[(i + 1) % v.len()].to_vector())).sum::<f64>() / 2.0
}

fn contains(polygon: &[Point], q: Point) -> bool {
    let mut inside = false;
    for i in 0..polygon.len() {
        let (a, b) = (polygon[i], polygon[(i + 1) % polygon.len()]);
        if (a.y > q.y) != (b.y > q.y) && q.x < a.x + (q.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            inside = !inside;
        }
    }
    inside
}

/// Where the lines `a + s·da` and `b + t·db` meet.
fn meet(a: Point, da: Vector, b: Point, db: Vector) -> Option<Point> {
    let det = da.cross(db);
    if det.abs() <= CORNER_MIN * da.length() * db.length() {
        return None;
    }
    let s = (b - a).cross(db) / det;
    Some(a + da * s)
}

fn unit(v: Vector) -> Vector {
    v * (1.0 / v.length())
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Slot,
    Tab,
}

/// A rectangular feature: indices of the base edge `i` (B→C); the walls are edges `i-1` and `i+1`.
struct Feature {
    base: usize,
    kind: Kind,
}

fn features(ring: &Ring, material_left: bool, spec: &ResizeSlots) -> Vec<Feature> {
    let n = ring.vertices.len();
    if n == 4 {
        return Vec::new();
    }
    let at = |i: usize, k: isize| ring.vertices[(i as isize + k).rem_euclid(n as isize) as usize];
    let mut found = Vec::new();
    for i in 0..n {
        let (a, b, c, d) = (at(i, -1), at(i, 0), at(i, 1), at(i, 2));
        let base = c - b;
        let width = base.length();
        if (width - spec.from_width).abs() > spec.match_tolerance {
            continue;
        }
        let (w1, w2) = (b - a, d - c);
        if w1.length() == 0.0 || w2.length() == 0.0 {
            continue;
        }
        let (e, u1, u2) = (unit(base), unit(w1), unit(w2));
        let right_angles = e.dot(u1).abs() <= ANGLE_EPS && e.dot(u2).abs() <= ANGLE_EPS;
        if !right_angles || u1.dot(u2) >= 0.0 || u1.cross(u2).abs() > ANGLE_EPS {
            continue;
        }
        let (prev, next) = (a - at(i, -2), at(i, 3) - d);
        if prev.length() == 0.0 || next.length() == 0.0 {
            continue;
        }
        if unit(prev).cross(u1).abs() < CORNER_MIN || unit(next).cross(u2).abs() < CORNER_MIN {
            continue;
        }
        let convex_for_left = w1.cross(base) > 0.0;
        let kind = if convex_for_left == material_left { Kind::Tab } else { Kind::Slot };
        found.push(Feature { base: i, kind });
    }
    found
}

fn wanted(select: SlotSelect, kind: Kind) -> bool {
    matches!((select, kind), (SlotSelect::Both, _) | (SlotSelect::Slots, Kind::Slot) | (SlotSelect::Tabs, Kind::Tab))
}

/// Resizes a rectangular hole about its centre; `None` when it is not a matching rectangle.
fn resize_rectangle(ring: &Ring, spec: &ResizeSlots) -> Option<Vec<Point>> {
    let v = &ring.vertices;
    let (e0, e1) = (v[1] - v[0], v[2] - v[1]);
    let (a, b) = (e0.length(), e1.length());
    if a == 0.0 || b == 0.0 || e0.dot(e1).abs() > ANGLE_EPS * a * b {
        return None;
    }
    let (e2, e3) = (v[3] - v[2], v[0] - v[3]);
    if (e0 + e2).length() > 1e-9 * (a + b) || (e1 + e3).length() > 1e-9 * (a + b) {
        return None;
    }
    let matches = |len: f64| (len - spec.from_width).abs() <= spec.match_tolerance;
    let (na, nb) = (if matches(a) { spec.to_width } else { a }, if matches(b) { spec.to_width } else { b });
    if na == a && nb == b {
        return None;
    }
    let centre = Point::new(v.iter().map(|p| p.x).sum::<f64>() / 4.0, v.iter().map(|p| p.y).sum::<f64>() / 4.0);
    let (u0, u1) = (unit(e0), unit(e1));
    Some(
        v.iter()
            .map(|p| {
                let r = *p - centre;
                let (x, y) = (r.dot(u0).signum(), r.dot(u1).signum());
                centre + u0 * (x * na / 2.0) + u1 * (y * nb / 2.0)
            })
            .collect(),
    )
}

impl Path {
    /// This path with the matching slots and tabs set to `spec.to_width`.
    ///
    /// Only straight-edged closed contours are searched. Collisions with geometry other than the
    /// neighbouring edges of a feature are not detected.
    pub fn resize_slots(&self, spec: &ResizeSlots) -> Result<SlotsOutcome, SlotsError> {
        let positive = |v: f64| v.is_finite() && v > 0.0;
        if !positive(spec.from_width)
            || !positive(spec.to_width)
            || !spec.match_tolerance.is_finite()
            || spec.match_tolerance < 0.0
        {
            return Err(SlotsError::InvalidParameter);
        }
        let flat = self.flatten(
            &ProductionTolerance::new(Length::from_mm(0.01).map_err(|_| SlotsError::InvalidParameter)?)
                .map_err(|_| SlotsError::InvalidParameter)?,
        );
        let mut subpaths = Vec::with_capacity(self.subpaths().len());
        let (mut resized, mut skipped) = (0, 0);
        for (index, sp) in self.subpaths().iter().enumerate() {
            let Some(mut ring) = ring_of(sp) else {
                subpaths.push(sp.clone());
                continue;
            };
            let probe = ring.vertices[0];
            let depth = flat
                .iter()
                .enumerate()
                .filter(|(j, pl)| *j != index && pl.closed && contains(&pl.points, probe))
                .count();
            let hole = depth % 2 == 1;
            let material_left = (shoelace(&ring.vertices) > 0.0) != hole;
            let n = ring.vertices.len();
            if n == 4 {
                if hole
                    && spec.select != SlotSelect::Tabs
                    && let Some(moved) = resize_rectangle(&ring, spec)
                {
                    ring.vertices = moved;
                    resized += 1;
                }
            } else {
                let mut found = features(&ring, material_left, spec);
                found.retain(|f| wanted(spec.select, f.kind));
                // Slots first: a wall shared with a tab goes to the slot.
                found.sort_by_key(|f| (f.kind == Kind::Tab, f.base));
                let mut claimed = vec![false; n];
                let mut moves: Vec<(usize, Point)> = Vec::new();
                for f in &found {
                    let (wall1, wall2) = ((f.base + n - 1) % n, (f.base + 1) % n);
                    if claimed[wall1] || claimed[wall2] {
                        skipped += 1;
                        continue;
                    }
                    claimed[wall1] = true;
                    claimed[wall2] = true;
                    let at = |k: isize| ring.vertices[(f.base as isize + k).rem_euclid(n as isize) as usize];
                    let (a, b, c, d) = (at(-1), at(0), at(1), at(2));
                    let e = unit(c - b);
                    let delta = (spec.to_width - (c - b).length()) / 2.0;
                    let (nb, nc) = (b - e * delta, c + e * delta);
                    let (u1, u2) = (unit(b - a), unit(d - c));
                    let prev = unit(a - at(-2));
                    let next = unit(at(3) - d);
                    let (Some(na), Some(nd)) = (meet(a, prev, nb, u1), meet(d, next, nc, u2)) else { continue };
                    let ix = |k: isize| (f.base as isize + k).rem_euclid(n as isize) as usize;
                    moves.extend([(ix(-1), na), (ix(0), nb), (ix(1), nc), (ix(2), nd)]);
                    resized += 1;
                }
                let mut moved = ring.vertices.clone();
                for (i, p) in moves {
                    moved[i] = p;
                }
                for k in 0..n {
                    let old = ring.vertices[(k + 1) % n] - ring.vertices[k];
                    let new = moved[(k + 1) % n] - moved[k];
                    if !new.is_finite() || new.dot(old) <= 1e-9 * old.dot(old) {
                        return Err(SlotsError::Collision);
                    }
                }
                ring.vertices = moved;
            }
            let mut segments: Vec<Segment> = ring.vertices[1..].iter().map(|&to| Segment::Line { to }).collect();
            if ring.explicit_close {
                segments.push(Segment::Line { to: ring.vertices[0] });
            }
            subpaths.push(Subpath::new(ring.vertices[0], segments, true).map_err(|_| SlotsError::InvalidParameter)?);
        }
        if resized == 0 {
            return Err(SlotsError::NoSlotsFound);
        }
        Ok(SlotsOutcome { path: Path::from_subpaths(subpaths), resized, skipped })
    }
}
