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
