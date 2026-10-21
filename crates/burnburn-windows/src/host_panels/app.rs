//! The app-level panels on Windows (W4-L8b): preferences, the shortcut editor (Ctrl instead of
//! Cmd), the undo history, the preview window with play and click-to-source, the RD inspector and
//! the help/support panel (support bundle, debug log, About, manual).
//!
//! Texts and intents come from the models shared with the Mac host (`crate::app_view`,
//! `AppIntent`); this module gives them control ids and decodes the events. The document is never
//! touched here: undo history jumps are `AppIntent::HistoryJump`, never replays of machine actions.

use burnburn_presentation::{
    AppIntent, AppViewModel, DebugLogViewModel, DiagnosticOptions, HelpInfo, HistoryViewModel, PlaybackSpeed, PrefKey,
    PrefsViewModel, PreviewIntent, PreviewViewModel, RdInspectViewModel, ShortcutsViewModel, UiIntent,
};
use burnburn_types::DocumentId;

use super::ids::APP_CONTROLS;
use super::spec::notify;
use super::{Chosen, ControlSpec, DialogRequest, FileFilter, FilePurpose, PanelEvent, PanelReaction, PanelSpec, Row};
use crate::app_menu::{AppAction, action_reaction, binding_from_keystroke, binding_glyphs};
use crate::app_text::{AppText, app_text};
use crate::app_view::{
    PrefsControl, PreviewFrame, ShortcutLine, history_lines, pref_choice_id, prefs_panel, preview_panel, rd_panel,
    render_preview, shortcut_lines, shortcut_problem_text,
};
use crate::l10n::Locale;

/// The windows of the app family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AppSheet {
    Prefs,
    Shortcuts,
    History,
    Preview,
    Rd,
    Support,
}

impl AppSheet {
    pub const ALL: [AppSheet; 6] =
        [AppSheet::Prefs, AppSheet::Shortcuts, AppSheet::History, AppSheet::Preview, AppSheet::Rd, AppSheet::Support];

    /// A small number that names the sheet in a window key.
    pub fn code(self) -> u8 {
        AppSheet::ALL.iter().position(|s| *s == self).map_or(0, |i| i as u8)
    }

    pub fn from_code(code: u8) -> Option<AppSheet> {
        AppSheet::ALL.get(usize::from(code)).copied()
    }
}

/// Control ids of the app family (`7800..8000`).
pub mod id {
    use super::APP_CONTROLS as B;

    // Preferences: caption `CAPTION + i`, control `PREF + i`, `i` = index in `PrefKey::ALL`.
    pub const PREF_CAPTION: u16 = B;
    pub const PREF: u16 = B + 16;
    pub const PREF_BANNER: u16 = B + 32;
    pub const PREF_RESET: u16 = B + 33;
    pub const PREF_NOTE: u16 = B + 34;

    // Shortcut editor.
    pub const SC_SEARCH_CAPTION: u16 = B + 40;
    pub const SC_SEARCH: u16 = B + 41;
    pub const SC_LIST: u16 = B + 42;
    pub const SC_RECORD: u16 = B + 43;
    pub const SC_CLEAR: u16 = B + 44;
    pub const SC_RESET_ONE: u16 = B + 45;
    pub const SC_RESET_ALL: u16 = B + 46;
    pub const SC_REPLACE: u16 = B + 47;
    pub const SC_NOTICE: u16 = B + 48;
    pub const SC_RECORDING: u16 = B + 49;

    // History.
    pub const HI_HINT: u16 = B + 60;
    pub const HI_LIST: u16 = B + 61;

    // Preview.
    pub const PV_PLAN: u16 = B + 70;
    pub const PV_TIME: u16 = B + 71;
    pub const PV_PLAY: u16 = B + 72;
    pub const PV_SLIDER: u16 = B + 73;
    pub const PV_SPEED: u16 = B + 74;
    pub const PV_TRAVEL: u16 = B + 75;
    pub const PV_STALE: u16 = B + 76;
    pub const PV_PICKED: u16 = B + 77;
    pub const PV_LEGEND: u16 = B + 78;
    pub const PV_BITMAP: u16 = B + 79;
    pub const PV_SPEED_CAPTION: u16 = B + 80;

    // RD inspector.
    pub const RD_HEADER: u16 = B + 90;
    pub const RD_BANNER: u16 = B + 91;
    pub const RD_LIST: u16 = B + 92;
    pub const RD_INSPECT: u16 = B + 93;
    pub const RD_NOTE: u16 = B + 94;

    // Help and support.
    pub const SU_ABOUT: u16 = B + 110;
    pub const SU_MANUAL: u16 = B + 111;
    pub const SU_MESSAGE: u16 = B + 112;
    pub const SU_PATHS: u16 = B + 113;
    pub const SU_IDS: u16 = B + 114;
    pub const SU_REPORT: u16 = B + 115;
    pub const SU_SAVE: u16 = B + 116;
    pub const SU_DEBUG: u16 = B + 117;
    pub const SU_LOG: u16 = B + 118;
    pub const SU_CLEAR_LOG: u16 = B + 119;
    pub const SU_HEADING: u16 = B + 120;
    pub const SU_LOG_HEADING: u16 = B + 121;
}

/// Size (pixels) of the picture the preview is drawn into, and its margin.
pub const PREVIEW_SIZE: [u32; 2] = [520, 340];
pub const PREVIEW_MARGIN: u32 = 8;

/// What the app panels are built from.
pub struct AppInput<'a> {
    pub locale: Locale,
    pub vm: &'a AppViewModel,
    /// The window the history/preview/RD panels are about (the active one).
    pub document: Option<DocumentId>,
    pub prefs: &'a PrefsViewModel,
    pub shortcuts: &'a ShortcutsViewModel,
    pub history: Option<&'a HistoryViewModel>,
    pub preview: Option<&'a PreviewViewModel>,
    pub rd: Option<&'a RdInspectViewModel>,
    pub help: HelpInfo,
    pub debug: &'a DebugLogViewModel,
    /// The text a support bundle would hold with the options ticked in the panel (the host asks
    /// the controller for it: what the user sees is what is written).
    pub support_text: &'a str,
    /// Host state of the shortcut editor.
    pub shortcut_filter: &'a str,
    pub shortcut_row: Option<usize>,
    /// The id of the entry waiting for a key (the Record button was pressed), if any.
    pub recording: Option<&'a str>,
    /// A key that was refused as a conflict and may be taken over: `(id, binding)`.
    pub pending_replace: Option<(&'a str, &'a burnburn_presentation::KeyBinding)>,
}

fn at(text: AppText, locale: Locale) -> &'static str {
    app_text(text, locale)
}

fn pref_index(key: PrefKey) -> u16 {
    PrefKey::ALL.iter().position(|k| *k == key).map_or(0, |i| i as u16)
}

pub fn app_spec(sheet: AppSheet, input: &AppInput<'_>) -> PanelSpec {
    match sheet {
        AppSheet::Prefs => prefs_spec(input),
        AppSheet::Shortcuts => shortcuts_spec(input),
        AppSheet::History => history_spec(input),
        AppSheet::Preview => preview_spec(input),
        AppSheet::Rd => rd_spec(input),
        AppSheet::Support => support_spec(input),
    }
}

fn prefs_spec(input: &AppInput<'_>) -> PanelSpec {
    let locale = input.locale;
    let panel = prefs_panel(input.prefs, locale);
    let mut rows = Vec::new();
    for row in &panel.rows {
        let i = pref_index(row.key);
        let (caption_text, control) = match &row.control {
            PrefsControl::Choice { titles, selected, .. } => {
                (row.caption.clone(), ControlSpec::combo(id::PREF + i, titles.clone(), Some(*selected), true))
            }
            PrefsControl::Check(on) => (row.caption.clone(), ControlSpec::check(id::PREF + i, "", *on, true)),
            PrefsControl::Field { text, suffix, invalid } => {
                let caption = format!("{} ({suffix})", row.caption);
                let mut edit = ControlSpec::edit(id::PREF + i, text.clone(), true);
                if *invalid {
                    edit = edit.tip(panel.banner.clone().unwrap_or_default());
                }
                (caption, edit)
            }
        };
        rows.push(Row::of(vec![ControlSpec::label(id::PREF_CAPTION + i, caption_text).weighted(2), control]));
    }
    rows.push(Row::one(ControlSpec::label(id::PREF_BANNER, panel.banner.clone().unwrap_or_default())));
    rows.push(Row::one(ControlSpec::label(id::PREF_NOTE, at(AppText::PrefsNote, locale))));
    rows.push(Row::one(ControlSpec::button(id::PREF_RESET, at(AppText::PrefsReset, locale), true)));
    PanelSpec { title: at(AppText::PrefsTitle, locale).to_owned(), rows }
}

/// The text of one line of the shortcut list: name, key, and the default when it was changed.
fn shortcut_text(line: &ShortcutLine, locale: Locale) -> String {
    if line.customized {
        format!("{}    {}    ({}: {})", line.name, line.key_text, at(AppText::ShortcutColDefault, locale), line.default_text)
    } else {
        format!("{}    {}", line.name, line.key_text)
    }
}

/// The register id of the list row the user means: the row of the event, else the host's.
pub fn shortcut_row_id(input: &AppInput<'_>, values: &super::PanelValues) -> Option<String> {
    let lines = shortcut_lines(input.shortcuts, input.shortcut_filter, input.locale);
    let row = values.selection.get(&id::SC_LIST).copied().flatten().or(input.shortcut_row)?;
    lines.get(row).map(|l| l.id.clone())
}

fn shortcuts_spec(input: &AppInput<'_>) -> PanelSpec {
    let locale = input.locale;
    let lines = shortcut_lines(input.shortcuts, input.shortcut_filter, locale);
    let items: Vec<String> = lines.iter().map(|l| shortcut_text(l, locale)).collect();
    let row = input.shortcut_row.filter(|r| *r < items.len());
    let attempted = input.pending_replace.map(|(_, b)| binding_glyphs(b)).unwrap_or_default();
    let notice = input
        .shortcuts
        .error
        .as_ref()
        .map(|e| shortcut_problem_text(&e.problem, &attempted, input.shortcuts, locale))
        .unwrap_or_default();
    let has_row = row.is_some();
    let rows = vec![
        Row::of(vec![
            ControlSpec::label(id::SC_SEARCH_CAPTION, at(AppText::ShortcutSearch, locale)).weighted(2),
            ControlSpec::edit(id::SC_SEARCH, input.shortcut_filter, true).weighted(2),
        ]),
        Row::one(ControlSpec::list(id::SC_LIST, 12, items, row, true)),
        Row::one(ControlSpec::label(id::SC_RECORDING, at(AppText::ShortcutRecording, locale)).hidden_if(input.recording.is_none())),
        Row::one(ControlSpec::label(id::SC_NOTICE, notice)),
        Row::of(vec![
            ControlSpec::button(id::SC_RECORD, at(AppText::ShortcutRecord, locale), has_row),
            ControlSpec::button(id::SC_CLEAR, at(AppText::ShortcutClear, locale), has_row),
            ControlSpec::button(id::SC_RESET_ONE, at(AppText::ShortcutResetOne, locale), has_row),
        ]),
        Row::of(vec![
            ControlSpec::button(id::SC_RESET_ALL, at(AppText::ShortcutResetAll, locale), true),
            ControlSpec::button(id::SC_REPLACE, at(AppText::ShortcutReplace, locale), input.pending_replace.is_some())
                .hidden_if(input.pending_replace.is_none()),
        ]),
    ];
    PanelSpec { title: at(AppText::ShortcutsTitle, locale).to_owned(), rows }
}

fn open_document(input: &AppInput<'_>) -> Option<DocumentId> {
    input.document.filter(|d| input.vm.document(*d).is_some())
}

fn history_spec(input: &AppInput<'_>) -> PanelSpec {
    let locale = input.locale;
    let title = at(AppText::HistoryTitle, locale).to_owned();
    let Some(history) = input.history.filter(|_| open_document(input).is_some()) else {
        return PanelSpec {
            title,
            rows: vec![
                Row::one(ControlSpec::label(id::HI_HINT, at(AppText::HistoryNoDocument, locale))),
                Row::one(ControlSpec::list(id::HI_LIST, 14, Vec::new(), None, false)),
            ],
        };
    };
    let lines = history_lines(history, locale);
    let current = lines.iter().position(|l| l.current);
    PanelSpec {
        title,
        rows: vec![
            Row::one(ControlSpec::label(id::HI_HINT, at(AppText::HistoryHint, locale))),
            Row::one(ControlSpec::list(id::HI_LIST, 14, lines.into_iter().map(|l| l.text).collect(), current, true)),
        ],
    }
}

/// Slider positions per estimated total.
const SLIDER_STEPS: i32 = 1000;

fn preview_spec(input: &AppInput<'_>) -> PanelSpec {
    let locale = input.locale;
    let title = at(AppText::PreviewTitle, locale).to_owned();
    let Some(view) = input.preview.filter(|_| open_document(input).is_some()) else {
        return PanelSpec {
            title,
            rows: vec![
                Row::one(ControlSpec::label(id::PV_PLAN, at(AppText::PreviewNoPlan, locale))),
                Row::one(ControlSpec::bitmap(id::PV_BITMAP, PREVIEW_SIZE[1] as i32, None)),
            ],
        };
    };
    let p = preview_panel(view, locale);
    let frame = PreviewFrame::for_view(view, PREVIEW_SIZE, PREVIEW_MARGIN);
    let pos = if p.slider_max > 0.0 { (p.slider_value / p.slider_max * f64::from(SLIDER_STEPS)).round() as i32 } else { 0 };
    let rows = vec![
        Row::one(ControlSpec::label(id::PV_PLAN, p.plan_text.clone())),
        Row::one(ControlSpec::label(id::PV_STALE, p.stale.clone().unwrap_or_default())),
        Row::of(vec![
            ControlSpec::button(id::PV_PLAY, p.play_title, true),
            ControlSpec::slider(id::PV_SLIDER, 0, SLIDER_STEPS, pos.clamp(0, SLIDER_STEPS), true).weighted(4),
        ]),
        Row::one(ControlSpec::label(id::PV_TIME, p.time_text.clone())),
        Row::of(vec![
            ControlSpec::label(id::PV_SPEED_CAPTION, at(AppText::PreviewSpeed, locale)),
            ControlSpec::combo(id::PV_SPEED, p.speed_titles.clone(), Some(p.speed_index), true),
            ControlSpec::check(id::PV_TRAVEL, at(AppText::PreviewShowTravel, locale), p.show_travel, true).weighted(2),
        ]),
        Row::one(ControlSpec::bitmap(id::PV_BITMAP, PREVIEW_SIZE[1] as i32, Some(render_preview(view, &frame)))),
        Row::one(ControlSpec::label(id::PV_PICKED, p.picked.clone().unwrap_or_else(|| p.picked_hint.clone()))),
        Row::one(ControlSpec::label(id::PV_LEGEND, p.legend)),
    ];
    PanelSpec { title, rows }
}

fn rd_spec(input: &AppInput<'_>) -> PanelSpec {
    let locale = input.locale;
    let title = at(AppText::RdTitle, locale).to_owned();
    let can_inspect = AppAction::InspectRd.enabled(input.vm, input.help.manual_url.is_some())
        && input.document.is_some_and(|d| input.vm.active == Some(d));
    let (header, banner, items) = match input.rd.filter(|_| open_document(input).is_some()) {
        Some(view) => {
            let p = rd_panel(view, locale);
            let items = p.rows.iter().map(|r| format!("{}   {}   {}   {}", r[0], r[1], r[2], r[3])).collect();
            (p.header, p.banner.unwrap_or_default(), items)
        }
        None => (at(AppText::RdNoPlan, locale).to_owned(), String::new(), Vec::new()),
    };
    let has_rows = !items.is_empty();
    let rows = vec![
        Row::one(ControlSpec::label(id::RD_HEADER, header)),
        Row::one(ControlSpec::label(id::RD_BANNER, banner)),
        Row::one(ControlSpec::list(id::RD_LIST, 16, items, None, has_rows)),
        Row::one(ControlSpec::button(id::RD_INSPECT, at(AppText::InspectRd, locale), can_inspect)),
        Row::one(ControlSpec::label(
            id::RD_NOTE,
            format!("{} {}", at(AppText::RdNameIsLabel, locale), at(AppText::RdNoUpload, locale)),
        )),
    ];
    PanelSpec { title, rows }
}

fn support_spec(input: &AppInput<'_>) -> PanelSpec {
    let locale = input.locale;
    let log = if input.debug.lines.is_empty() { String::new() } else { input.debug.lines.join("\n") };
    let rows = vec![
        Row::one(ControlSpec::heading(id::SU_HEADING, at(AppText::SupportTitle, locale))),
        Row::one(ControlSpec::label(id::SU_MESSAGE, at(AppText::SupportMessage, locale))),
        Row::one(ControlSpec::check(id::SU_PATHS, at(AppText::SupportPaths, locale), false, true)),
        Row::one(ControlSpec::check(id::SU_IDS, at(AppText::SupportIds, locale), false, true)),
        Row::one(ControlSpec::report(id::SU_REPORT, 10, input.support_text)),
        Row::one(ControlSpec::button(id::SU_SAVE, at(AppText::SupportSave, locale), true)),
        Row::one(ControlSpec::heading(id::SU_LOG_HEADING, at(AppText::DebugLog, locale))),
        Row::of(vec![
            ControlSpec::check(id::SU_DEBUG, at(AppText::DebugLog, locale), input.debug.enabled, true),
            ControlSpec::button(id::SU_CLEAR_LOG, at(AppText::ShortcutClear, locale), !input.debug.lines.is_empty()),
        ]),
        Row::one(ControlSpec::report(id::SU_LOG, 6, log)),
        Row::of(vec![
            ControlSpec::button(id::SU_ABOUT, at(AppText::About, locale), true),
            ControlSpec::button(id::SU_MANUAL, at(AppText::Manual, locale), input.help.manual_url.is_some())
                .tip(if input.help.manual_url.is_some() { "" } else { at(AppText::ManualUnavailable, locale) }),
        ]),
    ];
    PanelSpec { title: at(AppText::SupportTitle, locale).to_owned(), rows }
}

fn app_intent(i: AppIntent) -> PanelReaction {
    PanelReaction::Intent(UiIntent::App(i))
}

pub fn on_app_event(sheet: AppSheet, input: &AppInput<'_>, event: &PanelEvent) -> PanelReaction {
    match sheet {
        AppSheet::Prefs => on_prefs(event),
        AppSheet::Shortcuts => on_shortcuts(input, event),
        AppSheet::History => on_history(input, event),
        AppSheet::Preview => on_preview(input, event),
        AppSheet::Rd => on_rd(input, event),
        AppSheet::Support => on_support(input, event),
    }
}

fn on_prefs(event: &PanelEvent) -> PanelReaction {
    let (cid, code, v) = (event.id, event.code, &event.values);
    if cid == id::PREF_RESET && code == notify::CLICKED {
        return app_intent(AppIntent::ResetPreferences);
    }
    let Some(index) = cid.checked_sub(id::PREF).map(usize::from).filter(|i| *i < PrefKey::ALL.len()) else {
        return PanelReaction::Nothing;
    };
    let key = PrefKey::ALL[index];
    let text = match code {
        notify::CHANGED => {
            let choice = usize::try_from(v.selected(cid)).ok().and_then(|i| pref_choice_id(key, i));
            let Some(choice) = choice else { return PanelReaction::Nothing };
            choice.to_owned()
        }
        notify::CLICKED => (if v.checked(cid) { "on" } else { "off" }).to_owned(),
        notify::ENTER | notify::EDIT_DONE => v.text(cid).to_owned(),
        _ => return PanelReaction::Nothing,
    };
    app_intent(AppIntent::SetPreference { key, text })
}

fn on_shortcuts(input: &AppInput<'_>, event: &PanelEvent) -> PanelReaction {
    let (cid, code) = (event.id, event.code);
    if code == notify::KEY {
        // Only while an entry waits for a key; a bare modifier or Esc is not a key to bind.
        let (Some(target), Some(stroke)) = (input.recording, event.key) else { return PanelReaction::Nothing };
        return match binding_from_keystroke(&stroke) {
            Some(binding) if binding.key != "Escape" || stroke.ctrl || stroke.alt || stroke.shift => {
                app_intent(AppIntent::SetShortcut { id: target.to_owned(), binding, replace: false })
            }
            _ => PanelReaction::Nothing,
        };
    }
    if code != notify::CLICKED {
        return PanelReaction::Nothing;
    }
    match cid {
        id::SC_RESET_ALL => app_intent(AppIntent::ResetAllShortcuts),
        id::SC_REPLACE => match input.pending_replace {
            Some((target, binding)) => {
                app_intent(AppIntent::SetShortcut { id: target.to_owned(), binding: binding.clone(), replace: true })
            }
            None => PanelReaction::Nothing,
        },
        id::SC_CLEAR | id::SC_RESET_ONE => match shortcut_row_id(input, &event.values) {
            Some(target) if cid == id::SC_CLEAR => app_intent(AppIntent::ClearShortcut { id: target }),
            Some(target) => app_intent(AppIntent::ResetShortcut { id: target }),
            None => PanelReaction::Nothing,
        },
        _ => PanelReaction::Nothing,
    }
}

fn on_history(input: &AppInput<'_>, event: &PanelEvent) -> PanelReaction {
    let Some(document) = open_document(input) else { return PanelReaction::Nothing };
    if event.id != id::HI_LIST || event.code != notify::CHANGED || input.history.is_none() {
        return PanelReaction::Nothing;
    }
    match usize::try_from(event.values.selected(id::HI_LIST)) {
        Ok(position) => app_intent(AppIntent::HistoryJump { document, position }),
        Err(_) => PanelReaction::Nothing,
    }
}

fn on_preview(input: &AppInput<'_>, event: &PanelEvent) -> PanelReaction {
    let (Some(document), Some(view)) = (open_document(input), input.preview) else { return PanelReaction::Nothing };
    let (cid, code, v) = (event.id, event.code, &event.values);
    let preview = |i: PreviewIntent| app_intent(AppIntent::Preview(i));
    match (cid, code) {
        (id::PV_PLAY, notify::CLICKED) => preview(PreviewIntent::Play { document, playing: !view.playing }),
        (id::PV_SLIDER, notify::SLIDER) => {
            let pos = f64::from(v.slider(id::PV_SLIDER).unwrap_or(0).clamp(0, SLIDER_STEPS));
            preview(PreviewIntent::Seek { document, seconds: pos / f64::from(SLIDER_STEPS) * view.estimated_total_seconds })
        }
        (id::PV_SPEED, notify::CHANGED) => {
            let speed = usize::try_from(v.selected(id::PV_SPEED)).ok().and_then(|i| PlaybackSpeed::ALL.get(i));
            speed.map_or(PanelReaction::Nothing, |speed| preview(PreviewIntent::SetSpeed { document, speed: *speed }))
        }
        (id::PV_TRAVEL, notify::CLICKED) => preview(PreviewIntent::ShowTravel { document, show: v.checked(id::PV_TRAVEL) }),
        (id::PV_BITMAP, notify::BITMAP_CLICK) => {
            let (Some(click), Some(shown)) = (event.click_px, event.view_px) else { return PanelReaction::Nothing };
            if !(shown[0] > 0.0 && shown[1] > 0.0) {
                return PanelReaction::Nothing;
            }
            // The control stretches the bitmap: scale the click back to bitmap pixels.
            let px = [
                click[0] * f64::from(PREVIEW_SIZE[0]) / shown[0],
                click[1] * f64::from(PREVIEW_SIZE[1]) / shown[1],
            ];
            let frame = PreviewFrame::for_view(view, PREVIEW_SIZE, PREVIEW_MARGIN);
            preview(PreviewIntent::Pick { document, at_mm: frame.to_mm(px), tolerance_mm: frame.hit_radius_mm(4.0) })
        }
        _ => PanelReaction::Nothing,
    }
}

fn on_rd(input: &AppInput<'_>, event: &PanelEvent) -> PanelReaction {
    let Some(document) = open_document(input) else { return PanelReaction::Nothing };
    if event.id == id::RD_INSPECT
        && event.code == notify::CLICKED
        && AppAction::InspectRd.enabled(input.vm, input.help.manual_url.is_some())
    {
        return app_intent(AppIntent::Preview(PreviewIntent::InspectRd { document }));
    }
    PanelReaction::Nothing
}

fn on_support(input: &AppInput<'_>, event: &PanelEvent) -> PanelReaction {
    let (cid, v) = (event.id, &event.values);
    if event.code != notify::CLICKED {
        return PanelReaction::Nothing;
    }
    match cid {
        id::SU_SAVE => PanelReaction::Dialog(DialogRequest::Save {
            purpose: FilePurpose::SupportBundle {
                options: DiagnosticOptions {
                    include_project_paths: v.checked(id::SU_PATHS),
                    include_machine_ids: v.checked(id::SU_IDS),
                },
            },
            title: at(AppText::SupportSavePanelTitle, input.locale).to_owned(),
            suggested_name: "burnburn-support.txt".to_owned(),
            filters: vec![FileFilter { name: "Tekst / Text".to_owned(), extensions: vec!["txt"] }],
        }),
        id::SU_DEBUG => app_intent(AppIntent::SetDebugLog { enabled: v.checked(id::SU_DEBUG) }),
        id::SU_CLEAR_LOG => app_intent(AppIntent::ClearDebugLog),
        id::SU_ABOUT => action_reaction(AppAction::About, input.vm, &input.help, input.locale),
        id::SU_MANUAL => action_reaction(AppAction::Manual, input.vm, &input.help, input.locale),
        _ => PanelReaction::Nothing,
    }
}

/// A file dialog of this family closed (the support bundle).
pub fn on_app_file_chosen(purpose: &FilePurpose, path: Chosen) -> PanelReaction {
    match (purpose, path) {
        (FilePurpose::SupportBundle { options }, Some(path)) => {
            app_intent(AppIntent::ExportSupportBundle { options: *options, path })
        }
        _ => PanelReaction::Nothing,
    }
}
