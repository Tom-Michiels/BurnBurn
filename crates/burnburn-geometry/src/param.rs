//! Parametric basic shapes (W1-L2b): regular polygon, star, double star and rounded rectangle.
//! A [`ParametricShape`] is its parameters plus a placement; it resolves to an exact [`Path`]
//! and stays editable until it is converted to a plain path.

use std::f64::consts::{FRAC_PI_2, PI, TAU};
use std::fmt;

use burnburn_types::Angle;

use crate::math;
use crate::{Affine, Bounds, EllipticalArc, GeometryError, Path, PathBuilder, Point};

/// Largest accepted number of polygon sides / star tips (guards file input and typed values).
pub const MAX_SIDES: u32 = 1000;

/// What a basic shape is, independent of where it sits. Polygon and star shapes live on the
/// unit circle (first vertex at +y); the placement scales and moves that circle. A rounded
/// rectangle is `width × height` millimetres centred on the origin of its placement.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum ShapeParams {
    /// Regular polygon: 3 (triangle), 4, 5 (pentagon), 8 (octagon), … vertices.
    Polygon { sides: u32 },
    /// `points` tips on the circle alternating with valleys on `ratio` times that radius, `0 < ratio < 1`.
    Star { points: u32, ratio: f64 },
    /// Two such stars, the second turned half a step (`π / points`): two closed subpaths.
    DoubleStar { points: u32, ratio: f64 },
    /// Corner `radius` between 0 (sharp) and half the shorter side (stadium).
    RoundedRectangle { width: f64, height: f64, radius: f64 },
}

/// Why parameters or a placement were rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ShapeParamError {
    TooFewSides,
    TooFewPoints,
    TooMany,
    RatioOutOfRange,
    NonPositiveSize,
    RadiusOutOfRange,
    Geometry(GeometryError),
}

impl fmt::Display for ShapeParamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ShapeParamError {}

/// A basic shape as data: parameters plus the placement (an affine map from the parameter space
/// to world millimetres). Every affine edit of the placement keeps the shape parametric where
/// the family allows it ([`ParametricShape::transformed`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParametricShape {
    params: ShapeParams,
    frame: Affine,
}

const UNIT_SNAP: f64 = 1e-12;
const SHEAR_TOLERANCE: f64 = 1e-9;

fn angle(rad: f64) -> Result<Angle, GeometryError> {
    Angle::from_radians(rad).map_err(|_| GeometryError::NotFinite)
}

fn on_circle(radius: f64, rad: f64) -> Point {
    let (s, c) = math::sin_cos(rad);
    Point::new(radius * c, radius * s)
}

/// The vertex rings of a polygon-family shape on the unit circle; empty for a rounded rectangle.
fn rings(params: &ShapeParams) -> Vec<Vec<Point>> {
    let star = |points: u32, ratio: f64, turn: f64| -> Vec<Point> {
        let n = 2 * points;
        (0..n)
            .map(|i| {
                let a = FRAC_PI_2 + turn + TAU * f64::from(i) / f64::from(n);
                on_circle(if i % 2 == 0 { 1.0 } else { ratio }, a)
            })
            .collect()
    };
    match *params {
        ShapeParams::Polygon { sides } => {
            vec![(0..sides).map(|i| on_circle(1.0, FRAC_PI_2 + TAU * f64::from(i) / f64::from(sides))).collect()]
        }
        ShapeParams::Star { points, ratio } => vec![star(points, ratio, 0.0)],
        ShapeParams::DoubleStar { points, ratio } => {
            vec![star(points, ratio, 0.0), star(points, ratio, PI / f64::from(points))]
        }
        ShapeParams::RoundedRectangle { .. } => Vec::new(),
    }
}

fn ring_bounds(rings: &[Vec<Point>]) -> Option<Bounds> {
    let mut all = rings.iter().flatten();
    let mut b = Bounds::from_point(*all.next()?);
    all.for_each(|p| b.include(*p));
    Some(b)
}

fn rounded_rectangle(width: f64, height: f64, radius: f64) -> Result<Path, GeometryError> {
    let (hx, hy, r) = (width / 2.0, height / 2.0, radius);
    if r == 0.0 {
        let corners = [Point::new(-hx, -hy), Point::new(hx, -hy), Point::new(hx, hy), Point::new(-hx, hy)];
        return Path::polyline(&corners, true);
    }
    // Counter-clockwise from the bottom edge: edge, corner arc, edge, … An edge the radius
    // consumes entirely (a stadium) is left out.
    let (x_edge, y_edge) = (width - 2.0 * r > 0.0, height - 2.0 * r > 0.0);
    let arc = |center: Point, start: f64| -> Result<EllipticalArc, GeometryError> {
        EllipticalArc::circle(center, r, angle(start)?, angle(FRAC_PI_2)?)
    };
    let mut b = PathBuilder::new().move_to(Point::new(-hx + r, -hy));
    if x_edge {
        b = b.line_to(Point::new(hx - r, -hy));
    }
    b = b.arc(arc(Point::new(hx - r, -hy + r), -FRAC_PI_2)?);
    if y_edge {
        b = b.line_to(Point::new(hx, hy - r));
    }
    b = b.arc(arc(Point::new(hx - r, hy - r), 0.0)?);
    if x_edge {
        b = b.line_to(Point::new(-hx + r, hy));
    }
    b = b.arc(arc(Point::new(-hx + r, hy - r), FRAC_PI_2)?);
    if y_edge {
        b = b.line_to(Point::new(-hx, -hy + r));
    }
    b = b.arc(arc(Point::new(-hx + r, -hy + r), PI)?);
    b.close().build()
}

impl ShapeParams {
    pub fn validate(&self) -> Result<(), ShapeParamError> {
        let count = |n: u32, few: ShapeParamError| match n {
            n if n < 3 => Err(few),
            n if n > MAX_SIDES => Err(ShapeParamError::TooMany),
            _ => Ok(()),
        };
        let ratio_ok = |r: f64| if r > 0.0 && r < 1.0 { Ok(()) } else { Err(ShapeParamError::RatioOutOfRange) };
        match *self {
            ShapeParams::Polygon { sides } => count(sides, ShapeParamError::TooFewSides),
            ShapeParams::Star { points, ratio } | ShapeParams::DoubleStar { points, ratio } => {
                count(points, ShapeParamError::TooFewPoints)?;
                ratio_ok(ratio)
            }
            ShapeParams::RoundedRectangle { width, height, radius } => {
                if !(width.is_finite() && width > 0.0 && height.is_finite() && height > 0.0) {
                    return Err(ShapeParamError::NonPositiveSize);
                }
                if radius.is_finite() && radius >= 0.0 && radius <= width.min(height) / 2.0 {
                    Ok(())
                } else {
                    Err(ShapeParamError::RadiusOutOfRange)
                }
            }
        }
    }

    /// Size of the unplaced shape (width, height): the bounds of the unit-circle shape, or the
    /// rectangle itself. Equal-sided drawing (Shift) uses this aspect ratio.
    pub fn natural_size(&self) -> (f64, f64) {
        match *self {
            ShapeParams::RoundedRectangle { width, height, .. } => (width, height),
            _ => ring_bounds(&rings(self)).map_or((0.0, 0.0), |b| (b.width(), b.height())),
        }
    }
}

impl ParametricShape {
    pub fn new(params: ShapeParams, frame: Affine) -> Result<Self, ShapeParamError> {
        params.validate()?;
        frame.inverse().map_err(ShapeParamError::Geometry)?;
        Ok(ParametricShape { params, frame })
    }

    /// The shape stretched so that its outline fills exactly `bounds` (the drawing gesture).
    /// A rounded rectangle takes the size of `bounds` (its own width and height are ignored); its
    /// radius is kept, clamped to fit.
    pub fn fit(params: ShapeParams, bounds: Bounds) -> Result<Self, ShapeParamError> {
        let (bw, bh) = (bounds.width(), bounds.height());
        if !(bw > 0.0 && bh > 0.0) {
            return Err(ShapeParamError::NonPositiveSize);
        }
        let to = |m: Result<Affine, GeometryError>| m.map_err(ShapeParamError::Geometry);
        if let ShapeParams::RoundedRectangle { radius, .. } = params {
            if !(radius.is_finite() && radius >= 0.0) {
                return Err(ShapeParamError::RadiusOutOfRange);
            }
            let params = ShapeParams::RoundedRectangle {
                width: bw,
                height: bh,
                radius: radius.min(bw.min(bh) / 2.0),
            };
            let center = to(Affine::translate(bounds.center().to_vector()))?;
            return Self::new(params, center);
        }
        params.validate()?;
        let local = ring_bounds(&rings(&params)).ok_or(ShapeParamError::NonPositiveSize)?;
        let frame = to(Affine::translate(-local.min().to_vector()))?
            .then(&to(Affine::scale(bw / local.width(), bh / local.height()))?)
            .then(&to(Affine::translate(bounds.min().to_vector()))?);
        Self::new(params, frame)
    }

    pub fn params(&self) -> &ShapeParams {
        &self.params
    }

    /// Parameter space → world millimetres.
    pub fn frame(&self) -> &Affine {
        &self.frame
    }

    /// The same placement with other parameters (the panel edited sides, ratio or radius).
    pub fn with_params(&self, params: ShapeParams) -> Result<Self, ShapeParamError> {
        Self::new(params, self.frame)
    }

    /// The exact outline in world millimetres: counter-clockwise unless the placement mirrors.
    pub fn to_path(&self) -> Result<Path, GeometryError> {
        let local = match self.params {
            ShapeParams::RoundedRectangle { width, height, radius } => rounded_rectangle(width, height, radius)?,
            _ => Path::from_subpaths(
                rings(&self.params)
                    .iter()
                    .map(|ring| Path::polyline(ring, true).map(|p| p.subpaths().to_vec()))
                    .collect::<Result<Vec<_>, _>>()?
                    .into_iter()
                    .flatten()
                    .collect(),
            ),
        };
        Ok(local.transform(&self.frame))
    }

    /// This shape after an affine edit `m` (move, rotate, scale, mirror), still parametric, or
    /// `None` when the result is no longer a member of the family: a collapsing map, or a shear
    /// of a rounded rectangle. Polygon, star and double star are closed under every invertible
    /// map; a rounded rectangle takes rotations, mirrors and axis scales (width and height
    /// follow the scale, the radius follows the smaller factor, clamped to fit).
    pub fn transformed(&self, m: &Affine) -> Option<ParametricShape> {
        let frame = self.frame.then(m);
        frame.inverse().ok()?;
        let ShapeParams::RoundedRectangle { width, height, radius } = self.params else {
            return Some(ParametricShape { params: self.params, frame });
        };
        let [a, b, c, d, e, f] = frame.coefficients();
        let (sx, sy) = (math::hypot(a, b), math::hypot(c, d));
        if !(sx > 0.0 && sy > 0.0) || (a * c + b * d).abs() > SHEAR_TOLERANCE * sx * sy {
            return None;
        }
        let snap = |s: f64| if (s - 1.0).abs() < UNIT_SNAP { 1.0 } else { s };
        let (sx, sy) = (snap(sx), snap(sy));
        let (width, height) = (width * sx, height * sy);
        let params = ShapeParams::RoundedRectangle {
            width,
            height,
            radius: (radius * sx.min(sy)).min(width.min(height) / 2.0),
        };
        let frame = Affine::new([a / sx, b / sx, c / sy, d / sy, e, f]).ok()?;
        Some(ParametricShape { params, frame })
    }
}
