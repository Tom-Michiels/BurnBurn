//! The golf-5 panels as forms (G5-native): project notes, offset and repair, centre-line decisions
//! and the structure panel. The lines, words and intents are the macOS panels' (`crate::panels::g5`,
//! compiled from the macOS source file); a control reports its value and `g5_intent` decides what it
//! means (fields as typed, a choice as its index, a check as `1` / `0`).

use super::{
    ChoiceItem, ControlId, ControlValue, Form, FormButton, FormContext, FormKind, FormLine, FormOutcome, FormValues,
    PanelState, Placement,
};
use crate::panels::{G5Control, G5Line, G5Panel, g5_intent, g5_opens_by_itself, g5_panel};

/// A button title this long gets a row of its own (two such buttons side by side would not fit).
const SHORT_BUTTON: usize = 22;

/// What was typed in the panel's controls, as the headless model reads it.
fn panel_values(values: &FormValues) -> Vec<(G5Control, String)> {
    values
        .iter()
        .filter_map(|(id, v)| match (id, v) {
            (ControlId::G5(c), ControlValue::Text(t)) => Some((*c, t.clone())),
            (ControlId::G5(c), ControlValue::Choice(i)) => Some((*c, i.to_string())),
            (ControlId::G5(c), ControlValue::Toggle(on)) => Some((*c, (if *on { "1" } else { "0" }).to_owned())),
            _ => None,
        })
        .collect()
}

/// The form of `panel`, or `None` when it has nothing to show or waits for its menu item.
fn form(ctx: &FormContext<'_>, state: &PanelState, values: &FormValues, panel: G5Panel) -> Option<Form> {
    if !(g5_opens_by_itself(panel) || state.g5_shown.contains(&panel)) {
        return None;
    }
    let model = g5_panel(ctx.vm, ctx.document, panel, ctx.locale, &panel_values(values))?;
    let mut lines: Vec<FormLine> = Vec::new();
    for line in model.lines {
        let id = |c: G5Control| ControlId::G5(c);
        match line {
            G5Line::Heading(t) => lines.push(FormLine::Heading(t)),
            G5Line::Text(t) => lines.push(FormLine::Text(t)),
            G5Line::Problem(t) => lines.push(FormLine::Problem(t)),
            G5Line::Memo { control, text, max_chars } => {
                lines.push(FormLine::Memo { id: id(control), text, max_chars })
            }
            G5Line::Field { control, caption, text } => lines.push(FormLine::Field {
                id: id(control),
                caption,
                text,
                enabled: true,
                invalid: false,
                hint: String::new(),
            }),
            G5Line::Choice { control, caption, options, selected } => lines.push(FormLine::Choice {
                id: id(control),
                caption,
                items: options.into_iter().map(ChoiceItem::plain).collect(),
                selected,
                enabled: true,
            }),
            G5Line::Check { control, caption, on } => {
                lines.push(FormLine::Check { id: id(control), caption, on, enabled: true })
            }
            G5Line::Image(image) => lines.push(FormLine::Image(image)),
            G5Line::Button { control, title, enabled } => {
                let short = title.chars().count() <= SHORT_BUTTON;
                let button = FormButton::new(id(control), title, enabled);
                // Neighbouring short buttons share a row (at most two: the panel is narrow).
                match lines.last_mut() {
                    Some(FormLine::Buttons(row))
                        if short && row.len() < 2 && row.iter().all(|b| b.title.chars().count() <= SHORT_BUTTON) =>
                    {
                        row.push(button)
                    }
                    _ => lines.push(FormLine::Buttons(vec![button])),
                }
            }
        }
    }
    Some(Form { kind: FormKind::G5(panel), title: model.title, placement: Placement::Floating, lines })
}

/// Every golf-5 form the window shows now: the notes while they are open, the centre-line decisions
/// while a centre-line trace is open, and the panels the menu asked for. `values` are the controls'
/// values now (the notes panel counts what is typed).
pub fn forms(ctx: &FormContext<'_>, state: &PanelState, values: &FormValues) -> Vec<Form> {
    G5Panel::ALL.iter().filter_map(|panel| form(ctx, state, values, *panel)).collect()
}

/// The intent (or follow-up) of a control event of panel `panel`.
pub fn react(panel: G5Panel, id: &ControlId, values: &FormValues, ctx: &FormContext<'_>) -> FormOutcome {
    let ControlId::G5(control) = id else { return FormOutcome::Nothing };
    g5_intent(ctx.vm, ctx.document, panel, *control, &panel_values(values))
        .map_or(FormOutcome::Nothing, FormOutcome::Intent)
}
