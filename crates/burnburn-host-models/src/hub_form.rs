//! The machine hub window as a [`FormSpec`] (LF6c): one tab per subject (device, production,
//! history, calibration), the lists as read-only scrolling text, the choices as pop-ups and the
//! actions as buttons in their section. The ids below are also the tags the Mac host puts on the
//! controls, so a used control maps to a [`crate::hub_panel::HubEvent`] without a table.

use crate::form::{Control, FormRow, FormSection, FormSpec, split_caption};
use crate::hub_panel::{HubPanel, PROOF_KINDS, proof_kind_title};
use crate::l10n::Locale;
use crate::l10n_machine::{MachineText as T, machine_text};

pub const ID_DEVICE: u32 = 2;
pub const ID_STOP: u32 = 3;
pub const ID_REPEAT: u32 = 4;
pub const ID_CONFIRM: u32 = 5;
pub const ID_CANCEL: u32 = 6;
pub const ID_RECORD: u32 = 7;
pub const ID_REMOVE: u32 = 8;
pub const ID_HISTORY: u32 = 20;
pub const ID_PROOFS: u32 = 21;
pub const ID_KIND: u32 = 22;
/// The six fields of the calibration form (material, speed, value, value Y, method, date).
pub const ID_FIELD_FIRST: u32 = 30;

/// Ids of the read-only rows (notes, texts, logs).
const ID_NOTE_BASE: u32 = 60;

/// The caption without the colon the old layout put behind it.
fn plain(caption: &str) -> String {
    caption.trim_end().trim_end_matches(':').to_owned()
}

/// An empty list as a row: the symbol and one sentence (LF17).
fn empty_row(id: u32, state: &crate::empty_state::EmptyState) -> FormRow {
    FormRow::new(id, "", Control::EmptyState { symbol: state.symbol.to_owned(), text: state.text.clone() })
}

/// The hub window for `panel`.
pub fn hub_form(panel: &HubPanel, locale: Locale) -> FormSpec {
    let text = |t| machine_text(t, locale);
    let none = |v: &[String], t| if v.is_empty() { vec![text(t).to_owned()] } else { v.to_vec() };
    let mut note_id = ID_NOTE_BASE;
    let mut next_note = || {
        note_id += 1;
        note_id
    };
    let note = |id: u32, value: &str| FormRow::new(id, "", Control::ReadOnly { value: value.to_owned() });
    let log = |id: u32, label: &str, value: String| FormRow::new(id, label, Control::Log { text: value });
    let action = |id: u32, title: &str, enabled: bool| {
        let mut r = FormRow::new(id, "", Control::Action { title: title.to_owned() });
        r.enabled = enabled;
        r
    };

    // Device.
    let devices = none(&panel.devices, T::NoDevices);
    let mut device = FormRow::new(
        ID_DEVICE,
        &plain(text(T::DeviceLabel)),
        Control::Choice { options: devices, selected: panel.active_device.unwrap_or(0) },
    )
    .help(text(T::DeviceNote));
    device.enabled = !panel.devices.is_empty();
    let device_row = match &panel.empty.devices {
        Some(e) => empty_row(next_note(), e),
        None => device,
    };
    let device_tab = [
        FormSection::new(None, vec![device_row]),
        FormSection::new(Some(text(T::SupportHeader)), vec![log(next_note(), "", panel.support.join("\n"))]),
    ];

    // Production.
    let production_tab = [
        FormSection::new(
            Some(text(T::UploadHeader)),
            vec![note(next_note(), &panel.upload), action(ID_STOP, text(T::StopUpload), panel.stop_enabled)],
        ),
        FormSection::new(
            Some(text(T::QueueHeader)),
            vec![
                note(next_note(), &panel.queue_counts),
                note(next_note(), text(T::QueueNote)),
                match &panel.empty.queue {
                    Some(e) => empty_row(next_note(), e),
                    None => log(next_note(), "", panel.queue.join("\n")),
                },
            ],
        ),
    ];

    // History.
    let history = none(&panel.history, T::HistoryNone);
    let (shown, can_confirm) = match &panel.repeat {
        Some(r) => {
            let mut lines = vec![r.header.clone()];
            lines.extend(r.changes.iter().map(|c| format!("  {c}")));
            lines.extend(r.refusal.clone());
            (lines.join("\n"), r.can_confirm)
        }
        None => (String::new(), false),
    };
    let mut history_row = FormRow::new(ID_HISTORY, "", Control::Choice { options: history, selected: 0 });
    history_row.enabled = !panel.history.is_empty();
    let history_row = match &panel.empty.history {
        Some(e) => empty_row(next_note(), e),
        None => history_row,
    };
    let history_tab = [FormSection::new(
        Some(text(T::HistoryHeader)),
        vec![
            history_row,
            action(ID_REPEAT, text(T::RepeatButton), !panel.history.is_empty()),
            log(next_note(), "", shown),
            action(ID_CONFIRM, text(T::RepeatConfirm), can_confirm),
            action(ID_CANCEL, text(T::RepeatCancel), panel.repeat.is_some()),
            note(next_note(), text(T::RepeatNote)),
        ],
    )];

    // Calibration.
    let kinds: Vec<String> = PROOF_KINDS.iter().map(|k| proof_kind_title(*k, locale).to_owned()).collect();
    let mut rows = vec![FormRow::new(ID_KIND, text(T::LabelKind), Control::Choice { options: kinds, selected: 0 })];
    for (i, label) in [T::LabelMaterial, T::LabelSpeed, T::LabelValue, T::LabelValueY, T::LabelMethod, T::LabelDate]
        .into_iter()
        .enumerate()
    {
        let split = split_caption(text(label));
        let mut row = FormRow::new(
            ID_FIELD_FIRST + i as u32,
            &split.label,
            Control::Text { value: String::new(), placeholder: String::new() },
        );
        row.unit = split.unit;
        rows.push(row);
    }
    let mut record = action(ID_RECORD, text(T::RecordProof), true);
    if !panel.proof_problem.is_empty() {
        record.error = Some(panel.proof_problem.clone());
    }
    rows.push(record);
    let proofs = none(&panel.proofs, T::ProofNone);
    let mut proofs_row = FormRow::new(ID_PROOFS, "", Control::Choice { options: proofs, selected: 0 });
    proofs_row.enabled = !panel.proofs.is_empty();
    rows.push(match &panel.empty.proofs {
        Some(e) => empty_row(next_note(), e),
        None => proofs_row,
    });
    rows.push(action(ID_REMOVE, text(T::RemoveProof), !panel.proofs.is_empty()));
    rows.push(note(next_note(), text(T::CalibrationNote)));
    let calibration_tab = [FormSection::new(Some(text(T::CalibrationHeader)), rows)];

    let mut sections = Vec::new();
    for (tab, parts) in [
        (T::TabDevice, device_tab.to_vec()),
        (T::TabProduction, production_tab.to_vec()),
        (T::TabHistory, history_tab.to_vec()),
        (T::TabCalibration, calibration_tab.to_vec()),
    ] {
        sections.extend(parts.into_iter().map(|s| s.on_tab(text(tab))));
    }
    FormSpec { title: text(T::HubTitle).to_owned(), sections, ..FormSpec::default() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::form::{Control, FormRow};
    use crate::format::Quantity;
    use crate::hub_panel::RepeatPanel;

    fn panel() -> HubPanel {
        HubPanel {
            devices: vec!["Ruida 600x400".into()],
            active_device: Some(0),
            support: vec!["Snijden: ondersteund".into(), "Graveren: niet geverifieerd".into()],
            upload: "Geen upload".into(),
            stop_enabled: false,
            queue_counts: "0 in wachtrij".into(),
            queue: vec!["Deur: gereed".into()],
            history: vec!["Deur: klaar".into()],
            repeat: None,
            proofs: vec![],
            proof_problem: String::new(),
            empty: Default::default(),
        }
    }

    fn row(spec: &FormSpec, id: u32) -> &FormRow {
        spec.sections.iter().flat_map(|s| &s.rows).find(|r| r.id.0 == id).expect("row")
    }

    #[test]
    fn the_hub_has_a_tab_per_subject_and_no_form_problems() {
        let spec = hub_form(&panel(), Locale::Nl);
        assert_eq!(spec.title, "Machine en productie");
        assert_eq!(spec.tabs(), vec!["Apparaat", "Productie", "Historie", "Kalibratie"]);
        assert_eq!(spec.problems(), Vec::new());
        let en = hub_form(&panel(), Locale::En);
        assert_eq!(en.tabs(), vec!["Device", "Production", "History", "Calibration"]);
    }

    #[test]
    fn the_device_tab_has_the_choice_without_a_colon_and_the_support_lines_as_a_log() {
        let spec = hub_form(&panel(), Locale::Nl);
        let device = row(&spec, ID_DEVICE);
        assert_eq!(device.label, "Actief apparaat");
        assert!(matches!(&device.control, Control::Choice { options, selected: 0 } if options == &["Ruida 600x400"]));
        let log = spec.sections.iter().flat_map(|s| &s.rows).find_map(|r| match &r.control {
            Control::Log { text } if text.contains("Snijden") => Some(text.clone()),
            _ => None,
        });
        assert_eq!(log.as_deref(), Some("Snijden: ondersteund\nGraveren: niet geverifieerd"));
    }

    #[test]
    fn actions_follow_what_the_model_allows() {
        let mut p = panel();
        let spec = hub_form(&p, Locale::Nl);
        assert!(!row(&spec, ID_STOP).enabled, "no upload to stop");
        assert!(row(&spec, ID_REPEAT).enabled);
        assert!(!row(&spec, ID_CONFIRM).enabled && !row(&spec, ID_CANCEL).enabled, "no repeat in preparation");
        p.stop_enabled = true;
        p.repeat = Some(RepeatPanel {
            header: "Herhaal Deur".into(),
            changes: vec!["geen".into()],
            refusal: None,
            can_confirm: true,
        });
        let spec = hub_form(&p, Locale::Nl);
        assert!(row(&spec, ID_STOP).enabled && row(&spec, ID_CONFIRM).enabled && row(&spec, ID_CANCEL).enabled);
        p.history.clear();
        p.devices.clear();
        let spec = hub_form(&p, Locale::Nl);
        assert!(!row(&spec, ID_REPEAT).enabled, "nothing to repeat");
        assert!(!row(&spec, ID_DEVICE).enabled, "no device to choose");
    }

    #[test]
    fn the_proof_problem_is_said_once_at_the_record_button() {
        let mut p = panel();
        p.proof_problem = "Vul een snelheid in.".into();
        let spec = hub_form(&p, Locale::Nl);
        let record = row(&spec, ID_RECORD);
        assert_eq!(record.error.as_deref(), Some("Vul een snelheid in."));
        let said = spec.sections.iter().flat_map(|s| s.texts()).filter(|t| t == "Vul een snelheid in.").count();
        assert_eq!(said, 1);
        for i in 0..6 {
            assert!(matches!(row(&spec, ID_FIELD_FIRST + i).control, Control::Text { .. }));
        }
        let speed = row(&spec, ID_FIELD_FIRST + 1);
        assert_eq!((speed.label.as_str(), speed.unit), ("Snelheid", Some(Quantity::MillimetersPerSecond)));
    }
}
