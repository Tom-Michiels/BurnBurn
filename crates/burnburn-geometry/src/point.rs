use std::ops::{Add, Mul, Neg, Sub};

use crate::GeometryError;
use crate::math;

/// A position in canonical millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// A displacement in canonical millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vector {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const ORIGIN: Point = Point { x: 0.0, y: 0.0 };

    pub const fn new(x: f64, y: f64) -> Self {
        Point { x, y }
    }

    /// A point from untrusted input; NaN and infinities are rejected.
    pub fn try_new(x: f64, y: f64) -> Result<Self, GeometryError> {
        let p = Point { x, y };
        if p.is_finite() { Ok(p) } else { Err(GeometryError::NotFinite) }
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    pub fn to_vector(self) -> Vector {
        Vector::new(self.x, self.y)
    }

    pub fn distance(self, other: Point) -> f64 {
        (other - self).length()
    }

    pub fn lerp(self, other: Point, t: f64) -> Point {
        self + (other - self) * t
    }
}

impl Vector {
    pub const ZERO: Vector = Vector { x: 0.0, y: 0.0 };

    pub const fn new(x: f64, y: f64) -> Self {
        Vector { x, y }
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    pub fn length(self) -> f64 {
        math::hypot(self.x, self.y)
    }

    pub fn dot(self, other: Vector) -> f64 {
        self.x * other.x + self.y * other.y
    }

    /// The z component of the 3D cross product; positive when `other` is counter-clockwise.
    pub fn cross(self, other: Vector) -> f64 {
        self.x * other.y - self.y * other.x
    }
}

impl Add<Vector> for Point {
    type Output = Point;
    fn add(self, v: Vector) -> Point {
        Point::new(self.x + v.x, self.y + v.y)
    }
}

impl Sub<Vector> for Point {
    type Output = Point;
    fn sub(self, v: Vector) -> Point {
        Point::new(self.x - v.x, self.y - v.y)
    }
}

impl Sub for Point {
    type Output = Vector;
    fn sub(self, other: Point) -> Vector {
        Vector::new(self.x - other.x, self.y - other.y)
    }
}

impl Add for Vector {
    type Output = Vector;
    fn add(self, v: Vector) -> Vector {
        Vector::new(self.x + v.x, self.y + v.y)
    }
}

impl Sub for Vector {
    type Output = Vector;
    fn sub(self, v: Vector) -> Vector {
        Vector::new(self.x - v.x, self.y - v.y)
    }
}

impl Mul<f64> for Vector {
    type Output = Vector;
    fn mul(self, s: f64) -> Vector {
        Vector::new(self.x * s, self.y * s)
    }
}

impl Neg for Vector {
    type Output = Vector;
    fn neg(self) -> Vector {
        Vector::new(-self.x, -self.y)
    }
}
