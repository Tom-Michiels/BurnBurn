//! The camera window in the real AppKit app (W3-UI-camera), against a fake camera port and an
//! in-process fake controller: the strip's "Camerabeheer…" button opens it without asking for
//! camera access; "Camera gebruiken" starts the camera; an uncalibrated camera says so; the
//! alignment wizard takes a photo in which the marks are clicked with real mouse events; the fit
//! and the saved alignment place the picture on the bed (a texture in the canvas scene, with the
//! slider's opacity); the marks job is proposed without sending and goes to the machine only on its
//! own button; a camera restarted at another resolution shows its calibration as ONGELDIG.
//!
//! No AVFoundation, no privacy prompt, no real laser.
//!
//! Manual (not covered here): the real camera and its permission prompt, how the picture and the
//! window look (set `BURNBURN_NATIVE_SCREENSHOT` for a window capture), burning the marks.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("camera_window: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("camera_window_aligns_places_the_picture_and_shows_invalid_calibrations", scenario())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn scenario() -> Result<(), String> {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use burnburn_editor::Overlay;
    use burnburn_image::SourceImage;
    use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
    use burnburn_machine::{
        ControllerPort, Envelope, ExecutionActor, ExecutionId, Incoming, MachineId, MachineIdentity, Policy, PortError,
        Reply, Request,
    };
    use burnburn_macos::panels::CameraCommand;
    use burnburn_presentation::{
        CameraAuthorization, CameraDevice, CameraDeviceId, CameraError, CameraFrame, CameraMode, CameraPixelFormat,
        CameraPort, HistoryStore, HistoryStoreError, IntentRecord, LiveIntentRecord, MachineService, OutcomeRecord,
        Recorded, UiIntent, UploadRecord,
    };
    use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};
    use burnburn_types::{Clock, MonotonicTime};
    use common::expect;
    use objc2::Message;
    use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};
    use objc2_foundation::NSPoint;

    // --- fakes ----------------------------------------------------------------------------------

    struct Cam {
        log: Mutex<Vec<String>>,
        modes: Mutex<Vec<CameraMode>>,
    }
    let mode =
        |w: u32| CameraMode { width: w, height: w * 9 / 16, fps_milli: 30_000, pixel_format: CameraPixelFormat::Bgra8 };
    impl CameraPort for Cam {
        fn devices(&self) -> Result<Vec<CameraDevice>, CameraError> {
            Ok(vec![CameraDevice {
                id: CameraDeviceId("cam-A".into()),
                name: "Bedcamera".into(),
                modes: self.modes.lock().map(|m| m.clone()).unwrap_or_default(),
            }])
        }
        fn authorization(&self) -> CameraAuthorization {
            CameraAuthorization::Authorized
        }
        fn request_authorization(&self) {
            self.log.lock().map(|mut l| l.push("request".into())).ok();
        }
        fn start(&self, d: &CameraDeviceId, m: &CameraMode) -> Result<(), CameraError> {
            self.log.lock().map(|mut l| l.push(format!("start {} {}", d.0, m.width))).ok();
            Ok(())
        }
        fn stop(&self, d: &CameraDeviceId) {
            self.log.lock().map(|mut l| l.push(format!("stop {}", d.0))).ok();
        }
        fn latest_frame(&self, d: &CameraDeviceId) -> Option<Arc<CameraFrame>> {
            let started = self.log.lock().ok()?.iter().rev().find(|l| l.starts_with("start"))?.clone();
            let width: u32 = started.rsplit(' ').next()?.parse().ok()?;
            let m =
                CameraMode { width, height: width * 9 / 16, fps_milli: 30_000, pixel_format: CameraPixelFormat::Bgra8 };
            let mut rgba = vec![90u8; (m.width * m.height * 4) as usize];
            for px in rgba.as_chunks_mut::<4>().0 {
                px[3] = 255;
            }
            Some(Arc::new(CameraFrame {
                device: d.clone(),
                mode: m,
                seq: 1,
                image: SourceImage::from_rgba8(m.width, m.height, rgba).ok()?,
                time: MonotonicTime::ZERO,
            }))
        }
    }
    let camera = Arc::new(Cam { log: Mutex::new(Vec::new()), modes: Mutex::new(vec![mode(640)]) });

    struct Port {
        sent: Arc<Mutex<Vec<Request>>>,
        replies: VecDeque<Incoming>,
        machine: MachineId,
    }
    impl ControllerPort for Port {
        fn send(&mut self, e: Envelope) -> Result<(), PortError> {
            self.sent.lock().map_err(|_| PortError::Disconnected)?.push(e.request);
            self.replies.push_back(Incoming { from: self.machine.clone(), tag: e.tag, reply: Reply::Ack });
            Ok(())
        }
        fn try_recv(&mut self) -> Result<Option<Incoming>, PortError> {
            Ok(self.replies.pop_front())
        }
        fn reconnect(&mut self) -> Result<(), PortError> {
            Ok(())
        }
    }
    struct Zero;
    impl Clock for Zero {
        fn now(&self) -> MonotonicTime {
            MonotonicTime::ZERO
        }
    }
    struct Journal;
    impl HistoryStore for Journal {
        fn record_upload(&self, _: &UploadRecord) -> Result<(ExecutionId, Recorded), HistoryStoreError> {
            Ok((ExecutionId::from_raw(1), Recorded::Fresh))
        }
        fn record_intent(&self, _: &IntentRecord) -> Result<Recorded, HistoryStoreError> {
            Ok(Recorded::Fresh)
        }
        fn record_outcome(&self, _: &OutcomeRecord) -> Result<Recorded, HistoryStoreError> {
            Ok(Recorded::Fresh)
        }
        fn record_live_intent(&self, _: &LiveIntentRecord) -> Result<Recorded, HistoryStoreError> {
            Ok(Recorded::Fresh)
        }
    }

    // --- the app --------------------------------------------------------------------------------

    let s = common::start()?;
    let doc = s.new_document()?;
    s.app.attach_camera(camera.clone());
    let sent = Arc::new(Mutex::new(Vec::new()));
    let machine = MachineId::new("bench");
    let identity = MachineIdentity { id: machine.clone(), model: Some("RDC6445G".into()), firmware: None };
    let port = Port { sent: sent.clone(), replies: VecDeque::new(), machine };
    let service = Arc::new(
        MachineService::new(
            ExecutionActor::new(identity, port, Arc::new(Zero), Policy::default()),
            Arc::new(Journal),
            64,
        )
        .map_err(|e| e.0)?,
    );
    let profile = MachineProfile::new(
        "bench-1500x900",
        1,
        MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).map_err(|e| format!("{e:?}"))?,
        1000.0,
        400.0,
        PowerRatio::new(0.0).map_err(|e| format!("{e:?}"))?,
        PowerRatio::new(1.0).map_err(|e| format!("{e:?}"))?,
    )
    .map_err(|e| format!("{e:?}"))?;
    let ruida = RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged };
    s.app.dispatch(UiIntent::SetMachineProfile { profile: Box::new(profile), ruida });
    s.app.attach_machine(service.clone());
    s.pump(0.2);

    let click = |command: CameraCommand| -> Result<(), String> {
        let ok = s
            .app
            .with_camera_window(|p| {
                let b = p.commands.iter().find(|(c, _)| *c == command).map(|(_, b)| b.clone());
                b.map(|b| {
                    // A disabled button does nothing when clicked; whether it was on is the answer.
                    let was_enabled = b.isEnabled();
                    // SAFETY: a click on our own button, as a user would; no sender.
                    unsafe { b.performClick(None) };
                    was_enabled
                })
            })
            .flatten();
        s.pump(0.15);
        if ok == Some(true) { Ok(()) } else { Err(format!("{command:?} was not an enabled button ({ok:?})")) }
    };
    let enabled = |command: CameraCommand| -> bool {
        s.app
            .with_camera_window(|p| p.commands.iter().find(|(c, _)| *c == command).is_some_and(|(_, b)| b.isEnabled()))
            .unwrap_or(false)
    };
    let text = |pick: fn(&burnburn_macos::appkit::CameraParts<'_>) -> String| -> String {
        s.app.with_camera_window(pick).unwrap_or_default()
    };
    let scene_images = || -> Vec<f32> {
        s.app
            .scene(doc)
            .map(|scene| {
                scene
                    .overlays
                    .iter()
                    .filter_map(|o| match o {
                        Overlay::BedImage { opacity, .. } => Some(*opacity),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    };

    // --- open the window: nothing is asked or started ---------------------------------------------

    expect(|| "the window is closed before the button is used".into(), !s.app.camera_window_shown())?;
    let button = s.app.with_window(doc, |w| w.camera_window_button.retain()).ok_or("no camera window button")?;
    // SAFETY: a click on our own button, as a user would; no sender.
    unsafe { button.performClick(None) };
    s.pump(0.3);
    expect(|| "the camera window is shown".into(), s.app.camera_window_shown())?;
    expect(
        || format!("opening asked or started something: {:?}", camera.log.lock().map(|l| l.clone())),
        camera.log.lock().map(|l| l.is_empty()).unwrap_or(false),
    )?;
    let devices =
        s.app.with_camera_window(|p| p.devices.itemTitles().iter().map(|t| t.to_string()).collect::<Vec<_>>());
    expect(|| format!("devices {devices:?}"), devices.as_deref() == Some(&["Bedcamera".to_owned()][..]))?;
    expect(|| "no camera in use: update is off".into(), !enabled(CameraCommand::UpdateOverlay))?;

    // --- use the camera ---------------------------------------------------------------------------

    click(CameraCommand::UseCamera)?;
    let log = camera.log.lock().map(|l| l.clone()).unwrap_or_default();
    expect(|| format!("camera log {log:?}"), log == ["start cam-A 640"])?;
    let running =
        s.app.with_camera_window(|p| p.running.itemTitles().iter().map(|t| t.to_string()).collect::<Vec<_>>());
    expect(
        || format!("running {running:?}"),
        running.as_ref().is_some_and(|r| r.len() == 1 && r[0].contains("Bedcamera") && r[0].contains("640×360")),
    )?;

    // --- an uncalibrated camera says so, and the picture is not guessed onto the bed ----------------

    click(CameraCommand::UpdateOverlay)?;
    let status = text(|p| p.overlay_status.stringValue().to_string());
    expect(|| format!("overlay status {status:?}"), status.contains("nog niet uitgelijnd"))?;
    expect(|| "no picture on the bed yet".into(), scene_images().is_empty())?;

    // --- the alignment wizard: photo, then the marks are clicked in it ---------------------------------

    click(CameraCommand::StartAlignment)?;
    let marks = s.app.with_camera_window(|p| p.marks.numberOfItems()).unwrap_or(0);
    expect(|| format!("{marks} marks"), marks == 6)?;
    click(CameraCommand::TakePhoto)?;
    let alignment = text(|p| p.alignment_status.stringValue().to_string());
    expect(|| format!("alignment status {alignment:?}"), alignment.contains("Foto 640 × 360 px"))?;
    // The camera looks straight down at 2.5 mm per pixel: bed (x, y) is pixel (x / 2.5, (900 − y) / 2.5).
    // These are the default mark places on the 1500 × 900 bed.
    let places: [(f64, f64); 6] =
        [(225.0, 135.0), (1275.0, 135.0), (1275.0, 765.0), (225.0, 765.0), (750.0, 450.0), (525.0, 585.0)];
    let panel = s.app.with_camera_window(|p| p.panel.retain()).ok_or("no panel")?;
    let (view_w, view_h, image_w, image_h) = s
        .app
        .with_camera_window(|p| {
            let b = p.photo.bounds().size;
            (b.width, b.height, 640.0, 360.0)
        })
        .ok_or("no photo view")?;
    let scale = (view_w / image_w).min(view_h / image_h);
    let (ox, oy) = ((view_w - image_w * scale) / 2.0, (view_h - image_h * scale) / 2.0);
    for (i, (x, y)) in places.iter().enumerate() {
        let (px, py) = (x / 2.5, (900.0 - y) / 2.5);
        // Pixel to view point: the inverse of what the host does with a click (origin bottom-left).
        let at = NSPoint::new(ox + px * scale, oy + (image_h - py) * scale);
        let in_window = s.app.with_camera_window(|p| p.photo.convertPoint_toView(at, None)).ok_or("no photo view")?;
        let event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            NSEventType::LeftMouseDown,
            in_window,
            NSEventModifierFlags::empty(),
            0.0,
            panel.windowNumber(),
            None,
            0,
            1,
            1.0,
        )
        .ok_or("no mouse event")?;
        panel.sendEvent(&event);
        s.pump(0.1);
        let selected = s.app.with_camera_window(|p| p.marks.indexOfSelectedItem()).unwrap_or(-1);
        let expected_next = if i + 1 < places.len() { (i + 1) as isize } else { selected };
        expect(
            || format!("after click {i} the next mark is chosen ({selected} vs {expected_next})"),
            selected == expected_next,
        )?;
    }
    let marks_text =
        s.app.with_camera_window(|p| p.marks.itemTitles().iter().map(|t| t.to_string()).collect::<Vec<_>>());
    expect(
        || format!("marks {marks_text:?}"),
        marks_text.as_ref().is_some_and(|m| m.iter().all(|t| t.contains("px") && !t.contains("nog niet aangeklikt"))),
    )?;
    expect(|| "fit is on once the marks are clicked".into(), enabled(CameraCommand::FitAlignment))?;
    click(CameraCommand::FitAlignment)?;
    let alignment = text(|p| p.alignment_status.stringValue().to_string());
    expect(|| format!("alignment status {alignment:?}"), alignment.contains("Pasfout 0.00 mm"))?;
    click(CameraCommand::SaveAlignment)?;
    let calibrations =
        s.app.with_camera_window(|p| p.calibrations.itemTitles().iter().map(|t| t.to_string()).collect::<Vec<_>>());
    expect(
        || format!("calibrations {calibrations:?}"),
        calibrations.as_ref().is_some_and(|c| {
            c.len() == 1 && c[0].contains("uitlijning") && c[0].contains("geldig") && !c[0].contains("ONGELDIG")
        }),
    )?;

    // --- the picture is placed on the bed, with the slider's opacity -------------------------------------

    click(CameraCommand::UpdateOverlay)?;
    let status = text(|p| p.overlay_status.stringValue().to_string());
    expect(|| format!("overlay status {status:?}"), status.contains("geplaatst"))?;
    expect(|| format!("scene images {:?}", scene_images()), scene_images() == vec![0.5])?;
    s.app.with_camera_window(|p| {
        p.opacity.setDoubleValue(25.0);
        // SAFETY: the slider's own action, as dragging it would send it.
        unsafe { p.opacity.performClick(None) };
    });
    s.pump(0.1);
    expect(|| format!("scene images {:?}", scene_images()), scene_images() == vec![0.25])?;
    s.app.with_camera_window(|p| {
        // SAFETY: a click on our own checkbox, as a user would.
        unsafe { p.visible.performClick(None) };
    });
    s.pump(0.1);
    expect(|| format!("switched off: scene images {:?}", scene_images()), scene_images().is_empty())?;
    s.app.with_camera_window(|p| {
        // SAFETY: as above.
        unsafe { p.visible.performClick(None) };
    });
    s.pump(0.1);
    expect(|| "switched on again".into(), scene_images() == vec![0.25])?;

    // --- the marks job: proposed without sending, sent only by its own button --------------------------------

    expect(|| "nothing proposed: nothing to send".into(), !enabled(CameraCommand::SendMarks))?;
    s.app.dispatch(UiIntent::CameraWorkspace(burnburn_presentation::CameraWorkspaceIntent::StartAlignment {
        device: CameraDeviceId("cam-A".into()),
        material_height_mm: "0".into(),
    }));
    s.pump(0.1);
    click(CameraCommand::ProposeMarks)?;
    let status = text(|p| p.marks_status.stringValue().to_string());
    expect(|| format!("marks status {status:?}"), status.contains("Voorstel") && !status.contains("Verstuurd"))?;
    expect(|| "proposing sent nothing".into(), sent.lock().map(|v| v.is_empty()).unwrap_or(false))?;
    click(CameraCommand::SendMarks)?;
    for _ in 0..4 {
        let _ = service.poll();
    }
    let requests = sent.lock().map(|v| v.clone()).unwrap_or_default();
    expect(|| format!("requests {requests:?}"), matches!(requests.first(), Some(Request::QueryIdentity)))?;
    let status = text(|p| p.marks_status.stringValue().to_string());
    expect(|| format!("marks status {status:?}"), status.contains("Verstuurd als upload"))?;
    click(CameraCommand::CancelAlignment)?;

    // --- another resolution: the calibration is ONGELDIG -------------------------------------------------------

    if let Ok(mut modes) = camera.modes.lock() {
        modes.push(mode(1280));
    }
    click(CameraCommand::UseCamera)?;
    let calibrations =
        s.app.with_camera_window(|p| p.calibrations.itemTitles().iter().map(|t| t.to_string()).collect::<Vec<_>>());
    expect(
        || format!("calibrations {calibrations:?}"),
        calibrations.as_ref().is_some_and(|c| c.len() == 1 && c[0].contains("ONGELDIG: resolutie gewijzigd")),
    )?;
    click(CameraCommand::UpdateOverlay)?;
    let status = text(|p| p.overlay_status.stringValue().to_string());
    expect(|| format!("overlay status {status:?}"), status.contains("geldt niet meer"))?;
    expect(|| "the stale picture of the old resolution is replaced by nothing".into(), scene_images().is_empty())?;
    let requested = camera.log.lock().map(|l| l.iter().any(|e| e == "request")).unwrap_or(true);
    expect(|| "the privacy prompt was never asked for".into(), !requested)?;
    common::screenshot(&panel);

    // --- closing it again ---------------------------------------------------------------------------------------
    // SAFETY: a click on our own button, as a user would; no sender.
    unsafe { button.performClick(None) };
    s.pump(0.1);
    expect(|| "the window is hidden again".into(), !s.app.camera_window_shown())?;
    Ok(())
}
