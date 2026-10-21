//! The geometry value dialog (join, round corners, optimize) and the shape / tool parameter rows
//! of the inspector (W2-L1-tekenen on the Mac; same rows, texts and intents here).

use super::{
    ControlId, ControlValue, Form, FormButton, FormContext, FormKind, FormLine, FormOutcome, FormValues, Placement,
    text_of,
};
use crate::l10n::{HostText, host_text};
use crate::panels::{
    ParamSection, geometry_dialog_cancel_intent, geometry_dialog_confirm_intent, geometry_dialog_panel, param_intent,
    shapes_panel,
};
use burnburn_presentation::UiIntent;

/// The parameter rows of the selected basic shape and of the active tool, as inspector lines: a
/// heading per section that has rows, a field per number, a check per on/off option.
pub(super) fn param_lines(ctx: &FormContext<'_>) -> Vec<FormLine> {
    let panel = shapes_panel(ctx.vm, ctx.document, ctx.locale);
    let mut lines = Vec::new();
    for (section, title, rows) in
        [(ParamSection::Shape, &panel.shape_title, &panel.shape), (ParamSection::Tool, &panel.tool_title, &panel.tool)]
    {
        if rows.is_empty() {
            continue;
        }
        lines.push(FormLine::Heading(title.clone()));
        for (slot, row) in rows.iter().enumerate() {
            let id = ControlId::Param(section, slot);
            if row.toggle {
                lines.push(FormLine::Check { id, caption: row.label.clone(), on: row.text == "1", enabled: true });
            } else {
                lines.push(FormLine::Field {
                    id,
                    caption: row.label.clone(),
                    text: row.text.clone(),
                    enabled: true,
                    invalid: !row.error.is_empty(),
                    hint: String::new(),
                });
            }
            if !row.error.is_empty() {
                lines.push(FormLine::Problem(row.error.clone()));
            }
        }
    }
    lines
}

/// The intent of a parameter row: text goes on as typed, a check as `1` or `0`.
pub(super) fn param_intent_of(
    ctx: &FormContext<'_>,
    section: ParamSection,
    slot: usize,
    value: &ControlValue,
) -> Option<UiIntent> {
    let text = match value {
        ControlValue::Text(t) => t.clone(),
        ControlValue::Toggle(on) => (if *on { "1" } else { "0" }).to_owned(),
        _ => return None,
    };
    param_intent(ctx.vm, ctx.document, section, slot, &text)
}

pub fn dialog_form(ctx: &FormContext<'_>) -> Option<Form> {
    let panel = geometry_dialog_panel(ctx.vm, ctx.document, ctx.locale)?;
    let t = |text: HostText| host_text(text, ctx.locale);
    let mut lines = vec![FormLine::field(ControlId::GeometryValue, panel.label, panel.text)];
    if let Some(FormLine::Field { invalid, .. }) = lines.first_mut() {
        *invalid = !panel.error.is_empty();
    }
    if !panel.error.is_empty() {
        lines.push(FormLine::Problem(panel.error));
    }
    lines.push(FormLine::Buttons(vec![
        FormButton::new(ControlId::GeometryCancel, t(HostText::GeometryCancel), true),
        FormButton::new(ControlId::GeometryOk, t(HostText::GeometryOk), true),
    ]));
    Some(Form { kind: FormKind::GeometryDialog, title: panel.title, placement: Placement::Floating, lines })
}

pub fn dialog_react(id: &ControlId, value: &ControlValue, values: &FormValues, ctx: &FormContext<'_>) -> FormOutcome {
    let intent = match (id, value) {
        (ControlId::GeometryOk, ControlValue::Press) => {
            geometry_dialog_confirm_intent(ctx.vm, ctx.document, &text_of(values, &ControlId::GeometryValue))
        }
        (ControlId::GeometryCancel, ControlValue::Press) => geometry_dialog_cancel_intent(ctx.vm, ctx.document),
        _ => None,
    };
    intent.map_or(FormOutcome::Nothing, FormOutcome::Intent)
}
