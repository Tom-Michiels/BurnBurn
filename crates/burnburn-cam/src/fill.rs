//! Fill planning: scan rows at row centres, even-odd spans, bidirectional order.

use burnburn_geometry::Point;
use burnburn_types::{CancellationToken, GeometryId, OperationId};

use crate::CompileError;
use crate::compile::Step;
use crate::line::Contour;

/// Row spacing and direction of one scan pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Rows {
    pub interval_mm: f64,
    pub bidirectional: bool,
}

/// The frame scan rows are horizontal in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ScanFrame {
    /// Rows along bed X.
    X,
    /// Rows along bed Y (the problem is transposed).
    Y,
    /// Rows along the direction `(cos, sin)` (counter-clockwise from bed X).
    Rotated { cos: f64, sin: f64 },
}

impl ScanFrame {
    pub fn to_local(self, p: Point) -> Point {
        match self {
            ScanFrame::X => p,
            ScanFrame::Y => Point::new(p.y, p.x),
            ScanFrame::Rotated { cos, sin } => Point::new(cos * p.x + sin * p.y, -sin * p.x + cos * p.y),
        }
    }

    pub fn to_bed(self, p: Point) -> Point {
        match self {
            ScanFrame::X => p,
            ScanFrame::Y => Point::new(p.y, p.x),
            ScanFrame::Rotated { cos, sin } => Point::new(cos * p.x - sin * p.y, sin * p.x + cos * p.y),
        }
    }
}

/// Upper bound on scan rows per operation pass; keeps one setting from making a compile unbounded.
pub(crate) const MAX_SCAN_ROWS: u64 = 2_000_000;

/// Plans one fill pass over the closed contours of an operation; updates `head`.
///
/// Rows lie at row centres: `n = floor(h / interval)` rows, centred in the extent `h`, so for
/// an integral ratio the first and last row are half an interval inside the edges (as in the
/// LightBurn Fill fixture). Spans use the even-odd rule over all closed contours of the
/// operation, with a half-open crossing rule (`y0 <= y < y1`) so a vertex is counted once.
/// The pass starts at the bounds corner nearest the head (ties: bottom-left, bottom-right,
/// top-left, top-right) and zig-zags when bidirectional. Open contours are ignored here and
/// reported by preflight. Axis Y scans run on the transposed problem, angled scans on the
/// rotated one (`frame`).
pub(crate) fn plan(
    contours: &[Contour],
    spec: Rows,
    frame: ScanFrame,
    head: &mut Point,
    operation: OperationId,
    cancel: &CancellationToken,
) -> Result<Vec<Step>, CompileError> {
    let t = |p: Point| frame.to_local(p);
    let back = |p: Point| frame.to_bed(p);
    let mut edges: Vec<(Point, Point, GeometryId)> = Vec::new();
    for c in contours.iter().filter(|c| c.closed && c.points.len() >= 3) {
        let n = c.points.len();
        for k in 0..n {
            edges.push((t(c.points[k]), t(c.points[(k + 1) % n]), c.geometry));
        }
    }
    let Some((min, max)) = extent(&edges) else { return Ok(Vec::new()) };
    let interval = spec.interval_mm;
    let h = max.y - min.y;
    // A tiny relative slack so 15 / 0.1 = 149.99999999999997 still gives 150 rows.
    let ratio = h / interval;
    let mut rows = (ratio + ratio.abs() * 1e-12).floor();
    if rows < 1.0 && h > 0.0 {
        rows = 1.0;
    }
    if !(rows <= MAX_SCAN_ROWS as f64) {
        return Err(CompileError::TooManyScanRows { operation, rows: rows as u64 });
    }
    let rows = rows as u64;
    let offset = (h - rows as f64 * interval) / 2.0;

    let start = t(*head);
    let corners = [(min.x, min.y), (max.x, min.y), (min.x, max.y), (max.x, max.y)];
    let mut corner = corners[0];
    for c in corners {
        if start.distance(Point::new(c.0, c.1)) < start.distance(Point::new(corner.0, corner.1)) {
            corner = c;
        }
    }
    let bottom_first = corner.1 == min.y;
    let left_first = corner.0 == min.x;
    let bidirectional = spec.bidirectional;

    let mut steps = Vec::new();
    let mut emitted = 0u64;
    for r in 0..rows {
        cancel.check()?;
        let k = if bottom_first { r } else { rows - 1 - r };
        let y = min.y + offset + (k as f64 + 0.5) * interval;
        let mut spans = spans_at(&edges, y);
        if spans.is_empty() {
            continue;
        }
        let forward = left_first != (bidirectional && emitted % 2 == 1);
        if !forward {
            spans.reverse();
        }
        for (x0, x1, g) in spans {
            let (a, b) = if forward { (x0, x1) } else { (x1, x0) };
            let (from, to) = (back(Point::new(a, y)), back(Point::new(b, y)));
            steps.push(Step::Scan { from, to, source: g });
            *head = to;
        }
        emitted += 1;
    }
    Ok(steps)
}

fn extent(edges: &[(Point, Point, GeometryId)]) -> Option<(Point, Point)> {
    let mut it = edges.iter().map(|e| e.0);
    let first = it.next()?;
    let (mut min, mut max) = (first, first);
    for p in it {
        min = Point::new(min.x.min(p.x), min.y.min(p.y));
        max = Point::new(max.x.max(p.x), max.y.max(p.y));
    }
    Some((min, max))
}

/// Even-odd spans `(x_start, x_end, source)` on the horizontal line `y`, left to right.
/// The source is the geometry of the edge where the span starts.
fn spans_at(edges: &[(Point, Point, GeometryId)], y: f64) -> Vec<(f64, f64, GeometryId)> {
    let mut xs: Vec<(f64, GeometryId)> = edges
        .iter()
        .filter(|(a, b, _)| (a.y <= y && y < b.y) || (b.y <= y && y < a.y))
        .map(|(a, b, g)| (a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y), *g))
        .collect();
    xs.sort_by(|p, q| p.0.total_cmp(&q.0).then(p.1.cmp(&q.1)));
    let (pairs, _) = xs.as_chunks::<2>();
    pairs.iter().filter(|[a, b]| a.0 < b.0).map(|[a, b]| (a.0, b.0, a.1)).collect()
}
