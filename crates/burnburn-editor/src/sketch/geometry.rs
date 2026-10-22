//! Where a sketch is: the last valid solution when there is one, else the drawn positions.

use burnburn_document::sketch::{Entity, EntityId, Sketch};
use burnburn_geometry::Point;

/// Positions of a sketch's entities in sketch-local (= document) millimetres.
#[derive(Debug, Clone, Copy)]
pub struct SketchGeometry<'a> {
    sketch: &'a Sketch,
}

impl<'a> SketchGeometry<'a> {
    pub fn new(sketch: &'a Sketch) -> Self {
        SketchGeometry { sketch }
    }

    pub fn sketch(&self) -> &'a Sketch {
        self.sketch
    }

    pub fn point(&self, id: EntityId) -> Option<Point> {
        match self.sketch.entity(id)?.entity {
            Entity::Point { at } => Some(self.sketch.solution().and_then(|s| s.points.get(&id).copied()).unwrap_or(at)),
            _ => None,
        }
    }

    pub fn line(&self, id: EntityId) -> Option<(Point, Point)> {
        match self.sketch.entity(id)?.entity {
            Entity::Line { start, end } => Some((self.point(start)?, self.point(end)?)),
            _ => None,
        }
    }

    /// Centre, major-axis end and semi-minor radius of an ellipse.
    pub fn ellipse(&self, id: EntityId) -> Option<(Point, Point, f64)> {
        match self.sketch.entity(id)?.entity {
            Entity::Ellipse { center, major, minor } => Some((self.point(center)?, self.point(major)?, minor)),
            _ => None,
        }
    }

    /// Centre and radius of a circle, or of the circle an arc lies on.
    pub fn circle(&self, id: EntityId) -> Option<(Point, f64)> {
        match self.sketch.entity(id)?.entity {
            Entity::Circle { center, radius } => {
                let r = self.sketch.solution().and_then(|s| s.radii.get(&id).copied()).unwrap_or(radius);
                Some((self.point(center)?, r))
            }
            Entity::Arc { center, start, .. } => {
                let c = self.point(center)?;
                Some((c, c.distance(self.point(start)?)))
            }
            _ => None,
        }
    }
}

/// How far `at` is from the outline of the ellipse (centre, major-axis end, semi-minor radius): the
/// first-order (Sampson) estimate `|f - 1| * f / |grad f|` with `f = sqrt((x/a)^2 + (y/b)^2)` in the
/// ellipse's own frame. Exact on the outline, within a few percent at pick distances. `None` for a
/// degenerate ellipse.
pub(crate) fn ellipse_distance(center: Point, major: Point, minor: f64, at: Point) -> Option<f64> {
    let axis = major - center;
    let a = axis.length();
    if a <= 0.0 || minor <= 0.0 || !a.is_finite() || !minor.is_finite() {
        return None;
    }
    let (u, v) = (axis * (1.0 / a), burnburn_geometry::Vector::new(-axis.y / a, axis.x / a));
    let d = at - center;
    let (x, y) = (d.dot(u), d.dot(v));
    let f = ((x / a).powi(2) + (y / minor).powi(2)).sqrt();
    let grad = ((x / (a * a)).powi(2) + (y / (minor * minor)).powi(2)).sqrt();
    if grad <= 0.0 {
        return Some(a.min(minor)); // the centre
    }
    Some((f - 1.0).abs() * f / grad)
}
