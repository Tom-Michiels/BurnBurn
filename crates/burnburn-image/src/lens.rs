//! Radial lens distortion of a photo (G7-trace, T03): a polynomial model with two coefficients,
//! corrected before the plane correction so straight edges of the object are straight again.
//!
//! Model (Brown–Conrady radial part, no tangential terms): a point of the undistorted image at
//! normalised radius `r_u` appears in the photo at `r_d = r_u · (1 + k1·r_u² + k2·r_u⁴)`. Radii are
//! measured from the centre of the image and divided by half the diagonal, so `r = 1` is a corner and
//! the coefficients do not depend on the resolution. Negative `k1` is barrel distortion (straight
//! lines bulge outwards), positive `k1` is pincushion.

use burnburn_types::{CancellationToken, CanonicalEncode, CanonicalWriter};

use crate::error::ImageError;
use crate::gray::GrayImage;

/// The two radial coefficients of the photo's lens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LensDistortion {
    pub k1: f64,
    pub k2: f64,
}

impl LensDistortion {
    /// The largest `|k|` accepted: beyond this the model is not a plausible lens.
    pub const MAX_COEFFICIENT: f64 = 1.0;

    /// Refuses non-finite values, values beyond [`Self::MAX_COEFFICIENT`] and coefficients for which
    /// the mapping folds back on itself inside the image (`d r_d / d r_u ≤ 0` somewhere on `[0, 1]`).
    pub fn validate(&self) -> Result<(), ImageError> {
        let finite = self.k1.is_finite() && self.k2.is_finite();
        if !finite || self.k1.abs() > Self::MAX_COEFFICIENT || self.k2.abs() > Self::MAX_COEFFICIENT {
            return Err(ImageError::InvalidLens);
        }
        let folds = (0..=100).any(|i| {
            let r = f64::from(i) / 100.0;
            1.0 + 3.0 * self.k1 * r * r + 5.0 * self.k2 * r.powi(4) <= 1e-3
        });
        if folds { Err(ImageError::InvalidLens) } else { Ok(()) }
    }

    /// Where the undistorted point `(x, y)` (pixel coordinates in an image of `w × h`) lies in the photo.
    pub fn distort(&self, w: u32, h: u32, p: [f64; 2]) -> [f64; 2] {
        let (cx, cy) = (f64::from(w) / 2.0, f64::from(h) / 2.0);
        let half_diagonal = cx.hypot(cy);
        let (dx, dy) = ((p[0] - cx) / half_diagonal, (p[1] - cy) / half_diagonal);
        let r2 = dx * dx + dy * dy;
        let f = 1.0 + self.k1 * r2 + self.k2 * r2 * r2;
        [cx + dx * f * half_diagonal, cy + dy * f * half_diagonal]
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
        out.tag("lens-v1");
        out.f64(self.k1);
        out.f64(self.k2);
    }
}
