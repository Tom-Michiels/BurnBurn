//! The Schikken panel: one floating form that shows the flow in progress (Resize Slots, nest,
//! Print and Cut, Center Finder, Warp / Deform, Tangent Circle) or else the Dock / Move Together
//! options. The lines come from the macOS panel model (`crate::panels::schikken_panel`); a control
//! reports its value and `schikken_intent` decides what it means (fields as typed, a choice as its
//! index, a check as `1` / `0`).

use super::{
    ChoiceItem, ControlId, ControlValue, Form, FormButton, FormContext, FormKind, FormLine, FormOutcome, FormValues,
    PanelState, Placement,
};
use crate::panels::{SchikkenControl, SchikkenLine, schikken_intent, schikken_panel};

/// The panel shows while a flow is in progress, or when the menu asked for it.
pub fn form(ctx: &FormContext<'_>, state: &PanelState) -> Option<Form> {
    let panel = schikken_panel(ctx.vm, ctx.document, ctx.locale)?;
    if !(panel.flow || state.schikken_shown) {
        return None;
    }
    let mut lines: Vec<FormLine> = Vec::new();
    for line in panel.lines {
        let id = |c: SchikkenControl| ControlId::Schikken(c);
        match line {
            SchikkenLine::Heading(t) => lines.push(FormLine::Heading(t)),
            SchikkenLine::Text(t) => lines.push(FormLine::Text(t)),
            SchikkenLine::Problem(t) => lines.push(FormLine::Problem(t)),
            SchikkenLine::Field { control, caption, text, invalid } => lines.push(FormLine::Field {
                id: id(control),
                caption,
                text,
                enabled: true,
                invalid,
                hint: String::new(),
            }),
            SchikkenLine::Choice { control, caption, options, selected } => lines.push(FormLine::Choice {
                id: id(control),
                caption,
                items: options.into_iter().map(ChoiceItem::plain).collect(),
                selected,
                enabled: true,
            }),
            SchikkenLine::Check { control, caption, on } => {
                lines.push(FormLine::Check { id: id(control), caption, on, enabled: true })
            }
            SchikkenLine::Button { control, title, enabled } => {
                let button = FormButton::new(id(control), title, enabled);
                // Neighbouring buttons share a row.
                match lines.last_mut() {
                    Some(FormLine::Buttons(row)) => row.push(button),
                    _ => lines.push(FormLine::Buttons(vec![button])),
                }
            }
        }
    }
    Some(Form { kind: FormKind::Schikken, title: panel.title, placement: Placement::Floating, lines })
}

/// The panel's values as `schikken_intent` reads them.
fn panel_values(values: &FormValues) -> Vec<(SchikkenControl, String)> {
    values
        .iter()
        .filter_map(|(id, v)| match (id, v) {
            (ControlId::Schikken(c), ControlValue::Text(t)) => Some((*c, t.clone())),
            (ControlId::Schikken(c), ControlValue::Choice(i)) => Some((*c, i.to_string())),
            (ControlId::Schikken(c), ControlValue::Toggle(on)) => Some((*c, (if *on { "1" } else { "0" }).to_owned())),
            _ => None,
        })
        .collect()
}

pub fn react(id: &ControlId, _value: &ControlValue, values: &FormValues, ctx: &FormContext<'_>) -> FormOutcome {
    let ControlId::Schikken(control) = id else { return FormOutcome::Nothing };
    schikken_intent(ctx.vm, ctx.document, *control, &panel_values(values))
        .map_or(FormOutcome::Nothing, FormOutcome::Intent)
}
