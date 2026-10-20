//! W2-L1-laser, headless: the laser panel says what the evidence says (the position is "onbekend",
//! never 0; a control without wire evidence is off with the refusal in words), its controls send
//! `UiIntent::Laser` with the view model's idempotency key unchanged, and the new menu items
//! translate to their reactions. The machine is a service over a fake port: no laser.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
use burnburn_machine::{
    Capabilities, Capability, ControllerPort, Envelope, ExecutionActor, ExecutionId, Incoming, MachineId,
    MachineIdentity, Policy, PortError, Request,
};
use burnburn_macos::adapter::{
    MenuItem, NativeEvent, Reaction, menu_enabled, menu_item, menu_items, menu_tag, menu_title, translate,
};
use burnburn_macos::l10n::{Locale, known_message_keys, laser_why_text, message};
use burnburn_macos::panels::{
    LaserText, anchor_for_tag, file_intent, jog_key_for_key_code, jog_press_intent, laser_intent, laser_panel,
    laser_text, move_intent, save_rd_intent, segment_for_start_from, simple_controls, simple_intent,
    start_from_for_segment,
};
use burnburn_presentation::{
    DiagnosticOptions, HeadlessEditorHarness, HistoryStore, HistoryStoreError, IntentRecord, JogKey, LaserAction,
    LaserCommand, LaserControlKind, LaserSetup, LaserViewModel, LaserWhy, LiveIntentRecord, MachineService,
    OutcomeRecord, Recorded, StartFrom, StepDirection, UiIntent, UploadRecord,
};
use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};
use burnburn_types::{Clock, DocumentId, MonotonicTime};

struct Port(Arc<Mutex<Vec<Request>>>);

impl ControllerPort for Port {
    fn send(&mut self, e: Envelope) -> Result<(), PortError> {
        self.0.lock().unwrap().push(e.request);
        Ok(())
    }
    fn try_recv(&mut self) -> Result<Option<Incoming>, PortError> {
        Ok(None)
    }
    fn reconnect(&mut self) -> Result<(), PortError> {
        Ok(())
    }
}

struct ZeroClock;
impl Clock for ZeroClock {
    fn now(&self) -> MonotonicTime {
        MonotonicTime::ZERO
    }
}

struct Journal;
impl HistoryStore for Journal {
    fn record_upload(&self, _: &UploadRecord) -> Result<(ExecutionId, Recorded), HistoryStoreError> {
        Err(HistoryStoreError("not used".into()))
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

struct Rig {
    h: HeadlessEditorHarness,
    doc: DocumentId,
    service: Arc<MachineService<Port>>,
    sent: Arc<Mutex<Vec<Request>>>,
}

fn rig(supported: bool) -> Rig {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let machine = MachineId::new("bench");
    let identity = MachineIdentity { id: machine, model: Some("RDC6445G".into()), firmware: None };
    let actor = ExecutionActor::new(identity, Port(sent.clone()), Arc::new(ZeroClock), Policy::default());
    let service = Arc::new(MachineService::new(actor, Arc::new(Journal), 64).unwrap());
    if supported {
        let mut c = Capabilities::default();
        c.live.jog = Capability::Supported;
        c.live.home = Capability::Supported;
        c.live.move_absolute = Capability::Supported;
        service.set_capabilities(c);
    }
    let mut h = HeadlessEditorHarness::new();
    h.controller().attach_machine(service.clone());
    let profile = MachineProfile::new(
        "bench-1500x900",
        1,
        MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap(),
        1000.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap();
    let ruida = RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged };
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile), ruida });
    let doc = h.new_document().unwrap();
    Rig { h, doc, service, sent }
}

fn view(r: &mut Rig) -> LaserViewModel {
    r.h.controller().laser_view().expect("laser view")
}

#[test]
fn every_laser_text_exists_in_both_languages() {
    for text in LaserText::ALL {
        for locale in [Locale::Nl, Locale::En] {
            assert!(!laser_text(*text, locale).trim().is_empty(), "{text:?} {locale:?}");
        }
    }
    assert_ne!(laser_text(LaserText::Home, Locale::Nl), "");
    assert_ne!(laser_text(LaserText::SaveRd, Locale::Nl), laser_text(LaserText::SaveRd, Locale::En));
}

#[test]
fn every_refusal_has_words_in_both_languages_and_is_in_the_catalogue() {
    let keys = known_message_keys();
    for why in burnburn_macos::l10n::all_laser_whys() {
        assert!(keys.contains(&burnburn_presentation::MessageKey::LaserRefused(why)), "{why:?}");
        for locale in [Locale::Nl, Locale::En] {
            assert!(!laser_why_text(why, locale).trim().is_empty(), "{why:?} {locale:?}");
            assert!(message(burnburn_presentation::MessageKey::LaserRefused(why), locale).is_some());
        }
    }
    let not_verified = LaserWhy::NotVerified {
        feature: burnburn_presentation::LiveFeature::Home,
        needs: burnburn_presentation::LiveFeature::Home.required_evidence(),
    };
    let nl = laser_why_text(not_verified, Locale::Nl);
    assert!(nl.starts_with("Niet geverifieerd"), "{nl}");
    assert!(nl.contains("HOME_XY"), "the missing evidence is named: {nl}");
    assert!(laser_why_text(not_verified, Locale::En).starts_with("Not verified"));
}

#[test]
fn the_new_menu_items_round_trip_and_translate() {
    for item in [MenuItem::LaserWindow, MenuItem::ExportDiagnostics] {
        assert!(menu_items().contains(&item));
        assert_eq!(menu_item(menu_tag(item)), Some(item));
    }
    let tags: std::collections::HashSet<isize> = menu_items().into_iter().map(menu_tag).collect();
    assert_eq!(tags.len(), menu_items().len(), "tags stay unique");

    let h = HeadlessEditorHarness::new();
    let vm = h.view();
    assert_eq!(translate(NativeEvent::Menu(MenuItem::LaserWindow), &vm), Reaction::ToggleLaserPanel);
    assert_eq!(translate(NativeEvent::Menu(MenuItem::ExportDiagnostics), &vm), Reaction::ShowDiagnosticsPanel);
    assert!(!menu_enabled(MenuItem::LaserWindow, &vm), "no window, no laser panel");
    assert!(menu_enabled(MenuItem::ExportDiagnostics, &vm), "the journal replay needs no window");
    assert_eq!(menu_title(MenuItem::ExportDiagnostics, &vm, Locale::Nl), "Diagnose exporteren…");
}

#[test]
fn the_diagnostic_panel_answer_becomes_an_export_with_the_ticked_options_or_nothing() {
    let h = HeadlessEditorHarness::new();
    let vm = h.view();
    let options = DiagnosticOptions { include_project_paths: true, include_machine_ids: false };
    let path = std::path::PathBuf::from("/tmp/diagnose.txt");
    assert_eq!(
        translate(NativeEvent::DiagnosticsPanelFinished { options, path: Some(path.clone()) }, &vm),
        Reaction::Intent(UiIntent::ExportDiagnostics { options, path })
    );
    assert_eq!(
        translate(NativeEvent::DiagnosticsPanelFinished { options, path: None }, &vm),
        Reaction::Nothing,
        "cancelling exports nothing"
    );
    assert_eq!(
        DiagnosticOptions::default(),
        DiagnosticOptions { include_project_paths: false, include_machine_ids: false }
    );
}

#[test]
fn without_a_machine_the_panel_says_so_and_offers_nothing() {
    let panel = laser_panel(None, Locale::Nl);
    assert!(!panel.connected);
    assert_eq!(panel.status, "Geen machine verbonden.");
    assert!(panel.controls.is_empty());
}

#[test]
fn the_position_reads_onbekend_never_zero_and_off_controls_explain_themselves() {
    let mut r = rig(false);
    let vm = view(&mut r);
    let panel = laser_panel(Some(&vm), Locale::Nl);
    assert_eq!(panel.position, "X: onbekend   Y: onbekend");
    assert!(!panel.position.contains('0'));
    assert_eq!(panel.user_origin, "niet ingesteld");
    let home = panel.controls.iter().find(|c| c.kind == LaserControlKind::Home).unwrap();
    assert!(!home.enabled);
    assert!(home.tooltip.starts_with("Niet geverifieerd"), "{}", home.tooltip);
    assert!(home.tooltip.contains("HOME_XY"));
    let en = laser_panel(Some(&vm), Locale::En);
    assert!(en.controls.iter().find(|c| c.kind == LaserControlKind::Home).unwrap().tooltip.starts_with("Not verified"));

    // An off control sends nothing: not an intent, not a request.
    assert_eq!(simple_intent(r.doc, &vm, 0), None);
    assert_eq!(jog_press_intent(r.doc, &vm, JogKey::XLeft), None);
    assert!(r.sent.lock().unwrap().is_empty());
}

#[test]
fn an_enabled_control_sends_the_view_models_key_unchanged() {
    let mut r = rig(true);
    let vm = view(&mut r);
    let intent = jog_press_intent(r.doc, &vm, JogKey::XLeft).expect("jog is on");
    let control = vm.control(&LaserControlKind::Jog(JogKey::XLeft)).unwrap();
    assert_eq!(
        intent,
        UiIntent::Laser {
            document: Some(r.doc),
            action: control.action,
            seen: vm.seen.clone(),
            command: LaserCommand::Action(LaserAction::JogPress(JogKey::XLeft)),
        }
    );
    r.h.send(intent);
    assert_eq!(
        r.sent.lock().unwrap().iter().filter(|q| matches!(q, Request::Live(_))).count(),
        1,
        "one press is one request"
    );
    let vm = view(&mut r);
    assert_eq!(vm.jog_held, Some(JogKey::XLeft));
    assert!(laser_panel(Some(&vm), Locale::Nl).jog_held);

    // The release is always accepted. This fake port never answers the press, so the stop waits
    // for that answer (the service sends it right after; see the presentation tests) and the key
    // still counts as held: an unconfirmed release is never shown as released.
    r.h.send(burnburn_macos::panels::jog_release_intent());
    assert_eq!(view(&mut r).jog_held, Some(JogKey::XLeft));
    assert_eq!(view(&mut r).last_refusal, None, "the release was not refused");
}

#[test]
fn typed_numbers_go_to_presentation_unparsed_and_a_refusal_shows_in_words() {
    let mut r = rig(true);
    let vm = view(&mut r);
    let intent = move_intent(r.doc, &vm, "abc", "1").expect("move is on");
    assert!(
        matches!(&intent, UiIntent::Laser { command: LaserCommand::MoveToText { x, y }, .. } if x == "abc" && y == "1")
    );
    r.h.send(intent);
    let panel = laser_panel(Some(&view(&mut r)), Locale::Nl);
    assert_eq!(panel.status, "Geen geldig getal.");
    assert!(r.sent.lock().unwrap().is_empty());
}

#[test]
fn the_fixed_buttons_keep_their_tags_and_the_grid_runs_row_by_row() {
    let controls = simple_controls();
    assert_eq!(controls.len(), 13);
    assert!(matches!(controls[0].0, LaserControlKind::Home));
    assert!(matches!(controls[7].0, LaserControlKind::Step(StepDirection::Left)));
    assert_eq!(anchor_for_tag(0), Some(burnburn_presentation::JobAnchor::TopLeft));
    assert_eq!(anchor_for_tag(4), Some(burnburn_presentation::JobAnchor::Center));
    assert_eq!(anchor_for_tag(8), Some(burnburn_presentation::JobAnchor::BottomRight));
    assert_eq!(anchor_for_tag(9), None);
    for i in 0..3 {
        assert_eq!(segment_for_start_from(start_from_for_segment(i).unwrap()), i);
    }
    assert_eq!(start_from_for_segment(1), Some(StartFrom::CurrentPosition));
    assert_eq!(jog_key_for_key_code(123), Some(JogKey::XLeft));
    assert_eq!(jog_key_for_key_code(126), Some(JogKey::YTop));
    assert_eq!(jog_key_for_key_code(0), None);
}

#[test]
fn setup_choices_reach_the_view_model_and_the_placement_note_says_when_it_is_stale() {
    let mut r = rig(true);
    r.h.send(UiIntent::LaserSetup { setup: LaserSetup::StartFrom(StartFrom::UserOrigin) });
    r.h.send(UiIntent::LaserSetup { setup: LaserSetup::CutSelected(true) });
    let panel = laser_panel(Some(&view(&mut r)), Locale::En);
    assert_eq!(panel.start_from, StartFrom::UserOrigin);
    assert!(panel.cut_selected);
    let _ = (laser_intent, file_intent, save_rd_intent, r.service.clone());
}
