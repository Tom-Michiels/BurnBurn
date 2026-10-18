//! Booleans and offsets of closed areas: our types in, our types out. `i_overlay` (ADR 0007) is
//! an implementation detail of this module and never appears in the public API.

use std::fmt;

use burnburn_types::CancellationToken;

use std::f64::consts::PI;

use i_overlay::core::fill_rule::FillRule as IFill;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::scale::FixedScaleFloatOverlay;
use i_overlay::mesh::float::outline::offset::OutlineOffset;
use i_overlay::mesh::float::style::{LineJoin, OutlineStyle};

use crate::flatten::flatten_subpath;
use crate::math;
use crate::{Bounds, Path, Point, ProductionTolerance, Segment, Subpath};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BooleanOp {
    Union,
    /// Subject minus clip.
    Difference,
    Intersection,
    Xor,
}

/// How overlapping contours of one operand define its area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FillRule {
    /// Alternating inside/outside (holes need no particular winding). Default for laser work.
    EvenOdd,
    /// Winding number ≠ 0 (holes must wind opposite to their outer contour).
    NonZero,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BooleanOptions {
    pub fill: FillRule,
    /// Chord deviation for flattening curves before the overlay. Output is polygonal.
    pub tolerance: ProductionTolerance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BooleanError {
    /// Booleans need closed contours; `operand` 0 is the subject, 1 the clip.
    OpenContour {
        operand: u8,
        subpath: usize,
    },
    /// Coordinates exceed the overlay's integer range at the chosen precision.
    OutOfRange,
    Cancelled,
}

impl fmt::Display for BooleanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BooleanError::OpenContour { operand, subpath } => {
                write!(f, "operand {operand} has an open contour (subpath {subpath})")
            }
            BooleanError::OutOfRange => f.write_str("coordinates are out of range for the boolean precision"),
            BooleanError::Cancelled => f.write_str("boolean cancelled"),
        }
    }
}

impl std::error::Error for BooleanError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OffsetJoin {
    Miter,
    Round,
    Bevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum OffsetError {
    NotFinite,
    OpenContour {
        subpath: usize,
    },
    /// An inward offset removes the whole area; nothing is returned.
    Collapsed,
    OutOfRange,
    Cancelled,
}

impl fmt::Display for OffsetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OffsetError::NotFinite => f.write_str("offset distance must be finite"),
            OffsetError::OpenContour { subpath } => write!(f, "offset needs closed contours (subpath {subpath})"),
            OffsetError::Collapsed => f.write_str("the inward offset removes the whole shape"),
            OffsetError::OutOfRange => f.write_str("coordinates are out of range for the offset precision"),
            OffsetError::Cancelled => f.write_str("offset cancelled"),
        }
    }
}

impl std::error::Error for OffsetError {}

type Contour = Vec<[f64; 2]>;

/// Overlay grid relative to the production tolerance: rounding to the grid moves a vertex by at
/// most `tolerance / GRID_DIVISOR`, far below what the tolerance allows.
const GRID_DIVISOR: f64 = 1000.0;
/// `i_overlay`'s conservative coordinate budget for the i32 engine (`I::BITS − 3`), minus a bit of headroom.
const I32_BUDGET: f64 = (1u64 << 28) as f64;

/// Flattens closed subpaths to contours relative to `origin`. Open subpaths are an error.
fn contours(path: &Path, origin: Point, dev: f64, operand: u8) -> Result<Vec<Contour>, BooleanError> {
    let mut out = Vec::with_capacity(path.subpaths().len());
    for (si, sp) in path.subpaths().iter().enumerate() {
        if !sp.is_closed() {
            return Err(BooleanError::OpenContour { operand, subpath: si });
        }
        let poly = flatten_subpath(sp, dev);
        let mut pts: Contour = poly.points.iter().map(|p| [p.x - origin.x, p.y - origin.y]).collect();
        if pts.len() > 1 && pts.first() == pts.last() {
            pts.pop();
        }
        if pts.len() >= 3 {
            out.push(pts);
        }
    }
    Ok(out)
}

fn to_path(shapes: Vec<Vec<Contour>>, origin: Point) -> Path {
    let mut subpaths = Vec::new();
    for shape in shapes {
        for contour in shape {
            let mut pts = contour.iter().map(|&[x, y]| Point::new(x + origin.x, y + origin.y));
            let Some(start) = pts.next() else { continue };
            let segments: Vec<Segment> = pts.map(|to| Segment::Line { to }).collect();
            if segments.len() < 2 {
                continue;
            }
            // Coordinates come from finite inputs through a finite scale.
            if let Ok(sp) = Subpath::new(start, segments, true) {
                subpaths.push(sp);
            }
        }
    }
    Path::from_subpaths(subpaths)
}

fn center_of(paths: &[&Path]) -> Option<Bounds> {
    paths.iter().filter_map(|p| p.bounds()).reduce(Bounds::union)
}

fn fill_rule(f: FillRule) -> IFill {
    match f {
        FillRule::EvenOdd => IFill::EvenOdd,
        FillRule::NonZero => IFill::NonZero,
    }
}

/// `subject op clip`. Curves are flattened within `options.tolerance`; the result is polygonal,
/// outer contours counter-clockwise and holes clockwise. Deterministic for identical input.
///
/// Cancellation is checked before and after the overlay; the overlay call itself is not
/// interruptible (bounded by the input size).
pub fn boolean(
    subject: &Path,
    clip: &Path,
    op: BooleanOp,
    options: &BooleanOptions,
    cancel: &CancellationToken,
) -> Result<Path, BooleanError> {
    cancel.check().map_err(|_| BooleanError::Cancelled)?;
    let tol = options.tolerance.mm();
    let origin = center_of(&[subject, clip]).map_or(Point::ORIGIN, |b| b.center());
    let (s, c) = (contours(subject, origin, tol, 0)?, contours(clip, origin, tol, 1)?);
    if s.is_empty() && c.is_empty() {
        return Ok(Path::default());
    }
    let rule = match op {
        BooleanOp::Union => OverlayRule::Union,
        BooleanOp::Difference => OverlayRule::Difference,
        BooleanOp::Intersection => OverlayRule::Intersect,
        BooleanOp::Xor => OverlayRule::Xor,
    };
    let scale = GRID_DIVISOR / tol;
    let shapes = s
        .overlay_with_fixed_scale_as::<i64>(&c, rule, fill_rule(options.fill), scale)
        .map_err(|_| BooleanError::OutOfRange)?;
    cancel.check().map_err(|_| BooleanError::Cancelled)?;
    Ok(to_path(shapes, origin))
}

/// Offsets closed contours by `distance` (positive grows the area). Contours are first
/// normalised (even-odd) so their windings are valid for the offset. Polygonal output.
pub fn offset(
    path: &Path,
    distance: f64,
    join: OffsetJoin,
    tolerance: &ProductionTolerance,
    cancel: &CancellationToken,
) -> Result<Path, OffsetError> {
    if !distance.is_finite() {
        return Err(OffsetError::NotFinite);
    }
    cancel.check().map_err(|_| OffsetError::Cancelled)?;
    let tol = tolerance.mm();
    let Some(bounds) = path.bounds() else { return Ok(Path::default()) };
    // Every area lies inside its bounds, so its inradius is at most half the smaller side.
    if distance < 0.0 && -2.0 * distance >= bounds.width().min(bounds.height()) {
        return Err(OffsetError::Collapsed);
    }
    let origin = bounds.center();
    let src = contours(path, origin, tol, 0).map_err(|e| match e {
        BooleanError::OpenContour { subpath, .. } => OffsetError::OpenContour { subpath },
        _ => OffsetError::OutOfRange,
    })?;
    // The offset engine is i32: pick the finest grid that fits, and refuse a grid coarser than
    // the tolerance allows.
    let reach = 0.5 * bounds.width().max(bounds.height()) + distance.abs() + tol;
    let scale = (GRID_DIVISOR / tol).min(I32_BUDGET / reach.max(tol));
    if !(scale * tol >= 10.0) {
        return Err(OffsetError::OutOfRange);
    }
    let empty: Vec<Contour> = Vec::new();
    let normal = src
        .overlay_with_fixed_scale_as::<i64>(&empty, OverlayRule::Subject, IFill::EvenOdd, scale)
        .map_err(|_| OffsetError::OutOfRange)?;
    cancel.check().map_err(|_| OffsetError::Cancelled)?;
    let r = distance.abs();
    let style = OutlineStyle::new(distance).line_join(match join {
        OffsetJoin::Miter => LineJoin::Miter(0.01 * PI),
        OffsetJoin::Bevel => LineJoin::Bevel,
        // Segment angle whose chord sagitta r·(1 − cos(θ/2)) equals the tolerance.
        OffsetJoin::Round => LineJoin::Round(if tol >= r { PI / 2.0 } else { 2.0 * math::acos(1.0 - tol / r) }),
    });
    let shapes = normal.outline_fixed_scale(&style, scale).map_err(|_| OffsetError::OutOfRange)?;
    cancel.check().map_err(|_| OffsetError::Cancelled)?;
    let out = to_path(shapes, origin);
    if out.is_empty() && !normal.is_empty() {
        return Err(OffsetError::Collapsed);
    }
    Ok(out)
}
