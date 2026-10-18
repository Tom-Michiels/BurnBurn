use burnburn_types::Angle;

use crate::math;
use crate::{GeometryError, Point, Vector};

/// A 2D affine transform `p' = M p + t` with `M = [[a, c], [b, d]]` and `t = (e, f)`.
/// All entries are finite by construction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
}

/// Relative determinant threshold below which a matrix is treated as singular.
/// It is a numerical-conditioning bound, not a user tolerance.
const SINGULAR_RELATIVE_DET: f64 = 1e-12;

impl Affine {
    pub const IDENTITY: Affine = Affine { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };

    /// Raw coefficients (column-major `[a, b, c, d, e, f]`); rejects non-finite entries.
    pub fn new(coefficients: [f64; 6]) -> Result<Self, GeometryError> {
        if coefficients.iter().any(|v| !v.is_finite()) {
            return Err(GeometryError::NotFinite);
        }
        let [a, b, c, d, e, f] = coefficients;
        Ok(Affine { a, b, c, d, e, f })
    }

    pub fn coefficients(&self) -> [f64; 6] {
        [self.a, self.b, self.c, self.d, self.e, self.f]
    }

    pub fn translate(v: Vector) -> Result<Self, GeometryError> {
        Affine::new([1.0, 0.0, 0.0, 1.0, v.x, v.y])
    }

    pub fn scale(sx: f64, sy: f64) -> Result<Self, GeometryError> {
        Affine::new([sx, 0.0, 0.0, sy, 0.0, 0.0])
    }

    /// Scaling that keeps `fixed` in place.
    pub fn scale_about(fixed: Point, sx: f64, sy: f64) -> Result<Self, GeometryError> {
        let to_origin = Affine::translate(-fixed.to_vector())?;
        let back = Affine::translate(fixed.to_vector())?;
        Ok(to_origin.then(&Affine::scale(sx, sy)?).then(&back))
    }

    /// Counter-clockwise rotation about the origin.
    pub fn rotate(angle: Angle) -> Self {
        let (s, c) = math::sin_cos(angle.radians());
        Affine { a: c, b: s, c: -s, d: c, e: 0.0, f: 0.0 }
    }

    /// Mirror across the vertical axis `x = 0` (x ↦ −x).
    pub fn mirror_x() -> Self {
        Affine { a: -1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 }
    }

    pub fn determinant(&self) -> f64 {
        self.a * self.d - self.b * self.c
    }

    /// `self` followed by `next`: `(self.then(next)).apply(p) == next.apply(self.apply(p))`.
    pub fn then(&self, next: &Affine) -> Affine {
        let n = next;
        Affine {
            a: n.a * self.a + n.c * self.b,
            b: n.b * self.a + n.d * self.b,
            c: n.a * self.c + n.c * self.d,
            d: n.b * self.c + n.d * self.d,
            e: n.a * self.e + n.c * self.f + n.e,
            f: n.b * self.e + n.d * self.f + n.f,
        }
    }

    /// The inverse transform, or [`GeometryError::Singular`] when the linear part collapses area.
    pub fn inverse(&self) -> Result<Affine, GeometryError> {
        let det = self.determinant();
        let scale = self.a.abs().max(self.b.abs()).max(self.c.abs()).max(self.d.abs());
        if !(det.abs() > SINGULAR_RELATIVE_DET * scale * scale) {
            return Err(GeometryError::Singular);
        }
        let (a, b, c, d) = (self.d / det, -self.b / det, -self.c / det, self.a / det);
        let e = -(a * self.e + c * self.f);
        let f = -(b * self.e + d * self.f);
        // Entries can overflow for extreme (but finite) inputs.
        Affine::new([a, b, c, d, e, f]).map_err(|_| GeometryError::Singular)
    }

    pub fn apply(&self, p: Point) -> Point {
        Point::new(self.a * p.x + self.c * p.y + self.e, self.b * p.x + self.d * p.y + self.f)
    }

    pub fn apply_vector(&self, v: Vector) -> Vector {
        Vector::new(self.a * v.x + self.c * v.y, self.b * v.x + self.d * v.y)
    }

    /// True when the transform flips orientation (negative determinant).
    pub fn is_mirroring(&self) -> bool {
        self.determinant() < 0.0
    }
}
