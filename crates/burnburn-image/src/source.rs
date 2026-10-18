use std::sync::Arc;

use burnburn_types::{CanonicalWriter, ContentHash, HashDomain};

use crate::error::ImageError;

/// Sample layout of the original pixels. 16-bit sources stay 16-bit (no silent loss).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PixelFormat {
    Gray8,
    GrayAlpha8,
    Rgb8,
    Rgba8,
    Gray16,
    GrayAlpha16,
    Rgb16,
    Rgba16,
}

impl PixelFormat {
    pub fn channels(self) -> usize {
        match self {
            PixelFormat::Gray8 | PixelFormat::Gray16 => 1,
            PixelFormat::GrayAlpha8 | PixelFormat::GrayAlpha16 => 2,
            PixelFormat::Rgb8 | PixelFormat::Rgb16 => 3,
            PixelFormat::Rgba8 | PixelFormat::Rgba16 => 4,
        }
    }

    pub fn bytes_per_sample(self) -> usize {
        match self {
            PixelFormat::Gray8 | PixelFormat::GrayAlpha8 | PixelFormat::Rgb8 | PixelFormat::Rgba8 => 1,
            _ => 2,
        }
    }

    fn tag(self) -> &'static str {
        match self {
            PixelFormat::Gray8 => "gray8",
            PixelFormat::GrayAlpha8 => "grayalpha8",
            PixelFormat::Rgb8 => "rgb8",
            PixelFormat::Rgba8 => "rgba8",
            PixelFormat::Gray16 => "gray16",
            PixelFormat::GrayAlpha16 => "grayalpha16",
            PixelFormat::Rgb16 => "rgb16",
            PixelFormat::Rgba16 => "rgba16",
        }
    }
}

/// Metadata read from the file. Suggestions only: none of it is applied without a recipe decision.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SourceMetadata {
    /// Resolution stored in the file (PNG `pHYs` in metres, JPEG JFIF density), as (x, y) DPI.
    pub dpi_suggestion: Option<(f64, f64)>,
}

/// An immutable source image. The original samples are kept byte-for-byte (16-bit samples in
/// native-endian `u16` order) and identified by a SHA-256 content hash over format, size and samples.
#[derive(Debug, Clone)]
pub struct SourceImage {
    width: u32,
    height: u32,
    format: PixelFormat,
    samples: Arc<[u8]>,
    hash: ContentHash,
    metadata: SourceMetadata,
}

impl SourceImage {
    /// `samples` holds `width × height × channels` samples, row-major, top row first; 16-bit
    /// formats store each sample as two native-endian bytes.
    pub fn new(width: u32, height: u32, format: PixelFormat, samples: Vec<u8>) -> Result<Self, ImageError> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(format.channels() * format.bytes_per_sample()))
            .ok_or(ImageError::BadDimensions)?;
        if width == 0 || height == 0 || samples.len() != expected {
            return Err(ImageError::BadDimensions);
        }
        let hash = pixel_hash(width, height, format, &samples);
        Ok(SourceImage { width, height, format, samples: samples.into(), hash, metadata: SourceMetadata::default() })
    }

    pub fn from_gray8(width: u32, height: u32, gray: Vec<u8>) -> Result<Self, ImageError> {
        Self::new(width, height, PixelFormat::Gray8, gray)
    }

    pub fn from_rgba8(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, ImageError> {
        Self::new(width, height, PixelFormat::Rgba8, rgba)
    }

    /// Metadata does not take part in the content hash.
    pub fn with_metadata(mut self, metadata: SourceMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn format(&self) -> PixelFormat {
        self.format
    }

    /// The original samples, unchanged.
    pub fn samples(&self) -> &[u8] {
        &self.samples
    }

    pub fn content_hash(&self) -> ContentHash {
        self.hash
    }

    pub fn metadata(&self) -> SourceMetadata {
        self.metadata
    }

    /// Pixel `(x, y)` as straight (non-premultiplied) RGBA scaled to `0.0..=255.0`.
    /// Out-of-range coordinates give transparent black.
    pub fn rgba_at(&self, x: u32, y: u32) -> [f32; 4] {
        if x >= self.width || y >= self.height {
            return [0.0; 4];
        }
        let ch = self.format.channels();
        let bps = self.format.bytes_per_sample();
        let base = (y as usize * self.width as usize + x as usize) * ch * bps;
        let s = |i: usize| -> f32 {
            let at = base + i * bps;
            if bps == 1 {
                self.samples.get(at).map_or(0.0, |&v| f32::from(v))
            } else {
                match (self.samples.get(at), self.samples.get(at + 1)) {
                    (Some(&a), Some(&b)) => f32::from(u16::from_ne_bytes([a, b])) * (255.0 / 65535.0),
                    _ => 0.0,
                }
            }
        };
        match ch {
            1 => [s(0), s(0), s(0), 255.0],
            2 => [s(0), s(0), s(0), s(1)],
            3 => [s(0), s(1), s(2), 255.0],
            _ => [s(0), s(1), s(2), s(3)],
        }
    }
}

/// `ContentHash::of(SourcePixels, canonical(format, width, height, samples))` (DR-17, ADR 0004).
fn pixel_hash(width: u32, height: u32, format: PixelFormat, samples: &[u8]) -> ContentHash {
    let mut w = CanonicalWriter::new();
    w.tag(format.tag());
    w.u64(u64::from(width));
    w.u64(u64::from(height));
    w.bytes(samples);
    // Only integers and bytes were written: `finish` cannot report a non-finite float.
    let payload = w.finish().unwrap_or_default();
    ContentHash::of(HashDomain::SourcePixels, &payload)
}
