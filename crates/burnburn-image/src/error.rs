use std::fmt;

use burnburn_types::Cancelled;

use crate::scale::ScaleError;

/// Why bytes could not become a [`crate::SourceImage`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeError {
    /// Only PNG, JPEG, BMP and TIFF are accepted.
    UnsupportedFormat,
    /// The declared dimensions exceed the configured limit; nothing was allocated for pixels.
    TooLarge { width: u32, height: u32, max_pixels: u64 },
    /// A zero width or height.
    Empty,
    /// The decoder rejected the data (truncated, corrupt, unsupported variant).
    Malformed(String),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::UnsupportedFormat => f.write_str("only PNG, JPEG, BMP and TIFF images are supported"),
            DecodeError::TooLarge { width, height, max_pixels } => {
                write!(f, "image of {width}×{height} pixels exceeds the limit of {max_pixels} pixels")
            }
            DecodeError::Empty => f.write_str("image has no pixels"),
            DecodeError::Malformed(why) => write!(f, "image data is malformed: {why}"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Why an image operation was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImageError {
    /// Pixel buffer length does not match `width × height × channels`, or a dimension is zero.
    BadDimensions,
    /// The crop rectangle is empty or reaches outside the source image.
    CropOutOfBounds,
    /// A region polygon has fewer than three vertices or a non-finite coordinate.
    BadRegion,
    /// Filter radius beyond the supported maximum.
    RadiusTooLarge {
        radius: u32,
        max: u32,
    },
    /// Perspective points are too few, collinear, non-finite, or the output size is zero.
    DegeneratePerspective,
    /// Physical scale problem (missing scale is [`ScaleError::NeedsScale`]).
    Scale(ScaleError),
    Cancelled(Cancelled),
}

impl From<Cancelled> for ImageError {
    fn from(c: Cancelled) -> Self {
        ImageError::Cancelled(c)
    }
}

impl From<ScaleError> for ImageError {
    fn from(e: ScaleError) -> Self {
        ImageError::Scale(e)
    }
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImageError::BadDimensions => f.write_str("pixel buffer does not match the image dimensions"),
            ImageError::CropOutOfBounds => f.write_str("crop rectangle is empty or outside the image"),
            ImageError::BadRegion => f.write_str("region mask polygon is invalid"),
            ImageError::RadiusTooLarge { radius, max } => write!(f, "filter radius {radius} exceeds {max}"),
            ImageError::DegeneratePerspective => f.write_str("perspective correction points are degenerate"),
            ImageError::Scale(e) => e.fmt(f),
            ImageError::Cancelled(c) => c.fmt(f),
        }
    }
}

impl std::error::Error for ImageError {}
