//! The golf-7 workshop panels, headless (G7-werk): the confirmation before a start (M06). Same
//! plain [`G5Line`]s and [`G5Control`]s as the golf-5 panels, so both hosts draw it without code of
//! their own. It shows what presentation says is about to be started and sends exactly that.

use burnburn_presentation::{
    CandidateDecision, DocumentViewModel, RecognitionIntent, RecognitionPhase, RecognitionProblem, RemnantIntent,
    RemnantLinkStatus, RemnantProblem, START_NEEDS, StartConfirmViewModel, StartEvidence, UiIntent,
};
use burnburn_types::DocumentId;

use crate::l10n::Locale;
use crate::l10n_g7_werk::{G7Text, g7_fmt, g7_text};

use super::g5::{G5Control, G5Line, flag_of, index_of, value_of};

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

// --- The remnant library (W07) ---------------------------------------------------------------------

fn problem_text(problem: RemnantProblem, locale: Locale) -> String {
    let text = match problem {
        RemnantProblem::NotReady => G7Text::ProblemNotReady,
        RemnantProblem::Unreadable => G7Text::ProblemUnreadable,
        RemnantProblem::NeedsSelection => G7Text::ProblemNeedsSelection,
        RemnantProblem::NoClosedOutline => G7Text::ProblemNoClosedOutline,
        RemnantProblem::NeedsName => G7Text::ProblemNeedsName,
        RemnantProblem::NeedsMaterial => G7Text::ProblemNeedsMaterial,
        RemnantProblem::InvalidThickness => G7Text::ProblemInvalidThickness,
        RemnantProblem::InvalidDate => G7Text::ProblemInvalidDate,
        RemnantProblem::NameTaken => G7Text::ProblemNameTaken,
        RemnantProblem::ZoneOutsideSheet => G7Text::ProblemZoneOutside,
        RemnantProblem::NotASheet => G7Text::ProblemNotASheet,
        RemnantProblem::Unknown => G7Text::ProblemUnknown,
        // A problem this build has no words for is still a problem, not a success.
        _ => G7Text::ProblemNotSaved,
    };
    t(text, locale)
}

fn field(control: G5Control, caption: G7Text, locale: Locale) -> G5Line {
    G5Line::Field { control, caption: t(caption, locale), text: String::new() }
}

/// The remnant panel: the kept remnants, the form that keeps the selection, and the remnant of each
/// order. It always has something to show: an empty library says so.
pub(super) fn remnants_lines(
    doc: &DocumentViewModel,
    locale: Locale,
    draft: &[(G5Control, String)],
) -> Option<Vec<G5Line>> {
    let _ = draft;
    let remnants = &doc.remnants;
    let mut lines = vec![G5Line::Heading(t(G7Text::RemnantsHeading, locale))];
    if remnants.rows.is_empty() {
        lines.push(G5Line::Text(t(G7Text::RemnantsNone, locale)));
    }
    for r in &remnants.rows {
        lines.push(G5Line::Text(g7_fmt(
            G7Text::RemnantLine,
            locale,
            &[
                &r.name,
                &r.material,
                &num(r.thickness_mm, 3, locale),
                &num(r.width_mm, 1, locale),
                &num(r.height_mm, 1, locale),
                &num(r.usable_cm2, 1, locale),
                &r.zones.to_string(),
                &r.date,
            ],
        )));
        if !r.note.is_empty() {
            lines.push(G5Line::Text(g7_fmt(G7Text::RemnantNote, locale, &[&r.note])));
        }
        lines.push(G5Line::Button {
            control: G5Control::RemnantPlace { id: r.id },
            title: g7_fmt(G7Text::RemnantPlaceButton, locale, &[&r.name]),
            enabled: true,
        });
        lines.push(G5Line::Button {
            control: G5Control::RemnantRemove { id: r.id },
            title: g7_fmt(G7Text::RemnantRemoveButton, locale, &[&r.name]),
            enabled: true,
        });
    }
    if let Some(problem) = remnants.problem {
        lines.push(G5Line::Problem(problem_text(problem, locale)));
    }
    lines.push(G5Line::Heading(t(G7Text::SaveHeading, locale)));
    lines.push(G5Line::Text(t(G7Text::SaveHint, locale)));
    lines.push(field(G5Control::RemnantName, G7Text::CaptionName, locale));
    lines.push(field(G5Control::RemnantMaterial, G7Text::CaptionMaterial, locale));
    lines.push(field(G5Control::RemnantThickness, G7Text::CaptionThickness, locale));
    lines.push(field(G5Control::RemnantNote, G7Text::CaptionNote, locale));
    lines.push(field(G5Control::RemnantDate, G7Text::CaptionDate, locale));
    lines.push(G5Line::Button { control: G5Control::RemnantSave, title: t(G7Text::SaveButton, locale), enabled: true });
    if let Some(o) = doc.operator.as_ref().filter(|o| !o.orders.is_empty()) {
        lines.push(G5Line::Heading(t(G7Text::OrdersHeading, locale)));
        for (index, order) in o.orders.iter().enumerate() {
            let mut options = vec![t(G7Text::OrderNoRemnant, locale)];
            options.extend(remnants.rows.iter().map(|r| r.name.clone()));
            let selected = order
                .remnant
                .as_ref()
                .and_then(|name| remnants.rows.iter().position(|r| &r.name == name))
                .map_or(0, |i| i + 1);
            lines.push(G5Line::Choice {
                control: G5Control::RemnantOrder { index },
                caption: g7_fmt(G7Text::OrderCaption, locale, &[&order.name]),
                options,
                selected,
            });
            match (&order.remnant, &order.remnant_link) {
                (Some(name), Some(RemnantLinkStatus::Missing)) => {
                    lines.push(G5Line::Problem(g7_fmt(G7Text::OrderLinkMissing, locale, &[name, &order.name])))
                }
                (Some(_), Some(RemnantLinkStatus::Known { material, material_matches: Some(false), .. })) => {
                    lines.push(G5Line::Problem(g7_fmt(G7Text::OrderLinkMismatch, locale, &[&order.name, material])));
                }
                _ => {}
            }
        }
        lines.push(G5Line::Text(t(G7Text::OrderLinkNote, locale)));
    }
    Some(lines)
}

/// What a control of the remnant panel asks for. Typed text goes on unparsed; presentation says what
/// is wrong with it.
pub(super) fn remnants_intent(
    doc: &DocumentViewModel,
    document: DocumentId,
    control: G5Control,
    values: &[(G5Control, String)],
) -> Option<UiIntent> {
    let typed = |c: G5Control| value_of(values, c).unwrap_or_default().to_owned();
    match control {
        G5Control::RemnantSave => Some(UiIntent::Remnant(RemnantIntent::SaveFromSelection {
            document,
            name: typed(G5Control::RemnantName),
            material: typed(G5Control::RemnantMaterial),
            thickness: typed(G5Control::RemnantThickness),
            note: typed(G5Control::RemnantNote),
            date: typed(G5Control::RemnantDate),
        })),
        G5Control::RemnantPlace { id } => Some(UiIntent::Remnant(RemnantIntent::Place { document, id })),
        G5Control::RemnantRemove { id } => Some(UiIntent::Remnant(RemnantIntent::Remove { id })),
        G5Control::RemnantOrder { index } => {
            let order = doc.operator.as_ref()?.orders.get(index)?;
            let choice = index_of(values, control)?;
            let remnant = match choice {
                0 => None,
                n => Some(doc.remnants.rows.get(n - 1)?.name.clone()),
            };
            Some(UiIntent::SetOrderRemnant { order: order.id, remnant })
        }
        _ => None,
    }
}

// --- Product recognition (W06) ----------------------------------------------------------------------

fn recog_problem_text(problem: RecognitionProblem, locale: Locale) -> String {
    let text = match problem {
        RecognitionProblem::NoCameraPort => G7Text::RecogProblemNoCameraPort,
        RecognitionProblem::NoFrame => G7Text::RecogProblemNoFrame,
        RecognitionProblem::InvalidNumber => G7Text::RecogProblemInvalidNumber,
        RecognitionProblem::OutsideFrame => G7Text::RecogProblemOutside,
        RecognitionProblem::FlatTemplate => G7Text::RecogProblemFlat,
        RecognitionProblem::TemplateTooSmall => G7Text::RecogProblemTooSmall,
        RecognitionProblem::TemplateLargerThanFrame => G7Text::RecogProblemTooLarge,
        RecognitionProblem::NoTemplate => G7Text::RecogProblemNoTemplate,
        RecognitionProblem::FrameConfigChanged => G7Text::RecogProblemConfig,
        RecognitionProblem::Busy => G7Text::RecogProblemBusy,
        RecognitionProblem::NothingFound => G7Text::RecogProblemNothingFound,
        RecognitionProblem::UnknownCandidate => G7Text::RecogProblemUnknownCandidate,
        RecognitionProblem::Undecided => G7Text::RecogProblemUndecided,
        RecognitionProblem::NothingAccepted => G7Text::RecogProblemNothingAccepted,
        RecognitionProblem::NothingSelected => G7Text::RecogProblemNothingSelected,
        RecognitionProblem::NotCalibrated => G7Text::RecogProblemNotCalibrated,
        RecognitionProblem::UnknownTemplate => G7Text::RecogProblemUnknownTemplate,
        RecognitionProblem::LibraryNotReady => G7Text::RecogProblemLibraryNotReady,
        RecognitionProblem::LibraryUnreadable => G7Text::RecogProblemLibraryUnreadable,
        // A problem this build has no words for is still a problem, not a success.
        _ => G7Text::RecogProblemSearchFailed,
    };
    t(text, locale)
}

fn recog_field(control: G5Control, caption: G7Text, locale: Locale) -> G5Line {
    G5Line::Field { control, caption: t(caption, locale), text: String::new() }
}

fn recog_button(control: G5Control, title: String, enabled: bool) -> G5Line {
    G5Line::Button { control, title, enabled }
}

/// The recognition panel: the template, the search, the matches with the operator's decision per
/// match, and the placement. The similarity is said as a similarity; the place in millimetres only
/// when the camera is calibrated.
pub(super) fn recognition_lines(
    doc: &DocumentViewModel,
    locale: Locale,
    draft: &[(G5Control, String)],
) -> Option<Vec<G5Line>> {
    let _ = draft;
    let r = &doc.recognition;
    let mut lines = vec![G5Line::Heading(t(G7Text::RecogHeading, locale)), G5Line::Text(t(G7Text::RecogHint, locale))];
    match &r.template {
        None => lines.push(G5Line::Text(t(G7Text::RecogNoTemplate, locale))),
        Some(template) => lines.push(G5Line::Text(g7_fmt(
            G7Text::RecogTemplateLine,
            locale,
            &[&template.name, &template.width_px.to_string(), &template.height_px.to_string(), &template.device.0],
        ))),
    }
    if let Some(template) = &r.template {
        let kept = if template.saved { G7Text::RecogKept } else { G7Text::RecogNotKept };
        lines.push(G5Line::Text(t(kept, locale)));
    }
    if r.library_unreadable {
        lines.push(G5Line::Problem(t(G7Text::RecogLibraryUnreadable, locale)));
    }
    if r.cameras.is_empty() {
        lines.push(G5Line::Unavailable(t(G7Text::RecogNoCamera, locale)));
    } else {
        lines.push(G5Line::Choice {
            control: G5Control::RecogCamera,
            caption: t(G7Text::RecogCameraCaption, locale),
            options: r.cameras.iter().map(|c| c.name.clone()).collect(),
            selected: r
                .template
                .as_ref()
                .and_then(|tpl| r.cameras.iter().position(|c| c.device == tpl.device))
                .unwrap_or(0),
        });
    }
    lines.push(recog_field(G5Control::RecogX, G7Text::RecogX, locale));
    lines.push(recog_field(G5Control::RecogY, G7Text::RecogY, locale));
    lines.push(recog_field(G5Control::RecogWidth, G7Text::RecogWidth, locale));
    lines.push(recog_field(G5Control::RecogHeight, G7Text::RecogHeight, locale));
    lines.push(recog_field(G5Control::RecogName, G7Text::RecogName, locale));
    lines.push(recog_button(G5Control::RecogLearn, t(G7Text::RecogLearn, locale), !r.cameras.is_empty()));
    // The box can also be pointed at in the camera picture on the sheet (two corners, or a drag).
    lines.push(recog_button(G5Control::RecogPoint, t(G7Text::RecogPoint, locale), !r.cameras.is_empty()));
    if let Some(pointing) = &r.pointing {
        lines.push(G5Line::Text(t(G7Text::RecogPointing, locale)));
        if let Some([x, y]) = pointing.first {
            lines.push(G5Line::Text(g7_fmt(
                G7Text::RecogPointingFirst,
                locale,
                &[&num(x, 0, locale), &num(y, 0, locale)],
            )));
        }
        lines.push(recog_button(G5Control::RecogEndPoint, t(G7Text::RecogEndPoint, locale), true));
    }
    lines.push(G5Line::Check {
        control: G5Control::RecogRotation,
        caption: t(G7Text::RecogRotation, locale),
        on: r.search.rotation,
    });
    lines.push(G5Line::Check {
        control: G5Control::RecogScale,
        caption: t(G7Text::RecogScale, locale),
        on: r.search.scale,
    });
    for (control, text, on) in [
        (G5Control::RecogMirror, G7Text::RecogMirror, r.search.mirror),
        (G5Control::RecogPerspective, G7Text::RecogPerspective, r.search.perspective),
        (G5Control::RecogPartial, G7Text::RecogPartial, r.search.partial),
    ] {
        lines.push(G5Line::Check { control, caption: t(text, locale), on });
    }
    if !r.saved.is_empty() {
        lines.push(G5Line::Heading(t(G7Text::RecogSavedHeading, locale)));
        for (index, saved) in r.saved.iter().enumerate() {
            let in_use = if saved.in_use { t(G7Text::RecogInUse, locale) } else { String::new() };
            lines.push(G5Line::Text(g7_fmt(
                G7Text::RecogSavedLine,
                locale,
                &[&saved.name, &saved.width_px.to_string(), &saved.height_px.to_string(), &in_use],
            )));
            lines.push(recog_button(
                G5Control::RecogUse { index },
                g7_fmt(G7Text::RecogUse, locale, &[&saved.name]),
                !saved.in_use,
            ));
            lines.push(recog_button(
                G5Control::RecogDelete { index },
                g7_fmt(G7Text::RecogDelete, locale, &[&saved.name]),
                true,
            ));
        }
    }
    if r.template.is_some() {
        lines.push(recog_button(G5Control::RecogForget, t(G7Text::RecogForget, locale), true));
        lines.push(recog_field(G5Control::RecogMinPercent, G7Text::RecogMinPercent, locale));
        let searching = r.phase == RecognitionPhase::Searching;
        lines.push(recog_button(G5Control::RecogFind, t(G7Text::RecogFind, locale), !searching));
        if searching {
            lines.push(G5Line::Text(t(G7Text::RecogSearching, locale)));
            lines.push(recog_button(G5Control::RecogCancel, t(G7Text::RecogCancel, locale), true));
        }
    }
    if !r.candidates.is_empty() {
        lines.push(G5Line::Heading(t(G7Text::RecogCandidatesHeading, locale)));
        if r.needs_review {
            lines.push(G5Line::Problem(t(G7Text::RecogReviewNeeded, locale)));
        }
        for c in &r.candidates {
            let place = match c.bed_mm {
                Some([x, y]) => g7_fmt(G7Text::RecogOnBed, locale, &[&num(x, 2, locale), &num(y, 2, locale)]),
                None => t(G7Text::RecogNotCalibrated, locale),
            };
            let decision = match c.decision {
                CandidateDecision::Pending => G7Text::RecogPending,
                CandidateDecision::Accepted => G7Text::RecogAccepted,
                CandidateDecision::Rejected => G7Text::RecogRejected,
            };
            let n = (c.index + 1).to_string();
            lines.push(G5Line::Text(g7_fmt(
                G7Text::RecogCandidateLine,
                locale,
                &[
                    &n,
                    &num(c.similarity_percent, 1, locale),
                    &num(c.x_px, 1, locale),
                    &num(c.y_px, 1, locale),
                    &place,
                    g7_text(decision, locale),
                ],
            )));
            if c.angle_deg != 0.0 || c.scale != 1.0 {
                lines.push(G5Line::Text(g7_fmt(
                    G7Text::RecogPoseLine,
                    locale,
                    &[&n, &num(c.angle_deg, 1, locale), &num(c.scale * 100.0, 0, locale)],
                )));
            }
            if c.mirrored {
                lines.push(G5Line::Text(g7_fmt(G7Text::RecogMirroredLine, locale, &[&n])));
            }
            if c.visible_percent < 100.0 {
                lines.push(G5Line::Problem(g7_fmt(
                    G7Text::RecogPartlyVisible,
                    locale,
                    &[&n, &num(c.visible_percent, 0, locale)],
                )));
            }
            lines.push(recog_button(
                G5Control::RecogAccept { index: c.index },
                g7_fmt(G7Text::RecogAccept, locale, &[&n]),
                c.decision != CandidateDecision::Accepted,
            ));
            lines.push(recog_button(
                G5Control::RecogReject { index: c.index },
                g7_fmt(G7Text::RecogReject, locale, &[&n]),
                c.decision != CandidateDecision::Rejected,
            ));
        }
        lines.push(G5Line::Text(t(G7Text::RecogNote, locale)));
        lines.push(recog_button(G5Control::RecogPlace, t(G7Text::RecogPlace, locale), r.can_place));
    }
    if let Some(problem) = r.problem {
        lines.push(G5Line::Problem(recog_problem_text(problem, locale)));
    }
    Some(lines)
}

/// What a control of the recognition panel asks for. Typed text goes on unparsed; presentation says
/// what is wrong with it.
pub(super) fn recognition_intent(
    doc: &DocumentViewModel,
    document: DocumentId,
    control: G5Control,
    values: &[(G5Control, String)],
) -> Option<UiIntent> {
    let typed = |c: G5Control| value_of(values, c).unwrap_or_default().to_owned();
    let r = &doc.recognition;
    match control {
        G5Control::RecogLearn => {
            let chosen = index_of(values, G5Control::RecogCamera).unwrap_or(0);
            let device = r.cameras.get(chosen).or_else(|| r.cameras.first())?.device.clone();
            Some(UiIntent::Recognition(RecognitionIntent::Learn {
                device,
                x: typed(G5Control::RecogX),
                y: typed(G5Control::RecogY),
                width: typed(G5Control::RecogWidth),
                height: typed(G5Control::RecogHeight),
                name: typed(G5Control::RecogName),
            }))
        }
        G5Control::RecogForget => Some(UiIntent::Recognition(RecognitionIntent::Forget)),
        G5Control::RecogFind => {
            Some(UiIntent::Recognition(RecognitionIntent::Find { min_percent: typed(G5Control::RecogMinPercent) }))
        }
        G5Control::RecogCancel => Some(UiIntent::Recognition(RecognitionIntent::CancelFind)),
        G5Control::RecogAccept { index } => {
            Some(UiIntent::Recognition(RecognitionIntent::Decide { index, decision: CandidateDecision::Accepted }))
        }
        G5Control::RecogReject { index } => {
            Some(UiIntent::Recognition(RecognitionIntent::Decide { index, decision: CandidateDecision::Rejected }))
        }
        G5Control::RecogPlace => Some(UiIntent::Recognition(RecognitionIntent::Place { document })),
        G5Control::RecogRotation | G5Control::RecogScale => {
            // The one that was switched reports its new state; the other is as it stands.
            let flag = |c: G5Control, now: bool| flag_of(values, c).unwrap_or(now);
            Some(UiIntent::Recognition(RecognitionIntent::SetSearch {
                rotation: flag(G5Control::RecogRotation, r.search.rotation),
                scale: flag(G5Control::RecogScale, r.search.scale),
            }))
        }
        G5Control::RecogMirror | G5Control::RecogPerspective | G5Control::RecogPartial => {
            let flag = |c: G5Control, now: bool| flag_of(values, c).unwrap_or(now);
            Some(UiIntent::Recognition(RecognitionIntent::SetSearchExtras {
                mirror: flag(G5Control::RecogMirror, r.search.mirror),
                perspective: flag(G5Control::RecogPerspective, r.search.perspective),
                partial: flag(G5Control::RecogPartial, r.search.partial),
            }))
        }
        G5Control::RecogPoint => {
            let chosen = index_of(values, G5Control::RecogCamera).unwrap_or(0);
            let device = r.cameras.get(chosen).or_else(|| r.cameras.first())?.device.clone();
            Some(UiIntent::Recognition(RecognitionIntent::PointBox { device, name: typed(G5Control::RecogName) }))
        }
        G5Control::RecogEndPoint => Some(UiIntent::Recognition(RecognitionIntent::EndPoint)),
        G5Control::RecogUse { index } => {
            Some(UiIntent::Recognition(RecognitionIntent::Use { id: r.saved.get(index)?.id }))
        }
        G5Control::RecogDelete { index } => {
            Some(UiIntent::Recognition(RecognitionIntent::Delete { id: r.saved.get(index)?.id }))
        }
        _ => None,
    }
}
