//! Straight-line living hinge (kerf pattern): staggered cut columns with bridges.

use burnburn_geometry::{Path, Point};

use crate::error::{GenError, positive};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LivingHinge {
    /// Extent across the bend (columns are spread over it).
    pub width: f64,
    /// Extent along the cuts.
    pub height: f64,
    /// Desired cut length; adjusted so the even columns fill `height` exactly.
    pub cut_length: f64,
    /// Material left between cuts in a column and at the ends.
    pub bridge: f64,
    /// Distance between columns.
    pub spacing: f64,
}

/// Open cut lines, column by column, bottom to top.
///
/// Even columns hold `k` cuts with bridges at both ends and between (k and the cut length are
/// chosen so they fill `height` exactly); odd columns are staggered by half a pitch, centred on
/// the even columns' bridges and kept a bridge away from the ends.
pub fn living_hinge(p: &LivingHinge) -> Result<Vec<Path>, GenError> {
    let width = positive("width", p.width)?;
    let height = positive("height", p.height)?;
    let cut = positive("cut_length", p.cut_length)?;
    let bridge = positive("bridge", p.bridge)?;
    let spacing = positive("spacing", p.spacing)?;
    let k = ((height - bridge) / (cut + bridge)).round();
    if !(k >= 1.0) {
        return Err(GenError::TooSmall { param: "height", min_mm: cut + 2.0 * bridge, value_mm: height });
    }
    if k > 100_000.0 {
        return Err(GenError::TooSmall { param: "cut_length", min_mm: height / 100_000.0, value_mm: cut });
    }
    let c = (height - (k + 1.0) * bridge) / k;
    if c <= bridge {
        return Err(GenError::TooSmall { param: "cut_length", min_mm: bridge, value_mm: c });
    }
    let columns = (width / spacing).floor() + 1.0;
    if columns > 100_000.0 {
        return Err(GenError::TooSmall { param: "spacing", min_mm: width / 100_000.0, value_mm: spacing });
    }
    let x0 = (width - (columns - 1.0) * spacing) / 2.0;
    let pitch = c + bridge;
    let line = |x: f64, a: f64, b: f64| Path::line(Point::new(x, a), Point::new(x, b));
    let mut cuts = Vec::new();
    for col in 0..columns as u64 {
        let x = x0 + col as f64 * spacing;
        if col % 2 == 0 {
            for j in 0..k as u64 {
                let a = bridge + j as f64 * pitch;
                cuts.push(line(x, a, a + c)?);
            }
        } else {
            for j in 0..=k as u64 {
                let centre = bridge / 2.0 + j as f64 * pitch;
                let (a, b) = ((centre - c / 2.0).max(bridge), (centre + c / 2.0).min(height - bridge));
                if b > a {
                    cuts.push(line(x, a, b)?);
                }
            }
        }
    }
    Ok(cuts)
}
