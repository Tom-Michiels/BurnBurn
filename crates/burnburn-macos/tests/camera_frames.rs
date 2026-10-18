//! IM27 camera adapter, pure part (no camera is opened; the privacy prompt never appears in
//! automatic tests): AVFoundation authorisation codes, pixel-format FourCCs and BGRA frames with
//! row padding become the shared types.

#![allow(clippy::unwrap_used)]

use burnburn_macos::camera::{FrameError, bgra_frame, camera_access, pixel_format};
use burnburn_presentation::{CameraAuthorization as CameraAccess, CameraPixelFormat};

#[test]
fn authorisation_status_codes_map_and_unknown_codes_are_not_authorised() {
    // AVAuthorizationStatus: 0 notDetermined, 1 restricted, 2 denied, 3 authorized.
    assert_eq!(camera_access(0), CameraAccess::NotDetermined);
    assert_eq!(camera_access(1), CameraAccess::Restricted);
    assert_eq!(camera_access(2), CameraAccess::Denied);
    assert_eq!(camera_access(3), CameraAccess::Authorized);
    assert_eq!(camera_access(42), CameraAccess::Restricted, "an unknown status never grants access");
    assert_eq!(camera_access(-1), CameraAccess::Restricted);
}

#[test]
fn fourcc_codes_name_the_pixel_format() {
    assert_eq!(pixel_format(u32::from_be_bytes(*b"BGRA")), CameraPixelFormat::Bgra8);
    assert_eq!(pixel_format(u32::from_be_bytes(*b"420v")), CameraPixelFormat::Yuv420Video);
    let other = u32::from_be_bytes(*b"2vuy");
    assert_eq!(pixel_format(other), CameraPixelFormat::Other(other));
}

#[test]
fn bgra_rows_with_padding_become_an_rgba_source_image() {
    // 2 × 2 pixels, 12 bytes per row (4 bytes padding): B G R A order in memory.
    let data = [
        1, 2, 3, 255, 10, 20, 30, 128, 0xEE, 0xEE, 0xEE, 0xEE, //
        100, 110, 120, 255, 0, 0, 0, 0, 0xEE, 0xEE, 0xEE, 0xEE,
    ];
    let image = bgra_frame(2, 2, 12, &data).unwrap();
    assert_eq!((image.width(), image.height()), (2, 2));
    let px = |x, y| image.rgba_at(x, y).map(|c| c.round() as u8); // rgba_at is on the 0–255 scale
    assert_eq!(px(0, 0), [3, 2, 1, 255]);
    assert_eq!(px(1, 0), [30, 20, 10, 128]);
    assert_eq!(px(0, 1), [120, 110, 100, 255]);
    assert_eq!(px(1, 1), [0, 0, 0, 0]);
}

#[test]
fn inconsistent_frame_layouts_are_rejected() {
    assert_eq!(bgra_frame(2, 2, 7, &[0; 64]).err(), Some(FrameError::BadLayout), "row shorter than 4 × width");
    assert_eq!(bgra_frame(2, 2, 8, &[0; 15]).err(), Some(FrameError::BadLayout), "buffer too small");
    assert_eq!(bgra_frame(0, 2, 8, &[0; 16]).err(), Some(FrameError::BadLayout), "empty frame");
}
