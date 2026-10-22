//! The machine hub window on Windows (G5-machine-rest, G5-native): devices, the support list, the
//! upload with its stop button, the queue and counts, the history with Repeat, the repeat preview
//! and the calibration proofs, as one scrolling panel. The words, rows and intents are the macOS
//! window's (`crate::hub_panel`, compiled from the macOS source file): this module only gives them
//! control ids and decodes the events. Nothing here starts, frames or uploads anything.

use super::app::AppInput;
use super::{ControlSpec, PanelEvent, PanelReaction, PanelSpec, Row, notify};
use crate::hub_panel::{HubEvent, PROOF_KINDS, ProofForm, hub_intent, hub_panel, proof_kind_title};
use crate::l10n::Locale;
use crate::l10n_machine::{MachineText as T, machine_text};

/// Control ids of the hub (`B + 130 ..= B + 199` of the app block).
pub mod id {
    use super::super::ids::APP_CONTROLS as B;

    pub const DEVICE_HEADING: u16 = B + 130;
    pub const DEVICE: u16 = B + 131;
    pub const DEVICE_CAPTION: u16 = B + 153;
    pub const SUPPORT: u16 = B + 132;
    pub const UPLOAD_HEADING: u16 = B + 133;
    pub const UPLOAD: u16 = B + 134;
    pub const STOP: u16 = B + 135;
    pub const QUEUE_HEADING: u16 = B + 136;
    pub const QUEUE_COUNTS: u16 = B + 137;
    pub const QUEUE: u16 = B + 138;
    pub const HISTORY_HEADING: u16 = B + 139;
    pub const HISTORY: u16 = B + 140;
    pub const REPEAT: u16 = B + 141;
    pub const REPEAT_HEADER: u16 = B + 142;
    pub const REPEAT_CHANGES: u16 = B + 143;
    pub const REPEAT_REFUSAL: u16 = B + 144;
    pub const CONFIRM: u16 = B + 145;
    pub const CANCEL: u16 = B + 146;
    pub const CALIBRATION_HEADING: u16 = B + 150;
    pub const PROOFS: u16 = B + 151;
    pub const REMOVE: u16 = B + 152;
    /// Captions of the form: `+0..+6`.
    pub const CAPTION: u16 = B + 160;
    pub const KIND: u16 = B + 167;
    pub const MATERIAL: u16 = B + 168;
    pub const SPEED: u16 = B + 169;
    pub const VALUE: u16 = B + 170;
    pub const VALUE_Y: u16 = B + 171;
    pub const METHOD: u16 = B + 172;
    pub const DATE: u16 = B + 173;
    pub const RECORD: u16 = B + 174;
    pub const PROBLEM: u16 = B + 175;
}

fn mt(text: T, locale: Locale) -> &'static str {
    machine_text(text, locale)
}

pub fn hub_spec(input: &AppInput<'_>) -> PanelSpec {
    let locale = input.locale;
    let p = hub_panel(input.hub, input.document.is_some(), locale);
    let mut rows = vec![
        Row::one(ControlSpec::heading(id::DEVICE_HEADING, mt(T::TabDevice, locale))),
        Row::of(vec![
            ControlSpec::label(id::DEVICE_CAPTION, mt(T::DeviceLabel, locale)).weighted(2),
            ControlSpec::combo(id::DEVICE, p.devices.clone(), p.active_device, !p.devices.is_empty()),
        ]),
    ];
    let support = if p.devices.is_empty() { mt(T::NoDevices, locale).to_owned() } else { p.support.join("\n") };
    rows.push(Row::one(ControlSpec::report(id::SUPPORT, 8, support)));

    rows.push(Row::one(ControlSpec::heading(id::UPLOAD_HEADING, mt(T::UploadHeader, locale))));
    rows.push(Row::of(vec![
        ControlSpec::label(id::UPLOAD, p.upload.clone()).weighted(3),
        ControlSpec::button(id::STOP, mt(T::StopUpload, locale), p.stop_enabled),
    ]));

    rows.push(Row::one(ControlSpec::heading(id::QUEUE_HEADING, mt(T::QueueHeader, locale))));
    rows.push(Row::one(ControlSpec::label(id::QUEUE_COUNTS, p.queue_counts.clone())));
    rows.push(Row::one(ControlSpec::list(id::QUEUE, 5, p.queue.clone(), None, true)));

    rows.push(Row::one(ControlSpec::heading(id::HISTORY_HEADING, mt(T::HistoryHeader, locale))));
    let history = if p.history.is_empty() { vec![mt(T::HistoryNone, locale).to_owned()] } else { p.history.clone() };
    rows.push(Row::one(ControlSpec::list(id::HISTORY, 6, history, None, !p.history.is_empty())));
    rows.push(Row::one(ControlSpec::button(id::REPEAT, mt(T::RepeatButton, locale), !p.history.is_empty())));
    let repeat = p.repeat.as_ref();
    rows.push(Row::one(
        ControlSpec::label(id::REPEAT_HEADER, repeat.map_or(String::new(), |r| r.header.clone()))
            .hidden_if(repeat.is_none()),
    ));
    rows.push(Row::one(
        ControlSpec::report(id::REPEAT_CHANGES, 5, repeat.map_or(String::new(), |r| r.changes.join("\n")))
            .hidden_if(repeat.is_none()),
    ));
    rows.push(Row::one(
        ControlSpec::label(id::REPEAT_REFUSAL, repeat.and_then(|r| r.refusal.clone()).unwrap_or_default())
            .hidden_if(repeat.is_none()),
    ));
    let can_confirm = repeat.is_some_and(|r| r.can_confirm);
    rows.push(Row::of(vec![
        ControlSpec::button(id::CONFIRM, mt(T::RepeatConfirm, locale), can_confirm).hidden_if(repeat.is_none()),
        ControlSpec::button(id::CANCEL, mt(T::RepeatCancel, locale), repeat.is_some()).hidden_if(repeat.is_none()),
    ]));

    rows.push(Row::one(ControlSpec::heading(id::CALIBRATION_HEADING, mt(T::CalibrationHeader, locale))));
    let proofs = if p.proofs.is_empty() { vec![mt(T::ProofNone, locale).to_owned()] } else { p.proofs.clone() };
    rows.push(Row::one(ControlSpec::list(id::PROOFS, 4, proofs, None, !p.proofs.is_empty())));
    rows.push(Row::one(ControlSpec::button(id::REMOVE, mt(T::RemoveProof, locale), !p.proofs.is_empty())));
    let kinds: Vec<String> = PROOF_KINDS.iter().map(|k| proof_kind_title(*k, locale).to_owned()).collect();
    let captions =
        [T::LabelKind, T::LabelMaterial, T::LabelSpeed, T::LabelValue, T::LabelValueY, T::LabelMethod, T::LabelDate];
    let labelled = |n: u16, control: ControlSpec| {
        Row::of(vec![ControlSpec::label(id::CAPTION + n, mt(captions[usize::from(n)], locale)).weighted(2), control])
    };
    rows.push(labelled(0, ControlSpec::combo(id::KIND, kinds, Some(0), true)));
    for (n, cid) in
        [(1, id::MATERIAL), (2, id::SPEED), (3, id::VALUE), (4, id::VALUE_Y), (5, id::METHOD), (6, id::DATE)]
    {
        rows.push(labelled(n, ControlSpec::edit(cid, "", true)));
    }
    rows.push(Row::one(ControlSpec::button(id::RECORD, mt(T::RecordProof, locale), true)));
    rows.push(Row::one(ControlSpec::label(id::PROBLEM, p.proof_problem.clone())));
    PanelSpec { title: mt(T::HubTitle, locale).to_owned(), rows }
}

/// A row of a list or pop-up the user chose.
fn chosen(values: &super::PanelValues, cid: u16) -> Option<usize> {
    usize::try_from(values.selected(cid)).ok()
}

pub fn on_hub(input: &AppInput<'_>, event: &PanelEvent) -> PanelReaction {
    let (cid, code, v) = (event.id, event.code, &event.values);
    let hub_event = match (cid, code) {
        (id::DEVICE, notify::CHANGED) => chosen(v, id::DEVICE).map(HubEvent::DeviceChosen),
        (id::STOP, notify::CLICKED) => Some(HubEvent::StopUpload),
        (id::REPEAT, notify::CLICKED) => chosen(v, id::HISTORY).map(HubEvent::Repeat),
        (id::CONFIRM, notify::CLICKED) => Some(HubEvent::ConfirmRepeat),
        (id::CANCEL, notify::CLICKED) => Some(HubEvent::CancelRepeat),
        (id::REMOVE, notify::CLICKED) => chosen(v, id::PROOFS).map(HubEvent::RemoveProof),
        (id::RECORD, notify::CLICKED) => Some(HubEvent::RecordProof(ProofForm {
            kind: chosen(v, id::KIND).unwrap_or(0),
            material: v.text(id::MATERIAL).to_owned(),
            speed: v.text(id::SPEED).to_owned(),
            value: v.text(id::VALUE).to_owned(),
            value_y: v.text(id::VALUE_Y).to_owned(),
            method: v.text(id::METHOD).to_owned(),
            date: v.text(id::DATE).to_owned(),
        })),
        _ => None,
    };
    hub_event
        .and_then(|e| hub_intent(input.hub, input.document, e))
        .map_or(PanelReaction::Nothing, PanelReaction::Intent)
}
