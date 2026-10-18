//! PNG/JPEG source images (IM10, P03): validated, measured and stored as original bytes.

use std::fmt;

use burnburn_types::Length;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RasterFormat {
    Png,
    Jpeg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RasterLimits {
    pub max_bytes: usize,
    pub max_pixels: u64,
}

impl Default for RasterLimits {
    fn default() -> Self {
        RasterLimits { max_bytes: 256 << 20, max_pixels: 400_000_000 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RasterError {
    TooLarge {
        limit: usize,
        actual: usize,
    },
    TooManyPixels {
        limit: u64,
        actual: u64,
    },
    UnsupportedFormat,
    /// The decoder rejected the data (truncated, corrupt).
    Decode(String),
    /// The file has no physical density; the user must give a DPI or a size.
    DpiRequired,
    InvalidDpi,
}

impl fmt::Display for RasterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for RasterError {}

/// What BurnBurn knows about a source image without changing its pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct RasterInfo {
    pub format: RasterFormat,
    pub width_px: u32,
    pub height_px: u32,
    /// Pixels per inch from the file (PNG `pHYs` in metres, JPEG JFIF in inch/cm).
    pub dpi: Option<(f64, f64)>,
}

impl RasterInfo {
    pub fn media_type(&self) -> &'static str {
        match self.format {
            RasterFormat::Png => "image/png",
            RasterFormat::Jpeg => "image/jpeg",
        }
    }

    /// Width and height in mm, from `dpi` when given (a user override) or the file's density.
    pub fn physical_size(&self, dpi: Option<f64>) -> Result<(Length, Length), RasterError> {
        let (dx, dy) = match dpi {
            Some(d) if d.is_finite() && d > 0.0 => (d, d),
            Some(_) => return Err(RasterError::InvalidDpi),
            None => self.dpi.ok_or(RasterError::DpiRequired)?,
        };
        let mm = |px: u32, d: f64| Length::from_mm(f64::from(px) / d * 25.4).map_err(|_| RasterError::InvalidDpi);
        Ok((mm(self.width_px, dx)?, mm(self.height_px, dy)?))
    }
}

const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Validates (full decode within limits) and measures a PNG or JPEG. Pixels are not changed or kept.
pub fn inspect_raster(bytes: &[u8], limits: &RasterLimits) -> Result<RasterInfo, RasterError> {
    if bytes.len() > limits.max_bytes {
        return Err(RasterError::TooLarge { limit: limits.max_bytes, actual: bytes.len() });
    }
    let (format, image_format, dpi) = if bytes.starts_with(PNG_MAGIC) {
        (RasterFormat::Png, image::ImageFormat::Png, png_dpi(bytes))
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        (RasterFormat::Jpeg, image::ImageFormat::Jpeg, jpeg_dpi(bytes))
    } else {
        return Err(RasterError::UnsupportedFormat);
    };
    let decode_err = |e: image::ImageError| RasterError::Decode(e.to_string());
    let reader = || image::ImageReader::with_format(std::io::Cursor::new(bytes), image_format);
    let (w, h) = reader().into_dimensions().map_err(decode_err)?;
    let pixels = u64::from(w) * u64::from(h);
    if pixels > limits.max_pixels {
        return Err(RasterError::TooManyPixels { limit: limits.max_pixels, actual: pixels });
    }
    let mut r = reader();
    let mut l = image::Limits::default();
    l.max_alloc = Some(pixels.saturating_mul(16).max(1 << 20));
    r.limits(l);
    r.decode().map_err(decode_err)?;
    Ok(RasterInfo { format, width_px: w, height_px: h, dpi })
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4).and_then(|s| s.try_into().ok()).map(u32::from_be_bytes)
}

/// `pHYs` with unit 1 (metre) → pixels per inch; unit 0 is an aspect ratio only.
fn png_dpi(b: &[u8]) -> Option<(f64, f64)> {
    let mut at = PNG_MAGIC.len();
    while let Some(len) = be32(b, at) {
        let kind = b.get(at + 4..at + 8)?;
        let data = b.get(at + 8..at + 8 + len as usize)?;
        match kind {
            b"pHYs" => {
                let (x, y, unit) = (be32(data, 0)?, be32(data, 4)?, *data.get(8)?);
                return (unit == 1 && x > 0 && y > 0).then(|| (f64::from(x) * 0.0254, f64::from(y) * 0.0254));
            }
            b"IDAT" | b"IEND" => return None,
            _ => {}
        }
        at = at.checked_add(12 + len as usize)?;
    }
    None
}

/// JFIF APP0 density: unit 1 = per inch, 2 = per cm, 0 = aspect ratio only.
fn jpeg_dpi(b: &[u8]) -> Option<(f64, f64)> {
    let mut at = 2;
    while *b.get(at)? == 0xff {
        let marker = *b.get(at + 1)?;
        if marker == 0xda || marker == 0xd9 {
            return None; // start of scan / end: no JFIF header before the image data
        }
        let len = usize::from(u16::from_be_bytes([*b.get(at + 2)?, *b.get(at + 3)?]));
        let seg = b.get(at + 4..at + 2 + len)?;
        if marker == 0xe0 && seg.starts_with(b"JFIF\0") {
            let unit = *seg.get(7)?;
            let x = f64::from(u16::from_be_bytes([*seg.get(8)?, *seg.get(9)?]));
            let y = f64::from(u16::from_be_bytes([*seg.get(10)?, *seg.get(11)?]));
            let per_inch = match unit {
                1 => 1.0,
                2 => 2.54,
                _ => return None,
            };
            return (x > 0.0 && y > 0.0).then_some((x * per_inch, y * per_inch));
        }
        at += 2 + len;
    }
    None
}
