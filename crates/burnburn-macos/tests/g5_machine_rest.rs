//! G5-machine-rest, macOS host without AppKit: the quit guard (update gate) and the memory notice
//! of the undo history in words.

#![allow(clippy::unwrap_used, clippy::panic)]

use burnburn_machine::{ExecutionState, UnknownReason};
use burnburn_macos::adapter::{guard_choice, response};
use burnburn_macos::l10n::{Locale, known_message_keys, message};
use burnburn_macos::quit_guard::{GUARD_ALERT_BUTTONS, GuardChoice, guard_button, guard_message, quit_guard};
use burnburn_presentation::{HeadlessEditorHarness, MachineActivity, MessageKey};
use burnburn_runtime::update_gate::DeferReason;

fn activity(states: &[ExecutionState], live: bool) -> MachineActivity {
    MachineActivity { executions: states.to_vec(), live_control: live }
}

#[test]
fn quitting_is_free_when_the_machine_is_idle() {
    assert_eq!(quit_guard(&MachineActivity::default()), None);
    assert_eq!(quit_guard(&activity(&[ExecutionState::Completed, ExecutionState::Cancelled], false)), None);
    let mut h = HeadlessEditorHarness::new();
    h.new_document().unwrap();
    assert_eq!(quit_guard(&h.controller().machine_activity()), None, "no machine attached");
}

#[test]
fn quitting_during_a_transfer_a_run_or_live_control_asks_with_the_most_urgent_reason() {
    let up = ExecutionState::Uploading { acked: 1, total: 4 };
    assert_eq!(quit_guard(&activity(&[up], false)), Some(DeferReason::TransferInProgress));
    assert_eq!(quit_guard(&activity(&[ExecutionState::Stored], false)), Some(DeferReason::JobStored));
    assert_eq!(quit_guard(&activity(&[], true)), Some(DeferReason::LiveControl));
    let lost = ExecutionState::Unknown(UnknownReason::StartReplyLost);
    assert_eq!(quit_guard(&activity(&[lost], false)), Some(DeferReason::OutcomeUnresolved));
    assert_eq!(
        quit_guard(&activity(&[ExecutionState::Stored, ExecutionState::Running], true)),
        Some(DeferReason::JobRunning)
    );
}

#[test]
fn every_reason_has_its_own_words_in_both_languages_and_the_buttons_are_named() {
    let reasons = [
        DeferReason::LiveControl,
        DeferReason::JobStored,
        DeferReason::TransferInProgress,
        DeferReason::OutcomeUnresolved,
        DeferReason::StartPending,
        DeferReason::JobRunning,
    ];
    for locale in [Locale::Nl, Locale::En] {
        let mut seen = std::collections::BTreeSet::new();
        for reason in reasons {
            let (title, info) = guard_message(reason, locale);
            assert!(!title.trim().is_empty() && !info.trim().is_empty(), "{reason:?} {locale:?}");
            assert!(seen.insert(info), "{reason:?} repeats another reason's explanation");
        }
        for choice in GUARD_ALERT_BUTTONS {
            assert!(!guard_button(choice, locale).trim().is_empty(), "{choice:?} {locale:?}");
        }
    }
    assert!(guard_message(DeferReason::JobRunning, Locale::Nl).1.contains("loopt"), "says the job is running");
}

#[test]
fn the_first_button_stays_and_anything_unexpected_keeps_the_app_open() {
    assert_eq!(GUARD_ALERT_BUTTONS[0], GuardChoice::Stay);
    assert_eq!(guard_choice(response::FIRST_BUTTON), GuardChoice::Stay);
    assert_eq!(guard_choice(response::SECOND_BUTTON), GuardChoice::QuitAnyway);
    assert_eq!(guard_choice(response::THIRD_BUTTON), GuardChoice::Stay);
    assert_eq!(guard_choice(0), GuardChoice::Stay);
    assert_eq!(guard_choice(-5), GuardChoice::Stay);
}

#[test]
fn the_trim_notice_says_how_many_steps_can_no_longer_be_undone() {
    let key = MessageKey::HistoryTrimmed(7);
    assert!(known_message_keys().iter().any(|k| matches!(k, MessageKey::HistoryTrimmed(_))));
    let nl = message(key, Locale::Nl).unwrap();
    assert_eq!(nl, "De oudste 7 stappen zijn niet meer ongedaan te maken (geheugen).");
    let en = message(key, Locale::En).unwrap();
    assert!(en.contains('7') && en.contains("undo"), "{en}");
    assert_eq!(
        message(MessageKey::HistoryTrimmed(1), Locale::Nl).unwrap(),
        "De oudste stap is niet meer ongedaan te maken (geheugen)."
    );
}
