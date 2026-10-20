//! W3-UI-camera, controller level (W04, W05, W2-L5e): the camera picture on the bed, several
//! cameras in one bed system, the lens-calibration and workspace-alignment wizards and the
//! management of stored calibrations, through the real `AppController`, a fake `CameraPort`, the
//! machine service over the real execution actor and a scripted fake controller. Ground truth is
//! synthetic (straight-down mappings with known answers, a rendered checkerboard). No AVFoundation,
//! no real laser, no camera permission prompt.

#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)] // test helpers

mod common;
#[path = "common/camera.rs"]
mod rig;

use std::sync::{Arc, Mutex};

use burnburn_editor::{BedPicture, BedQuad, Overlay};
use burnburn_image::calib::{CalibrationStore, CornerError, Invalidity, StoreMiss};
use burnburn_presentation::{
    CalibrationKind, CalibrationValidity, CameraSpawner, CameraWhy, CameraWorkspaceIntent as I, CameraWorkspaceView,
    HeadlessEditorHarness, MachineWhy, UiIntent, config_key,
};
use burnburn_types::DocumentId;

use rig::*;

const A: &str = "cam-A";
const B: &str = "cam-B";

struct Rig {
    h: HeadlessEditorHarness,
    doc: DocumentId,
    cam: Arc<FakeCam>,
}

fn rig_with(devices: &[(&str, &[burnburn_presentation::CameraMode])]) -> Rig {
    let cam = FakeCam::new(devices);
    cam.authorize();
    let mut h = HeadlessEditorHarness::new();
    h.controller().attach_camera(cam.clone());
    // Work runs at once, on the calling thread.
    h.controller().set_camera_spawner(Arc::new(|job| job()));
    let doc = h.new_document().unwrap();
    Rig { h, doc, cam }
}

fn rig_a() -> Rig {
    rig_with(&[(A, &[mode(640, 360), mode(1280, 720)])])
}

fn go(r: &mut Rig, intent: I) -> CameraWorkspaceView {
    r.h.send(UiIntent::CameraWorkspace(intent));
    r.h.controller().poll_camera_work();
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

fn store_with(alignments: Vec<burnburn_image::calib::WorkspaceAlignment>) -> CalibrationStore {
    let mut store = CalibrationStore::default();
    for a in alignments {
        store.insert_alignment(a);
    }
    store
}

type Layers = Vec<(Arc<BedPicture>, Arc<[BedQuad]>, f32)>;

fn bed_images(r: &Rig) -> Layers {
    let scene = r.h.scene(r.doc).unwrap();
    scene
        .overlays
        .iter()
        .filter_map(|o| match o {
            Overlay::BedImage { picture, quads, opacity } => Some((Arc::clone(picture), Arc::clone(quads), *opacity)),
            _ => None,
        })
        .collect()
}

fn area(q: &BedQuad) -> f64 {
    let p = q.bed_mm;
    let mut sum = 0.0;
    for i in 0..4 {
        let (a, b) = (p[i], p[(i + 1) % 4]);
        sum += a.x * b.y - b.x * a.y;
    }
    sum.abs() / 2.0
}

fn centre(q: &BedQuad) -> [f64; 2] {
    [q.bed_mm.iter().map(|p| p.x).sum::<f64>() / 4.0, q.bed_mm.iter().map(|p| p.y).sum::<f64>() / 4.0]
}

fn near(got: [f64; 2], want: [f64; 2]) {
    assert!((got[0] - want[0]).abs() < 1e-6 && (got[1] - want[1]).abs() < 1e-6, "{got:?} vs {want:?}");
}

const MAP: Mapping = Mapping { mm_per_px: 0.5, origin_mm: [100.0, 300.0] };

// --- the picture on the bed --------------------------------------------------------------------

/// Camera A looks straight down: bed = (100 + 0.5·px, 300 − 0.5·py). The 640 × 360 px picture is
/// therefore 320 × 180 mm with its top-left corner at (100, 300): the mesh must cover exactly that,
/// its corners must be where the mapping says, and the texture coordinates must run over the whole
/// picture.
#[test]
fn updating_the_overlay_places_the_camera_picture_on_the_bed_in_millimetres() {
    let mut r = rig_a();
    r.h.controller().attach_calibrations(store_with(vec![aligned(A, mode(640, 360), 0.0, MAP)]));
    use_camera(&mut r, A, mode(640, 360));
    assert!(bed_images(&r).is_empty(), "nothing is shown before the user asks for an update");

    let v = go(&mut r, I::UpdateOverlay);

    assert_eq!(v.overlay.layers.len(), 1);
    assert_eq!(v.overlay.layers[0].not_placed, None, "{:?}", v.overlay.layers);
    let layers = bed_images(&r);
    assert_eq!(layers.len(), 1);
    let (picture, quads, opacity) = &layers[0];
    assert_eq!((picture.width, picture.height), (640, 360));
    assert_eq!(*opacity, 0.5, "half transparent unless the user chooses otherwise");
    assert!(!quads.is_empty());
    near([quads[0].bed_mm[0].x, quads[0].bed_mm[0].y], MAP.bed([0.0, 0.0]));
    let last = quads.last().unwrap();
    near([last.bed_mm[2].x, last.bed_mm[2].y], MAP.bed([640.0, 360.0]));
    let total: f64 = quads.iter().map(area).sum();
    assert!((total - 320.0 * 180.0).abs() < 1e-6, "the mesh covers the 320 × 180 mm the camera sees: {total}");
    let uv_min = quads.iter().flat_map(|q| q.uv).fold([1.0f32; 2], |a, p| [a[0].min(p[0]), a[1].min(p[1])]);
    let uv_max = quads.iter().flat_map(|q| q.uv).fold([0.0f32; 2], |a, p| [a[0].max(p[0]), a[1].max(p[1])]);
    assert_eq!((uv_min, uv_max), ([0.0, 0.0], [1.0, 1.0]));
    assert_eq!(v.overlay.note, burnburn_presentation::MessageKey::VisualPlacementOnly);
}

#[test]
fn the_overlay_can_be_switched_off_and_its_opacity_follows_the_slider() {
    let mut r = rig_a();
    r.h.controller().attach_calibrations(store_with(vec![aligned(A, mode(640, 360), 0.0, MAP)]));
    use_camera(&mut r, A, mode(640, 360));
    go(&mut r, I::UpdateOverlay);

    go(&mut r, I::SetOverlayOpacity { percent: 25.0 });
    assert!((bed_images(&r)[0].2 - 0.25).abs() < 1e-6);

    let v = go(&mut r, I::SetOverlayVisible(false));
    assert!(!v.overlay.visible);
    assert!(bed_images(&r).is_empty(), "off: not drawn, but the captured frame is kept");
    let v = go(&mut r, I::SetOverlayVisible(true));
    assert!(v.overlay.visible);
    assert_eq!(bed_images(&r).len(), 1);

    // Opacity must be a number from 0 to 100; anything else changes nothing and says so.
    for bad in [f64::NAN, -1.0, 100.5, f64::INFINITY] {
        let v = go(&mut r, I::SetOverlayOpacity { percent: bad });
        assert_eq!(v.last_refusal, Some(CameraWhy::InvalidNumber));
        assert_eq!(v.overlay.opacity_percent, 25.0);
    }
}

#[test]
fn without_a_calibration_the_picture_is_not_placed_and_the_reason_is_typed() {
    let mut r = rig_a();
    use_camera(&mut r, A, mode(640, 360));

    let v = go(&mut r, I::UpdateOverlay);

    assert_eq!(v.overlay.layers[0].not_placed, Some(CameraWhy::NotCalibrated(StoreMiss::NotCalibrated)));
    assert!(bed_images(&r).is_empty(), "an uncalibrated picture is never guessed onto the bed");
}

#[test]
fn updating_with_no_running_camera_is_a_typed_refusal() {
    let mut r = rig_a();
    let v = go(&mut r, I::UpdateOverlay);
    assert_eq!(v.last_refusal, Some(CameraWhy::NothingToUpdate));
    assert!(!v.overlay.can_update);
    assert_eq!(v.overlay.update_why_not, Some(CameraWhy::NothingToUpdate));
}

/// W04 `calibration_is_invalid_after_camera_resolution_changes`, as the user meets it: the camera
/// is restarted in another resolution, the stored calibration shows as invalid with the reason, and
/// the picture of that camera is not placed with it. Back in the original resolution it applies again.
#[test]
fn calibration_is_invalid_after_camera_resolution_changes() {
    let mut r = rig_a();
    r.h.controller().attach_calibrations(store_with(vec![aligned(A, mode(640, 360), 0.0, MAP)]));
    use_camera(&mut r, A, mode(640, 360));
    let v = view(&mut r);
    assert_eq!(v.calibrations.len(), 1);
    assert_eq!(v.calibrations[0].validity, CalibrationValidity::Valid);

    // The camera is now used at 1280 × 720.
    use_camera(&mut r, A, mode(1280, 720));
    let v = go(&mut r, I::UpdateOverlay);
    assert_eq!(v.calibrations[0].validity, CalibrationValidity::Invalid(Invalidity::ResolutionChanged));
    assert_eq!(
        v.overlay.layers[0].not_placed,
        Some(CameraWhy::NotCalibrated(StoreMiss::Invalid(Invalidity::ResolutionChanged)))
    );
    assert!(bed_images(&r).is_empty());

    use_camera(&mut r, A, mode(640, 360));
    let v = go(&mut r, I::UpdateOverlay);
    assert_eq!(v.calibrations[0].validity, CalibrationValidity::Valid);
    assert_eq!(bed_images(&r).len(), 1);
}

#[test]
fn a_changed_lens_profile_or_frame_rate_invalidates_a_calibration_too() {
    let mut r = rig_a();
    r.h.controller().attach_calibrations(store_with(vec![aligned(A, mode(640, 360), 0.0, MAP)]));
    use_camera(&mut r, A, mode(640, 360));

    let v = go(&mut r, I::SetLensProfile { device: dev(A), lens: "wide angle".into() });
    assert_eq!(v.calibrations[0].validity, CalibrationValidity::Invalid(Invalidity::LensChanged));

    go(&mut r, I::SetLensProfile { device: dev(A), lens: "standard".into() });
    let mut slower = mode(640, 360);
    slower.fps_milli = 15_000;
    use_camera(&mut r, A, slower);
    let v = view(&mut r);
    assert_eq!(v.calibrations[0].validity, CalibrationValidity::Invalid(Invalidity::FrameRateChanged));
}

#[test]
fn a_calibration_of_a_camera_that_is_not_in_use_says_so_instead_of_claiming_validity() {
    let mut r = rig_a();
    r.h.controller().attach_calibrations(store_with(vec![aligned(A, mode(640, 360), 0.0, MAP)]));
    let v = view(&mut r);
    assert_eq!(v.calibrations[0].validity, CalibrationValidity::CameraNotInUse);
}

/// Two cameras in one bed. Camera A sees x 0..320 mm, camera B x 160..480 mm (same rows). Where both
/// see a cell, the camera whose view centre is nearer wins: the centres are at x = 160 and x = 320,
/// so A wins left of x = 240 and B right of it. Cells are 10 mm wide with centres at 5, 15, ...
#[test]
fn two_cameras_share_one_bed_and_the_better_view_wins_where_they_overlap() {
    let mut r = rig_with(&[(A, &[mode(640, 360)]), (B, &[mode(640, 360)])]);
    let map_a = Mapping { mm_per_px: 0.5, origin_mm: [0.0, 300.0] };
    let map_b = Mapping { mm_per_px: 0.5, origin_mm: [160.0, 300.0] };
    r.h.controller().attach_calibrations(store_with(vec![
        aligned(A, mode(640, 360), 0.0, map_a),
        aligned(B, mode(640, 360), 0.0, map_b),
    ]));
    use_camera(&mut r, A, mode(640, 360));
    use_camera(&mut r, B, mode(640, 360));

    let v = go(&mut r, I::UpdateOverlay);

    let cells: Vec<usize> = v.overlay.layers.iter().map(|l| l.cells).collect();
    assert_eq!(cells, vec![24 * 18, 24 * 18], "{:?}", v.overlay.layers);
    let layers = bed_images(&r);
    assert_eq!(layers.len(), 2);
    for (_, quads, _) in &layers[..1] {
        assert!(quads.iter().all(|q| centre(q)[0] < 240.0), "camera A keeps the left of the overlap");
    }
    for (_, quads, _) in &layers[1..] {
        assert!(quads.iter().all(|q| centre(q)[0] > 240.0), "camera B keeps the right of it");
    }
    // Together they cover the whole 0..480 mm band once: 48 columns of 10 mm.
    let covered: f64 = layers.iter().flat_map(|(_, q, _)| q.iter()).map(area).sum();
    assert!((covered - 480.0 * 180.0).abs() < 1e-6, "{covered}");
}

#[test]
fn a_camera_with_a_changed_configuration_is_left_out_while_the_other_still_shows() {
    let mut r = rig_with(&[(A, &[mode(640, 360), mode(1280, 720)]), (B, &[mode(640, 360)])]);
    let map_b = Mapping { mm_per_px: 0.5, origin_mm: [160.0, 300.0] };
    r.h.controller().attach_calibrations(store_with(vec![
        aligned(A, mode(640, 360), 0.0, MAP),
        aligned(B, mode(640, 360), 0.0, map_b),
    ]));
    use_camera(&mut r, A, mode(1280, 720));
    use_camera(&mut r, B, mode(640, 360));

    let v = go(&mut r, I::UpdateOverlay);

    let a = v.overlay.layers.iter().find(|l| l.device == dev(A)).unwrap();
    let b = v.overlay.layers.iter().find(|l| l.device == dev(B)).unwrap();
    assert!(matches!(a.not_placed, Some(CameraWhy::NotCalibrated(StoreMiss::Invalid(Invalidity::ResolutionChanged)))));
    assert_eq!(b.not_placed, None);
    assert_eq!(bed_images(&r).len(), 1);
}

#[test]
fn the_camera_started_in_the_strip_is_a_camera_of_the_bed_system_too() {
    let mut r = rig_a();
    r.h.controller().attach_calibrations(store_with(vec![aligned(A, mode(640, 360), 0.0, MAP)]));
    r.cam.show(A, mode(640, 360), grey(640, 360, 90));
    r.h.send(UiIntent::BedCamera(burnburn_presentation::BedCameraIntent::Start {
        device: dev(A),
        mode: mode(640, 360),
    }));

    let v = go(&mut r, I::UpdateOverlay);

    assert_eq!(v.cameras.len(), 1);
    assert_eq!(v.overlay.layers[0].not_placed, None);
}

#[test]
fn a_camera_is_never_started_or_asked_for_access_without_permission() {
    let cam = FakeCam::new(&[(A, &[mode(640, 360)])]);
    let mut h = HeadlessEditorHarness::new();
    h.controller().attach_camera(cam.clone());
    h.send(UiIntent::CameraWorkspace(I::UseCamera { device: dev(A), mode: mode(640, 360), lens: "standard".into() }));
    let v = h.controller().camera_workspace_view().unwrap();
    assert_eq!(v.last_refusal, Some(CameraWhy::CameraNotAuthorized));
    assert!(v.cameras.is_empty());
    assert!(cam.started.lock().unwrap().is_empty());
    assert_eq!(*cam.access_requests.lock().unwrap(), 0, "the OS prompt appears only on the access button");
}

#[test]
fn dropping_a_camera_stops_it_and_removes_its_picture() {
    let mut r = rig_a();
    r.h.controller().attach_calibrations(store_with(vec![aligned(A, mode(640, 360), 0.0, MAP)]));
    use_camera(&mut r, A, mode(640, 360));
    go(&mut r, I::UpdateOverlay);

    let v = go(&mut r, I::DropCamera { device: dev(A) });

    assert!(v.cameras.is_empty());
    assert!(v.overlay.layers.is_empty());
    assert!(bed_images(&r).is_empty());
    assert_eq!(*r.cam.stopped.lock().unwrap(), vec![dev(A)]);
}

/// Cells whose centre lies within the edge margin of their picture are not trusted: with a margin of
/// 15 px the outer ring of 20 px cells (centres 10 px from the edge) goes, 30 × 16 of 32 × 18 stay.
#[test]
fn an_edge_margin_drops_the_unreliable_rim_of_a_picture() {
    let mut r = rig_a();
    r.h.controller().attach_calibrations(store_with(vec![aligned(A, mode(640, 360), 0.0, MAP)]));
    use_camera(&mut r, A, mode(640, 360));
    go(&mut r, I::UpdateOverlay);
    assert_eq!(bed_images(&r)[0].1.len(), 32 * 18);

    let v = go(&mut r, I::SetEdgeMargin { px: "15".into() });

    assert_eq!(v.overlay.edge_margin_px, 15.0);
    assert_eq!(v.overlay.layers[0].cells, 30 * 16);
    assert_eq!(bed_images(&r)[0].1.len(), 30 * 16);
    let v = go(&mut r, I::SetEdgeMargin { px: "-2".into() });
    assert_eq!(v.last_refusal, Some(CameraWhy::InvalidNumber));
    assert_eq!(v.overlay.edge_margin_px, 15.0);
}

// --- material height ------------------------------------------------------------------------------

/// Two alignments of the same camera, at 0 and 10 mm; the camera sees the higher plane shifted by
/// 10 mm in x. At 5 mm the placement is halfway; outside 0..10 there is nothing to interpolate.
#[test]
fn the_material_height_selects_the_alignment_and_is_interpolated_between_two() {
    let mut r = rig_a();
    let high = Mapping { mm_per_px: 0.5, origin_mm: [110.0, 300.0] };
    r.h.controller().attach_calibrations(store_with(vec![
        aligned(A, mode(640, 360), 0.0, MAP),
        aligned(A, mode(640, 360), 10.0, high),
    ]));
    use_camera(&mut r, A, mode(640, 360));
    go(&mut r, I::UpdateOverlay);
    let first_x = |r: &Rig| bed_images(r)[0].1[0].bed_mm[0].x;
    assert!((first_x(&r) - 100.0).abs() < 1e-6, "height 0");

    go(&mut r, I::SetMaterialHeight { mm: "5".into() });
    assert!((first_x(&r) - 105.0).abs() < 1e-6, "halfway between the two alignments");
    go(&mut r, I::SetMaterialHeight { mm: "10 mm".into() });
    assert!((first_x(&r) - 110.0).abs() < 1e-6);

    let v = go(&mut r, I::SetMaterialHeight { mm: "20".into() });
    assert_eq!(v.material_height_mm, 20.0);
    assert!(matches!(
        v.overlay.layers[0].not_placed,
        Some(CameraWhy::NotCalibrated(StoreMiss::NoAlignmentForHeight { .. }))
    ));
    assert!(bed_images(&r).is_empty(), "no extrapolation");

    let v = go(&mut r, I::SetMaterialHeight { mm: "abc".into() });
    assert_eq!(v.last_refusal, Some(CameraWhy::InvalidNumber));
    assert_eq!(v.material_height_mm, 20.0, "unchanged");
    let v = go(&mut r, I::SetMaterialHeight { mm: "-1".into() });
    assert_eq!(v.last_refusal, Some(CameraWhy::InvalidNumber));
}

// --- managing calibrations -------------------------------------------------------------------------

#[test]
fn deleting_a_calibration_removes_only_that_one() {
    let mut r = rig_a();
    r.h.controller().attach_calibrations(store_with(vec![
        aligned(A, mode(640, 360), 0.0, MAP),
        aligned(A, mode(640, 360), 10.0, MAP),
        aligned(A, mode(1280, 720), 0.0, MAP),
    ]));
    let v = view(&mut r);
    assert_eq!(v.calibrations.len(), 3);
    let target = v
        .calibrations
        .iter()
        .find(|c| c.width == 640 && matches!(c.reference.kind, CalibrationKind::Alignment { height_um: 10_000 }))
        .unwrap()
        .reference
        .clone();

    let v = go(&mut r, I::DeleteCalibration { calibration: target.clone() });

    assert_eq!(v.calibrations.len(), 2);
    assert!(v.calibrations.iter().all(|c| c.reference != target));
    assert_eq!(r.h.controller().calibrations().alignment_count(), 2);

    let v = go(&mut r, I::DeleteCalibration { calibration: target });
    assert_eq!(v.last_refusal, Some(CameraWhy::NotCalibrated(StoreMiss::NotCalibrated)), "already gone");
    assert_eq!(v.calibrations.len(), 2);
}

#[test]
fn recalibrating_starts_the_wizard_for_the_row_only_when_the_camera_runs_in_that_configuration() {
    let mut r = rig_a();
    r.h.controller().attach_calibrations(store_with(vec![aligned(A, mode(640, 360), 7.5, MAP)]));
    let row = view(&mut r).calibrations[0].reference.clone();

    let v = go(&mut r, I::Recalibrate { calibration: row.clone() });
    assert_eq!(v.last_refusal, Some(CameraWhy::CameraNotRunning));
    assert!(v.alignment.is_none());

    use_camera(&mut r, A, mode(1280, 720));
    let v = go(&mut r, I::Recalibrate { calibration: row.clone() });
    assert_eq!(v.last_refusal, Some(CameraWhy::FrameConfigChanged), "the camera runs in another resolution");

    use_camera(&mut r, A, mode(640, 360));
    let v = go(&mut r, I::Recalibrate { calibration: row });
    let alignment = v.alignment.expect("the alignment wizard for that height");
    assert_eq!(alignment.material_height_mm, 7.5);
    assert_eq!(alignment.device, dev(A));
}

#[test]
fn recalibrating_a_lens_row_opens_the_lens_wizard() {
    let mut r = rig_a();
    use_camera(&mut r, A, mode(640, 360));
    start_lens(&mut r);
    for pose in 0..2 {
        photograph(&mut r, pose, -0.12);
        go(&mut r, I::AddPatternView);
    }
    go(&mut r, I::FitLens { two_coefficients: false });
    go(&mut r, I::AcceptLensCalibration);
    let row = view(&mut r).calibrations[0].reference.clone();
    assert_eq!(row.kind, CalibrationKind::Lens);

    let v = go(&mut r, I::Recalibrate { calibration: row });

    assert!(v.lens.is_some_and(|w| w.views.is_empty()), "a fresh wizard for the same camera");
}

// --- lens calibration wizard --------------------------------------------------------------------------

fn start_lens(r: &mut Rig) -> CameraWorkspaceView {
    go(
        r,
        I::StartLensCalibration {
            device: dev(A),
            cols: BOARD_COLS as u32,
            rows: BOARD_ROWS as u32,
            pitch_mm: "30".into(),
        },
    )
}

fn photograph(r: &mut Rig, pose: usize, k1: f64) {
    r.cam.show(A, mode(640, 360), photo_of_board(&POSES[pose], k1));
}

/// The whole wizard on rendered photos of a printed board seen through a lens with k1 = −0.12:
/// the corners are detected, the fit finds the lens and reports its residual, and accepting stores
/// the lens calibration for exactly this camera configuration.
#[test]
fn the_lens_wizard_detects_corners_fits_the_lens_and_stores_it_for_this_configuration() {
    let mut r = rig_a();
    use_camera(&mut r, A, mode(640, 360));
    let v = start_lens(&mut r);
    let wizard = v.lens.expect("wizard");
    assert_eq!((wizard.cols, wizard.rows, wizard.pitch_mm), (9, 6, 30.0));
    assert!(wizard.views.is_empty() && !wizard.can_fit);

    for pose in 0..3 {
        photograph(&mut r, pose, -0.12);
        go(&mut r, I::AddPatternView);
    }
    let wizard = view(&mut r).lens.unwrap();
    assert_eq!(wizard.views, vec![54, 54, 54], "{:?}", wizard.error);
    assert!(wizard.can_fit);

    let v = go(&mut r, I::FitLens { two_coefficients: false });
    let wizard = v.lens.unwrap();
    let result = wizard.result.expect("fit result");
    assert!((result.k1 + 0.12).abs() < 0.03, "k1 {}", result.k1);
    assert!(result.rms_px < 0.7, "rms {}", result.rms_px);
    assert_eq!((result.views, result.points), (3, 162));
    assert!(wizard.can_accept);

    let v = go(&mut r, I::AcceptLensCalibration);
    assert!(v.lens.is_none(), "the wizard is done");
    let cfg = config_key(&dev(A), &mode(640, 360), "standard");
    let stored = r.h.controller().calibrations().lens_for(&cfg).expect("stored for this configuration");
    assert!((stored.intrinsics.k1 + 0.12).abs() < 0.03);
    assert!(
        v.calibrations
            .iter()
            .any(|c| c.reference.kind == CalibrationKind::Lens && c.validity == CalibrationValidity::Valid)
    );
    let other = config_key(&dev(A), &mode(1280, 720), "standard");
    assert!(r.h.controller().calibrations().lens_for(&other).is_err());
}

#[test]
fn a_picture_without_the_board_is_a_typed_refusal_and_adds_no_view() {
    let mut r = rig_a();
    use_camera(&mut r, A, mode(640, 360));
    start_lens(&mut r);
    let v = go(&mut r, I::AddPatternView);
    let wizard = v.lens.unwrap();
    assert!(wizard.views.is_empty());
    assert_eq!(wizard.error, Some(CameraWhy::Corners(CornerError::NotFound { found: 0 })));
}

#[test]
fn fitting_without_views_and_accepting_without_a_result_are_typed_refusals() {
    let mut r = rig_a();
    use_camera(&mut r, A, mode(640, 360));
    start_lens(&mut r);
    let v = go(&mut r, I::FitLens { two_coefficients: false });
    assert_eq!(v.last_refusal, Some(CameraWhy::NoPatternViews));
    let v = go(&mut r, I::AcceptLensCalibration);
    assert_eq!(v.last_refusal, Some(CameraWhy::NoLensResult));
    assert_eq!(r.h.controller().calibrations().lens_count(), 0);
}

#[test]
fn a_frame_taken_in_another_mode_is_not_added_to_the_wizard() {
    let mut r = rig_a();
    use_camera(&mut r, A, mode(640, 360));
    start_lens(&mut r);
    r.cam.show(A, mode(1280, 720), grey(1280, 720, 100));
    let v = go(&mut r, I::AddPatternView);
    assert_eq!(v.last_refusal, Some(CameraWhy::FrameConfigChanged));
    assert!(v.lens.unwrap().views.is_empty());
}

/// Work that is queued but not yet run (a slow detection): cancelling the wizard cancels it, and the
/// result that arrives late changes nothing.
#[test]
fn cancelling_the_lens_wizard_discards_running_work_and_a_late_result_changes_nothing() {
    let mut r = rig_a();
    let queue: Arc<Mutex<Vec<Box<dyn FnOnce() + Send>>>> = Arc::default();
    let q = Arc::clone(&queue);
    let deferred: CameraSpawner = Arc::new(move |job| q.lock().unwrap().push(job));
    r.h.controller().set_camera_spawner(deferred);
    use_camera(&mut r, A, mode(640, 360));
    start_lens(&mut r);
    photograph(&mut r, 0, -0.12);
    let v = go(&mut r, I::AddPatternView);
    assert!(v.busy && v.lens.as_ref().unwrap().detecting, "detection is queued");
    assert!(go(&mut r, I::AddPatternView).last_refusal == Some(CameraWhy::Busy), "one job at a time");

    let v = go(&mut r, I::CancelLensCalibration);
    assert!(v.lens.is_none());
    for job in std::mem::take(&mut *queue.lock().unwrap()) {
        job(); // the cancelled work finishes (or stops) and reports
    }
    let v = go(&mut r, I::AcceptLensCalibration);
    assert!(v.lens.is_none(), "a late result does not bring the wizard back");
    assert_eq!(r.h.controller().calibrations().lens_count(), 0);
    assert!(!v.busy);
}

/// The detection of the first wizard finishes (successfully) but is not yet delivered; the wizard is
/// cancelled and started again. When the result is delivered it belongs to a request that is no
/// longer current, so the new wizard must not receive the old photo.
#[test]
fn a_result_for_a_restarted_wizard_is_rejected_by_its_request_identity() {
    let mut r = rig_a();
    use_camera(&mut r, A, mode(640, 360));
    start_lens(&mut r);
    photograph(&mut r, 0, -0.12);
    let send = |r: &mut Rig, i: I| {
        r.h.send(UiIntent::CameraWorkspace(i));
    };
    send(&mut r, I::AddPatternView); // inline spawner: the corners are found and wait for delivery
    send(&mut r, I::CancelLensCalibration);
    send(&mut r, I::StartLensCalibration { device: dev(A), cols: 9, rows: 6, pitch_mm: "30".into() });

    r.h.controller().poll_camera_work(); // the first wizard's result arrives now
    let v = view(&mut r);

    assert!(v.lens.unwrap().views.is_empty(), "the first wizard's photo is not in the second");
    let v = go(&mut r, I::FitLens { two_coefficients: false });
    assert_eq!(v.last_refusal, Some(CameraWhy::NoPatternViews));
}

#[test]
fn a_view_can_be_removed_again() {
    let mut r = rig_a();
    use_camera(&mut r, A, mode(640, 360));
    start_lens(&mut r);
    // Through the lens that pulls the whole board into the picture (see the lens wizard test).
    photograph(&mut r, 0, -0.12);
    go(&mut r, I::AddPatternView);
    photograph(&mut r, 1, -0.12);
    go(&mut r, I::AddPatternView);
    assert_eq!(view(&mut r).lens.unwrap().views.len(), 2);
    let v = go(&mut r, I::RemovePatternView { index: 0 });
    assert_eq!(v.lens.unwrap().views.len(), 1);
    let v = go(&mut r, I::RemovePatternView { index: 5 });
    assert_eq!(v.last_refusal, Some(CameraWhy::MarkIndex));
}

#[test]
fn a_bad_pattern_description_is_refused_before_the_wizard_opens() {
    let mut r = rig_a();
    use_camera(&mut r, A, mode(640, 360));
    for (cols, rows, pitch) in [(1, 6, "30"), (9, 6, "0"), (9, 6, "abc"), (9, 6, "-3")] {
        let v = go(&mut r, I::StartLensCalibration { device: dev(A), cols, rows, pitch_mm: pitch.into() });
        assert!(v.lens.is_none(), "{cols} {rows} {pitch}");
        assert!(matches!(v.last_refusal, Some(CameraWhy::BadGrid | CameraWhy::InvalidNumber)));
    }
    let v = go(&mut r, I::StartLensCalibration { device: dev(B), cols: 9, rows: 6, pitch_mm: "30".into() });
    assert_eq!(v.last_refusal, Some(CameraWhy::CameraNotRunning));
}

// --- workspace alignment wizard -------------------------------------------------------------------------

use burnburn_machine::{MachineId, MachineIdentity, Request};
use burnburn_presentation::Role;
use common::laser::{Lab, lab};

/// The camera wizard on a straight-down mapping: marks at their default places, the photo, clicks at
/// the pixels where the mapping puts each mark (plus a deliberate 2 mm error on one check mark).
fn start_alignment(r: &mut Rig, height: &str) -> CameraWorkspaceView {
    go(r, I::StartAlignment { device: dev(A), material_height_mm: height.into() })
}

fn attach_machine(r: &mut Rig, lab: &Lab) {
    r.h.controller().attach_machine(lab.service.clone());
    r.h.send(UiIntent::SetMachineProfile { profile: Box::new(common::profile(1)), ruida: common::RUIDA });
}

#[test]
fn the_alignment_wizard_proposes_a_layout_of_fit_and_check_marks_inside_the_bed() {
    let mut r = rig_a();
    let lab = lab();
    attach_machine(&mut r, &lab);
    use_camera(&mut r, A, mode(640, 360));

    let v = start_alignment(&mut r, "0");

    let wizard = v.alignment.expect("wizard");
    let fit = wizard.marks.iter().filter(|m| m.role == burnburn_image::calib::MarkRole::Fit).count();
    let check = wizard.marks.iter().filter(|m| m.role == burnburn_image::calib::MarkRole::Check).count();
    assert!(fit >= 4 && check >= 1, "{fit} fit marks and {check} check marks");
    for m in &wizard.marks {
        assert!(
            (0.0..=1500.0).contains(&m.bed_mm[0]) && (0.0..=900.0).contains(&m.bed_mm[1]),
            "{m:?} on the 1500 × 900 bed"
        );
        assert_eq!(m.pixel, None);
    }
    assert!(wizard.proposal.is_none() && wizard.photo.is_none() && wizard.result.is_none());
}

#[test]
fn marks_are_proposed_without_sending_and_sent_only_after_the_user_confirms_that_job() {
    let mut r = rig_a();
    let lab = lab();
    attach_machine(&mut r, &lab);
    use_camera(&mut r, A, mode(640, 360));
    start_alignment(&mut r, "0");

    let v = go(&mut r, I::ProposeMarksJob { power_percent: "12".into(), speed_mm_s: "80".into() });

    let proposal = v.alignment.unwrap().proposal.expect("proposal");
    assert_eq!((proposal.power_percent, proposal.speed_mm_s), (12.0, 80.0));
    assert!(!proposal.sent);
    assert!(lab.peer.sent().is_empty(), "proposing sends nothing to the machine");
    assert!(lab.history.inner.executions.lock().unwrap().is_empty());
    let confirm = proposal.confirm.clone().expect("the machine is idle: confirmation offered");

    r.h.send(UiIntent::CameraWorkspace(I::ConfirmMarksJob {
        plan: proposal.plan,
        action: confirm.action,
        seen: confirm.seen,
    }));

    let sent = lab.peer.sent();
    assert!(
        matches!(sent.first(), Some(Request::QueryIdentity)),
        "the upload starts with the identity check: {sent:?}"
    );
    let executions = lab.history.inner.executions.lock().unwrap();
    assert_eq!(executions.len(), 1, "exactly one job was handed to the machine service");
    assert_eq!(executions[0].job.plan, proposal.plan, "the job that was shown is the job that was sent");
    drop(executions);
    let machine = r.h.machine_view().unwrap();
    assert_eq!(machine.status, burnburn_presentation::MachineStatusKey::VerifyingIdentity);
    assert!(view(&mut r).alignment.unwrap().proposal.unwrap().sent);
    assert!(
        !matches!(machine.status, burnburn_presentation::MachineStatusKey::Running),
        "starting stays an explicit step in the machine panel"
    );
}

/// The job burns one small cross per mark: every burning move lies within the arm length of a mark,
/// every mark is crossed, and nothing leaves the bed.
#[test]
fn the_marks_job_burns_a_cross_on_every_mark_and_nothing_else() {
    let mut r = rig_a();
    let lab = lab();
    attach_machine(&mut r, &lab);
    use_camera(&mut r, A, mode(640, 360));
    let marks: Vec<[f64; 2]> = start_alignment(&mut r, "0").alignment.unwrap().marks.iter().map(|m| m.bed_mm).collect();
    let v = go(&mut r, I::ProposeMarksJob { power_percent: "10".into(), speed_mm_s: "100".into() });
    let proposal = v.alignment.unwrap().proposal.unwrap();
    assert_eq!(proposal.marks_mm, marks);
    let confirm = proposal.confirm.clone().unwrap();
    r.h.send(UiIntent::CameraWorkspace(I::ConfirmMarksJob {
        plan: proposal.plan,
        action: confirm.action,
        seen: confirm.seen,
    }));

    let executions = lab.history.inner.executions.lock().unwrap();
    let plan = &executions[0].job.inputs.as_ref().expect("frozen inputs").plan;
    use burnburn_job::PlanEvent;
    let mut at = plan.start();
    let mut burned: Vec<(burnburn_job::MachinePoint, burnburn_job::MachinePoint)> = Vec::new();
    for e in plan.events() {
        match e {
            PlanEvent::Travel { to } => at = *to,
            PlanEvent::Cut { to, .. } => {
                burned.push((at, *to));
                at = *to;
            }
            _ => {}
        }
    }
    assert_eq!(burned.len(), marks.len() * 2, "two strokes per cross");
    for (from, to) in &burned {
        assert!(to.x_mm() >= 0.0 && to.x_mm() <= 1500.0 && to.y_mm() >= 0.0 && to.y_mm() <= 900.0);
        let near_a_mark = marks.iter().any(|m| {
            let mid = [(from.x_mm() + to.x_mm()) / 2.0, (from.y_mm() + to.y_mm()) / 2.0];
            (mid[0] - m[0]).abs() < 1e-6 && (mid[1] - m[1]).abs() < 1e-6
        });
        assert!(near_a_mark, "each stroke is centred on a mark: {from:?} → {to:?}");
    }
    let settings = plan.settings();
    assert_eq!(settings.len(), 1);
    assert_eq!(settings[0].speed_mm_s, 100.0);
    assert!((settings[0].max_power.get() - 0.10).abs() < 1e-9);
}

#[test]
fn a_proposal_that_is_no_longer_the_one_on_screen_is_not_sent() {
    let mut r = rig_a();
    let lab = lab();
    attach_machine(&mut r, &lab);
    use_camera(&mut r, A, mode(640, 360));
    start_alignment(&mut r, "0");
    let v = go(&mut r, I::ProposeMarksJob { power_percent: "12".into(), speed_mm_s: "80".into() });
    let old = v.alignment.unwrap().proposal.unwrap();
    let confirm = old.confirm.clone().unwrap();

    // The user moves a mark: the proposal is gone, and confirming the old one does nothing.
    let v = go(&mut r, I::SetAlignmentMark { index: 0, x_mm: "123".into(), y_mm: "234".into() });
    assert!(v.alignment.unwrap().proposal.is_none());
    let v = go(&mut r, I::ConfirmMarksJob { plan: old.plan, action: confirm.action, seen: confirm.seen });

    assert_eq!(v.last_refusal, Some(CameraWhy::NotProposed));
    assert!(lab.peer.sent().is_empty());
    assert!(lab.history.inner.executions.lock().unwrap().is_empty());
}

#[test]
fn marks_that_leave_the_bed_or_a_too_fast_speed_give_no_proposal() {
    let mut r = rig_a();
    let lab = lab();
    attach_machine(&mut r, &lab);
    use_camera(&mut r, A, mode(640, 360));
    start_alignment(&mut r, "0");

    go(&mut r, I::SetAlignmentMark { index: 0, x_mm: "1499".into(), y_mm: "100".into() }); // the cross would leave the bed
    let v = go(&mut r, I::ProposeMarksJob { power_percent: "10".into(), speed_mm_s: "100".into() });
    assert_eq!(v.last_refusal, Some(CameraWhy::MarksOutsideBed));
    assert!(v.alignment.unwrap().proposal.is_none());

    go(&mut r, I::SetAlignmentMark { index: 0, x_mm: "100".into(), y_mm: "100".into() });
    let v = go(&mut r, I::ProposeMarksJob { power_percent: "10".into(), speed_mm_s: "5000".into() });
    assert_eq!(v.last_refusal, Some(CameraWhy::SpeedTooHigh), "profile maximum is 1000 mm/s");
    for (power, speed) in [("0", "100"), ("101", "100"), ("abc", "100"), ("10", "0"), ("10", "x")] {
        let v = go(&mut r, I::ProposeMarksJob { power_percent: power.into(), speed_mm_s: speed.into() });
        assert_eq!(v.last_refusal, Some(CameraWhy::InvalidNumber), "{power} {speed}");
    }
    assert!(lab.peer.sent().is_empty());
}

#[test]
fn without_a_machine_or_a_profile_the_marks_job_is_refused_with_a_typed_reason() {
    let mut r = rig_a();
    use_camera(&mut r, A, mode(640, 360));
    start_alignment(&mut r, "0");
    let v = go(&mut r, I::ProposeMarksJob { power_percent: "10".into(), speed_mm_s: "100".into() });
    assert_eq!(v.last_refusal, Some(CameraWhy::NoMachineProfile));

    let lab = lab();
    r.h.controller().attach_machine(lab.service.clone()); // a machine, but no profile yet
    let v = go(&mut r, I::ProposeMarksJob { power_percent: "10".into(), speed_mm_s: "100".into() });
    assert_eq!(v.last_refusal, Some(CameraWhy::NoMachineProfile));
}

/// A controller whose identity is not configured can never be checked, so no job byte may go to it.
#[test]
fn a_machine_that_cannot_be_verified_refuses_the_marks_job_and_nothing_is_sent() {
    let mut r = rig_a();
    let lab = lab();
    let unverifiable = {
        use burnburn_machine::ExecutionActor;
        let machine = MachineId::new(common::MACHINE);
        let port = common::FakePort::new(&lab.peer, machine.clone());
        let policy = burnburn_machine::Policy { reply_timeout: common::TIMEOUT, max_query_attempts: 2 };
        let identity = MachineIdentity { id: machine, model: None, firmware: None };
        let actor = ExecutionActor::new(identity, port, Arc::new(lab.clock.clone()), policy);
        Arc::new(burnburn_presentation::MachineService::new(actor, lab.history.clone(), common::CHUNK).unwrap())
    };
    r.h.controller().attach_machine(unverifiable);
    r.h.send(UiIntent::SetMachineProfile { profile: Box::new(common::profile(1)), ruida: common::RUIDA });
    use_camera(&mut r, A, mode(640, 360));
    start_alignment(&mut r, "0");
    let v = go(&mut r, I::ProposeMarksJob { power_percent: "10".into(), speed_mm_s: "100".into() });
    let proposal = v.alignment.unwrap().proposal.expect("a proposal can still be shown");
    let confirm = proposal.confirm.clone().expect("offered; the machine service decides");

    let v = go(&mut r, I::ConfirmMarksJob { plan: proposal.plan, action: confirm.action, seen: confirm.seen });

    assert_eq!(v.last_refusal, Some(CameraWhy::MachineRefused(MachineWhy::IdentityNotConfigured)));
    assert!(lab.peer.sent().is_empty(), "no byte went to the controller");
    assert!(!v.alignment.unwrap().proposal.unwrap().sent);
}

#[test]
fn an_operator_cannot_send_the_marks_job() {
    let mut r = rig_a();
    let lab = lab();
    attach_machine(&mut r, &lab);
    use_camera(&mut r, A, mode(640, 360));
    start_alignment(&mut r, "0");
    let v = go(&mut r, I::ProposeMarksJob { power_percent: "10".into(), speed_mm_s: "100".into() });
    let proposal = v.alignment.unwrap().proposal.unwrap();
    let confirm = proposal.confirm.clone().unwrap();
    r.h.controller().set_role(Role::Operator);

    // Operators cannot reach the camera workspace at all.
    r.h.send(UiIntent::CameraWorkspace(I::ConfirmMarksJob {
        plan: proposal.plan,
        action: confirm.action,
        seen: confirm.seen,
    }));
    assert!(lab.peer.sent().is_empty());
    assert!(lab.history.inner.executions.lock().unwrap().is_empty());
}

#[test]
fn a_job_in_progress_withholds_the_confirmation_with_the_reason() {
    let mut r = rig_a();
    let lab = lab();
    attach_machine(&mut r, &lab);
    use_camera(&mut r, A, mode(640, 360));
    start_alignment(&mut r, "0");
    let v = go(&mut r, I::ProposeMarksJob { power_percent: "10".into(), speed_mm_s: "100".into() });
    let proposal = v.alignment.unwrap().proposal.unwrap();
    let confirm = proposal.confirm.unwrap();
    r.h.send(UiIntent::CameraWorkspace(I::ConfirmMarksJob {
        plan: proposal.plan,
        action: confirm.action,
        seen: confirm.seen,
    }));

    // The marks job is still being checked and stored: another one cannot be sent now.
    let again = go(&mut r, I::ProposeMarksJob { power_percent: "10".into(), speed_mm_s: "100".into() });
    let proposal = again.alignment.unwrap().proposal.unwrap();
    assert!(proposal.confirm.is_none());
    assert_eq!(proposal.confirm_why_not, Some(CameraWhy::MachineRefused(MachineWhy::Busy)));
}

/// Photo, clicks, fit, save: the marks are clicked where the mapping puts them; the fit uses the four
/// fit marks, the check marks measure it independently (one is clicked 2 mm off on purpose).
#[test]
fn clicked_marks_fit_an_alignment_whose_check_residuals_are_independent_and_saving_makes_it_usable() {
    let mut r = rig_a();
    use_camera(&mut r, A, mode(640, 360));
    let wizard = start_alignment(&mut r, "0").alignment.unwrap();
    assert!(!wizard.can_fit);
    let v = go(&mut r, I::CaptureAlignmentPhoto);
    assert_eq!(v.alignment.unwrap().photo, Some((640, 360)));

    // The marks lie where the default layout puts them; the camera sees bed = (100 + 0.5·px, 300 − 0.5·py)
    // only for part of the bed, so move the marks onto the part it sees.
    let places: [[f64; 2]; 6] =
        [[130.0, 280.0], [390.0, 280.0], [390.0, 140.0], [130.0, 140.0], [260.0, 210.0], [200.0, 170.0]];
    for (i, p) in places.iter().enumerate() {
        go(&mut r, I::SetAlignmentMark { index: i, x_mm: p[0].to_string(), y_mm: p[1].to_string() });
    }
    let marks = view(&mut r).alignment.unwrap().marks;
    assert_eq!(marks.len(), 6, "the default layout has four fit and two check marks");
    for (i, p) in places.iter().enumerate() {
        let mut pixel = MAP.pixel(*p);
        if i == 5 {
            pixel[0] += 4.0; // 4 px = 2 mm along x
        }
        let v = go(&mut r, I::PlaceAlignmentMark { index: i, pixel });
        assert_eq!(v.last_refusal, None);
    }
    assert!(view(&mut r).alignment.unwrap().can_fit);

    let v = go(&mut r, I::FitAlignment);

    let result = v.alignment.as_ref().unwrap().result.clone().expect("fit result");
    assert!(result.fit_rms_mm < 1e-6, "exact marks: {}", result.fit_rms_mm);
    assert_eq!(result.check_errors_mm.len(), 2);
    assert!(result.check_errors_mm[0] < 1e-6, "{:?}", result.check_errors_mm);
    assert!(
        (result.check_errors_mm[1] - 2.0).abs() < 1e-6,
        "the 2 mm click error shows up: {:?}",
        result.check_errors_mm
    );
    assert!((result.check_max_mm - 2.0).abs() < 1e-6);

    let v = go(&mut r, I::SaveAlignment);
    assert!(v.alignment.is_none());
    assert_eq!(r.h.controller().calibrations().alignment_count(), 1);
    let cfg = config_key(&dev(A), &mode(640, 360), "standard");
    assert!(r.h.controller().calibrations().alignment_for(&cfg, 0.0).is_ok());

    // And the picture can be placed with it.
    let v = go(&mut r, I::UpdateOverlay);
    assert_eq!(v.overlay.layers[0].not_placed, None);
    let first = bed_images(&r)[0].1[0].bed_mm[0];
    near([first.x, first.y], MAP.bed([0.0, 0.0]));
}

/// W05 `collinear_marks_give_no_valid_registration`, through the wizard: marks that all lie on one line
/// fix no homography. The fit is refused with the typed reason, no result is offered and nothing can be
/// saved, however consistent the clicks are.
#[test]
fn collinear_marks_give_no_valid_registration() {
    let mut r = rig_a();
    use_camera(&mut r, A, mode(640, 360));
    start_alignment(&mut r, "0");
    go(&mut r, I::CaptureAlignmentPhoto);
    for i in 0..6 {
        let bed = [100.0 + 60.0 * i as f64, 200.0];
        go(&mut r, I::SetAlignmentMark { index: i, x_mm: bed[0].to_string(), y_mm: bed[1].to_string() });
        go(&mut r, I::PlaceAlignmentMark { index: i, pixel: MAP.pixel(bed) });
    }

    let v = go(&mut r, I::FitAlignment);

    assert_eq!(
        v.last_refusal,
        Some(CameraWhy::Align(burnburn_image::calib::AlignError::Calibration(
            burnburn_image::calib::CalibError::Collinear
        )))
    );
    let wizard = v.alignment.unwrap();
    assert!(wizard.result.is_none() && !wizard.can_save);
    let v = go(&mut r, I::SaveAlignment);
    assert_eq!(v.last_refusal, Some(CameraWhy::NoAlignmentResult));
    assert_eq!(r.h.controller().calibrations().alignment_count(), 0);
}

#[test]
fn too_few_marks_a_click_outside_the_photo_or_a_missing_photo_are_typed_refusals() {
    let mut r = rig_a();
    use_camera(&mut r, A, mode(640, 360));
    start_alignment(&mut r, "0");

    let v = go(&mut r, I::PlaceAlignmentMark { index: 0, pixel: [10.0, 10.0] });
    assert_eq!(v.last_refusal, Some(CameraWhy::NoPhoto));
    let v = go(&mut r, I::FitAlignment);
    assert_eq!(v.last_refusal, Some(CameraWhy::NoPhoto));

    go(&mut r, I::CaptureAlignmentPhoto);
    for outside in [[-1.0, 5.0], [641.0, 5.0], [5.0, 361.0], [f64::NAN, 5.0]] {
        let v = go(&mut r, I::PlaceAlignmentMark { index: 0, pixel: outside });
        assert_eq!(v.last_refusal, Some(CameraWhy::OutsideImage), "{outside:?}");
        assert_eq!(v.alignment.unwrap().marks[0].pixel, None);
    }
    let v = go(&mut r, I::PlaceAlignmentMark { index: 99, pixel: [5.0, 5.0] });
    assert_eq!(v.last_refusal, Some(CameraWhy::MarkIndex));

    go(&mut r, I::PlaceAlignmentMark { index: 0, pixel: [100.0, 100.0] });
    let v = go(&mut r, I::FitAlignment);
    assert_eq!(v.last_refusal, Some(CameraWhy::MarksIncomplete));
    assert!(v.alignment.unwrap().result.is_none());
    let v = go(&mut r, I::SaveAlignment);
    assert_eq!(v.last_refusal, Some(CameraWhy::NoAlignmentResult));
    assert_eq!(r.h.controller().calibrations().alignment_count(), 0);
}

#[test]
fn cancelling_the_alignment_wizard_keeps_the_store_as_it_was() {
    let mut r = rig_a();
    use_camera(&mut r, A, mode(640, 360));
    start_alignment(&mut r, "3");
    go(&mut r, I::CaptureAlignmentPhoto);
    let v = go(&mut r, I::CancelAlignment);
    assert!(v.alignment.is_none());
    assert_eq!(r.h.controller().calibrations().alignment_count(), 0);
    let v = go(&mut r, I::FitAlignment);
    assert_eq!(v.last_refusal, Some(CameraWhy::NoAlignmentWizard));
}

#[test]
fn a_lens_calibration_of_the_configuration_is_used_by_the_alignment_fit() {
    let mut r = rig_a();
    use_camera(&mut r, A, mode(640, 360));
    start_lens(&mut r);
    for pose in 0..3 {
        photograph(&mut r, pose, -0.12);
        go(&mut r, I::AddPatternView);
    }
    go(&mut r, I::FitLens { two_coefficients: false });
    go(&mut r, I::AcceptLensCalibration);
    r.cam.show(A, mode(640, 360), grey(640, 360, 128));

    let v = start_alignment(&mut r, "0");

    assert!(v.alignment.unwrap().uses_lens_calibration);
}
