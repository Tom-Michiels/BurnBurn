//! The golf-7 workshop panels, headless (G7-werk): the confirmation before a start (M06). Same
//! plain [`G5Line`]s and [`G5Control`]s as the golf-5 panels, so both hosts draw it without code of
//! their own. It shows what presentation says is about to be started and sends exactly that.

use burnburn_presentation::{DocumentViewModel, START_NEEDS, StartConfirmViewModel, StartEvidence, UiIntent};
use burnburn_types::DocumentId;

use crate::l10n::Locale;
use crate::l10n_g7_werk::{G7Text, g7_fmt, g7_text};

use super::g5::{G5Control, G5Line};

/// A number with up to `decimals` decimals, without trailing zeros, in the window language.
fn num(value: f64, decimals: usize, locale: Locale) -> String {
    let mut text = format!("{value:.decimals$}");
    if text.contains('.') {
        text = text.trim_end_matches('0').trim_end_matches('.').to_owned();
    }
    if text == "-0" {
        text = "0".to_owned();
    }
    match locale {
        Locale::Nl => text.replace('.', ","),
        Locale::En => text,
    }
}

fn t(text: G7Text, locale: Locale) -> String {
    g7_text(text, locale).to_owned()
}

/// The plan identity as far as a person needs it to tell two jobs apart.
fn short_plan(plan: &impl std::fmt::Display) -> String {
    plan.to_string().chars().take("plan-".len() + 12).collect()
}

pub(super) fn start_confirm_lines(doc: &DocumentViewModel, locale: Locale) -> Option<Vec<G5Line>> {
    let c: &StartConfirmViewModel = doc.start_confirm.as_ref()?;
    let not_declared = t(G7Text::NotDeclared, locale);
    let unknown = t(G7Text::Unknown, locale);
    let mut lines = vec![
        G5Line::Heading(t(G7Text::StartConfirmHeading, locale)),
        G5Line::Text(g7_fmt(
            G7Text::StartJobLine,
            locale,
            &[&c.job_name, &c.revision.to_string(), &short_plan(&c.plan)],
        )),
        G5Line::Text(g7_fmt(
            G7Text::StartMachineLine,
            locale,
            &[
                &c.machine,
                c.declared_model.as_deref().unwrap_or(&not_declared),
                c.declared_firmware.as_deref().unwrap_or(&not_declared),
            ],
        )),
        G5Line::Text(g7_fmt(G7Text::StartProfileLine, locale, &[&c.profile_id, &c.profile_revision.to_string()])),
        G5Line::Text(g7_fmt(G7Text::StartMaterialLine, locale, &[c.material.as_deref().unwrap_or(&unknown)])),
    ];
    let size = c.bounds.map_or_else(
        || unknown.clone(),
        |b| format!("{} × {} mm", num(b.width(), 1, locale), num(b.height(), 1, locale)),
    );
    lines.push(G5Line::Text(g7_fmt(G7Text::StartSizeLine, locale, &[&size])));
    lines.push(G5Line::Text(match c.placed_from_mm {
        Some((x, y)) => g7_fmt(G7Text::StartPlacementOnHead, locale, &[&num(x, 3, locale), &num(y, 3, locale)]),
        None => t(G7Text::StartPlacementDesigned, locale),
    }));
    // Every evidence state is said as what it is; one this build does not know is not read as a proof.
    let needs = match &c.evidence {
        StartEvidence::NotProven { needs } => *needs,
        _ => START_NEEDS,
    };
    lines.push(G5Line::Problem(g7_fmt(G7Text::StartNotProven, locale, &[needs])));
    lines.push(G5Line::Button {
        control: G5Control::StartConfirm,
        title: t(G7Text::StartConfirmButton, locale),
        enabled: true,
    });
    lines.push(G5Line::Button {
        control: G5Control::StartCancel,
        title: t(G7Text::StartCancelButton, locale),
        enabled: true,
    });
    Some(lines)
}

/// The confirm button sends exactly the confirmation on screen (action and seen copied); cancel only
/// closes it.
pub(super) fn start_confirm_intent(
    doc: &DocumentViewModel,
    document: DocumentId,
    control: G5Control,
) -> Option<UiIntent> {
    match control {
        G5Control::StartConfirm => {
            let c = doc.start_confirm.as_ref()?;
            Some(UiIntent::ConfirmStart { document: Some(document), action: c.action, seen: c.seen.clone() })
        }
        G5Control::StartCancel => doc.start_confirm.as_ref().map(|_| UiIntent::CancelStart),
        _ => None,
    }
}
