//! W3-UI-camera: removing stored calibrations (Manage Cameras ▸ delete). A removal takes out exactly
//! the configuration (and height) named, and nothing else; asking for something that is not there
//! removes nothing and says so.

#![allow(clippy::unwrap_used, clippy::panic)]

use burnburn_image::calib::{
    AlignmentLens, CalibrationStore, CameraConfigKey, CameraDeviceId, CameraMode, CameraPixelFormat, Intrinsics,
    LensCalibration, LensFitReport, LensModel, MarkObservation, MarkRole, WorkspaceAlignment,
};
use burnburn_types::{RequestId, StableId};

fn config(device: &str, width: u32) -> CameraConfigKey {
    CameraConfigKey {
        device: CameraDeviceId(device.into()),
        mode: CameraMode { width, height: width / 2, pixel_format: CameraPixelFormat::Bgra8, frame_rate_mhz: 30_000 },
        lens: "standard".into(),
    }
}

fn lens(cfg: &CameraConfigKey) -> LensCalibration {
    LensCalibration {
        request: RequestId::from_raw(1),
        valid_for: cfg.clone(),
        intrinsics: Intrinsics { center_px: [100.0, 50.0], radius_px: 111.8, k1: -0.05, k2: 0.0 },
        report: LensFitReport {
            views: 3,
            points: 150,
            rms_px: 0.2,
            max_px: 0.5,
            rms_plane_mm: 0.1,
            evaluations: 80,
            converged: true,
        },
    }
}

/// An exact affine view of the bed: 0.5 mm per pixel.
fn alignment(cfg: &CameraConfigKey, height_mm: f64) -> WorkspaceAlignment {
    let obs = |px: [f64; 2], role| MarkObservation { pixel: px, bed_mm: [px[0] * 0.5, px[1] * 0.5], role };
    let marks = vec![
        obs([20.0, 10.0], MarkRole::Fit),
        obs([180.0, 10.0], MarkRole::Fit),
        obs([180.0, 90.0], MarkRole::Fit),
        obs([20.0, 90.0], MarkRole::Fit),
        obs([100.0, 50.0], MarkRole::Check),
    ];
    WorkspaceAlignment::align(&marks, cfg.clone(), AlignmentLens::Profile(LensModel::Fixed { k1: 0.0 }), height_mm, 0.5)
        .unwrap()
}

#[test]
fn removing_a_lens_calibration_takes_out_exactly_that_configuration() {
    let (a, b) = (config("cam-A", 200), config("cam-A", 400));
    let mut store = CalibrationStore::default();
    store.insert_lens(lens(&a));
    store.insert_lens(lens(&b));

    assert!(store.remove_lens(&a).is_some());

    assert!(store.lens_for(&a).is_err());
    assert!(store.lens_for(&b).is_ok(), "the other resolution keeps its calibration");
    assert_eq!(store.lens_count(), 1);
    assert!(store.remove_lens(&a).is_none(), "nothing left to remove");
}

#[test]
fn removing_an_alignment_takes_out_one_height_of_one_configuration() {
    let (a, b) = (config("cam-A", 200), config("cam-B", 200));
    let mut store = CalibrationStore::default();
    for (cfg, z) in [(&a, 0.0), (&a, 10.0), (&b, 0.0)] {
        store.insert_alignment(alignment(cfg, z));
    }

    let removed = store.remove_alignment(&a, 10.0).expect("stored");

    assert_eq!(removed.material_height_mm, 10.0);
    assert!(store.alignment_for(&a, 10.0).is_err());
    assert!(store.alignment_for(&a, 0.0).is_ok(), "other heights stay");
    assert!(store.alignment_for(&b, 0.0).is_ok(), "other cameras stay");
    assert!(store.remove_alignment(&a, 10.0).is_none());
    assert!(store.remove_alignment(&a, f64::NAN).is_none(), "an impossible height removes nothing");
    assert_eq!(store.alignment_count(), 2);
}

#[test]
fn every_stored_calibration_can_be_listed_in_canonical_order() {
    let (b, a) = (config("cam-B", 200), config("cam-A", 200));
    let mut store = CalibrationStore::default();
    store.insert_lens(lens(&b));
    store.insert_lens(lens(&a));
    store.insert_alignment(alignment(&b, 0.0));
    store.insert_alignment(alignment(&a, 10.0));
    store.insert_alignment(alignment(&a, 0.0));

    let lenses: Vec<&str> = store.lenses().map(|l| l.valid_for.device.0.as_str()).collect();
    let alignments: Vec<(&str, f64)> =
        store.alignments().map(|a| (a.config().device.0.as_str(), a.material_height_mm)).collect();

    assert_eq!(lenses, vec!["cam-A", "cam-B"], "sorted by configuration, not by insertion");
    assert_eq!(alignments, vec![("cam-A", 0.0), ("cam-A", 10.0), ("cam-B", 0.0)]);
}
