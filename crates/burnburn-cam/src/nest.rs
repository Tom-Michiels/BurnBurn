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
    Affine, BooleanOp, BooleanOptions, Bounds, FillRule, OffsetJoin, Path, Point, ProductionTolerance, Vector,
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
    /// Placed as the mirror image of the part (mirrored first, then rotated; see [`MirrorPolicy`]).
    pub mirrored: bool,
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

/// What is used and what is left of one sheet (remnants for a later job).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StockUse {
    pub stock: StockId,
    /// Net area of the sheet: outline minus holes and keep-out zones, mm².
    pub net_mm2: f64,
    /// Net area of the parts placed on it, mm².
    pub placed_mm2: f64,
    /// `net - placed`: what is left, mm².
    pub free_mm2: f64,
}

/// Whether a part may be placed as its mirror image. Mirroring changes what a part is (a cut
/// bracket is not interchangeable with its reflection), so it is a choice and never a way to get a
/// better score: [`MirrorPolicy::Forbidden`] is the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MirrorPolicy {
    #[default]
    Forbidden,
    Allowed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NestOptions {
    pub mirror: MirrorPolicy,
}

/// How much of an order was placed, per part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartStatus {
    pub part: PartKey,
    pub ordered: u32,
    pub placed: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NestResult {
    pub placements: Vec<Placement>,
    pub unplaced: Vec<Unplaced>,
    pub utilisation: Utilisation,
    /// Per sheet, in the order of the stock given.
    pub stock_use: Vec<StockUse>,
}

/// Ordered and placed count of every part, in the order of `parts`.
pub fn order_status(result: &NestResult, parts: &[NestPart]) -> Vec<PartStatus> {
    parts
        .iter()
        .map(|p| PartStatus {
            part: p.key,
            ordered: p.count,
            placed: u32::try_from(result.placements.iter().filter(|pl| pl.part == p.key).count()).unwrap_or(u32::MAX),
        })
        .collect()
}

/// How many complete sets were placed: `per_set` lists the parts of one set with the number of
/// each. A part that was not ordered or placed at all makes it zero; no parts is no set.
pub fn complete_sets(result: &NestResult, per_set: &[(PartKey, u32)]) -> u32 {
    per_set
        .iter()
        .filter(|(_, n)| *n > 0)
        .map(|(key, n)| {
            let placed = result.placements.iter().filter(|pl| pl.part == *key).count();
            u32::try_from(placed).unwrap_or(u32::MAX) / n
        })
        .min()
        .unwrap_or(0)
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
    boundary_cells_grown(polys, origin, step, 0.0)
}

/// [`boundary_cells`] with every cell grown by `eps` on all sides: with a small positive `eps` a
/// segment along a cell border meets both cells beside it.
fn boundary_cells_grown(polys: &[Vec<Point>], origin: Point, step: f64, eps: f64) -> Vec<(i64, i64)> {
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
                    if segment_meets_box(a, b, x0 - eps, y0 - eps, x0 + step + eps, y0 + step + eps) {
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
    mirrored: bool,
    /// Part coordinates → local coordinates (bbox minimum at the origin).
    local: Affine,
    raw: Vec<(i64, i64)>,
    grown: Vec<(i64, i64)>,
    /// Only for [`Packing::Tight`].
    tight: Option<TightShape>,
}

/// What exact placement needs of a shape (local coordinates, bbox minimum at the origin).
#[derive(Debug, Clone)]
struct TightShape {
    /// Cells whose centre lies inside the shape: the optimistic raster, validated exactly.
    raw_centres: Vec<(i64, i64)>,
    /// The shape pulled in by the slack, so that the exact checks accept a position that is at
    /// most the slack inside a neighbour and never one with a poking corner.
    shrunk: Path,
    /// The shape grown by the spacing: what a later part must keep out of.
    grown: Path,
    /// Width and height of the shape (its bounds start at the local origin).
    size: (f64, f64),
    /// Bounds of `shrunk` (local coordinates).
    shrunk_bounds: Option<Bounds>,
    /// Points along the boundary of `shrunk`, at most [`DEEP_SAMPLE_SPACING`] apart (local
    /// coordinates): a sample in a deep cell of the sheet proves an overlap (see [`DeepMask`]).
    samples: Vec<Point>,
}

/// A cancelled offset is a cancelled nest, not a geometry problem.
fn offset_error(e: burnburn_geometry::OffsetError, what: &'static str) -> NestError {
    match e {
        burnburn_geometry::OffsetError::Cancelled => NestError::Cancelled,
        _ => NestError::Geometry { what },
    }
}

#[cfg(test)]
fn rotated(
    part: &NestPart,
    deg: f64,
    mirrored: bool,
    p: &NestParams,
    cancel: &CancellationToken,
) -> Result<Shape, NestError> {
    rotated_with(part, deg, mirrored, p, false, cancel)
}

/// How far the exact checks may let two parts overlap (mm): a tenth of the production tolerance.
fn slack_mm(p: &NestParams) -> f64 {
    p.tolerance.mm() / 10.0
}

fn rotated_with(
    part: &NestPart,
    deg: f64,
    mirrored: bool,
    p: &NestParams,
    tight: bool,
    cancel: &CancellationToken,
) -> Result<Shape, NestError> {
    let bad = |what| move |_| NestError::Geometry { what };
    let turn = rotation_about(Point::ORIGIN, deg).map_err(bad("rotation"))?;
    // A mirrored placement is the part reflected in its own y axis first, then turned.
    let r =
        if mirrored { Affine::new([-1.0, 0.0, 0.0, 1.0, 0.0, 0.0]).map_err(bad("mirror"))?.then(&turn) } else { turn };
    let turned = part.outline.transform(&r);
    let b = turned.bounds().ok_or(NestError::Geometry { what: "empty part" })?;
    let to_local = Affine::translate(Point::ORIGIN - b.min()).map_err(bad("translation"))?;
    let local = r.then(&to_local);
    let shape = part.outline.transform(&local);
    let step = p.step.mm();
    let raw = touched_cells(&polygons(&shape, &p.tolerance)?, Point::ORIGIN, step);
    let grown_path = if p.spacing.mm() > 0.0 {
        burnburn_geometry::offset(&shape, p.spacing.mm(), OffsetJoin::Round, &p.tolerance, cancel)
            .map_err(|e| offset_error(e, "spacing offset"))?
    } else {
        shape.clone()
    };
    let grown = touched_cells(&polygons(&grown_path, &p.tolerance)?, Point::ORIGIN, step);
    let tight = if tight {
        // A part too thin to pull in keeps its outline: the exact check is then merely strict.
        let shrunk = match burnburn_geometry::offset(&shape, -slack_mm(p), OffsetJoin::Miter, &p.tolerance, cancel) {
            Ok(path) if !path.is_empty() => path,
            Err(burnburn_geometry::OffsetError::Cancelled) => return Err(NestError::Cancelled),
            _ => shape.clone(),
        };
        let raw_centres = centre_cells(&polygons(&shape, &p.tolerance)?, Point::ORIGIN, step);
        let size = shape.bounds().map_or((0.0, 0.0), |b| (b.width(), b.height()));
        let samples = boundary_samples(&polygons(&shrunk, &p.tolerance)?, deep_cell(p));
        let shrunk_bounds = shrunk.bounds();
        Some(TightShape { raw_centres, shrunk, grown: grown_path, size, shrunk_bounds, samples })
    } else {
        None
    };
    Ok(Shape { rotation_deg: deg, mirrored, local, raw, grown, tight })
}

/// Cells whose centre is inside the area bounded by `polys` (even-odd).
fn centre_cells(polys: &[Vec<Point>], origin: Point, step: f64) -> Vec<(i64, i64)> {
    let cell = |v: f64, o: f64| ((v - o) / step).floor() as i64;
    let mut bounds: Option<Bounds> = None;
    for p in polys.iter().flatten() {
        match bounds.as_mut() {
            Some(b) => b.include(*p),
            None => bounds = Some(Bounds::from_point(*p)),
        }
    }
    let mut cells = Vec::new();
    if let Some(b) = bounds {
        for r in cell(b.min().y, origin.y)..=cell(b.max().y, origin.y) {
            for c in cell(b.min().x, origin.x)..=cell(b.max().x, origin.x) {
                if inside_even_odd(centre(origin, step, c, r), polys) {
                    cells.push((c, r));
                }
            }
        }
    }
    cells
}

/// Free/blocked raster of one sheet plus its net area.
struct Sheet<'a> {
    stock: &'a Stock,
    blocked: Grid,
    net_area: f64,
    placed: Vec<Bounds>,
    /// Net area of the parts placed so far, mm².
    placed_mm2: f64,
    /// Only for [`Packing::Tight`].
    tight: Option<TightSheet>,
}

/// The exact side of a sheet: the usable area and the grown outline of every placed part.
struct TightSheet {
    usable: Path,
    usable_bounds: Option<Bounds>,
    /// Optimistic raster: blocked when the centre of a cell is outside the usable area or inside
    /// a placed part's grown outline. A candidate is always confirmed exactly.
    blocked: Grid,
    placed: Vec<(Bounds, Path)>,
    /// The usable area is the rectangle `usable_bounds`: staying inside its bounds is staying inside it.
    usable_is_rect: bool,
    /// Where a placed part's grown outline is certainly deep.
    deep: DeepMask,
    /// Overlays done so far (see [`NestWork`]).
    overlays: std::cell::Cell<u64>,
}

#[cfg(test)]
fn sheet<'a>(stock: &'a Stock, p: &NestParams, cancel: &CancellationToken) -> Result<Sheet<'a>, NestError> {
    sheet_with(stock, p, false, cancel)
}

fn sheet_with<'a>(
    stock: &'a Stock,
    p: &NestParams,
    tight: bool,
    cancel: &CancellationToken,
) -> Result<Sheet<'a>, NestError> {
    let options = BooleanOptions { fill: FillRule::EvenOdd, tolerance: p.tolerance };
    let mut usable = stock.outline.clone();
    for k in &stock.keep_out {
        usable =
            burnburn_geometry::boolean(&usable, k, BooleanOp::Difference, &options, cancel).map_err(|e| match e {
                burnburn_geometry::BooleanError::Cancelled => NestError::Cancelled,
                _ => NestError::Geometry { what: "keep-out" },
            })?;
    }
    let net_area = net_area(&polygons(&usable, &p.tolerance)?);
    if p.edge_margin.mm() > 0.0 {
        usable = burnburn_geometry::offset(&usable, -p.edge_margin.mm(), OffsetJoin::Miter, &p.tolerance, cancel)
            .map_err(|e| offset_error(e, "edge margin"))?;
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
    // At this point `blocked` holds exactly the cells whose centre is outside the usable area.
    let tight = if tight {
        let usable_bounds = usable.bounds();
        // A contour that fills its own bounding box is that box.
        let usable_is_rect = usable_bounds.is_some_and(|ub| {
            let filled = self::net_area(&polys);
            polys.len() == 1 && (filled - ub.width() * ub.height()).abs() <= 1e-9 * (1.0 + filled)
        });
        Some(TightSheet {
            usable_bounds,
            usable: usable.clone(),
            blocked: blocked.clone(),
            placed: Vec::new(),
            usable_is_rect,
            deep: DeepMask::new(b, deep_cell(p))?,
            overlays: std::cell::Cell::new(0),
        })
    } else {
        None
    };
    for (c, r) in boundary_cells(&polys, b.min(), step) {
        blocked.set(c, r);
    }
    Ok(Sheet { stock, blocked, net_area, placed: Vec::new(), placed_mm2: 0.0, tight })
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

fn grain_allows(part: &NestPart, stock: &Stock, deg: f64, mirrored: bool) -> bool {
    match (part.grain_deg, stock.grain_deg) {
        (Some(pg), Some(sg)) => {
            // Mirroring in the y axis turns a grain direction `g` into `180 - g`.
            let pg = if mirrored { -pg } else { pg };
            let d = (pg + deg - sg).rem_euclid(180.0);
            d.abs() < 1e-9 || (180.0 - d).abs() < 1e-9
        }
        _ => true,
    }
}

/// Nests `parts` on `stock` (sheets in the given order). See the module documentation. Mirroring
/// is forbidden; [`nest_with`] can allow it.
pub fn nest(
    parts: &[NestPart],
    stock: &[Stock],
    p: &NestParams,
    cancel: &CancellationToken,
) -> Result<NestResult, NestError> {
    nest_with(parts, stock, p, &NestOptions::default(), cancel)
}

/// [`nest`] with options.
pub fn nest_with(
    parts: &[NestPart],
    stock: &[Stock],
    p: &NestParams,
    options: &NestOptions,
    cancel: &CancellationToken,
) -> Result<NestResult, NestError> {
    nest_packed(parts, stock, p, options, Packing::Grid, cancel)
}

/// How closely accepted placements are packed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Packing {
    /// Positions on the grid of `NestParams::step` only: a part occupies whole cells.
    #[default]
    Grid,
    /// After the grid has found a position, slide it against its neighbours with exact geometry.
    Tight,
}

/// What a nest cost, counted rather than timed (deterministic, so tests can bound it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NestWork {
    /// Exact polygon overlays ([`Packing::Tight`] only): the expensive step of a tight nest.
    pub overlays: u64,
}

/// [`nest_with`] with a choice of [`Packing`].
pub fn nest_packed(
    parts: &[NestPart],
    stock: &[Stock],
    p: &NestParams,
    options: &NestOptions,
    packing: Packing,
    cancel: &CancellationToken,
) -> Result<NestResult, NestError> {
    nest_packed_counted(parts, stock, p, options, packing, cancel).map(|(result, _)| result)
}

/// [`nest_packed`] that also reports how much work it did.
pub fn nest_packed_counted(
    parts: &[NestPart],
    stock: &[Stock],
    p: &NestParams,
    options: &NestOptions,
    packing: Packing,
    cancel: &CancellationToken,
) -> Result<(NestResult, NestWork), NestError> {
    let tight = packing == Packing::Tight;
    let mirror_choices: &[bool] = match options.mirror {
        MirrorPolicy::Forbidden => &[false],
        // Unmirrored first: at equal positions the part is not mirrored.
        MirrorPolicy::Allowed => &[false, true],
    };
    if !(p.step.mm() > 0.0) {
        return Err(NestError::InvalidInput { field: "step" });
    }
    if !(p.spacing.mm() >= 0.0) || !(p.edge_margin.mm() >= 0.0) {
        return Err(NestError::InvalidInput { field: "spacing" });
    }
    cancel.check()?;
    let mut sheets: Vec<Sheet<'_>> = stock.iter().map(|s| sheet_with(s, p, tight, cancel)).collect::<Result<_, _>>()?;
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
        for &mirrored in mirror_choices {
            for &deg in part.rotations.degrees() {
                if stock.iter().any(|s| grain_allows(part, s, deg, mirrored)) {
                    shapes.push(rotated_with(part, deg, mirrored, p, tight, cancel)?);
                }
            }
        }
        if shapes.is_empty() {
            unplaced.push(Unplaced { part: part.key, count: part.count, reason: UnplacedReason::NoAllowedRotation });
            continue;
        }
        let mut copy = 0;
        while copy < part.count {
            cancel.check()?;
            let mut found: Option<(usize, &Shape, Point)> = None;
            let mut exhausted = false;
            for (si, sh) in sheets.iter().enumerate() {
                let mut best: Option<(&Shape, Point)> = None;
                for shape in shapes.iter().filter(|s| grain_allows(part, sh.stock, s.rotation_deg, s.mirrored)) {
                    let fit = match &sh.tight {
                        Some(t) => first_fit_tight(t, shape, p, &mut budget, cancel)?,
                        None => first_fit(&sh.blocked, shape, &mut budget, cancel)?.map(|(c, r)| {
                            Point::new(
                                sh.blocked.origin.x + c as f64 * p.step.mm(),
                                sh.blocked.origin.y + r as f64 * p.step.mm(),
                            )
                        }),
                    };
                    match fit {
                        // Lowest row, then lowest column (the order of the grid search).
                        Some(at) => {
                            if best.is_none_or(|(_, b)| at.y.total_cmp(&b.y).then(at.x.total_cmp(&b.x)).is_lt()) {
                                best = Some((shape, at));
                            }
                        }
                        None if budget == 0 => exhausted = true,
                        None => {}
                    }
                }
                if let Some((shape, at)) = best {
                    found = Some((si, shape, at));
                    break;
                }
            }
            let Some((si, shape, at)) = found else {
                let reason = if exhausted { UnplacedReason::Budget } else { UnplacedReason::NoFit };
                unplaced.push(Unplaced { part: part.key, count: part.count - copy, reason });
                break;
            };
            let sh = &mut sheets[si];
            let transform = shape
                .local
                .then(&Affine::translate(at - Point::ORIGIN).map_err(|_| NestError::Geometry { what: "placement" })?);
            match (&mut sh.tight, &shape.tight) {
                (Some(t), Some(ts)) => occupy_exact(t, ts, at, p)?,
                _ => {
                    // The grid position is a whole number of cells from the origin.
                    let step = p.step.mm();
                    let (c, r) = (
                        ((at.x - sh.blocked.origin.x) / step).round() as i64,
                        ((at.y - sh.blocked.origin.y) / step).round() as i64,
                    );
                    for &(dc, dr) in &shape.grown {
                        sh.blocked.set(c + dc, r + dr);
                    }
                }
            }
            if let Some(b) = part.outline.transform(&transform).bounds() {
                sh.placed.push(b);
            }
            placements.push(Placement {
                part: part.key,
                copy,
                stock: sh.stock.id,
                rotation_deg: shape.rotation_deg,
                mirrored: shape.mirrored,
                transform,
            });
            sh.placed_mm2 += area;
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
    let work = NestWork { overlays: sheets.iter().filter_map(|s| s.tight.as_ref()).map(|t| t.overlays.get()).sum() };
    let result = NestResult {
        placements,
        unplaced,
        utilisation: Utilisation { used_mm2, basis: p.basis, denominator_mm2, value },
        stock_use: sheets
            .iter()
            .map(|s| StockUse {
                stock: s.stock.id,
                net_mm2: s.net_area,
                placed_mm2: s.placed_mm2,
                free_mm2: s.net_area - s.placed_mm2,
            })
            .collect(),
    };
    Ok((result, work))
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

/// Total length of the straight edges that placed parts share: edges of two different placed parts
/// that lie within `edge_tolerance_mm` of one common line and overlap along it. Parts nested
/// without spacing and packed tight touch like this, and the shared edge can be cut once
/// (common-line cutting, C12). Outlines are flattened within `tolerance`.
pub fn shared_edge_length_mm(
    parts: &[NestPart],
    result: &NestResult,
    tolerance: &ProductionTolerance,
    edge_tolerance_mm: f64,
) -> f64 {
    // The placed outlines as segments, with their bounds.
    let mut placed: Vec<(Bounds, Vec<(Point, Point)>)> = Vec::new();
    for pl in &result.placements {
        let Some(part) = parts.iter().find(|p| p.key == pl.part) else { continue };
        let moved = part.outline.transform(&pl.transform);
        let Some(bounds) = moved.bounds() else { continue };
        let mut segments = Vec::new();
        for pl in moved.flatten(tolerance) {
            let n = pl.points.len();
            let closed = pl.closed && n > 2;
            for k in 0..n.saturating_sub(1) {
                segments.push((pl.points[k], pl.points[k + 1]));
            }
            if closed && pl.points.first() != pl.points.last() {
                segments.push((pl.points[n - 1], pl.points[0]));
            }
        }
        placed.push((bounds, segments));
    }
    let mut total = 0.0;
    for (i, (bi, si)) in placed.iter().enumerate() {
        for (bj, sj) in &placed[i + 1..] {
            if !bi.expanded(edge_tolerance_mm).intersects(bj) {
                continue;
            }
            for &(a, b) in si {
                for &(c, d) in sj {
                    total += collinear_overlap_mm(a, b, c, d, edge_tolerance_mm);
                }
            }
        }
    }
    total
}

/// Length along which segment `c..d` lies within `tol` of the line through `a..b` and projects
/// onto `a..b`.
fn collinear_overlap_mm(a: Point, b: Point, c: Point, d: Point, tol: f64) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = dx.hypot(dy);
    if len <= 0.0 {
        return 0.0;
    }
    let (ux, uy) = (dx / len, dy / len);
    // Distance of a point from the line, and its position along it.
    let across = |p: Point| ((p.x - a.x) * -uy + (p.y - a.y) * ux).abs();
    let along = |p: Point| (p.x - a.x) * ux + (p.y - a.y) * uy;
    if across(c) > tol || across(d) > tol {
        return 0.0;
    }
    let (t0, t1) = (along(c).min(along(d)), along(c).max(along(d)));
    (t1.min(len) - t0.max(0.0)).max(0.0)
}

/// Slides at most this many steps in one direction in one sweep.
const MAX_SLIDE_STEPS: u32 = 4;
/// Sub-cell offsets tried for each free cell, in the order of a bottom-left search.
const SUB_CELL: [f64; 4] = [0.0, 0.25, 0.5, 0.75];

/// Largest distance between boundary samples of a shape, as a multiple of the deep cell size.
const DEEP_SAMPLE_SPACING: f64 = 1.0;
/// Most cells of a [`DeepMask`].
const DEEP_MAX_CELLS: f64 = 4_000_000.0;

/// Edge of a cell of the [`DeepMask`] (mm): an eighth of the step, never below four production
/// tolerances so that "deep" always means clearly more than the tolerance of the overlay.
fn deep_cell(p: &NestParams) -> f64 {
    (p.step.mm() / 8.0).max(4.0 * p.tolerance.mm())
}

/// Points along the closed contours `polys`: every vertex and enough points on each edge that
/// none lies further than `spacing` from the next one.
fn boundary_samples(polys: &[Vec<Point>], spacing: f64) -> Vec<Point> {
    let mut out = Vec::new();
    for poly in polys {
        let n = poly.len();
        for k in 0..n {
            let (a, b) = (poly[k], poly[(k + 1) % n]);
            let parts = ((a.distance(b) / (spacing * DEEP_SAMPLE_SPACING)).ceil() as usize).max(1);
            for i in 0..parts {
                let t = i as f64 / parts as f64;
                out.push(Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
            }
        }
    }
    out
}

/// A raster of the sheet that marks cells lying *deep* inside the grown outline of a placed part:
/// the cell centre is inside and no boundary of that outline touches the cell or one of its eight neighbours, so
/// every point of the cell is at least one cell edge (at least four tolerances) inside the outline.
///
/// A point on the boundary of a later part that falls in a deep cell lies that far inside a
/// placed part's grown outline, and a boundary point of a part always has part of its interior
/// beside it: the overlap of the two areas is several times the tolerance wide, far more than the
/// "negligible" sliver the exact check allows. So such a candidate is rejected without an overlay;
/// every other candidate is decided by the overlay as before.
struct DeepMask {
    origin: Point,
    cell: f64,
    cols: i64,
    rows: i64,
    cells: Vec<bool>,
}

impl DeepMask {
    fn new(stock: Bounds, cell: f64) -> Result<Self, NestError> {
        // Few enough cells: a very large sheet gets coarser cells (fewer certain rejections).
        let cell = cell.max((stock.width() * stock.height() / DEEP_MAX_CELLS).sqrt());
        let cols = (stock.width() / cell).ceil() as i64 + 1;
        let rows = (stock.height() / cell).ceil() as i64 + 1;
        let n = (cols.max(0) as u64).saturating_mul(rows.max(0) as u64);
        if n > MAX_GRID_CELLS {
            return Err(NestError::GridTooLarge);
        }
        Ok(DeepMask { origin: stock.min(), cell, cols, rows, cells: vec![false; n as usize] })
    }

    fn at(&self, c: i64, r: i64) -> Option<usize> {
        (c >= 0 && r >= 0 && c < self.cols && r < self.rows).then(|| (r * self.cols + c) as usize)
    }

    /// Whether `p` lies in a deep cell.
    fn is_deep(&self, p: Point) -> bool {
        let c = ((p.x - self.origin.x) / self.cell).floor() as i64;
        let r = ((p.y - self.origin.y) / self.cell).floor() as i64;
        self.at(c, r).is_some_and(|i| self.cells[i])
    }

    /// Marks the deep cells of the area bounded by `polys` (a placed part's grown outline).
    fn add(&mut self, polys: &[Vec<Point>]) {
        let (origin, cell) = (self.origin, self.cell);
        // Every cell whose closure the boundary touches, and the cells around those.
        let mut edge: std::collections::HashSet<(i64, i64)> =
            boundary_cells_grown(polys, origin, cell, cell * 1e-6).into_iter().collect();
        let marked: Vec<(i64, i64)> = edge.iter().copied().collect();
        for (c, r) in marked {
            for dr in -1..=1 {
                for dc in -1..=1 {
                    edge.insert((c + dc, r + dr));
                }
            }
        }
        for (c, r) in centre_cells(polys, origin, cell) {
            if !edge.contains(&(c, r))
                && let Some(i) = self.at(c, r)
            {
                self.cells[i] = true;
            }
        }
    }
}

/// Whether an overlay result is empty up to rounding of the overlay grid.
fn negligible(path: &Path) -> bool {
    path.subpaths().iter().filter_map(|sp| sp.signed_area()).map(f64::abs).sum::<f64>() < 1e-9
}

/// The exact check of a tight nest: the shape (pulled in by the slack) at translation `at` lies
/// inside the usable area and clear of the grown outline of every placed part.
fn exact_valid(
    t: &TightSheet,
    ts: &TightShape,
    at: Point,
    p: &NestParams,
    cancel: &CancellationToken,
) -> Result<bool, NestError> {
    let Some(local) = ts.shrunk_bounds else { return Ok(false) };
    let shift = at - Point::ORIGIN;
    let b = Bounds::from_corners(local.min() + shift, local.max() + shift);
    // A boundary point deep inside a placed part's grown outline: a certain overlap, no overlay.
    if ts.samples.iter().any(|&s| t.deep.is_deep(s + shift)) {
        return Ok(false);
    }
    let moved = ts.shrunk.transform(&Affine::translate(shift).map_err(|_| NestError::Geometry { what: "placement" })?);
    let options = BooleanOptions { fill: FillRule::EvenOdd, tolerance: p.tolerance };
    let boolean = |a: &Path, c: &Path, op| {
        t.overlays.set(t.overlays.get() + 1);
        burnburn_geometry::boolean(a, c, op, &options, cancel).map_err(|e| match e {
            burnburn_geometry::BooleanError::Cancelled => NestError::Cancelled,
            _ => NestError::Geometry { what: "exact placement" },
        })
    };
    let inside_rect = t.usable_is_rect
        && t.usable_bounds.is_some_and(|u| {
            b.min().x >= u.min().x && b.min().y >= u.min().y && b.max().x <= u.max().x && b.max().y <= u.max().y
        });
    if !inside_rect && !negligible(&boolean(&moved, &t.usable, BooleanOp::Difference)?) {
        return Ok(false);
    }
    for (other, grown) in &t.placed {
        if other.intersects(&b) && !negligible(&boolean(&moved, grown, BooleanOp::Intersection)?) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Bottom-left first position of `shape` on a tight sheet: the optimistic raster proposes cells,
/// the exact check confirms a position inside the cell, and the result slides down and to the
/// left against what is there until it touches (within the slack).
fn first_fit_tight(
    t: &TightSheet,
    shape: &Shape,
    p: &NestParams,
    budget: &mut u64,
    cancel: &CancellationToken,
) -> Result<Option<Point>, NestError> {
    let Some(ts) = &shape.tight else { return Ok(None) };
    let grid = grid_fit_tight(t, ts, p, budget, cancel)?;
    let contact = contact_fit_tight(t, ts, grid, p, cancel)?;
    // The lower row wins, then the lower column.
    Ok(match (grid, contact) {
        (Some(a), Some(b)) => Some(if b.y.total_cmp(&a.y).then(b.x.total_cmp(&a.x)).is_lt() { b } else { a }),
        (a, b) => a.or(b),
    })
}

/// Most contact positions tried for one part.
const CONTACT_LIMIT: usize = 1024;

/// Positions where the part touches the usable area or a placed part: the part's lower left corner
/// at an edge of the usable area or at the far side of a neighbour's grown outline. The raster's
/// cells cannot propose these (a part that fits exactly into what is left of a row), so they are
/// tried in addition, lower rows first, and only those not worse than `known`.
fn contact_fit_tight(
    t: &TightSheet,
    ts: &TightShape,
    known: Option<Point>,
    p: &NestParams,
    cancel: &CancellationToken,
) -> Result<Option<Point>, NestError> {
    let Some(usable) = t.usable_bounds else { return Ok(None) };
    let mut xs = vec![usable.min().x];
    let mut ys = vec![usable.min().y];
    for (b, _) in &t.placed {
        xs.push(b.max().x);
        ys.push(b.max().y);
    }
    for v in [&mut xs, &mut ys] {
        v.sort_by(f64::total_cmp);
        v.dedup();
    }
    let slack = slack_mm(p);
    let fits = |x: f64, y: f64| x + ts.size.0 <= usable.max().x + slack && y + ts.size.1 <= usable.max().y + slack;
    let mut tried = 0;
    for &y in &ys {
        if known.is_some_and(|k| y > k.y + slack) {
            break;
        }
        for &x in &xs {
            if !fits(x, y) {
                continue;
            }
            tried += 1;
            if tried > CONTACT_LIMIT {
                return Ok(None);
            }
            cancel.check()?;
            let at = Point::new(x, y);
            if exact_valid(t, ts, at, p, cancel)? {
                return slide(t, ts, at, p, cancel).map(Some);
            }
        }
    }
    Ok(None)
}

/// The raster's bottom-left position (see [`first_fit_tight`]).
fn grid_fit_tight(
    t: &TightSheet,
    ts: &TightShape,
    p: &NestParams,
    budget: &mut u64,
    cancel: &CancellationToken,
) -> Result<Option<Point>, NestError> {
    let (grid, step) = (&t.blocked, p.step.mm());
    for r in 0..grid.rows {
        cancel.check()?;
        for c in 0..grid.cols {
            if *budget == 0 {
                return Ok(None);
            }
            *budget -= 1;
            let free = ts.raw_centres.iter().all(|&(dc, dr)| {
                let (x, y) = (c + dc, r + dr);
                x >= 0 && y >= 0 && x < grid.cols && y < grid.rows && !grid.get(x, y)
            });
            if !free {
                continue;
            }
            for fy in SUB_CELL {
                for fx in SUB_CELL {
                    if *budget == 0 {
                        return Ok(None);
                    }
                    *budget -= 1;
                    let at = Point::new(grid.origin.x + (c as f64 + fx) * step, grid.origin.y + (r as f64 + fy) * step);
                    if exact_valid(t, ts, at, p, cancel)? {
                        return slide(t, ts, at, p, cancel).map(Some);
                    }
                }
            }
        }
    }
    Ok(None)
}

/// Moves a valid position down, then left, as far as it stays valid (bisection down to the
/// slack), until a sweep moves nothing.
fn slide(
    t: &TightSheet,
    ts: &TightShape,
    mut at: Point,
    p: &NestParams,
    cancel: &CancellationToken,
) -> Result<Point, NestError> {
    let (step, slack) = (p.step.mm(), slack_mm(p));
    for _ in 0..3 {
        let before = at;
        for dir in [Vector::new(0.0, -1.0), Vector::new(-1.0, 0.0)] {
            for _ in 0..MAX_SLIDE_STEPS {
                let far = at + dir * step;
                if exact_valid(t, ts, far, p, cancel)? {
                    at = far;
                    continue;
                }
                let (mut ok, mut bad) = (0.0, step);
                while bad - ok > slack {
                    let mid = 0.5 * (ok + bad);
                    if exact_valid(t, ts, at + dir * mid, p, cancel)? {
                        ok = mid;
                    } else {
                        bad = mid;
                    }
                }
                at = at + dir * ok;
                break;
            }
        }
        if before.distance(at) <= slack {
            break;
        }
    }
    Ok(at)
}

/// Marks a tight placement at `at`: the grown outline for later exact checks and the cells whose
/// centre it covers for the optimistic raster.
fn occupy_exact(t: &mut TightSheet, ts: &TightShape, at: Point, p: &NestParams) -> Result<(), NestError> {
    let grown = ts
        .grown
        .transform(&Affine::translate(at - Point::ORIGIN).map_err(|_| NestError::Geometry { what: "placement" })?);
    if let Some(b) = grown.bounds() {
        let polys = polygons(&grown, &p.tolerance)?;
        for (c, r) in centre_cells(&polys, t.blocked.origin, p.step.mm()) {
            t.blocked.set(c, r);
        }
        t.deep.add(&polys);
        t.placed.push((b, grown));
    }
    Ok(())
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

#[cfg(test)]
mod cancel_tests {
    //! Cancellation inside the geometry steps of a nest (G5-cam): a token that is cancelled while
    //! the spacing offset, keep-out boolean or edge-margin offset runs must give `Cancelled`, never
    //! a geometry error. Deterministic: the token is cancelled before the step is entered.
    use super::*;
    use burnburn_geometry::Rect;
    use burnburn_types::CancellationSource;

    fn mm(v: f64) -> Length {
        Length::from_mm(v).unwrap()
    }

    fn square(size: f64) -> Path {
        Path::rectangle(Rect::new(Point::ORIGIN, mm(size), mm(size)).unwrap())
    }

    fn params(spacing: f64, margin: f64) -> NestParams {
        NestParams {
            spacing: mm(spacing),
            edge_margin: mm(margin),
            step: mm(1.0),
            tolerance: ProductionTolerance::new(mm(0.01)).unwrap(),
            max_candidates: u64::MAX,
            basis: UtilisationBasis::StockArea,
        }
    }

    fn cancelled() -> CancellationToken {
        let source = CancellationSource::new();
        source.cancel();
        source.token()
    }

    #[test]
    fn a_cancel_during_the_spacing_offset_is_cancelled_not_a_geometry_error() {
        let part = NestPart {
            key: PartKey(1),
            outline: square(10.0),
            count: 1,
            rotations: RotationSet::new(vec![0.0]).unwrap(),
            grain_deg: None,
        };
        let r = rotated(&part, 0.0, false, &params(1.0, 0.0), &cancelled());
        assert!(matches!(r, Err(NestError::Cancelled)), "{:?}", r.map(|_| ()));
    }

    #[test]
    fn a_cancel_during_keep_out_and_edge_margin_is_cancelled_too() {
        let stock = Stock {
            id: StockId(1),
            outline: square(100.0),
            keep_out: vec![Path::rectangle(Rect::new(Point::new(10.0, 10.0), mm(5.0), mm(5.0)).unwrap())],
            grain_deg: None,
        };
        let r = sheet(&stock, &params(0.0, 0.0), &cancelled());
        assert!(matches!(r, Err(NestError::Cancelled)), "keep-out: {:?}", r.map(|_| ()));
        let plain = Stock { keep_out: vec![], ..stock };
        let r = sheet(&plain, &params(0.0, 2.0), &cancelled());
        assert!(matches!(r, Err(NestError::Cancelled)), "edge margin: {:?}", r.map(|_| ()));
    }
}
