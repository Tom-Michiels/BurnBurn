use burnburn_types::{CanonicalEncode, CanonicalWriter};

use crate::gray::GrayImage;

/// Binarisation level. A pixel is foreground when its gray value is `<= level` (dark on light),
/// or `> level` when the recipe inverts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Threshold {
    Fixed(u8),
    /// Otsu's between-class variance maximum over the 256-bin histogram of the working image.
    #[default]
    Otsu,
    /// Local (adaptive) threshold for uneven lighting: a pixel is foreground when it is more than
    /// `offset` gray levels darker than the mean of the `(2·radius+1)²` window around it (window
    /// clipped at the image border). Inverted recipes compare "brighter" instead.
    Adaptive {
        radius: u32,
        offset: u8,
    },
}

/// Largest adaptive-threshold window radius.
pub const MAX_ADAPTIVE_RADIUS: u32 = 256;

/// Otsu threshold `t`: classes are `round(v) <= t` and `> t`. Uniform images give 127.
pub fn otsu_threshold(img: &GrayImage) -> u8 {
    let mut hist = [0u64; 256];
    for &v in &img.values {
        hist[v.round().clamp(0.0, 255.0) as usize] += 1;
    }
    let total: u64 = hist.iter().sum();
    let sum_all: f64 = hist.iter().enumerate().map(|(i, &n)| i as f64 * n as f64).sum();
    let (mut w0, mut sum0) = (0u64, 0.0f64);
    let mut best: Option<(f64, usize)> = None;
    for (t, &n) in hist.iter().enumerate() {
        w0 += n;
        sum0 += t as f64 * n as f64;
        let w1 = total - w0;
        if w0 == 0 || w1 == 0 {
            continue;
        }
        let m0 = sum0 / w0 as f64;
        let m1 = (sum_all - sum0) / w1 as f64;
        let between = w0 as f64 * w1 as f64 * (m0 - m1) * (m0 - m1);
        // Strictly greater keeps the lowest t of a plateau; take the plateau midpoint below.
        match best {
            Some((b, _)) if between <= b => {}
            _ => best = Some((between, t)),
        }
    }
    let Some((_, lo)) = best else { return 127 };
    // Every t between `lo` and the next occupied bin gives the same split; use the middle.
    let hi = (lo + 1..256).find(|&i| hist[i] > 0).map_or(lo, |i| i - 1);
    ((lo + hi) / 2) as u8
}

impl CanonicalEncode for Threshold {
    fn encode(&self, out: &mut CanonicalWriter) {
        match *self {
            Threshold::Fixed(t) => {
                out.tag("threshold-fixed");
                out.u64(u64::from(t));
            }
            Threshold::Otsu => out.tag("threshold-otsu"),
            Threshold::Adaptive { radius, offset } => {
                out.tag("threshold-adaptive");
                out.u64(u64::from(radius));
                out.u64(u64::from(offset));
            }
        }
    }
}
