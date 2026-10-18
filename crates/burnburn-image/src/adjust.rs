use burnburn_types::{CanonicalEncode, CanonicalWriter};

use crate::error::ImageError;

/// An axis-aligned pixel rectangle `[x, x+width) × [y, y+height)` in y-down pixel coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Lossless rotation applied after the crop, viewed on screen (y down).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum QuarterTurn {
    #[default]
    None,
    Clockwise90,
    Half,
    CounterClockwise90,
}

impl PixelRect {
    pub(crate) fn validate_within(&self, width: u32, height: u32) -> Result<(), ImageError> {
        let fits_x = self.x.checked_add(self.width).is_some_and(|r| r <= width);
        let fits_y = self.y.checked_add(self.height).is_some_and(|b| b <= height);
        if self.width == 0 || self.height == 0 || !fits_x || !fits_y {
            return Err(ImageError::CropOutOfBounds);
        }
        Ok(())
    }
}

/// Maps between adjusted (cropped, rotated) pixel indices and source pixel indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Orientation {
    pub crop: PixelRect,
    pub turn: QuarterTurn,
}

impl Orientation {
    /// Size of the adjusted image.
    pub fn size(&self) -> (u32, u32) {
        match self.turn {
            QuarterTurn::None | QuarterTurn::Half => (self.crop.width, self.crop.height),
            QuarterTurn::Clockwise90 | QuarterTurn::CounterClockwise90 => (self.crop.height, self.crop.width),
        }
    }

    /// Source pixel shown at adjusted pixel `(x, y)`.
    pub fn source_of(&self, x: u32, y: u32) -> (u32, u32) {
        let (w, h) = (self.crop.width, self.crop.height);
        let (cx, cy) = match self.turn {
            QuarterTurn::None => (x, y),
            // Clockwise: source (sx, sy) appears at (h-1-sy, sx).
            QuarterTurn::Clockwise90 => (y, h.saturating_sub(1).saturating_sub(x)),
            QuarterTurn::Half => (w.saturating_sub(1).saturating_sub(x), h.saturating_sub(1).saturating_sub(y)),
            // Counter-clockwise: source (sx, sy) appears at (sy, w-1-sx).
            QuarterTurn::CounterClockwise90 => (w.saturating_sub(1).saturating_sub(y), x),
        };
        (self.crop.x + cx, self.crop.y + cy)
    }
}

impl CanonicalEncode for QuarterTurn {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.tag(match self {
            QuarterTurn::None => "turn-0",
            QuarterTurn::Clockwise90 => "turn-cw90",
            QuarterTurn::Half => "turn-180",
            QuarterTurn::CounterClockwise90 => "turn-ccw90",
        });
    }
}

impl CanonicalEncode for PixelRect {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.tag("pixel-rect");
        out.u64(u64::from(self.x));
        out.u64(u64::from(self.y));
        out.u64(u64::from(self.width));
        out.u64(u64::from(self.height));
    }
}
