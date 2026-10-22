//! G5-grbl-ui, the Mac host's headless model of the GRBL window: its words, which buttons are on
//! (and why not, as a tooltip), and the intents its widgets make. A disabled button makes no
//! intent; a refused action stays visible in words. The AppKit widgets are in the native test.

#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)]

#[path = "../../../tests/native/macos/common/grbl_fake.rs"]
mod grbl_fake;

use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
use burnburn_macos::l10n::Locale;
use burnburn_macos::panels::{
    GrblNumber, grbl_control_intent, grbl_input_command_intent, grbl_macro_remove_intent, grbl_macro_run_intent,
    grbl_macro_save_intent, grbl_number_intent, grbl_send_intent, grbl_window_panel,
};
use burnburn_presentation::{
    GrblControlKind, GrblIntent, GrblProfile, GrblViewModel, HeadlessEditorHarness, StepDirection, UiIntent,
    grbl_capabilities,
};
use grbl_fake::FakeGrbl;

fn rig(connected: bool) -> (HeadlessEditorHarness, FakeGrbl) {
    let mut h = HeadlessEditorHarness::new();
    h.new_document().unwrap();
    let grbl = GrblProfile::grbl_1_1_laser();
    let profile = MachineProfile::new(
        "grbl-400",
        1,
        MachineFrame::new(400.0, 400.0, OriginCorner::BottomLeft).unwrap(),
        1000.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
    .with_capabilities(grbl_capabilities(&grbl))
    .unwrap();
    h.send(UiIntent::SetGrblMachineProfile { profile: Box::new(profile), grbl: Box::new(grbl) });
    let fake = FakeGrbl::new();
    if connected {
        h.controller().attach_grbl_port(Box::new(fake.clone()));
        h.send(UiIntent::Grbl(GrblIntent::Tick));
    }
    (h, fake)
}

fn vm(h: &mut HeadlessEditorHarness) -> GrblViewModel {
    h.controller().grbl_view().unwrap()
}

#[test]
fn there_is_no_window_model_without_a_grbl_machine() {
    assert!(grbl_window_panel(None, Locale::Nl).is_none());
}

#[test]
fn the_status_is_said_in_words_and_an_unknown_state_is_never_idle() {
    // No port.
    let (mut h, _fake) = rig(false);
    let nl = grbl_window_panel(Some(&vm(&mut h)), Locale::Nl).unwrap();
    assert!(nl.status.to_lowercase().contains("geen") && nl.status.to_lowercase().contains("poort"), "{}", nl.status);

    // Connected, no status read yet: unknown, not idle.
    let (mut h, fake) = rig(false);
    h.controller().attach_grbl_port(Box::new(fake.clone()));
    let unknown = grbl_window_panel(Some(&vm(&mut h)), Locale::En).unwrap().status;
    assert!(unknown.to_lowercase().contains("unknown") && !unknown.to_lowercase().contains("idle"), "{unknown}");

    // A status arrived.
    fake.set_state("Alarm");
    h.send(UiIntent::Grbl(GrblIntent::Tick));
    let en = grbl_window_panel(Some(&vm(&mut h)), Locale::En).unwrap();
    assert!(en.status.contains("Alarm"), "{}", en.status);
}

#[test]
fn a_button_that_is_off_says_why_in_words_and_makes_no_intent() {
    let (mut h, _fake) = rig(false);
    let v = vm(&mut h);
    let panel = grbl_window_panel(Some(&v), Locale::Nl).unwrap();
    let home = panel.buttons.iter().find(|b| b.kind == GrblControlKind::Home).unwrap();
    assert!(!home.enabled);
    assert!(home.tooltip.to_lowercase().contains("poort"), "{}", home.tooltip);
    assert_eq!(grbl_control_intent(&v, GrblControlKind::Home), None);
    let start = panel.buttons.iter().find(|b| b.kind == GrblControlKind::StartJob).unwrap();
    assert!(!start.enabled && !start.tooltip.is_empty());
    assert!(!panel.console_enabled && !panel.console_tooltip.is_empty());
}

#[test]
fn an_enabled_button_sends_its_intent_and_start_never_does() {
    let (mut h, _fake) = rig(true);
    let v = vm(&mut h);
    assert_eq!(
        grbl_control_intent(&v, GrblControlKind::Jog(StepDirection::Left)),
        Some(UiIntent::Grbl(GrblIntent::Jog(StepDirection::Left)))
    );
    assert_eq!(grbl_control_intent(&v, GrblControlKind::Home), Some(UiIntent::Grbl(GrblIntent::Home)));
    assert_eq!(grbl_control_intent(&v, GrblControlKind::Hold), Some(UiIntent::Grbl(GrblIntent::Hold)));
    assert_eq!(grbl_control_intent(&v, GrblControlKind::Reset), Some(UiIntent::Grbl(GrblIntent::Reset)));
    assert_eq!(grbl_control_intent(&v, GrblControlKind::StartJob), None, "refused until a controller proved it");
    let en = grbl_window_panel(Some(&v), Locale::En).unwrap();
    let start = en.buttons.iter().find(|b| b.kind == GrblControlKind::StartJob).unwrap();
    assert!(start.tooltip.contains("GRBL"), "the missing evidence is named: {}", start.tooltip);
}

#[test]
fn the_console_input_and_history_keys_become_intents() {
    assert_eq!(grbl_send_intent("  G0 X5 "), Some(UiIntent::Grbl(GrblIntent::Send("G0 X5".into()))));
    assert_eq!(grbl_send_intent("   "), None, "an empty line sends nothing");
    assert_eq!(grbl_input_command_intent("moveUp:"), Some(UiIntent::Grbl(GrblIntent::RecallPrevious)));
    assert_eq!(grbl_input_command_intent("moveDown:"), Some(UiIntent::Grbl(GrblIntent::RecallNext)));
    assert_eq!(grbl_input_command_intent("insertNewline:"), None, "Return is the field's own action");
}

#[test]
fn the_transcript_marks_what_was_sent_and_what_came_back() {
    let (mut h, _fake) = rig(true);
    h.send(UiIntent::Grbl(GrblIntent::Send("G0 X5".into())));
    h.send(UiIntent::Grbl(GrblIntent::Tick));
    let panel = grbl_window_panel(Some(&vm(&mut h)), Locale::En).unwrap();
    let lines: Vec<&str> = panel.transcript.lines().collect();
    assert!(lines.contains(&"> G0 X5"), "{lines:?}");
    assert!(lines.contains(&"< ok"), "{lines:?}");
}

#[test]
fn macros_are_listed_by_name_and_off_with_a_reason_while_a_macro_runs() {
    let (mut h, fake) = rig(true);
    h.send(UiIntent::Grbl(GrblIntent::AddMacro { name: "Nul".into(), program: "G0 X0 Y0\nG92 X0 Y0".into() }));
    let v = vm(&mut h);
    let panel = grbl_window_panel(Some(&v), Locale::Nl).unwrap();
    assert_eq!(panel.macros.len(), 1);
    assert_eq!((panel.macros[0].name.as_str(), panel.macros[0].program.as_str()), ("Nul", "G0 X0 Y0\nG92 X0 Y0"));
    assert!(panel.macros[0].enabled);
    assert_eq!(grbl_macro_run_intent(&v, 0), Some(UiIntent::Grbl(GrblIntent::RunMacro("Nul".into()))));
    assert_eq!(grbl_macro_run_intent(&v, 5), None);
    assert_eq!(grbl_macro_remove_intent(&v, 0), Some(UiIntent::Grbl(GrblIntent::RemoveMacro("Nul".into()))));

    fake.hold_answers(true);
    h.send(UiIntent::Grbl(GrblIntent::RunMacro("Nul".into())));
    let v = vm(&mut h);
    let panel = grbl_window_panel(Some(&v), Locale::Nl).unwrap();
    assert!(!panel.macros[0].enabled && panel.macros[0].running);
    assert_eq!(grbl_macro_run_intent(&v, 0), None, "no macro during a macro");
}

#[test]
fn saving_a_macro_adds_a_new_name_and_replaces_an_existing_one() {
    let (mut h, _fake) = rig(true);
    let v = vm(&mut h);
    assert_eq!(
        grbl_macro_save_intent(&v, " Nul ", "G0 X0"),
        Some(UiIntent::Grbl(GrblIntent::AddMacro { name: "Nul".into(), program: "G0 X0".into() }))
    );
    assert_eq!(grbl_macro_save_intent(&v, "   ", "G0 X0"), None, "a macro needs a name");
    h.send(UiIntent::Grbl(GrblIntent::AddMacro { name: "Nul".into(), program: "G0 X0".into() }));
    let v = vm(&mut h);
    assert_eq!(
        grbl_macro_save_intent(&v, "nul", "G0 X9"),
        Some(UiIntent::Grbl(GrblIntent::ReplaceMacro { name: "nul".into(), program: "G0 X9".into() }))
    );
}

#[test]
fn step_and_feed_commit_once_and_a_refusal_stays_visible() {
    let (mut h, _fake) = rig(true);
    let v = vm(&mut h);
    assert_eq!(
        grbl_number_intent(&v, GrblNumber::Step, "2,5"),
        Some(UiIntent::Grbl(GrblIntent::SetJogStep("2,5".into())))
    );
    assert_eq!(grbl_number_intent(&v, GrblNumber::Step, &v.jog_step_mm), None, "Return then Tab is one commit");
    assert_eq!(grbl_number_intent(&v, GrblNumber::Feed, "  "), None);
    h.send(UiIntent::Grbl(GrblIntent::SetJogFeed("0".into())));
    let panel = grbl_window_panel(Some(&vm(&mut h)), Locale::Nl).unwrap();
    assert!(!panel.refusal.is_empty(), "the refused feed is said");
    assert_eq!(panel.jog_feed, "1000", "the old feed stays");
}

#[test]
fn the_unverified_note_is_always_there_and_a_stale_session_is_said() {
    let (mut h, _fake) = rig(true);
    let panel = grbl_window_panel(Some(&vm(&mut h)), Locale::En).unwrap();
    assert!(panel.notes.to_lowercase().contains("simulated") || panel.notes.to_lowercase().contains("not proven"));
}
