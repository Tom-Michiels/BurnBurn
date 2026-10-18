//! Named snap targets (E04). A snap is a temporary placement aid; it never creates a constraint.

use crate::intersect::segment_intersections;
use crate::query::subpath_segments;
use crate::{HitTestTolerance, Path, Point, Segment};

/// Snap target kinds, in priority order (earlier wins).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SnapKind {
    Endpoint,
    Intersection,
    Midpoint,
    Center,
    Nearest,
}

/// Which kinds are offered (a held modifier can pass [`SnapFilter::none`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapFilter {
    pub endpoint: bool,
    pub intersection: bool,
    pub midpoint: bool,
    pub center: bool,
    pub nearest: bool,
}

impl SnapFilter {
    pub fn all() -> Self {
        SnapFilter { endpoint: true, intersection: true, midpoint: true, center: true, nearest: true }
    }

    pub fn none() -> Self {
        SnapFilter { endpoint: false, intersection: false, midpoint: false, center: false, nearest: false }
    }

    fn allows(&self, kind: SnapKind) -> bool {
        match kind {
            SnapKind::Endpoint => self.endpoint,
            SnapKind::Intersection => self.intersection,
            SnapKind::Midpoint => self.midpoint,
            SnapKind::Center => self.center,
            SnapKind::Nearest => self.nearest,
        }
    }
}

/// One snap target near the cursor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapCandidate<K> {
    pub kind: SnapKind,
    pub point: Point,
    pub distance: f64,
    pub source: K,
    /// The second shape of an intersection (`None` for an intersection of a shape with itself).
    pub other: Option<K>,
}

/// Arc-length midpoint of a segment: exact for lines and circular arcs (parameter midpoint);
/// numeric (Gauss–Legendre length, bisection) for cubics.
fn midpoint(from: Point, seg: &Segment) -> Point {
    match seg {
        Segment::Cubic { .. } => {
            let total = arc_length(from, seg, 1.0);
            let (mut lo, mut hi) = (0.0, 1.0);
            for _ in 0..50 {
                let mid = 0.5 * (lo + hi);
                if arc_length(from, seg, mid) < 0.5 * total {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            seg.point_at(from, 0.5 * (lo + hi))
        }
        _ => seg.point_at(from, 0.5),
    }
}

/// Length of `seg` over parameters `[0, t]` (8 × 5-point Gauss–Legendre).
fn arc_length(from: Point, seg: &Segment, t: f64) -> f64 {
    const NODES: [(f64, f64); 5] = [
        (-0.906_179_845_938_664, 0.236_926_885_056_189_1),
        (-0.538_469_310_105_683, 0.478_628_670_499_366_5),
        (0.0, 0.568_888_888_888_888_9),
        (0.538_469_310_105_683, 0.478_628_670_499_366_5),
        (0.906_179_845_938_664, 0.236_926_885_056_189_1),
    ];
    const PIECES: u32 = 8;
    let h = t / f64::from(PIECES);
    (0..PIECES)
        .map(|i| {
            let mid = h * (f64::from(i) + 0.5);
            NODES.iter().map(|&(x, w)| w * seg.derivative(from, mid + 0.5 * h * x).length()).sum::<f64>() * 0.5 * h
        })
        .sum()
}

/// All targets within `tol` of `cursor`, sorted by kind priority, then distance, then source.
pub fn snap_candidates<K: Copy + Ord>(
    shapes: &[(K, &Path)],
    cursor: Point,
    tol: &HitTestTolerance,
    filter: &SnapFilter,
) -> Vec<SnapCandidate<K>> {
    let r = tol.mm();
    let mut out: Vec<SnapCandidate<K>> = Vec::new();
    let offer = |out: &mut Vec<SnapCandidate<K>>, kind: SnapKind, point: Point, source: K, other: Option<K>| {
        let distance = point.distance(cursor);
        if filter.allows(kind)
            && distance <= r
            && !out.iter().any(|c| c.kind == kind && c.point.distance(point) < 1e-9 && c.source == source)
        {
            out.push(SnapCandidate { kind, point, distance, source, other });
        }
    };
    // Segments near the cursor, for intersections.
    let mut near: Vec<(K, crate::SegmentRef, Point, Segment)> = Vec::new();
    for &(key, path) in shapes {
        let Some(b) = path.bounds() else { continue };
        if b.distance_to(cursor) > r {
            continue;
        }
        for (si, sp) in path.subpaths().iter().enumerate() {
            let segs = subpath_segments(si, sp);
            offer(&mut out, SnapKind::Endpoint, sp.start(), key, None);
            let mut has_arc = false;
            for (at, from, seg) in &segs {
                offer(&mut out, SnapKind::Endpoint, seg.end(), key, None);
                offer(&mut out, SnapKind::Midpoint, midpoint(*from, seg), key, None);
                if let Segment::Arc(arc) = seg {
                    has_arc = true;
                    offer(&mut out, SnapKind::Center, arc.center(), key, None);
                }
                if seg.bounds(*from).distance_to(cursor) <= r {
                    near.push((key, *at, *from, *seg));
                }
            }
            if sp.is_closed()
                && !has_arc
                && let Some(sb) = crate::Path::from_subpaths(vec![sp.clone()]).bounds()
            {
                offer(&mut out, SnapKind::Center, sb.center(), key, None);
            }
        }
        if let Some(n) = path.nearest(cursor) {
            offer(&mut out, SnapKind::Nearest, n.point, key, None);
        }
    }
    if filter.intersection {
        for (i, (ka, ra, fa, a)) in near.iter().enumerate() {
            for (kb, rb, fb, b) in near.iter().skip(i + 1) {
                let same_subpath = ka == kb && ra.subpath == rb.subpath;
                for (ta, tb, point) in segment_intersections(*fa, a, *fb, b) {
                    // Shared vertices of one contour are endpoints, not intersections.
                    let at_ends = |t: f64| !(t > 1e-9 && t < 1.0 - 1e-9);
                    if same_subpath && at_ends(ta) && at_ends(tb) {
                        continue;
                    }
                    let (source, other) = if ka <= kb { (*ka, *kb) } else { (*kb, *ka) };
                    offer(&mut out, SnapKind::Intersection, point, source, (ka != kb).then_some(other));
                }
            }
        }
    }
    out.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.distance.total_cmp(&b.distance)).then(a.source.cmp(&b.source)));
    out
}
