//! G7-werk, W04: camera calibrations (lens calibration and workspace alignment) survive a restart.
//! The file is written atomically through the filesystem port; a newer schema is reported with its
//! version, damage is a typed error and never an empty store, and what is read back is bit-identical
//! (checked through the store's canonical encoding, which does not depend on the encoder under test).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use burnburn_image::calib::{
    CalibrationStore, CameraCalibration, CameraConfigKey, CameraDeviceId, CameraMode, CameraPixelFormat, CheckPoint,
    Intrinsics, LensCalibration, LensFitReport, ResidualStats, WorkspaceAlignment,
};
use burnburn_io::LoadError;
use burnburn_io::camera_calibration_library::{
    CAMERA_CALIBRATION_SCHEMA, decode_camera_calibrations, encode_camera_calibrations, load_camera_calibrations,
    save_camera_calibrations,
};
use burnburn_io::fs::{FileSystem, FsOp};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_types::{CanonicalEncode, CanonicalWriter, RequestId, StableId};

fn config(device: &str, w: u32, h: u32, format: CameraPixelFormat, lens: &str) -> CameraConfigKey {
    CameraConfigKey {
        device: CameraDeviceId(device.into()),
        mode: CameraMode { width: w, height: h, pixel_format: format, frame_rate_mhz: 29_970 },
        lens: lens.into(),
    }
}

fn lens(cfg: CameraConfigKey, k1: f64) -> LensCalibration {
    LensCalibration {
        request: RequestId::from_raw(41),
        intrinsics: Intrinsics { center_px: [960.5, 540.25], radius_px: 1101.9, k1, k2: 0.0123456789012345 },
        valid_for: cfg,
        report: LensFitReport {
            views: 5,
            points: 175,
            rms_px: 0.31,
            max_px: 0.9,
            rms_plane_mm: 0.07,
            evaluations: 88,
            converged: true,
        },
    }
}

fn alignment(cfg: CameraConfigKey, height_mm: f64, with_lens: bool) -> WorkspaceAlignment {
    WorkspaceAlignment {
        calibration: CameraCalibration {
            valid_for: cfg.clone(),
            intrinsics: Intrinsics { center_px: [960.0, 540.0], radius_px: 1101.9, k1: -0.0911, k2: 0.0 },
            bed_homography: [[0.4, 0.001, -12.3], [0.002, 0.41, -8.7], [1e-6, -2e-6, 1.0]],
            fit_residuals_mm: ResidualStats { count: 12, rms_mm: 0.11, max_mm: 0.25 },
            residuals_mm: ResidualStats { count: 2, rms_mm: 0.19, max_mm: 0.22 },
            check_points: vec![CheckPoint { pixel: [100.5, 200.25], bed_mm: [30.0, 40.0], error_mm: 0.19 }],
        },
        material_height_mm: height_mm,
        lens_calibration: with_lens.then(|| lens(cfg, -0.0911)),
    }
}

fn cam_a() -> CameraConfigKey {
    config("cam-A", 1920, 1080, CameraPixelFormat::Bgra8, "standard")
}

fn cam_b() -> CameraConfigKey {
    config("cam-B", 1280, 720, CameraPixelFormat::Yuv420Video, "wide")
}

fn filled() -> CalibrationStore {
    let mut store = CalibrationStore::default();
    store.insert_lens(lens(cam_a(), -0.0911));
    store.insert_lens(lens(cam_b(), -0.2));
    store.insert_alignment(alignment(cam_a(), 0.0, true));
    store.insert_alignment(alignment(cam_a(), 12.5, true));
    store.insert_alignment(alignment(
        config("cam-B", 1280, 720, CameraPixelFormat::Other(0x7975_7673), "wide"),
        3.0,
        false,
    ));
    store
}

/// The canonical form leaves out the runtime request id, so two stores are equal when they say the
/// same about the camera and the bed.
fn canon(store: &CalibrationStore) -> Vec<u8> {
    let mut w = CanonicalWriter::new();
    store.encode(&mut w);
    w.finish().unwrap()
}

#[test]
fn lenses_and_alignments_survive_a_save_and_load_bit_for_bit() {
    let fs = MemoryFileSystem::new();
    let path = Path::new("/lib/camera-calibration.json");
    let store = filled();
    save_camera_calibrations(&fs, path, &store).unwrap();
    let back = load_camera_calibrations(&fs, path).unwrap();
    assert_eq!((back.lens_count(), back.alignment_count()), (2, 3));
    assert_eq!(canon(&back), canon(&store));
}

#[test]
fn a_loaded_calibration_applies_to_its_own_configuration_and_to_no_other() {
    let back = decode_camera_calibrations(&encode_camera_calibrations(&filled()).unwrap()).unwrap();
    let found = back.alignment_for(&cam_a(), 12.5).unwrap();
    assert!(found.pixel_to_bed(&cam_a(), [10.0, 10.0]).is_ok());
    // Same camera at another resolution, another source, another lens profile: never served.
    let other_res = config("cam-A", 1280, 720, CameraPixelFormat::Bgra8, "standard");
    assert!(back.alignment_for(&other_res, 12.5).is_err());
    let other_cam = config("cam-C", 1920, 1080, CameraPixelFormat::Bgra8, "standard");
    assert!(back.alignment_for(&other_cam, 12.5).is_err());
    assert!(back.lens_for(&config("cam-A", 1920, 1080, CameraPixelFormat::Bgra8, "tele")).is_err());
    assert!(back.alignment_for(&cam_a(), 7.0).is_err(), "a height that was never calibrated");
}

#[test]
fn the_request_id_is_a_runtime_correlation_and_is_not_stored() {
    let back = decode_camera_calibrations(&encode_camera_calibrations(&filled()).unwrap()).unwrap();
    assert_eq!(back.lens_count(), 2);
    assert!(back.lenses().all(|l| l.request == RequestId::from_raw(0)));
}

#[test]
fn the_file_uses_its_documented_names() {
    // Typed out by hand: the names of the format are pinned independently of the encoder.
    let bytes = br#"{"format":"burnburn-camera-calibrations","schema":1,
      "lenses":[{"config":{"device":"cam-X","width":640,"height":480,"pixel_format":{"kind":"yuv420-video"},
                           "frame_rate_mhz":30000,"lens":"standard"},
                 "center_px":[320.0,240.0],"radius_px":400.0,"k1":-0.05,"k2":0.01,
                 "report":{"views":3,"points":99,"rms_px":0.4,"max_px":1.0,"rms_plane_mm":0.1,
                           "evaluations":40,"converged":true}}],
      "alignments":[]}"#;
    let store = decode_camera_calibrations(bytes).unwrap();
    let cfg = CameraConfigKey {
        device: CameraDeviceId("cam-X".into()),
        mode: CameraMode {
            width: 640,
            height: 480,
            pixel_format: CameraPixelFormat::Yuv420Video,
            frame_rate_mhz: 30_000,
        },
        lens: "standard".into(),
    };
    let lens = store.lens_for(&cfg).unwrap();
    assert_eq!((lens.intrinsics.k1, lens.intrinsics.k2, lens.report.views), (-0.05, 0.01, 3));
    assert_eq!(lens.intrinsics.center_px, [320.0, 240.0]);
}

#[test]
fn a_newer_schema_a_foreign_file_and_damage_are_refused_not_read_as_empty() {
    let text = String::from_utf8(encode_camera_calibrations(&filled()).unwrap()).unwrap();
    assert!(text.contains("\"lenses\""), "the encoder writes the lenses");
    let newer = text.replace(
        &format!("\"schema\":{CAMERA_CALIBRATION_SCHEMA}"),
        &format!("\"schema\":{}", CAMERA_CALIBRATION_SCHEMA + 1),
    );
    assert_ne!(newer, text);
    assert_eq!(
        decode_camera_calibrations(newer.as_bytes()).unwrap_err(),
        LoadError::NewerSchema { found: CAMERA_CALIBRATION_SCHEMA + 1, supported: CAMERA_CALIBRATION_SCHEMA }
    );
    assert!(decode_camera_calibrations(br#"{"format":"something-else","schema":1}"#).is_err());
    assert!(decode_camera_calibrations(b"not json").is_err());
    assert!(decode_camera_calibrations(b"").is_err());
    let truncated = &text.as_bytes()[..text.len() / 2];
    assert!(decode_camera_calibrations(truncated).is_err());
    let unknown = text.replacen("\"lenses\"", "\"surprise\":1,\"lenses\"", 1);
    assert!(matches!(decode_camera_calibrations(unknown.as_bytes()), Err(LoadError::UnknownField { .. })));
}

#[test]
fn values_that_mean_nothing_are_refused() {
    let text = String::from_utf8(encode_camera_calibrations(&filled()).unwrap()).unwrap();
    // A picture of zero width, a frame rate of zero and a lens profile without a name.
    for (from, to) in [
        ("\"width\":1920", "\"width\":0"),
        ("\"frame_rate_mhz\":29970", "\"frame_rate_mhz\":0"),
        ("\"lens\":\"wide\"", "\"lens\":\"\""),
    ] {
        assert!(text.contains(from), "fixture lacks {from}");
        let bad = text.replace(from, to);
        assert!(decode_camera_calibrations(bad.as_bytes()).is_err(), "{from} -> {to}");
    }
    // A homography with a non-number cannot be written either.
    let mut store = CalibrationStore::default();
    let mut a = alignment(cam_a(), 0.0, false);
    a.calibration.bed_homography[0][0] = f64::NAN;
    store.insert_alignment(a);
    let fs = MemoryFileSystem::new();
    assert!(save_camera_calibrations(&fs, Path::new("/lib/c.json"), &store).is_err());
    assert!(fs.files().is_empty(), "nothing is written for a store that would not load");
}

#[test]
fn two_entries_for_one_configuration_are_refused_not_silently_merged() {
    let one = {
        let mut s = CalibrationStore::default();
        s.insert_lens(lens(cam_a(), -0.1));
        String::from_utf8(encode_camera_calibrations(&s).unwrap()).unwrap()
    };
    // Duplicate the single lens entry by hand.
    let start = one.find("\"lenses\":[").unwrap() + "\"lenses\":[".len();
    let end = one[start..].find("],\"alignments\"").unwrap() + start;
    let entry = &one[start..end];
    let dup = format!("{}{},{}{}", &one[..start], entry, entry, &one[end..]);
    assert!(decode_camera_calibrations(dup.as_bytes()).is_err());
}

#[test]
fn a_missing_file_is_a_typed_error_and_a_failed_save_keeps_the_old_file() {
    let fs = MemoryFileSystem::new();
    let path = Path::new("/lib/camera-calibration.json");
    assert!(matches!(load_camera_calibrations(&fs, path), Err(LoadError::Fs(_))));
    save_camera_calibrations(&fs, path, &filled()).unwrap();
    let before = fs.read(path, 1 << 20).unwrap();
    fs.fail_on(FsOp::Replace, 0);
    let mut smaller = CalibrationStore::default();
    smaller.insert_lens(lens(cam_a(), -0.3));
    assert!(save_camera_calibrations(&fs, path, &smaller).is_err());
    assert_eq!(fs.read(path, 1 << 20).unwrap(), before);
}
