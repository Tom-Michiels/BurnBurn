//! W2-L5e workspace alignments and the calibration registry. Ground truth is synthetic: a pinhole
//! camera looking straight down, so the bed position of a pixel at material height `z` is known in
//! closed form (and is linear in `z`, which the height interpolation relies on). No real camera.

#![allow(clippy::unwrap_used, clippy::panic)]

use burnburn_image::calib::{
    AlignError, AlignmentLens, CalibError, CalibrationStore, CameraConfigKey, CameraDeviceId, CameraMode,
    CameraPixelFormat, HeightLookupError, Invalidity, LensCalibration, LensModel, MarkObservation, MarkRole, StoreMiss,
    WorkspaceAlignment,
};
use burnburn_types::{CanonicalEncode, CanonicalWriter, RequestId, StableId};

fn config(device: &str, width: u32, height: u32, format: CameraPixelFormat, fps_mhz: u32) -> CameraConfigKey {
    CameraConfigKey {
        device: CameraDeviceId(device.into()),
        mode: CameraMode { width, height, pixel_format: format, frame_rate_mhz: fps_mhz },
        lens: "standard".into(),
    }
}

fn cam_a() -> CameraConfigKey {
    config("cam-A", 1920, 1080, CameraPixelFormat::Bgra8, 30_000)
}

/// Pinhole camera at (300, 200, 900) mm looking straight down, focal length 1500 px (for 1920 px
/// wide images), principal point at the image centre. `scale` adapts the focal length to a resolution.
struct Pinhole {
    scale: f64,
    width: f64,
    height: f64,
}

impl Pinhole {
    const CAMERA: [f64; 3] = [300.0, 200.0, 900.0];
    const FOCAL: f64 = 1500.0;

    fn for_config(cfg: &CameraConfigKey) -> Pinhole {
        let width = f64::from(cfg.mode.width);
        Pinhole { scale: width / 1920.0, width, height: f64::from(cfg.mode.height) }
    }

    /// Where a bed point at material height `z` appears.
    fn pixel(&self, bed: [f64; 2], z: f64) -> [f64; 2] {
        let depth = Self::CAMERA[2] - z;
        let f = Self::FOCAL * self.scale;
        [
            self.width / 2.0 + f * (bed[0] - Self::CAMERA[0]) / depth,
            self.height / 2.0 + f * (bed[1] - Self::CAMERA[1]) / depth,
        ]
    }

    /// The bed point a pixel sees at material height `z` (the ray intersected with the plane).
    fn bed(&self, pixel: [f64; 2], z: f64) -> [f64; 2] {
        let depth = Self::CAMERA[2] - z;
        let f = Self::FOCAL * self.scale;
        [
            Self::CAMERA[0] + (pixel[0] - self.width / 2.0) * depth / f,
            Self::CAMERA[1] + (pixel[1] - self.height / 2.0) * depth / f,
        ]
    }
}

fn marks(cam: &Pinhole, z: f64) -> Vec<MarkObservation> {
    let mut obs = Vec::new();
    for x in [0.0, 200.0, 400.0, 600.0] {
        for y in [0.0, 200.0, 400.0] {
            obs.push(MarkObservation { pixel: cam.pixel([x, y], z), bed_mm: [x, y], role: MarkRole::Fit });
        }
    }
    for bed in [[150.0, 100.0], [450.0, 300.0]] {
        obs.push(MarkObservation { pixel: cam.pixel(bed, z), bed_mm: bed, role: MarkRole::Check });
    }
    obs
}

fn align(cfg: &CameraConfigKey, z: f64) -> WorkspaceAlignment {
    let cam = Pinhole::for_config(cfg);
    WorkspaceAlignment::align(
        &marks(&cam, z),
        cfg.clone(),
        AlignmentLens::Profile(LensModel::Fixed { k1: 0.0 }),
        z,
        0.5,
    )
    .unwrap()
}

fn assert_near(got: [f64; 2], want: [f64; 2], tol: f64) {
    assert!((got[0] - want[0]).abs() < tol && (got[1] - want[1]).abs() < tol, "{got:?} vs {want:?}");
}

#[test]
fn an_alignment_carries_the_material_height_and_the_lens_calibration_it_was_made_with() {
    let cfg = cam_a();
    let cam = Pinhole::for_config(&cfg);
    let (k1, k2) = (-0.09, 0.02);
    let lens = burnburn_image::calib::Intrinsics { center_px: [960.0, 540.0], radius_px: 960f64.hypot(540.0), k1, k2 };
    // Observed marks are the pinhole pixels pushed through the lens.
    let distort = |p: [f64; 2]| {
        let d = [p[0] - lens.center_px[0], p[1] - lens.center_px[1]];
        let r2 = (d[0] * d[0] + d[1] * d[1]) / (lens.radius_px * lens.radius_px);
        let f = 1.0 + k1 * r2 + k2 * r2 * r2;
        [lens.center_px[0] + d[0] * f, lens.center_px[1] + d[1] * f]
    };
    let observed: Vec<MarkObservation> =
        marks(&cam, 12.5).into_iter().map(|o| MarkObservation { pixel: distort(o.pixel), ..o }).collect();
    let lens_cal = LensCalibration {
        request: RequestId::from_raw(3),
        valid_for: cfg.clone(),
        intrinsics: lens,
        report: burnburn_image::calib::LensFitReport {
            views: 3,
            points: 351,
            rms_px: 0.0,
            max_px: 0.0,
            rms_plane_mm: 0.0,
            evaluations: 1,
            converged: true,
        },
    };
    let alignment =
        WorkspaceAlignment::align(&observed, cfg.clone(), AlignmentLens::Calibrated(lens_cal.clone()), 12.5, 0.5)
            .unwrap();
    assert_eq!(alignment.material_height_mm, 12.5);
    assert_eq!(alignment.lens_calibration.as_ref(), Some(&lens_cal));
    assert_eq!((alignment.calibration.intrinsics.k1, alignment.calibration.intrinsics.k2), (k1, k2));
    let probe = [333.0, 111.0];
    let got = alignment.pixel_to_bed(&cfg, distort(cam.pixel(probe, 12.5))).unwrap();
    assert_near(got, probe, 1e-6);

    // A lens calibration of another resolution is refused, never silently applied.
    let mut other = cfg.clone();
    other.mode.width = 1280;
    other.mode.height = 720;
    let wrong = WorkspaceAlignment::align(&observed, other, AlignmentLens::Calibrated(lens_cal), 12.5, 0.5);
    assert_eq!(wrong, Err(AlignError::LensCalibrationInvalid(Invalidity::ResolutionChanged)));
}

#[test]
fn alignment_refuses_an_impossible_material_height_and_inherits_the_mark_checks() {
    let cfg = cam_a();
    let cam = Pinhole::for_config(&cfg);
    let obs = marks(&cam, 0.0);
    let lens = || AlignmentLens::Profile(LensModel::Fixed { k1: 0.0 });
    for bad in [-1.0, f64::NAN, f64::INFINITY, 10_000.0] {
        assert_eq!(WorkspaceAlignment::align(&obs, cfg.clone(), lens(), bad, 0.5), Err(AlignError::BadHeight), "{bad}");
    }
    let few: Vec<_> = obs.iter().filter(|o| o.role == MarkRole::Check).copied().collect();
    assert!(matches!(
        WorkspaceAlignment::align(&few, cfg, lens(), 0.0, 0.5),
        Err(AlignError::Calibration(CalibError::TooFewFitPoints { .. }))
    ));
}

#[test]
fn several_calibrations_per_camera_are_found_by_exact_configuration_and_by_resolution() {
    let a_1080_30 = cam_a();
    let a_1080_15 = config("cam-A", 1920, 1080, CameraPixelFormat::Bgra8, 15_000);
    let a_720 = config("cam-A", 1280, 720, CameraPixelFormat::Bgra8, 30_000);
    let b_1080 = config("cam-B", 1920, 1080, CameraPixelFormat::Yuv420Video, 30_000);

    let mut store = CalibrationStore::default();
    // Inserted in a deliberately scrambled order.
    for cfg in [&b_1080, &a_720, &a_1080_30, &a_1080_15] {
        assert!(store.insert_alignment(align(cfg, 0.0)).is_none());
    }
    assert_eq!(store.alignment_count(), 4);

    let exact = store.alignment_for(&a_720, 0.0).unwrap();
    assert_eq!(exact.calibration.valid_for, a_720);
    // Each stored alignment maps with its own homography (resolution-dependent here).
    let cam720 = Pinhole::for_config(&a_720);
    assert_near(exact.pixel_to_bed(&a_720, cam720.pixel([77.0, 321.0], 0.0)).unwrap(), [77.0, 321.0], 1e-6);

    let at_1080: Vec<_> = store
        .alignments_for_resolution(&CameraDeviceId("cam-A".into()), 1920, 1080)
        .iter()
        .map(|a| a.calibration.valid_for.mode.frame_rate_mhz)
        .collect();
    assert_eq!(at_1080, vec![15_000, 30_000], "both frame rates of the resolution, in canonical order");
    let at_720 = store.alignments_for_resolution(&CameraDeviceId("cam-A".into()), 1280, 720);
    assert_eq!(at_720.len(), 1);
    assert_eq!(at_720[0].calibration.valid_for, a_720);
    let b = store.alignments_for_resolution(&CameraDeviceId("cam-B".into()), 1920, 1080);
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].calibration.valid_for.device, CameraDeviceId("cam-B".into()));
    assert!(store.alignments_for_resolution(&CameraDeviceId("cam-B".into()), 1280, 720).is_empty());
    assert!(store.alignments_for_resolution(&CameraDeviceId("cam-C".into()), 1920, 1080).is_empty());

    // Re-calibrating the same configuration and height replaces it and hands back the old one.
    let old = store.insert_alignment(align(&a_720, 0.0)).unwrap();
    assert_eq!(old.calibration.valid_for, a_720);
    assert_eq!(store.alignment_count(), 4);
}

#[test]
fn a_changed_camera_configuration_never_reuses_a_stored_calibration() {
    let cfg = cam_a();
    let mut store = CalibrationStore::default();
    store.insert_alignment(align(&cfg, 0.0));
    assert!(store.alignment_for(&cfg, 0.0).is_ok());

    let mut resolution = cfg.clone();
    resolution.mode.width = 1600;
    resolution.mode.height = 900;
    assert_eq!(store.alignment_for(&resolution, 0.0), Err(StoreMiss::Invalid(Invalidity::ResolutionChanged)));
    let mut fps = cfg.clone();
    fps.mode.frame_rate_mhz = 60_000;
    assert_eq!(store.alignment_for(&fps, 0.0), Err(StoreMiss::Invalid(Invalidity::FrameRateChanged)));
    let mut format = cfg.clone();
    format.mode.pixel_format = CameraPixelFormat::Yuv420Video;
    assert_eq!(store.alignment_for(&format, 0.0), Err(StoreMiss::Invalid(Invalidity::PixelFormatChanged)));
    let mut lens = cfg.clone();
    lens.lens = "wide".into();
    assert_eq!(store.alignment_for(&lens, 0.0), Err(StoreMiss::Invalid(Invalidity::LensChanged)));
    let other = config("cam-Z", 1920, 1080, CameraPixelFormat::Bgra8, 30_000);
    assert_eq!(store.alignment_for(&other, 0.0), Err(StoreMiss::NotCalibrated));

    // The closest stored configuration names the reason: resolution AND fps changed -> resolution first.
    let mut both = cfg.clone();
    both.mode.width = 1280;
    both.mode.frame_rate_mhz = 60_000;
    assert_eq!(store.alignment_for(&both, 0.0), Err(StoreMiss::Invalid(Invalidity::ResolutionChanged)));
}

#[test]
fn lens_calibrations_follow_the_same_exact_configuration_rule() {
    let cfg = cam_a();
    let mut store = CalibrationStore::default();
    assert_eq!(store.lens_for(&cfg), Err(StoreMiss::NotCalibrated));
    let lens = LensCalibration {
        request: RequestId::from_raw(1),
        valid_for: cfg.clone(),
        intrinsics: burnburn_image::calib::Intrinsics {
            center_px: [960.0, 540.0],
            radius_px: 960f64.hypot(540.0),
            k1: -0.05,
            k2: 0.0,
        },
        report: burnburn_image::calib::LensFitReport {
            views: 1,
            points: 100,
            rms_px: 0.1,
            max_px: 0.2,
            rms_plane_mm: 0.01,
            evaluations: 50,
            converged: true,
        },
    };
    assert!(store.insert_lens(lens.clone()).is_none());
    assert_eq!(store.lens_for(&cfg), Ok(&lens));
    let mut other = cfg.clone();
    other.mode.width = 1280;
    assert_eq!(store.lens_for(&other), Err(StoreMiss::Invalid(Invalidity::ResolutionChanged)));
    assert_eq!(store.lens_count(), 1);
    assert_eq!(store.insert_lens(lens.clone()), Some(lens));
}

#[test]
fn the_store_has_a_canonical_form_independent_of_insertion_order() {
    let configs = [
        cam_a(),
        config("cam-A", 1280, 720, CameraPixelFormat::Bgra8, 30_000),
        config("cam-B", 1920, 1080, CameraPixelFormat::Yuv420Video, 30_000),
    ];
    let encode = |store: &CalibrationStore| {
        let mut w = CanonicalWriter::new();
        store.encode(&mut w);
        w.finish().unwrap()
    };
    let mut forward = CalibrationStore::default();
    let mut backward = CalibrationStore::default();
    for cfg in &configs {
        forward.insert_alignment(align(cfg, 0.0));
        forward.insert_alignment(align(cfg, 20.0));
    }
    for cfg in configs.iter().rev() {
        backward.insert_alignment(align(cfg, 20.0));
        backward.insert_alignment(align(cfg, 0.0));
    }
    assert_eq!(forward, backward);
    assert_eq!(encode(&forward), encode(&backward));
    backward.insert_alignment(align(&configs[0], 5.0));
    assert_ne!(encode(&forward), encode(&backward));
    assert_ne!(encode(&forward), encode(&CalibrationStore::default()));
}

#[test]
fn height_lookup_is_exact_and_interpolation_follows_the_camera_ray() {
    let cfg = cam_a();
    let cam = Pinhole::for_config(&cfg);
    let mut store = CalibrationStore::default();
    store.insert_alignment(align(&cfg, 0.0));
    store.insert_alignment(align(&cfg, 20.0));

    assert_eq!(store.alignment_for(&cfg, 20.0).unwrap().material_height_mm, 20.0);
    assert_eq!(
        store.alignment_for(&cfg, 10.0),
        Err(StoreMiss::NoAlignmentForHeight { available_mm: vec![0.0, 20.0] }),
        "a different height is not silently served by a neighbouring alignment"
    );

    // The point seen at pixel p moves linearly with material height along its ray.
    let pixel = cam.pixel([420.0, 130.0], 0.0);
    for z in [0.0, 5.0, 10.0, 17.5, 20.0] {
        let got = store.pixel_to_bed_at_height(&cfg, pixel, z).unwrap();
        assert_near(got, cam.bed(pixel, z), 1e-6);
    }
    assert_eq!(
        store.pixel_to_bed_at_height(&cfg, pixel, 30.0),
        Err(HeightLookupError::OutsideCalibratedRange { min_mm: 0.0, max_mm: 20.0 })
    );
    assert_eq!(
        store.pixel_to_bed_at_height(&cfg, pixel, -0.5),
        Err(HeightLookupError::OutsideCalibratedRange { min_mm: 0.0, max_mm: 20.0 })
    );
    let mut changed = cfg;
    changed.mode.width = 1280;
    assert_eq!(
        store.pixel_to_bed_at_height(&changed, pixel, 10.0),
        Err(HeightLookupError::Miss(StoreMiss::Invalid(Invalidity::ResolutionChanged)))
    );
}
