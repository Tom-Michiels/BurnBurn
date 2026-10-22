//! G5-windows: the headless half of the Media Foundation camera source (format choice, errors as
//! typed states, authorization, frame conversion). The Media Foundation calls are Windows-only and
//! not run here (manual).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_presentation::{
    CameraAuthorization, CameraDevice, CameraDeviceId, CameraError, CameraMode, CameraPixelFormat,
};
use burnburn_windows::host_panels::camera_source::{
    AccessState, DeviceCache, FrameError, MfFailure, NativeMode, NativeSubtype, camera_error, camera_format,
    camera_modes, choose_native, rgb32_frame, subtype_from_data1, unpack_pair,
};

fn mode(w: u32, h: u32, num: u32, den: u32, subtype: NativeSubtype) -> NativeMode {
    NativeMode { width: w, height: h, fps_num: num, fps_den: den, subtype }
}

#[test]
fn subtypes_are_recognised_from_the_first_field_of_the_mf_guid() {
    // MFVideoFormat_RGB24 / ARGB32 / RGB32 are D3DFMT 20 / 21 / 22; the others are FourCCs.
    assert_eq!(subtype_from_data1(20), NativeSubtype::Rgb24);
    assert_eq!(subtype_from_data1(21), NativeSubtype::Argb32);
    assert_eq!(subtype_from_data1(22), NativeSubtype::Rgb32);
    assert_eq!(subtype_from_data1(u32::from_le_bytes(*b"NV12")), NativeSubtype::Nv12);
    assert_eq!(subtype_from_data1(u32::from_le_bytes(*b"YUY2")), NativeSubtype::Yuy2);
    assert_eq!(subtype_from_data1(u32::from_le_bytes(*b"MJPG")), NativeSubtype::Mjpg);
    let odd = u32::from_le_bytes(*b"H264");
    assert_eq!(subtype_from_data1(odd), NativeSubtype::Other(odd));
}

#[test]
fn subtypes_map_to_the_shared_pixel_formats() {
    assert_eq!(camera_format(NativeSubtype::Rgb32), CameraPixelFormat::Bgra8);
    assert_eq!(camera_format(NativeSubtype::Argb32), CameraPixelFormat::Bgra8);
    assert_eq!(camera_format(NativeSubtype::Rgb24), CameraPixelFormat::Rgb24);
    assert_eq!(camera_format(NativeSubtype::Nv12), CameraPixelFormat::Yuv420Video);
    assert_eq!(camera_format(NativeSubtype::Yuy2), CameraPixelFormat::Yuy2);
    let mjpg = u32::from_le_bytes(*b"MJPG");
    assert_eq!(camera_format(NativeSubtype::Mjpg), CameraPixelFormat::Other(mjpg));
    assert_eq!(camera_format(NativeSubtype::Other(7)), CameraPixelFormat::Other(7));
}

#[test]
fn a_packed_size_or_rate_has_the_first_value_in_the_high_half() {
    assert_eq!(unpack_pair((1280u64 << 32) | 720), (1280, 720));
    assert_eq!(unpack_pair((30_000u64 << 32) | 1001), (30_000, 1001));
    assert_eq!(unpack_pair(0), (0, 0));
}

#[test]
fn modes_carry_the_frame_rate_in_thousandths_without_duplicates_biggest_first() {
    let native = [
        mode(640, 480, 30, 1, NativeSubtype::Yuy2),
        mode(1920, 1080, 30_000, 1001, NativeSubtype::Nv12),
        mode(1920, 1080, 30_000, 1001, NativeSubtype::Nv12),
        mode(1920, 1080, 60, 1, NativeSubtype::Nv12),
        mode(640, 480, 15, 1, NativeSubtype::Yuy2),
    ];
    let got: Vec<(u32, u32, u32)> = camera_modes(&native).iter().map(|m| (m.width, m.height, m.fps_milli)).collect();
    assert_eq!(got, [(1920, 1080, 60_000), (1920, 1080, 29_970), (640, 480, 30_000), (640, 480, 15_000)]);
    assert_eq!(camera_modes(&native)[0].pixel_format, CameraPixelFormat::Yuv420Video);
}

#[test]
fn a_mode_without_a_valid_frame_rate_or_size_is_dropped() {
    let native = [
        mode(640, 480, 30, 0, NativeSubtype::Yuy2),
        mode(0, 480, 30, 1, NativeSubtype::Yuy2),
        mode(640, 480, 30, 1, NativeSubtype::Yuy2),
    ];
    assert_eq!(camera_modes(&native).len(), 1);
}

#[test]
fn the_requested_mode_is_found_by_size_format_and_nearly_equal_frame_rate() {
    let native = [
        mode(640, 480, 30, 1, NativeSubtype::Yuy2),
        mode(1920, 1080, 30_000, 1001, NativeSubtype::Nv12),
        mode(1920, 1080, 30_000, 1001, NativeSubtype::Mjpg),
    ];
    let asked = camera_modes(&native);
    for m in &asked {
        let i = choose_native(m, &native).unwrap();
        assert_eq!((native[i].width, native[i].height), (m.width, m.height));
        assert_eq!(camera_format(native[i].subtype), m.pixel_format);
    }
    // 29.97 and a request for 30.0 are the same mode.
    let thirty =
        CameraMode { width: 1920, height: 1080, fps_milli: 30_000, pixel_format: CameraPixelFormat::Yuv420Video };
    assert_eq!(choose_native(&thirty, &native), Ok(1));
}

#[test]
fn a_mode_the_camera_does_not_have_is_unsupported_not_a_silent_neighbour() {
    let native = [mode(640, 480, 30, 1, NativeSubtype::Yuy2)];
    let other_size = CameraMode { width: 800, height: 600, fps_milli: 30_000, pixel_format: CameraPixelFormat::Yuy2 };
    let other_rate = CameraMode { width: 640, height: 480, fps_milli: 60_000, pixel_format: CameraPixelFormat::Yuy2 };
    let other_format =
        CameraMode { width: 640, height: 480, fps_milli: 30_000, pixel_format: CameraPixelFormat::Bgra8 };
    for asked in [other_size, other_rate, other_format] {
        assert_eq!(choose_native(&asked, &native), Err(CameraError::ModeUnsupported), "{asked:?}");
    }
    assert_eq!(choose_native(&other_size, &[]), Err(CameraError::ModeUnsupported));
}

#[test]
fn failures_become_typed_states() {
    assert_eq!(camera_error(MfFailure::AccessDenied), CameraError::PermissionDenied);
    assert_eq!(camera_error(MfFailure::InUse), CameraError::Busy);
    assert_eq!(camera_error(MfFailure::DeviceGone), CameraError::DeviceGone);
    assert_eq!(camera_error(MfFailure::UnsupportedType), CameraError::ModeUnsupported);
    assert_eq!(camera_error(MfFailure::Other(-5)), CameraError::Platform { code: -5 });
}

#[test]
fn access_is_authorized_until_a_start_is_refused_and_a_request_tries_again() {
    let mut access = AccessState::default();
    assert_eq!(access.authorization(), CameraAuthorization::Authorized, "no prompt exists on Windows");
    access.note_start(&Err(CameraError::Busy));
    assert_eq!(access.authorization(), CameraAuthorization::Authorized, "another app having it is no denial");
    access.note_start(&Err(CameraError::PermissionDenied));
    assert_eq!(access.authorization(), CameraAuthorization::Denied);
    access.request();
    assert_eq!(access.authorization(), CameraAuthorization::Authorized, "the next start finds out again");
    access.note_start(&Err(CameraError::PermissionDenied));
    access.note_start(&Ok(()));
    assert_eq!(access.authorization(), CameraAuthorization::Authorized);
}

#[test]
fn an_rgb32_frame_becomes_opaque_rgba_and_ignores_the_row_padding() {
    // 2x2, stride 12 (8 bytes of pixels + 4 padding). Pixels B,G,R,X; X is garbage on purpose.
    let data = [
        10, 20, 30, 0, 40, 50, 60, 7, 99, 99, 99, 99, // row 0
        70, 80, 90, 1, 100, 110, 120, 2, 99, 99, 99, 99, // row 1
    ];
    let image = rgb32_frame(2, 2, 12, &data).unwrap();
    assert_eq!((image.width(), image.height()), (2, 2));
    assert_eq!(image.samples(), [30, 20, 10, 255, 60, 50, 40, 255, 90, 80, 70, 255, 120, 110, 100, 255]);
}

#[test]
fn a_negative_stride_is_a_bottom_up_picture_and_comes_out_top_down() {
    // Memory holds the bottom row first.
    let data = [70, 80, 90, 0, 100, 110, 120, 0, 10, 20, 30, 0, 40, 50, 60, 0];
    let image = rgb32_frame(2, 2, -8, &data).unwrap();
    assert_eq!(image.samples(), [30, 20, 10, 255, 60, 50, 40, 255, 90, 80, 70, 255, 120, 110, 100, 255]);
}

#[test]
fn a_frame_that_does_not_fit_its_description_is_refused() {
    assert_eq!(rgb32_frame(2, 2, 8, &[0; 15]).unwrap_err(), FrameError::BadLayout, "buffer too short");
    assert_eq!(rgb32_frame(2, 2, 4, &[0; 16]).unwrap_err(), FrameError::BadLayout, "stride below a row");
    assert_eq!(rgb32_frame(0, 2, 8, &[0; 16]).unwrap_err(), FrameError::BadLayout, "no width");
    assert_eq!(rgb32_frame(2, 0, 8, &[0; 16]).unwrap_err(), FrameError::BadLayout, "no height");
    assert_eq!(rgb32_frame(2, 2, 0, &[0; 16]).unwrap_err(), FrameError::BadLayout, "stride zero");
}

fn cam(name: &str) -> CameraDevice {
    CameraDevice { id: CameraDeviceId(name.into()), name: name.into(), modes: Vec::new() }
}

#[test]
fn the_device_list_is_listed_once_per_ttl_and_not_on_every_refresh() {
    let mut cache = DeviceCache::new(5_000);
    let mut calls = 0;
    let list = |cache: &mut DeviceCache, now: u64, streaming: bool, calls: &mut u32| {
        cache
            .get(now, streaming, || {
                *calls += 1;
                Ok(vec![cam(&format!("cam{calls}"))])
            })
            .unwrap()
    };
    assert_eq!(list(&mut cache, 0, false, &mut calls)[0].name, "cam1");
    assert_eq!(list(&mut cache, 4_999, false, &mut calls)[0].name, "cam1", "still fresh");
    assert_eq!(calls, 1);
    assert_eq!(list(&mut cache, 5_000, false, &mut calls)[0].name, "cam2", "expired");
    cache.invalidate();
    assert_eq!(list(&mut cache, 5_001, false, &mut calls)[0].name, "cam3", "invalidated");
}

#[test]
fn while_a_camera_streams_the_stored_list_is_kept_even_when_it_is_old() {
    let mut cache = DeviceCache::new(1_000);
    cache.get(0, false, || Ok(vec![cam("a")])).unwrap();
    let kept = cache.get(60_000, true, || panic!("the open camera must not be listed again")).unwrap();
    assert_eq!(kept[0].name, "a");
    // Streaming without any stored list has to list.
    let mut empty = DeviceCache::new(1_000);
    assert_eq!(empty.get(0, true, || Ok(vec![cam("b")])).unwrap()[0].name, "b");
}

#[test]
fn a_failed_listing_is_returned_and_not_stored() {
    let mut cache = DeviceCache::new(5_000);
    assert_eq!(cache.get(0, false, || Err(CameraError::Platform { code: 1 })), Err(CameraError::Platform { code: 1 }));
    assert_eq!(cache.get(1, false, || Ok(vec![cam("a")])).unwrap()[0].name, "a", "tried again at once");
}
