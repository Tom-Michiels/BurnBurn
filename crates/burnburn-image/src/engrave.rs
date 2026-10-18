//! The engraving pipeline from a resampled gray raster to the pixels the laser burns.
//!
//! Order (fixed, part of the contract): the raster is already resampled to the physical grid and
//! converted to gray (alpha over paper). Then **unsharp → tone (gamma/brightness/contrast) →
//! negative → mode**. Dithering therefore always sees the final tone at the physical size.
//! `PassThrough` skips every adjustment.

use burnburn_types::CancellationToken;
use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder};

use crate::gray::GrayImage;
use crate::raster::{BitImage, DitherAlgorithm, RasterError, ToneCurve, dither, tone_map};
use crate::screen::{halftone, sketch, unsharp};

/// Unsharp mask in output pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sharpen {
    pub radius_px: u32,
    pub amount: f64,
}

/// How the adjusted gray raster becomes laser output.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum EngraveMode {
    Dither(DitherAlgorithm),
    /// Round-dot screen, cell size in output pixels.
    Halftone {
        cell_px: f64,
        angle_deg: f64,
    },
    /// The classic 45° newspaper screen.
    Newsprint {
        cell_px: f64,
    },
    /// Sobel edges at or above `level`.
    Sketch {
        level: u8,
    },
    /// The pixels as they are: threshold 128, no adjustment.
    PassThrough,
    /// Power levels `0..=255` (0 = full power).
    Grayscale,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EngraveParams {
    pub tone: ToneCurve,
    pub sharpen: Option<Sharpen>,
    pub negative: bool,
    pub mode: EngraveMode,
    /// Error diffusion runs odd rows right to left.
    pub serpentine: bool,
}

/// The processed pixels of one raster: exactly what is scanned, and what "save processed
/// bitmap" writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Engraved {
    Bits(BitImage),
    /// Power levels, row-major, top row first; 0 = full power, 255 = off.
    Levels {
        width: u32,
        height: u32,
        levels: Vec<u8>,
    },
}

impl Engraved {
    pub fn width(&self) -> u32 {
        match self {
            Engraved::Bits(b) => b.width(),
            Engraved::Levels { width, .. } => *width,
        }
    }

    pub fn height(&self) -> u32 {
        match self {
            Engraved::Bits(b) => b.height(),
            Engraved::Levels { height, .. } => *height,
        }
    }

    /// 8-bit gray, row-major, top row first: burn = 0 and white = 255 for bit images, the power
    /// levels themselves for grayscale.
    pub fn to_gray8(&self) -> Vec<u8> {
        match self {
            Engraved::Bits(b) => b.bits().iter().map(|&on| if on { 0 } else { 255 }).collect(),
            Engraved::Levels { levels, .. } => levels.clone(),
        }
    }
}

/// Runs the pipeline on `img` (gray values `0..=255` on the physical grid).
pub fn engrave(img: &GrayImage, params: &EngraveParams, cancel: &CancellationToken) -> Result<Engraved, RasterError> {
    if params.mode == EngraveMode::PassThrough {
        let bits = img.values.iter().map(|&v| (v + 0.5).floor() < 128.0).collect();
        return BitImage::new(img.width, img.height, bits)
            .map(Engraved::Bits)
            .ok_or(RasterError::BadOutputSize { width: img.width, height: img.height });
    }
    let sharpened;
    let base = match params.sharpen {
        Some(s) => {
            sharpened = unsharp(img, s.radius_px, s.amount, cancel)?;
            &sharpened
        }
        None => img,
    };
    let mut levels = tone_map(base, &params.tone);
    if params.negative {
        levels.iter_mut().for_each(|v| *v = 255 - *v);
    }
    if params.mode == EngraveMode::Grayscale {
        return Ok(Engraved::Levels { width: img.width, height: img.height, levels });
    }
    let toned =
        GrayImage { width: img.width, height: img.height, values: levels.iter().map(|&v| f32::from(v)).collect() };
    let bits = match params.mode {
        EngraveMode::Dither(a) => dither(&toned, &ToneCurve::IDENTITY, a, params.serpentine, cancel)?,
        EngraveMode::Halftone { cell_px, angle_deg } => halftone(&toned, cell_px, angle_deg, cancel)?,
        EngraveMode::Newsprint { cell_px } => halftone(&toned, cell_px, 45.0, cancel)?,
        EngraveMode::Sketch { level } => sketch(&toned, level, cancel)?,
        EngraveMode::PassThrough | EngraveMode::Grayscale => return Err(RasterError::EncodeFailed),
    };
    Ok(Engraved::Bits(bits))
}

/// PNG bytes of an 8-bit gray image (`gray` holds `width × height` samples, top row first).
pub fn encode_png_gray8(width: u32, height: u32, gray: &[u8]) -> Result<Vec<u8>, RasterError> {
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) != gray.len() as u64 {
        return Err(RasterError::BadOutputSize { width, height });
    }
    let mut out = Vec::new();
    PngEncoder::new(&mut out)
        .write_image(gray, width, height, ExtendedColorType::L8)
        .map_err(|_| RasterError::EncodeFailed)?;
    Ok(out)
}
