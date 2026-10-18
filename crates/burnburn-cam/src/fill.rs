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
    /// Scan each connected region completely before the next (flood fill).
    pub flood: bool,
    /// Power ramp length at both ends of each run, 0 = none.
    pub ramp_mm: f64,
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

/// Spans of one scan row, in the scan frame: `(x_start, x_end, source)`, left to right.
type Spans = Vec<(f64, f64, GeometryId)>;

/// One non-empty scan row: its index `k` from the bottom and its `y`.
struct Row {
    k: u64,
    y: f64,
    spans: Spans,
}

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
///
/// With `spec.flood` the spans are split into connected regions (spans on adjacent rows whose
/// x-ranges overlap); each region is scanned completely, nearest region first, which avoids
/// the long empty travels of one row-by-row sweep over separate lobes.
pub(crate) fn plan(
    contours: &[&Contour],
    spec: Rows,
    frame: ScanFrame,
    head: &mut Point,
    operation: OperationId,
    cancel: &CancellationToken,
) -> Result<Vec<Step>, CompileError> {
    let t = |p: Point| frame.to_local(p);
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

    let mut all: Vec<Row> = Vec::new();
    for k in 0..rows {
        cancel.check()?;
        let y = min.y + offset + (k as f64 + 0.5) * interval;
        let spans = spans_at(&edges, y);
        if !spans.is_empty() {
            all.push(Row { k, y, spans });
        }
    }

    let mut steps = Vec::new();
    if !spec.flood {
        scan_region(&all, (min, max), spec, frame, head, &mut steps);
        return Ok(steps);
    }
    let mut regions: Vec<(Vec<Row>, (Point, Point))> = regions(all)
        .into_iter()
        .map(|r| {
            let bounds = region_extent(&r);
            (r, bounds)
        })
        .collect();
    while !regions.is_empty() {
        cancel.check()?;
        let start = t(*head);
        // Nearest region by its nearest bounds corner; ties keep the earlier region.
        let mut best = 0;
        let mut best_distance = f64::INFINITY;
        for (i, (_, (lo, hi))) in regions.iter().enumerate() {
            let (c, _, _) = nearest_corner(start, *lo, *hi);
            let d = start.distance(c);
            if d < best_distance {
                best = i;
                best_distance = d;
            }
        }
        let (region, bounds) = regions.remove(best);
        scan_region(&region, bounds, spec, frame, head, &mut steps);
    }
    Ok(steps)
}

/// The bounds corner nearest `from` (ties: bottom-left, bottom-right, top-left, top-right), and
/// whether the pass starts at the bottom / the left.
fn nearest_corner(from: Point, min: Point, max: Point) -> (Point, bool, bool) {
    let corners = [(min.x, min.y), (max.x, min.y), (min.x, max.y), (max.x, max.y)];
    let mut corner = corners[0];
    for c in corners {
        if from.distance(Point::new(c.0, c.1)) < from.distance(Point::new(corner.0, corner.1)) {
            corner = c;
        }
    }
    (Point::new(corner.0, corner.1), corner.1 == min.y, corner.0 == min.x)
}

/// Emits the rows of one region from the corner nearest the head, zig-zagging when
/// bidirectional.
fn scan_region(
    region: &[Row],
    (min, max): (Point, Point),
    spec: Rows,
    frame: ScanFrame,
    head: &mut Point,
    steps: &mut Vec<Step>,
) {
    let (_, bottom_first, left_first) = nearest_corner(frame.to_local(*head), min, max);
    for r in 0..region.len() {
        let row = &region[if bottom_first { r } else { region.len() - 1 - r }];
        let mut spans = row.spans.clone();
        // `r` counts the emitted rows: the zig-zag alternates with every one.
        let forward = left_first != (spec.bidirectional && r % 2 == 1);
        if !forward {
            spans.reverse();
        }
        for (x0, x1, g) in spans {
            let (a, b) = if forward { (x0, x1) } else { (x1, x0) };
            let (from, to) = (frame.to_bed(Point::new(a, row.y)), frame.to_bed(Point::new(b, row.y)));
            steps.push(if spec.ramp_mm > 0.0 {
                Step::RampScan { from, to, ramp_mm: spec.ramp_mm, source: g }
            } else {
                Step::Scan { from, to, source: g }
            });
            *head = to;
        }
    }
}

fn region_extent(rows: &[Row]) -> (Point, Point) {
    let (mut lo_x, mut hi_x) = (f64::INFINITY, f64::NEG_INFINITY);
    for (a, b, _) in rows.iter().flat_map(|r| r.spans.iter()) {
        lo_x = lo_x.min(*a);
        hi_x = hi_x.max(*b);
    }
    let (first, last) = (rows.first().map_or(0.0, |r| r.y), rows.last().map_or(0.0, |r| r.y));
    (Point::new(lo_x, first), Point::new(hi_x, last))
}

/// Splits rows into connected regions: spans on adjacent rows (`k` and `k + 1`) whose x-ranges
/// overlap belong together. Regions are ordered by their lowest row, then their leftmost span.
fn regions(rows: Vec<Row>) -> Vec<Vec<Row>> {
    // Union-find over (row index, span index) flattened.
    let starts: Vec<usize> = rows
        .iter()
        .scan(0, |n, r| {
            let s = *n;
            *n += r.spans.len();
            Some(s)
        })
        .collect();
    let total: usize = rows.iter().map(|r| r.spans.len()).sum();
    let mut parent: Vec<usize> = (0..total).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for i in 1..rows.len() {
        if rows[i].k != rows[i - 1].k + 1 {
            continue;
        }
        for (a, sa) in rows[i - 1].spans.iter().enumerate() {
            for (b, sb) in rows[i].spans.iter().enumerate() {
                if sa.0 < sb.1 && sb.0 < sa.1 {
                    let (ra, rb) = (find(&mut parent, starts[i - 1] + a), find(&mut parent, starts[i] + b));
                    if ra != rb {
                        parent[rb.max(ra)] = rb.min(ra);
                    }
                }
            }
        }
    }
    let mut order: Vec<usize> = Vec::new();
    let mut out: Vec<Vec<Row>> = Vec::new();
    for (i, row) in rows.into_iter().enumerate() {
        for (j, span) in row.spans.into_iter().enumerate() {
            let root = find(&mut parent, starts[i] + j);
            let slot = match order.iter().position(|&r| r == root) {
                Some(s) => s,
                None => {
                    order.push(root);
                    out.push(Vec::new());
                    out.len() - 1
                }
            };
            match out[slot].last_mut() {
                Some(last) if last.k == row.k => last.spans.push(span),
                _ => out[slot].push(Row { k: row.k, y: row.y, spans: vec![span] }),
            }
        }
    }
    out
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
