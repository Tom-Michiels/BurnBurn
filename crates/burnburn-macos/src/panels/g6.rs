//! The golf-6 editor panels, headless (G6-editor): the generator panel (E10). Shares the plain-line
//! model of the golf-5 panels (`G5Line`, `G5Control`): both hosts draw the lines and report a
//! control with the panel's current values; field text goes to presentation unparsed (DR-05) and a
//! value presentation already shows sends nothing.

use burnburn_presentation::{
    DocumentViewModel, GeneratorEdit, GeneratorField, GeneratorIssue, GeneratorKind, GeneratorProblem, UiIntent,
};
use burnburn_types::DocumentId;

use super::g5::{G5Control, G5Line, flag_of, index_of, value_of};
use crate::l10n::Locale;
use crate::l10n_g6::{G6Text, g6_fmt, g6_text};

const KINDS: [GeneratorKind; 2] = [GeneratorKind::FingerBox, GeneratorKind::LivingHinge];

/// A number of millimetres with up to three decimals, in the window language.
fn mm_text(value: f64, locale: Locale) -> String {
    let mut text = format!("{value:.3}");
    if text.contains('.') {
        text = text.trim_end_matches('0').trim_end_matches('.').to_owned();
    }
    match locale {
        Locale::Nl => text.replace('.', ","),
        Locale::En => text,
    }
}

fn caption_text(kind: GeneratorKind, field: GeneratorField) -> G6Text {
    use GeneratorField as F;
    match (kind, field) {
        (GeneratorKind::FingerBox, F::Width) => G6Text::FieldBoxWidth,
        (GeneratorKind::FingerBox, F::Depth) => G6Text::FieldBoxDepth,
        (GeneratorKind::FingerBox, F::Height) => G6Text::FieldBoxHeight,
        (GeneratorKind::LivingHinge, F::Width) => G6Text::FieldHingeWidth,
        (GeneratorKind::LivingHinge, F::Height) => G6Text::FieldHingeHeight,
        (_, F::Depth) => G6Text::FieldBoxDepth,
        (_, F::Thickness) => G6Text::FieldThickness,
        (_, F::FingerWidth) => G6Text::FieldFingerWidth,
        (_, F::PanelSpacing) => G6Text::FieldPanelSpacing,
        (_, F::CutLength) => G6Text::FieldCutLength,
        (_, F::Bridge) => G6Text::FieldBridge,
        (_, F::ColumnSpacing) => G6Text::FieldColumnSpacing,
    }
}

fn problem_text(kind: GeneratorKind, p: &GeneratorProblem, locale: Locale) -> String {
    let caption = g6_text(caption_text(kind, p.field), locale);
    match p.issue {
        GeneratorIssue::InvalidNumber => g6_fmt(G6Text::ProblemNumber, locale, &[caption]),
        GeneratorIssue::NotPositive => g6_fmt(G6Text::ProblemNotPositive, locale, &[caption]),
        GeneratorIssue::Negative => g6_fmt(G6Text::ProblemNegative, locale, &[caption]),
        GeneratorIssue::TooSmall { min_mm, value_mm } => {
            g6_fmt(G6Text::ProblemTooSmall, locale, &[caption, &mm_text(min_mm, locale), &mm_text(value_mm, locale)])
        }
        GeneratorIssue::Count { min, max, value } => g6_fmt(
            G6Text::ProblemCount,
            locale,
            &[caption, &min.to_string(), &max.to_string(), &value.to_string()],
        ),
        _ => g6_fmt(G6Text::ProblemEngine, locale, &[caption]),
    }
}

/// The lines of the generator panel, or `None` while the dialog is closed.
pub(super) fn generator_lines(doc: &DocumentViewModel, locale: Locale) -> Option<Vec<G5Line>> {
    let d = doc.generator_dialog.as_ref()?;
    let t = |x: G6Text| g6_text(x, locale).to_owned();
    let mut lines = vec![G5Line::Choice {
        control: G5Control::GeneratorKind,
        caption: t(G6Text::KindCaption),
        options: vec![t(G6Text::KindBox), t(G6Text::KindHinge)],
        selected: KINDS.iter().position(|k| *k == d.kind).unwrap_or(0),
    }];
    for row in &d.fields {
        lines.push(G5Line::Field {
            control: G5Control::GeneratorField(row.field),
            caption: t(caption_text(d.kind, row.field)),
            text: row.text.clone(),
        });
    }
    if let Some(open) = d.lid_open {
        lines.push(G5Line::Check { control: G5Control::GeneratorLid, caption: t(G6Text::LidOff), on: open });
    }
    if d.kind == GeneratorKind::FingerBox {
        lines.push(G5Line::Text(t(G6Text::KerfNote)));
    }
    for p in &d.problems {
        lines.push(G5Line::Problem(problem_text(d.kind, p, locale)));
    }
    if let Some(p) = &d.preview {
        let (first, text) = match d.kind {
            GeneratorKind::FingerBox => (p.parts, G6Text::PreviewBox),
            GeneratorKind::LivingHinge => (p.cuts, G6Text::PreviewHinge),
        };
        lines.push(G5Line::Text(g6_fmt(text, locale, &[&first.to_string(), &p.width, &p.height])));
    }
    lines.push(G5Line::Button {
        control: G5Control::GeneratorInsert,
        title: t(if d.editing { G6Text::Apply } else { G6Text::Insert }),
        enabled: d.can_insert,
    });
    lines.push(G5Line::Button { control: G5Control::GeneratorCancel, title: t(G6Text::Cancel), enabled: true });
    Some(lines)
}

/// The intent of a generator-panel control.
pub(super) fn generator_intent(
    doc: &DocumentViewModel,
    document: DocumentId,
    control: G5Control,
    values: &[(G5Control, String)],
) -> Option<UiIntent> {
    let d = doc.generator_dialog.as_ref()?;
    let edit = match control {
        G5Control::GeneratorKind => {
            let kind = *KINDS.get(index_of(values, control)?)?;
            (kind != d.kind).then_some(GeneratorEdit::Kind(kind))?
        }
        G5Control::GeneratorField(field) => {
            let text = value_of(values, control)?;
            let shown = d.fields.iter().find(|r| r.field == field)?;
            (text != shown.text).then(|| GeneratorEdit::Field(field, text.to_owned()))?
        }
        G5Control::GeneratorLid => {
            let open = flag_of(values, control)?;
            (Some(open) != d.lid_open && d.lid_open.is_some()).then_some(GeneratorEdit::LidOpen(open))?
        }
        G5Control::GeneratorInsert => d.can_insert.then_some(GeneratorEdit::Insert)?,
        G5Control::GeneratorCancel => GeneratorEdit::Cancel,
        _ => return None,
    };
    Some(UiIntent::Generator { document, edit })
}
