//! The Layers panel: the 30-colour palette, one line per layer, the editors of the selected layer
//! (name, speed, power, passes, output, visible, Cut Settings) and the add row. Same rows, words and
//! intents as the macOS layer table; here the table cells are editors under the list, because a
//! list box has no editable cells.

use burnburn_presentation::{OperationField, UiIntent};

use super::{
    ChoiceItem, ControlId, ControlValue, Form, FormButton, FormContext, FormKind, FormLine, FormOutcome, FormValues,
    PanelState, Placement, choice_of, text_of,
};
use crate::l10n::{HostText, host_text};
use crate::panels::{
    LayerColumn, OPERATION_KINDS, OperationLine, operation_kind, operation_kind_name, operations_panel,
    palette_buttons, palette_slot,
};

/// Palette buttons per row.
const PALETTE_COLUMNS: usize = 10;

/// The list line of a layer: colour, name, mode, speed, power, passes, objects, and what is off.
fn summary(row: &OperationLine, locale: crate::l10n::Locale) -> String {
    let mark = |on: bool| if on { "\u{2713}" } else { "\u{2717}" };
    format!(
        "{}  {}  {}  {}  {}  \u{00D7}{}  {}  {} {}  {} {}",
        row.layer,
        row.name,
        row.kind,
        row.speed,
        row.power,
        row.passes,
        row.targets,
        LayerColumn::Output.caption(locale),
        mark(row.output),
        LayerColumn::Visible.caption(locale),
        mark(row.visible),
    )
}

pub fn form(ctx: &FormContext<'_>, state: &PanelState) -> Option<Form> {
    ctx.vm.document(ctx.document)?;
    let locale = ctx.locale;
    let t = |text: HostText| host_text(text, locale);
    let panel = operations_panel(ctx.vm, ctx.document, locale);
    let mut lines = vec![FormLine::Heading(t(HostText::PaletteLabel).to_owned())];
    lines.push(FormLine::Grid {
        columns: PALETTE_COLUMNS,
        buttons: palette_buttons(ctx.vm, ctx.document, locale)
            .into_iter()
            .map(|b| {
                let mut button = FormButton::new(ControlId::Palette(b.slot), b.label, true);
                button.pressed = b.used;
                button
            })
            .collect(),
    });
    lines.push(FormLine::Heading(t(HostText::OperationsTitle).to_owned()));
    if panel.rows.is_empty() {
        lines.push(FormLine::Text(panel.empty_text.clone()));
    }
    let selected_row = state.layer_row.filter(|r| *r < panel.rows.len());
    lines.push(FormLine::List {
        id: ControlId::LayerList,
        items: panel.rows.iter().map(|r| summary(r, locale)).collect(),
        selected: selected_row.into_iter().collect(),
        multi: false,
    });
    if let Some(row) = selected_row.and_then(|r| panel.rows.get(r)) {
        let caption = |c: LayerColumn| c.caption(locale).to_owned();
        let field = |c: LayerColumn| {
            let mut f = FormLine::field(ControlId::LayerField(c), caption(c), row.text(c));
            if let FormLine::Field { invalid, .. } = &mut f {
                *invalid = !panel.error.is_empty();
            }
            f
        };
        lines.push(FormLine::Row(vec![field(LayerColumn::Name), field(LayerColumn::Passes)]));
        lines.push(FormLine::Row(vec![field(LayerColumn::Speed), field(LayerColumn::Power)]));
        for c in [LayerColumn::Output, LayerColumn::Visible] {
            lines.push(FormLine::Check {
                id: ControlId::LayerField(c),
                caption: caption(c),
                on: row.checked(c).unwrap_or(false),
                enabled: true,
            });
        }
        lines.push(FormLine::Buttons(vec![FormButton::new(
            ControlId::LayerCutSettings,
            t(HostText::CutSettingsButton),
            true,
        )]));
    }
    if !panel.error.is_empty() {
        lines.push(FormLine::Problem(panel.error));
    }
    // The add row: a new layer for the selection with raw name, speed and power text.
    lines.push(FormLine::Heading(t(HostText::OperationAdd).to_owned()));
    lines.push(FormLine::field(ControlId::LayerAddName, t(HostText::OperationName), String::new()));
    lines.push(FormLine::Choice {
        id: ControlId::LayerAddKind,
        caption: t(HostText::OperationKind).to_owned(),
        items: OPERATION_KINDS.iter().map(|k| ChoiceItem::plain(operation_kind_name(*k, locale))).collect(),
        selected: 0,
        enabled: true,
    });
    lines.push(FormLine::Row(vec![
        FormLine::field(ControlId::LayerAddSpeed, t(HostText::OperationSpeed), String::new()),
        FormLine::field(ControlId::LayerAddPower, t(HostText::OperationPower), String::new()),
    ]));
    lines.push(FormLine::Buttons(vec![FormButton::new(ControlId::LayerAdd, t(HostText::OperationAdd), true)]));
    Some(Form {
        kind: FormKind::Layers,
        title: t(HostText::OperationsTitle).to_owned(),
        placement: Placement::DockLeft,
        lines,
    })
}

pub fn react(
    id: &ControlId,
    value: &ControlValue,
    values: &FormValues,
    ctx: &FormContext<'_>,
    state: &PanelState,
) -> FormOutcome {
    let Some(doc) = ctx.vm.document(ctx.document) else { return FormOutcome::Nothing };
    let document = ctx.document;
    let selected_op = || state.layer_row.and_then(|r| doc.operations.get(r));
    let intent = match (id, value) {
        (ControlId::Palette(slot), ControlValue::Press | ControlValue::ShiftPress) => {
            let add = matches!(value, ControlValue::ShiftPress);
            palette_slot(isize::from(*slot)).map(|slot| UiIntent::AssignLayer { document, slot, add })
        }
        (ControlId::LayerList, ControlValue::Rows(rows)) => {
            return FormOutcome::SelectLayerRow(rows.first().copied());
        }
        (ControlId::LayerList, ControlValue::Activate(row)) => {
            doc.operations.get(*row).map(|op| UiIntent::OpenCutSettings { document, operation: op.id })
        }
        (ControlId::LayerCutSettings, ControlValue::Press) => {
            selected_op().map(|op| UiIntent::OpenCutSettings { document, operation: op.id })
        }
        (ControlId::LayerField(column), ControlValue::Text(text)) => {
            let (op, field) = (selected_op(), column.field());
            match (op, field) {
                (Some(op), Some(field)) => {
                    let shown = match field {
                        OperationField::Name => &op.name,
                        OperationField::Speed => &op.speed,
                        OperationField::PowerMax => &op.power,
                        OperationField::Passes => &op.passes,
                        _ => return FormOutcome::Nothing,
                    };
                    // Return and the following focus loss are one commit; a retry after an error still goes.
                    (doc.operation_error.is_some() || shown != text).then(|| UiIntent::SetOperationField {
                        document,
                        operation: op.id,
                        field,
                        text: text.clone(),
                    })
                }
                _ => None,
            }
        }
        (ControlId::LayerField(column), ControlValue::Toggle(on)) => selected_op().and_then(|op| match column {
            LayerColumn::Output if op.output_enabled != *on => {
                Some(UiIntent::SetOperationOutput { document, operation: op.id, enabled: *on })
            }
            LayerColumn::Visible if op.visible != *on => {
                Some(UiIntent::SetOperationVisible { document, operation: op.id, visible: *on })
            }
            _ => None,
        }),
        (ControlId::LayerAdd, ControlValue::Press) => {
            let kind = operation_kind(choice_of(values, &ControlId::LayerAddKind).map_or(0, |i| i as isize));
            kind.map(|kind| UiIntent::AddOperationText {
                document,
                name: text_of(values, &ControlId::LayerAddName),
                kind,
                speed: text_of(values, &ControlId::LayerAddSpeed),
                power: text_of(values, &ControlId::LayerAddPower),
            })
        }
        _ => None,
    };
    intent.map_or(FormOutcome::Nothing, FormOutcome::Intent)
}
