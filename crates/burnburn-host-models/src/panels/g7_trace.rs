//! The golf-7 trace sections of the preparation panel, headless (G7-trace): lens correction (T03),
//! pointing in the image (T03), protected and kept contours and the node goal (T05, T06), pieces the
//! filter dropped (T06), own recipes, the A/B comparison and the batch (T10). The same plain
//! [`G5Line`]s and [`G5Control`]s as the other panels, so both hosts draw them without code of their
//! own. Field text goes to presentation unparsed (DR-05).

use std::path::PathBuf;

use burnburn_geometry::Path;
use burnburn_presentation::{
    BatchProblemView, CompareCorrectionState, CompareViewModel, DocumentViewModel, MaskMode, PickKind, PrepField,
    PrepIssue, RecipeChoice, RecipeIssue, TracePick, TracePrepIntent, TracePrepViewModel, TracePreset,
    TraceRecipeIntent, TraceRecipesViewModel, UiIntent, VariantSummary,
};
use burnburn_types::DocumentId;

use crate::l10n::Locale;
use crate::l10n_g7_trace::{G7Text, g7_fmt, g7_text};
use crate::panels_image::{BitmapImage, trace_preset_name};

use super::g5::{G5Control, G5Line};

/// Longest side (pixels) of the picture of the A/B comparison.
const COMPARE_PX: u32 = 280;
/// Radius shown in the brush field before the user types one.
const DEFAULT_BRUSH_PX: &str = "10";

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

fn field(control: G5Control, caption: String, text: String) -> G5Line {
    G5Line::Field { control, caption, text }
}

fn button(control: G5Control, title: String, enabled: bool) -> G5Line {
    G5Line::Button { control, title, enabled }
}

/// The name of a field the golf-7 sections introduced (the older ones are named in `g6_werk`).
pub(super) fn field_name(field: PrepField, locale: Locale) -> String {
    match field {
        PrepField::LensK1 => t(G7Text::FieldLensK1, locale),
        PrepField::LensK2 => t(G7Text::FieldLensK2, locale),
        PrepField::LensP1 => t(G7Text::FieldLensP1, locale),
        PrepField::LensP2 => t(G7Text::FieldLensP2, locale),
        PrepField::NodeLimit => t(G7Text::FieldNodeLimit, locale),
        PrepField::NodeTolerance => t(G7Text::FieldNodeTolerance, locale),
        PrepField::BrushRadius => t(G7Text::FieldBrushRadius, locale),
        _ => String::new(),
    }
}

/// The words for a refusal the golf-7 actions introduced; `None` for the older ones.
pub(super) fn issue_text(issue: PrepIssue, locale: Locale) -> Option<String> {
    Some(t(
        match issue {
            PrepIssue::UnknownContour => G7Text::IssueUnknownContour,
            PrepIssue::UnknownRemoved => G7Text::IssueUnknownRemoved,
            PrepIssue::PickNotStarted => G7Text::IssuePickNotStarted,
            PrepIssue::InvalidStroke => G7Text::IssueInvalidStroke,
            PrepIssue::CornersTooClose => G7Text::IssueCornersTooClose,
            PrepIssue::NothingToProtect => G7Text::IssueNothingToProtect,
            PrepIssue::UnknownCorner => G7Text::IssueUnknownCorner,
            _ => return None,
        },
        locale,
    ))
}

fn recipe_issue_text(issue: RecipeIssue, locale: Locale) -> String {
    t(
        match issue {
            RecipeIssue::EmptyName => G7Text::RecipeIssueEmptyName,
            RecipeIssue::NameTooLong => G7Text::RecipeIssueNameTooLong,
            RecipeIssue::TooMany => G7Text::RecipeIssueTooMany,
            RecipeIssue::Unknown => G7Text::RecipeIssueUnknown,
            RecipeIssue::NeedsContourMode => G7Text::RecipeIssueContourOnly,
            RecipeIssue::NeedsScale => G7Text::RecipeIssueNeedsScale,
            RecipeIssue::NoFiles => G7Text::RecipeIssueNoFiles,
            _ => G7Text::RecipeIssueInvalid,
        },
        locale,
    )
}

// --- The sections --------------------------------------------------------------------------------

/// The golf-7 sections, appended to the lines of the preparation panel.
pub(super) fn lines(doc: &DocumentViewModel, p: &TracePrepViewModel, locale: Locale) -> Vec<G5Line> {
    let mut lines = Vec::new();
    lens_lines(p, locale, &mut lines);
    pick_lines(p, locale, &mut lines);
    contour_lines(p, locale, &mut lines);
    if let Some(r) = &doc.trace_recipes {
        recipe_lines(r, locale, &mut lines);
    }
    lines
}

fn lens_lines(p: &TracePrepViewModel, locale: Locale, out: &mut Vec<G5Line>) {
    out.push(G5Line::Heading(t(G7Text::LensHeading, locale)));
    out.push(G5Line::Text(t(G7Text::LensHint, locale)));
    out.push(G5Line::Text(if p.lens.on && (p.lens.p1 != "0" || p.lens.p2 != "0") {
        g7_fmt(G7Text::LensOnTangential, locale, &[&p.lens.k1, &p.lens.k2, &p.lens.p1, &p.lens.p2])
    } else if p.lens.on {
        g7_fmt(G7Text::LensOn, locale, &[&p.lens.k1, &p.lens.k2])
    } else {
        t(G7Text::LensOff, locale)
    }));
    out.push(field(G5Control::LensK1, t(G7Text::LensK1Caption, locale), p.lens.k1.clone()));
    out.push(field(G5Control::LensK2, t(G7Text::LensK2Caption, locale), p.lens.k2.clone()));
    out.push(field(G5Control::LensP1, t(G7Text::LensP1Caption, locale), p.lens.p1.clone()));
    out.push(field(G5Control::LensP2, t(G7Text::LensP2Caption, locale), p.lens.p2.clone()));
    out.push(button(G5Control::LensApply, t(G7Text::LensApply, locale), true));
    out.push(button(G5Control::LensClear, t(G7Text::LensClear, locale), p.lens.on));
}

fn pick_lines(p: &TracePrepViewModel, locale: Locale, out: &mut Vec<G5Line>) {
    out.push(G5Line::Heading(t(G7Text::PickHeading, locale)));
    out.push(G5Line::Text(t(G7Text::PickHint, locale)));
    out.push(G5Line::Text(match p.pick.as_ref().map(|k| &k.kind) {
        None => t(G7Text::PickNone, locale),
        Some(PickKind::Corners { picked }) => g7_fmt(G7Text::PickCorners, locale, &[&picked.len().to_string()]),
        Some(PickKind::ProtectContours) => t(G7Text::PickProtectContours, locale),
        Some(PickKind::ProtectCorners) => t(G7Text::PickProtectCorners, locale),
        Some(PickKind::Brush { mode, radius_px }) => g7_fmt(
            if *mode == MaskMode::Keep { G7Text::PickBrushKeep } else { G7Text::PickBrushRemove },
            locale,
            &[&num(*radius_px, 1, locale)],
        ),
    }));
    out.push(button(G5Control::PickCorners, t(G7Text::PickCornersButton, locale), true));
    out.push(field(G5Control::BrushRadius, t(G7Text::BrushRadiusCaption, locale), DEFAULT_BRUSH_PX.to_owned()));
    out.push(button(G5Control::PickBrushKeep, t(G7Text::PickBrushKeepButton, locale), true));
    out.push(button(G5Control::PickBrushRemove, t(G7Text::PickBrushRemoveButton, locale), true));
    // Protecting works on the contours of the result in force.
    let protectable = !p.contours.is_empty();
    out.push(button(G5Control::PickProtectContours, t(G7Text::PickProtectContoursButton, locale), protectable));
    out.push(button(G5Control::PickProtectCorners, t(G7Text::PickProtectCornersButton, locale), protectable));
    for (i, c) in p.corners.iter().enumerate() {
        let n = (i + 1).to_string();
        out.push(G5Line::Text(g7_fmt(
            G7Text::CornerLine,
            locale,
            &[&n, &num(c.x_mm, 2, locale), &num(c.y_mm, 2, locale)],
        )));
        out.push(button(G5Control::CornerRemove { index: i }, g7_fmt(G7Text::CornerRemoveButton, locale, &[&n]), true));
    }
    out.push(button(G5Control::PickEnd, t(G7Text::PickEndButton, locale), p.pick.is_some()));
}

fn contour_lines(p: &TracePrepViewModel, locale: Locale, out: &mut Vec<G5Line>) {
    if !p.contours.is_empty() {
        out.push(G5Line::Heading(t(G7Text::ContoursHeading, locale)));
        out.push(G5Line::Text(t(G7Text::ContoursHint, locale)));
        for c in &p.contours {
            let id = c.id.to_string();
            let kind = t(if c.hole { G7Text::ContourHole } else { G7Text::ContourPart }, locale);
            out.push(G5Line::Text(g7_fmt(
                G7Text::ContourLine,
                locale,
                &[&id, &kind, &num(c.area_mm2, 2, locale), &num(c.x_mm, 1, locale), &num(c.y_mm, 1, locale)],
            )));
            out.push(G5Line::Check {
                control: G5Control::ContourProtect { id: c.id },
                caption: g7_fmt(G7Text::ContourProtectCaption, locale, &[&id]),
                on: c.protected,
            });
            out.push(G5Line::Check {
                control: G5Control::ContourKeep { id: c.id },
                caption: g7_fmt(G7Text::ContourKeepCaption, locale, &[&id]),
                on: c.kept,
            });
        }
    }
    out.push(G5Line::Heading(t(G7Text::NodeHeading, locale)));
    match &p.node_target {
        Some(n) => out.push(G5Line::Text(g7_fmt(G7Text::NodeSet, locale, &[&n.max_nodes, &n.max_tolerance_mm]))),
        None => out.push(G5Line::Text(t(G7Text::NodeNone, locale))),
    }
    let (nodes, tolerance) = p
        .node_target
        .as_ref()
        .map_or((String::new(), String::new()), |n| (n.max_nodes.clone(), n.max_tolerance_mm.clone()));
    out.push(field(G5Control::NodeLimit, t(G7Text::NodeLimitCaption, locale), nodes));
    out.push(field(G5Control::NodeTolerance, t(G7Text::NodeToleranceCaption, locale), tolerance));
    out.push(button(G5Control::NodeApply, t(G7Text::NodeApply, locale), true));
    for (i, c) in p.corrections.iter().enumerate() {
        if c.protected == 0 && c.node_goal.is_none() {
            continue;
        }
        let goal = match c.node_goal {
            Some(g) => g7_fmt(
                if g.met { G7Text::CorrectionGoalMet } else { G7Text::CorrectionGoalMissed },
                locale,
                &[&g.max_nodes.to_string(), &g.nodes.to_string(), &num(g.tolerance_mm, 3, locale)],
            ),
            None => String::new(),
        };
        let line = g7_fmt(G7Text::CorrectionExtras, locale, &[&(i + 1).to_string(), &c.protected.to_string(), &goal]);
        let missed = c.node_goal.is_some_and(|g| !g.met);
        out.push(if missed { G5Line::Problem(line) } else { G5Line::Text(line) });
    }
    if let Some(report) = p.report.as_ref().filter(|r| !r.removed.is_empty()) {
        out.push(G5Line::Heading(t(G7Text::RestoreHeading, locale)));
        for (i, f) in report.removed.iter().enumerate() {
            let n = (i + 1).to_string();
            let kind = t(if f.hole { G7Text::ContourHole } else { G7Text::ContourPart }, locale);
            out.push(G5Line::Text(g7_fmt(
                G7Text::RestoreLine,
                locale,
                &[&n, &kind, &num(f.area_mm2, 3, locale), &num(f.x_mm, 2, locale), &num(f.y_mm, 2, locale)],
            )));
            out.push(button(
                G5Control::RestoreRemoved { index: i },
                g7_fmt(G7Text::RestoreButton, locale, &[&n]),
                true,
            ));
        }
    }
}

fn variant_line(text: G7Text, v: &VariantSummary, locale: Locale) -> String {
    g7_fmt(
        text,
        locale,
        &[
            &v.paths.to_string(),
            &v.closed_shapes.to_string(),
            &v.holes.to_string(),
            &v.nodes.to_string(),
            &v.removed.to_string(),
            &num(v.max_deviation_mm, 3, locale),
        ],
    )
}

fn choice_name(choice: &RecipeChoice, locale: Locale) -> String {
    match choice {
        RecipeChoice::Saved(name) => name.clone(),
        RecipeChoice::Preset(preset) => trace_preset_name(*preset, locale),
    }
}

fn recipe_lines(r: &TraceRecipesViewModel, locale: Locale, out: &mut Vec<G5Line>) {
    out.push(G5Line::Heading(t(G7Text::RecipesHeading, locale)));
    out.push(G5Line::Text(t(G7Text::RecipesHint, locale)));
    if r.load_problem {
        out.push(G5Line::Problem(t(G7Text::RecipeLoadProblem, locale)));
    }
    if r.save_failed {
        out.push(G5Line::Problem(t(G7Text::RecipeSaveFailed, locale)));
    }
    if r.saved.is_empty() {
        out.push(G5Line::Text(t(G7Text::RecipesNone, locale)));
    }
    for (index, saved) in r.saved.iter().enumerate() {
        let mode = t(
            if saved.mode == burnburn_presentation::TraceMode::Centerline {
                G7Text::RecipeCentreLine
            } else {
                G7Text::RecipeContour
            },
            locale,
        );
        out.push(G5Line::Text(g7_fmt(G7Text::RecipeLine, locale, &[&saved.name, &mode])));
        out.push(button(G5Control::RecipeApply { index }, g7_fmt(G7Text::RecipeApply, locale, &[&saved.name]), true));
        out.push(button(
            G5Control::RecipeCompare { index },
            g7_fmt(G7Text::RecipeCompare, locale, &[&saved.name]),
            true,
        ));
        out.push(button(G5Control::RecipeDelete { index }, g7_fmt(G7Text::RecipeDelete, locale, &[&saved.name]), true));
    }
    out.push(field(G5Control::RecipeName, t(G7Text::RecipeNameCaption, locale), String::new()));
    out.push(button(G5Control::RecipeSave, t(G7Text::RecipeSave, locale), true));
    out.push(G5Line::Choice {
        control: G5Control::RecipePreset,
        caption: t(G7Text::RecipePresetCaption, locale),
        options: TracePreset::ALL.iter().map(|p| trace_preset_name(*p, locale)).collect(),
        selected: 0,
    });
    out.push(button(G5Control::RecipeComparePreset, t(G7Text::RecipeComparePreset, locale), true));
    if let Some(c) = &r.compare {
        compare_lines(c, locale, out);
    }

    out.push(G5Line::Heading(t(G7Text::BatchHeading, locale)));
    out.push(G5Line::Text(t(G7Text::BatchHint, locale)));
    out.push(G5Line::Memo { control: G5Control::BatchFiles, text: String::new(), max_chars: 20_000 });
    let running = r.batch.as_ref().is_some_and(|b| b.running);
    out.push(button(G5Control::BatchChoose, t(G7Text::BatchChoose, locale), !running));
    out.push(button(G5Control::BatchRun, t(G7Text::BatchRun, locale), !running));
    out.push(button(G5Control::BatchCancel, t(G7Text::BatchCancel, locale), running));
    if let Some(b) = &r.batch {
        if b.running {
            out.push(G5Line::Text(g7_fmt(G7Text::BatchRunning, locale, &[&b.total.to_string()])));
        } else if b.cancelled {
            out.push(G5Line::Text(t(G7Text::BatchCancelled, locale)));
        } else {
            out.push(G5Line::Text(g7_fmt(
                G7Text::BatchSummary,
                locale,
                &[&b.rows.len().to_string(), &b.problem_count().to_string()],
            )));
        }
        for row in &b.rows {
            out.push(G5Line::Text(match &row.summary {
                Some(s) => g7_fmt(
                    G7Text::BatchRow,
                    locale,
                    &[&row.name, &s.closed_shapes.to_string(), &s.holes.to_string(), &s.nodes.to_string()],
                ),
                None => g7_fmt(G7Text::BatchRowNone, locale, &[&row.name]),
            }));
            for problem in &row.problems {
                let text = match problem {
                    BatchProblemView::NoGeometry => t(G7Text::BatchNoGeometry, locale),
                    BatchProblemView::RemovedFeatures { count } => {
                        g7_fmt(G7Text::BatchRemoved, locale, &[&count.to_string()])
                    }
                    BatchProblemView::NeedsScale => t(G7Text::BatchNeedsScale, locale),
                    BatchProblemView::Unreadable => t(G7Text::BatchUnreadable, locale),
                    _ => t(G7Text::BatchFailed, locale),
                };
                out.push(G5Line::Problem(text));
            }
        }
        out.push(button(G5Control::BatchClear, t(G7Text::BatchClear, locale), !b.running));
    }
    if let Some(issue) = r.issue {
        out.push(G5Line::Problem(recipe_issue_text(issue, locale)));
    }
}

fn compare_lines(c: &CompareViewModel, locale: Locale, out: &mut Vec<G5Line>) {
    out.push(G5Line::Heading(t(G7Text::CompareHeading, locale)));
    out.push(G5Line::Text(g7_fmt(G7Text::CompareWith, locale, &[&choice_name(&c.b_choice, locale)])));
    if c.computing {
        out.push(G5Line::Text(t(G7Text::CompareComputing, locale)));
    }
    if let Some(a) = &c.a {
        out.push(G5Line::Text(variant_line(G7Text::CompareA, a, locale)));
    }
    if let Some(b) = &c.b {
        out.push(G5Line::Text(variant_line(G7Text::CompareB, b, locale)));
    }
    if c.a.is_some() {
        out.push(G5Line::Text(t(if c.same_geometry { G7Text::CompareSame } else { G7Text::CompareDiffer }, locale)));
    }
    if let Some(k) = &c.corrections {
        out.push(G5Line::Text(g7_fmt(
            G7Text::CompareCorrections,
            locale,
            &[&k.stored.to_string(), &k.a_applied.to_string(), &k.b_applied.to_string()],
        )));
    }
    for row in &c.b_corrections {
        let n = (row.index + 1).to_string();
        let (line, button_title) = match row.state {
            CompareCorrectionState::Unbound { lost, new_in_region } => (
                g7_fmt(G7Text::CompareUnbound, locale, &[&n, &lost.to_string(), &new_in_region.to_string()]),
                g7_fmt(G7Text::CompareRelinkButton, locale, &[&n]),
            ),
            CompareCorrectionState::Relinked => {
                (g7_fmt(G7Text::CompareRelinked, locale, &[&n]), g7_fmt(G7Text::CompareUnlinkButton, locale, &[&n]))
            }
        };
        out.push(G5Line::Text(line));
        out.push(button(G5Control::CompareRelink { index: row.index }, button_title, true));
    }
    if c.a_outline.is_some() && c.b_outline.is_some() {
        out.push(G5Line::Text(t(G7Text::CompareOnSheet, locale)));
    }
    if let (Some(a), Some(b)) = (&c.a_outline, &c.b_outline)
        && let Some(image) = compare_bitmap(a, b, COMPARE_PX)
    {
        out.push(G5Line::Image(image));
    }
    out.push(button(G5Control::CompareAdopt, t(G7Text::CompareAdopt, locale), c.b.is_some()));
    out.push(button(G5Control::CompareClear, t(G7Text::CompareClear, locale), true));
}

/// Both outlines in the frame of their union, A black and B red on white, at one scale (so the
/// two can be judged against each other). `None` when there is nothing to draw.
pub fn compare_bitmap(a: &Path, b: &Path, max_px: u32) -> Option<BitmapImage> {
    let (ba, bb) = (a.bounds(), b.bounds());
    let bounds = match (ba, bb) {
        (Some(x), Some(y)) => x.union(y),
        (Some(x), None) | (None, Some(x)) => x,
        (None, None) => return None,
    };
    let margin = 4u32;
    let inner = max_px.checked_sub(2 * margin + 1).filter(|n| *n > 0)?;
    let extent = bounds.width().max(bounds.height());
    if !(extent.is_finite() && extent > 0.0) {
        return None;
    }
    let scale = f64::from(inner) / extent;
    let tolerance = burnburn_geometry::DisplayTolerance::new(
        burnburn_types::Length::from_mm(extent / f64::from(max_px) / 4.0).ok()?,
    )
    .ok()?;
    let width = (bounds.width() * scale).round() as u32 + 2 * margin + 1;
    let height = (bounds.height() * scale).round() as u32 + 2 * margin + 1;
    let mut rgba = vec![255u8; width as usize * height as usize * 4];
    let to_px = |p: burnburn_geometry::Point| {
        (
            i64::from(margin) + ((p.x - bounds.min().x) * scale).round() as i64,
            i64::from(margin) + ((bounds.max().y - p.y) * scale).round() as i64,
        )
    };
    for (path, colour) in [(a, [0u8, 0, 0]), (b, [220u8, 30, 30])] {
        let mut plot = |x: i64, y: i64| {
            if let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y))
                && x < width
                && y < height
            {
                let i = (y as usize * width as usize + x as usize) * 4;
                rgba[i..i + 3].copy_from_slice(&colour);
            }
        };
        for line in path.flatten(&tolerance) {
            let mut points: Vec<_> = line.points.iter().map(|p| to_px(*p)).collect();
            if line.closed
                && let Some(first) = points.first().copied()
            {
                points.push(first);
            }
            if let [only] = points[..] {
                plot(only.0, only.1);
            }
            for pair in points.windows(2) {
                segment(pair[0], pair[1], &mut plot);
            }
        }
    }
    Some(BitmapImage { width, height, rgba })
}

/// Bresenham between two pixels.
fn segment(from: (i64, i64), to: (i64, i64), plot: &mut impl FnMut(i64, i64)) {
    let (mut x, mut y) = from;
    let (dx, dy) = ((to.0 - x).abs(), -(to.1 - y).abs());
    let (sx, sy) = (if x < to.0 { 1 } else { -1 }, if y < to.1 { 1 } else { -1 });
    let mut err = dx + dy;
    loop {
        plot(x, y);
        if (x, y) == to {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

// --- Controls to intents --------------------------------------------------------------------------

fn value_of(values: &[(G5Control, String)], control: G5Control) -> Option<&str> {
    values.iter().find(|(c, _)| *c == control).map(|(_, v)| v.as_str())
}

/// The intent of a control of the golf-7 sections. Fields and choices only hold text; a button
/// sends it all, unparsed, as one intent; a check sends its new state.
pub(super) fn intent(
    doc: &DocumentViewModel,
    document: DocumentId,
    control: G5Control,
    values: &[(G5Control, String)],
) -> Option<UiIntent> {
    let typed = |c: G5Control, shown: &str| value_of(values, c).unwrap_or(shown).to_owned();
    let prep = |intent: TracePrepIntent| Some(UiIntent::TracePrep { document, intent });
    let pick = |pick: TracePick| Some(UiIntent::TracePick { document, pick });
    let recipe = |intent: TraceRecipeIntent| Some(UiIntent::TraceRecipe { document, intent });
    let saved = |index: usize| doc.trace_recipes.as_ref().and_then(|r| r.saved.get(index)).map(|s| s.name.clone());
    let p = doc.trace_prep.as_ref()?;
    match control {
        G5Control::LensApply => prep(TracePrepIntent::UseLens {
            k1: typed(G5Control::LensK1, &p.lens.k1),
            k2: typed(G5Control::LensK2, &p.lens.k2),
            p1: typed(G5Control::LensP1, &p.lens.p1),
            p2: typed(G5Control::LensP2, &p.lens.p2),
        }),
        G5Control::LensClear => {
            p.lens.on.then_some(UiIntent::TracePrep { document, intent: TracePrepIntent::ClearLens })
        }
        G5Control::PickCorners => pick(TracePick::BeginCorners),
        G5Control::PickBrushKeep | G5Control::PickBrushRemove => pick(TracePick::BeginBrushTyped {
            mode: if control == G5Control::PickBrushKeep { MaskMode::Keep } else { MaskMode::Remove },
            radius: typed(G5Control::BrushRadius, DEFAULT_BRUSH_PX),
        }),
        G5Control::PickProtectContours => pick(TracePick::BeginProtectContours),
        G5Control::PickProtectCorners => pick(TracePick::BeginProtectCorners),
        G5Control::CornerRemove { index } => prep(TracePrepIntent::RemoveProtectedCorner { index }),
        G5Control::PickEnd => p.pick.is_some().then_some(UiIntent::TracePick { document, pick: TracePick::End }),
        G5Control::ContourProtect { id } => {
            let on = value_of(values, control).map(|v| v == "1")?;
            prep(TracePrepIntent::ProtectContour { id, on })
        }
        G5Control::ContourKeep { id } => {
            let keep = value_of(values, control).map(|v| v == "1")?;
            prep(TracePrepIntent::KeepContour { id, keep })
        }
        G5Control::NodeApply => prep(TracePrepIntent::SetNodeTarget {
            max_nodes: typed(G5Control::NodeLimit, ""),
            max_tolerance_mm: typed(G5Control::NodeTolerance, ""),
        }),
        G5Control::RestoreRemoved { index } => prep(TracePrepIntent::RestoreRemoved { index }),
        G5Control::RecipeSave => recipe(TraceRecipeIntent::Save { name: typed(G5Control::RecipeName, "") }),
        G5Control::RecipeApply { index } => recipe(TraceRecipeIntent::Apply { name: saved(index)? }),
        G5Control::RecipeDelete { index } => recipe(TraceRecipeIntent::Delete { name: saved(index)? }),
        G5Control::RecipeCompare { index } => {
            recipe(TraceRecipeIntent::Compare { with: RecipeChoice::Saved(saved(index)?) })
        }
        G5Control::RecipeComparePreset => {
            let index =
                value_of(values, G5Control::RecipePreset).and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(0);
            recipe(TraceRecipeIntent::Compare { with: RecipeChoice::Preset(*TracePreset::ALL.get(index)?) })
        }
        G5Control::CompareAdopt => recipe(TraceRecipeIntent::AdoptCompared),
        G5Control::CompareClear => recipe(TraceRecipeIntent::ClearCompare),
        G5Control::CompareRelink { index } => {
            // The button's meaning follows the row: an unbound correction is linked, a linked one is taken back.
            let rows = &doc.trace_recipes.as_ref()?.compare.as_ref()?.b_corrections;
            let on = matches!(rows.iter().find(|r| r.index == index)?.state, CompareCorrectionState::Unbound { .. });
            recipe(TraceRecipeIntent::RelinkCorrection { index, on })
        }
        G5Control::BatchRun => {
            let paths: Vec<PathBuf> = typed(G5Control::BatchFiles, "")
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(PathBuf::from)
                .collect();
            recipe(TraceRecipeIntent::RunBatch { paths })
        }
        // Choosing files is the host's: it shows the panel and sends `batch_files_intent`.
        G5Control::BatchChoose => None,
        G5Control::BatchCancel => recipe(TraceRecipeIntent::CancelBatch),
        G5Control::BatchClear => recipe(TraceRecipeIntent::ClearBatch),
        _ => None,
    }
}

/// The batch that the files chosen in the host's panel are: each file once (its first place), in the
/// order given. `None` when nothing was chosen (Cancel).
pub fn batch_files_intent(document: DocumentId, paths: Vec<PathBuf>) -> Option<UiIntent> {
    let mut unique: Vec<PathBuf> = Vec::with_capacity(paths.len());
    for path in paths {
        if !unique.contains(&path) {
            unique.push(path);
        }
    }
    (!unique.is_empty())
        .then_some(UiIntent::TraceRecipe { document, intent: TraceRecipeIntent::RunBatch { paths: unique } })
}
