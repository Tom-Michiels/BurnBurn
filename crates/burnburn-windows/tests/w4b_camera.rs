//! W4-L8b family 4, headless: the Windows camera window says what the presentation's camera
//! workspace says (an invalid calibration is badged with its reason, a refusal is in words, a
//! control that cannot act is off and sends nothing) and its controls become the `UiIntent` of the
//! view model they were offered in. The camera is a fake `CameraPort`; the real Windows camera
//! source (Media Foundation) does not exist yet and is reported as unavailable, never faked.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use burnburn_image::SourceImage;
use burnburn_image::calib::{AlignmentLens, LensModel, MarkObservation, MarkRole, WorkspaceAlignment};
use burnburn_presentation::{
    BedCameraIntent, CameraAuthorization, CameraDevice, CameraDeviceId, CameraError, CameraFrame, CameraMode,
    CameraPixelFormat, CameraPort, CameraWorkspaceIntent as I, CameraWorkspaceView, HeadlessEditorHarness, UiIntent,
    config_key,
};
use burnburn_types::MonotonicTime;
use burnburn_windows::host_panels::camera::{CameraInput, camera_spec, id, next_mark_after, on_camera_event};
use burnburn_windows::host_panels::camera_port::UnavailableCameraPort;
use burnburn_windows::host_panels::{ControlKind, PanelReaction, PanelSpec, PanelValues, notify};
use burnburn_windows::l10n::Locale;
use common::{click, event, texts};

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
    fn latest_frame(&self, _: &CameraDeviceId) -> Option<Arc<CameraFrame>> {
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

fn go(r: &mut Rig, intent: I) {
    r.h.send(UiIntent::CameraWorkspace(intent));
    r.h.controller().poll_camera_work();
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

struct Snap {
    workspace: Option<CameraWorkspaceView>,
    primary: Option<burnburn_presentation::CameraViewModel>,
}

fn snap(r: &mut Rig) -> Snap {
    Snap { workspace: r.h.controller().camera_workspace_view(), primary: r.h.controller().camera_view() }
}

impl Snap {
    fn input(&self, locale: Locale) -> CameraInput<'_> {
        CameraInput {
            locale,
            workspace: self.workspace.as_ref(),
            primary: self.primary.as_ref(),
            photo: None,
            mark: None,
        }
    }
    fn spec(&self, locale: Locale) -> PanelSpec {
        camera_spec(&self.input(locale))
    }
    fn react(&self, ev: burnburn_windows::host_panels::PanelEvent) -> PanelReaction {
        on_camera_event(&self.input(Locale::En), &ev)
    }
}

fn text(spec: &PanelSpec, cid: u16) -> String {
    spec.control(cid).unwrap_or_else(|| panic!("control {cid} missing")).text.clone()
}

fn on(spec: &PanelSpec, cid: u16) -> bool {
    spec.control(cid).unwrap().enabled
}

fn list_items(spec: &PanelSpec, cid: u16) -> Vec<String> {
    match &spec.control(cid).unwrap().kind {
        ControlKind::Combo { items, .. } | ControlKind::List { items, .. } => items.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_unavailable_port_says_so_and_cannot_start_anything() {
    let port = UnavailableCameraPort;
    assert_eq!(port.devices().unwrap(), Vec::new());
    assert_eq!(port.authorization(), CameraAuthorization::Restricted, "no source is built: access is not offered");
    assert!(port.start(&cam(), &mode(640)).is_err(), "a camera that does not exist cannot start");
    assert!(port.latest_frame(&cam()).is_none());
    // The window over it shows the permission state in words and offers no start.
    let mut h = HeadlessEditorHarness::new();
    h.controller().attach_camera(Arc::new(UnavailableCameraPort));
    let primary = h.controller().camera_view();
    let s = camera_spec(&CameraInput {
        locale: Locale::Nl,
        workspace: None,
        primary: primary.as_ref(),
        photo: None,
        mark: None,
    });
    assert!(!on(&s, id::BED_START) && !on(&s, id::BED_STOP) && !on(&s, id::BED_ACCESS));
    assert!(!text(&s, id::BED_STATUS).is_empty());
}

#[test]
fn every_control_id_is_unique_and_inside_the_camera_block() {
    let mut r = rig();
    start_camera(&mut r, 640);
    let s = snap(&mut r).spec(Locale::En);
    let mut seen = HashSet::new();
    for c in s.controls() {
        assert!((7600..7800).contains(&c.id), "{}", c.id);
        assert!(seen.insert(c.id), "duplicate {}", c.id);
    }
    assert!(s.controls().count() > 60);
}

#[test]
fn the_bed_section_starts_the_largest_bgra_mode_and_stop_waits_for_a_running_camera() {
    let mut r = rig();
    let sn = snap(&mut r);
    let s = sn.spec(Locale::Nl);
    assert_eq!(list_items(&s, id::BED_DEVICES), ["Bedcamera"]);
    assert!(on(&s, id::BED_START) && !on(&s, id::BED_STOP) && !on(&s, id::BED_ACCESS));
    let mut values = PanelValues::default();
    values.selection.insert(id::BED_DEVICES, Some(0));
    assert_eq!(
        sn.react(click(id::BED_START, values.clone())),
        PanelReaction::Intent(UiIntent::BedCamera(BedCameraIntent::Start { device: cam(), mode: mode(1280) })),
        "the Yuy2 mode of 1920 is not offered: only BGRA frames are decoded"
    );
    assert_eq!(sn.react(click(id::BED_STOP, values.clone())), PanelReaction::Nothing);
    r.h.send(UiIntent::BedCamera(BedCameraIntent::Start { device: cam(), mode: mode(1280) }));
    let sn = snap(&mut r);
    assert!(on(&sn.spec(Locale::En), id::BED_STOP));
    assert_eq!(
        sn.react(click(id::BED_STOP, values)),
        PanelReaction::Intent(UiIntent::BedCamera(BedCameraIntent::Stop))
    );
}

#[test]
fn the_overlay_controls_send_their_values_as_typed_and_an_unchanged_edit_sends_nothing() {
    let mut r = rig();
    start_camera(&mut r, 640);
    let sn = snap(&mut r);
    let mut v = PanelValues::default();
    v.checks.insert(id::OVERLAY_VISIBLE, false);
    assert_eq!(
        sn.react(click(id::OVERLAY_VISIBLE, v)),
        PanelReaction::Intent(UiIntent::CameraWorkspace(I::SetOverlayVisible(false)))
    );
    let mut slide = PanelValues::default();
    slide.sliders.insert(id::OPACITY, 37);
    assert_eq!(
        sn.react(event(id::OPACITY, notify::SLIDER, slide)),
        PanelReaction::Intent(UiIntent::CameraWorkspace(I::SetOverlayOpacity { percent: 37.0 }))
    );
    assert_eq!(
        sn.react(event(id::HEIGHT, notify::ENTER, texts(&[(id::HEIGHT, "12,5")]))),
        PanelReaction::Intent(UiIntent::CameraWorkspace(I::SetMaterialHeight { mm: "12,5".into() }))
    );
    assert_eq!(
        sn.react(event(id::MARGIN, notify::EDIT_DONE, texts(&[(id::MARGIN, "8")]))),
        PanelReaction::Intent(UiIntent::CameraWorkspace(I::SetEdgeMargin { px: "8".into() }))
    );
    let shown = text(&sn.spec(Locale::En), id::HEIGHT);
    assert_eq!(sn.react(event(id::HEIGHT, notify::EDIT_DONE, texts(&[(id::HEIGHT, &shown)]))), PanelReaction::Nothing);
    assert_eq!(sn.react(event(id::HEIGHT, notify::EDIT_DONE, texts(&[(id::HEIGHT, "  ")]))), PanelReaction::Nothing);
}

#[test]
fn an_invalid_calibration_is_badged_with_its_reason_and_a_refusal_is_in_words() {
    let mut r = rig();
    start_camera(&mut r, 640);
    let valid = text(&snap(&mut r).spec(Locale::Nl), id::CALIBRATIONS);
    let _ = valid;
    let rows = list_items(&snap(&mut r).spec(Locale::Nl), id::CALIBRATIONS);
    assert!(rows[0].contains("geldig") && !rows[0].contains("ONGELDIG"), "{rows:?}");
    start_camera(&mut r, 1280);
    let rows = list_items(&snap(&mut r).spec(Locale::Nl), id::CALIBRATIONS);
    assert!(rows[0].contains("ONGELDIG: resolutie gewijzigd"), "{rows:?}");

    let mut r = rig();
    go(&mut r, I::UpdateOverlay); // no camera running: refused
    let s = snap(&mut r).spec(Locale::Nl);
    assert!(!text(&s, id::REFUSAL).is_empty(), "the refusal is shown");
    assert!(!on(&s, id::UPDATE_OVERLAY), "and the button is off");
}

#[test]
fn buttons_become_the_intents_of_the_state_they_were_offered_in_and_off_buttons_send_nothing() {
    let mut r = rig();
    let off = snap(&mut r);
    for button in [
        id::UPDATE_OVERLAY,
        id::DROP_CAMERA,
        id::START_LENS,
        id::ADD_PHOTO,
        id::FIT_LENS,
        id::START_ALIGN,
        id::TAKE_PHOTO,
        id::SAVE_ALIGN,
    ] {
        assert_eq!(off.react(click(button, PanelValues::default())), PanelReaction::Nothing, "{button}");
    }
    start_camera(&mut r, 640);
    let sn = snap(&mut r);
    let mut values = texts(&[
        (id::COLS, "9"),
        (id::ROWS, "6"),
        (id::PITCH, "25"),
        (id::ALIGN_HEIGHT, "2,5"),
        (id::LENS_PROFILE, "  groothoek "),
    ]);
    values.selection.insert(id::RUNNING, Some(0));
    values.selection.insert(id::DEVICE, Some(0));
    values.selection.insert(id::CALIBRATIONS, Some(0));
    assert_eq!(
        sn.react(click(id::UPDATE_OVERLAY, values.clone())),
        PanelReaction::Intent(UiIntent::CameraWorkspace(I::UpdateOverlay))
    );
    assert_eq!(
        sn.react(click(id::START_LENS, values.clone())),
        PanelReaction::Intent(UiIntent::CameraWorkspace(I::StartLensCalibration {
            device: cam(),
            cols: 9,
            rows: 6,
            pitch_mm: "25".into()
        }))
    );
    let mut junk = values.clone();
    junk.texts.insert(id::COLS, "negen".into());
    assert!(
        matches!(
            sn.react(click(id::START_LENS, junk)),
            PanelReaction::Intent(UiIntent::CameraWorkspace(I::StartLensCalibration { cols: 0, .. }))
        ),
        "not a number: presentation gets 0 and refuses a bad grid"
    );
    assert_eq!(
        sn.react(click(id::START_ALIGN, values.clone())),
        PanelReaction::Intent(UiIntent::CameraWorkspace(I::StartAlignment {
            device: cam(),
            material_height_mm: "2,5".into()
        }))
    );
    assert!(matches!(
        sn.react(click(id::USE_CAMERA, values.clone())),
        PanelReaction::Intent(UiIntent::CameraWorkspace(I::UseCamera { mode, lens, .. })) if mode == self::mode(1280) && lens == "groothoek"
    ));
    let row = sn.workspace.as_ref().unwrap().calibrations[0].reference.clone();
    assert_eq!(
        sn.react(click(id::DELETE_CAL, values.clone())),
        PanelReaction::Intent(UiIntent::CameraWorkspace(I::DeleteCalibration { calibration: row }))
    );
    let mut stale = values;
    stale.selection.insert(id::CALIBRATIONS, Some(5));
    assert_eq!(sn.react(click(id::DELETE_CAL, stale)), PanelReaction::Nothing, "no such row");
}

#[test]
fn a_click_in_the_photo_is_flipped_to_the_photo_pixel_and_goes_to_the_next_unplaced_mark() {
    let mut r = rig();
    start_camera(&mut r, 640);
    go(&mut r, I::StartAlignment { device: cam(), material_height_mm: "0".into() });
    let none = snap(&mut r);
    let mut v = PanelValues::default();
    v.selection.insert(id::MARK, Some(2));
    let mut ev = event(id::PHOTO, notify::BITMAP_CLICK, v.clone());
    ev.click_px = Some([320.0, 160.0]);
    ev.view_px = Some([640.0, 320.0]);
    assert_eq!(none.react(ev.clone()), PanelReaction::Nothing, "no photo yet");

    go(&mut r, I::CaptureAlignmentPhoto);
    let sn = snap(&mut r);
    // Windows measures from the top; the photo model from the bottom: the centre stays the centre,
    // and a click near the top edge of the view is near the top of the photo.
    assert_eq!(
        sn.react(ev),
        PanelReaction::Intent(UiIntent::CameraWorkspace(I::PlaceAlignmentMark { index: 2, pixel: [320.0, 160.0] }))
    );
    let mut top = event(id::PHOTO, notify::BITMAP_CLICK, v.clone());
    top.click_px = Some([100.0, 10.0]);
    top.view_px = Some([640.0, 320.0]);
    assert_eq!(
        sn.react(top),
        PanelReaction::Intent(UiIntent::CameraWorkspace(I::PlaceAlignmentMark { index: 2, pixel: [100.0, 10.0] }))
    );
    let mut bar = event(id::PHOTO, notify::BITMAP_CLICK, v);
    bar.click_px = Some([700.0, 10.0]);
    bar.view_px = Some([640.0, 320.0]);
    assert_eq!(sn.react(bar), PanelReaction::Nothing, "beside the photo");
    assert_eq!(next_mark_after(&sn.input(Locale::En), 0), Some(1));
    assert_eq!(next_mark_after(&sn.input(Locale::En), -1), None);
}

#[test]
fn the_mark_list_shows_the_mark_the_host_chose_and_moves_on_after_a_click() {
    let mut r = rig();
    start_camera(&mut r, 640);
    go(&mut r, I::StartAlignment { device: cam(), material_height_mm: "0".into() });
    let sn = snap(&mut r);
    let selected =
        |mark| match &camera_spec(&CameraInput { mark, ..sn.input(Locale::En) }).control(id::MARK).unwrap().kind {
            ControlKind::Combo { selected, .. } => *selected,
            other => panic!("{other:?}"),
        };
    assert_eq!(selected(None), Some(0), "the first mark by default");
    assert_eq!(selected(Some(3)), Some(3));
    assert_eq!(selected(Some(99)), Some(0), "a mark that does not exist falls back to the first");
}
