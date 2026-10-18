use crate::math;
use crate::{Point, Vector};

/// An axis-aligned bounding box in canonical millimetres. Always `min <= max`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    min: Point,
    max: Point,
}

impl Bounds {
    /// The degenerate box containing exactly `p`.
    pub fn from_point(p: Point) -> Self {
        Bounds { min: p, max: p }
    }

    /// The smallest box containing both corners, in any order.
    pub fn from_corners(a: Point, b: Point) -> Self {
        Bounds { min: Point::new(a.x.min(b.x), a.y.min(b.y)), max: Point::new(a.x.max(b.x), a.y.max(b.y)) }
    }

    pub fn min(&self) -> Point {
        self.min
    }

    pub fn max(&self) -> Point {
        self.max
    }

    pub fn width(&self) -> f64 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f64 {
        self.max.y - self.min.y
    }

    pub fn size(&self) -> Vector {
        self.max - self.min
    }

    pub fn center(&self) -> Point {
        self.min.lerp(self.max, 0.5)
    }

    pub fn include(&mut self, p: Point) {
        self.min = Point::new(self.min.x.min(p.x), self.min.y.min(p.y));
        self.max = Point::new(self.max.x.max(p.x), self.max.y.max(p.y));
    }

    pub fn union(mut self, other: Bounds) -> Bounds {
        self.include(other.min);
        self.include(other.max);
        self
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }

    /// True when the boxes overlap or touch.
    pub fn intersects(&self, other: &Bounds) -> bool {
        self.min.x <= other.max.x && other.min.x <= self.max.x && self.min.y <= other.max.y && other.min.y <= self.max.y
    }

    /// The box grown by `margin` on every side (`margin >= 0`).
    pub fn expanded(&self, margin: f64) -> Bounds {
        let m = Vector::new(margin, margin);
        Bounds { min: self.min - m, max: self.max + m }
    }

    /// Distance from `p` to the box (0 inside).
    pub fn distance_to(&self, p: Point) -> f64 {
        let dx = (self.min.x - p.x).max(0.0).max(p.x - self.max.x);
        let dy = (self.min.y - p.y).max(0.0).max(p.y - self.max.y);
        math::hypot(dx, dy)
    }
}
