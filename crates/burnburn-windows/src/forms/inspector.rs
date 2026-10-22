//! The inspector: the nine transform fields (two per row), the 3x3 reference point with the aspect
//! lock, the flip buttons, the cut-order priority and the shape / tool parameter sections. Like
//! the macOS inspector it only shows the view model and forwards typed text unparsed (DR-05).

use burnburn_presentation::{FieldKey, MessageKey, UiIntent};
use burnburn_types::Unit;

use super::{
    ControlId, ControlValue, Form, FormButton, FormContext, FormKind, FormLine, FormOutcome, Placement, shapes,
};
use crate::adapter::message_text;
use crate::l10n::{HostText, host_text, message_or_key};
use crate::panels::{
    INSPECTOR_FLIP_BUTTONS, INSPECTOR_ROWS, cam_priority_hint, field_unit_for, flip_intent, inspector_controls,
    reference_point_intent,
};

/// The glyphs of the reference-point buttons, row by row (`RefPoint::ALL` order).
const REFERENCE_GLYPHS: [&str; 9] =
    ["\u{2196}", "\u{2191}", "\u{2197}", "\u{2190}", "\u{25CF}", "\u{2192}", "\u{2199}", "\u{2193}", "\u{2198}"];
/// The glyphs of the flip buttons (the command names are their menu items).
const FLIP_GLYPHS: [&str; 2] = ["\u{2194}", "\u{2195}"];

pub fn form(ctx: &FormContext<'_>) -> Option<Form> {
    let doc = ctx.vm.document(ctx.document)?;
    let locale = ctx.locale;
    let controls = inspector_controls(ctx.vm, ctx.document);
    let field_line = |key: FieldKey| {
        let model = doc.inspector.as_ref().and_then(|i| i.field(key));
        let unit = model.map_or(Unit::Millimeter, |f| f.unit);
        let caption =
            format!("{} ({})", message_or_key(MessageKey::Field(key), locale), field_unit_for(key, unit, locale));
        FormLine::Field {
            id: ControlId::Inspector(key),
            caption,
            text: model.map(|f| f.text.clone()).unwrap_or_default(),
            enabled: model.is_some_and(|f| f.editable),
            invalid: model.is_some_and(|f| f.error.is_some()),
            hint: String::new(),
        }
    };
    let mut lines = Vec::new();
    for (left, right) in INSPECTOR_ROWS {
        let mut row = vec![field_line(left)];
        match right {
            Some(right) => row.push(field_line(right)),
            // The last row has room beside Rotation: the two flip buttons.
            None => row.push(FormLine::Buttons(
                INSPECTOR_FLIP_BUTTONS
                    .iter()
                    .enumerate()
                    .map(|(n, _)| FormButton::new(ControlId::Flip(n), FLIP_GLYPHS[n], controls.flip_enabled[n]))
                    .collect(),
            )),
        }
        lines.push(FormLine::Row(row));
    }
    // The reference point (3x3) with the aspect lock beside it.
    lines.push(FormLine::Heading(host_text(HostText::ReferencePoint, locale).to_owned()));
    let grid = FormLine::Grid {
        columns: 3,
        buttons: burnburn_presentation::RefPoint::ALL
            .iter()
            .enumerate()
            .map(|(n, point)| {
                let mut b = FormButton::new(ControlId::RefPoint(n), REFERENCE_GLYPHS[n], controls.enabled);
                b.pressed = *point == controls.reference_point;
                b
            })
            .collect(),
    };
    lines.push(FormLine::Row(vec![
        grid,
        FormLine::Check {
            id: ControlId::AspectLock,
            caption: host_text(HostText::AspectLock, locale).to_owned(),
            on: controls.aspect_locked,
            enabled: controls.enabled,
        },
    ]));
    // The cut-order priority joins the field list, with its one line of advice.
    let mut priority = field_line(FieldKey::Priority);
    if let FormLine::Field { caption, hint, .. } = &mut priority {
        *caption = message_or_key(MessageKey::Field(FieldKey::Priority), locale);
        *hint = cam_priority_hint(locale).to_owned();
    }
    lines.push(priority);
    lines.push(FormLine::Text(crate::panels::cam_priority_hint_short(locale).to_owned()));
    lines.extend(shapes::param_lines(ctx));
    // The stack of the selection, under its own heading, while the selection belongs to a chain.
    if let Some(stack) = crate::panels::g5_panel(ctx.vm, ctx.document, crate::panels::G5Panel::Stack, locale, &[]) {
        lines.push(FormLine::Heading(stack.title));
        lines.extend(super::g5::form_lines(stack.lines));
    }
    // The first refused field says why, as the macOS status line does.
    if let Some(error) = doc.inspector.as_ref().and_then(|i| i.fields.iter().find_map(|f| f.error)) {
        lines.push(FormLine::Problem(message_text(error, locale)));
    }
    Some(Form {
        kind: FormKind::Inspector,
        title: host_text(HostText::InspectorLabel, locale).to_owned(),
        placement: Placement::DockRight,
        lines,
    })
}

pub fn react(id: &ControlId, value: &ControlValue, ctx: &FormContext<'_>) -> FormOutcome {
    let Some(doc) = ctx.vm.document(ctx.document) else { return FormOutcome::Nothing };
    let intent = match (id, value) {
        (ControlId::Inspector(field), ControlValue::Text(text)) => {
            crate::adapter::commit(doc, ctx.document, *field, text)
        }
        (ControlId::RefPoint(n), ControlValue::Press) => {
            reference_point_intent(ctx.document, isize::try_from(*n).unwrap_or(-1))
        }
        (ControlId::AspectLock, ControlValue::Toggle(on)) => {
            (*on != doc.aspect_locked).then_some(UiIntent::SetAspectLock { document: ctx.document, locked: *on })
        }
        (ControlId::Flip(n), ControlValue::Press) => flip_intent(ctx.document, isize::try_from(*n).unwrap_or(-1)),
        (ControlId::Param(section, slot), value) => shapes::param_intent_of(ctx, *section, *slot, value),
        _ => None,
    };
    intent.map_or(FormOutcome::Nothing, FormOutcome::Intent)
}
