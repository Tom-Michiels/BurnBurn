//! Parametric drawing shapes resolved to exact paths (E01).

use std::f64::consts::{FRAC_PI_2, TAU};

use burnburn_types::{Angle, Length};

use crate::math;
use crate::{Affine, EllipticalArc, GeometryError, Path, PathBuilder, Point, Rect, Vector};

/// A drawing primitive as data (what a tool commits). `to_path` resolves it to world mm.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Shape {
    Line {
        from: Point,
        to: Point,
    },
    Rectangle(Rect),
    Circle {
        center: Point,
        radius: Length,
    },
    /// Semi-axes `rx`, `ry` before rotating counter-clockwise by `rotation`.
    Ellipse {
        center: Point,
        rx: Length,
        ry: Length,
        rotation: Angle,
    },
    /// A regular polygon with `sides` vertices on a circle of `radius`; the first vertex at `rotation`.
    Polygon {
        center: Point,
        radius: Length,
        sides: u32,
        rotation: Angle,
    },
    /// `points` tips on `outer`, alternating with valleys on `inner`; the first tip at `rotation`.
    Star {
        center: Point,
        outer: Length,
        inner: Length,
        points: u32,
        rotation: Angle,
    },
    Polyline {
        points: Vec<Point>,
        closed: bool,
    },
}

impl Shape {
    pub fn to_path(&self) -> Result<Path, GeometryError> {
        match self {
            Shape::Line { from, to } => Path::line(*from, *to),
            Shape::Rectangle(r) => Ok(Path::rectangle(*r)),
            Shape::Circle { center, radius } => Path::circle(*center, radius.mm()),
            Shape::Ellipse { center, rx, ry, rotation } => Path::ellipse(*center, rx.mm(), ry.mm(), *rotation),
            Shape::Polygon { center, radius, sides, rotation } => {
                Path::regular_polygon(*center, radius.mm(), *sides, *rotation)
            }
            Shape::Star { center, outer, inner, points, rotation } => {
                Path::star(*center, outer.mm(), inner.mm(), *points, *rotation)
            }
            Shape::Polyline { points, closed } => Path::polyline(points, *closed),
        }
    }
}

fn positive(v: f64) -> Result<f64, GeometryError> {
    if !v.is_finite() {
        Err(GeometryError::NotFinite)
    } else if v > 0.0 {
        Ok(v)
    } else {
        Err(GeometryError::DegenerateArc)
    }
}

impl Path {
    /// A full circle as one closed subpath of four quarter arcs, counter-clockwise from angle 0.
    pub fn circle(center: Point, radius: f64) -> Result<Path, GeometryError> {
        Path::ellipse(center, radius, radius, Angle::ZERO)
    }

    /// A full ellipse as four quarter arcs (exact), counter-clockwise, starting on the rotated +x semi-axis.
    pub fn ellipse(center: Point, rx: f64, ry: f64, rotation: Angle) -> Result<Path, GeometryError> {
        let (rx, ry) = (positive(rx)?, positive(ry)?);
        if !center.is_finite() {
            return Err(GeometryError::NotFinite);
        }
        let rot = Affine::rotate(rotation);
        let (u, v) = (rot.apply_vector(Vector::new(rx, 0.0)), rot.apply_vector(Vector::new(0.0, ry)));
        let quarter = Angle::from_radians(FRAC_PI_2).map_err(|_| GeometryError::NotFinite)?;
        let mut b = PathBuilder::new().move_to(center + u);
        for i in 0..4 {
            let start = Angle::from_radians(FRAC_PI_2 * f64::from(i)).map_err(|_| GeometryError::NotFinite)?;
            b = b.arc(EllipticalArc::new(center, u, v, start, quarter)?);
        }
        b.close().build()
    }

    /// A closed regular polygon (counter-clockwise).
    pub fn regular_polygon(center: Point, radius: f64, sides: u32, rotation: Angle) -> Result<Path, GeometryError> {
        if sides < 3 {
            return Err(GeometryError::TooFewPoints);
        }
        let r = positive(radius)?;
        let pts: Vec<Point> = (0..sides)
            .map(|i| {
                let a = rotation.radians() + TAU * f64::from(i) / f64::from(sides);
                center + Vector::new(r * math::cos(a), r * math::sin(a))
            })
            .collect();
        Path::polyline(&pts, true)
    }

    /// A closed star with `points` tips (counter-clockwise).
    pub fn star(center: Point, outer: f64, inner: f64, points: u32, rotation: Angle) -> Result<Path, GeometryError> {
        if points < 2 {
            return Err(GeometryError::TooFewPoints);
        }
        let (ro, ri) = (positive(outer)?, positive(inner)?);
        let n = 2 * points;
        let pts: Vec<Point> = (0..n)
            .map(|i| {
                let a = rotation.radians() + TAU * f64::from(i) / f64::from(n);
                let r = if i % 2 == 0 { ro } else { ri };
                center + Vector::new(r * math::cos(a), r * math::sin(a))
            })
            .collect();
        Path::polyline(&pts, true)
    }

    /// Straight lines through `points` (at least two), optionally closed.
    pub fn polyline(points: &[Point], closed: bool) -> Result<Path, GeometryError> {
        let (first, rest) = points.split_first().ok_or(GeometryError::TooFewPoints)?;
        if rest.is_empty() {
            return Err(GeometryError::TooFewPoints);
        }
        let mut b = PathBuilder::new().move_to(*first);
        for p in rest {
            b = b.line_to(*p);
        }
        if closed { b.close().build() } else { b.build() }
    }
}
