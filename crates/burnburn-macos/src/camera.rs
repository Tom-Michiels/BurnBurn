//! IM27 camera, host side without AVFoundation: the pure conversions the AVFoundation adapter
//! (`appkit::camera`, feature `camera`) uses for `burnburn_presentation::CameraPort`. Camera
//! pixels are a placement aid only: nothing here feeds CAM.

use burnburn_image::SourceImage;
use burnburn_presentation::{CameraAuthorization, CameraPixelFormat};

/// `AVAuthorizationStatus` raw value → presentation's authorization; anything unknown is treated
/// as no access.
pub fn camera_access(status: isize) -> CameraAuthorization {
    match status {
        0 => CameraAuthorization::NotDetermined,
        2 => CameraAuthorization::Denied,
        3 => CameraAuthorization::Authorized,
        _ => CameraAuthorization::Restricted,
    }
}

/// CoreVideo pixel-format FourCC → shared pixel format.
pub fn pixel_format(fourcc: u32) -> CameraPixelFormat {
    match &fourcc.to_be_bytes() {
        b"BGRA" => CameraPixelFormat::Bgra8,
        b"420v" => CameraPixelFormat::Yuv420Video,
        _ => CameraPixelFormat::Other(fourcc),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    /// Size, stride and buffer length do not describe a BGRA frame.
    BadLayout,
}

/// A locked `CVPixelBuffer` in BGRA (rows of `bytes_per_row`, possibly padded) → RGBA image.
pub fn bgra_frame(width: u32, height: u32, bytes_per_row: usize, data: &[u8]) -> Result<SourceImage, FrameError> {
    let row = (width as usize).checked_mul(4).ok_or(FrameError::BadLayout)?;
    let needed = bytes_per_row
        .checked_mul((height as usize).saturating_sub(1))
        .and_then(|n| n.checked_add(row))
        .ok_or(FrameError::BadLayout)?;
    if width == 0 || height == 0 || bytes_per_row < row || data.len() < needed {
        return Err(FrameError::BadLayout);
    }
    let mut rgba = Vec::with_capacity(row * height as usize);
    for y in 0..height as usize {
        let start = y * bytes_per_row;
        for px in data[start..start + row].as_chunks::<4>().0 {
            rgba.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
        }
    }
    SourceImage::from_rgba8(width, height, rgba).map_err(|_| FrameError::BadLayout)
}
