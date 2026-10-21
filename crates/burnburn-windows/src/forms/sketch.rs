//! The sketch panel: the constraint list of the open sketch (multi-select), the value editor of
//! the selected dimension, Measure/Drive, Remove, the degrees of freedom or the conflict in words,
//! and the parameters. Like the macOS panel it shows the view model and forwards typed text
//! unparsed (DR-05); the list is a list box, so the value cell of the table is a field below it.

use burnburn_presentation::{CommandKey, UiIntent};

use super::{
    ControlId, ControlValue, Form, FormButton, FormContext, FormKind, FormLine, FormOutcome, FormValues, Placement,
    text_of,
};
use crate::l10n::{HostText, host_text};
use crate::panels::{
    SketchColumn, SketchLine, sketch_panel, sketch_parameter_intent, sketch_problem_intent, sketch_role_intent,
    sketch_rows_intent, sketch_value_intent,
};

fn summary(row: &SketchLine, locale: crate::l10n::Locale) -> String {
    let _ = locale;
    let mut parts = vec![row.text(SketchColumn::Kind), row.text(SketchColumn::Entities)];
    if !row.value.is_empty() {
        parts.push(if row.driven { format!("({})", row.value) } else { row.value.clone() });
    }
    parts.push(row.text(SketchColumn::Status));
    parts.join("  |  ")
}

pub fn form(ctx: &FormContext<'_>) -> Option<Form> {
    let locale = ctx.locale;
    let t = |text: HostText| host_text(text, locale).to_owned();
    let p = sketch_panel(ctx.vm, ctx.document, locale)?;
    let mut lines = Vec::new();
    if !p.freedom.is_empty() {
        lines.push(FormLine::Text(p.freedom.clone()));
    }
    if !p.problem.is_empty() {
        lines.push(FormLine::Buttons(vec![FormButton::new(ControlId::SketchProblem, p.problem.clone(), true)]));
        lines.push(FormLine::Text(p.problem_hint.clone()));
    }
    if !p.refused.is_empty() {
        lines.push(FormLine::Problem(p.refused.clone()));
    }
    if p.rows.is_empty() {
        lines.push(FormLine::Text(p.empty_text.clone()));
    }
    lines.push(FormLine::List {
        id: ControlId::SketchList,
        items: p.rows.iter().map(|r| summary(r, locale)).collect(),
        selected: p.rows.iter().enumerate().filter(|(_, r)| r.selected).map(|(i, _)| i).collect(),
        multi: true,
    });
    // The value cell of the selected dimension.
    let editing = p.rows.iter().find(|r| r.selected && r.editable);
    lines.push(FormLine::Field {
        id: ControlId::SketchValue,
        caption: t(HostText::SketchColumnValue),
        text: editing.map(|r| r.value.clone()).unwrap_or_default(),
        enabled: editing.is_some(),
        invalid: false,
        hint: String::new(),
    });
    lines.push(FormLine::Buttons(vec![
        FormButton::new(ControlId::SketchRole, t(HostText::SketchToggleRole), p.role_enabled),
        FormButton::new(ControlId::SketchRemove, t(HostText::SketchRemove), p.remove_enabled),
    ]));
    lines.push(FormLine::Heading(t(HostText::SketchParametersTitle)));
    for (n, (name, text)) in p.parameters.iter().enumerate() {
        lines.push(FormLine::field(ControlId::SketchParam(n), name.clone(), text.clone()));
    }
    lines.push(FormLine::Row(vec![
        FormLine::field(ControlId::SketchParamName, t(HostText::SketchParameterName), String::new()),
        FormLine::field(ControlId::SketchParamText, t(HostText::SketchParameterValue), String::new()),
    ]));
    lines.push(FormLine::Buttons(vec![FormButton::new(
        ControlId::SketchParamSet,
        t(HostText::SketchParameterSet),
        true,
    )]));
    lines.push(FormLine::Text(p.hint.clone()));
    Some(Form { kind: FormKind::Sketch, title: p.title, placement: Placement::Floating, lines })
}

pub fn react(id: &ControlId, value: &ControlValue, values: &FormValues, ctx: &FormContext<'_>) -> FormOutcome {
    let (vm, document) = (ctx.vm, ctx.document);
    let Some(mode) = vm.document(document).and_then(|d| d.sketch_mode.as_ref()) else { return FormOutcome::Nothing };
    let intent: Option<UiIntent> = match (id, value) {
        (ControlId::SketchList, ControlValue::Rows(rows)) => sketch_rows_intent(vm, document, rows),
        (ControlId::SketchValue, ControlValue::Text(text)) => mode
            .rows
            .iter()
            .position(|r| r.selected && r.text.is_some())
            .and_then(|row| sketch_value_intent(vm, document, row, text)),
        (ControlId::SketchRole, ControlValue::Press) => sketch_role_intent(vm, document),
        (ControlId::SketchProblem, ControlValue::Press) => sketch_problem_intent(vm, document),
        (ControlId::SketchRemove, ControlValue::Press) => mode
            .selected_constraints
            .first()
            .map(|_| UiIntent::Invoke { document: Some(document), command: CommandKey::SketchRemoveConstraint }),
        (ControlId::SketchParamSet, ControlValue::Press) => sketch_parameter_intent(
            vm,
            document,
            &text_of(values, &ControlId::SketchParamName),
            &text_of(values, &ControlId::SketchParamText),
        ),
        (ControlId::SketchParam(n), ControlValue::Text(text)) => {
            mode.parameters.get(*n).and_then(|(name, _)| sketch_parameter_intent(vm, document, name, text))
        }
        _ => None,
    };
    intent.map_or(FormOutcome::Nothing, FormOutcome::Intent)
}
