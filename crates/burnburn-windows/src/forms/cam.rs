//! The CAM dialogs: Optimization Settings, Material / Interval / Focus Test and Device Settings.
//! One generic form per dialog, built from presentation's `DialogViewModel` exactly as the macOS
//! panel is: a row per field that is not hidden, the reason a field is greyed or its error under
//! it, arrows beside the `Order by` levels, the notes, the capability list, the test result and the
//! buttons the dialog has. Nothing is applied before Apply; a test dialog never touches the document.

use std::path::PathBuf;

use burnburn_presentation::{
    CamDialog, DialogActionKind, DialogControl, DialogField, DialogRow, DialogValue, DialogViewModel, FieldState,
};

use super::{
    ChoiceItem, ControlId, ControlValue, FilePick, Form, FormButton, FormContext, FormKind, FormLine, FormOutcome,
    Placement,
};
use crate::l10n::{Locale, message_or_key};
use crate::panels::{
    cam_action_intent, cam_action_title, cam_caption, cam_choice_label, cam_error, cam_field_intent, cam_label,
    cam_move_intent, cam_note, cam_result_text, cam_support_line, cam_title,
};

/// Buttons in the order of the bar, left to right.
const ACTION_KINDS: [DialogActionKind; 8] = [
    DialogActionKind::ShowSource,
    DialogActionKind::AddRow,
    DialogActionKind::RemoveRow,
    DialogActionKind::ReturnToFlatbed,
    DialogActionKind::Generate,
    DialogActionKind::ExportRd,
    DialogActionKind::Close,
    DialogActionKind::Apply,
];

fn choice_items(row: &DialogRow, locale: Locale) -> Vec<ChoiceItem> {
    // A pop-up whose entries depend on the document brings its own titles.
    if !row.labels.is_empty() {
        return row.labels.iter().map(|l| ChoiceItem::plain(cam_label(l, locale))).collect();
    }
    row.field
        .choices()
        .iter()
        .map(|choice| {
            let label = cam_choice_label(*choice, locale);
            match row.blocked_choices.iter().find(|(c, _)| c == choice) {
                Some((_, reason)) => ChoiceItem {
                    title: format!("{label} \u{2014} {}", message_or_key(*reason, locale)),
                    enabled: false,
                },
                None => ChoiceItem::plain(label),
            }
        })
        .collect()
}

fn arrow_title(up: bool) -> &'static str {
    if up { "\u{2191}" } else { "\u{2193}" }
}

pub fn form(ctx: &FormContext<'_>, dialog: CamDialog, model: &DialogViewModel) -> Option<Form> {
    let locale = ctx.locale;
    let mut lines = Vec::new();
    for row in &model.rows {
        let editable = match row.state {
            FieldState::Hidden => continue,
            FieldState::Editable => true,
            FieldState::Disabled(_) => false,
        };
        let id = ControlId::Cam(row.field);
        let caption = cam_caption(row.field, model, locale);
        lines.push(match (row.field.control(), &row.value) {
            (DialogControl::Text, value) => FormLine::Field {
                id,
                caption,
                text: value.text().to_owned(),
                enabled: editable,
                invalid: row.error.is_some(),
                hint: String::new(),
            },
            (DialogControl::Flag, value) => FormLine::Check { id, caption, on: value.flag(), enabled: editable },
            (DialogControl::Choice, value) => FormLine::Choice {
                id,
                caption,
                items: choice_items(row, locale),
                selected: if let DialogValue::Choice(i) = value { *i } else { 0 },
                enabled: editable,
            },
        });
        // The arrows beside an `Order by` level.
        if let (Some((can_up, can_down)), DialogField::OrderEnabled(key)) = (row.reorder, row.field) {
            lines.push(FormLine::Buttons(vec![
                FormButton::new(ControlId::CamMove(key, true), arrow_title(true), can_up),
                FormButton::new(ControlId::CamMove(key, false), arrow_title(false), can_down),
            ]));
        }
        // The error beats the reason a field is greyed; neither: nothing.
        match (row.error, row.state) {
            (Some(why), _) => lines.push(FormLine::Problem(cam_error(why, locale))),
            (None, FieldState::Disabled(Some(reason))) => lines.push(FormLine::Text(message_or_key(reason, locale))),
            _ => {}
        }
    }
    for note in &model.notes {
        let text = cam_note(*note, locale);
        if !text.is_empty() {
            lines.push(FormLine::Text(text));
        }
    }
    for line in &model.lines {
        lines.push(FormLine::Text(line.clone()));
    }
    for capability in &model.capabilities {
        lines.push(FormLine::Text(cam_support_line(capability, locale)));
    }
    match (&model.result, model.busy) {
        (Some(result), _) => lines.push(FormLine::Text(cam_result_text(result, locale))),
        (None, true) => lines.push(FormLine::Text(
            match locale {
                Locale::Nl => "Bezig met compileren\u{2026}",
                Locale::En => "Compiling\u{2026}",
            }
            .to_owned(),
        )),
        (None, false) => {}
    }
    let buttons: Vec<FormButton> = ACTION_KINDS
        .iter()
        .filter_map(|kind| {
            let state = model.action(*kind)?;
            Some(FormButton::new(ControlId::CamAction(*kind), cam_action_title(*kind, dialog, locale), state.enabled))
        })
        .collect();
    lines.push(FormLine::Buttons(buttons));
    Some(Form {
        kind: FormKind::Cam(dialog),
        title: cam_title(dialog, locale).to_owned(),
        placement: Placement::Floating,
        lines,
    })
}

pub fn react(
    dialog: CamDialog,
    model: Option<&DialogViewModel>,
    id: &ControlId,
    value: &ControlValue,
    ctx: &FormContext<'_>,
) -> FormOutcome {
    let document = ctx.document;
    let intent = match (id, value) {
        (ControlId::Cam(field), ControlValue::Text(t)) => {
            cam_field_intent(model, document, dialog, *field, DialogValue::Text(t.clone()))
        }
        (ControlId::Cam(field), ControlValue::Toggle(on)) => {
            cam_field_intent(model, document, dialog, *field, DialogValue::Flag(*on))
        }
        (ControlId::Cam(field), ControlValue::Choice(i)) => {
            cam_field_intent(model, document, dialog, *field, DialogValue::Choice(*i))
        }
        // Save RD needs a path from the save dialog first.
        (ControlId::CamAction(DialogActionKind::ExportRd), ControlValue::Press) => {
            let enabled = model.and_then(|m| m.action(DialogActionKind::ExportRd)).is_some_and(|a| a.enabled);
            return if enabled { FormOutcome::PickFile(FilePick::CamExportRd(dialog)) } else { FormOutcome::Nothing };
        }
        (ControlId::CamAction(kind), ControlValue::Press) => {
            model.and_then(|m| cam_action_intent(m, document, dialog, *kind, None))
        }
        (ControlId::CamMove(key, up), ControlValue::Press) => {
            model.and_then(|m| cam_move_intent(m, document, *key, *up))
        }
        _ => None,
    };
    intent.map_or(FormOutcome::Nothing, FormOutcome::Intent)
}

/// The path chosen for Save RD of a test dialog.
pub fn file_chosen(
    dialog: CamDialog,
    model: Option<&DialogViewModel>,
    path: PathBuf,
    ctx: &FormContext<'_>,
) -> FormOutcome {
    model
        .and_then(|m| cam_action_intent(m, ctx.document, dialog, DialogActionKind::ExportRd, Some(path)))
        .map_or(FormOutcome::Nothing, FormOutcome::Intent)
}
