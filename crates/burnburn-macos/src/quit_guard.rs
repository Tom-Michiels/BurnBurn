//! Quit guard (G5, update gate X07): quitting while a transfer, a stored job, a start or a live
//! machine action is in progress needs an explicit confirmation that names the reason. The
//! decision is `burnburn_runtime::update_gate::decide_update`; this module only maps the answer to
//! words and to the alert's buttons, so it is tested without AppKit.

use burnburn_presentation::MachineActivity;
use burnburn_runtime::update_gate::{DeferReason, UpdateDecision, decide_update};

use crate::l10n::Locale;

/// The user's answer to the guard alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardChoice {
    /// Do not quit (the safe default and the first button).
    Stay,
    /// Quit anyway; the machine state stays unknown until the operator resolves it.
    QuitAnyway,
}

/// Buttons of the guard alert, in the order the host adds them.
pub const GUARD_ALERT_BUTTONS: [GuardChoice; 2] = [GuardChoice::Stay, GuardChoice::QuitAnyway];

/// Why quitting needs a confirmation right now, if it does.
pub fn quit_guard(activity: &MachineActivity) -> Option<DeferReason> {
    match decide_update(&activity.executions, activity.live_control) {
        UpdateDecision::Apply => None,
        UpdateDecision::Defer(reason) => Some(reason),
    }
}

/// Title and explanation of the alert for `reason`.
pub fn guard_message(reason: DeferReason, locale: Locale) -> (&'static str, &'static str) {
    let (title, info) = match reason {
        DeferReason::LiveControl => (
            ("De machine wordt nu bediend", "The machine is being operated"),
            (
                "Een beweging, frame of andere directe machineactie loopt nog. Na afsluiten is niet zeker waar de kop staat.",
                "A move, frame or other live machine action is still active. After quitting the head position is not certain.",
            ),
        ),
        DeferReason::JobStored => (
            ("Er staat een job op de controller", "A job is stored on the controller"),
            (
                "De job is opgeslagen maar niet gestart. Afsluiten start niets; de job blijft op de controller staan.",
                "The job is stored but not started. Quitting starts nothing; the job stays on the controller.",
            ),
        ),
        DeferReason::TransferInProgress => (
            ("Een upload is bezig", "An upload is in progress"),
            (
                "De job wordt nog naar de controller gestuurd. Afsluiten onderbreekt de overdracht; de uitkomst blijft onbekend.",
                "The job is still being sent to the controller. Quitting interrupts the transfer; the outcome stays unknown.",
            ),
        ),
        DeferReason::OutcomeUnresolved => (
            ("Een uitkomst is nog onbekend", "An outcome is still unknown"),
            (
                "Van een eerdere job is niet vastgesteld wat er gebeurde. Registreer de uitkomst voordat je afsluit, of sluit toch af.",
                "What happened to an earlier job has not been established. Record the outcome before quitting, or quit anyway.",
            ),
        ),
        DeferReason::StartPending => (
            ("Een start is aangevraagd", "A start was requested"),
            (
                "De controller heeft nog niet bevestigd dat de job draait. Afsluiten laat de uitkomst onbekend.",
                "The controller has not confirmed that the job is running. Quitting leaves the outcome unknown.",
            ),
        ),
        DeferReason::JobRunning => (
            ("De machine voert een job uit", "The machine is running a job"),
            (
                "De job loopt op de controller en stopt niet door af te sluiten. BurnBurn kan hem daarna niet meer volgen of stoppen.",
                "The job is running on the controller and does not stop when you quit. BurnBurn can no longer follow or stop it.",
            ),
        ),
    };
    match locale {
        Locale::Nl => (title.0, info.0),
        Locale::En => (title.1, info.1),
    }
}

/// Button caption.
pub fn guard_button(choice: GuardChoice, locale: Locale) -> &'static str {
    match (choice, locale) {
        (GuardChoice::Stay, Locale::Nl) => "Blijf in BurnBurn",
        (GuardChoice::Stay, Locale::En) => "Stay in BurnBurn",
        (GuardChoice::QuitAnyway, Locale::Nl) => "Toch afsluiten",
        (GuardChoice::QuitAnyway, Locale::En) => "Quit Anyway",
    }
}

/// The choice for an alert response; anything unexpected keeps the app open.
pub fn guard_choice(response: isize) -> GuardChoice {
    match response.checked_sub(crate::adapter::response::FIRST_BUTTON).and_then(|i| usize::try_from(i).ok()) {
        Some(1) => GuardChoice::QuitAnyway,
        _ => GuardChoice::Stay,
    }
}
