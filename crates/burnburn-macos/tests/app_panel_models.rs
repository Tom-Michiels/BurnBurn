//! W3-UI-app, headless: the app menu items, the events of the app panels and everything the panels
//! show, decided without AppKit. The view models come from the real `AppController`.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use std::collections::BTreeSet;

use burnburn_macos::adapter::{
    MenuItem, NativeEvent, Reaction, flags, menu_enabled, menu_item, menu_items, menu_tag, translate,
};
use burnburn_macos::app_menu::{AppAction, AppEvent, ArrowKey, arrow_key, binding_from_event, binding_glyphs};
use burnburn_macos::app_text::{AppText, app_text};
use burnburn_macos::app_view::{
    PrefsControl, PreviewFrame, clock_text, history_lines, prefs_panel, preview_panel, rd_panel, render_preview,
    shortcut_lines,
};
use burnburn_macos::l10n::Locale;
use burnburn_presentation::{
    AppIntent, CommandKey, HeadlessEditorHarness, KeyBinding, NudgeDirection, NudgeStep, PlaybackSpeed, PrefKey,
    PreviewIntent, ShortcutProblem, UiIntent,
};
use burnburn_types::{DocumentId, StableId};

fn key(text: &str) -> KeyBinding {
    KeyBinding::parse(text).unwrap()
}

fn compiled() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.send(UiIntent::SetMachineProfile { profile: Box::new(machine_profile()), ruida: ruida() });
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (130.0, 90.0));
    let rect = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![rect], mode: burnburn_presentation::SelectMode::Replace });
    h.send(UiIntent::AddOperation { document: doc, draft: cut() });
    h.invoke(doc, CommandKey::ToolSelect);
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    (h, doc)
}

fn machine_profile() -> burnburn_job::MachineProfile {
    use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
    MachineProfile::new(
        "research-1500x900",
        1,
        MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap(),
        1000.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
}

fn ruida() -> burnburn_ruida::RuidaProfile {
    burnburn_ruida::RuidaProfile {
        magic: burnburn_ruida::Magic::FIXTURE_0X88,
        overscan: burnburn_ruida::OverscanPolicy::ControllerManaged,
    }
}

fn cut() -> burnburn_document::process::OperationDraft {
    use burnburn_document::process::{
        Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessSettings, Speed,
    };
    let p = PowerPercent::new(20.0).unwrap();
    OperationDraft {
        name: "Snijden".into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: ProcessSettings {
            speed: Speed::mm_per_s(100.0).unwrap(),
            power: PowerRange::new(p, p).unwrap(),
            passes: Passes::new(1).unwrap(),
            air_assist: true,
        },
        output_enabled: true,
    }
}

// ---- menu items ----------------------------------------------------------------------------------

#[test]
fn the_app_menu_items_have_tags_of_their_own_that_round_trip() {
    let mut tags = BTreeSet::new();
    for item in menu_items() {
        assert!(tags.insert(menu_tag(item)), "{item:?} shares its tag");
    }
    for action in AppAction::ALL {
        let item = MenuItem::App(action);
        assert!(menu_items().contains(&item), "{action:?} is a menu item");
        assert_eq!(menu_item(menu_tag(item)), Some(item), "{action:?}");
    }
    assert_eq!(AppAction::from_tag(AppAction::Preview.tag()), Some(AppAction::Preview));
    assert_eq!(AppAction::from_tag(-1), None);
}

#[test]
fn every_text_exists_in_both_languages_and_the_languages_differ() {
    let mut differing = 0;
    for text in AppText::ALL {
        let (nl, en) = (app_text(*text, Locale::Nl), app_text(*text, Locale::En));
        assert!(!nl.trim().is_empty() && !en.trim().is_empty(), "{text:?}");
        differing += usize::from(nl != en);
    }
    assert!(differing * 10 > AppText::ALL.len() * 7, "most texts are really translated: {differing}");
    for action in AppAction::ALL {
        assert!(!action.title(Locale::Nl).is_empty() && !action.title(Locale::En).is_empty());
    }
}

#[test]
fn history_preview_and_rd_need_what_they_show_and_the_rest_is_always_there() {
    let mut empty = HeadlessEditorHarness::new();
    let vm = empty.view();
    for always in
        [AppAction::About, AppAction::Preferences, AppAction::Shortcuts, AppAction::SupportData, AppAction::DebugLog]
    {
        assert!(menu_enabled(MenuItem::App(always), &vm), "{always:?}");
    }
    for needs_window in [AppAction::History, AppAction::Preview, AppAction::InspectRd] {
        assert!(!menu_enabled(MenuItem::App(needs_window), &vm), "{needs_window:?} without a window");
    }
    let doc = empty.new_document().unwrap();
    assert!(menu_enabled(MenuItem::App(AppAction::History), &empty.view()));
    assert!(!menu_enabled(MenuItem::App(AppAction::Preview), &empty.view()), "a window without a plan");
    let _ = doc;
    let (h, _) = compiled();
    let vm = h.view();
    assert!(menu_enabled(MenuItem::App(AppAction::Preview), &vm));
    assert!(menu_enabled(MenuItem::App(AppAction::InspectRd), &vm));
}

#[test]
fn a_menu_choice_is_a_reaction_the_host_performs_not_an_intent() {
    let vm = HeadlessEditorHarness::new().view();
    for action in AppAction::ALL {
        assert_eq!(translate(NativeEvent::Menu(MenuItem::App(action)), &vm), Reaction::App(action));
    }
}

// ---- events become the intents presentation expects -----------------------------------------------

fn intent(event: AppEvent, vm: &burnburn_presentation::AppViewModel) -> Option<UiIntent> {
    match translate(NativeEvent::App(event), vm) {
        Reaction::Intent(i) => Some(i),
        Reaction::Nothing => None,
        other => panic!("unexpected reaction {other:?}"),
    }
}

#[test]
fn panel_events_become_exactly_the_intents_presentation_defines() {
    let (h, doc) = compiled();
    let vm = h.view();
    let app = UiIntent::App;
    assert_eq!(
        intent(AppEvent::PrefEdited { key: PrefKey::GridSnap, text: "2,5".into() }, &vm),
        Some(app(AppIntent::SetPreference { key: PrefKey::GridSnap, text: "2,5".into() })),
        "raw text goes to presentation unparsed (DR-05)"
    );
    assert_eq!(intent(AppEvent::PrefsReset, &vm), Some(app(AppIntent::ResetPreferences)));
    assert_eq!(
        intent(AppEvent::ShortcutRecorded { id: "cmd.Save".into(), binding: key("Cmd+K"), replace: true }, &vm),
        Some(app(AppIntent::SetShortcut { id: "cmd.Save".into(), binding: key("Cmd+K"), replace: true }))
    );
    assert_eq!(
        intent(AppEvent::ShortcutCleared { id: "cmd.Save".into() }, &vm),
        Some(app(AppIntent::ClearShortcut { id: "cmd.Save".into() }))
    );
    assert_eq!(
        intent(AppEvent::ShortcutReset { id: "cmd.Save".into() }, &vm),
        Some(app(AppIntent::ResetShortcut { id: "cmd.Save".into() }))
    );
    assert_eq!(intent(AppEvent::ShortcutsResetAll, &vm), Some(app(AppIntent::ResetAllShortcuts)));
    assert_eq!(
        intent(AppEvent::HistoryRowClicked { document: doc, row: 2 }, &vm),
        Some(app(AppIntent::HistoryJump { document: doc, position: 2 })),
        "row 0 is the start, row n the state after n steps"
    );
    let preview = |p| Some(app(AppIntent::Preview(p)));
    assert_eq!(
        intent(AppEvent::PreviewSeeked { document: doc, seconds: 1.5 }, &vm),
        preview(PreviewIntent::Seek { document: doc, seconds: 1.5 })
    );
    assert_eq!(
        intent(AppEvent::PreviewPlayToggled { document: doc, playing: true }, &vm),
        preview(PreviewIntent::Play { document: doc, playing: true })
    );
    assert_eq!(
        intent(AppEvent::PreviewSpeedChosen { document: doc, index: 2 }, &vm),
        preview(PreviewIntent::SetSpeed { document: doc, speed: PlaybackSpeed::ALL[2] })
    );
    assert_eq!(intent(AppEvent::PreviewSpeedChosen { document: doc, index: 99 }, &vm), None);
    assert_eq!(
        intent(AppEvent::PreviewTravelToggled { document: doc, show: false }, &vm),
        preview(PreviewIntent::ShowTravel { document: doc, show: false })
    );
    assert_eq!(
        intent(AppEvent::PreviewClicked { document: doc, at_mm: [3.0, 4.0], tolerance_mm: 0.5 }, &vm),
        preview(PreviewIntent::Pick { document: doc, at_mm: [3.0, 4.0], tolerance_mm: 0.5 })
    );
    assert_eq!(
        intent(AppEvent::PreviewMoveChosen { document: doc, index: 3 }, &vm),
        preview(PreviewIntent::SelectMove { document: doc, index: 3 })
    );
    assert_eq!(
        intent(AppEvent::RdInspectRequested { document: doc }, &vm),
        preview(PreviewIntent::InspectRd { document: doc })
    );
    let options = burnburn_presentation::DiagnosticOptions { include_project_paths: true, include_machine_ids: false };
    let path = std::path::PathBuf::from("/tmp/support.txt");
    assert_eq!(
        intent(AppEvent::SupportDialogFinished { options, path: Some(path.clone()) }, &vm),
        Some(app(AppIntent::ExportSupportBundle { options, path }))
    );
    assert_eq!(intent(AppEvent::SupportDialogFinished { options, path: None }, &vm), None, "cancel does nothing");
    assert_eq!(
        intent(AppEvent::DebugLogSwitched { enabled: true }, &vm),
        Some(app(AppIntent::SetDebugLog { enabled: true }))
    );
}

#[test]
fn events_of_a_closed_window_reach_nothing() {
    let vm = HeadlessEditorHarness::new().view();
    let gone = DocumentId::from_raw(42);
    assert_eq!(intent(AppEvent::HistoryRowClicked { document: gone, row: 1 }, &vm), None);
    assert_eq!(intent(AppEvent::PreviewSeeked { document: gone, seconds: 1.0 }, &vm), None);
    assert_eq!(intent(AppEvent::ArrowKey { document: gone, key: ArrowKey::Left, flags: 0 }, &vm), None);
}

#[test]
fn arrow_keys_nudge_with_ctrl_small_plain_normal_and_shift_large() {
    let (h, doc) = compiled();
    let vm = h.view();
    let nudge = |key, raw| intent(AppEvent::ArrowKey { document: doc, key, flags: raw }, &vm);
    let want = |direction, step| Some(UiIntent::App(AppIntent::Nudge { document: doc, direction, step }));
    assert_eq!(nudge(ArrowKey::Left, 0), want(NudgeDirection::Left, NudgeStep::Normal));
    assert_eq!(nudge(ArrowKey::Right, flags::CONTROL), want(NudgeDirection::Right, NudgeStep::Small));
    assert_eq!(nudge(ArrowKey::Up, flags::SHIFT), want(NudgeDirection::Up, NudgeStep::Large));
    assert_eq!(nudge(ArrowKey::Down, flags::SHIFT | flags::CONTROL), want(NudgeDirection::Down, NudgeStep::Small));
    // Cmd+arrow is not ours (text and AppKit use it).
    assert_eq!(nudge(ArrowKey::Down, flags::COMMAND), None);
    assert_eq!(
        (arrow_key(123), arrow_key(124), arrow_key(125), arrow_key(126)),
        (Some(ArrowKey::Left), Some(ArrowKey::Right), Some(ArrowKey::Down), Some(ArrowKey::Up))
    );
    assert_eq!(arrow_key(0), None);
}

// ---- key bindings from events -----------------------------------------------------------------

#[test]
fn a_key_event_becomes_a_binding_and_a_binding_becomes_glyphs() {
    let cmd_shift = flags::COMMAND | flags::SHIFT;
    assert_eq!(binding_from_event("z", 6, cmd_shift), Some(key("Cmd+Shift+Z")));
    assert_eq!(binding_from_event("Z", 6, cmd_shift), Some(key("Cmd+Shift+Z")), "the shifted letter is the same key");
    assert_eq!(binding_from_event("v", 9, 0), Some(key("V")));
    assert_eq!(binding_from_event("p", 35, flags::OPTION | flags::COMMAND), Some(key("Cmd+Opt+P")));
    assert_eq!(binding_from_event("\t", 48, flags::COMMAND), Some(key("Cmd+Tab")));
    assert_eq!(binding_from_event("\u{1b}", 53, 0), Some(key("Escape")));
    assert_eq!(binding_from_event("", 123, flags::CONTROL), Some(key("Ctrl+Left")), "named keys by key code");
    assert_eq!(binding_from_event("", 120, 0), Some(key("F2")));
    assert_eq!(binding_from_event("", 255, 0), None, "a key we have no name for");
    assert_eq!(binding_from_event("ab", 0, 0), None, "composed text is not a key");

    assert_eq!(binding_glyphs(&key("Cmd+Shift+Z")), "⇧⌘Z");
    assert_eq!(binding_glyphs(&key("Ctrl+Opt+Shift+Cmd+K")), "⌃⌥⇧⌘K");
    assert_eq!(binding_glyphs(&key("V")), "V");
    assert_eq!(binding_glyphs(&key("Cmd+Tab")), "⌘⇥");
    assert_eq!(binding_glyphs(&key("Cmd+,")), "⌘,");
    assert_eq!(binding_glyphs(&key("Left")), "←");
}

// ---- preferences panel --------------------------------------------------------------------------

#[test]
fn the_preferences_panel_shows_each_value_in_the_chosen_unit_with_its_suffix() {
    let mut h = HeadlessEditorHarness::new();
    let panel = prefs_panel(&h.controller().prefs_view(), Locale::Nl);
    assert_eq!(panel.rows.len(), PrefKey::ALL.len());
    let row = |panel: &burnburn_macos::app_view::PrefsPanel, k| panel.rows.iter().find(|r| r.key == k).unwrap().clone();
    match row(&panel, PrefKey::GridSnap).control {
        PrefsControl::Field { text, suffix, invalid } => {
            assert_eq!((text.as_str(), suffix.as_str(), invalid), ("1", "mm", false));
        }
        other => panic!("{other:?}"),
    }
    h.send(UiIntent::App(AppIntent::SetPreference { key: PrefKey::Units, text: "inch".into() }));
    let panel = prefs_panel(&h.controller().prefs_view(), Locale::En);
    match row(&panel, PrefKey::NudgeNormal).control {
        PrefsControl::Field { text, suffix, .. } => assert_eq!((text.as_str(), suffix.as_str()), ("0.039", "in")),
        other => panic!("{other:?}"),
    }
    match row(&panel, PrefKey::Units).control {
        PrefsControl::Choice { titles, ids, selected } => {
            assert_eq!(titles, vec!["Millimetres", "Inches"]);
            assert_eq!((ids, selected), (vec!["mm", "inch"], 1));
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(row(&panel, PrefKey::SnapToObjects).control, PrefsControl::Check(true)));
    assert_eq!(row(&panel, PrefKey::AutosaveSeconds).caption, "Autosave (s, 0 = off)");
    assert_eq!(panel.banner, None);
}

#[test]
fn a_refused_value_marks_its_field_and_says_why_in_words() {
    let mut h = HeadlessEditorHarness::new();
    h.send(UiIntent::App(AppIntent::SetPreference { key: PrefKey::GridSnap, text: "abc".into() }));
    let panel = prefs_panel(&h.controller().prefs_view(), Locale::Nl);
    let grid = panel.rows.iter().find(|r| r.key == PrefKey::GridSnap).unwrap();
    assert!(matches!(&grid.control, PrefsControl::Field { invalid: true, .. }));
    assert_eq!(panel.banner.as_deref(), Some("Geen geldig getal."));
    h.send(UiIntent::App(AppIntent::SetPreference { key: PrefKey::GridSnap, text: "0".into() }));
    let panel = prefs_panel(&h.controller().prefs_view(), Locale::En);
    assert_eq!(panel.banner.as_deref(), Some("The value is outside the allowed range."));
}

#[test]
fn a_popup_item_is_the_id_presentation_lists_at_that_position() {
    use burnburn_macos::app_view::pref_choice_id;
    assert_eq!((pref_choice_id(PrefKey::Units, 0), pref_choice_id(PrefKey::Units, 1)), (Some("mm"), Some("inch")));
    assert_eq!(pref_choice_id(PrefKey::Language, 2), Some("en"));
    assert_eq!(pref_choice_id(PrefKey::Quality, 0), Some("coarse"));
    assert_eq!(pref_choice_id(PrefKey::Quality, 2), None);
    assert_eq!(pref_choice_id(PrefKey::GridSnap, 0), None, "a number field has no items");
    // The pop-up titles and the ids stay in step.
    let mut h = HeadlessEditorHarness::new();
    for row in prefs_panel(&h.controller().prefs_view(), Locale::En).rows {
        if let PrefsControl::Choice { titles, ids, .. } = row.control {
            assert_eq!(titles.len(), ids.len(), "{:?}", row.key);
            for (i, id) in ids.iter().enumerate() {
                assert_eq!(pref_choice_id(row.key, i), Some(*id));
            }
        }
    }
}

#[test]
fn the_inspector_caption_names_the_unit_the_field_is_shown_in() {
    use burnburn_macos::panels::field_unit_for;
    use burnburn_presentation::FieldKey;
    use burnburn_types::Unit;
    for key in [FieldKey::X, FieldKey::Y, FieldKey::Width, FieldKey::Height] {
        assert_eq!(field_unit_for(key, Unit::Millimeter, Locale::Nl), "mm");
        assert_eq!(field_unit_for(key, Unit::Inch, Locale::Nl), "in");
        assert_eq!(field_unit_for(key, Unit::Inch, Locale::En), "in");
    }
    // Angles and percentages are not lengths.
    assert_eq!(field_unit_for(FieldKey::Rotation, Unit::Inch, Locale::En), "°");
    assert_eq!(field_unit_for(FieldKey::ScaleX, Unit::Inch, Locale::En), "%");
}

// ---- shortcut editor lines ----------------------------------------------------------------------

#[test]
fn shortcut_lines_are_named_like_the_menus_sorted_and_filterable() {
    let mut h = HeadlessEditorHarness::new();
    let view = h.controller().shortcuts_view();
    let lines = shortcut_lines(&view, "", Locale::En);
    assert_eq!(lines.len(), view.rows.len());
    let undo = lines.iter().find(|l| l.id == "cmd.Undo").unwrap();
    assert_eq!((undo.name.as_str(), undo.key_text.as_str(), undo.default_text.as_str()), ("Undo", "⌘Z", "⌘Z"));
    let select = lines.iter().find(|l| l.id == "tool.select").unwrap();
    assert_eq!(select.key_text, "V");
    assert!(lines.windows(2).all(|w| w[0].name.to_lowercase() <= w[1].name.to_lowercase()), "sorted by name");
    let filtered = shortcut_lines(&view, "UNDO", Locale::En);
    assert!(filtered.iter().any(|l| l.id == "cmd.Undo") && filtered.len() < 5, "{} lines", filtered.len());
    let dutch = shortcut_lines(&view, "", Locale::Nl);
    assert_eq!(dutch.iter().find(|l| l.id == "cmd.Save").unwrap().name, "Bewaar");
    assert!(lines.iter().all(|l| !l.name.contains("cmd.") && !l.name.contains("tool.")), "never a raw id");
}

#[test]
fn a_customized_line_says_so_and_an_unbound_one_says_none() {
    let mut h = HeadlessEditorHarness::new();
    h.send(UiIntent::App(AppIntent::ClearShortcut { id: "cmd.Undo".into() }));
    let lines = shortcut_lines(&h.controller().shortcuts_view(), "", Locale::En);
    let undo = lines.iter().find(|l| l.id == "cmd.Undo").unwrap();
    assert!(undo.customized);
    assert_eq!(undo.key_text, "none");
    assert_eq!(undo.default_text, "⌘Z");
    let problem = ShortcutProblem::Conflict { with: "cmd.Save".into() };
    let text =
        burnburn_macos::app_view::shortcut_problem_text(&problem, "⌘S", &h.controller().shortcuts_view(), Locale::En);
    assert_eq!(text, "⌘S is already used by Save");
}

// ---- history -----------------------------------------------------------------------------------

#[test]
fn the_history_lists_the_start_every_step_and_marks_the_current_state_and_the_undone_ones() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (30.0, 30.0));
    h.invoke(doc, CommandKey::ToolSelect);
    h.send(UiIntent::App(AppIntent::Nudge {
        document: doc,
        direction: NudgeDirection::Right,
        step: NudgeStep::Normal,
    }));
    h.invoke(doc, CommandKey::Undo);
    let lines = history_lines(&h.controller().history_view(doc).unwrap(), Locale::En);
    let texts: Vec<_> = lines.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(texts, ["Start (as opened)", "Draw Rectangle ◀ current state", "Move (undone)"]);
    assert_eq!(lines.iter().map(|l| l.current).collect::<Vec<_>>(), [false, true, false]);
    assert_eq!(lines.iter().map(|l| l.undone).collect::<Vec<_>>(), [false, false, true]);
}

// ---- preview -----------------------------------------------------------------------------------

#[test]
fn the_clock_reads_minutes_and_seconds_and_hours_when_needed() {
    assert_eq!(clock_text(0.0), "0:00");
    assert_eq!(clock_text(3.4), "0:03");
    assert_eq!(clock_text(64.0), "1:04");
    assert_eq!(clock_text(3723.0), "1:02:03");
    assert_eq!(clock_text(-5.0), "0:00");
    assert_eq!(clock_text(f64::NAN), "0:00");
}

#[test]
fn every_time_the_preview_shows_is_labelled_as_an_estimate() {
    let (mut h, doc) = compiled();
    for locale in [Locale::Nl, Locale::En] {
        let label = app_text(AppText::PreviewEstimate, locale);
        let view = h.controller().preview_view(doc).unwrap();
        let panel = preview_panel(&view, locale);
        assert!(panel.time_text.contains(label), "{}", panel.time_text);
        assert!(panel.time_text.contains('≈'), "{}", panel.time_text);
    }
    h.send(UiIntent::App(AppIntent::Preview(PreviewIntent::Seek { document: doc, seconds: 1.0 })));
    let panel = preview_panel(&h.controller().preview_view(doc).unwrap(), Locale::En);
    assert!(panel.time_text.starts_with("Time 0:01 of ≈ 0:"), "{}", panel.time_text);
    assert!(panel.time_text.ends_with("(estimate)"), "{}", panel.time_text);
}

#[test]
fn the_preview_panel_follows_play_speed_travel_stale_and_the_pick() {
    let (mut h, doc) = compiled();
    let mut panel = || preview_panel(&h_view(&mut h, doc), Locale::En);
    fn h_view(h: &mut HeadlessEditorHarness, doc: DocumentId) -> burnburn_presentation::PreviewViewModel {
        h.controller().preview_view(doc).unwrap()
    }
    let p = panel();
    assert_eq!((p.play_title, p.speed_index, p.show_travel, p.stale, p.picked), ("Play", 0, true, None, None));
    assert_eq!(p.speed_titles, ["1×", "2×", "5×", "10×", "50×"]);
    assert!(p.picked_hint.contains("Click a move"));
    let view = h_view(&mut h, doc);
    let cut = view.preview.moves.iter().position(|m| m.kind == burnburn_presentation::MoveKind::Cut).unwrap();
    let m = view.preview.moves[cut];
    let at = [(m.from.x_mm() + m.to.x_mm()) / 2.0, (m.from.y_mm() + m.to.y_mm()) / 2.0];
    h.send(UiIntent::App(AppIntent::Preview(PreviewIntent::Pick { document: doc, at_mm: at, tolerance_mm: 0.5 })));
    h.send(UiIntent::App(AppIntent::Preview(PreviewIntent::SetSpeed { document: doc, speed: PlaybackSpeed::X10 })));
    h.send(UiIntent::App(AppIntent::Preview(PreviewIntent::Play { document: doc, playing: true })));
    let mut panel = || preview_panel(&h.controller().preview_view(doc).unwrap(), Locale::En);
    let p = panel();
    assert_eq!((p.play_title, p.speed_index), ("Pause", 3));
    let picked = p.picked.expect("a pick");
    assert!(picked.contains("cut") && picked.contains("source") && picked.contains("operation"), "{picked}");
    h.invoke(doc, CommandKey::SelectAll);
    h.invoke(doc, CommandKey::FlipHorizontal);
    let p = preview_panel(&h.controller().preview_view(doc).unwrap(), Locale::Nl);
    assert!(p.stale.as_deref().is_some_and(|t| t.contains("verouderd")), "{:?}", p.stale);
}

#[test]
fn the_preview_frame_maps_millimetres_to_pixels_and_back_with_y_up_in_the_machine() {
    let frame = PreviewFrame::fit([1500.0, 900.0], [760, 456], 8);
    assert_eq!(frame.size_px(), [760, 456]);
    let origin = frame.to_px([0.0, 0.0]);
    let far = frame.to_px([1500.0, 900.0]);
    assert!(origin[1] > far[1], "machine y grows upward, pixel y downward: {origin:?} {far:?}");
    assert!(origin[0] >= 8.0 - 1e-9 && far[0] <= 760.0 - 8.0 + 1e-9, "inside the margin");
    for mm in [[0.0, 0.0], [750.0, 450.0], [1234.5, 77.7]] {
        let back = frame.to_mm(frame.to_px(mm));
        assert!((back[0] - mm[0]).abs() < 1e-9 && (back[1] - mm[1]).abs() < 1e-9, "{mm:?} -> {back:?}");
    }
    // The hit radius of a click is about three pixels, in millimetres.
    let mm_per_px = frame.mm_per_px();
    // The bed is the limit in height: 440 px inside the margin for 900 mm.
    assert!((mm_per_px - 900.0 / 440.0).abs() < 1e-6, "{mm_per_px}");
    assert_eq!(frame.hit_radius_mm(3.0), 3.0 * mm_per_px);
}

/// Whether some pixel within two pixels of `at` has (about) the colour `want`: a thin diagonal line
/// does not cover every pixel next to its rounded midpoint.
fn near(image: &burnburn_macos::panels_image::BitmapImage, at: [f64; 2], want: [u8; 3]) -> bool {
    let (cx, cy) = (at[0].round() as i64, at[1].round() as i64);
    (-2..=2).any(|dy| {
        (-2..=2).any(|dx| {
            let (x, y) = (cx + dx, cy + dy);
            if x < 0 || y < 0 || x >= i64::from(image.width) || y >= i64::from(image.height) {
                return false;
            }
            let i = (y as usize * image.width as usize + x as usize) * 4;
            (0..3).all(|c| (i32::from(image.rgba[i + c]) - i32::from(want[c])).abs() <= 40)
        })
    })
}

#[test]
fn the_preview_bitmap_draws_unplayed_cuts_red_played_green_the_pick_blue_and_travel_only_when_shown() {
    let (mut h, doc) = compiled();
    let view = h.controller().preview_view(doc).unwrap();
    let frame = PreviewFrame::fit(view.bed_mm.unwrap(), [760, 456], 8);
    let moves = &view.preview.moves;
    let cut = moves.iter().position(|m| m.kind == burnburn_presentation::MoveKind::Cut).unwrap();
    let travel = moves.iter().position(|m| m.kind == burnburn_presentation::MoveKind::Travel).unwrap();
    let mid = |i: usize| {
        let m = moves[i];
        frame.to_px([(m.from.x_mm() + m.to.x_mm()) / 2.0, (m.from.y_mm() + m.to.y_mm()) / 2.0])
    };
    let (red, grey, green, blue, white) =
        ([220, 40, 40], [170, 170, 170], [30, 160, 60], [0, 100, 255], [255, 255, 255]);
    let image = render_preview(&view, &frame);
    assert_eq!((image.width, image.height), (760, 456));
    assert!(near(&image, mid(cut), red), "an unplayed cut is red");
    assert!(!near(&image, mid(cut), green) && !near(&image, mid(cut), blue));
    assert!(near(&image, mid(travel), grey), "travel is grey");
    assert!(near(&image, [2.0, 2.0], white), "outside the bed is white");

    h.send(UiIntent::App(AppIntent::Preview(PreviewIntent::ShowTravel { document: doc, show: false })));
    let hidden = render_preview(&h.controller().preview_view(doc).unwrap(), &frame);
    assert!(!near(&hidden, mid(travel), grey), "hidden travel is not drawn");

    let total = view.estimated_total_seconds;
    h.send(UiIntent::App(AppIntent::Preview(PreviewIntent::Seek { document: doc, seconds: total })));
    let played = render_preview(&h.controller().preview_view(doc).unwrap(), &frame);
    assert!(near(&played, mid(cut), green) && !near(&played, mid(cut), red), "played is green");

    h.send(UiIntent::App(AppIntent::Preview(PreviewIntent::Seek { document: doc, seconds: 0.0 })));
    h.send(UiIntent::App(AppIntent::Preview(PreviewIntent::SelectMove { document: doc, index: cut })));
    let picked = render_preview(&h.controller().preview_view(doc).unwrap(), &frame);
    assert!(near(&picked, mid(cut), blue), "the pick is blue");
}

// ---- RD inspector ------------------------------------------------------------------------------

#[test]
fn the_rd_panel_lists_offset_bytes_name_and_decoded_record_and_warns_about_an_older_plan() {
    let (mut h, doc) = compiled();
    assert_eq!(h.controller().rd_inspect_view(doc), None);
    h.send(UiIntent::App(AppIntent::Preview(PreviewIntent::InspectRd { document: doc })));
    let view = h.controller().rd_inspect_view(doc).unwrap();
    let panel = rd_panel(&view, Locale::En);
    assert_eq!(panel.rows.len(), view.records.len());
    assert!(panel.header.contains(&format!("{}", view.records.len())), "{}", panel.header);
    let first = &panel.rows[0];
    assert_eq!(first[0], format!("{:#06X}", view.records[0].offset));
    assert_eq!(first[1], view.records[0].hex);
    assert_eq!(first[3], view.records[0].decoded);
    assert_eq!(panel.banner, None);
    h.invoke(doc, CommandKey::SelectAll);
    h.invoke(doc, CommandKey::FlipHorizontal);
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    let panel = rd_panel(&h.controller().rd_inspect_view(doc).unwrap(), Locale::Nl);
    assert!(panel.banner.as_deref().is_some_and(|b| b.contains("ouder plan")), "{:?}", panel.banner);
}

#[test]
fn tool_lines_are_named_like_the_tool_strip_in_both_languages() {
    let mut h = HeadlessEditorHarness::new();
    let view = h.controller().shortcuts_view();
    let name = |locale, id: &str| shortcut_lines(&view, "", locale).into_iter().find(|l| l.id == id).map(|l| l.name);
    assert_eq!(name(Locale::Nl, "tool.select").as_deref(), Some("Selecteren"));
    assert_eq!(name(Locale::En, "tool.select").as_deref(), Some("Select"));
    assert_eq!(name(Locale::Nl, "tool.select").map(|n| n.to_lowercase().contains("selecteren")), Some(true));
}
