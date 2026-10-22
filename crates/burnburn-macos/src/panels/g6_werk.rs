//! The golf-6 workshop panels, headless (G6-werk): preparing an image for tracing (T03 local
//! threshold, plane correction and masks; T05 local corrections; T06 the report). Same plain
//! [`G5Line`]s and [`G5Control`]s as the golf-5 panels, so both hosts draw them without code of
//! their own. Field text goes to presentation unparsed (DR-05).

use burnburn_presentation::{
    CorrectionStatus, DocumentViewModel, MaskMode, PrepField, PrepIssue, TracePrepIntent, TracePrepViewModel, UiIntent,
};
use burnburn_types::DocumentId;

use crate::l10n::Locale;
use crate::l10n_g6_werk::{G6Text, g6_fmt, g6_text};

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

fn t(text: G6Text, locale: Locale) -> String {
    g6_text(text, locale).to_owned()
}

fn field_name(field: PrepField, locale: Locale) -> String {
    match field {
        PrepField::Radius => t(G6Text::FieldRadius, locale),
        PrepField::Offset => t(G6Text::FieldOffset, locale),
        PrepField::CornerX(i) | PrepField::CornerY(i) => g6_fmt(G6Text::FieldCorner, locale, &[&(i + 1).to_string()]),
        PrepField::OutWidth | PrepField::MaskWidth => t(G6Text::FieldWidth, locale),
        PrepField::OutHeight | PrepField::MaskHeight => t(G6Text::FieldHeight, locale),
        PrepField::MaskX => t(G6Text::FieldMaskX, locale),
        PrepField::MaskY => t(G6Text::FieldMaskY, locale),
        PrepField::RegionThreshold => t(G6Text::FieldThreshold, locale),
        PrepField::Anchors => t(G6Text::FieldAnchors, locale),
    }
}

fn issue_text(issue: PrepIssue, locale: Locale) -> String {
    match issue {
        PrepIssue::InvalidNumber(f) => g6_fmt(G6Text::IssueNumber, locale, &[&field_name(f, locale)]),
        PrepIssue::OutOfRange(f) => g6_fmt(G6Text::IssueRange, locale, &[&field_name(f, locale)]),
        PrepIssue::DegenerateCorners => t(G6Text::IssueCorners, locale),
        PrepIssue::MaskOutsideImage => t(G6Text::IssueOutside, locale),
        PrepIssue::UnknownMask => t(G6Text::IssueUnknown, locale),
        PrepIssue::NothingToCorrect => t(G6Text::IssueNothing, locale),
        PrepIssue::StoredCorrectionsUnreadable => t(G6Text::IssueStored, locale),
        // A newer issue this host has no words for yet.
        _ => t(G6Text::IssueUnknown, locale),
    }
}

fn field(control: G5Control, caption: String, text: String) -> G5Line {
    G5Line::Field { control, caption, text }
}

fn button(control: G5Control, title: String, enabled: bool) -> G5Line {
    G5Line::Button { control, title, enabled }
}

/// The preparation panel: shown while a tracer is open.
pub(super) fn trace_prep_lines(doc: &DocumentViewModel, locale: Locale) -> Option<Vec<G5Line>> {
    let p = doc.trace_prep.as_ref()?;
    let mut lines = Vec::new();

    lines.push(G5Line::Heading(t(G6Text::ThresholdHeading, locale)));
    lines.push(G5Line::Text(if p.threshold.local {
        g6_fmt(G6Text::ThresholdLocal, locale, &[&p.threshold.radius, &p.threshold.offset])
    } else {
        t(G6Text::ThresholdGlobal, locale)
    }));
    lines.push(field(G5Control::PrepRadius, t(G6Text::RadiusCaption, locale), p.threshold.radius.clone()));
    lines.push(field(G5Control::PrepOffset, t(G6Text::OffsetCaption, locale), p.threshold.offset.clone()));
    lines.push(button(G5Control::PrepUseLocal, t(G6Text::UseLocal, locale), true));
    lines.push(button(G5Control::PrepUseGlobal, t(G6Text::UseGlobal, locale), p.threshold.local));

    lines.push(G5Line::Heading(t(G6Text::PerspectiveHeading, locale)));
    lines.push(G5Line::Text(t(G6Text::PerspectiveHint, locale)));
    for (index, corner) in p.perspective.corners.iter().enumerate() {
        let n = (index + 1).to_string();
        lines.push(field(
            G5Control::PrepCorner { index, axis: 0 },
            g6_fmt(G6Text::CornerXCaption, locale, &[&n]),
            corner[0].clone(),
        ));
        lines.push(field(
            G5Control::PrepCorner { index, axis: 1 },
            g6_fmt(G6Text::CornerYCaption, locale, &[&n]),
            corner[1].clone(),
        ));
    }
    lines.push(field(G5Control::PrepWidth, t(G6Text::CorrectedWidth, locale), p.perspective.width.clone()));
    lines.push(field(G5Control::PrepHeight, t(G6Text::CorrectedHeight, locale), p.perspective.height.clone()));
    lines.push(button(G5Control::PrepApplyCorners, t(G6Text::ApplyCorrection, locale), true));
    lines.push(button(G5Control::PrepClearCorners, t(G6Text::ClearCorrection, locale), p.perspective.on));
    if let Some(tr) = &p.perspective.transform {
        let mut shown: Vec<String> = tr.matrix[..8].iter().map(|v| num(*v, 4, locale)).collect();
        shown.push(num(tr.residual_px, 4, locale));
        let values: Vec<&str> = shown.iter().map(String::as_str).collect();
        lines.push(G5Line::Text(g6_fmt(G6Text::TransformLine, locale, &values)));
    }

    lines.push(G5Line::Heading(t(G6Text::MasksHeading, locale)));
    let (w, h) = p.source_px;
    lines.push(G5Line::Choice {
        control: G5Control::PrepMaskKind,
        caption: t(G6Text::MaskKind, locale),
        options: vec![t(G6Text::MaskKeep, locale), t(G6Text::MaskRemove, locale)],
        selected: 0,
    });
    lines.push(field(G5Control::PrepAreaX, t(G6Text::MaskXCaption, locale), "0".to_owned()));
    lines.push(field(G5Control::PrepAreaY, t(G6Text::MaskYCaption, locale), "0".to_owned()));
    lines.push(field(G5Control::PrepAreaWidth, t(G6Text::MaskWidthCaption, locale), w.to_string()));
    lines.push(field(G5Control::PrepAreaHeight, t(G6Text::MaskHeightCaption, locale), h.to_string()));
    lines.push(button(G5Control::PrepAddMask, t(G6Text::AddMask, locale), true));
    if p.masks.is_empty() {
        lines.push(G5Line::Text(t(G6Text::MasksNone, locale)));
    }
    for (index, m) in p.masks.iter().enumerate() {
        let kind = t(if m.mode == MaskMode::Keep { G6Text::MaskKeepShort } else { G6Text::MaskRemoveShort }, locale);
        let n = (index + 1).to_string();
        lines.push(G5Line::Text(g6_fmt(
            G6Text::MaskLine,
            locale,
            &[&n, &kind, &m.x.to_string(), &m.y.to_string(), &m.width.to_string(), &m.height.to_string()],
        )));
        lines.push(button(G5Control::PrepRemoveMask { index }, g6_fmt(G6Text::RemoveMask, locale, &[&n]), true));
    }

    lines.push(G5Line::Heading(t(G6Text::CorrectionsHeading, locale)));
    lines.push(field(G5Control::PrepRegionThreshold, t(G6Text::RegionThresholdCaption, locale), String::new()));
    lines.push(field(G5Control::PrepAnchors, t(G6Text::AnchorsCaption, locale), String::new()));
    lines.push(button(G5Control::PrepRetrace, t(G6Text::RetraceRegion, locale), true));
    if p.corrections.is_empty() {
        lines.push(G5Line::Text(t(G6Text::CorrectionsNone, locale)));
    }
    for (index, c) in p.corrections.iter().enumerate() {
        let n = (index + 1).to_string();
        let (status, problem) = match c.status {
            CorrectionStatus::Applied => (t(G6Text::CorrectionApplied, locale), false),
            CorrectionStatus::NeedsRelink { lost, new_in_region } => {
                (g6_fmt(G6Text::CorrectionRelink, locale, &[&lost.to_string(), &new_in_region.to_string()]), true)
            }
            CorrectionStatus::Failed => (t(G6Text::CorrectionFailed, locale), true),
        };
        let line = g6_fmt(
            G6Text::CorrectionLine,
            locale,
            &[
                &n,
                &c.x.to_string(),
                &c.y.to_string(),
                &c.width.to_string(),
                &c.height.to_string(),
                &c.threshold,
                &c.anchors.to_string(),
                &status,
            ],
        );
        lines.push(if problem { G5Line::Problem(line) } else { G5Line::Text(line) });
        lines.push(button(
            G5Control::PrepRemoveCorrection { index },
            g6_fmt(G6Text::RemoveCorrection, locale, &[&n]),
            true,
        ));
    }

    if let Some(r) = &p.report {
        lines.push(G5Line::Heading(t(G6Text::ReportHeading, locale)));
        lines.push(G5Line::Text(g6_fmt(
            G6Text::ReportBudget,
            locale,
            &[
                &num(r.tolerance_mm, 3, locale),
                &num(r.max_deviation_mm, 3, locale),
                &num(r.p95_deviation_mm, 3, locale),
            ],
        )));
        if r.max_deviation_mm > r.tolerance_mm + 1e-6 {
            lines.push(G5Line::Problem(t(G6Text::ReportOver, locale)));
        }
        for d in &r.worst {
            lines.push(G5Line::Text(g6_fmt(
                G6Text::ReportWorst,
                locale,
                &[&num(d.mm, 3, locale), &num(d.x_mm, 2, locale), &num(d.y_mm, 2, locale)],
            )));
        }
        lines.push(G5Line::Text(g6_fmt(G6Text::ReportShapes, locale, &[&r.parts.to_string(), &r.holes.to_string()])));
        for f in &r.removed {
            let kind = t(if f.hole { G6Text::RemovedHole } else { G6Text::RemovedPart }, locale);
            lines.push(G5Line::Problem(g6_fmt(
                G6Text::ReportRemoved,
                locale,
                &[&kind, &num(f.area_mm2, 3, locale), &num(f.x_mm, 2, locale), &num(f.y_mm, 2, locale)],
            )));
        }
    }
    if let Some(issue) = p.issue {
        lines.push(G5Line::Problem(issue_text(issue, locale)));
    }
    Some(lines)
}

fn value_of<'a>(values: &'a [(G5Control, String)], control: G5Control) -> Option<&'a str> {
    values.iter().find(|(c, _)| *c == control).map(|(_, v)| v.as_str())
}

/// The intent of a control of the preparation panel. Fields and choices only hold text; a button
/// sends it all, unparsed, as one intent.
pub(super) fn trace_prep_intent(
    doc: &DocumentViewModel,
    document: DocumentId,
    control: G5Control,
    values: &[(G5Control, String)],
) -> Option<UiIntent> {
    let p: &TracePrepViewModel = doc.trace_prep.as_ref()?;
    let typed = |c: G5Control, shown: &str| value_of(values, c).unwrap_or(shown).to_owned();
    let send = |intent: TracePrepIntent| Some(UiIntent::TracePrep { document, intent });
    let (w, h) = p.source_px;
    match control {
        G5Control::PrepUseLocal => send(TracePrepIntent::UseLocalThreshold {
            radius: typed(G5Control::PrepRadius, &p.threshold.radius),
            offset: typed(G5Control::PrepOffset, &p.threshold.offset),
        }),
        G5Control::PrepUseGlobal => send(TracePrepIntent::UseGlobalThreshold),
        G5Control::PrepApplyCorners => {
            let corner = |i: usize, axis: usize| {
                typed(G5Control::PrepCorner { index: i, axis }, &p.perspective.corners[i][axis])
            };
            send(TracePrepIntent::ApplyPerspective {
                corners: std::array::from_fn(|i| [corner(i, 0), corner(i, 1)]),
                width: typed(G5Control::PrepWidth, &p.perspective.width),
                height: typed(G5Control::PrepHeight, &p.perspective.height),
            })
        }
        G5Control::PrepClearCorners => {
            p.perspective.on.then(|| UiIntent::TracePrep { document, intent: TracePrepIntent::ClearPerspective })
        }
        G5Control::PrepAddMask => send(TracePrepIntent::AddMask {
            mode: if value_of(values, G5Control::PrepMaskKind).and_then(|v| v.trim().parse::<usize>().ok()) == Some(1) {
                MaskMode::Remove
            } else {
                MaskMode::Keep
            },
            x: typed(G5Control::PrepAreaX, "0"),
            y: typed(G5Control::PrepAreaY, "0"),
            width: typed(G5Control::PrepAreaWidth, &w.to_string()),
            height: typed(G5Control::PrepAreaHeight, &h.to_string()),
        }),
        G5Control::PrepRemoveMask { index } => send(TracePrepIntent::RemoveMask { index }),
        G5Control::PrepRetrace => send(TracePrepIntent::RetraceRegion {
            x: typed(G5Control::PrepAreaX, "0"),
            y: typed(G5Control::PrepAreaY, "0"),
            width: typed(G5Control::PrepAreaWidth, &w.to_string()),
            height: typed(G5Control::PrepAreaHeight, &h.to_string()),
            threshold: typed(G5Control::PrepRegionThreshold, ""),
            anchors: typed(G5Control::PrepAnchors, ""),
        }),
        G5Control::PrepRemoveCorrection { index } => send(TracePrepIntent::RemoveCorrection { index }),
        _ => None,
    }
}

// --- Parts and references (S06, S07) ---------------------------------------------------------------

use burnburn_presentation::{InstanceRow, PartRow, ReferenceRow, ReferenceStatus, StructureEdit, StructureViewModel};

/// The "make a part" section: name, how many objects are selected, the button.
pub(super) fn define_lines(s: &StructureViewModel, locale: Locale) -> Vec<G5Line> {
    vec![
        G5Line::Heading(t(G6Text::DefineHeading, locale)),
        G5Line::Text(g6_fmt(G6Text::DefineSelected, locale, &[&s.selected.to_string()])),
        field(G5Control::StructDefineName, t(G6Text::PartNameCaption, locale), String::new()),
        button(G5Control::StructDefine, t(G6Text::DefineButton, locale), s.selected > 0),
    ]
}

/// What a part can be done with: placed, its defaults changed, variants added and valued, removed.
pub(super) fn part_edit_lines(part: &PartRow, locale: Locale) -> Vec<G5Line> {
    let id = part.id;
    let name = part.name.as_str();
    let mut lines = vec![
        field(G5Control::PartPlaceX { part: id }, g6_fmt(G6Text::PlaceXCaption, locale, &[name]), "0".to_owned()),
        field(G5Control::PartPlaceY { part: id }, g6_fmt(G6Text::PlaceYCaption, locale, &[name]), "0".to_owned()),
        button(G5Control::PartPlace { part: id }, g6_fmt(G6Text::PlaceButton, locale, &[name]), true),
    ];
    for (parameter, p) in part.parameters.iter().enumerate() {
        lines.push(field(
            G5Control::PartParam { part: id, parameter },
            g6_fmt(G6Text::ParamCaption, locale, &[name, &p.name]),
            p.value.clone(),
        ));
    }
    for (variant, v) in part.variant_values.iter().enumerate() {
        for (parameter, p) in part.parameters.iter().enumerate() {
            let current = v.values.iter().find(|x| x.name == p.name).map_or("", |x| x.value.as_str());
            lines.push(field(
                G5Control::PartVariantValue { part: id, variant, parameter },
                g6_fmt(G6Text::VariantValueCaption, locale, &[name, &v.name, &p.name]),
                current.to_owned(),
            ));
        }
    }
    lines.push(field(
        G5Control::PartVariantName { part: id },
        g6_fmt(G6Text::VariantNameCaption, locale, &[name]),
        String::new(),
    ));
    lines.push(button(G5Control::PartAddVariant { part: id }, g6_fmt(G6Text::AddVariantButton, locale, &[name]), true));
    lines.push(button(
        G5Control::PartRemove { part: id },
        g6_fmt(G6Text::RemovePartButton, locale, &[name]),
        part.instances.is_empty(),
    ));
    lines
}

pub(super) fn instance_remove_line(instance: &InstanceRow, locale: Locale) -> G5Line {
    button(
        G5Control::InstanceRemove { instance: instance.id },
        g6_fmt(G6Text::RemoveInstanceButton, locale, &[&instance.id.to_string()]),
        true,
    )
}

/// The "make a reference" section: the edges of the single selected object to choose from.
pub(super) fn reference_make_lines(s: &StructureViewModel, locale: Locale) -> Vec<G5Line> {
    let mut lines = vec![G5Line::Heading(t(G6Text::RefMakeHeading, locale))];
    if s.edges.is_empty() {
        lines.push(G5Line::Text(t(G6Text::RefNoEdges, locale)));
        return lines;
    }
    lines.push(field(G5Control::RefName, t(G6Text::RefNameCaption, locale), String::new()));
    lines.push(G5Line::Choice {
        control: G5Control::RefEdge,
        caption: t(G6Text::RefEdgeCaption, locale),
        options: s
            .edges
            .iter()
            .enumerate()
            .map(|(i, e)| g6_fmt(G6Text::RefEdgeItem, locale, &[&(i + 1).to_string(), &e.length]))
            .collect(),
        selected: 0,
    });
    lines.push(button(G5Control::RefAdd, t(G6Text::RefAddButton, locale), true));
    lines
}

/// What can be done with one reference: remove it, or choose among the candidates when ambiguous.
pub(super) fn reference_edit_lines(r: &ReferenceRow, locale: Locale) -> Vec<G5Line> {
    let mut lines = Vec::new();
    if matches!(r.status, ReferenceStatus::Ambiguous { .. }) && !r.candidates.is_empty() {
        lines.push(G5Line::Choice {
            control: G5Control::RefCandidate { reference: r.id },
            caption: g6_fmt(G6Text::RefCandidateCaption, locale, &[&r.name]),
            options: r.candidates.clone(),
            selected: 0,
        });
        lines.push(button(
            G5Control::RefRebind { reference: r.id },
            g6_fmt(G6Text::RefRebindButton, locale, &[&r.name]),
            true,
        ));
    }
    lines.push(button(
        G5Control::RefRemove { reference: r.id },
        g6_fmt(G6Text::RefRemoveButton, locale, &[&r.name]),
        true,
    ));
    lines
}

/// The intent of a control of the part and reference editing in the structure panel.
pub(super) fn structure_edit_intent(
    doc: &DocumentViewModel,
    document: DocumentId,
    control: G5Control,
    values: &[(G5Control, String)],
) -> Option<UiIntent> {
    let s = &doc.structure;
    let typed = |c: G5Control, shown: &str| value_of(values, c).unwrap_or(shown).to_owned();
    let index_of = |c: G5Control| value_of(values, c).and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(0);
    let send = |edit: StructureEdit| Some(UiIntent::Structure { document, edit });
    let part_of = |id: u64| s.parts.iter().find(|p| p.id == id);
    match control {
        G5Control::StructDefine => send(StructureEdit::DefinePart { name: typed(G5Control::StructDefineName, "") }),
        G5Control::PartPlace { part } => send(StructureEdit::PlaceInstance {
            definition: part,
            x: typed(G5Control::PartPlaceX { part }, "0"),
            y: typed(G5Control::PartPlaceY { part }, "0"),
        }),
        G5Control::PartParam { part, parameter } => {
            let p = part_of(part)?;
            let current = p.parameters.get(parameter)?;
            let text = value_of(values, control)?.trim().to_owned();
            (text != current.value).then(|| UiIntent::Structure {
                document,
                edit: StructureEdit::SetParameter { definition: part, parameter: current.name.clone(), text },
            })
        }
        G5Control::PartVariantValue { part, variant, parameter } => {
            let p = part_of(part)?;
            let v = p.variant_values.get(variant)?;
            let named = p.parameters.get(parameter)?;
            let current = v.values.iter().find(|x| x.name == named.name).map_or("", |x| x.value.as_str());
            let text = value_of(values, control)?.trim().to_owned();
            (text != current).then(|| UiIntent::Structure {
                document,
                edit: StructureEdit::SetVariantValue {
                    definition: part,
                    variant: v.name.clone(),
                    parameter: named.name.clone(),
                    text,
                },
            })
        }
        G5Control::PartAddVariant { part } => {
            send(StructureEdit::AddVariant { definition: part, name: typed(G5Control::PartVariantName { part }, "") })
        }
        G5Control::PartRemove { part } => send(StructureEdit::RemovePart { definition: part }),
        G5Control::InstanceRemove { instance } => send(StructureEdit::RemoveInstance { instance }),
        G5Control::RefAdd => {
            let object = s.edge_object?;
            let edge = s.edges.get(index_of(G5Control::RefEdge))?;
            send(StructureEdit::AddReference {
                name: typed(G5Control::RefName, ""),
                object,
                subpath: edge.subpath,
                segment: edge.segment,
            })
        }
        G5Control::RefRebind { reference } => send(StructureEdit::RebindReference {
            reference,
            candidate: index_of(G5Control::RefCandidate { reference }),
        }),
        G5Control::RefRemove { reference } => send(StructureEdit::RemoveReference { reference }),
        _ => None,
    }
}
