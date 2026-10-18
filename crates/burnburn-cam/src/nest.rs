//! Mixed-part nesting (IM26, C10): bottom-left fill on a conservative occupancy grid.
//!
//! Stock and parts are rasterised on a grid of `step` millimetres. A part occupies every cell
//! its area reaches into (the closed union of those cells covers the part); a stock cell is
//! free only when the closed cell lies inside the usable area (stock minus holes and keep-out
//! zones, shrunk by the edge margin). Placed parts block their footprint grown by `spacing`.
//! Every accepted placement is therefore geometrically valid — interiors never overlap,
//! distances are at least `spacing` and `edge_margin` (touching at exactly that distance is
//! allowed) — at the cost of up to one grid step of packing density per side. Candidates are tried bottom row first, then left to
//! right (bottom-left fill); parts by decreasing net area, then by key; rotations only from
//! the part's `RotationSet`, filtered by grain. Deterministic: no randomness, no hash maps.

use std::fmt;

use burnburn_geometry::{
    Affine, BooleanOp, BooleanOptions, Bounds, FillRule, OffsetJoin, Path, Point, ProductionTolerance,
};
use burnburn_types::{CancellationToken, Cancelled, Length};

use crate::array::rotation_about;

/// Caller's identity of a part (for example a part instance or order line).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PartKey(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StockId(pub u32);

/// Allowed rotations in degrees, normalised to `[0, 360)`, without duplicates, in given order.
#[derive(Debug, Clone, PartialEq)]
pub struct RotationSet(Vec<f64>);

impl RotationSet {
    pub fn new(degrees: Vec<f64>) -> Result<Self, NestError> {
        let mut out: Vec<f64> = Vec::with_capacity(degrees.len());
        for d in degrees {
            if !d.is_finite() {
                return Err(NestError::InvalidInput { field: "rotation" });
            }
            let r = d.rem_euclid(360.0);
            let r = if r >= 360.0 { 0.0 } else { r };
            if !out.contains(&r) {
                out.push(r);
            }
        }
        if out.is_empty() {
            return Err(NestError::InvalidInput { field: "rotation" });
        }
        Ok(RotationSet(out))
    }

    pub fn degrees(&self) -> &[f64] {
        &self.0
    }
}

/// One ordered part. The outline is closed (outer contour + holes, mm, part coordinates).
#[derive(Debug, Clone, PartialEq)]
pub struct NestPart {
    pub key: PartKey,
    pub outline: Path,
    pub count: u32,
    pub rotations: RotationSet,
    /// Grain direction of the part in part coordinates (degrees); `None` = no grain rule.
    pub grain_deg: Option<f64>,
}

/// A sheet (or remnant). `outline` holes and `keep_out` areas are forbidden zones.
#[derive(Debug, Clone, PartialEq)]
pub struct Stock {
    pub id: StockId,
    pub outline: Path,
    pub keep_out: Vec<Path>,
    /// Grain direction of the sheet (degrees); parts with a grain must follow it (mod 180°).
    pub grain_deg: Option<f64>,
}

/// What the utilisation is relative to (always reported with the value).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UtilisationBasis {
    /// Net area of all given stock (outline minus holes and keep-out zones).
    StockArea,
    /// Sum over used sheets of the bounding box of the placed parts.
    UsedBoundingBox,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NestParams {
    /// Minimum distance between parts.
    pub spacing: Length,
    /// Minimum distance to stock edges, holes and keep-out zones.
    pub edge_margin: Length,
    /// Grid step of the candidate search and occupancy raster.
    pub step: Length,
    pub tolerance: ProductionTolerance,
    /// Budget: candidate positions tested in total; when exhausted the rest is reported unplaced.
    pub max_candidates: u64,
    pub basis: UtilisationBasis,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub part: PartKey,
    /// 0-based copy number within the part's order count.
    pub copy: u32,
    pub stock: StockId,
    pub rotation_deg: f64,
    /// Part coordinates → stock coordinates (rotation about the origin, then translation).
    pub transform: Affine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnplacedReason {
    /// No free position on any stock.
    NoFit,
    /// The grain rule leaves no allowed rotation on any stock.
    NoAllowedRotation,
    /// The candidate budget ran out.
    Budget,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Unplaced {
    pub part: PartKey,
    pub count: u32,
    pub reason: UnplacedReason,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Utilisation {
    pub used_mm2: f64,
    pub basis: UtilisationBasis,
    pub denominator_mm2: f64,
    /// `used / denominator` (0 when the denominator is 0).
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NestResult {
    pub placements: Vec<Placement>,
    pub unplaced: Vec<Unplaced>,
    pub utilisation: Utilisation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NestError {
    InvalidInput {
        field: &'static str,
    },
    /// A part or stock outline could not be processed (open contour, offset/boolean failure).
    Geometry {
        what: &'static str,
    },
    /// The occupancy grid would be too large for the stock and step.
    GridTooLarge,
    Cancelled,
}

impl From<Cancelled> for NestError {
    fn from(_: Cancelled) -> Self {
        NestError::Cancelled
    }
}

impl fmt::Display for NestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NestError::InvalidInput { field } => write!(f, "nesting input {field} is invalid"),
            NestError::Geometry { what } => write!(f, "nesting geometry: {what}"),
            NestError::GridTooLarge => f.write_str("nesting grid too large for this step"),
            NestError::Cancelled => f.write_str("nesting cancelled"),
        }
    }
}

impl std::error::Error for NestError {}

/// Upper bound on cells of one occupancy grid.
pub const MAX_GRID_CELLS: u64 = 50_000_000;

/// Closed contours of `path` flattened within `tol`, without the repeated start point.
fn polygons(path: &Path, tol: &ProductionTolerance) -> Result<Vec<Vec<Point>>, NestError> {
    let mut out = Vec::new();
    for pl in path.flatten(tol) {
        if !pl.closed {
            return Err(NestError::Geometry { what: "open contour" });
        }
        let mut pts = pl.points;
        if pts.len() > 1 && pts.first() == pts.last() {
            pts.pop();
        }
        if pts.len() >= 3 {
            out.push(pts);
        }
    }
    Ok(out)
}

fn area2(p: &[Point]) -> f64 {
    let n = p.len();
    (0..n).map(|k| p[k].x * p[(k + 1) % n].y - p[(k + 1) % n].x * p[k].y).sum()
}

/// Even-odd net area of closed contours.
fn net_area(polys: &[Vec<Point>]) -> f64 {
    polys
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let depth = polys.iter().enumerate().filter(|(j, q)| *j != i && inside(p[0], q)).count();
            let a = 0.5 * area2(p).abs();
            if depth % 2 == 0 { a } else { -a }
        })
        .sum()
}

fn inside(p: Point, poly: &[Point]) -> bool {
    crate::line::point_in_polygon(p, poly)
}

fn inside_even_odd(p: Point, polys: &[Vec<Point>]) -> bool {
    polys.iter().filter(|q| inside(p, q)).count() % 2 == 1
}

/// A boolean occupancy raster with origin `origin` and `cols × rows` cells of `step`.
#[derive(Debug, Clone)]
struct Grid {
    origin: Point,
    cols: i64,
    rows: i64,
    cells: Vec<bool>,
}

impl Grid {
    fn new(origin: Point, step: f64, cols: i64, rows: i64) -> Result<Self, NestError> {
        let n = (cols.max(0) as u64).saturating_mul(rows.max(0) as u64);
        if n > MAX_GRID_CELLS {
            return Err(NestError::GridTooLarge);
        }
        let _ = step;
        Ok(Grid { origin, cols, rows, cells: vec![false; n as usize] })
    }

    fn get(&self, c: i64, r: i64) -> bool {
        c >= 0 && r >= 0 && c < self.cols && r < self.rows && self.cells[(r * self.cols + c) as usize]
    }

    fn set(&mut self, c: i64, r: i64) {
        if c >= 0 && r >= 0 && c < self.cols && r < self.rows {
            self.cells[(r * self.cols + c) as usize] = true;
        }
    }
}

/// Cells (relative to `origin`) that the area bounded by `polys` reaches into: every cell whose
/// open interior a boundary segment passes through, plus every cell whose centre is inside.
/// The closed union of these cells covers the area.
fn touched_cells(polys: &[Vec<Point>], origin: Point, step: f64) -> Vec<(i64, i64)> {
    let mut cells = boundary_cells(polys, origin, step);
    let cell = |v: f64, o: f64| ((v - o) / step).floor() as i64;
    let mut bounds: Option<Bounds> = None;
    for p in polys.iter().flatten() {
        match bounds.as_mut() {
            Some(b) => b.include(*p),
            None => bounds = Some(Bounds::from_point(*p)),
        }
    }
    if let Some(b) = bounds {
        for r in cell(b.min().y, origin.y)..=cell(b.max().y, origin.y) {
            for c in cell(b.min().x, origin.x)..=cell(b.max().x, origin.x) {
                if inside_even_odd(centre(origin, step, c, r), polys) {
                    cells.push((c, r));
                }
            }
        }
    }
    cells.sort_unstable();
    cells.dedup();
    cells
}

fn centre(origin: Point, step: f64, c: i64, r: i64) -> Point {
    Point::new(origin.x + (c as f64 + 0.5) * step, origin.y + (r as f64 + 0.5) * step)
}

/// Every cell whose open interior a boundary segment passes through.
fn boundary_cells(polys: &[Vec<Point>], origin: Point, step: f64) -> Vec<(i64, i64)> {
    let cell = |v: f64, o: f64| ((v - o) / step).floor() as i64;
    let mut cells = Vec::new();
    for poly in polys {
        let n = poly.len();
        for k in 0..n {
            let (a, b) = (poly[k], poly[(k + 1) % n]);
            let (c0, c1) = (cell(a.x.min(b.x), origin.x) - 1, cell(a.x.max(b.x), origin.x));
            let (r0, r1) = (cell(a.y.min(b.y), origin.y) - 1, cell(a.y.max(b.y), origin.y));
            for r in r0..=r1 {
                for c in c0..=c1 {
                    let (x0, y0) = (origin.x + c as f64 * step, origin.y + r as f64 * step);
                    if segment_meets_box(a, b, x0, y0, x0 + step, y0 + step) {
                        cells.push((c, r));
                    }
                }
            }
        }
    }
    cells
}

/// One part in one rotation: raw and spacing-grown cell masks relative to the cell of the
/// rotated outline's bounding-box minimum.
#[derive(Debug, Clone)]
struct Shape {
    rotation_deg: f64,
    /// Part coordinates → local coordinates (bbox minimum at the origin).
    local: Affine,
    raw: Vec<(i64, i64)>,
    grown: Vec<(i64, i64)>,
}

fn rotated(part: &NestPart, deg: f64, p: &NestParams, cancel: &CancellationToken) -> Result<Shape, NestError> {
    let bad = |what| move |_| NestError::Geometry { what };
    let r = rotation_about(Point::ORIGIN, deg).map_err(bad("rotation"))?;
    let turned = part.outline.transform(&r);
    let b = turned.bounds().ok_or(NestError::Geometry { what: "empty part" })?;
    let to_local = Affine::translate(Point::ORIGIN - b.min()).map_err(bad("translation"))?;
    let local = r.then(&to_local);
    let shape = part.outline.transform(&local);
    let step = p.step.mm();
    let raw = touched_cells(&polygons(&shape, &p.tolerance)?, Point::ORIGIN, step);
    let grown_path = if p.spacing.mm() > 0.0 {
        burnburn_geometry::offset(&shape, p.spacing.mm(), OffsetJoin::Round, &p.tolerance, cancel)
            .map_err(|_| NestError::Geometry { what: "spacing offset" })?
    } else {
        shape
    };
    let grown = touched_cells(&polygons(&grown_path, &p.tolerance)?, Point::ORIGIN, step);
    Ok(Shape { rotation_deg: deg, local, raw, grown })
}

/// Free/blocked raster of one sheet plus its net area.
struct Sheet<'a> {
    stock: &'a Stock,
    blocked: Grid,
    net_area: f64,
    placed: Vec<Bounds>,
}

fn sheet<'a>(stock: &'a Stock, p: &NestParams, cancel: &CancellationToken) -> Result<Sheet<'a>, NestError> {
    let options = BooleanOptions { fill: FillRule::EvenOdd, tolerance: p.tolerance };
    let mut usable = stock.outline.clone();
    for k in &stock.keep_out {
        usable = burnburn_geometry::boolean(&usable, k, BooleanOp::Difference, &options, cancel)
            .map_err(|_| NestError::Geometry { what: "keep-out" })?;
    }
    let net_area = net_area(&polygons(&usable, &p.tolerance)?);
    if p.edge_margin.mm() > 0.0 {
        usable = burnburn_geometry::offset(&usable, -p.edge_margin.mm(), OffsetJoin::Miter, &p.tolerance, cancel)
            .map_err(|_| NestError::Geometry { what: "edge margin" })?;
    }
    let polys = polygons(&usable, &p.tolerance)?;
    let b = stock.outline.bounds().ok_or(NestError::Geometry { what: "empty stock" })?;
    let step = p.step.mm();
    let cols = (b.width() / step).ceil() as i64 + 1;
    let rows = (b.height() / step).ceil() as i64 + 1;
    let mut blocked = Grid::new(b.min(), step, cols, rows)?;
    // Free = centre inside the usable area and no usable boundary meets the (closed) cell.
    for r in 0..rows {
        cancel.check()?;
        for c in 0..cols {
            if !inside_even_odd(centre(b.min(), step, c, r), &polys) {
                blocked.set(c, r);
            }
        }
    }
    for (c, r) in boundary_cells(&polys, b.min(), step) {
        blocked.set(c, r);
    }
    Ok(Sheet { stock, blocked, net_area, placed: Vec::new() })
}

/// Liang–Barsky on the *open* box: does segment `a b` pass through the cell interior? A
/// segment along a cell edge does not (touching is allowed; see the module documentation).
fn segment_meets_box(a: Point, b: Point, x0: f64, y0: f64, x1: f64, y1: f64) -> bool {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [(-dx, a.x - x0), (dx, x1 - a.x), (-dy, a.y - y0), (dy, y1 - a.y)] {
        if p == 0.0 {
            if q <= 0.0 {
                return false;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    t0 < t1
}

fn grain_allows(part: &NestPart, stock: &Stock, deg: f64) -> bool {
    match (part.grain_deg, stock.grain_deg) {
        (Some(pg), Some(sg)) => {
            let d = (pg + deg - sg).rem_euclid(180.0);
            d.abs() < 1e-9 || (180.0 - d).abs() < 1e-9
        }
        _ => true,
    }
}

/// Nests `parts` on `stock` (sheets in the given order). See the module documentation.
pub fn nest(
    parts: &[NestPart],
    stock: &[Stock],
    p: &NestParams,
    cancel: &CancellationToken,
) -> Result<NestResult, NestError> {
    if !(p.step.mm() > 0.0) {
        return Err(NestError::InvalidInput { field: "step" });
    }
    if !(p.spacing.mm() >= 0.0) || !(p.edge_margin.mm() >= 0.0) {
        return Err(NestError::InvalidInput { field: "spacing" });
    }
    cancel.check()?;
    let mut sheets: Vec<Sheet<'_>> = stock.iter().map(|s| sheet(s, p, cancel)).collect::<Result<_, _>>()?;
    let mut order: Vec<(usize, f64)> = Vec::with_capacity(parts.len());
    for (i, part) in parts.iter().enumerate() {
        order.push((i, net_area(&polygons(&part.outline, &p.tolerance)?)));
    }
    // Larger parts first; equal areas by key, then input order (stable sort).
    order.sort_by(|a, b| b.1.total_cmp(&a.1).then(parts[a.0].key.cmp(&parts[b.0].key)));

    let mut placements = Vec::new();
    let mut unplaced = Vec::new();
    let mut budget = p.max_candidates;
    let mut used_mm2 = 0.0;
    for (i, area) in order {
        let part = &parts[i];
        let mut shapes: Vec<Shape> = Vec::new();
        for &deg in part.rotations.degrees() {
            if stock.iter().any(|s| grain_allows(part, s, deg)) {
                shapes.push(rotated(part, deg, p, cancel)?);
            }
        }
        if shapes.is_empty() {
            unplaced.push(Unplaced { part: part.key, count: part.count, reason: UnplacedReason::NoAllowedRotation });
            continue;
        }
        let mut copy = 0;
        while copy < part.count {
            cancel.check()?;
            let mut found: Option<(usize, &Shape, i64, i64)> = None;
            let mut exhausted = false;
            for (si, sh) in sheets.iter().enumerate() {
                let mut best: Option<(&Shape, i64, i64)> = None;
                for shape in shapes.iter().filter(|s| grain_allows(part, sh.stock, s.rotation_deg)) {
                    if let Some((c, r)) = first_fit(&sh.blocked, shape, &mut budget, cancel)? {
                        if best.is_none_or(|(_, bc, br)| (r, c) < (br, bc)) {
                            best = Some((shape, c, r));
                        }
                    } else if budget == 0 {
                        exhausted = true;
                    }
                }
                if let Some((shape, c, r)) = best {
                    found = Some((si, shape, c, r));
                    break;
                }
            }
            let Some((si, shape, c, r)) = found else {
                let reason = if exhausted { UnplacedReason::Budget } else { UnplacedReason::NoFit };
                unplaced.push(Unplaced { part: part.key, count: part.count - copy, reason });
                break;
            };
            let sh = &mut sheets[si];
            for &(dc, dr) in &shape.grown {
                sh.blocked.set(c + dc, r + dr);
            }
            let at =
                Point::new(sh.blocked.origin.x + c as f64 * p.step.mm(), sh.blocked.origin.y + r as f64 * p.step.mm());
            let transform = shape
                .local
                .then(&Affine::translate(at - Point::ORIGIN).map_err(|_| NestError::Geometry { what: "placement" })?);
            if let Some(b) = part.outline.transform(&transform).bounds() {
                sh.placed.push(b);
            }
            placements.push(Placement {
                part: part.key,
                copy,
                stock: sh.stock.id,
                rotation_deg: shape.rotation_deg,
                transform,
            });
            used_mm2 += area;
            copy += 1;
        }
    }
    let denominator_mm2 = match p.basis {
        UtilisationBasis::StockArea => sheets.iter().map(|s| s.net_area).sum(),
        UtilisationBasis::UsedBoundingBox => sheets
            .iter()
            .filter_map(|s| s.placed.iter().copied().reduce(Bounds::union))
            .map(|b| b.width() * b.height())
            .sum(),
    };
    let value = if denominator_mm2 > 0.0 { used_mm2 / denominator_mm2 } else { 0.0 };
    Ok(NestResult {
        placements,
        unplaced,
        utilisation: Utilisation { used_mm2, basis: p.basis, denominator_mm2, value },
    })
}

/// Bottom-left first free position of `shape` on `grid`: lowest row, then lowest column, where
/// no raw cell of the shape is blocked. Decrements `budget` per candidate.
fn first_fit(
    grid: &Grid,
    shape: &Shape,
    budget: &mut u64,
    cancel: &CancellationToken,
) -> Result<Option<(i64, i64)>, NestError> {
    for r in 0..grid.rows {
        cancel.check()?;
        for c in 0..grid.cols {
            if *budget == 0 {
                return Ok(None);
            }
            *budget -= 1;
            if shape.raw.iter().all(|&(dc, dr)| {
                let (x, y) = (c + dc, r + dr);
                x >= 0 && y >= 0 && x < grid.cols && y < grid.rows && !grid.get(x, y)
            }) {
                return Ok(Some((c, r)));
            }
        }
    }
    Ok(None)
}

/// The placements of `result` on `stock` as one document command on `plate` (IM26 ↔ IM21):
/// `StructureCommand::ApplyNest`, one undo step. `instances[part]` lists the part instances of
/// each part key in copy order. No placement on `stock`, or a placement without an instance,
/// is an error (never a guess). Unplaced parts are not in the command.
pub fn apply_nest_command(
    result: &NestResult,
    stock: StockId,
    plate: burnburn_document::structure::PlateId,
    instances: &std::collections::BTreeMap<PartKey, Vec<burnburn_document::structure::PartInstanceId>>,
) -> Result<burnburn_document::structure::StructureCommand, NestError> {
    let mut placements = Vec::new();
    for p in result.placements.iter().filter(|p| p.stock == stock) {
        let instance = instances
            .get(&p.part)
            .and_then(|list| list.get(p.copy as usize))
            .ok_or(NestError::InvalidInput { field: "instances" })?;
        placements.push((*instance, p.transform));
    }
    if placements.is_empty() {
        return Err(NestError::InvalidInput { field: "stock" });
    }
    Ok(burnburn_document::structure::StructureCommand::ApplyNest { plate, placements })
}
