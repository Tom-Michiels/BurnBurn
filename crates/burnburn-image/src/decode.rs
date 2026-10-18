use std::io::Cursor;

use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};

use crate::error::DecodeError;
use crate::source::{PixelFormat, SourceImage, SourceMetadata};

/// Resource limits for decoding untrusted bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeLimits {
    /// Maximum `width × height`; checked from the header before pixel memory is allocated.
    pub max_pixels: u64,
}

impl Default for DecodeLimits {
    /// 40 MP: twice the 20 MP robustness target of the tracer plan.
    fn default() -> Self {
        DecodeLimits { max_pixels: 40_000_000 }
    }
}

/// Decodes PNG or JPEG bytes. Other formats are rejected even if the decoder could read them.
/// The decoded samples are stored unchanged (no colour conversion, no EXIF rotation).
pub fn decode(bytes: &[u8], limits: DecodeLimits) -> Result<SourceImage, DecodeError> {
    let format = match image::guess_format(bytes) {
        Ok(f @ (ImageFormat::Png | ImageFormat::Jpeg)) => f,
        _ => return Err(DecodeError::UnsupportedFormat),
    };
    let malformed = |e: image::ImageError| DecodeError::Malformed(e.to_string());

    let mut reader = ImageReader::new(Cursor::new(bytes));
    reader.set_format(format);
    let decoder = reader.into_decoder().map_err(malformed)?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 {
        return Err(DecodeError::Empty);
    }
    let pixels = u64::from(width) * u64::from(height);
    if pixels > limits.max_pixels {
        return Err(DecodeError::TooLarge { width, height, max_pixels: limits.max_pixels });
    }
    let mut decoder_limits = image::Limits::default();
    decoder_limits.max_image_width = Some(width);
    decoder_limits.max_image_height = Some(height);
    // 16-bit RGBA is the widest layout png/jpeg produce: 8 bytes per pixel, plus decoder slack.
    decoder_limits.max_alloc = Some(pixels.saturating_mul(16).saturating_add(1 << 20));
    let mut decoder = decoder;
    decoder.set_limits(decoder_limits).map_err(malformed)?;
    let img = DynamicImage::from_decoder(decoder).map_err(malformed)?;

    let (format, samples) = match img {
        DynamicImage::ImageLuma8(b) => (PixelFormat::Gray8, b.into_raw()),
        DynamicImage::ImageLumaA8(b) => (PixelFormat::GrayAlpha8, b.into_raw()),
        DynamicImage::ImageRgb8(b) => (PixelFormat::Rgb8, b.into_raw()),
        DynamicImage::ImageRgba8(b) => (PixelFormat::Rgba8, b.into_raw()),
        DynamicImage::ImageLuma16(b) => (PixelFormat::Gray16, u16_bytes(b.into_raw())),
        DynamicImage::ImageLumaA16(b) => (PixelFormat::GrayAlpha16, u16_bytes(b.into_raw())),
        DynamicImage::ImageRgb16(b) => (PixelFormat::Rgb16, u16_bytes(b.into_raw())),
        DynamicImage::ImageRgba16(b) => (PixelFormat::Rgba16, u16_bytes(b.into_raw())),
        _ => return Err(DecodeError::Malformed("unsupported sample layout".into())),
    };
    let image = SourceImage::new(width, height, format, samples)
        .map_err(|_| DecodeError::Malformed("decoder returned a mismatched buffer".into()))?;
    let dpi = match format_of(bytes) {
        Some(ImageFormat::Png) => png_dpi(bytes),
        Some(ImageFormat::Jpeg) => jfif_dpi(bytes),
        _ => None,
    };
    Ok(image.with_metadata(SourceMetadata { dpi_suggestion: dpi }))
}

fn format_of(bytes: &[u8]) -> Option<ImageFormat> {
    image::guess_format(bytes).ok()
}

fn u16_bytes(samples: Vec<u16>) -> Vec<u8> {
    samples.into_iter().flat_map(u16::to_ne_bytes).collect()
}

fn be_u32(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at.checked_add(4)?)?;
    Some(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

fn be_u16(b: &[u8], at: usize) -> Option<u16> {
    let s = b.get(at..at.checked_add(2)?)?;
    Some(u16::from_be_bytes([s[0], s[1]]))
}

/// PNG `pHYs` with unit "metre" (1). Walks chunks until `IDAT`; bounded by the input length.
fn png_dpi(bytes: &[u8]) -> Option<(f64, f64)> {
    let mut at = 8usize;
    loop {
        let len = usize::try_from(be_u32(bytes, at)?).ok()?;
        let kind = bytes.get(at + 4..at + 8)?;
        if kind == b"IDAT" || kind == b"IEND" {
            return None;
        }
        if kind == b"pHYs" && len == 9 {
            let x = be_u32(bytes, at + 8)?;
            let y = be_u32(bytes, at + 12)?;
            let unit = *bytes.get(at + 16)?;
            if unit != 1 || x == 0 || y == 0 {
                return None;
            }
            return Some((f64::from(x) * 0.0254, f64::from(y) * 0.0254));
        }
        at = at.checked_add(len)?.checked_add(12)?;
    }
}

/// JPEG `APP0` JFIF density with units 1 (DPI) or 2 (dots per cm). Only the first segment is read.
fn jfif_dpi(bytes: &[u8]) -> Option<(f64, f64)> {
    if bytes.get(2..4)? != [0xff, 0xe0] || bytes.get(6..11)? != b"JFIF\0" {
        return None;
    }
    let units = *bytes.get(13)?;
    let x = f64::from(be_u16(bytes, 14)?);
    let y = f64::from(be_u16(bytes, 16)?);
    if x == 0.0 || y == 0.0 {
        return None;
    }
    match units {
        1 => Some((x, y)),
        2 => Some((x * 2.54, y * 2.54)),
        _ => None,
    }
}
