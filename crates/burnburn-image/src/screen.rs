//! Screen and filter kernels (W1-L5a): round-dot halftone, Sobel sketch and unsharp masking.
//!
//! Arithmetic is integer where the definition allows it; the halftone spot function uses `libm`
//! trigonometry so Mac and Windows agree (DR-13).

use std::f64::consts::{PI, SQRT_2};

use burnburn_types::CancellationToken;

use crate::gray::GrayImage;
use crate::raster::{BitImage, MAX_RASTER_PIXELS, RasterError};

fn checked(img: &GrayImage) -> Result<(usize, usize), RasterError> {
    let (w, h) = (img.width as usize, img.height as usize);
    let n = u64::from(img.width) * u64::from(img.height);
    if w == 0 || h == 0 || n > MAX_RASTER_PIXELS || img.values.len() != w * h {
        return Err(RasterError::BadOutputSize { width: img.width, height: img.height });
    }
    Ok((w, h))
}

fn to_u8(v: f32) -> u8 {
    (v + 0.5).floor().clamp(0.0, 255.0) as u8
}

/// Fraction of the unit square (centred on the origin) that lies within distance `r` of its
/// centre: the cumulative distribution of the round-dot spot function.
fn disc_fraction(r: f64) -> f64 {
    if r <= 0.0 {
        0.0
    } else if r <= 0.5 {
        PI * r * r
    } else if r >= SQRT_2 / 2.0 {
        1.0
    } else {
        // The disc minus its four circular segments beyond the square's sides.
        PI * r * r - 4.0 * r * r * libm::acos(0.5 / r) + 2.0 * (r * r - 0.25).sqrt()
    }
}

/// Round-dot halftone (AM screen). The raster is cut into square cells of `cell_px` pixels whose
/// lattice is rotated by `angle_deg` counter-clockwise on screen. A pixel burns when its centre
/// lies inside the dot of its cell: the disc around the cell centre whose area fraction (within
/// the cell) equals the darkness `1 − v/255`; beyond mid-tone the discs merge into the corners.
/// Pure white never burns, pure black burns everywhere.
pub fn halftone(
    img: &GrayImage,
    cell_px: f64,
    angle_deg: f64,
    cancel: &CancellationToken,
) -> Result<BitImage, RasterError> {
    if !(cell_px.is_finite() && cell_px > 0.0) {
        return Err(RasterError::InvalidTone { field: "cell" });
    }
    if !angle_deg.is_finite() {
        return Err(RasterError::InvalidTone { field: "angle" });
    }
    let (w, h) = checked(img)?;
    let theta = angle_deg.to_radians();
    let (sin, cos) = (libm::sin(theta), libm::cos(theta));
    let mut bits = Vec::with_capacity(w * h);
    for y in 0..h {
        cancel.check()?;
        let py = y as f64 + 0.5;
        for x in 0..w {
            let px = x as f64 + 0.5;
            let darkness = 1.0 - f64::from(to_u8(img.values[y * w + x])) / 255.0;
            // Lattice axes e1 = (cos, −sin), e2 = (sin, cos) in y-down raster coordinates.
            let (u, v) = ((px * cos - py * sin) / cell_px, (px * sin + py * cos) / cell_px);
            let (fu, fv) = (u - libm::floor(u) - 0.5, v - libm::floor(v) - 0.5);
            let r = (fu * fu + fv * fv).sqrt();
            bits.push(darkness > 0.0 && darkness >= disc_fraction(r));
        }
    }
    BitImage::new(img.width, img.height, bits)
        .ok_or(RasterError::BadOutputSize { width: img.width, height: img.height })
}

/// Sobel edge sketch: gradient magnitude `m = min(255, ⌊√(gx² + gy²) / 4⌋)` of the 8-bit gray
/// values (borders replicate); a pixel burns when `m ≥ level`. A step of 255 gives `m = 255`.
pub fn sketch(img: &GrayImage, level: u8, cancel: &CancellationToken) -> Result<BitImage, RasterError> {
    let (w, h) = checked(img)?;
    let g: Vec<i64> = img.values.iter().map(|&v| i64::from(to_u8(v))).collect();
    let at = |x: i64, y: i64| g[y.clamp(0, h as i64 - 1) as usize * w + x.clamp(0, w as i64 - 1) as usize];
    let mut bits = Vec::with_capacity(w * h);
    for y in 0..h as i64 {
        cancel.check()?;
        for x in 0..w as i64 {
            let gx = (at(x + 1, y - 1) + 2 * at(x + 1, y) + at(x + 1, y + 1))
                - (at(x - 1, y - 1) + 2 * at(x - 1, y) + at(x - 1, y + 1));
            let gy = (at(x - 1, y + 1) + 2 * at(x, y + 1) + at(x + 1, y + 1))
                - (at(x - 1, y - 1) + 2 * at(x, y - 1) + at(x + 1, y - 1));
            let m = ((gx * gx + gy * gy) as u64).isqrt() / 4;
            bits.push(m.min(255) >= u64::from(level));
        }
    }
    BitImage::new(img.width, img.height, bits)
        .ok_or(RasterError::BadOutputSize { width: img.width, height: img.height })
}

/// Unsharp mask: `v + amount · (v − box_mean_r(v))`, where the mean covers the `(2r+1)²` window
/// with replicated borders. The result is rounded half up and clamped to `0..=255`. `radius 0`
/// or `amount 0` return the image unchanged.
pub fn unsharp(
    img: &GrayImage,
    radius: u32,
    amount: f64,
    cancel: &CancellationToken,
) -> Result<GrayImage, RasterError> {
    if !(amount.is_finite() && amount >= 0.0) {
        return Err(RasterError::InvalidTone { field: "amount" });
    }
    let (w, h) = checked(img)?;
    if radius == 0 || amount == 0.0 {
        return Ok(img.clone());
    }
    let r = i64::from(radius);
    let v: Vec<f64> = img.values.iter().map(|&p| f64::from(p)).collect();
    // Separable box sums with replicated borders: exact for integral inputs.
    let mut horizontal = vec![0.0f64; w * h];
    for y in 0..h {
        cancel.check()?;
        for x in 0..w as i64 {
            horizontal[y * w + x as usize] =
                (-r..=r).map(|d| v[y * w + (x + d).clamp(0, w as i64 - 1) as usize]).sum::<f64>();
        }
    }
    let area = ((2 * r + 1) * (2 * r + 1)) as f64;
    let mut values = Vec::with_capacity(w * h);
    for y in 0..h as i64 {
        cancel.check()?;
        for x in 0..w {
            let sum: f64 = (-r..=r).map(|d| horizontal[(y + d).clamp(0, h as i64 - 1) as usize * w + x]).sum();
            let p = v[y as usize * w + x];
            let out = p + amount * (p - sum / area);
            values.push(libm::floor(out + 0.5).clamp(0.0, 255.0) as f32);
        }
    }
    Ok(GrayImage { width: img.width, height: img.height, values })
}
