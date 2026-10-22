//! G7-werk, W04: camera calibrations (lens calibration, workspace alignment) are kept in the
//! library folder and come back after a restart, for exactly the camera and resolution they were
//! made for. Another source never gets an old calibration silently: the row says why it does not
//! apply and the picture is not placed. Real `AppController`, fake camera port, in-memory disk.

#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)] // test helpers

mod common;
#[path = "common/camera.rs"]
mod rig;

use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use burnburn_image::calib::{Invalidity, StoreMiss};
use burnburn_io::fs::FileSystem;
use burnburn_io::memory::MemoryFileSystem;
use burnburn_presentation::{
    CalibrationKind, CalibrationValidity, CameraSpawner, CameraWhy, CameraWorkspaceIntent as I, CameraWorkspaceView,
    HeadlessEditorHarness, UiIntent,
};

use rig::*;

const A: &str = "cam-A";
const B: &str = "cam-B";
const MAP: Mapping = Mapping { mm_per_px: 0.5, origin_mm: [100.0, 300.0] };
const FILE: &str = "/lib/camera-calibration.json";

struct Rig {
    h: HeadlessEditorHarness,
    cam: Arc<FakeCam>,
}

/// One "run of the app" on the shared disk: the library folder is set before the camera port is
/// attached, as the hosts do at startup; the file jobs have run when this returns.
fn start(fs: &Arc<MemoryFileSystem>) -> Rig {
    let cam = FakeCam::new(&[
        (A, &[mode(640, 360), mode(1280, 720)]),
        (B, &[mode(640, 360)]),
    ]);
    cam.authorize();
    let mut h = HeadlessEditorHarness::with_fs(Arc::clone(fs));
    h.fs().create_dir_all_for_test(FsPath::new("/lib"));
    h.controller().set_library_dir(Some(PathBuf::from("/lib")));
    h.controller().attach_camera(cam.clone());
    let inline: CameraSpawner = Arc::new(|job| job());
    h.controller().set_camera_spawner(inline);
    h.new_document().unwrap();
    h.run_tasks();
    Rig { h, cam }
}

fn go(r: &mut Rig, intent: I) -> CameraWorkspaceView {
    r.h.send(UiIntent::CameraWorkspace(intent));
    r.h.controller().poll_camera_work();
    r.h.run_tasks();
    view(r)
}

fn view(r: &mut Rig) -> CameraWorkspaceView {
    r.h.controller().camera_workspace_view().expect("camera workspace")
}

fn use_camera(r: &mut Rig, device: &str, m: burnburn_presentation::CameraMode) {
    r.cam.show(device, m, grey(m.width, m.height, 128));
    let v = go(r, I::UseCamera { device: dev(device), mode: m, lens: "standard".into() });
    assert!(v.cameras.iter().any(|c| c.device == dev(device) && c.mode == m), "{:?}", v.last_refusal);
}

/// The alignment wizard on a straight-down mapping, the same clicks as in `camera_workspace.rs`.
fn calibrate_alignment(r: &mut Rig, height: &str) {
    go(r, I::StartAlignment { device: dev(A), material_height_mm: height.into() });
    go(r, I::CaptureAlignmentPhoto);
    let places: [[f64; 2]; 6] =
        [[130.0, 280.0], [390.0, 280.0], [390.0, 140.0], [130.0, 140.0], [260.0, 210.0], [200.0, 170.0]];
    for (i, p) in places.iter().enumerate() {
        go(r, I::SetAlignmentMark { index: i, x_mm: p[0].to_string(), y_mm: p[1].to_string() });
        let v = go(r, I::PlaceAlignmentMark { index: i, pixel: MAP.pixel(*p) });
        assert_eq!(v.last_refusal, None);
    }
    go(r, I::FitAlignment);
    let v = go(r, I::SaveAlignment);
    assert!(v.alignment.is_none(), "saved: {:?}", v.last_refusal);
}

/// A disk with one saved alignment of cam-A at 640x360 (height 0) and one at height 10.
fn calibrated_disk() -> Arc<MemoryFileSystem> {
    let fs = Arc::new(MemoryFileSystem::new());
    let mut first = start(&fs);
    use_camera(&mut first, A, mode(640, 360));
    calibrate_alignment(&mut first, "0");
    calibrate_alignment(&mut first, "10");
    assert_eq!(first.h.controller().calibrations().alignment_count(), 2);
    drop(first);
    fs
}

#[test]
fn a_saved_alignment_is_back_after_a_restart_and_places_the_picture_for_the_same_camera() {
    let fs = calibrated_disk();
    assert!(fs.exists(FsPath::new(FILE)).unwrap(), "the calibration file is written next to the other libraries");

    let mut second = start(&fs);
    assert_eq!(second.h.controller().calibrations().alignment_count(), 2, "both heights are back");
    use_camera(&mut second, A, mode(640, 360));
    let v = view(&mut second);
    assert_eq!(v.calibrations.len(), 2);
    assert!(v.calibrations.iter().all(|c| c.validity == CalibrationValidity::Valid));
    assert!(v.calibrations.iter().any(|c| c.reference.kind == CalibrationKind::Alignment { height_um: 10_000 }));

    let v = go(&mut second, I::UpdateOverlay);
    assert_eq!(v.overlay.layers[0].not_placed, None, "the restored alignment places the picture");
}

#[test]
fn another_resolution_or_another_source_never_gets_the_old_calibration() {
    let fs = calibrated_disk();
    let mut second = start(&fs);

    // The same camera in another resolution: the row says what differs, the picture is not placed.
    use_camera(&mut second, A, mode(1280, 720));
    let v = view(&mut second);
    assert!(
        v.calibrations.iter().all(|c| c.validity == CalibrationValidity::Invalid(Invalidity::ResolutionChanged)),
        "{:?}",
        v.calibrations.iter().map(|c| c.validity).collect::<Vec<_>>()
    );
    let v = go(&mut second, I::UpdateOverlay);
    assert!(matches!(
        v.overlay.layers[0].not_placed,
        Some(CameraWhy::NotCalibrated(StoreMiss::Invalid(Invalidity::ResolutionChanged)))
    ));

    // A different camera (another source) at the stored resolution: nothing of cam-A applies.
    go(&mut second, I::DropCamera { device: dev(A) });
    use_camera(&mut second, B, mode(640, 360));
    let v = go(&mut second, I::UpdateOverlay);
    assert!(matches!(v.overlay.layers[0].not_placed, Some(CameraWhy::NotCalibrated(StoreMiss::NotCalibrated))));
    assert!(v.calibrations.iter().all(|c| c.validity == CalibrationValidity::CameraNotInUse));
}

#[test]
fn a_deleted_calibration_stays_deleted_after_a_restart() {
    let fs = calibrated_disk();
    let mut second = start(&fs);
    let target = view(&mut second)
        .calibrations
        .iter()
        .find(|c| c.reference.kind == CalibrationKind::Alignment { height_um: 10_000 })
        .unwrap()
        .reference
        .clone();
    go(&mut second, I::DeleteCalibration { calibration: target });
    drop(second);

    let mut third = start(&fs);
    let rows = view(&mut third).calibrations;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].reference.kind, CalibrationKind::Alignment { height_um: 0 });
}

#[test]
fn an_accepted_lens_calibration_is_kept_too() {
    let fs = Arc::new(MemoryFileSystem::new());
    let mut first = start(&fs);
    use_camera(&mut first, A, mode(640, 360));
    go(
        &mut first,
        I::StartLensCalibration { device: dev(A), cols: BOARD_COLS as u32, rows: BOARD_ROWS as u32, pitch_mm: "30".into() },
    );
    for pose in 0..3 {
        first.cam.show(A, mode(640, 360), photo_of_board(&POSES[pose], -0.12));
        go(&mut first, I::AddPatternView);
    }
    go(&mut first, I::FitLens { two_coefficients: false });
    go(&mut first, I::AcceptLensCalibration);
    assert_eq!(first.h.controller().calibrations().lens_count(), 1);
    drop(first);

    let mut second = start(&fs);
    let cfg = burnburn_presentation::config_key(&dev(A), &mode(640, 360), "standard");
    let lens = second.h.controller().calibrations().lens_for(&cfg).expect("the lens came back").clone();
    assert!((lens.intrinsics.k1 + 0.12).abs() < 0.03, "k1 {}", lens.intrinsics.k1);
}

#[test]
fn a_damaged_calibration_file_is_left_alone_while_the_session_still_works() {
    let fs = Arc::new(MemoryFileSystem::new());
    fs.create_dir_all_for_test(FsPath::new("/lib"));
    fs.insert(FsPath::new(FILE), b"{ not json".to_vec());
    let mut h = start(&fs);
    use_camera(&mut h, A, mode(640, 360));
    calibrate_alignment(&mut h, "0");
    assert_eq!(h.h.controller().calibrations().alignment_count(), 1, "usable in this session");
    assert_eq!(fs.read(FsPath::new(FILE), 1 << 20).unwrap(), b"{ not json".to_vec(), "the damaged file is not overwritten");
}
