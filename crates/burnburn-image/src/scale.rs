use std::fmt;

use burnburn_types::{CanonicalEncode, CanonicalWriter, Length};

/// Physical size of one pixel. Independent per axis (scanners and some cameras are anisotropic).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelScale {
    x_mm: f64,
    y_mm: f64,
}

/// Why a physical scale is unusable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScaleError {
    /// No scale was declared. File DPI is only a suggestion and is never applied silently.
    NeedsScale,
    /// A scale, DPI or calibration distance must be finite and strictly positive.
    NotPositive,
}

impl fmt::Display for ScaleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScaleError::NeedsScale => f.write_str("image needs a physical scale before tracing"),
            ScaleError::NotPositive => f.write_str("scale must be finite and strictly positive"),
        }
    }
}

impl std::error::Error for ScaleError {}

const MM_PER_INCH: f64 = 25.4;

fn positive(v: f64) -> Result<f64, ScaleError> {
    if v.is_finite() && v > 0.0 { Ok(v) } else { Err(ScaleError::NotPositive) }
}

impl PixelScale {
    /// Explicit millimetres per pixel along x and y.
    pub fn from_mm_per_pixel(x_mm: f64, y_mm: f64) -> Result<Self, ScaleError> {
        Ok(PixelScale { x_mm: positive(x_mm)?, y_mm: positive(y_mm)? })
    }

    pub fn uniform_mm_per_pixel(mm: f64) -> Result<Self, ScaleError> {
        Self::from_mm_per_pixel(mm, mm)
    }

    /// Dots per inch along x and y.
    pub fn from_dpi(x_dpi: f64, y_dpi: f64) -> Result<Self, ScaleError> {
        Self::from_mm_per_pixel(MM_PER_INCH / positive(x_dpi)?, MM_PER_INCH / positive(y_dpi)?)
    }

    /// Calibration: a known physical `length` spans `pixels` (isotropic pixels assumed).
    pub fn from_known_distance(pixels: f64, length: Length) -> Result<Self, ScaleError> {
        Self::uniform_mm_per_pixel(positive(length.mm())? / positive(pixels)?)
    }

    pub fn mm_per_pixel_x(&self) -> f64 {
        self.x_mm
    }

    pub fn mm_per_pixel_y(&self) -> f64 {
        self.y_mm
    }

    /// The scale after a quarter turn: the axes trade places.
    pub fn swapped(self) -> Self {
        PixelScale { x_mm: self.y_mm, y_mm: self.x_mm }
    }
}

impl CanonicalEncode for PixelScale {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.tag("pixel-scale");
        out.f64(self.x_mm);
        out.f64(self.y_mm);
    }
}
