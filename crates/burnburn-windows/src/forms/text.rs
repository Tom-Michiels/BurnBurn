//! The text panels: the text toolbar, the text-on-path sheet, the variable-text panel and the
//! barcode dialog as floating forms. What they show is the macOS panel model
//! (`crate::panels::text_panels`); a control reports a [`Slot`] with a value and
//! `text_panel_intent` decides whether that is a change (numbers go on as typed, DR-05).

use super::{
    ChoiceItem, ControlId, ControlValue, FilePick, Form, FormButton, FormContext, FormKind, FormLine, FormOutcome,
    Placement,
};
use crate::l10n::Locale;
use crate::panels::{
    BarcodePanel, PathPanel, Slot, SlotKind, SlotValue, TextText, ToolbarPanel, VariablePanel, text_panel_intent,
    text_panels, text_text,
};
use burnburn_presentation::UiIntent;

fn id(slot: Slot) -> ControlId {
    ControlId::Text(slot)
}

fn field(slot: Slot, caption: &str, text: &str) -> FormLine {
    FormLine::field(id(slot), caption, text)
}

fn choice(slot: Slot, caption: &str, items: Vec<ChoiceItem>, selected: usize) -> FormLine {
    FormLine::Choice { id: id(slot), caption: caption.to_owned(), items, selected, enabled: true }
}

fn check(slot: Slot, caption: &str, on: bool, enabled: bool) -> FormLine {
    FormLine::Check { id: id(slot), caption: caption.to_owned(), on, enabled }
}

fn plain(titles: &[&str]) -> Vec<ChoiceItem> {
    titles.iter().map(|t| ChoiceItem::plain(*t)).collect()
}

fn form_of(kind: FormKind, title: &str, lines: Vec<FormLine>) -> Form {
    Form { kind, title: title.to_owned(), placement: Placement::Floating, lines }
}

fn toolbar(p: &ToolbarPanel, locale: Locale) -> Form {
    let t = |x| text_text(x, locale);
    let mut lines = vec![choice(Slot::Family, t(TextText::Font), plain(&p.families.iter().map(String::as_str).collect::<Vec<_>>()), p.family)];
    lines.push(FormLine::Row(vec![
        check(Slot::Bold, t(TextText::Bold), p.bold.on, p.bold.enabled),
        check(Slot::Italic, t(TextText::Italic), p.italic.on, p.italic.enabled),
    ]));
    // A greyed style says why.
    for tip in [&p.bold.tip, &p.italic.tip, &p.welded.tip] {
        if !tip.is_empty() {
            lines.push(FormLine::Text(tip.clone()));
        }
    }
    lines.push(FormLine::Row(vec![
        check(Slot::Uppercase, t(TextText::Uppercase), p.uppercase, true),
        check(Slot::Welded, t(TextText::Welded), p.welded.on, p.welded.enabled),
    ]));
    lines.push(check(Slot::SingleLine, t(TextText::SingleLine), p.single_line, true));
    lines.push(choice(
        Slot::Align,
        t(TextText::Alignment),
        plain(&[t(TextText::AlignLeft), t(TextText::AlignCenter), t(TextText::AlignRight)]),
        p.align,
    ));
    lines.push(choice(
        Slot::VAlign,
        t(TextText::Vertical),
        plain(&[t(TextText::VBaseline), t(TextText::VTop), t(TextText::VMiddle), t(TextText::VBottom)]),
        p.valign,
    ));
    lines.push(field(Slot::HSpace, t(TextText::HSpace), &p.h_space));
    lines.push(field(Slot::VSpace, t(TextText::VSpace), &p.v_space));
    lines.push(field(Slot::Height, t(TextText::Height), &p.height));
    if !p.note.is_empty() {
        lines.push(FormLine::Problem(p.note.clone()));
    }
    lines.push(FormLine::Buttons(vec![FormButton::new(id(Slot::Convert), t(TextText::Convert), p.convert_enabled)]));
    form_of(FormKind::TextToolbar, t(TextText::ToolbarTitle), lines)
}

fn path(p: &PathPanel, locale: Locale) -> Form {
    let t = |x| text_text(x, locale);
    let mut lines = vec![
        choice(Slot::PathDistort, t(TextText::Distort), plain(&[t(TextText::DistortRigid), t(TextText::DistortWarp)]), p.distort),
        field(Slot::PathStart, t(TextText::PathStart), &p.start),
        field(Slot::PathOffset, t(TextText::PathOffset), &p.offset),
        check(Slot::PathReverse, t(TextText::PathReverse), p.reverse, true),
    ];
    if !p.note.is_empty() {
        lines.push(FormLine::Problem(p.note.clone()));
    }
    lines.push(FormLine::Buttons(vec![
        FormButton::new(id(Slot::PathCancel), t(TextText::Cancel), true),
        FormButton::new(id(Slot::PathApply), t(TextText::Apply), p.apply_enabled),
    ]));
    form_of(FormKind::TextPath, t(TextText::PathTitle), lines)
}

fn variable(p: &VariablePanel, locale: Locale) -> Form {
    let t = |x| text_text(x, locale);
    let mut lines = vec![
        choice(Slot::VarKind, t(TextText::Kind), plain(&[t(TextText::KindNormal), t(TextText::KindVariable)]), usize::from(p.variable)),
        field(Slot::VarOffset, t(TextText::VarOffset), &p.offset),
        FormLine::Row(vec![
            field(Slot::SerialCurrent, t(TextText::SerialCurrent), &p.current),
            field(Slot::SerialStart, t(TextText::SerialStart), &p.start),
        ]),
        FormLine::Row(vec![
            field(Slot::SerialEnd, t(TextText::SerialEnd), &p.end),
            field(Slot::SerialBy, t(TextText::SerialBy), &p.advance_by),
        ]),
        field(Slot::SerialDigits, t(TextText::SerialDigits), &p.digits),
        check(Slot::SerialWrap, t(TextText::SerialWrap), p.wrap, true),
        choice(
            Slot::VarPolicy,
            t(TextText::AutoAdvance),
            plain(&[t(TextText::PolicyManual), t(TextText::PolicyUpload), t(TextText::PolicyStart)]),
            p.policy,
        ),
        FormLine::Buttons(vec![
            FormButton::new(id(Slot::VarPrevious), t(TextText::Previous), true),
            FormButton::new(id(Slot::VarNext), t(TextText::Next), true),
            FormButton::new(id(Slot::VarReset), t(TextText::Reset), true),
        ]),
        FormLine::Buttons(vec![
            FormButton::new(id(Slot::VarCsv), t(TextText::ChooseCsv), true),
            FormButton::new(id(Slot::VarClearCsv), t(TextText::ClearCsv), p.csv_loaded),
        ]),
    ];
    if !p.csv_line.is_empty() {
        lines.push(FormLine::Text(p.csv_line.clone()));
    }
    if p.csv_loaded {
        lines.push(field(Slot::VarRow, t(TextText::Row), &p.row));
    }
    if !p.preview.is_empty() {
        let shown = format!("{}: {}", t(TextText::Preview), p.preview);
        lines.push(if p.preview_is_error { FormLine::Problem(p.preview.clone()) } else { FormLine::Text(shown) });
    }
    if !p.test.is_empty() {
        lines.push(FormLine::Text(p.test.clone()));
    }
    if !p.problem.is_empty() {
        lines.push(FormLine::Problem(p.problem.clone()));
    }
    if !p.bound.is_empty() {
        lines.push(FormLine::Text(p.bound.clone()));
    }
    lines.push(FormLine::Buttons(vec![
        FormButton::new(id(Slot::VarTest), t(TextText::Test), true),
        FormButton::new(id(Slot::VarBake), t(TextText::Bake), true),
    ]));
    form_of(FormKind::Variable, t(TextText::VariableTitle), lines)
}

fn barcode(p: &BarcodePanel, locale: Locale) -> Form {
    let t = |x| text_text(x, locale);
    let types = p.types.iter().map(|i| ChoiceItem { title: i.title.clone(), enabled: i.enabled }).collect();
    let mut lines = vec![choice(Slot::BcType, t(TextText::BarcodeType), types, p.symbology)];
    lines.push(field(Slot::BcPayload, t(TextText::Payload), &p.payload));
    if !p.payload_hint.is_empty() {
        lines.push(FormLine::Text(p.payload_hint.clone()));
    }
    lines.push(FormLine::Row(vec![
        field(Slot::BcModule, t(TextText::Module), &p.module),
        field(Slot::BcMinModule, t(TextText::MinModule), &p.min_module),
    ]));
    lines.push(field(Slot::BcQuiet, t(TextText::Quiet), &p.quiet));
    if !p.quiet_hint.is_empty() {
        lines.push(FormLine::Text(p.quiet_hint.clone()));
    }
    if let Some(height) = &p.height {
        lines.push(field(Slot::BcHeight, t(TextText::BarHeight), height));
    }
    if let Some((steps, selected)) = &p.ecc {
        lines.push(choice(Slot::BcEcc, t(TextText::Ecc), plain(&steps.iter().map(String::as_str).collect::<Vec<_>>()), *selected));
    }
    if let Some((modes, selected)) = &p.input_modes {
        lines.push(choice(Slot::BcInputMode, t(TextText::InputMode), plain(&modes.iter().map(String::as_str).collect::<Vec<_>>()), *selected));
    }
    if let Some(columns) = &p.columns {
        lines.push(field(Slot::BcColumns, t(TextText::Columns), columns));
    }
    lines.push(field(Slot::BcWidth, t(TextText::Width), &p.width));
    for text in [&p.size_line, &p.canonical_line, &p.readability] {
        if !text.is_empty() {
            lines.push(FormLine::Text(text.clone()));
        }
    }
    if !p.problems.is_empty() {
        lines.push(FormLine::Problem(p.problems.clone()));
    }
    lines.push(FormLine::Buttons(vec![
        FormButton::new(id(Slot::BcCancel), t(TextText::Cancel), true),
        FormButton::new(id(Slot::BcCreate), t(TextText::Create), p.create_enabled),
    ]));
    form_of(FormKind::Barcode, t(TextText::BarcodeTitle), lines)
}

/// The text panels that are open now, in the order of the macOS window.
pub fn forms(ctx: &FormContext<'_>) -> Vec<Form> {
    let panels = text_panels(ctx.vm, ctx.document, ctx.locale);
    let mut out = Vec::new();
    out.extend(panels.toolbar.as_ref().map(|p| toolbar(p, ctx.locale)));
    out.extend(panels.path.as_ref().map(|p| path(p, ctx.locale)));
    out.extend(panels.variable.as_ref().map(|p| variable(p, ctx.locale)));
    out.extend(panels.barcode.as_ref().map(|p| barcode(p, ctx.locale)));
    out
}

pub fn react(control: &ControlId, value: &ControlValue, ctx: &FormContext<'_>) -> FormOutcome {
    let ControlId::Text(slot) = control else { return FormOutcome::Nothing };
    let slot_value = match (slot.kind(), value) {
        (SlotKind::Text, ControlValue::Text(t)) => SlotValue::Text(t.clone()),
        (SlotKind::Toggle, ControlValue::Toggle(on)) => SlotValue::Toggle(*on),
        (SlotKind::Choice, ControlValue::Choice(i)) => SlotValue::Choice(*i),
        (SlotKind::Press, ControlValue::Press) => SlotValue::Press,
        // The CSV button asks for a file first.
        (SlotKind::Csv, ControlValue::Press) => return FormOutcome::PickFile(FilePick::Csv),
        _ => return FormOutcome::Nothing,
    };
    text_panel_intent(ctx.vm, ctx.document, *slot, slot_value).map_or(FormOutcome::Nothing, FormOutcome::Intent)
}

/// The CSV the user chose for the variable text (`None`: cancelled).
pub fn file_chosen(file: Option<(String, Vec<u8>)>, ctx: &FormContext<'_>) -> FormOutcome {
    text_panel_intent(ctx.vm, ctx.document, Slot::VarCsv, SlotValue::Csv(file))
        .map_or(FormOutcome::Nothing, |i: UiIntent| FormOutcome::Intent(i))
}
