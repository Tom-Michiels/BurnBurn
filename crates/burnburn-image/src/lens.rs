//! Lens distortion of a photo (G7-trace, T03; tangential terms G8-trace): the Brown–Conrady model,
//! corrected before the plane correction so straight edges of the object are straight again.
//!
//! A point of the undistorted image at normalised `(x, y)` with `r² = x² + y²` appears in the photo at
//!
//! ```text
//! x_d = x (1 + k1 r² + k2 r⁴) + 2 p1 x y + p2 (r² + 2 x²)
//! y_d = y (1 + k1 r² + k2 r⁴) + p1 (r² + 2 y²) + 2 p2 x y
//! ```
//!
//! Coordinates are measured from the centre of the image and divided by half the diagonal, so `r = 1`
//! is a corner and the coefficients do not depend on the resolution. Negative `k1` is barrel
//! distortion (straight lines bulge outwards), positive `k1` is pincushion; `p1`, `p2` describe a lens
//! that is not centred on the sensor.

use burnburn_types::{CancellationToken, CanonicalEncode, CanonicalWriter};

use crate::error::ImageError;
use crate::gray::GrayImage;

/// The two radial coefficients of the photo's lens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LensDistortion {
    pub k1: f64,
    pub k2: f64,
    /// Tangential coefficients (decentring of the lens), both 0 for a purely radial lens.
    pub p1: f64,
    pub p2: f64,
}

impl LensDistortion {
    /// A purely radial lens.
    pub const fn radial(k1: f64, k2: f64) -> Self {
        Self { k1, k2, p1: 0.0, p2: 0.0 }
    }

    /// The largest `|k|` accepted: beyond this the model is not a plausible lens.
    pub const MAX_COEFFICIENT: f64 = 1.0;

    /// Refuses non-finite values, values beyond [`Self::MAX_COEFFICIENT`] and coefficients for which
    /// the mapping folds back on itself inside the image (`d r_d / d r_u ≤ 0` somewhere on `[0, 1]`).
    pub fn validate(&self) -> Result<(), ImageError> {
        let all = [self.k1, self.k2, self.p1, self.p2];
        if all.iter().any(|k| !k.is_finite() || k.abs() > Self::MAX_COEFFICIENT) {
            return Err(ImageError::InvalidLens);
        }
        let folds = (0..=100).any(|i| {
            let r = f64::from(i) / 100.0;
            1.0 + 3.0 * self.k1 * r * r + 5.0 * self.k2 * r.powi(4) <= 1e-3
        });
        if folds || (self.has_tangential() && self.jacobian_folds()) { Err(ImageError::InvalidLens) } else { Ok(()) }
    }

    /// Whether `p1` or `p2` is in use.
    pub fn has_tangential(&self) -> bool {
        self.p1 != 0.0 || self.p2 != 0.0
    }

    /// Whether the determinant of the Jacobian of the whole mapping is not clearly positive somewhere
    /// on the unit disc (the corners of the image lie on it), that is, the mapping folds over.
    fn jacobian_folds(&self) -> bool {
        (-50..=50).any(|i| {
            (-50..=50).any(|j| {
                let (x, y) = (f64::from(i) / 50.0, f64::from(j) / 50.0);
                let r2 = x * x + y * y;
                if r2 > 1.0 {
                    return false;
                }
                let f = 1.0 + self.k1 * r2 + self.k2 * r2 * r2;
                let g = self.k1 + 2.0 * self.k2 * r2;
                let dxdx = f + 2.0 * x * x * g + 2.0 * self.p1 * y + 6.0 * self.p2 * x;
                let dxdy = 2.0 * x * y * g + 2.0 * self.p1 * x + 2.0 * self.p2 * y;
                let dydx = 2.0 * x * y * g + 2.0 * self.p1 * x + 2.0 * self.p2 * y;
                let dydy = f + 2.0 * y * y * g + 6.0 * self.p1 * y + 2.0 * self.p2 * x;
                dxdx * dydy - dxdy * dydx <= 1e-3
            })
        })
    }

    /// Where the undistorted point `(x, y)` (pixel coordinates in an image of `w × h`) lies in the photo.
    pub fn distort(&self, w: u32, h: u32, p: [f64; 2]) -> [f64; 2] {
        let (cx, cy) = (f64::from(w) / 2.0, f64::from(h) / 2.0);
        let half_diagonal = cx.hypot(cy);
        let (dx, dy) = ((p[0] - cx) / half_diagonal, (p[1] - cy) / half_diagonal);
        let r2 = dx * dx + dy * dy;
        let f = 1.0 + self.k1 * r2 + self.k2 * r2 * r2;
        let xd = dx * f + 2.0 * self.p1 * dx * dy + self.p2 * (r2 + 2.0 * dx * dx);
        let yd = dy * f + self.p1 * (r2 + 2.0 * dy * dy) + 2.0 * self.p2 * dx * dy;
        [cx + xd * half_diagonal, cy + yd * half_diagonal]
    }
}

/// The image as the lens would have seen a distortion-free scene: every output pixel samples the
/// photo where the model puts it (bilinear, nearest edge value outside).
pub(crate) fn undistort(
    gray: &GrayImage,
    lens: &LensDistortion,
    cancel: &CancellationToken,
) -> Result<GrayImage, ImageError> {
    lens.validate()?;
    let (w, h) = (gray.width, gray.height);
    let mut values = Vec::with_capacity(w as usize * h as usize);
    for v in 0..h {
        if v % 64 == 0 {
            cancel.check()?;
        }
        for u in 0..w {
            let q = lens.distort(w, h, [f64::from(u) + 0.5, f64::from(v) + 0.5]);
            let x = (q[0] - 0.5).clamp(0.0, f64::from(w) - 1.0);
            let y = (q[1] - 0.5).clamp(0.0, f64::from(h) - 1.0);
            let (x0, y0) = (x.floor(), y.floor());
            let (fx, fy) = ((x - x0) as f32, (y - y0) as f32);
            let (x0, y0) = (x0 as u32, y0 as u32);
            let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
            let top = gray.get(x0, y0) * (1.0 - fx) + gray.get(x1, y0) * fx;
            let bottom = gray.get(x0, y1) * (1.0 - fx) + gray.get(x1, y1) * fx;
            values.push(top * (1.0 - fy) + bottom * fy);
        }
    }
    Ok(GrayImage { width: w, height: h, values })
}

impl CanonicalEncode for LensDistortion {
    fn encode(&self, out: &mut CanonicalWriter) {
        // A purely radial lens keeps the encoding (and so the recipe keys) it always had.
        if self.has_tangential() {
            out.tag("lens-v2");
            out.f64(self.k1);
            out.f64(self.k2);
            out.f64(self.p1);
            out.f64(self.p2);
        } else {
            out.tag("lens-v1");
            out.f64(self.k1);
            out.f64(self.k2);
        }
    }
}
