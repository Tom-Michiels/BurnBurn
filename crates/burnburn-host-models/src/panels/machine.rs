//! The machine section.

use burnburn_presentation::{MachineButton, MachineViewModel, UiIntent};
use burnburn_types::DocumentId;

use crate::l10n::{HostText, Locale, host_text, machine_status_text, message_or_key};

/// The machine section: status in words (never "klaar" before the machine reports it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachinePanel {
    pub status: String,
    pub message: String,
    pub progress: String,
    pub job: String,
    /// `(label, enabled)` per control, in view-model order.
    pub buttons: Vec<(String, bool)>,
}

pub fn machine_panel(vm: Option<&MachineViewModel>, locale: Locale) -> MachinePanel {
    let Some(m) = vm else {
        return MachinePanel {
            status: host_text(HostText::NoMachine, locale).to_owned(),
            message: String::new(),
            progress: String::new(),
            job: String::new(),
            buttons: Vec::new(),
        };
    };
    let progress = m.progress.map_or_else(String::new, |(done, total)| match locale {
        Locale::Nl => format!("{done} van {total} pakketten ontvangen (transport)"),
        Locale::En => format!("{done} of {total} packets received (transport)"),
    });
    MachinePanel {
        status: machine_status_text(m.status, locale).to_owned(),
        message: m.message.map(|k| message_or_key(k, locale)).unwrap_or_default(),
        progress,
        job: m.job_name.clone().unwrap_or_default(),
        buttons: m.buttons.iter().map(|b| (message_or_key(b.label, locale), b.enabled)).collect(),
    }
}

/// A click on a machine control: `action` and `seen` go to presentation unchanged (idempotency,
/// IM31); a disabled control sends nothing. Start only asks for the confirmation (M06).
pub fn machine_intent(document: Option<DocumentId>, button: &MachineButton) -> Option<UiIntent> {
    if !button.enabled {
        return None;
    }
    // Start only asks (M06): the confirmation names job, machine, revision and place first.
    if button.action.kind == burnburn_presentation::MachineAction::Start {
        return Some(UiIntent::RequestStart { document });
    }
    Some(UiIntent::Machine { document, action: button.action, seen: button.seen.clone() })
}
