//! IM27 camera calibration, pure (no camera is opened): bed homography + radial lens model,
//! residuals measured on independent check points that are not part of the fit, and validity bound
//! to the exact camera configuration. Ground truth is synthetic: bed marks are projected through a
//! known perspective map and lens distortion, so the expected bed positions are exact.

#![allow(clippy::unwrap_used, clippy::panic)]

use burnburn_image::calib::{
    CalibError, CalibParams, CameraConfigKey, CameraDeviceId, CameraMode, CameraPixelFormat, Invalidity, LensModel,
    MarkObservation, MarkRole, calibrate,
};

const W: f64 = 1920.0;
const H: f64 = 1080.0;
/// Known bed (mm) → undistorted pixel map with real perspective (camera not square to the bed).
const G: [[f64; 3]; 3] = [[2.9, 0.15, 110.0], [-0.08, 2.4, 60.0], [0.000_12, 0.000_05, 1.0]];

fn key(width: u32, height: u32) -> CameraConfigKey {
    CameraConfigKey {
        device: CameraDeviceId("cam-A".into()),
        mode: CameraMode { width, height, pixel_format: CameraPixelFormat::Bgra8, frame_rate_mhz: 30_000 },
        lens: "standard".into(),
    }
}

fn project(bed: [f64; 2], k1: f64) -> [f64; 2] {
    project2(bed, k1, 0.0)
}

fn project2(bed: [f64; 2], k1: f64, k2: f64) -> [f64; 2] {
    let w = G[2][0] * bed[0] + G[2][1] * bed[1] + G[2][2];
    let u = [(G[0][0] * bed[0] + G[0][1] * bed[1] + G[0][2]) / w, (G[1][0] * bed[0] + G[1][1] * bed[1] + G[1][2]) / w];
    let c = [W / 2.0, H / 2.0];
    let radius = (c[0] * c[0] + c[1] * c[1]).sqrt();
    let d = [u[0] - c[0], u[1] - c[1]];
    let r2 = (d[0] * d[0] + d[1] * d[1]) / (radius * radius);
    [c[0] + d[0] * (1.0 + k1 * r2 + k2 * r2 * r2), c[1] + d[1] * (1.0 + k1 * r2 + k2 * r2 * r2)]
}

fn mark(bed: [f64; 2], role: MarkRole, k1: f64) -> MarkObservation {
    MarkObservation { pixel: project(bed, k1), bed_mm: bed, role }
}

/// 4 × 3 fit marks over a 600 × 400 mm bed and three check marks in between.
fn observations(k1: f64) -> Vec<MarkObservation> {
    let mut obs = Vec::new();
    for x in [0.0, 200.0, 400.0, 600.0] {
        for y in [0.0, 200.0, 400.0] {
            obs.push(mark([x, y], MarkRole::Fit, k1));
        }
    }
    for bed in [[150.0, 100.0], [450.0, 300.0], [300.0, 333.0]] {
        obs.push(mark(bed, MarkRole::Check, k1));
    }
    obs
}

fn params(lens: LensModel) -> CalibParams {
    CalibParams { lens, max_fit_residual_mm: 0.5 }
}

#[test]
fn calibration_is_invalid_after_camera_resolution_changes() {
    let cal = calibrate(&observations(0.0), key(1920, 1080), params(LensModel::Fixed { k1: 0.0 })).unwrap();
    assert_eq!(cal.validity(&key(1920, 1080)), Ok(()));
    assert_eq!(cal.validity(&key(1280, 720)), Err(Invalidity::ResolutionChanged));
    assert_eq!(cal.pixel_to_bed(&key(1280, 720), [960.0, 540.0]), Err(Invalidity::ResolutionChanged));

    let mut other = key(1920, 1080);
    other.device = CameraDeviceId("cam-B".into());
    assert_eq!(cal.validity(&other), Err(Invalidity::DeviceChanged));
    let mut lens = key(1920, 1080);
    lens.lens = "wide".into();
    assert_eq!(cal.validity(&lens), Err(Invalidity::LensChanged));
    let mut format = key(1920, 1080);
    format.mode.pixel_format = CameraPixelFormat::Yuv420Video;
    assert_eq!(cal.validity(&format), Err(Invalidity::PixelFormatChanged));
}

#[test]
fn calibration_is_invalid_after_camera_frame_rate_changes() {
    let cal = calibrate(&observations(0.0), key(1920, 1080), params(LensModel::Fixed { k1: 0.0 })).unwrap();
    let mut slower = key(1920, 1080);
    slower.mode.frame_rate_mhz = 15_000;
    assert_eq!(cal.validity(&slower), Err(Invalidity::FrameRateChanged));
    assert_eq!(cal.pixel_to_bed(&slower, [960.0, 540.0]), Err(Invalidity::FrameRateChanged));
    assert_eq!(cal.validity(&key(1920, 1080)), Ok(()));
}

#[test]
fn bed_positions_are_recovered_and_check_points_report_their_error_in_mm() {
    let cal = calibrate(&observations(0.0), key(1920, 1080), params(LensModel::Fixed { k1: 0.0 })).unwrap();
    for bed in [[37.0, 291.0], [580.0, 12.0], [300.0, 200.0]] {
        let got = cal.pixel_to_bed(&key(1920, 1080), project(bed, 0.0)).unwrap();
        assert!((got[0] - bed[0]).abs() < 1e-6 && (got[1] - bed[1]).abs() < 1e-6, "{got:?} vs {bed:?}");
    }
    assert_eq!(cal.check_points.len(), 3);
    assert_eq!(cal.residuals_mm.count, 3);
    assert!(cal.residuals_mm.max_mm < 1e-6, "{:?}", cal.residuals_mm);
    assert_eq!(cal.fit_residuals_mm.count, 12);

    // A check mark measured 0.5 mm off shows exactly that error and does not move the fit.
    let mut obs = observations(0.0);
    let last = obs.len() - 1;
    obs[last].bed_mm[0] += 0.3;
    obs[last].bed_mm[1] += 0.4;
    let off = calibrate(&obs, key(1920, 1080), params(LensModel::Fixed { k1: 0.0 })).unwrap();
    assert_eq!(off.bed_homography, cal.bed_homography, "check points are not part of the fit");
    assert!((off.residuals_mm.max_mm - 0.5).abs() < 1e-6, "{:?}", off.residuals_mm);
    assert!((off.check_points[2].error_mm - 0.5).abs() < 1e-6);
}

#[test]
fn radial_lens_distortion_is_modelled_and_estimated() {
    let k1 = -0.08;
    let obs = observations(k1);
    // Ignoring the lens leaves millimetres of error at the check points.
    let naive =
        calibrate(&obs, key(1920, 1080), CalibParams { lens: LensModel::Fixed { k1: 0.0 }, max_fit_residual_mm: 50.0 })
            .unwrap();
    assert!(naive.residuals_mm.max_mm > 0.5, "{:?}", naive.residuals_mm);
    // The known lens coefficient removes it.
    let known = calibrate(&obs, key(1920, 1080), params(LensModel::Fixed { k1 })).unwrap();
    assert!(known.residuals_mm.max_mm < 1e-6, "{:?}", known.residuals_mm);
    // Estimated from the fit marks only.
    let est = calibrate(&obs, key(1920, 1080), params(LensModel::EstimateRadial)).unwrap();
    assert!((est.intrinsics.k1 - k1).abs() < 1e-3, "k1 = {}", est.intrinsics.k1);
    assert!(est.residuals_mm.max_mm < 0.01, "{:?}", est.residuals_mm);
    let got = est.pixel_to_bed(&key(1920, 1080), project([520.0, 380.0], k1)).unwrap();
    assert!((got[0] - 520.0).abs() < 0.01 && (got[1] - 380.0).abs() < 0.01, "{got:?}");
}

#[test]
fn fixed_radial_lens_with_k1_and_k2_recovers_bed_positions() {
    let (k1, k2) = (-0.11, 0.04);
    let obs: Vec<MarkObservation> =
        observations(0.0).into_iter().map(|o| MarkObservation { pixel: project2(o.bed_mm, k1, k2), ..o }).collect();
    let cfg = key(1920, 1080);
    // A one-coefficient model cannot explain the k2 term: millimetres of error remain.
    let k1_only =
        calibrate(&obs, cfg.clone(), CalibParams { lens: LensModel::Fixed { k1 }, max_fit_residual_mm: 50.0 }).unwrap();
    assert!(k1_only.residuals_mm.max_mm > 0.05, "{:?}", k1_only.residuals_mm);
    let full = calibrate(&obs, cfg.clone(), params(LensModel::FixedRadial { k1, k2 })).unwrap();
    assert!(full.residuals_mm.max_mm < 1e-6, "{:?}", full.residuals_mm);
    assert_eq!((full.intrinsics.k1, full.intrinsics.k2), (k1, k2));
    let got = full.pixel_to_bed(&cfg, project2([520.0, 380.0], k1, k2)).unwrap();
    assert!((got[0] - 520.0).abs() < 1e-6 && (got[1] - 380.0).abs() < 1e-6, "{got:?}");
    assert_eq!(calibrate(&obs, cfg, params(LensModel::FixedRadial { k1, k2: f64::NAN })), Err(CalibError::NonFinite));
}

#[test]
fn bed_to_pixel_is_the_inverse_of_pixel_to_bed_including_the_lens() {
    let (k1, k2) = (-0.11, 0.04);
    let obs: Vec<MarkObservation> =
        observations(0.0).into_iter().map(|o| MarkObservation { pixel: project2(o.bed_mm, k1, k2), ..o }).collect();
    let cfg = key(1920, 1080);
    let cal = calibrate(&obs, cfg.clone(), params(LensModel::FixedRadial { k1, k2 })).unwrap();
    for bed in [[10.0, 20.0], [300.0, 200.0], [590.0, 395.0]] {
        // Independent oracle: the synthetic forward projection used to make the marks.
        let want = project2(bed, k1, k2);
        let got = cal.bed_to_pixel(&cfg, bed).unwrap().unwrap();
        assert!((got[0] - want[0]).abs() < 1e-6 && (got[1] - want[1]).abs() < 1e-6, "{got:?} vs {want:?}");
        let back = cal.pixel_to_bed(&cfg, got).unwrap();
        assert!((back[0] - bed[0]).abs() < 1e-6 && (back[1] - bed[1]).abs() < 1e-6);
    }
    assert_eq!(cal.bed_to_pixel(&key(1280, 720), [10.0, 10.0]), Err(Invalidity::ResolutionChanged));
}

#[test]
fn too_few_collinear_conflicting_or_unchecked_marks_give_no_calibration() {
    let cfg = || key(1920, 1080);
    let p = params(LensModel::Fixed { k1: 0.0 });
    let obs = observations(0.0);
    let fit: Vec<_> = obs.iter().filter(|o| o.role == MarkRole::Fit).copied().collect();
    let check: Vec<_> = obs.iter().filter(|o| o.role == MarkRole::Check).copied().collect();

    let three: Vec<_> = fit[..3].iter().chain(&check).copied().collect();
    assert_eq!(calibrate(&three, cfg(), p), Err(CalibError::TooFewFitPoints { need: 4, got: 3 }));
    // Estimating the lens needs one mark more than the homography alone.
    let four: Vec<_> = [fit[0], fit[2], fit[9], fit[11]].iter().chain(&check).copied().collect();
    assert!(calibrate(&four, cfg(), p).is_ok());
    assert_eq!(
        calibrate(&four, cfg(), params(LensModel::EstimateRadial)),
        Err(CalibError::TooFewFitPoints { need: 5, got: 4 })
    );

    assert_eq!(calibrate(&fit, cfg(), p), Err(CalibError::NoCheckPoints), "residuals need independent marks");

    let line: Vec<_> = [0.0, 100.0, 200.0, 300.0, 400.0]
        .iter()
        .map(|x| mark([*x, 0.5 * x], MarkRole::Fit, 0.0))
        .chain(check.iter().copied())
        .collect();
    assert_eq!(calibrate(&line, cfg(), p), Err(CalibError::Collinear));

    // Same pixel, two different bed positions.
    let mut conflict = obs.clone();
    let mut twin = conflict[0];
    twin.bed_mm[0] += 25.0;
    conflict.push(twin);
    assert_eq!(calibrate(&conflict, cfg(), p), Err(CalibError::ConflictingMeasurements));

    // One fit mark measured 20 mm off: the fit cannot explain it, so no silent result.
    let mut wrong = obs.clone();
    wrong[4].bed_mm[1] += 20.0;
    assert!(matches!(calibrate(&wrong, cfg(), p), Err(CalibError::Inconsistent { .. })));

    let mut nan = obs.clone();
    nan[1].pixel[0] = f64::NAN;
    assert_eq!(calibrate(&nan, cfg(), p), Err(CalibError::NonFinite));

    let mut outside = obs;
    outside[2].pixel = [W + 10.0, 5.0];
    assert_eq!(calibrate(&outside, cfg(), p), Err(CalibError::OutsideImage));
}
