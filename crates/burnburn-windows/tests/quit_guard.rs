//! Quit guard on Windows (X07, update gate): closing the last window is quitting, and quitting while
//! the machine is busy asks first, with the same reason and words as the macOS alert. Headless.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_machine::ExecutionState;
use burnburn_presentation::MachineActivity;
use burnburn_runtime::update_gate::DeferReason;
use burnburn_windows::adapter::{button, close_guard, on_guard_box};
use burnburn_windows::l10n::Locale;
use burnburn_windows::quit_guard::{GuardChoice, guard_message};

fn busy(states: &[ExecutionState], live: bool) -> MachineActivity {
    MachineActivity { executions: states.to_vec(), live_control: live }
}

#[test]
fn closing_the_last_window_during_a_run_asks_with_the_reason() {
    let running = busy(&[ExecutionState::Running], false);
    assert_eq!(close_guard(&running, 1), Some(DeferReason::JobRunning));
    let up = busy(&[ExecutionState::Uploading { acked: 1, total: 3 }], false);
    assert_eq!(close_guard(&up, 1), Some(DeferReason::TransferInProgress));
    assert_eq!(close_guard(&busy(&[], true), 1), Some(DeferReason::LiveControl));
    let (title, _) = guard_message(DeferReason::JobRunning, Locale::Nl);
    assert!(title.contains("job"), "{title}");
}

#[test]
fn closing_one_of_several_windows_or_an_idle_machine_never_asks() {
    assert_eq!(close_guard(&busy(&[ExecutionState::Running], true), 2), None);
    assert_eq!(close_guard(&MachineActivity::default(), 1), None);
    assert_eq!(close_guard(&busy(&[ExecutionState::Completed], false), 1), None);
}

#[test]
fn only_an_explicit_yes_quits_and_everything_else_stays() {
    assert_eq!(on_guard_box(button::IDYES), GuardChoice::QuitAnyway);
    assert_eq!(on_guard_box(button::IDNO), GuardChoice::Stay);
    assert_eq!(on_guard_box(button::IDCANCEL), GuardChoice::Stay);
    assert_eq!(on_guard_box(0), GuardChoice::Stay);
}
