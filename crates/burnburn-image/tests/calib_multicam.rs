//! W2-L5e several cameras, one bed coordinate system. Synthetic ground truth: pinhole cameras
//! looking straight down at known positions, so which camera sees which bed point (and how far from
//! its view centre) follows from the geometry. No real camera.

#![allow(clippy::unwrap_used, clippy::panic)]

use burnburn_image::calib::{
    AlignmentLens, CameraConfigKey, CameraDeviceId, CameraMode, CameraPixelFormat, CameraView, Invalidity, LensModel,
    MarkObservation, MarkRole, WorkspaceAlignment, cameras_seeing,
};

/// Focal length (px) chosen so that one camera at 900 mm height covers 500 × 280 mm of the bed.
const FOCAL: f64 = 3456.0;
const DEPTH: f64 = 900.0;

fn config(id: &str, width: u32, height: u32) -> CameraConfigKey {
    CameraConfigKey {
        device: CameraDeviceId(id.into()),
        mode: CameraMode { width, height, pixel_format: CameraPixelFormat::Bgra8, frame_rate_mhz: 30_000 },
        lens: "standard".into(),
    }
}

/// A camera straight above `(cx, 200)`; its alignment is fitted from marks like a user would place them.
fn camera(id: &str, cx: f64) -> (CameraConfigKey, WorkspaceAlignment) {
    let cfg = config(id, 1920, 1080);
    let pixel = |bed: [f64; 2]| [960.0 + FOCAL * (bed[0] - cx) / DEPTH, 540.0 + FOCAL * (bed[1] - 200.0) / DEPTH];
    let mut obs = Vec::new();
    for dx in [-200.0, -100.0, 0.0, 100.0, 200.0] {
        for dy in [-100.0, 0.0, 100.0] {
            let bed = [cx + dx, 200.0 + dy];
            obs.push(MarkObservation { pixel: pixel(bed), bed_mm: bed, role: MarkRole::Fit });
        }
    }
    for dx in [-150.0, 50.0] {
        let bed = [cx + dx, 230.0];
        obs.push(MarkObservation { pixel: pixel(bed), bed_mm: bed, role: MarkRole::Check });
    }
    let alignment =
        WorkspaceAlignment::align(&obs, cfg.clone(), AlignmentLens::Profile(LensModel::Fixed { k1: 0.0 }), 0.0, 0.5)
            .unwrap();
    (cfg, alignment)
}

fn ids(cover: &burnburn_image::calib::BedPointCoverage) -> Vec<&str> {
    cover.sightings.iter().map(|s| s.camera.0.as_str()).collect()
}

#[test]
fn each_bed_point_goes_to_the_camera_that_sees_it() {
    let (cfg_a, a) = camera("cam-A", 200.0); // covers x -50..450, y 60..340
    let (cfg_b, b) = camera("cam-B", 400.0); // covers x 150..650, y 60..340
    let views = [CameraView { current: &cfg_a, alignment: &a }, CameraView { current: &cfg_b, alignment: &b }];

    let only_a = cameras_seeing([20.0, 200.0], &views, 0.0);
    assert_eq!(ids(&only_a), vec!["cam-A"]);
    assert_eq!(only_a.chosen().unwrap().camera, CameraDeviceId("cam-A".into()));
    let px = only_a.chosen().unwrap().pixel;
    assert!((px[0] - (960.0 + FOCAL * (20.0 - 200.0) / DEPTH)).abs() < 1e-6, "{px:?}");

    assert_eq!(ids(&cameras_seeing([600.0, 200.0], &views, 0.0)), vec!["cam-B"]);
    assert!(cameras_seeing([300.0, 20.0], &views, 0.0).chosen().is_none(), "outside both footprints");
    assert!(cameras_seeing([-300.0, 200.0], &views, 0.0).sightings.is_empty());
}

#[test]
fn in_the_overlap_the_camera_whose_view_centre_is_closer_wins_and_ties_go_to_the_lower_id() {
    let (cfg_a, a) = camera("cam-A", 200.0);
    let (cfg_b, b) = camera("cam-B", 400.0);
    let views = [CameraView { current: &cfg_a, alignment: &a }, CameraView { current: &cfg_b, alignment: &b }];

    // x = 250: 50 mm from A's centre line, 150 mm from B's.
    let near_a = cameras_seeing([250.0, 200.0], &views, 0.0);
    assert_eq!(ids(&near_a), vec!["cam-A", "cam-B"], "best first, both listed");
    assert!(near_a.sightings[0].centre_distance < near_a.sightings[1].centre_distance);
    // x = 350: B is closer.
    assert_eq!(cameras_seeing([350.0, 200.0], &views, 0.0).chosen().unwrap().camera.0, "cam-B");
    // x = 300 is exactly halfway: the tie is broken by the lower device id, in either input order.
    let tie = cameras_seeing([300.0, 200.0], &views, 0.0);
    assert_eq!(tie.chosen().unwrap().camera.0, "cam-A");
    let reversed = [views[1].clone(), views[0].clone()];
    assert_eq!(cameras_seeing([300.0, 200.0], &reversed, 0.0), tie, "input order does not matter");
}

#[test]
fn a_margin_keeps_points_near_the_image_edge_out_and_hands_them_to_the_other_camera() {
    let (cfg_a, a) = camera("cam-A", 200.0);
    let (cfg_b, b) = camera("cam-B", 400.0);
    let views = [CameraView { current: &cfg_a, alignment: &a }, CameraView { current: &cfg_b, alignment: &b }];
    // x = 440 is 240 mm = 921.6 px right of A's centre (pixel x 1881.6), inside the 1920 px image.
    let loose = cameras_seeing([440.0, 200.0], &views, 0.0);
    assert!(ids(&loose).contains(&"cam-A"));
    assert!((loose.sightings.iter().find(|s| s.camera.0 == "cam-A").unwrap().edge_margin_px - 38.4).abs() < 1e-6);
    // A 100 px margin rejects A there; B (40 mm from its centre) still sees it.
    let strict = cameras_seeing([440.0, 200.0], &views, 100.0);
    assert_eq!(ids(&strict), vec!["cam-B"]);
}

#[test]
fn a_camera_whose_configuration_changed_is_skipped_with_the_reason_never_used() {
    let (cfg_a, a) = camera("cam-A", 200.0);
    let (cfg_b, b) = camera("cam-B", 400.0);
    let b_now = config("cam-B", 1280, 720);
    let views = [CameraView { current: &cfg_a, alignment: &a }, CameraView { current: &b_now, alignment: &b }];
    let cover = cameras_seeing([300.0, 200.0], &views, 0.0);
    assert_eq!(ids(&cover), vec!["cam-A"]);
    assert_eq!(cover.skipped, vec![(CameraDeviceId("cam-B".into()), Invalidity::ResolutionChanged)]);
    let _ = cfg_b;
}
