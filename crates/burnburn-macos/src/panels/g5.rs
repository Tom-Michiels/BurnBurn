//! The golf-5 panels, headless (G5-native): project notes (P04), offset and repair (E05, E09),
//! centre-line decisions (T04, T08) and the structure panel (S06, S07, P06). This file decides
//! which plain lines each panel shows, in the window language, and which intent a control sends.
//! No AppKit and no Win32: both hosts draw the same [`G5Line`]s and report a [`G5Control`] with the
//! panel's current values; raw text goes to presentation unparsed (DR-05).

use burnburn_editor::IssueKind;
use burnburn_presentation::{
    AppViewModel, BrokenReason, CenterlineEdit, DocumentViewModel, InstanceStatus, JunctionState, OffsetJoin,
    OffsetOptions, PartRow, PartValue, PlateIssueRow, PrimitiveKind, ReferenceStatus, StructureEdit, UiIntent,
};
use burnburn_types::DocumentId;

use crate::l10n::{Locale, message_or_key};
use crate::l10n_g5::{G5Text, g5_fmt, g5_text};

/// The panels, each a floating window of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum G5Panel {
    /// Archief ▸ Toon notities.
    Notes,
    /// Venster ▸ Offset en reparatie.
    Offset,
    /// The decisions of an open centre-line trace.
    Centerline,
    /// Venster ▸ Structuur.
    Structure,
    /// Tekenen ▸ Doos of scharnier…: the generator dialog (E10, G6-editor).
    Generator,
    /// Archief ▸ Zoek projecten…: the project search (P08, G6-editor).
    Search,
    /// Beeld ▸ Beeld voorbereiden: local threshold, plane correction, masks, local corrections and
    /// the trace report of the open tracer (G6-werk, T03, T05, T06).
    TracePrep,
    /// Venster ▸ Operatorweergave: machine, approved jobs and orders in one place (G6-werk, W08).
    Operator,
}

impl G5Panel {
    pub const ALL: [G5Panel; 8] = [
        G5Panel::Notes,
        G5Panel::Offset,
        G5Panel::Centerline,
        G5Panel::Structure,
        G5Panel::Generator,
        G5Panel::Search,
        G5Panel::TracePrep,
        G5Panel::Operator,
    ];
}

/// A control of a panel. Fields, choices, checks and the note report their value; buttons only the
/// click.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum G5Control {
    NotesText,
    NotesSave,
    NotesClose,
    OffsetDistance,
    OffsetJoin,
    OffsetKeepSources,
    OffsetPreview,
    OffsetApply,
    RepairIssue,
    RepairTolerance,
    RepairPreview,
    RepairApply,
    /// Pairing `candidate` at junction `node`.
    Junction {
        node: u32,
        candidate: usize,
    },
    /// Accept the best exact shape of `chain`.
    Primitive {
        chain: u32,
    },
    /// The plate Compile uses.
    Plate,
    /// The variant of an instance.
    Variant {
        instance: u64,
    },
    /// A local override of parameter number `parameter` (position in the part's list).
    Override {
        instance: u64,
        parameter: usize,
    },
    /// The kind of generator (finger-jointed box, living hinge).
    GeneratorKind,
    GeneratorField(burnburn_presentation::GeneratorField),
    /// Box: leave the lid off.
    GeneratorLid,
    GeneratorInsert,
    GeneratorCancel,
    SearchFolder,
    SearchQuery,
    SearchRescan,
    /// The found project the panel shows (a pop-up of the results).
    SearchResult,
    /// The frozen revision of that project (a pop-up).
    SearchRevision,
    SearchOpenProject,
    SearchOpenRevision,
    SearchClose,
    // --- Prepare image (G6-werk, T03, T05, T06).
    PrepRadius,
    PrepOffset,
    PrepUseLocal,
    PrepUseGlobal,
    /// Corner `index` (0…3), `axis` 0 = x, 1 = y.
    PrepCorner {
        index: usize,
        axis: usize,
    },
    PrepWidth,
    PrepHeight,
    PrepApplyCorners,
    PrepClearCorners,
    PrepMaskKind,
    /// The area shared by "add mask" and "trace this area again".
    PrepAreaX,
    PrepAreaY,
    PrepAreaWidth,
    PrepAreaHeight,
    PrepAddMask,
    PrepRemoveMask {
        index: usize,
    },
    PrepRegionThreshold,
    PrepAnchors,
    PrepRetrace,
    PrepRemoveCorrection {
        index: usize,
    },
    // --- Parts and references (G6-werk, S06, S07).
    StructDefineName,
    StructDefine,
    PartPlaceX {
        part: u64,
    },
    PartPlaceY {
        part: u64,
    },
    PartPlace {
        part: u64,
    },
    /// The definition's default of parameter number `parameter`.
    PartParam {
        part: u64,
        parameter: usize,
    },
    PartVariantName {
        part: u64,
    },
    PartAddVariant {
        part: u64,
    },
    /// What variant number `variant` sets for parameter number `parameter`.
    PartVariantValue {
        part: u64,
        variant: usize,
        parameter: usize,
    },
    PartRemove {
        part: u64,
    },
    InstanceRemove {
        instance: u64,
    },
    RefName,
    RefEdge,
    RefAdd,
    RefCandidate {
        reference: u64,
    },
    RefRebind {
        reference: u64,
    },
    RefRemove {
        reference: u64,
    },
    // --- The compact operator view (G6-werk, W08).
    /// Machine control number `index` as the machine service offers it.
    OperatorMachine {
        index: usize,
    },
    /// Upload of approved job number `index`.
    OperatorUpload {
        index: usize,
    },
    /// Upload of the job of order number `index`.
    OperatorOrderUpload {
        index: usize,
    },
    // --- The golf-7 editor sections (G7-editor): modifier stack, plates and variants.
    Editor(super::g7_editor::EditorControl),
}

/// One line of a panel.
#[derive(Debug, Clone, PartialEq)]
pub enum G5Line {
    Heading(String),
    Text(String),
    /// Why something did not work; drawn apart from the normal text.
    Problem(String),
    /// A multi-line text.
    Memo {
        control: G5Control,
        text: String,
        max_chars: usize,
    },
    Field {
        control: G5Control,
        caption: String,
        text: String,
    },
    Choice {
        control: G5Control,
        caption: String,
        options: Vec<String>,
        selected: usize,
    },
    Check {
        control: G5Control,
        caption: String,
        on: bool,
    },
    Button {
        control: G5Control,
        title: String,
        enabled: bool,
    },
    /// A small picture (the preview of a found project).
    Image(crate::panels_image::BitmapImage),
}

impl G5Line {
    pub fn control(&self) -> Option<G5Control> {
        match self {
            G5Line::Memo { control, .. }
            | G5Line::Field { control, .. }
            | G5Line::Choice { control, .. }
            | G5Line::Check { control, .. }
            | G5Line::Button { control, .. } => Some(*control),
            G5Line::Heading(_) | G5Line::Text(_) | G5Line::Problem(_) | G5Line::Image(_) => None,
        }
    }
}

/// What a panel shows now.
#[derive(Debug, Clone, PartialEq)]
pub struct G5PanelModel {
    pub panel: G5Panel,
    pub title: String,
    pub lines: Vec<G5Line>,
}

/// The panels that open and close by themselves with the view model (the notes while they are
/// open, the decisions while a centre-line trace is open); the others wait for their menu item.
pub fn g5_opens_by_itself(panel: G5Panel) -> bool {
    matches!(panel, G5Panel::Notes | G5Panel::Centerline | G5Panel::Generator | G5Panel::Search)
}

/// A number with up to `decimals` decimals, without trailing zeros, in the window language.
fn num(value: f64, decimals: usize, locale: Locale) -> String {
    let mut text = format!("{value:.decimals$}");
    if text.contains('.') {
        text = text.trim_end_matches('0').trim_end_matches('.').to_owned();
    }
    match locale {
        Locale::Nl => text.replace('.', ","),
        Locale::En => text,
    }
}

pub(super) fn value_of(values: &[(G5Control, String)], control: G5Control) -> Option<&str> {
    values.iter().find(|(c, _)| *c == control).map(|(_, v)| v.as_str())
}

pub(super) fn index_of(values: &[(G5Control, String)], control: G5Control) -> Option<usize> {
    value_of(values, control)?.trim().parse().ok()
}

pub(super) fn flag_of(values: &[(G5Control, String)], control: G5Control) -> Option<bool> {
    match value_of(values, control)? {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

fn text(t: G5Text, locale: Locale) -> G5Line {
    G5Line::Text(g5_text(t, locale).to_owned())
}

fn button(control: G5Control, title: String, enabled: bool) -> G5Line {
    G5Line::Button { control, title, enabled }
}

/// The panel for `document`, or `None` when it has nothing to show. `draft` holds what the user
/// has typed in the panel's controls so far (the notes panel counts it); it is empty when unknown.
pub fn g5_panel(
    vm: &AppViewModel,
    document: DocumentId,
    panel: G5Panel,
    locale: Locale,
    draft: &[(G5Control, String)],
) -> Option<G5PanelModel> {
    let doc = vm.document(document)?;
    let (title, lines) = match panel {
        G5Panel::Notes => (G5Text::NotesTitle, notes_lines(doc, locale, draft)?),
        G5Panel::Offset => (G5Text::OffsetTitle, offset_lines(doc, locale)),
        G5Panel::Centerline => (G5Text::CenterlineTitle, centerline_lines(doc, locale)?),
        G5Panel::Structure => (G5Text::StructureTitle, structure_lines(doc, locale)),
        G5Panel::Search => {
            return Some(G5PanelModel {
                panel,
                title: crate::l10n_g6::g6_text(crate::l10n_g6::G6Text::SearchTitle, locale).to_owned(),
                lines: super::g6::search_lines(doc, locale, draft)?,
            });
        }
        G5Panel::Generator => {
            return Some(G5PanelModel {
                panel,
                title: crate::l10n_g6::g6_text(crate::l10n_g6::G6Text::GeneratorTitle, locale).to_owned(),
                lines: super::g6::generator_lines(doc, locale)?,
            });
        }
        G5Panel::TracePrep => {
            let lines = super::g6_werk::trace_prep_lines(doc, locale)?;
            let title = crate::l10n_g6_werk::g6_text(crate::l10n_g6_werk::G6Text::TracePrepTitle, locale);
            return Some(G5PanelModel { panel, title: title.to_owned(), lines });
        }
        G5Panel::Operator => {
            let lines = super::g6_werk::operator_lines(doc, locale)?;
            let title = crate::l10n_g6_werk::g6_text(crate::l10n_g6_werk::G6Text::OperatorTitle, locale);
            return Some(G5PanelModel { panel, title: title.to_owned(), lines });
        }
    };
    Some(G5PanelModel { panel, title: g5_text(title, locale).to_owned(), lines })
}

// --- Notes ---------------------------------------------------------------------------------------

fn notes_lines(doc: &DocumentViewModel, locale: Locale, draft: &[(G5Control, String)]) -> Option<Vec<G5Line>> {
    let notes = doc.notes.as_ref()?;
    let typed = value_of(draft, G5Control::NotesText).map_or_else(|| notes.text.chars().count(), |t| t.chars().count());
    let too_long = typed > notes.max_chars;
    let (count, max) = (typed.to_string(), notes.max_chars.to_string());
    let mut lines = vec![
        G5Line::Memo { control: G5Control::NotesText, text: notes.text.clone(), max_chars: notes.max_chars },
        G5Line::Text(g5_fmt(G5Text::NotesLimit, locale, &[&count, &max])),
    ];
    if too_long {
        lines.push(G5Line::Problem(g5_fmt(G5Text::NotesTooLong, locale, &[&max])));
    }
    lines.push(button(G5Control::NotesSave, g5_text(G5Text::NotesSave, locale).to_owned(), !too_long));
    lines.push(button(G5Control::NotesClose, g5_text(G5Text::NotesClose, locale).to_owned(), true));
    Some(lines)
}

// --- Offset and repair ----------------------------------------------------------------------------

const JOINS: [OffsetJoin; 3] = [OffsetJoin::Round, OffsetJoin::Miter, OffsetJoin::Bevel];

fn issue_text(index: usize, issue: &burnburn_editor::GeometryIssue, locale: Locale) -> String {
    let n = (index + 1).to_string();
    match (issue.kind, issue.size_mm) {
        (IssueKind::OpenContour, Some(gap)) => g5_fmt(G5Text::IssueOpen, locale, &[&n, &num(gap, 3, locale)]),
        (IssueKind::SelfIntersection, _) => g5_fmt(G5Text::IssueCrossing, locale, &[&n]),
        _ => g5_fmt(G5Text::IssueOther, locale, &[&n]),
    }
}

fn offset_lines(doc: &DocumentViewModel, locale: Locale) -> Vec<G5Line> {
    let t = |x: G5Text| g5_text(x, locale).to_owned();
    let has_selection = !doc.selection.is_empty();
    let options = doc.offset.options;
    let mut lines = vec![
        G5Line::Heading(t(G5Text::OffsetHeading)),
        G5Line::Field {
            control: G5Control::OffsetDistance,
            caption: t(G5Text::OffsetDistance),
            text: doc.offset.preview.as_ref().map_or_else(|| "1 mm".to_owned(), |p| p.distance.clone()),
        },
        G5Line::Choice {
            control: G5Control::OffsetJoin,
            caption: t(G5Text::OffsetJoin),
            options: vec![t(G5Text::JoinRound), t(G5Text::JoinMiter), t(G5Text::JoinBevel)],
            selected: JOINS.iter().position(|j| *j == options.join).unwrap_or(0),
        },
        G5Line::Check {
            control: G5Control::OffsetKeepSources,
            caption: t(G5Text::OffsetKeep),
            on: options.keep_sources,
        },
        button(G5Control::OffsetPreview, t(G5Text::OffsetPreview), has_selection),
        button(G5Control::OffsetApply, t(G5Text::OffsetApply), has_selection),
    ];
    if let Some(p) = &doc.offset.preview {
        let area = num(p.area_mm2, 1, locale);
        lines.push(G5Line::Text(g5_fmt(G5Text::OffsetPreviewLine, locale, &[&p.width, &p.height, &area])));
    }
    lines.push(G5Line::Heading(t(G5Text::RepairHeading)));
    if doc.issues.is_empty() {
        lines.push(text(G5Text::RepairNone, locale));
    } else {
        let selected = doc.repair_preview.as_ref().map_or(0, |p| p.issue).min(doc.issues.len() - 1);
        lines.push(G5Line::Choice {
            control: G5Control::RepairIssue,
            caption: t(G5Text::RepairIssue),
            options: doc.issues.iter().enumerate().map(|(i, issue)| issue_text(i, issue, locale)).collect(),
            selected,
        });
        lines.push(G5Line::Field {
            control: G5Control::RepairTolerance,
            caption: t(G5Text::RepairTolerance),
            text: format!("{} mm", num(burnburn_editor::NEARLY_CLOSED_MM, 3, locale)),
        });
        lines.push(button(G5Control::RepairPreview, t(G5Text::RepairPreview), true));
        lines.push(button(G5Control::RepairApply, t(G5Text::RepairApply), true));
        if let Some(p) = &doc.repair_preview {
            lines.push(G5Line::Text(g5_fmt(
                G5Text::RepairResult,
                locale,
                &[&p.closes.to_string(), &p.stays_open.to_string()],
            )));
        }
    }
    if let Some(problem) = &doc.problem {
        lines.push(G5Line::Problem(message_or_key(problem.message, locale)));
    }
    lines
}

// --- Centre-line decisions ------------------------------------------------------------------------

fn primitive_name(kind: PrimitiveKind, locale: Locale) -> &'static str {
    g5_text(
        match kind {
            PrimitiveKind::Line => G5Text::PrimLine,
            PrimitiveKind::Arc => G5Text::PrimArc,
            PrimitiveKind::Circle => G5Text::PrimCircle,
        },
        locale,
    )
}

fn centerline_lines(doc: &DocumentViewModel, locale: Locale) -> Option<Vec<G5Line>> {
    let c = doc.centerline.as_ref()?;
    let mut lines = Vec::new();
    if c.junctions.is_empty() && c.chains.is_empty() {
        lines.push(text(G5Text::CenterlineNothing, locale));
    }
    for j in &c.junctions {
        let (n, x, y) = (j.node.to_string(), num(j.at.0, 1, locale), num(j.at.1, 1, locale));
        match j.state {
            JunctionState::Ambiguous { candidates } => {
                lines.push(G5Line::Text(g5_fmt(G5Text::JunctionOpen, locale, &[&n, &x, &y])));
                for candidate in 0..candidates {
                    lines.push(button(
                        G5Control::Junction { node: j.node, candidate },
                        g5_fmt(G5Text::JunctionPair, locale, &[&n, &(candidate + 1).to_string()]),
                        true,
                    ));
                }
            }
            JunctionState::Resolved => lines.push(G5Line::Text(g5_fmt(G5Text::JunctionDone, locale, &[&n, &x, &y]))),
        }
    }
    for chain in &c.chains {
        let n = chain.chain.to_string();
        if let Some(kind) = chain.confirmed {
            lines.push(G5Line::Text(g5_fmt(G5Text::ChainConfirmed, locale, &[&n, primitive_name(kind, locale)])));
            continue;
        }
        let described: Vec<String> = chain
            .candidates
            .iter()
            .map(|p| {
                let verdict = g5_text(if p.within_tolerance { G5Text::Within } else { G5Text::Outside }, locale);
                let residual = num(p.max_residual_mm, 3, locale);
                g5_fmt(G5Text::Residual, locale, &[primitive_name(p.kind, locale), verdict, &residual])
            })
            .collect();
        let summary =
            if described.is_empty() { g5_text(G5Text::ChainNoFit, locale).to_owned() } else { described.join("; ") };
        lines.push(G5Line::Text(g5_fmt(G5Text::ChainLine, locale, &[&n, &summary])));
        if let Some(best) = chain.candidates.iter().find(|p| p.within_tolerance) {
            lines.push(button(
                G5Control::Primitive { chain: chain.chain },
                g5_fmt(G5Text::ChainConfirm, locale, &[&n, primitive_name(best.kind, locale)]),
                true,
            ));
        }
    }
    if c.edits > 0 {
        lines.push(G5Line::Text(g5_fmt(G5Text::CenterlineEdits, locale, &[&c.edits.to_string()])));
    }
    if c.dropped > 0 {
        lines.push(G5Line::Problem(g5_fmt(G5Text::CenterlineDropped, locale, &[&c.dropped.to_string()])));
    }
    Some(lines)
}

// --- Structure ----------------------------------------------------------------------------------

fn pairs(values: &[PartValue]) -> String {
    values.iter().map(|v| format!("{} = {}", v.name, v.value)).collect::<Vec<_>>().join(", ")
}

fn structure_lines(doc: &DocumentViewModel, locale: Locale) -> Vec<G5Line> {
    let t = |x: G5Text| g5_text(x, locale).to_owned();
    let s = &doc.structure;
    let mut lines = vec![G5Line::Heading(t(G5Text::PlatesHeading))];
    if s.plates.is_empty() {
        lines.push(text(G5Text::PlateNone, locale));
    } else {
        let mut options = vec![t(G5Text::PlateWhole)];
        options.extend(s.plates.iter().map(|p| p.name.clone()));
        let selected = s.compile_plate.and_then(|id| s.plates.iter().position(|p| p.id == id)).map_or(0, |i| i + 1);
        lines.push(G5Line::Choice { control: G5Control::Plate, caption: t(G5Text::PlateCompile), options, selected });
        for p in &s.plates {
            let thickness = p
                .thickness
                .as_ref()
                .map_or_else(|| t(G5Text::PlateNoThickness), |th| g5_fmt(G5Text::PlateThickness, locale, &[th]));
            let made = if p.material.is_empty() { thickness } else { format!("{}, {thickness}", p.material) };
            lines.push(G5Line::Text(g5_fmt(
                G5Text::PlateLine,
                locale,
                &[&p.name, &p.width, &p.height, &made, &p.instances.to_string(), &p.objects.to_string()],
            )));
        }
    }
    for issue in &s.plate_issues {
        lines.push(G5Line::Problem(match issue {
            PlateIssueRow::TargetOffPlate { operation, object } => {
                g5_fmt(G5Text::IssueTargetOff, locale, &[operation, &object.to_string()])
            }
            PlateIssueRow::Blocked { object } => g5_fmt(G5Text::IssueBlocked, locale, &[&object.to_string()]),
        }));
    }

    lines.extend(super::g6_werk::define_lines(s, locale));
    lines.push(G5Line::Heading(t(G5Text::PartsHeading)));
    if s.parts.is_empty() {
        lines.push(text(G5Text::PartsNone, locale));
    }
    for part in &s.parts {
        lines.push(G5Line::Heading(part.name.clone()));
        lines.push(G5Line::Text(g5_fmt(G5Text::PartLine, locale, &[&part.name, &pairs(&part.parameters)])));
        for instance in &part.instances {
            let id = instance.id.to_string();
            let on = instance.plate.as_ref().map_or_else(String::new, |p| g5_fmt(G5Text::OnPlate, locale, &[p]));
            let (status, failed) = match &instance.status {
                InstanceStatus::Valid => (t(G5Text::StatusValid), false),
                InstanceStatus::Pending => (t(G5Text::StatusPending), false),
                InstanceStatus::Failed { parameter: Some(p) } => {
                    (g5_fmt(G5Text::StatusFailedParam, locale, &[p]), true)
                }
                InstanceStatus::Failed { parameter: None } => (t(G5Text::StatusFailed), true),
            };
            let line = g5_fmt(G5Text::InstanceLine, locale, &[&id, &on, &status]);
            lines.push(if failed { G5Line::Problem(line) } else { G5Line::Text(line) });
            let mut variants = vec![t(G5Text::VariantNone)];
            variants.extend(part.variants.iter().cloned());
            let selected =
                instance.variant.as_ref().and_then(|v| part.variants.iter().position(|x| x == v)).map_or(0, |i| i + 1);
            lines.push(G5Line::Choice {
                control: G5Control::Variant { instance: instance.id },
                caption: g5_fmt(G5Text::VariantCaption, locale, &[&id]),
                options: variants,
                selected,
            });
            for (parameter, p) in part.parameters.iter().enumerate() {
                let current = instance.overrides.iter().find(|o| o.name == p.name).map_or("", |o| o.value.as_str());
                lines.push(G5Line::Field {
                    control: G5Control::Override { instance: instance.id, parameter },
                    caption: g5_fmt(G5Text::OverrideCaption, locale, &[&id, &p.name]),
                    text: current.to_owned(),
                });
            }
            lines.push(G5Line::Text(g5_fmt(G5Text::EffectiveLine, locale, &[&pairs(&instance.effective)])));
            lines.push(super::g6_werk::instance_remove_line(instance, locale));
        }
        lines.extend(super::g6_werk::part_edit_lines(part, locale));
    }

    lines.push(G5Line::Heading(t(G5Text::ReferencesHeading)));
    if s.references.is_empty() {
        lines.push(text(G5Text::ReferencesNone, locale));
    }
    lines.extend(super::g6_werk::reference_make_lines(s, locale));
    for r in &s.references {
        lines.push(match r.status {
            ReferenceStatus::Bound => {
                G5Line::Text(g5_fmt(G5Text::RefBound, locale, &[&r.name, r.source.as_deref().unwrap_or("")]))
            }
            ReferenceStatus::Ambiguous { candidates } => {
                G5Line::Problem(g5_fmt(G5Text::RefAmbiguous, locale, &[&r.name, &candidates.to_string()]))
            }
            ReferenceStatus::Broken { reason: BrokenReason::ObjectMissing } => {
                G5Line::Problem(g5_fmt(G5Text::RefMissing, locale, &[&r.name]))
            }
            ReferenceStatus::Broken { reason: BrokenReason::OriginGone } => {
                G5Line::Problem(g5_fmt(G5Text::RefOrigin, locale, &[&r.name]))
            }
        });
        lines.extend(super::g6_werk::reference_edit_lines(r, locale));
    }
    lines.extend(super::g7_editor::modifier_lines(s, locale));
    lines
}

// --- Controls to intents ----------------------------------------------------------------------------

fn find_instance(doc: &DocumentViewModel, id: u64) -> Option<(&PartRow, &burnburn_presentation::InstanceRow)> {
    doc.structure.parts.iter().find_map(|p| p.instances.iter().find(|i| i.id == id).map(|i| (p, i)))
}

/// The intent a control sends, from the panel's values at that moment (field text unparsed, a
/// choice as its index, a check as `1` / `0`). A control whose value is what presentation already
/// shows sends nothing, so leaving a field untouched is not an edit.
pub fn g5_intent(
    vm: &AppViewModel,
    document: DocumentId,
    _panel: G5Panel,
    control: G5Control,
    values: &[(G5Control, String)],
) -> Option<UiIntent> {
    let doc = vm.document(document)?;
    let typed = |c: G5Control| value_of(values, c).map(str::to_owned);
    match control {
        G5Control::PrepRadius
        | G5Control::PrepOffset
        | G5Control::PrepUseLocal
        | G5Control::PrepUseGlobal
        | G5Control::PrepCorner { .. }
        | G5Control::PrepWidth
        | G5Control::PrepHeight
        | G5Control::PrepApplyCorners
        | G5Control::PrepClearCorners
        | G5Control::PrepMaskKind
        | G5Control::PrepAreaX
        | G5Control::PrepAreaY
        | G5Control::PrepAreaWidth
        | G5Control::PrepAreaHeight
        | G5Control::PrepAddMask
        | G5Control::PrepRemoveMask { .. }
        | G5Control::PrepRegionThreshold
        | G5Control::PrepAnchors
        | G5Control::PrepRetrace
        | G5Control::PrepRemoveCorrection { .. } => super::g6_werk::trace_prep_intent(doc, document, control, values),
        G5Control::OperatorMachine { .. }
        | G5Control::OperatorUpload { .. }
        | G5Control::OperatorOrderUpload { .. } => super::g6_werk::operator_intent(doc, document, control),
        G5Control::StructDefineName
        | G5Control::StructDefine
        | G5Control::PartPlaceX { .. }
        | G5Control::PartPlaceY { .. }
        | G5Control::PartPlace { .. }
        | G5Control::PartParam { .. }
        | G5Control::PartVariantName { .. }
        | G5Control::PartAddVariant { .. }
        | G5Control::PartVariantValue { .. }
        | G5Control::PartRemove { .. }
        | G5Control::InstanceRemove { .. }
        | G5Control::RefName
        | G5Control::RefEdge
        | G5Control::RefAdd
        | G5Control::RefCandidate { .. }
        | G5Control::RefRebind { .. }
        | G5Control::RefRemove { .. } => super::g6_werk::structure_edit_intent(doc, document, control, values),
        G5Control::Editor(editor) => super::g7_editor::editor_intent(doc, document, editor, values),
        G5Control::NotesSave => {
            let text = typed(G5Control::NotesText)?;
            let max = doc.notes.as_ref()?.max_chars;
            (text.chars().count() <= max).then_some(UiIntent::SetNotes { document, text })
        }
        G5Control::NotesClose => doc.notes.as_ref().map(|_| UiIntent::CloseNotes { document }),
        G5Control::NotesText => None,
        G5Control::OffsetPreview => (!doc.selection.is_empty()).then(|| UiIntent::PreviewOffset {
            document,
            distance: typed(G5Control::OffsetDistance).unwrap_or_default(),
        }),
        G5Control::OffsetApply => (!doc.selection.is_empty())
            .then(|| UiIntent::Offset { document, distance: typed(G5Control::OffsetDistance).unwrap_or_default() }),
        G5Control::OffsetDistance => None,
        G5Control::OffsetJoin | G5Control::OffsetKeepSources => {
            let current = doc.offset.options;
            let join =
                index_of(values, G5Control::OffsetJoin).and_then(|i| JOINS.get(i).copied()).unwrap_or(current.join);
            let keep_sources = flag_of(values, G5Control::OffsetKeepSources).unwrap_or(current.keep_sources);
            let options = OffsetOptions { join, keep_sources };
            (options != current).then_some(UiIntent::SetOffsetOptions { document, options })
        }
        G5Control::RepairPreview | G5Control::RepairApply => {
            let issue = index_of(values, G5Control::RepairIssue)?;
            (issue < doc.issues.len()).then_some(())?;
            let tolerance = typed(G5Control::RepairTolerance).unwrap_or_default();
            Some(if control == G5Control::RepairPreview {
                UiIntent::PreviewRepair { document, issue, tolerance }
            } else {
                UiIntent::RepairIssue { document, issue, tolerance }
            })
        }
        G5Control::RepairIssue | G5Control::RepairTolerance => None,
        G5Control::Junction { node, candidate } => {
            Some(UiIntent::CenterlineEdit { document, edit: CenterlineEdit::ResolveJunction { node, candidate } })
        }
        G5Control::Primitive { chain } => {
            Some(UiIntent::CenterlineEdit { document, edit: CenterlineEdit::ConfirmPrimitive { chain } })
        }
        G5Control::Plate => {
            let chosen = match index_of(values, G5Control::Plate)? {
                0 => None,
                i => Some(doc.structure.plates.get(i - 1)?.id),
            };
            (chosen != doc.structure.compile_plate).then_some(UiIntent::ChoosePlate { document, plate: chosen })
        }
        G5Control::Variant { instance } => {
            let (part, row) = find_instance(doc, instance)?;
            let chosen = match index_of(values, control)? {
                0 => None,
                i => Some(part.variants.get(i - 1)?.clone()),
            };
            (chosen != row.variant).then_some(UiIntent::Structure {
                document,
                edit: StructureEdit::SetVariant { instance, variant: chosen },
            })
        }
        G5Control::GeneratorKind
        | G5Control::GeneratorField(_)
        | G5Control::GeneratorLid
        | G5Control::GeneratorInsert
        | G5Control::GeneratorCancel => super::g6::generator_intent(doc, document, control, values),
        G5Control::SearchFolder
        | G5Control::SearchQuery
        | G5Control::SearchRescan
        | G5Control::SearchResult
        | G5Control::SearchRevision
        | G5Control::SearchOpenProject
        | G5Control::SearchOpenRevision
        | G5Control::SearchClose => super::g6::search_intent(doc, document, control, values),
        G5Control::Override { instance, parameter } => {
            let (part, row) = find_instance(doc, instance)?;
            let name = part.parameters.get(parameter)?.name.clone();
            let text = typed(control)?;
            let current = row.overrides.iter().find(|o| o.name == name).map_or("", |o| o.value.as_str());
            (text.trim() != current).then_some(UiIntent::Structure {
                document,
                edit: StructureEdit::SetOverride { instance, parameter: name, text },
            })
        }
    }
}
