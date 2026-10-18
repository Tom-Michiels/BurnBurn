//! Raster production (IM18, module owner CAM/job): area resampling of the *original* source
//! pixels to the physical output grid, tone correction and deterministic dithering to a
//! [`BitImage`].
//!
//! Everything here is integer arithmetic except the 256-entry tone table, which uses `libm`
//! (DR-13/DR-19), so Mac and Windows produce bit-identical results. Dithering is never stored
//! as document data: the compiler regenerates it from the source for the physical size and
//! pitch of the operation (`dither_runs_are_regenerated_from_source_after_physical_resize`).

use std::fmt;

use burnburn_types::{CancellationToken, Cancelled};

use crate::adjust::PixelRect;
use crate::gray::{AlphaPolicy, GrayConversion, GrayImage, to_gray_with};
use crate::source::SourceImage;

/// Upper bound on output pixels of one raster (keeps a setting from making a compile unbounded).
pub const MAX_RASTER_PIXELS: u64 = 200_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RasterError {
    /// A tone parameter outside its documented range (or not finite).
    InvalidTone {
        field: &'static str,
    },
    /// Ordered dithering supports Bayer matrices of size 2, 4 and 8.
    UnsupportedOrderedSize {
        size: u8,
    },
    /// Zero output width/height, or more than [`MAX_RASTER_PIXELS`].
    BadOutputSize {
        width: u32,
        height: u32,
    },
    /// A crop rectangle with no area or outside the source image.
    CropOutOfBounds,
    /// The PNG encoder rejected the pixels.
    EncodeFailed,
    Cancelled,
}

impl From<Cancelled> for RasterError {
    fn from(_: Cancelled) -> Self {
        RasterError::Cancelled
    }
}

impl fmt::Display for RasterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RasterError::InvalidTone { field } => write!(f, "tone {field} is out of range"),
            RasterError::UnsupportedOrderedSize { size } => write!(f, "ordered dither size {size} is not 2, 4 or 8"),
            RasterError::BadOutputSize { width, height } => write!(f, "raster output {width}×{height} is not allowed"),
            RasterError::EncodeFailed => f.write_str("the processed bitmap could not be encoded"),
            RasterError::CropOutOfBounds => f.write_str("crop rectangle is empty or outside the image"),
            RasterError::Cancelled => f.write_str("raster cancelled"),
        }
    }
}

impl std::error::Error for RasterError {}

/// Tone correction on gray values `v ∈ [0, 1]` (0 = black):
/// `out = clamp(contrast · (v − ½) + ½ + brightness, 0, 1) ^ gamma`.
///
/// Ranges: `gamma ∈ [0.1, 10]`, `brightness ∈ [−1, 1]`, `contrast ∈ [0, 10]`. Identity:
/// gamma 1, brightness 0, contrast 1. `gamma > 1` darkens mid-tones (more burning).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToneCurve {
    gamma: f64,
    brightness: f64,
    contrast: f64,
}

impl ToneCurve {
    pub const IDENTITY: ToneCurve = ToneCurve { gamma: 1.0, brightness: 0.0, contrast: 1.0 };

    pub fn new(gamma: f64, brightness: f64, contrast: f64) -> Result<Self, RasterError> {
        let within = |v: f64, lo: f64, hi: f64| v.is_finite() && v >= lo && v <= hi;
        if !within(gamma, 0.1, 10.0) {
            return Err(RasterError::InvalidTone { field: "gamma" });
        }
        if !within(brightness, -1.0, 1.0) {
            return Err(RasterError::InvalidTone { field: "brightness" });
        }
        if !within(contrast, 0.0, 10.0) {
            return Err(RasterError::InvalidTone { field: "contrast" });
        }
        Ok(ToneCurve { gamma, brightness, contrast })
    }

    pub fn gamma(&self) -> f64 {
        self.gamma
    }
    pub fn brightness(&self) -> f64 {
        self.brightness
    }
    pub fn contrast(&self) -> f64 {
        self.contrast
    }

    /// The curve as a table over 8-bit gray values (round half up).
    pub fn table(&self) -> [u8; 256] {
        let mut t = [0u8; 256];
        for (i, out) in t.iter_mut().enumerate() {
            let v = i as f64 / 255.0;
            let c = (self.contrast * (v - 0.5) + 0.5 + self.brightness).clamp(0.0, 1.0);
            let g = libm::pow(c, self.gamma);
            *out = libm::floor(g * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
        }
        t
    }
}

/// How gray becomes on/off dots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DitherAlgorithm {
    /// Burn where the toned value is below `level`.
    Threshold {
        level: u8,
    },
    FloydSteinberg,
    Jarvis,
    Stucki,
    Atkinson,
    /// Bayer matrix of `size` 2, 4 or 8.
    Ordered {
        size: u8,
    },
}

/// A binary raster, row-major, top row first; `true` = laser on (dark).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BitImage {
    width: u32,
    height: u32,
    bits: Vec<bool>,
}

impl BitImage {
    /// `bits` holds `width × height` entries, row-major, top row first; `None` on a length mismatch.
    pub fn new(width: u32, height: u32, bits: Vec<bool>) -> Option<Self> {
        (u64::from(width) * u64::from(height) == bits.len() as u64).then_some(BitImage { width, height, bits })
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    /// `false` outside the image.
    pub fn get(&self, x: u32, y: u32) -> bool {
        x < self.width && y < self.height && self.bits[y as usize * self.width as usize + x as usize]
    }
    pub fn bits(&self) -> &[bool] {
        &self.bits
    }
    /// Number of `true` pixels.
    pub fn count_on(&self) -> usize {
        self.bits.iter().filter(|b| **b).count()
    }
}

fn check_size(width: u32, height: u32) -> Result<(), RasterError> {
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_RASTER_PIXELS {
        return Err(RasterError::BadOutputSize { width, height });
    }
    Ok(())
}

/// Area-average resampling of the whole `src` (gray after `alpha`) onto an `out_w × out_h`
/// grid. Exact integer box weights in units of `1 / (out_w · out_h)` source pixels; the result
/// is rounded half up, so values are integral `0..=255`. Works from the original pixels only.
pub fn resample_area(
    src: &SourceImage,
    alpha: AlphaPolicy,
    out_w: u32,
    out_h: u32,
    cancel: &CancellationToken,
) -> Result<GrayImage, RasterError> {
    resample_area_with(src, None, alpha, GrayConversion::Rec709, out_w, out_h, cancel)
}

/// [`resample_area`] over the `crop` window of `src` (the whole image for `None`), converting
/// colour with `conversion`. A crop is a pixel rectangle of the original; no pixel outside it
/// contributes.
pub fn resample_area_with(
    src: &SourceImage,
    crop: Option<PixelRect>,
    alpha: AlphaPolicy,
    conversion: GrayConversion,
    out_w: u32,
    out_h: u32,
    cancel: &CancellationToken,
) -> Result<GrayImage, RasterError> {
    check_size(out_w, out_h)?;
    let window = crop.unwrap_or(PixelRect { x: 0, y: 0, width: src.width(), height: src.height() });
    window.validate_within(src.width(), src.height()).map_err(|_| RasterError::CropOutOfBounds)?;
    let (sw, sh) = (u64::from(window.width), u64::from(window.height));
    let (ow, oh) = (u64::from(out_w), u64::from(out_h));
    let mut gray = Vec::with_capacity((sw * sh) as usize);
    for y in window.y..window.y + window.height {
        for x in window.x..window.x + window.width {
            let g = to_gray_with(src.rgba_at(x, y), alpha, conversion);
            gray.push((g + 0.5).floor().clamp(0.0, 255.0) as u8);
        }
    }
    // Source pixel s spans [s·o, (s+1)·o); output pixel k spans [k·s_n, (k+1)·s_n).
    let overlap = |s: u64, o: u64, k: u64, n: u64| -> u64 {
        let (a0, a1, b0, b1) = (s * o, (s + 1) * o, k * n, (k + 1) * n);
        a1.min(b1).saturating_sub(a0.max(b0))
    };
    let total = u128::from(sw) * u128::from(sh);
    let mut values = Vec::with_capacity((ow * oh) as usize);
    for oy in 0..oh {
        cancel.check()?;
        let (y0, y1) = ((oy * sh) / oh, ((oy + 1) * sh).div_ceil(oh));
        for ox in 0..ow {
            let (x0, x1) = ((ox * sw) / ow, ((ox + 1) * sw).div_ceil(ow));
            let mut sum: u128 = 0;
            for sy in y0..y1.min(sh) {
                let wy = overlap(sy, oh, oy, sh);
                for sx in x0..x1.min(sw) {
                    let wx = overlap(sx, ow, ox, sw);
                    let g = gray[(sy * sw + sx) as usize];
                    sum += u128::from(g) * u128::from(wx) * u128::from(wy);
                }
            }
            // Weights sum to sw·sh per output pixel (in units of ow·oh).
            let v = (2 * sum + total) / (2 * total);
            values.push(v.min(255) as f32);
        }
    }
    Ok(GrayImage { width: out_w, height: out_h, values })
}

/// Applies `tone` to every pixel (values rounded half up to 8 bit first).
pub fn tone_map(img: &GrayImage, tone: &ToneCurve) -> Vec<u8> {
    let table = tone.table();
    img.values.iter().map(|&v| table[(v + 0.5).floor().clamp(0.0, 255.0) as usize]).collect()
}

/// Error-diffusion kernel: `(dx, dy, weight)` and divisor.
type Kernel = (&'static [(i32, i32, i32)], i32);

const FLOYD_STEINBERG: Kernel = (&[(1, 0, 7), (-1, 1, 3), (0, 1, 5), (1, 1, 1)], 16);
const JARVIS: Kernel = (
    &[
        (1, 0, 7),
        (2, 0, 5),
        (-2, 1, 3),
        (-1, 1, 5),
        (0, 1, 7),
        (1, 1, 5),
        (2, 1, 3),
        (-2, 2, 1),
        (-1, 2, 3),
        (0, 2, 5),
        (1, 2, 3),
        (2, 2, 1),
    ],
    48,
);
const STUCKI: Kernel = (
    &[
        (1, 0, 8),
        (2, 0, 4),
        (-2, 1, 2),
        (-1, 1, 4),
        (0, 1, 8),
        (1, 1, 4),
        (2, 1, 2),
        (-2, 2, 1),
        (-1, 2, 2),
        (0, 2, 4),
        (1, 2, 2),
        (2, 2, 1),
    ],
    42,
);
/// Atkinson diffuses only 6/8 of the error.
const ATKINSON: Kernel = (&[(1, 0, 1), (2, 0, 1), (-1, 1, 1), (0, 1, 1), (1, 1, 1), (0, 2, 1)], 8);

/// Dithers `img` after `tone`. Integer error diffusion (`i32`, shares truncated toward zero),
/// so the result is bit-identical on every platform. A pixel burns when its (error-adjusted)
/// value is below 128; `serpentine` processes odd rows right to left with a mirrored kernel.
pub fn dither(
    img: &GrayImage,
    tone: &ToneCurve,
    algorithm: DitherAlgorithm,
    serpentine: bool,
    cancel: &CancellationToken,
) -> Result<BitImage, RasterError> {
    check_size(img.width, img.height)?;
    let (w, h) = (img.width as usize, img.height as usize);
    if img.values.len() != w * h {
        return Err(RasterError::BadOutputSize { width: img.width, height: img.height });
    }
    let toned = tone_map(img, tone);
    let kernel = match algorithm {
        DitherAlgorithm::Threshold { level } => {
            let bits = toned.iter().map(|&v| v < level).collect();
            return Ok(BitImage { width: img.width, height: img.height, bits });
        }
        DitherAlgorithm::Ordered { size } => return ordered(img.width, img.height, &toned, size),
        DitherAlgorithm::FloydSteinberg => FLOYD_STEINBERG,
        DitherAlgorithm::Jarvis => JARVIS,
        DitherAlgorithm::Stucki => STUCKI,
        DitherAlgorithm::Atkinson => ATKINSON,
    };
    let (taps, divisor) = kernel;
    let mut buf: Vec<i32> = toned.iter().map(|&v| i32::from(v)).collect();
    let mut bits = vec![false; w * h];
    for y in 0..h {
        cancel.check()?;
        let reverse = serpentine && y % 2 == 1;
        for i in 0..w {
            let x = if reverse { w - 1 - i } else { i };
            let v = buf[y * w + x];
            let burn = v < 128;
            bits[y * w + x] = burn;
            let err = v - if burn { 0 } else { 255 };
            if err == 0 {
                continue;
            }
            for &(dx, dy, weight) in taps {
                let dx = if reverse { -dx } else { dx };
                let (nx, ny) = (x as i64 + i64::from(dx), y as i64 + i64::from(dy));
                if nx < 0 || nx >= w as i64 || ny >= h as i64 {
                    continue;
                }
                buf[ny as usize * w + nx as usize] += err * weight / divisor;
            }
        }
    }
    Ok(BitImage { width: img.width, height: img.height, bits })
}

/// Bayer index matrix of size `n` (power of two), values `0..n²`.
fn bayer(n: usize) -> Vec<u32> {
    let mut m = vec![0u32];
    let mut size = 1;
    while size < n {
        let mut next = vec![0u32; 4 * size * size];
        for y in 0..size {
            for x in 0..size {
                let v = 4 * m[y * size + x];
                let s2 = 2 * size;
                next[y * s2 + x] = v;
                next[y * s2 + x + size] = v + 2;
                next[(y + size) * s2 + x] = v + 3;
                next[(y + size) * s2 + x + size] = v + 1;
            }
        }
        m = next;
        size *= 2;
    }
    m
}

/// Ordered dither: pixel burns when `v + ½ < (M + ½) · 256 / n²`, evaluated in integers.
fn ordered(width: u32, height: u32, toned: &[u8], size: u8) -> Result<BitImage, RasterError> {
    if !matches!(size, 2 | 4 | 8) {
        return Err(RasterError::UnsupportedOrderedSize { size });
    }
    let n = usize::from(size);
    let m = bayer(n);
    let n2 = (n * n) as u32;
    let w = width as usize;
    let bits = toned
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let (x, y) = (i % w, i / w);
            let idx = m[(y % n) * n + (x % n)];
            (2 * u32::from(v) + 1) * n2 < (2 * idx + 1) * 256
        })
        .collect();
    Ok(BitImage { width, height, bits })
}
