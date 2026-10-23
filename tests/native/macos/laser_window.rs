//! The laser window in the real AppKit app (W2-L1-laser, M05–M07), against a fake controller:
//! Laser ▸ Laservenster opens the panel; the position reads "onbekend" (never 0); a control without
//! wire evidence is off with the refusal in words as its tooltip; Home sends exactly one request;
//! the arrow keys jog on key-down and release on key-up, key repeat is not a new press; losing the
//! keyboard and hiding the panel release a held key.
//!
//! No real laser: the machine service sits on a port that acknowledges everything in-process.
//!
//! Manual (not covered here): pressing and holding a jog button with the mouse (AppKit's tracking
//! loop), how the panel looks, and the diagnostic export's save sheet (set
//! `BURNBURN_NATIVE_SCREENSHOT` for a window capture).

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("laser_window: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("laser_window_shows_the_truth_and_jogs_only_while_a_key_is_held", scenario())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn scenario() -> Result<(), String> {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
    use burnburn_machine::{
        Capabilities, Capability, ControllerPort, Envelope, ExecutionActor, ExecutionId, Incoming, LiveRequest,
        MachineId, MachineIdentity, Policy, PortError, Reply, Request,
    };
    use burnburn_macos::adapter::MenuItem;
    use burnburn_presentation::{
        HistoryStore, HistoryStoreError, IntentRecord, LiveIntentRecord, MachineService, OutcomeRecord, Recorded,
        UiIntent, UploadRecord,
    };
    use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};
    use burnburn_types::{Clock, MonotonicTime};
    use common::expect;
    use objc2::Message;
    use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};
    use objc2_foundation::{NSPoint, NSString};

    /// Acknowledges every request at once, records what was sent.
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
            Err(HistoryStoreError("unused".into()))
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

    let s = common::start()?;
    let doc = s.new_document()?;
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
    let live = |sent: &Arc<Mutex<Vec<Request>>>| -> Vec<LiveRequest> {
        sent.lock()
            .map(|v| {
                v.iter()
                    .filter_map(|r| match r {
                        Request::Live(l) => Some(l.clone()),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let settle = || {
        for _ in 0..4 {
            let _ = service.poll();
        }
        s.pump(0.05);
    };

    // The menu item is there, enabled with a window, and opens the panel.
    let (enabled, title) = s.menu_item_state(MenuItem::LaserWindow).ok_or("no Laser menu")?;
    expect(|| format!("menu: enabled {enabled} title {title:?}"), enabled && title == "Laservenster tonen")?;
    s.menu(MenuItem::LaserWindow)?;
    s.pump(0.2);
    expect(|| "the laser panel is shown".into(), s.app.laser_panel_shown(doc))?;
    let visible = s.app.with_laser(doc, |p| p.panel.isVisible()).unwrap_or(false);
    expect(|| "the panel window is visible".into(), visible)?;
    // LFB-rest: the window is bounded by the visible part of its screen, not by a fixed height.
    if let Some((frame, Some(screen))) = s.app.with_laser(doc, |p| (p.panel.frame(), p.panel.screen())) {
        let v = screen.visibleFrame();
        expect(
            || format!("panel frame {frame:?} outside the visible screen {v:?}"),
            frame.origin.y >= v.origin.y - 0.5
                && frame.origin.y + frame.size.height <= v.origin.y + v.size.height + 0.5
                && frame.origin.x >= v.origin.x - 0.5
                && frame.origin.x + frame.size.width <= v.origin.x + v.size.width + 0.5,
        )?;
    }

    // Nothing is proven on this controller: position unknown (not 0), Home off with the reason.
    let position = s.app.with_laser(doc, |p| p.position.stringValue().to_string()).unwrap_or_default();
    expect(|| format!("position label {position:?}"), position == "X: onbekend   Y: onbekend")?;
    let (home_on, tip) = s
        .app
        .with_laser(doc, |p| {
            (p.simple[0].isEnabled(), p.simple[0].toolTip().map(|t| t.to_string()).unwrap_or_default())
        })
        .ok_or("no panel")?;
    expect(|| format!("Home enabled {home_on}, tooltip {tip:?}"), !home_on && tip.starts_with("Niet geverifieerd"))?;
    let status = s.app.with_laser(doc, |p| p.status.stringValue().to_string()).unwrap_or_default();
    expect(|| format!("status {status:?}"), status.is_empty())?;

    // Evidence arrives: the controls switch on; Home sends one request.
    let mut caps = Capabilities::default();
    caps.live.home = Capability::Supported;
    caps.live.jog = Capability::Supported;
    service.set_capabilities(caps);
    s.app.dispatch(UiIntent::LaserSetup { setup: burnburn_presentation::LaserSetup::StepMm("2".into()) });
    let home_on = s.app.with_laser(doc, |p| p.simple[0].isEnabled()).unwrap_or(false);
    expect(|| "Home is on once the capability is proven".into(), home_on)?;
    s.app.with_laser(doc, |p| {
        // SAFETY: a click on our own button, as a user would; no sender.
        unsafe { p.simple[0].performClick(None) }
    });
    settle();
    expect(
        || format!("one home request, got {:?}", live(&sent)),
        live(&sent) == vec![LiveRequest::Home(burnburn_machine::HomeTarget::Xy)],
    )?;

    // The red dot (W09): without a measured offset the switch is off and the note says so; once a
    // pointer proof is recorded the switch is on and names the offset, and a click switches it off.
    let (switch_on, note) = s
        .app
        .with_laser(doc, |p| (p.pointer_check.isEnabled(), p.pointer_note.stringValue().to_string()))
        .ok_or("no panel")?;
    expect(
        || format!("without proof: switch {switch_on}, note {note:?}"),
        !switch_on && note.contains("Geen pointeroffset"),
    )?;
    s.app.dispatch(UiIntent::RecordCalibration {
        input: burnburn_presentation::ProofInput {
            kind: burnburn_presentation::ProofKind::PointerOffset,
            material: String::new(),
            speed: String::new(),
            value: "2".into(),
            value_y: "-1".into(),
            method: "kruis gebrand".into(),
            date: "2026-10-22".into(),
        },
    });
    s.pump(0.1);
    let (switch_on, state, note) = s
        .app
        .with_laser(doc, |p| {
            (p.pointer_check.isEnabled(), p.pointer_check.state(), p.pointer_note.stringValue().to_string())
        })
        .ok_or("no panel")?;
    expect(
        || format!("with proof: switch {switch_on}, state {state}, note {note:?}"),
        switch_on && state == objc2_app_kit::NSControlStateValueOn && note == "Rode stip: (2; -1) mm",
    )?;
    s.app.with_laser(doc, |p| {
        // SAFETY: a click on our own checkbox, as a user would; no sender.
        unsafe { p.pointer_check.performClick(None) }
    });
    s.pump(0.1);
    let state = s.app.with_laser(doc, |p| p.pointer_check.state()).ok_or("no panel")?;
    expect(|| format!("switched off, state {state}"), state == objc2_app_kit::NSControlStateValueOff)?;

    // The arrow keys jog while the panel has the keyboard: key-down presses once, key repeat is not
    // a new press, key-up releases.
    let panel = s.app.with_laser(doc, |p| p.panel.retain()).ok_or("no panel")?;
    s.become_key(&panel);
    let key = |kind: NSEventType, repeat: bool| -> Result<(), String> {
        let chars = NSString::from_str("\u{f702}");
        let event = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            kind,
            NSPoint::new(0.0, 0.0),
            NSEventModifierFlags::empty(),
            0.0,
            panel.windowNumber(),
            None,
            &chars,
            &chars,
            repeat,
            123,
        )
        .ok_or("no key event")?;
        panel.sendEvent(&event);
        settle();
        Ok(())
    };
    key(NSEventType::KeyDown, false)?;
    key(NSEventType::KeyDown, true)?;
    key(NSEventType::KeyDown, true)?;
    let jogs = live(&sent);
    expect(
        || format!("one press for a held key, got {jogs:?}"),
        jogs.iter().filter(|r| matches!(r, LiveRequest::JogStart(_))).count() == 1,
    )?;
    key(NSEventType::KeyUp, false)?;
    let jogs = live(&sent);
    expect(
        || format!("the release followed, got {jogs:?}"),
        jogs.last() == Some(&LiveRequest::JogStop(burnburn_machine::JogKey::XLeft)),
    )?;

    // Losing the keyboard releases a held key.
    key(NSEventType::KeyDown, false)?;
    panel.resignKeyWindow();
    settle();
    let jogs = live(&sent);
    expect(
        || format!("resigning key released the key, got {jogs:?}"),
        jogs.last() == Some(&LiveRequest::JogStop(burnburn_machine::JogKey::XLeft)) && jogs.len() == 5,
    )?;

    // Hiding the panel (the menu toggles) leaves nothing held.
    s.menu(MenuItem::LaserWindow)?;
    s.pump(0.1);
    expect(|| "the panel is hidden".into(), !s.app.laser_panel_shown(doc))?;
    common::screenshot(&panel);
    Ok(())
}
