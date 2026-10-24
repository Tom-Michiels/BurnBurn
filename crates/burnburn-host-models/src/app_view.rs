//! What the app-level panels show (W3-UI-app), decided without AppKit: preference rows, shortcut
//! lines, history lines, the preview's texts and bitmap, and the RD listing.

use burnburn_presentation::{
    HistoryViewModel, LengthUnit, MessageKey, MoveKind, PlaybackSpeed, PrefKey, PrefProblem, PrefUnit, PrefValue,
    Preferences, PrefsViewModel, PreviewViewModel, RdInspectViewModel, ShortcutProblem, ShortcutTarget,
    ShortcutsViewModel,
};

use crate::app_text::{AppText, app_text};
use crate::format::{LengthPreference, format_length};
use crate::l10n::{HostText, Locale, host_text, message_or_key, undo_label_text};
pub use crate::preview_draw::{
    LegendEntry, PLAYED_OPACITY, PreviewStyle, Swatch, event_at_seconds, preview_legend, render_preview,
    render_preview_styled, render_swatch,
};

// ---- preferences ------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrefsControl {
    Choice { titles: Vec<String>, ids: Vec<&'static str>, selected: usize },
    Check(bool),
    Field { text: String, suffix: String, invalid: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefsPanelRow {
    pub key: PrefKey,
    pub caption: String,
    pub control: PrefsControl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefsPanel {
    pub rows: Vec<PrefsPanelRow>,
    /// The one notice above the buttons: a refused value, else a read problem, else a write problem.
    pub banner: Option<String>,
}

fn pref_caption(key: PrefKey) -> AppText {
    match key {
        PrefKey::Units => AppText::PrefUnits,
        PrefKey::Language => AppText::PrefLanguage,
        PrefKey::AutosaveSeconds => AppText::PrefAutosave,
        PrefKey::SnapToObjects => AppText::PrefSnapObjects,
        PrefKey::SnapToGrid => AppText::PrefSnapGrid,
        PrefKey::GridSnap => AppText::PrefGrid,
        PrefKey::ObjectSnapPixels => AppText::PrefObjectPx,
        PrefKey::NudgeSmall => AppText::PrefNudgeSmall,
        PrefKey::NudgeNormal => AppText::PrefNudgeNormal,
        PrefKey::NudgeLarge => AppText::PrefNudgeLarge,
        PrefKey::Quality => AppText::PrefQuality,
        PrefKey::DesktopAutomation => AppText::PrefDesktopAutomation,
    }
}

fn choice_title(id: &str, locale: Locale) -> String {
    let text = match id {
        "mm" => AppText::UnitMm,
        "inch" => AppText::UnitInch,
        "system" => AppText::LangSystem,
        "nl" => AppText::LangNl,
        "en" => AppText::LangEn,
        "coarse" => AppText::QualityCoarse,
        "smooth" => AppText::QualitySmooth,
        other => return other.to_owned(),
    };
    app_text(text, locale).to_owned()
}

/// The stable id of item `index` of a choice preference's pop-up, as presentation lists them.
pub fn pref_choice_id(key: PrefKey, index: usize) -> Option<&'static str> {
    match Preferences::default().row(key).value {
        PrefValue::Choice { options, .. } => options.get(index).copied(),
        _ => None,
    }
}

fn problem_text(problem: PrefProblem, locale: Locale) -> &'static str {
    app_text(
        match problem {
            PrefProblem::NotANumber => AppText::PrefNotANumber,
            PrefProblem::OutOfRange => AppText::PrefOutOfRange,
            PrefProblem::UnknownChoice => AppText::PrefUnknownChoice,
        },
        locale,
    )
}

pub fn prefs_panel(view: &PrefsViewModel, locale: Locale) -> PrefsPanel {
    let rows = view
        .rows
        .iter()
        .map(|row| {
            let control = match &row.value {
                PrefValue::Choice { selected, options } => PrefsControl::Choice {
                    titles: options.iter().map(|id| choice_title(id, locale)).collect(),
                    ids: options.to_vec(),
                    selected: options.iter().position(|id| id == selected).unwrap_or(0),
                },
                PrefValue::Flag(on) => PrefsControl::Check(*on),
                PrefValue::Number { text, unit } => PrefsControl::Field {
                    text: text.clone(),
                    suffix: app_text(
                        match (unit, view.unit) {
                            (PrefUnit::Length, LengthUnit::Millimeters) => AppText::SuffixMm,
                            (PrefUnit::Length, LengthUnit::Inches) => AppText::SuffixInch,
                            (PrefUnit::Pixels, _) => AppText::SuffixPx,
                            (PrefUnit::Seconds, _) => AppText::SuffixSeconds,
                        },
                        locale,
                    )
                    .to_owned(),
                    invalid: view.error.is_some_and(|e| e.key == row.key),
                },
            };
            PrefsPanelRow { key: row.key, caption: app_text(pref_caption(row.key), locale).to_owned(), control }
        })
        .collect();
    let banner = match (view.error, view.load_problem, view.save_failed) {
        (Some(e), _, _) => Some(problem_text(e.problem, locale).to_owned()),
        (None, true, _) => Some(app_text(AppText::PrefsLoadProblem, locale).to_owned()),
        (None, false, true) => Some(app_text(AppText::PrefsSaveFailed, locale).to_owned()),
        _ => None,
    };
    PrefsPanel { rows, banner }
}

// ---- shortcuts --------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutLine {
    pub id: String,
    pub name: String,
    pub key_text: String,
    pub default_text: String,
    pub customized: bool,
}

/// The name of a host action as its menu item reads.
fn host_name(name: &str, locale: Locale) -> String {
    use crate::panels::{LaserText, laser_text};
    let text = match name {
        "open" => return host_text(HostText::Open, locale).to_owned(),
        "import" => return host_text(HostText::Import, locale).to_owned(),
        "import_image" => return host_text(HostText::ImageMenuImport, locale).to_owned(),
        "trace_image" => return host_text(HostText::ImageMenuTrace, locale).to_owned(),
        "laser_window" => return laser_text(LaserText::MenuWindowItem, locale).to_owned(),
        "export_diagnostics" => return laser_text(LaserText::DiagnosticsItem, locale).to_owned(),
        "preferences" => AppText::Preferences,
        "shortcuts" => AppText::Shortcuts,
        "history" => AppText::UndoHistory,
        "preview" => AppText::Preview,
        "inspect_rd" => AppText::InspectRd,
        "about" => AppText::About,
        "manual" => AppText::Manual,
        "support_bundle" => AppText::SupportData,
        "debug_log" => AppText::DebugLog,
        other => return other.to_owned(),
    };
    app_text(text, locale).to_owned()
}

fn row_name(target: &ShortcutTarget, locale: Locale) -> String {
    match target {
        ShortcutTarget::Command(key) => message_or_key(MessageKey::Command(*key), locale),
        ShortcutTarget::Tool(id) => crate::toolbar::tool_label(id, locale),
        ShortcutTarget::Host(name) => host_name(name, locale),
    }
}

/// How a host draws a key binding (macOS: `⌘S`, Windows: `Ctrl+S`); the one host-specific part.
pub type BindingGlyphs = fn(&burnburn_presentation::KeyBinding) -> String;

fn key_text(binding: Option<&burnburn_presentation::KeyBinding>, glyphs: BindingGlyphs, locale: Locale) -> String {
    binding.map_or_else(|| app_text(AppText::ShortcutNone, locale).to_owned(), glyphs)
}

/// One line per register entry, sorted by name; `filter` keeps the names that contain it. The hosts
/// call this through their `app_view::shortcut_lines`, which passes their own `binding_glyphs`.
pub fn shortcut_lines_with(
    view: &ShortcutsViewModel,
    filter: &str,
    glyphs: BindingGlyphs,
    locale: Locale,
) -> Vec<ShortcutLine> {
    let needle = filter.trim().to_lowercase();
    let mut lines: Vec<ShortcutLine> = view
        .rows
        .iter()
        .map(|row| ShortcutLine {
            id: row.id.clone(),
            name: row_name(&row.target, locale),
            key_text: key_text(row.binding.as_ref(), glyphs, locale),
            default_text: key_text(row.default.as_ref(), glyphs, locale),
            customized: row.customized,
        })
        .filter(|line| !line.name.is_empty() && line.name.to_lowercase().contains(&needle))
        .collect();
    lines.sort_by_key(|l| (l.name.to_lowercase(), l.id.clone()));
    lines
}

/// Why a shortcut change was refused, in words; `attempted` is the binding as drawn.
pub fn shortcut_problem_text(
    problem: &ShortcutProblem,
    attempted: &str,
    view: &ShortcutsViewModel,
    locale: Locale,
) -> String {
    match problem {
        ShortcutProblem::Conflict { with } => {
            let owner = view.row(with).map(|r| row_name(&r.target, locale)).unwrap_or_else(|| with.clone());
            format!("{attempted} {} {owner}", app_text(AppText::ShortcutConflict, locale))
        }
        ShortcutProblem::Reserved => app_text(AppText::ShortcutReserved, locale).to_owned(),
        ShortcutProblem::NeedsModifier => app_text(AppText::ShortcutNeedsModifier, locale).to_owned(),
        ShortcutProblem::Unparsable | ShortcutProblem::UnknownId => {
            app_text(AppText::ShortcutUnknown, locale).to_owned()
        }
    }
}

// ---- history ----------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryLine {
    pub text: String,
    /// The document is in the state this line stands for.
    pub current: bool,
    pub undone: bool,
}

/// Row 0 is the start; row `n` the state after `n` steps (the row's number is the jump position).
pub fn history_lines(view: &HistoryViewModel, locale: Locale) -> Vec<HistoryLine> {
    let marker = app_text(AppText::HistoryCurrent, locale);
    let undone = app_text(AppText::HistoryUndone, locale);
    let mut lines = vec![HistoryLine {
        text: if view.position == 0 {
            format!("{} {marker}", app_text(AppText::HistoryStart, locale))
        } else {
            app_text(AppText::HistoryStart, locale).to_owned()
        },
        current: view.position == 0,
        undone: false,
    }];
    for (i, row) in view.rows.iter().enumerate() {
        let name = undo_label_text(row.label, locale).map_or_else(|| format!("{:?}", row.label), str::to_owned);
        let current = i + 1 == view.position;
        let text = match (row.applied, current) {
            (true, true) => format!("{name} {marker}"),
            (true, false) => name,
            (false, _) => format!("{name} {undone}"),
        };
        lines.push(HistoryLine { text, current, undone: !row.applied });
    }
    lines
}

// ---- preview ----------------------------------------------------------------------------------

/// `m:ss`, or `h:mm:ss` from an hour on; negative and non-finite are `0:00`.
pub fn clock_text(seconds: f64) -> String {
    let whole = if seconds.is_finite() && seconds > 0.0 { seconds.floor() as u64 } else { 0 };
    let (h, m, s) = (whole / 3600, whole / 60 % 60, whole % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PreviewPanel {
    pub plan_text: String,
    /// Always "Time m:ss of ≈ m:ss (estimate)": the clock is an estimate, never a measurement.
    pub time_text: String,
    pub play_title: &'static str,
    pub speed_titles: Vec<String>,
    pub speed_index: usize,
    pub show_travel: bool,
    pub stale: Option<String>,
    pub picked: Option<String>,
    pub picked_hint: String,
    /// The legend's words in reading order (the swatches are [`preview_legend`]).
    pub legend: String,
    /// Slider range and value in estimated seconds.
    pub slider_max: f64,
    pub slider_value: f64,
    /// Playback runs: the play button shows the pause symbol.
    pub playing: bool,
    /// "Cut 1,2 m · Scan … · Travel …" in the preferred length unit.
    pub lengths_text: String,
}

/// Names the host knows for the picked move's source, so the line reads "Rechthoek 1" and not a number.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PickNames {
    pub geometry: Option<String>,
    pub operation: Option<String>,
}

pub fn preview_panel(view: &PreviewViewModel, locale: Locale) -> PreviewPanel {
    preview_panel_named(view, &PickNames::default(), locale)
}

pub fn preview_panel_named(view: &PreviewViewModel, names: &PickNames, locale: Locale) -> PreviewPanel {
    preview_panel_units(view, names, locale, LengthPreference::Millimeters)
}

/// The preview's texts with lengths in `unit`.
pub fn preview_panel_units(
    view: &PreviewViewModel,
    names: &PickNames,
    locale: Locale,
    unit: LengthPreference,
) -> PreviewPanel {
    let t = |text| app_text(text, locale);
    let picked = view.selected.map(|p| {
        let kind = t(match p.kind {
            MoveKind::Cut => AppText::PreviewMoveCut,
            MoveKind::Scan => AppText::PreviewMoveScan,
            MoveKind::Travel => AppText::PreviewMoveTravel,
            MoveKind::Overscan => AppText::PreviewMoveOverscan,
            MoveKind::Dot => AppText::PreviewMoveDot,
        });
        let mut text = format!("{}: {kind}", t(AppText::PreviewPicked));
        if let Some(power) = p.power {
            // The share of the rated power this stretch is cut with (a tab with a lowered power).
            text.push_str(&format!(" {} {:.1} %", t(AppText::PreviewAtPower), power.get() * 100.0));
        }
        if let Some(source) = p.source {
            let geometry = names
                .geometry
                .clone()
                .unwrap_or_else(|| format!("#{}", burnburn_types::StableId::raw(source.geometry)));
            let operation = names
                .operation
                .clone()
                .unwrap_or_else(|| format!("#{}", burnburn_types::StableId::raw(source.operation)));
            text.push_str(&format!(
                " — {} {geometry} · {} {operation}",
                t(AppText::PreviewSource),
                t(AppText::PreviewOperation)
            ));
        }
        text
    });
    let length = |word: &str, mm: f64| format!("{word} {}", format_length(mm, unit, locale));
    let mut lengths = vec![length(t(AppText::PreviewLegendCut), view.preview.cut_length_mm)];
    if view.preview.scan_length_mm > 0.0 {
        lengths.push(length(t(AppText::PreviewLengthScan), view.preview.scan_length_mm));
    }
    lengths.push(length(t(AppText::PreviewLegendTravel), view.preview.travel_length_mm));
    let lengths_text = lengths.join(" · ");
    let id = view.plan.to_string();
    PreviewPanel {
        plan_text: format!("{} {}", t(AppText::PreviewPlanId), id.chars().take(21).collect::<String>()),
        time_text: format!(
            "{} {} {} ≈ {} {}",
            t(AppText::PreviewTime),
            clock_text(view.estimated_position_seconds),
            t(AppText::PreviewOf),
            clock_text(view.estimated_total_seconds),
            t(AppText::PreviewEstimate)
        ),
        play_title: t(if view.playing { AppText::PreviewPause } else { AppText::PreviewPlay }),
        speed_titles: PlaybackSpeed::ALL.iter().map(|s| format!("{}×", s.factor())).collect(),
        speed_index: PlaybackSpeed::ALL.iter().position(|s| *s == view.speed).unwrap_or(0),
        show_travel: view.show_travel,
        stale: view.stale.then(|| t(AppText::PreviewStale).to_owned()),
        picked,
        picked_hint: t(AppText::PreviewNothingPicked).to_owned(),
        legend: preview_legend(locale).iter().map(|e| e.word).collect::<Vec<_>>().join(" · "),
        slider_max: view.estimated_total_seconds,
        slider_value: view.estimated_position_seconds,
        playing: view.playing,
        lengths_text,
    }
}

/// The map from machine millimetres (y up) to the preview bitmap's pixels (y down).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewFrame {
    size: [u32; 2],
    min_mm: [f64; 2],
    max_mm: [f64; 2],
    scale: f64,
    offset_px: [f64; 2],
}

impl PreviewFrame {
    /// A frame showing `extent_mm` from the machine origin.
    pub fn fit(extent_mm: [f64; 2], size: [u32; 2], margin_px: u32) -> PreviewFrame {
        PreviewFrame::fit_bounds([0.0, 0.0], extent_mm, size, margin_px)
    }

    /// A frame showing the rectangle `min_mm`..`max_mm`, centred, with `margin_px` around it.
    pub fn fit_bounds(min_mm: [f64; 2], max_mm: [f64; 2], size: [u32; 2], margin_px: u32) -> PreviewFrame {
        let span = |a: usize| (max_mm[a] - min_mm[a]).max(1e-6);
        let inner = |a: usize| (f64::from(size[a]) - 2.0 * f64::from(margin_px)).max(1.0);
        let scale = (inner(0) / span(0)).min(inner(1) / span(1));
        let offset_px = [
            f64::from(margin_px) + (inner(0) - span(0) * scale) / 2.0,
            f64::from(margin_px) + (inner(1) - span(1) * scale) / 2.0,
        ];
        PreviewFrame { size, min_mm, max_mm, scale, offset_px }
    }

    /// The frame that shows the bed and every move of `view`.
    pub fn for_view(view: &PreviewViewModel, size: [u32; 2], margin_px: u32) -> PreviewFrame {
        let (mut min, mut max) = ([0.0_f64, 0.0], view.bed_mm.unwrap_or([0.0, 0.0]));
        let mut any = view.bed_mm.is_some();
        for m in &view.preview.moves {
            for p in [m.from, m.to] {
                let q = [p.x_mm(), p.y_mm()];
                if any {
                    min = [min[0].min(q[0]), min[1].min(q[1])];
                    max = [max[0].max(q[0]), max[1].max(q[1])];
                } else {
                    (min, max, any) = (q, q, true);
                }
            }
        }
        if !any {
            max = [100.0, 100.0];
        }
        PreviewFrame::fit_bounds(min, max, size, margin_px)
    }

    pub fn size_px(&self) -> [u32; 2] {
        self.size
    }

    pub fn to_px(&self, mm: [f64; 2]) -> [f64; 2] {
        [
            self.offset_px[0] + (mm[0] - self.min_mm[0]) * self.scale,
            self.offset_px[1] + (self.max_mm[1] - mm[1]) * self.scale,
        ]
    }

    pub fn to_mm(&self, px: [f64; 2]) -> [f64; 2] {
        [
            self.min_mm[0] + (px[0] - self.offset_px[0]) / self.scale,
            self.max_mm[1] - (px[1] - self.offset_px[1]) / self.scale,
        ]
    }

    pub fn mm_per_px(&self) -> f64 {
        1.0 / self.scale
    }

    /// The hit radius of a click `px` pixels wide, in millimetres.
    pub fn hit_radius_mm(&self, px: f64) -> f64 {
        px * self.mm_per_px()
    }
}

// ---- RD inspector -----------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdPanel {
    pub header: String,
    /// Offset, bytes, name, decoded.
    pub rows: Vec<[String; 4]>,
    pub banner: Option<String>,
}

pub fn rd_panel(view: &RdInspectViewModel, locale: Locale) -> RdPanel {
    let t = |text| app_text(text, locale);
    let mut header = format!("{} · {} bytes · {}", view.records.len() + view.truncated, view.bytes, view.plan);
    if view.truncated > 0 {
        header.push_str(&format!(" · {} {}", view.truncated, t(AppText::RdTruncated)));
    }
    let banner = match (&view.error, view.current) {
        (Some(e), _) => Some(format!("{}: {e}", t(AppText::RdFailed))),
        (None, false) => Some(t(AppText::RdNotCurrent).to_owned()),
        (None, true) => None,
    };
    let rows = view
        .records
        .iter()
        .map(|r| [format!("{:#06X}", r.offset), r.hex.clone(), r.label.unwrap_or("").to_owned(), r.decoded.clone()])
        .collect();
    RdPanel { header, rows, banner }
}
