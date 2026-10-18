use burnburn_types::{CancellationToken, CanonicalEncode, CanonicalWriter};

use crate::error::ImageError;

/// How transparency becomes a gray value. A transparent edge is never silently read as black.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AlphaPolicy {
    /// Composite over a flat background gray (255 = white paper).
    CompositeOver { background: u8 },
    /// Use opacity only: opaque = black (foreground), transparent = white; colour is ignored.
    Silhouette,
}

impl Default for AlphaPolicy {
    fn default() -> Self {
        AlphaPolicy::CompositeOver { background: 255 }
    }
}

/// A grayscale working image, values `0.0..=255.0` (0 = black). Never replaces the source.
#[derive(Debug, Clone, PartialEq)]
pub struct GrayImage {
    pub width: u32,
    pub height: u32,
    pub values: Vec<f32>,
}

impl GrayImage {
    pub fn get(&self, x: u32, y: u32) -> f32 {
        self.values.get(y as usize * self.width as usize + x as usize).copied().unwrap_or(255.0)
    }
}

/// Rec. 709 luma of gamma-encoded sRGB, after applying `alpha`.
pub fn to_gray(rgba: [f32; 4], alpha: AlphaPolicy) -> f32 {
    let [r, g, b, a] = rgba;
    let a = (a / 255.0).clamp(0.0, 1.0);
    match alpha {
        AlphaPolicy::Silhouette => 255.0 * (1.0 - a),
        AlphaPolicy::CompositeOver { background } => {
            let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            (luma * a + f32::from(background) * (1.0 - a)).clamp(0.0, 255.0)
        }
    }
}

/// Largest supported median radius (window 33×33).
pub const MAX_MEDIAN_RADIUS: u32 = 16;

/// Median over a `(2r+1)²` square window; edges replicate the border pixel.
pub fn median_filter(img: &GrayImage, radius: u32, cancel: &CancellationToken) -> Result<GrayImage, ImageError> {
    if radius > MAX_MEDIAN_RADIUS {
        return Err(ImageError::RadiusTooLarge { radius, max: MAX_MEDIAN_RADIUS });
    }
    if radius == 0 {
        return Ok(img.clone());
    }
    let (w, h) = (i64::from(img.width), i64::from(img.height));
    let r = i64::from(radius);
    let mut window = Vec::with_capacity(((2 * r + 1) * (2 * r + 1)) as usize);
    let mut values = Vec::with_capacity(img.values.len());
    for y in 0..h {
        cancel.check()?;
        for x in 0..w {
            window.clear();
            for dy in -r..=r {
                let sy = (y + dy).clamp(0, h - 1);
                for dx in -r..=r {
                    let sx = (x + dx).clamp(0, w - 1);
                    window.push(img.values.get((sy * w + sx) as usize).copied().unwrap_or(255.0));
                }
            }
            let mid = window.len() / 2;
            let (_, m, _) = window.select_nth_unstable_by(mid, f32::total_cmp);
            values.push(*m);
        }
    }
    Ok(GrayImage { width: img.width, height: img.height, values })
}

impl CanonicalEncode for AlphaPolicy {
    fn encode(&self, out: &mut CanonicalWriter) {
        match *self {
            AlphaPolicy::CompositeOver { background } => {
                out.tag("alpha-composite");
                out.u64(u64::from(background));
            }
            AlphaPolicy::Silhouette => out.tag("alpha-silhouette"),
        }
    }
}
