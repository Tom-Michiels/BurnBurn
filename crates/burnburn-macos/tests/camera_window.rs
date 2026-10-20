//! W3-UI-camera, headless: the camera window says what the presentation's camera workspace says
//! (an invalid calibration is badged with the reason, a refusal is in words, a control that cannot
//! act is off and sends nothing), and every control becomes the `UiIntent` of the view model it was
//! offered in. The camera is a fake `CameraPort`; no AVFoundation, no privacy prompt.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use burnburn_image::SourceImage;
use burnburn_image::calib::{
    AlignmentLens, CameraConfigKey, CornerError, Invalidity, LensModel, MarkObservation, MarkRole, StoreMiss,
    WorkspaceAlignment,
};
use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::{
    CameraCommand, CameraInputs, CameraText, all_camera_texts, calibration_row_text, camera_command_intent,
    camera_text, camera_why_text, camera_window_panel, height_intent, margin_intent, next_unplaced_mark,
    opacity_intent, overlay_visible_intent, photo_click_intent, photo_click_to_pixel, photo_with_marks,
};
use burnburn_presentation::{
    CameraAuthorization, CameraDevice, CameraDeviceId, CameraError, CameraFrame, CameraMode, CameraPixelFormat,
    CameraPort, CameraWhy, CameraWorkspaceIntent as I, CameraWorkspaceView, HeadlessEditorHarness, MachineWhy,
    UiIntent, config_key,
};
use burnburn_types::MonotonicTime;

#[derive(Default)]
struct FakeCam {
    frame: Mutex<Option<Arc<CameraFrame>>>,
}

fn mode(width: u32) -> CameraMode {
    CameraMode { width, height: width / 2, fps_milli: 30_000, pixel_format: CameraPixelFormat::Bgra8 }
}

fn cam() -> CameraDeviceId {
    CameraDeviceId("cam-A".into())
}

impl CameraPort for FakeCam {
    fn devices(&self) -> Result<Vec<CameraDevice>, CameraError> {
        Ok(vec![CameraDevice {
            id: cam(),
            name: "Bedcamera".into(),
            modes: vec![mode(640), mode(1280), CameraMode { pixel_format: CameraPixelFormat::Yuy2, ..mode(1920) }],
        }])
    }
    fn authorization(&self) -> CameraAuthorization {
        CameraAuthorization::Authorized
    }
    fn request_authorization(&self) {}
    fn start(&self, _: &CameraDeviceId, _: &CameraMode) -> Result<(), CameraError> {
        Ok(())
    }
    fn stop(&self, _: &CameraDeviceId) {}
    fn latest_frame(&self, d: &CameraDeviceId) -> Option<Arc<CameraFrame>> {
        let _ = d;
        self.frame.lock().unwrap().clone()
    }
}

fn alignment(width: u32) -> WorkspaceAlignment {
    let m = mode(width);
    let (w, h) = (f64::from(m.width), f64::from(m.height));
    let mark = |fx: f64, fy: f64, role| {
        let pixel = [w * fx, h * fy];
        MarkObservation { pixel, bed_mm: [pixel[0] * 0.5, 300.0 - pixel[1] * 0.5], role }
    };
    let marks = vec![
        mark(0.1, 0.1, MarkRole::Fit),
        mark(0.9, 0.1, MarkRole::Fit),
        mark(0.9, 0.9, MarkRole::Fit),
        mark(0.1, 0.9, MarkRole::Fit),
        mark(0.5, 0.5, MarkRole::Check),
    ];
    WorkspaceAlignment::align(
        &marks,
        config_key(&cam(), &m, "standard"),
        AlignmentLens::Profile(LensModel::Fixed { k1: 0.0 }),
        0.0,
        0.5,
    )
    .unwrap()
}

struct Rig {
    h: HeadlessEditorHarness,
    cam: Arc<FakeCam>,
}

fn rig() -> Rig {
    let cam = Arc::new(FakeCam::default());
    let mut h = HeadlessEditorHarness::new();
    h.controller().attach_camera(cam.clone());
    h.controller().set_camera_spawner(Arc::new(|job| job()));
    let mut store = burnburn_image::calib::CalibrationStore::default();
    store.insert_alignment(alignment(640));
    h.controller().attach_calibrations(store);
    Rig { h, cam }
}

fn view(r: &mut Rig) -> CameraWorkspaceView {
    r.h.controller().camera_workspace_view().unwrap()
}

fn go(r: &mut Rig, intent: I) -> CameraWorkspaceView {
    r.h.send(UiIntent::CameraWorkspace(intent));
    r.h.controller().poll_camera_work();
    view(r)
}

fn panel(r: &mut Rig, locale: Locale) -> burnburn_macos::panels::CameraWindowPanel {
    let primary = r.h.controller().camera_view();
    camera_window_panel(Some(&view(r)), primary.as_ref(), locale)
}

fn start_camera(r: &mut Rig, width: u32) {
    let m = mode(width);
    *r.cam.frame.lock().unwrap() = Some(Arc::new(CameraFrame {
        device: cam(),
        mode: m,
        seq: 1,
        image: SourceImage::from_rgba8(m.width, m.height, vec![100; (m.width * m.height * 4) as usize]).unwrap(),
        time: MonotonicTime::ZERO,
    }));
    go(r, I::UseCamera { device: cam(), mode: m, lens: "standard".into() });
}

#[test]
fn every_text_of_the_window_exists_in_both_languages() {
    for text in all_camera_texts() {
        let (nl, en) = (camera_text(text, Locale::Nl), camera_text(text, Locale::En));
        assert!(!nl.trim().is_empty() && !en.trim().is_empty(), "{text:?}");
    }
    let sentences = [
        CameraWhy::NoCameraPort,
        CameraWhy::CameraNotAuthorized,
        CameraWhy::CameraNotRunning,
        CameraWhy::NoFrame,
        CameraWhy::FrameConfigChanged,
        CameraWhy::InvalidNumber,
        CameraWhy::NothingToUpdate,
        CameraWhy::NotCalibrated(StoreMiss::NotCalibrated),
        CameraWhy::NotCalibrated(StoreMiss::Invalid(Invalidity::ResolutionChanged)),
        CameraWhy::NotCalibrated(StoreMiss::NoAlignmentForHeight { available_mm: vec![0.0, 10.0] }),
        CameraWhy::Corners(CornerError::NotFound { found: 0 }),
        CameraWhy::NoPatternViews,
        CameraWhy::BadGrid,
        CameraWhy::NoPhoto,
        CameraWhy::MarksIncomplete,
        CameraWhy::NoMachine,
        CameraWhy::NoMachineProfile,
        CameraWhy::MarksOutsideBed,
        CameraWhy::SpeedTooHigh,
        CameraWhy::NotProposed,
        CameraWhy::MachineRefused(MachineWhy::IdentityNotConfigured),
        CameraWhy::MachineRefused(MachineWhy::Busy),
        CameraWhy::MachineRefused(MachineWhy::StateChanged),
    ];
    for why in sentences {
        let (nl, en) = (camera_why_text(&why, Locale::Nl), camera_why_text(&why, Locale::En));
        assert!(!nl.is_empty() && !en.is_empty() && nl != en, "{why:?}: {nl} / {en}");
        assert!(!nl.contains("Debug") && !nl.contains('{'), "no raw debug output in {nl}");
    }
}

/// W04: the row of a calibration whose camera changed resolution is badged, in words, with the
/// reason — not hidden and not shown as valid.
#[test]
fn a_calibration_that_no_longer_applies_is_badged_invalid_with_the_reason() {
    let mut r = rig();
    start_camera(&mut r, 640);
    let valid = panel(&mut r, Locale::Nl);
    assert_eq!(valid.calibrations.len(), 1);
    assert!(valid.calibrations[0].contains("geldig") && !valid.calibrations[0].contains("ONGELDIG"), "{valid:?}");

    start_camera(&mut r, 1280);

    let nl = panel(&mut r, Locale::Nl);
    assert!(nl.calibrations[0].contains("ONGELDIG: resolutie gewijzigd"), "{}", nl.calibrations[0]);
    assert!(nl.calibrations[0].contains("640×320"), "the row names the resolution it was made for");
    let en = panel(&mut r, Locale::En);
    assert!(en.calibrations[0].contains("INVALID: resolution changed"), "{}", en.calibrations[0]);
    let _ = calibration_row_text;
}

#[test]
fn a_camera_that_is_not_in_use_is_not_called_valid() {
    let mut r = rig();
    let p = panel(&mut r, Locale::En);
    assert!(p.calibrations[0].contains("camera not in use"), "{}", p.calibrations[0]);
    assert!(p.running.is_empty());
}

#[test]
fn the_refusal_of_the_last_action_is_shown_in_words() {
    let mut r = rig();
    go(&mut r, I::UpdateOverlay); // no camera running
    let p = panel(&mut r, Locale::Nl);
    assert_eq!(p.refusal, camera_why_text(&CameraWhy::NothingToUpdate, Locale::Nl));
    assert!(!p.is_enabled(CameraCommand::UpdateOverlay), "and the button is off");
}

#[test]
fn a_button_that_cannot_act_sends_nothing() {
    let mut r = rig();
    let primary = r.h.controller().camera_view();
    let ws = view(&mut r);
    let inputs = CameraInputs::default();
    for command in [
        CameraCommand::UpdateOverlay,
        CameraCommand::DropCamera,
        CameraCommand::StartLens,
        CameraCommand::AddPhoto,
        CameraCommand::FitLens,
        CameraCommand::AcceptLens,
        CameraCommand::StartAlignment,
        CameraCommand::ProposeMarks,
        CameraCommand::SendMarks,
        CameraCommand::TakePhoto,
        CameraCommand::FitAlignment,
        CameraCommand::SaveAlignment,
    ] {
        assert_eq!(camera_command_intent(command, Some(&ws), primary.as_ref(), &inputs), None, "{command:?}");
    }
    assert_eq!(camera_command_intent(CameraCommand::UpdateOverlay, None, None, &inputs), None, "no workspace");
}

#[test]
fn the_buttons_become_the_intents_of_the_state_they_were_offered_in() {
    let mut r = rig();
    start_camera(&mut r, 640);
    let primary = r.h.controller().camera_view();
    let ws = view(&mut r);
    let intent = |c, inputs: &CameraInputs| camera_command_intent(c, Some(&ws), primary.as_ref(), inputs);
    let base = CameraInputs {
        cols: "9".into(),
        rows: "6".into(),
        pitch: "25".into(),
        align_height: "2,5".into(),
        ..CameraInputs::default()
    };

    assert_eq!(intent(CameraCommand::UpdateOverlay, &base), Some(UiIntent::CameraWorkspace(I::UpdateOverlay)));
    assert_eq!(
        intent(CameraCommand::StartLens, &base),
        Some(UiIntent::CameraWorkspace(I::StartLensCalibration {
            device: cam(),
            cols: 9,
            rows: 6,
            pitch_mm: "25".into()
        }))
    );
    // Not a number of corners: presentation gets 0 and refuses it as a bad grid.
    let junk = CameraInputs { cols: "negen".into(), ..base.clone() };
    assert_eq!(
        intent(CameraCommand::StartLens, &junk),
        Some(UiIntent::CameraWorkspace(I::StartLensCalibration {
            device: cam(),
            cols: 0,
            rows: 6,
            pitch_mm: "25".into()
        }))
    );
    assert_eq!(
        intent(CameraCommand::StartAlignment, &base),
        Some(UiIntent::CameraWorkspace(I::StartAlignment { device: cam(), material_height_mm: "2,5".into() })),
        "the typed height goes to presentation as typed"
    );
    let with_calibration = CameraInputs { calibration: 0, ..base.clone() };
    let row = ws.calibrations[0].reference.clone();
    assert_eq!(
        intent(CameraCommand::DeleteCalibration, &with_calibration),
        Some(UiIntent::CameraWorkspace(I::DeleteCalibration { calibration: row.clone() }))
    );
    assert_eq!(
        intent(CameraCommand::Recalibrate, &with_calibration),
        Some(UiIntent::CameraWorkspace(I::Recalibrate { calibration: row }))
    );
    assert_eq!(intent(CameraCommand::DeleteCalibration, &CameraInputs { calibration: 5, ..base.clone() }), None);
    assert_eq!(intent(CameraCommand::StartLens, &CameraInputs { running: 3, ..base }), None, "no such camera row");
}

#[test]
fn using_a_camera_starts_the_largest_bgra_mode_with_the_typed_lens_profile() {
    let mut r = rig();
    let primary = r.h.controller().camera_view();
    let ws = view(&mut r);
    let inputs = CameraInputs { lens_profile: "  groothoek ".into(), ..CameraInputs::default() };
    assert_eq!(
        camera_command_intent(CameraCommand::UseCamera, Some(&ws), primary.as_ref(), &inputs),
        Some(UiIntent::CameraWorkspace(I::UseCamera { device: cam(), mode: mode(1280), lens: "groothoek".into() })),
        "the Yuy2 mode of 1920 is not offered: only BGRA frames are decoded"
    );
}

#[test]
fn the_simple_controls_send_the_values_as_typed() {
    assert_eq!(overlay_visible_intent(false), UiIntent::CameraWorkspace(I::SetOverlayVisible(false)));
    assert_eq!(opacity_intent(37.0), UiIntent::CameraWorkspace(I::SetOverlayOpacity { percent: 37.0 }));
    assert_eq!(height_intent("12,5"), UiIntent::CameraWorkspace(I::SetMaterialHeight { mm: "12,5".into() }));
    assert_eq!(margin_intent("8"), UiIntent::CameraWorkspace(I::SetEdgeMargin { px: "8".into() }));
}

// --- clicking in the photo ----------------------------------------------------------------------------

/// A 200 × 100 px photo in a 400 × 200 pt view fills it at scale 2: view (100, 150), measured from the
/// bottom, is pixel (50, 25) measured from the top. In a 400 × 400 pt view the photo is centred with
/// 100 pt above and below; clicks in those bars, and outside the view, are no clicks on the photo.
#[test]
fn a_click_in_the_photo_is_a_pixel_and_a_click_beside_it_is_not() {
    assert_eq!(photo_click_to_pixel((400.0, 200.0), (200, 100), (100.0, 150.0)), Some([50.0, 25.0]));
    assert_eq!(photo_click_to_pixel((400.0, 200.0), (200, 100), (0.0, 200.0)), Some([0.0, 0.0]), "top-left corner");
    assert_eq!(photo_click_to_pixel((400.0, 400.0), (200, 100), (200.0, 300.0)), Some([100.0, 0.0]));
    assert_eq!(photo_click_to_pixel((400.0, 400.0), (200, 100), (200.0, 50.0)), None, "the bar below the photo");
    assert_eq!(photo_click_to_pixel((400.0, 400.0), (200, 100), (200.0, 350.0)), None, "the bar above it");
    assert_eq!(photo_click_to_pixel((400.0, 200.0), (200, 100), (401.0, 100.0)), None);
    assert_eq!(photo_click_to_pixel((0.0, 200.0), (200, 100), (0.0, 0.0)), None, "no view");
    assert_eq!(photo_click_to_pixel((400.0, 200.0), (0, 0), (10.0, 10.0)), None, "no photo");
    assert_eq!(photo_click_to_pixel((400.0, 200.0), (200, 100), (f64::NAN, 10.0)), None);
}

#[test]
fn a_click_without_a_photo_or_a_mark_becomes_no_intent() {
    let mut r = rig();
    start_camera(&mut r, 640);
    go(&mut r, I::StartAlignment { device: cam(), material_height_mm: "0".into() });
    let ws = view(&mut r);
    assert_eq!(photo_click_intent(Some(&ws), 0, (400.0, 200.0), (100.0, 100.0)), None, "no photo yet");

    go(&mut r, I::CaptureAlignmentPhoto);
    let ws = view(&mut r);
    let size = (640.0, 320.0);
    assert_eq!(
        photo_click_intent(Some(&ws), 2, size, (320.0, 160.0)),
        Some(UiIntent::CameraWorkspace(I::PlaceAlignmentMark { index: 2, pixel: [320.0, 160.0] }))
    );
    assert_eq!(photo_click_intent(Some(&ws), -1, size, (320.0, 160.0)), None, "no mark chosen");
    assert_eq!(photo_click_intent(None, 0, size, (320.0, 160.0)), None);
}

#[test]
fn clicked_marks_are_drawn_on_the_photo_without_touching_the_original() {
    let (w, h) = (20u32, 10u32);
    let grey = vec![50u8; (w * h * 4) as usize];
    let drawn = photo_with_marks(&grey, w, h, &[([10.0, 5.0], MarkRole::Fit), ([2.0, 2.0], MarkRole::Check)]);
    let px = |img: &[u8], x: u32, y: u32| {
        let i = ((y * w + x) * 4) as usize;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    };
    assert_eq!(px(&drawn, 10, 5), [255, 0, 0, 255], "a fit mark is red at its centre");
    assert_eq!(px(&drawn, 13, 5), [255, 0, 0, 255], "and the arm of its cross");
    assert_eq!(px(&drawn, 2, 2), [0, 80, 255, 255], "a check mark is blue");
    assert_eq!(px(&drawn, 7, 8), [50, 50, 50, 50], "elsewhere untouched");
    assert_eq!(px(&grey, 10, 5), [50, 50, 50, 50], "the original is unchanged");
    assert_eq!(
        photo_with_marks(&grey[..10], w, h, &[([1.0, 1.0], MarkRole::Fit)]).len(),
        10,
        "inconsistent input is passed on"
    );
}

#[test]
fn the_click_through_flow_goes_to_the_next_mark_without_a_position() {
    let mut r = rig();
    start_camera(&mut r, 640);
    go(&mut r, I::StartAlignment { device: cam(), material_height_mm: "0".into() });
    go(&mut r, I::CaptureAlignmentPhoto);
    let a = view(&mut r).alignment.unwrap();
    assert_eq!(next_unplaced_mark(&a, 0), Some(1));
    for i in 0..a.marks.len() {
        go(&mut r, I::PlaceAlignmentMark { index: i, pixel: [10.0 + i as f64, 10.0] });
    }
    let a = view(&mut r).alignment.unwrap();
    assert_eq!(next_unplaced_mark(&a, 0), None, "all marks are clicked");
    go(&mut r, I::ClearAlignmentMark { index: 3 });
    let a = view(&mut r).alignment.unwrap();
    assert_eq!(next_unplaced_mark(&a, 5), Some(3), "wraps around");
}

#[test]
fn the_window_describes_the_alignment_and_the_marks_it_waits_for() {
    let mut r = rig();
    start_camera(&mut r, 640);
    go(&mut r, I::StartAlignment { device: cam(), material_height_mm: "0".into() });
    let p = panel(&mut r, Locale::En);
    assert_eq!(p.marks.len(), 6);
    assert!(p.marks[0].contains("fit") && p.marks[0].contains("not clicked yet"), "{}", p.marks[0]);
    assert!(p.marks[4].contains("check"), "{}", p.marks[4]);
    assert!(p.alignment_status.contains("No photo yet"));
    assert!(p.is_enabled(CameraCommand::TakePhoto) && !p.is_enabled(CameraCommand::FitAlignment));
    assert!(p.is_enabled(CameraCommand::CancelAlignment) && !p.is_enabled(CameraCommand::StartAlignment));
    let _ = config_key as fn(&CameraDeviceId, &CameraMode, &str) -> CameraConfigKey;
}
