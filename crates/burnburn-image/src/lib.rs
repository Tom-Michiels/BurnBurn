//! Source images for tracing: original pixels preserved under a content hash, an explicit physical
//! scale, non-destructive crop/rotation and a binarisation pipeline (grayscale, median, threshold,
//! region masks, morphology) that yields a [`PreparedImage`] for `burnburn-trace`.
//!
//! Pixel coordinates are y-down with the origin at the top-left pixel corner; pixel `(x, y)` covers
//! `[x, x+1) × [y, y+1)`. Conversion to millimetres (y-up) happens in the tracer using [`PixelScale`].
//! This crate has no GUI, GPU, filesystem or network dependency: bytes come in, values go out.

mod adjust;
mod decode;
mod error;
mod gray;
mod mask;
mod perspective;
mod prepare;
mod raster;
mod scale;
mod source;
mod threshold;

pub use adjust::{PixelRect, QuarterTurn};
pub use decode::{DecodeLimits, decode};
pub use error::{DecodeError, ImageError};
pub use gray::{AlphaPolicy, GrayImage, median_filter, to_gray};
pub use mask::{BinaryMask, Morphology, RegionMask, RegionMode, RegionShape};
pub use perspective::{Homography, Perspective, PerspectiveReport};
pub use prepare::{ImageRecipe, PreparedImage, prepare};
pub use raster::{
    BitImage, DitherAlgorithm, MAX_RASTER_PIXELS, RasterError, ToneCurve, dither, resample_area, tone_map,
};
pub use scale::{PixelScale, ScaleError};
pub use source::{PixelFormat, SourceImage, SourceMetadata};
pub use threshold::{MAX_ADAPTIVE_RADIUS, Threshold, otsu_threshold};
