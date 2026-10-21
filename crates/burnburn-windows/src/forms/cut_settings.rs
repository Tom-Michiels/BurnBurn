//! The Cut Settings sheet of one layer: mode, speed, power, passes, kerf, output and visible, and
//! the option rows of the chosen mode (tabs, leads, overcut, perforation, dot, fill, cross-hatch,
//! scan angle, image, Z offset). The sheet's draft is the controls' values (`live`): a change of a
//! mode, a switch or a pop-up builds the sheet again from them, so the rows that exist and the rows
//! that are greyed follow what is on the sheet, as on the macOS sheet. OK sends every raw value in
//! one `ApplyCutSettings`.

use burnburn_presentation::{CutExtras, CutSettingsViewModel, CutValue, OperationKind, UiIntent};
use burnburn_types::OperationId;

use super::{
    ChoiceItem, ControlId, ControlValue, Form, FormButton, FormContext, FormKind, FormLine, FormOutcome, FormValues,
    Placement, choice_of, value_of,
};
use crate::l10n::{HostText, host_text};
use crate::panels::{
    ModeItem, OptionRow, RowControl, confirmed_extras, cut_settings_panel, option_caption, option_rows, sheet_mode_of,
};

/// The operation whose sheet is open, if any (the host starts a fresh draft when it changes).
pub fn open_operation(ctx: &FormContext<'_>) -> Option<OperationId> {
    ctx.vm.document(ctx.document)?.cut_settings.as_ref().map(|c| c.operation)
}

fn sheet<'a>(ctx: &'a FormContext<'_>) -> Option<&'a CutSettingsViewModel> {
    ctx.vm.document(ctx.document)?.cut_settings.as_ref()
}

/// The text of control `id` on the sheet now, else the stored `default`.
fn text_or(live: &FormValues, id: &ControlId, default: &str) -> String {
    match value_of(live, id) {
        Some(ControlValue::Text(t)) => t.clone(),
        _ => default.to_owned(),
    }
}

fn flag_or(live: &FormValues, id: &ControlId, default: bool) -> bool {
    match value_of(live, id) {
        Some(ControlValue::Toggle(on)) => *on,
        _ => default,
    }
}

/// The option values on the sheet now: the stored ones, replaced by what the controls hold.
fn live_extras(sheet: &CutSettingsViewModel, live: &FormValues) -> CutExtras {
    let mut extras = sheet.extras.clone();
    for (id, value) in live {
        if let ControlId::CutOption(field) = id {
            match value {
                ControlValue::Text(t) => extras.set(*field, CutValue::Text(t.clone())),
                ControlValue::Toggle(on) => extras.set(*field, CutValue::Flag(*on)),
                ControlValue::Choice(i) => extras.set(*field, CutValue::Choice(*i)),
                _ => {}
            }
        }
    }
    extras
}

fn items(modes: &[ModeItem]) -> Vec<ChoiceItem> {
    modes.iter().map(|m| ChoiceItem { title: m.title.clone(), enabled: m.enabled }).collect()
}

/// The mode shown by the sheet now.
fn live_mode(sheet: &CutSettingsViewModel, live: &FormValues) -> OperationKind {
    match choice_of(live, &ControlId::CutMode) {
        Some(i) => sheet_mode_of(sheet, i as isize),
        None => sheet.mode,
    }
}

pub fn form(ctx: &FormContext<'_>, live: &FormValues) -> Option<Form> {
    let locale = ctx.locale;
    let sheet = sheet(ctx)?;
    let panel = cut_settings_panel(ctx.vm, ctx.document, locale)?;
    let t = |text: HostText| host_text(text, locale).to_owned();
    let kind = live_mode(sheet, live);
    let invalid = !panel.error.is_empty();
    let text_field = |id: ControlId, caption: HostText, stored: &str, enabled: bool| FormLine::Field {
        id: id.clone(),
        caption: t(caption),
        text: text_or(live, &id, stored),
        enabled,
        invalid,
        hint: String::new(),
    };
    let mut lines = vec![FormLine::Choice {
        id: ControlId::CutMode,
        caption: t(HostText::CutSettingsMode),
        items: items(&panel.modes),
        selected: choice_of(live, &ControlId::CutMode).or(panel.mode_index).unwrap_or(0),
        enabled: true,
    }];
    lines.push(text_field(ControlId::CutSpeed, HostText::CutSettingsSpeed, &panel.speed, true));
    lines.push(FormLine::Row(vec![
        text_field(ControlId::CutPowerMin, HostText::CutSettingsPowerMin, &panel.power_min, true),
        text_field(ControlId::CutPowerMax, HostText::CutSettingsPowerMax, &panel.power_max, true),
    ]));
    lines.push(FormLine::Row(vec![
        text_field(ControlId::CutPasses, HostText::CutSettingsPasses, &panel.passes, true),
        text_field(ControlId::CutKerf, HostText::CutSettingsKerf, &panel.kerf, kind == OperationKind::Line),
    ]));
    for (id, caption, stored) in [
        (ControlId::CutOutput, HostText::CutSettingsOutput, panel.output),
        (ControlId::CutVisible, HostText::CutSettingsVisible, panel.visible),
    ] {
        lines.push(FormLine::Check { on: flag_or(live, &id, stored), id, caption: t(caption), enabled: true });
    }
    if invalid {
        lines.push(FormLine::Problem(panel.error.clone()));
    }
    let rows: Vec<OptionRow> = option_rows(sheet, kind, &live_extras(sheet, live), locale);
    if !rows.is_empty() {
        lines.push(FormLine::Heading(t(HostText::CutSettingsOptions)));
    }
    for row in rows {
        let id = ControlId::CutOption(row.field);
        let caption = option_caption(row.field, locale).to_owned();
        lines.push(match row.control {
            RowControl::Text(text) => {
                FormLine::Field { id, caption, text, enabled: row.enabled, invalid: false, hint: row.note.clone() }
            }
            RowControl::Check(on) => FormLine::Check { id, caption, on, enabled: row.enabled },
            RowControl::Popup { items: popup, selected } => {
                FormLine::Choice { id, caption, items: items(&popup), selected, enabled: row.enabled }
            }
        });
        if !row.note.is_empty() {
            lines.push(FormLine::Text(row.note));
        }
    }
    lines.push(FormLine::Buttons(vec![
        FormButton::new(ControlId::CutCancel, t(HostText::CutSettingsCancel), true),
        FormButton::new(ControlId::CutOk, t(HostText::CutSettingsOk), true),
    ]));
    Some(Form { kind: FormKind::CutSettings, title: panel.title, placement: Placement::Floating, lines })
}

pub fn react(id: &ControlId, value: &ControlValue, values: &FormValues, ctx: &FormContext<'_>) -> FormOutcome {
    let Some(sheet) = sheet(ctx) else { return FormOutcome::Nothing };
    let document = ctx.document;
    match (id, value) {
        (ControlId::CutOk, ControlValue::Press) => {
            let text = |id: ControlId, stored: &str| text_or(values, &id, stored);
            let mode = match choice_of(values, &ControlId::CutMode) {
                Some(i) => sheet_mode_of(sheet, i as isize),
                None => sheet.mode,
            };
            let options = values
                .iter()
                .filter_map(|(id, v)| match (id, v) {
                    (ControlId::CutOption(field), ControlValue::Text(t)) => Some((*field, CutValue::Text(t.clone()))),
                    (ControlId::CutOption(field), ControlValue::Toggle(on)) => Some((*field, CutValue::Flag(*on))),
                    (ControlId::CutOption(field), ControlValue::Choice(i)) => Some((*field, CutValue::Choice(*i))),
                    _ => None,
                })
                .collect();
            FormOutcome::Intent(UiIntent::ApplyCutSettings {
                document,
                operation: sheet.operation,
                settings: burnburn_presentation::CutSettingsInput {
                    mode,
                    speed: text(ControlId::CutSpeed, &sheet.speed),
                    power_min: text(ControlId::CutPowerMin, &sheet.power_min),
                    power_max: text(ControlId::CutPowerMax, &sheet.power_max),
                    passes: text(ControlId::CutPasses, &sheet.passes),
                    kerf: text(ControlId::CutKerf, &sheet.kerf),
                    output_enabled: flag_or(values, &ControlId::CutOutput, sheet.output_enabled),
                    visible: flag_or(values, &ControlId::CutVisible, sheet.visible),
                    extras: confirmed_extras(options),
                },
            })
        }
        (ControlId::CutCancel, ControlValue::Press) => FormOutcome::Intent(UiIntent::CancelCutSettings { document }),
        // A mode, a switch or a pop-up changes which rows exist and which are greyed.
        (ControlId::CutMode, ControlValue::Choice(_)) => FormOutcome::Redraw,
        (ControlId::CutOption(_), ControlValue::Toggle(_) | ControlValue::Choice(_)) => FormOutcome::Redraw,
        _ => FormOutcome::Nothing,
    }
}
