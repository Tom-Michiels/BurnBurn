//! W4-L8b family 5, headless: the Windows app panels (preferences, shortcut editor with Ctrl instead
//! of Cmd, undo history, preview with play and click-to-source, RD inspector, help/support/debug
//! log). The view models come from the real `AppController` (in memory); no window, no dialog.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::HashSet;
use std::path::PathBuf;

use burnburn_presentation::{
    AppController, AppIntent, CommandKey, DebugLogViewModel, DiagnosticOptions, HeadlessEditorHarness, KeyBinding,
    NudgeDirection, NudgeStep, PlaybackSpeed, PrefKey, PreviewIntent, SelectMode, UiIntent,
};
use burnburn_types::{DocumentId, StableId};
const _: fn() = || {
    let _ = <DocumentId as StableId>::from_raw;
};
use burnburn_windows::app_menu::{AppAction, action_reaction, binding_from_keystroke, binding_glyphs, nudge_intent};
use burnburn_windows::host_panels::app::{
    AppInput, AppSheet, PREVIEW_SIZE, app_spec, id, on_app_event, on_app_file_chosen,
};
use burnburn_windows::host_panels::ids;
use burnburn_windows::host_panels::{
    ControlKind, DialogRequest, Family, FilePurpose, HostRequest, KeyStroke, PanelEvent, PanelReaction, PanelSpec,
    PanelValues, notify,
};
use burnburn_windows::l10n::Locale;
use common::{click, event, texts};

fn key(text: &str) -> KeyBinding {
    KeyBinding::parse(text).unwrap()
}

fn stroke(vk: u16, ctrl: bool, alt: bool, shift: bool) -> KeyStroke {
    KeyStroke { vk, ctrl, alt, shift }
}

// ---- menu items and keys -------------------------------------------------------------------------

#[test]
fn the_app_menu_items_have_ids_of_their_own_that_round_trip_with_titles_in_both_languages() {
    let mut seen = HashSet::new();
    for action in AppAction::ALL {
        let id = action.id();
        assert_eq!(ids::menu_owner(id).map(|(f, _)| f), Some(Family::App), "{action:?} is in the app block");
        assert!(seen.insert(id), "{action:?} shares its id");
        assert_eq!(AppAction::from_id(id), Some(action));
        assert!(action.shortcut_id().starts_with("host."));
        for locale in [Locale::Nl, Locale::En] {
            assert!(!action.title(locale).is_empty(), "{action:?} {locale:?}");
        }
    }
    assert_eq!(AppAction::from_id(101), None);
    assert_eq!(AppAction::from_id(ids::menu_base(Family::App) + 90), None);
}

fn compiled() -> (HeadlessEditorHarness, DocumentId) {
    use burnburn_document::process::{
        Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessSettings, Speed,
    };
    use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let profile = MachineProfile::new(
        "research-1500x900",
        1,
        MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap(),
        1000.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap();
    let ruida = burnburn_ruida::RuidaProfile {
        magic: burnburn_ruida::Magic::FIXTURE_0X88,
        overscan: burnburn_ruida::OverscanPolicy::ControllerManaged,
    };
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile), ruida });
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (130.0, 90.0));
    let rect = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![rect], mode: SelectMode::Replace });
    let p = PowerPercent::new(20.0).unwrap();
    h.send(UiIntent::AddOperation {
        document: doc,
        draft: OperationDraft {
            name: "Snijden".into(),
            mode: OperationMode::Line { kerf: Kerf::NONE },
            settings: ProcessSettings {
                speed: Speed::mm_per_s(100.0).unwrap(),
                power: PowerRange::new(p, p).unwrap(),
                passes: Passes::new(1).unwrap(),
                air_assist: true,
            },
            output_enabled: true,
        },
    });
    h.invoke(doc, CommandKey::ToolSelect);
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    (h, doc)
}

#[test]
fn history_preview_and_rd_need_what_they_show_and_the_manual_a_known_address() {
    let mut empty = HeadlessEditorHarness::new();
    let vm = empty.view();
    for always in
        [AppAction::About, AppAction::Preferences, AppAction::Shortcuts, AppAction::SupportData, AppAction::DebugLog]
    {
        assert!(always.enabled(&vm, false), "{always:?}");
    }
    for needs_window in [AppAction::History, AppAction::Preview, AppAction::InspectRd] {
        assert!(!needs_window.enabled(&vm, false), "{needs_window:?} without a window");
    }
    empty.new_document().unwrap();
    assert!(AppAction::History.enabled(&empty.view(), false));
    assert!(!AppAction::Preview.enabled(&empty.view(), false), "a window without a plan");
    let (h, _) = compiled();
    assert!(AppAction::Preview.enabled(&h.view(), false) && AppAction::InspectRd.enabled(&h.view(), false));
    assert!(!AppAction::Manual.enabled(&vm, false) && AppAction::Manual.enabled(&vm, true));
}

#[test]
fn bindings_are_drawn_the_windows_way_with_ctrl_for_the_register_cmd() {
    assert_eq!(binding_glyphs(&key("Cmd+Shift+S")), "Ctrl+Shift+S");
    assert_eq!(binding_glyphs(&key("Cmd+Opt+Left")), "Ctrl+Alt+Left");
    assert_eq!(binding_glyphs(&key("V")), "V");
    assert_eq!(binding_glyphs(&key("F5")), "F5");
    assert_eq!(binding_glyphs(&key("Shift+Delete")), "Shift+Delete");
    // The Mac Control flag has no key here: it is shown for what it is, not as Ctrl.
    assert_eq!(binding_glyphs(&key("Ctrl+Z")), "Win+Z");
}

#[test]
fn a_key_press_is_the_binding_of_the_register_and_unknown_keys_are_none() {
    assert_eq!(binding_from_keystroke(&stroke(0x5A, true, false, false)), Some(key("Cmd+Z")));
    assert_eq!(binding_from_keystroke(&stroke(0x53, true, false, true)), Some(key("Cmd+Shift+S")));
    assert_eq!(binding_from_keystroke(&stroke(0x41, false, true, false)), Some(key("Opt+A")));
    assert_eq!(binding_from_keystroke(&stroke(0x31, true, false, false)), Some(key("Cmd+1")));
    assert_eq!(binding_from_keystroke(&stroke(0x25, false, false, false)), Some(key("Left")));
    assert_eq!(binding_from_keystroke(&stroke(0x74, false, false, false)), Some(key("F5")));
    assert_eq!(binding_from_keystroke(&stroke(0x2E, false, false, false)), Some(key("Delete")));
    assert_eq!(binding_from_keystroke(&stroke(0x08, false, false, false)), Some(key("Delete")));
    assert_eq!(binding_from_keystroke(&stroke(0x1B, false, false, false)), Some(key("Escape")));
    assert_eq!(binding_from_keystroke(&stroke(0x0D, true, false, false)), Some(key("Cmd+Return")));
    assert_eq!(binding_from_keystroke(&stroke(0x20, false, false, false)), Some(key("Space")));
    // Bare modifier keys and keys the register has no name for.
    for vk in [0x10, 0x11, 0x12, 0x5B, 0xFF] {
        assert_eq!(binding_from_keystroke(&stroke(vk, false, false, false)), None, "{vk:#x}");
    }
}

#[test]
fn arrow_keys_nudge_by_their_step_and_ctrl_arrow_belongs_to_text() {
    let h = HeadlessEditorHarness::new();
    let mut h = h;
    let doc = h.new_document().unwrap();
    let vm = h.view();
    let nudge = |s: KeyStroke| nudge_intent(doc, &vm, &s);
    let expect = |direction, step| Some(UiIntent::App(AppIntent::Nudge { document: doc, direction, step }));
    assert_eq!(nudge(stroke(0x25, false, false, false)), expect(NudgeDirection::Left, NudgeStep::Normal));
    assert_eq!(nudge(stroke(0x27, false, false, true)), expect(NudgeDirection::Right, NudgeStep::Large));
    assert_eq!(nudge(stroke(0x26, false, true, false)), expect(NudgeDirection::Up, NudgeStep::Small));
    assert_eq!(nudge(stroke(0x28, false, false, false)), expect(NudgeDirection::Down, NudgeStep::Normal));
    assert_eq!(nudge(stroke(0x25, true, false, false)), None, "Ctrl+arrow is text navigation");
    assert_eq!(nudge(stroke(0x41, false, false, false)), None, "not an arrow");
    let gone = DocumentId::from_raw(4_000);
    assert_eq!(nudge_intent(gone, &vm, &stroke(0x25, false, false, false)), None, "a closed window");
}

// ---- panels ------------------------------------------------------------------------------------

struct Views {
    prefs: burnburn_presentation::PrefsViewModel,
    shortcuts: burnburn_presentation::ShortcutsViewModel,
    history: Option<burnburn_presentation::HistoryViewModel>,
    preview: Option<burnburn_presentation::PreviewViewModel>,
    rd: Option<burnburn_presentation::RdInspectViewModel>,
    debug: DebugLogViewModel,
    vm: std::sync::Arc<burnburn_presentation::AppViewModel>,
    doc: DocumentId,
}

fn views(h: &mut HeadlessEditorHarness, doc: DocumentId) -> Views {
    let c = h.controller();
    Views {
        prefs: c.prefs_view(),
        shortcuts: c.shortcuts_view(),
        history: c.history_view(doc),
        preview: c.preview_view(doc),
        rd: c.rd_inspect_view(doc),
        debug: c.debug_log_view(),
        vm: h.view(),
        doc,
    }
}

struct Host<'a> {
    filter: &'a str,
    row: Option<usize>,
    recording: Option<&'a str>,
    pending: Option<(&'a str, &'a KeyBinding)>,
    support: &'a str,
}

fn host() -> Host<'static> {
    Host { filter: "", row: None, recording: None, pending: None, support: "support data" }
}

impl Views {
    fn input<'a>(&'a self, locale: Locale, host: &'a Host<'a>) -> AppInput<'a> {
        AppInput {
            locale,
            vm: &self.vm,
            document: Some(self.doc),
            prefs: &self.prefs,
            shortcuts: &self.shortcuts,
            history: self.history.as_ref(),
            preview: self.preview.as_ref(),
            rd: self.rd.as_ref(),
            help: AppController::help_info(),
            debug: &self.debug,
            support_text: host.support,
            shortcut_filter: host.filter,
            shortcut_row: host.row,
            recording: host.recording,
            pending_replace: host.pending,
        }
    }
    fn spec(&self, sheet: AppSheet) -> PanelSpec {
        let h = host();
        app_spec(sheet, &self.input(Locale::En, &h))
    }
    fn react(&self, sheet: AppSheet, ev: PanelEvent) -> PanelReaction {
        let h = host();
        on_app_event(sheet, &self.input(Locale::En, &h), &ev)
    }
}

fn text(spec: &PanelSpec, cid: u16) -> String {
    spec.control(cid).unwrap_or_else(|| panic!("control {cid} missing")).text.clone()
}

fn app_intent(reaction: PanelReaction) -> AppIntent {
    match reaction {
        PanelReaction::Intent(UiIntent::App(i)) => i,
        other => panic!("expected an app intent, got {other:?}"),
    }
}

#[test]
fn every_sheet_has_unique_ids_inside_the_app_block() {
    let (mut h, doc) = compiled();
    let v = views(&mut h, doc);
    let mut all = HashSet::new();
    for sheet in AppSheet::ALL {
        for c in v.spec(sheet).controls() {
            assert!((7800..8000).contains(&c.id), "{sheet:?} {}", c.id);
            assert!(all.insert(c.id), "duplicate {} in {sheet:?}", c.id);
        }
        assert!(!v.spec(sheet).title.is_empty());
    }
    assert!(AppSheet::ALL.iter().all(|s| AppSheet::from_code(s.code()) == Some(*s)));
}

#[test]
fn preferences_show_every_row_and_send_the_text_the_id_or_on_off_as_the_mac_does() {
    let (mut h, doc) = compiled();
    let v = views(&mut h, doc);
    let s = v.spec(AppSheet::Prefs);
    let index = |k: PrefKey| PrefKey::ALL.iter().position(|x| *x == k).unwrap() as u16;
    // A choice, a check and a number field each have their control.
    assert!(matches!(s.control(id::PREF + index(PrefKey::Units)).unwrap().kind, ControlKind::Combo { .. }));
    assert!(matches!(s.control(id::PREF + index(PrefKey::SnapToGrid)).unwrap().kind, ControlKind::Check { .. }));
    assert!(matches!(s.control(id::PREF + index(PrefKey::AutosaveSeconds)).unwrap().kind, ControlKind::Edit));
    assert!(!text(&s, id::PREF_CAPTION + index(PrefKey::Units)).is_empty());

    let mut choice = PanelValues::default();
    choice.selection.insert(id::PREF + index(PrefKey::Units), Some(1));
    assert_eq!(
        app_intent(v.react(AppSheet::Prefs, event(id::PREF + index(PrefKey::Units), notify::CHANGED, choice))),
        AppIntent::SetPreference { key: PrefKey::Units, text: "inch".into() }
    );
    let mut flag = PanelValues::default();
    flag.checks.insert(id::PREF + index(PrefKey::SnapToGrid), true);
    assert_eq!(
        app_intent(v.react(AppSheet::Prefs, click(id::PREF + index(PrefKey::SnapToGrid), flag))),
        AppIntent::SetPreference { key: PrefKey::SnapToGrid, text: "on".into() }
    );
    let number = id::PREF + index(PrefKey::AutosaveSeconds);
    assert_eq!(
        app_intent(v.react(AppSheet::Prefs, event(number, notify::ENTER, texts(&[(number, "45")])))),
        AppIntent::SetPreference { key: PrefKey::AutosaveSeconds, text: "45".into() }
    );
    assert_eq!(
        app_intent(v.react(AppSheet::Prefs, click(id::PREF_RESET, PanelValues::default()))),
        AppIntent::ResetPreferences
    );

    // A refused value comes back as a banner in words.
    h.send(UiIntent::App(AppIntent::SetPreference { key: PrefKey::AutosaveSeconds, text: "abc".into() }));
    let v = views(&mut h, doc);
    assert!(!text(&v.spec(AppSheet::Prefs), id::PREF_BANNER).is_empty());
}

#[test]
fn the_shortcut_editor_lists_ctrl_bindings_records_a_key_and_offers_replace_on_a_conflict() {
    let (mut h, doc) = compiled();
    let v = views(&mut h, doc);
    let s = v.spec(AppSheet::Shortcuts);
    let ControlKind::List { items, .. } = &s.control(id::SC_LIST).unwrap().kind else { panic!("list") };
    assert!(items.iter().any(|l| l.contains("Ctrl+N")), "Cmd+N of the register reads Ctrl+N: {items:?}");
    assert!(items.iter().all(|l| !l.contains('⌘') && !l.contains("Cmd")), "{items:?}");
    assert!(!s.control(id::SC_REPLACE).unwrap().visible && !s.control(id::SC_RECORDING).unwrap().visible);

    // The list row to act on is host state; the buttons need a row.
    let host_row = Host { row: Some(0), ..host() };
    let lines = burnburn_windows::app_view::shortcut_lines(&v.shortcuts, "", Locale::En);
    let row_id = lines[0].id.clone();
    let inp = v.input(Locale::En, &host_row);
    let sel = |cid: u16| {
        let mut values = PanelValues::default();
        values.selection.insert(id::SC_LIST, Some(0));
        click(cid, values)
    };
    assert_eq!(
        app_intent(on_app_event(AppSheet::Shortcuts, &inp, &sel(id::SC_CLEAR))),
        AppIntent::ClearShortcut { id: row_id.clone() }
    );
    assert_eq!(
        app_intent(on_app_event(AppSheet::Shortcuts, &inp, &sel(id::SC_RESET_ONE))),
        AppIntent::ResetShortcut { id: row_id.clone() }
    );
    assert_eq!(
        app_intent(v.react(AppSheet::Shortcuts, click(id::SC_RESET_ALL, PanelValues::default()))),
        AppIntent::ResetAllShortcuts
    );
    assert_eq!(
        v.react(AppSheet::Shortcuts, click(id::SC_CLEAR, PanelValues::default())),
        PanelReaction::Nothing,
        "no row chosen"
    );

    // While recording, the next key becomes the binding; a bare modifier is not a key.
    let recording = Host { row: Some(0), recording: Some(row_id.as_str()), ..host() };
    let inp = v.input(Locale::En, &recording);
    let press = |s: KeyStroke| PanelEvent { id: id::SC_LIST, code: notify::KEY, key: Some(s), ..PanelEvent::default() };
    assert_eq!(
        app_intent(on_app_event(AppSheet::Shortcuts, &inp, &press(stroke(0x4B, true, false, true)))),
        AppIntent::SetShortcut { id: row_id.clone(), binding: key("Cmd+Shift+K"), replace: false }
    );
    assert_eq!(
        on_app_event(AppSheet::Shortcuts, &inp, &press(stroke(0x11, true, false, false))),
        PanelReaction::Nothing
    );
    // Without a recording a key press does nothing.
    assert_eq!(v.react(AppSheet::Shortcuts, press(stroke(0x4B, true, false, true))), PanelReaction::Nothing);
    assert!(!text(&v.spec(AppSheet::Shortcuts), id::SC_RECORD).is_empty());

    // After a conflict the user may take the key over: the replace button sends replace: true.
    let binding = key("Cmd+N");
    let conflict = Host { row: Some(0), pending: Some((row_id.as_str(), &binding)), ..host() };
    let inp = v.input(Locale::En, &conflict);
    assert!(app_spec(AppSheet::Shortcuts, &inp).control(id::SC_REPLACE).unwrap().visible);
    assert_eq!(
        app_intent(on_app_event(AppSheet::Shortcuts, &inp, &click(id::SC_REPLACE, PanelValues::default()))),
        AppIntent::SetShortcut { id: row_id, binding, replace: true }
    );
}

#[test]
fn the_shortcut_search_filters_the_list_by_name() {
    let (mut h, doc) = compiled();
    let v = views(&mut h, doc);
    let all = v.spec(AppSheet::Shortcuts);
    let filtered_host = Host { filter: "undo", ..host() };
    let filtered = app_spec(AppSheet::Shortcuts, &v.input(Locale::En, &filtered_host));
    let count = |s: &PanelSpec| match &s.control(id::SC_LIST).unwrap().kind {
        ControlKind::List { items, .. } => items.len(),
        _ => 0,
    };
    assert!(count(&filtered) < count(&all) && count(&filtered) > 0);
}

#[test]
fn the_history_lists_the_steps_and_a_click_jumps_to_that_state() {
    let (mut h, doc) = compiled();
    let v = views(&mut h, doc);
    let s = v.spec(AppSheet::History);
    let ControlKind::List { items, .. } = &s.control(id::HI_LIST).unwrap().kind else { panic!("list") };
    assert!(items.len() >= 2, "the start and at least the steps taken: {items:?}");
    assert!(items[0].starts_with("As opened") || !items[0].is_empty());
    let mut sel = PanelValues::default();
    sel.selection.insert(id::HI_LIST, Some(1));
    assert_eq!(
        app_intent(v.react(AppSheet::History, event(id::HI_LIST, notify::CHANGED, sel))),
        AppIntent::HistoryJump { document: doc, position: 1 }
    );
    // A window without a history (closed) reaches nothing.
    let mut gone = views(&mut h, doc);
    gone.doc = DocumentId::from_raw(9_999);
    gone.vm = std::sync::Arc::new((*gone.vm).clone());
    let host = host();
    let mut sel = PanelValues::default();
    sel.selection.insert(id::HI_LIST, Some(1));
    assert_eq!(
        on_app_event(AppSheet::History, &gone.input(Locale::En, &host), &event(id::HI_LIST, notify::CHANGED, sel)),
        PanelReaction::Nothing
    );
}

#[test]
fn the_preview_shows_an_estimate_plays_seeks_and_a_click_picks_the_move_under_it() {
    let (mut h, doc) = compiled();
    let v = views(&mut h, doc);
    let s = v.spec(AppSheet::Preview);
    assert!(text(&s, id::PV_TIME).contains("estimate"), "the clock is an estimate: {}", text(&s, id::PV_TIME));
    assert!(text(&s, id::PV_PLAN).starts_with("Plan"));
    match &s.control(id::PV_BITMAP).unwrap().kind {
        ControlKind::Bitmap { image: Some(img), height_px } => {
            assert_eq!([img.width, img.height], PREVIEW_SIZE);
            assert_eq!(*height_px, PREVIEW_SIZE[1] as i32);
        }
        other => panic!("{other:?}"),
    }
    let play = app_intent(v.react(AppSheet::Preview, click(id::PV_PLAY, PanelValues::default())));
    assert_eq!(play, AppIntent::Preview(PreviewIntent::Play { document: doc, playing: true }));
    let mut speed = PanelValues::default();
    speed.selection.insert(id::PV_SPEED, Some(1));
    assert_eq!(
        app_intent(v.react(AppSheet::Preview, event(id::PV_SPEED, notify::CHANGED, speed))),
        AppIntent::Preview(PreviewIntent::SetSpeed { document: doc, speed: PlaybackSpeed::ALL[1] })
    );
    let mut travel = PanelValues::default();
    travel.checks.insert(id::PV_TRAVEL, false);
    assert_eq!(
        app_intent(v.react(AppSheet::Preview, click(id::PV_TRAVEL, travel))),
        AppIntent::Preview(PreviewIntent::ShowTravel { document: doc, show: false })
    );
    // The slider runs 0..=1000 over the estimated total; its middle is half the estimate.
    let total = v.preview.as_ref().unwrap().estimated_total_seconds;
    let mut slide = PanelValues::default();
    slide.sliders.insert(id::PV_SLIDER, 500);
    match app_intent(v.react(AppSheet::Preview, event(id::PV_SLIDER, notify::SLIDER, slide))) {
        AppIntent::Preview(PreviewIntent::Seek { document, seconds }) => {
            assert_eq!(document, doc);
            assert!((seconds - total / 2.0).abs() < 1e-9, "{seconds} vs {total}");
        }
        other => panic!("{other:?}"),
    }
    // A click on the drawn rectangle's left edge picks a move with a tolerance of a few pixels.
    let mut pick = PanelEvent { id: id::PV_BITMAP, code: notify::BITMAP_CLICK, ..PanelEvent::default() };
    let frame = burnburn_windows::app_view::PreviewFrame::for_view(v.preview.as_ref().unwrap(), PREVIEW_SIZE, 8);
    let on_edge = frame.to_px([10.0, 50.0]);
    // The control is shown twice as wide and tall as the bitmap: the click is scaled back.
    pick.click_px = Some([on_edge[0] * 2.0, on_edge[1] * 2.0]);
    pick.view_px = Some([f64::from(PREVIEW_SIZE[0]) * 2.0, f64::from(PREVIEW_SIZE[1]) * 2.0]);
    match app_intent(v.react(AppSheet::Preview, pick)) {
        AppIntent::Preview(PreviewIntent::Pick { document, at_mm, tolerance_mm }) => {
            assert_eq!(document, doc);
            assert!((at_mm[0] - 10.0).abs() < 0.5 && (at_mm[1] - 50.0).abs() < 0.5, "{at_mm:?}");
            // 4 pixels of a 1500 mm bed drawn into 520 px (about 3 mm per pixel): roughly 12 mm.
            assert!((8.0..16.0).contains(&tolerance_mm), "{tolerance_mm}");
        }
        other => panic!("{other:?}"),
    }
    // A preview that does not exist (no plan) is an empty sheet and sends nothing.
    let mut empty = HeadlessEditorHarness::new();
    let d = empty.new_document().unwrap();
    let none = views(&mut empty, d);
    assert_eq!(none.react(AppSheet::Preview, click(id::PV_PLAY, PanelValues::default())), PanelReaction::Nothing);
}

#[test]
fn the_rd_inspector_lists_the_records_and_inspect_asks_presentation() {
    let (mut h, doc) = compiled();
    assert_eq!(
        app_intent(views(&mut h, doc).react(AppSheet::Rd, click(id::RD_INSPECT, PanelValues::default()))),
        AppIntent::Preview(PreviewIntent::InspectRd { document: doc })
    );
    h.send(UiIntent::App(AppIntent::Preview(PreviewIntent::InspectRd { document: doc })));
    h.run_tasks();
    let v = views(&mut h, doc);
    let s = v.spec(AppSheet::Rd);
    assert!(text(&s, id::RD_HEADER).contains("bytes"), "{}", text(&s, id::RD_HEADER));
    let ControlKind::List { items, .. } = &s.control(id::RD_LIST).unwrap().kind else { panic!("list") };
    assert!(!items.is_empty() && items[0].starts_with("0x0000"), "{items:?}");
}

#[test]
fn support_data_writes_what_the_user_saw_through_a_save_dialog_and_the_debug_log_switches() {
    let (mut h, doc) = compiled();
    let v = views(&mut h, doc);
    let s = v.spec(AppSheet::Support);
    assert_eq!(text(&s, id::SU_REPORT), "support data", "the panel shows the very text that would be written");
    assert!(matches!(s.control(id::SU_PATHS).unwrap().kind, ControlKind::Check { checked: false }));
    let mut ticked = PanelValues::default();
    ticked.checks.insert(id::SU_IDS, true);
    let options = DiagnosticOptions { include_project_paths: false, include_machine_ids: true };
    match v.react(AppSheet::Support, click(id::SU_SAVE, ticked)) {
        PanelReaction::Dialog(DialogRequest::Save { purpose, suggested_name, .. }) => {
            assert_eq!(purpose, FilePurpose::SupportBundle { options });
            assert!(suggested_name.ends_with(".txt"));
            let path = PathBuf::from("C:/tmp/support.txt");
            assert_eq!(
                on_app_file_chosen(&purpose, Some(path.clone())),
                PanelReaction::Intent(UiIntent::App(AppIntent::ExportSupportBundle { options, path }))
            );
            assert_eq!(on_app_file_chosen(&purpose, None), PanelReaction::Nothing);
        }
        other => panic!("{other:?}"),
    }
    let mut on = PanelValues::default();
    on.checks.insert(id::SU_DEBUG, true);
    assert_eq!(
        app_intent(v.react(AppSheet::Support, click(id::SU_DEBUG, on))),
        AppIntent::SetDebugLog { enabled: true }
    );
    assert_eq!(
        app_intent(v.react(AppSheet::Support, click(id::SU_CLEAR_LOG, PanelValues::default()))),
        AppIntent::ClearDebugLog
    );
}

#[test]
fn about_and_the_manual_are_host_requests_and_the_menu_opens_the_right_sheet() {
    let (mut h, doc) = compiled();
    let v = views(&mut h, doc);
    let help = AppController::help_info();
    let vm = &v.vm;
    match action_reaction(AppAction::About, vm, &help, Locale::En) {
        PanelReaction::Host(HostRequest::MessageBox { title, body }) => {
            assert!(title.contains(help.app_name) || body.contains(help.app_name));
            assert!(body.contains(help.version), "{body}");
        }
        other => panic!("{other:?}"),
    }
    let sheet = |a| match action_reaction(a, vm, &help, Locale::En) {
        PanelReaction::Host(HostRequest::ShowSheet(Family::App, code)) => AppSheet::from_code(code),
        other => panic!("{other:?}"),
    };
    assert_eq!(sheet(AppAction::Preferences), Some(AppSheet::Prefs));
    assert_eq!(sheet(AppAction::Shortcuts), Some(AppSheet::Shortcuts));
    assert_eq!(sheet(AppAction::History), Some(AppSheet::History));
    assert_eq!(sheet(AppAction::Preview), Some(AppSheet::Preview));
    assert_eq!(sheet(AppAction::InspectRd), Some(AppSheet::Rd));
    assert_eq!(sheet(AppAction::SupportData), Some(AppSheet::Support));
    assert_eq!(sheet(AppAction::DebugLog), Some(AppSheet::Support));
    match (help.manual_url, action_reaction(AppAction::Manual, vm, &help, Locale::En)) {
        (Some(url), PanelReaction::Host(HostRequest::OpenUrl(u))) => assert_eq!(u, url),
        (None, PanelReaction::Nothing) => {}
        other => panic!("{other:?}"),
    }
    // History without a window does not open.
    let empty = HeadlessEditorHarness::new().view();
    assert_eq!(action_reaction(AppAction::History, &empty, &help, Locale::En), PanelReaction::Nothing);
}
