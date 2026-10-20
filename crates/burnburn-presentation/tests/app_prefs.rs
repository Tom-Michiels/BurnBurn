//! X02/X03 (W3-UI-app): the preferences window's model. Values live in the user's folder through
//! the injected filesystem, never in a project, and every field is parsed here (DR-05), not in
//! the host.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path, PathBuf};
use std::sync::Arc;

use burnburn_io::fs::FsOp;
use burnburn_io::memory::MemoryFileSystem;
use burnburn_presentation::{
    AppIntent, DisplayQuality, HeadlessEditorHarness, LanguageChoice, LengthUnit, PrefKey, PrefProblem, PrefValue,
    PrefsViewModel, UiIntent,
};

const USER_DIR: &str = "/Users/ik/Library/Application Support/BurnBurn";

fn user_file() -> PathBuf {
    Path::new(USER_DIR).join("preferences.conf")
}

fn started(fs: Arc<MemoryFileSystem>) -> HeadlessEditorHarness {
    let mut h = HeadlessEditorHarness::with_fs(fs);
    h.controller().set_user_dir(PathBuf::from(USER_DIR));
    h
}

fn set(h: &mut HeadlessEditorHarness, key: PrefKey, text: &str) {
    h.send(UiIntent::App(AppIntent::SetPreference { key, text: text.into() }));
    h.run_tasks();
}

fn number(vm: &PrefsViewModel, key: PrefKey) -> String {
    match &vm.row(key).expect("row").value {
        PrefValue::Number { text, .. } => text.clone(),
        other => panic!("{key:?} is not a number row: {other:?}"),
    }
}

#[test]
fn a_change_is_written_to_the_user_folder_and_read_back_by_the_next_start() {
    let fs = Arc::new(MemoryFileSystem::new());
    let mut h = started(Arc::clone(&fs));
    set(&mut h, PrefKey::AutosaveSeconds, "120");
    set(&mut h, PrefKey::Language, "en");
    set(&mut h, PrefKey::Quality, "coarse");
    assert!(fs.files().contains(&user_file()), "files: {:?}", fs.files());

    let mut second = started(Arc::clone(&fs));
    let prefs = *second.controller().preferences();
    assert_eq!(prefs.autosave_seconds, 120);
    assert_eq!(prefs.language, LanguageChoice::English);
    assert_eq!(prefs.quality, DisplayQuality::Coarse);
    assert_eq!(prefs.units, LengthUnit::Millimeters, "untouched values keep their default");
}

#[test]
fn lengths_are_stored_in_millimetres_and_shown_in_the_chosen_unit() {
    let mut h = started(Arc::new(MemoryFileSystem::new()));
    set(&mut h, PrefKey::GridSnap, "2,5");
    assert_eq!(h.controller().preferences().grid_snap_mm, 2.5);
    set(&mut h, PrefKey::Units, "inch");
    assert_eq!(h.controller().preferences().grid_snap_mm, 2.5, "switching the display unit keeps the physical value");
    assert_eq!(number(&h.controller().prefs_view(), PrefKey::GridSnap), "0.098");
    // A bare number now means inches; an explicit unit always wins.
    set(&mut h, PrefKey::NudgeNormal, "0.5");
    assert!((h.controller().preferences().nudge_normal_mm - 12.7).abs() < 1e-9);
    set(&mut h, PrefKey::NudgeLarge, "30 mm");
    assert_eq!(h.controller().preferences().nudge_large_mm, 30.0);
}

#[test]
fn invalid_input_is_refused_with_the_reason_and_changes_nothing() {
    let fs = Arc::new(MemoryFileSystem::new());
    let mut h = started(Arc::clone(&fs));
    let before = *h.controller().preferences();
    for (key, text, why) in [
        (PrefKey::GridSnap, "abc", PrefProblem::NotANumber),
        (PrefKey::GridSnap, "0", PrefProblem::OutOfRange),
        (PrefKey::NudgeSmall, "-1", PrefProblem::OutOfRange),
        (PrefKey::AutosaveSeconds, "5", PrefProblem::OutOfRange),
        (PrefKey::AutosaveSeconds, "10,5", PrefProblem::NotANumber),
        (PrefKey::ObjectSnapPixels, "500", PrefProblem::OutOfRange),
        (PrefKey::Language, "klingon", PrefProblem::UnknownChoice),
        (PrefKey::Units, "furlong", PrefProblem::UnknownChoice),
    ] {
        h.send(UiIntent::App(AppIntent::SetPreference { key, text: text.into() }));
        let view = h.controller().prefs_view();
        assert_eq!(view.error.map(|e| (e.key, e.problem)), Some((key, why)), "{key:?} = {text:?}");
    }
    assert_eq!(*h.controller().preferences(), before);
    assert!(h.run_tasks().is_empty(), "a refused value writes no file");
    assert!(fs.files().is_empty());
    // The next good value clears the error.
    set(&mut h, PrefKey::GridSnap, "2");
    assert_eq!(h.controller().prefs_view().error, None);
}

#[test]
fn autosave_can_be_switched_off_with_zero() {
    let mut h = started(Arc::new(MemoryFileSystem::new()));
    set(&mut h, PrefKey::AutosaveSeconds, "0");
    assert_eq!(h.controller().preferences().autosave_seconds, 0);
}

#[test]
fn snap_defaults_are_what_a_new_window_starts_with_and_do_not_reach_open_windows() {
    let mut h = started(Arc::new(MemoryFileSystem::new()));
    let open = h.new_document().expect("window");
    set(&mut h, PrefKey::SnapToGrid, "on");
    set(&mut h, PrefKey::GridSnap, "2");
    set(&mut h, PrefKey::SnapToObjects, "off");
    set(&mut h, PrefKey::ObjectSnapPixels, "12");
    let fresh = h.new_document().expect("second window");
    let snap = h.document(fresh).expect("view").snap_settings;
    assert_eq!(snap.grid_mm, Some(2.0));
    assert!(!snap.objects);
    assert_eq!(snap.radius_pt, 12.0);
    let old = h.document(open).expect("view").snap_settings;
    assert_eq!(old.grid_mm, None, "a window keeps the snapping it was given");
}

#[test]
fn preferences_are_never_part_of_a_project_file() {
    let save_with = |autosave: &str| {
        let fs = Arc::new(MemoryFileSystem::new());
        let mut h = started(Arc::clone(&fs));
        set(&mut h, PrefKey::AutosaveSeconds, autosave);
        set(&mut h, PrefKey::GridSnap, "3");
        let doc = h.new_document().expect("window");
        let path = PathBuf::from("/work/p.burnburn");
        fs.create_dir_all_for_test(Path::new("/work"));
        h.send(UiIntent::Save { document: doc, path: Some(path.clone()) });
        h.run_tasks();
        burnburn_io::fs::FileSystem::read(fs.as_ref(), &path, u64::MAX).expect("project written")
    };
    assert_eq!(save_with("120"), save_with("900"), "the project does not depend on any preference");
}

#[test]
fn a_damaged_file_gives_defaults_and_says_so_and_is_only_replaced_by_a_deliberate_change() {
    let fs = Arc::new(MemoryFileSystem::new());
    fs.insert(&user_file(), b"\xff\xfe not a preferences file \x00".to_vec());
    let mut h = started(Arc::clone(&fs));
    assert_eq!(*h.controller().preferences(), burnburn_presentation::Preferences::default());
    assert!(h.controller().prefs_view().load_problem);
    let untouched = burnburn_io::fs::FileSystem::read(fs.as_ref(), &user_file(), u64::MAX).unwrap();
    assert_eq!(untouched, b"\xff\xfe not a preferences file \x00".to_vec(), "reading never rewrites");
    set(&mut h, PrefKey::GridSnap, "2");
    assert!(!h.controller().prefs_view().load_problem);
    let text = String::from_utf8(burnburn_io::fs::FileSystem::read(fs.as_ref(), &user_file(), u64::MAX).unwrap())
        .expect("text");
    assert!(text.contains("grid_snap_mm"), "{text}");
}

#[test]
fn a_failed_write_is_reported_and_the_previous_file_survives() {
    let fs = Arc::new(MemoryFileSystem::new());
    let mut h = started(Arc::clone(&fs));
    set(&mut h, PrefKey::AutosaveSeconds, "60");
    let good = burnburn_io::fs::FileSystem::read(fs.as_ref(), &user_file(), u64::MAX).unwrap();
    fs.fail_on(FsOp::Replace, 0);
    set(&mut h, PrefKey::AutosaveSeconds, "90");
    assert_eq!(burnburn_io::fs::FileSystem::read(fs.as_ref(), &user_file(), u64::MAX).unwrap(), good);
    assert!(h.controller().prefs_view().save_failed, "the window says the file was not saved");
    // The running app still uses what the user chose.
    assert_eq!(h.controller().preferences().autosave_seconds, 90);
    set(&mut h, PrefKey::AutosaveSeconds, "100");
    assert!(!h.controller().prefs_view().save_failed, "a later successful write clears the report");
}

#[test]
fn reset_restores_the_defaults_and_keys_this_version_does_not_know_survive_a_rewrite() {
    let fs = Arc::new(MemoryFileSystem::new());
    fs.insert(&user_file(), b"burnburn-preferences 1\nfuture_option = 42\nautosave_seconds = 300\n".to_vec());
    let mut h = started(Arc::clone(&fs));
    assert_eq!(h.controller().preferences().autosave_seconds, 300);
    set(&mut h, PrefKey::GridSnap, "4");
    let text = String::from_utf8(burnburn_io::fs::FileSystem::read(fs.as_ref(), &user_file(), u64::MAX).unwrap())
        .expect("text");
    assert!(text.contains("future_option = 42"), "{text}");
    h.send(UiIntent::App(AppIntent::ResetPreferences));
    h.run_tasks();
    assert_eq!(*h.controller().preferences(), burnburn_presentation::Preferences::default());
    let text = String::from_utf8(burnburn_io::fs::FileSystem::read(fs.as_ref(), &user_file(), u64::MAX).unwrap())
        .expect("text");
    assert!(text.contains("future_option = 42"), "reset does not throw away what a newer version wrote: {text}");
    assert!(!text.contains("autosave_seconds = 300"), "{text}");
}

#[test]
fn without_a_user_folder_changes_apply_for_this_run_and_write_nothing() {
    let fs = Arc::new(MemoryFileSystem::new());
    let mut h = HeadlessEditorHarness::with_fs(Arc::clone(&fs));
    set(&mut h, PrefKey::GridSnap, "2");
    assert_eq!(h.controller().preferences().grid_snap_mm, 2.0);
    assert!(fs.files().is_empty());
}

#[test]
fn quick_successive_changes_are_written_one_at_a_time_and_the_last_one_wins() {
    let fs = Arc::new(MemoryFileSystem::new());
    let mut h = started(Arc::clone(&fs));
    h.send(UiIntent::App(AppIntent::SetPreference { key: PrefKey::AutosaveSeconds, text: "60".into() }));
    h.send(UiIntent::App(AppIntent::SetPreference { key: PrefKey::AutosaveSeconds, text: "70".into() }));
    h.send(UiIntent::App(AppIntent::SetPreference { key: PrefKey::AutosaveSeconds, text: "80".into() }));
    assert_eq!(h.take_task_requests().len(), 1, "one write in flight, the rest waits for it");
    // The test lets that one request go by and delivers nothing: nothing else may be submitted.
    let mut h = started(Arc::clone(&fs));
    h.send(UiIntent::App(AppIntent::SetPreference { key: PrefKey::AutosaveSeconds, text: "60".into() }));
    h.send(UiIntent::App(AppIntent::SetPreference { key: PrefKey::AutosaveSeconds, text: "80".into() }));
    h.run_tasks();
    let text = String::from_utf8(burnburn_io::fs::FileSystem::read(fs.as_ref(), &user_file(), u64::MAX).unwrap())
        .expect("text");
    assert!(text.contains("autosave_seconds = 80"), "{text}");
}

#[test]
fn the_snap_to_grid_command_uses_the_chosen_grid_distance() {
    let mut h = started(Arc::new(MemoryFileSystem::new()));
    set(&mut h, PrefKey::GridSnap, "4");
    let doc = h.new_document().expect("window");
    assert_eq!(h.document(doc).unwrap().snap_settings.grid_mm, None, "grid snapping stays off until switched on");
    h.invoke(doc, burnburn_presentation::CommandKey::SnapToGrid);
    assert_eq!(h.document(doc).unwrap().snap_settings.grid_mm, Some(4.0));
    h.invoke(doc, burnburn_presentation::CommandKey::SnapToGrid);
    assert_eq!(h.document(doc).unwrap().snap_settings.grid_mm, None);
}

// --- nudge distances ------------------------------------------------------------------------

use burnburn_presentation::{CommandKey, FieldKey, NudgeDirection, NudgeStep};

fn drawn_square(h: &mut HeadlessEditorHarness) -> burnburn_types::DocumentId {
    let doc = h.new_document().expect("window");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (30.0, 30.0));
    h.invoke(doc, CommandKey::ToolSelect);
    doc
}

fn position(h: &HeadlessEditorHarness, doc: burnburn_types::DocumentId) -> (String, String) {
    let view = h.document(doc).expect("view");
    let inspector = view.inspector.expect("a selection has an inspector");
    let text = |k| inspector.field(k).expect("field").text.clone();
    (text(FieldKey::X), text(FieldKey::Y))
}

fn nudge(h: &mut HeadlessEditorHarness, doc: burnburn_types::DocumentId, direction: NudgeDirection, step: NudgeStep) {
    h.send(UiIntent::App(AppIntent::Nudge { document: doc, direction, step }));
}

#[test]
fn nudging_moves_the_selection_by_the_chosen_distance_and_up_is_up() {
    let mut h = started(Arc::new(MemoryFileSystem::new()));
    set(&mut h, PrefKey::NudgeSmall, "0,25");
    set(&mut h, PrefKey::NudgeNormal, "2");
    set(&mut h, PrefKey::NudgeLarge, "15");
    let doc = drawn_square(&mut h);
    let (x0, y0) = position(&h, doc);
    nudge(&mut h, doc, NudgeDirection::Right, NudgeStep::Normal);
    assert_eq!(position(&h, doc), (format!("{}", x0.parse::<f64>().unwrap() + 2.0), y0.clone()));
    nudge(&mut h, doc, NudgeDirection::Up, NudgeStep::Large);
    let (_, y) = position(&h, doc);
    assert_eq!(y.parse::<f64>().unwrap(), y0.parse::<f64>().unwrap() + 15.0, "world y points up");
    nudge(&mut h, doc, NudgeDirection::Left, NudgeStep::Small);
    let (x, _) = position(&h, doc);
    assert_eq!(x.parse::<f64>().unwrap(), x0.parse::<f64>().unwrap() + 2.0 - 0.25);
}

#[test]
fn every_nudge_is_one_undo_entry_and_nothing_selected_means_nothing_happens() {
    let mut h = started(Arc::new(MemoryFileSystem::new()));
    let doc = drawn_square(&mut h);
    let before = position(&h, doc);
    let revision = h.document(doc).unwrap().revision;
    nudge(&mut h, doc, NudgeDirection::Down, NudgeStep::Normal);
    assert_ne!(position(&h, doc), before);
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(position(&h, doc), before, "one nudge, one undo");
    h.send(UiIntent::Select { document: doc, ids: vec![], mode: burnburn_presentation::SelectMode::Replace });
    let revision_after_undo = h.document(doc).unwrap().revision;
    assert_ne!(revision, revision_after_undo, "sanity: undo is a revision");
    let update = h.send(UiIntent::App(AppIntent::Nudge {
        document: doc,
        direction: NudgeDirection::Left,
        step: NudgeStep::Normal,
    }));
    assert!(update.effects.is_empty());
    assert_eq!(h.document(doc).unwrap().revision, revision_after_undo, "no selection: no command");
}

#[test]
fn a_locked_object_is_not_nudged() {
    let mut h = started(Arc::new(MemoryFileSystem::new()));
    let doc = drawn_square(&mut h);
    h.invoke(doc, CommandKey::LockSelected);
    let before = position(&h, doc);
    let update = h.send(UiIntent::App(AppIntent::Nudge {
        document: doc,
        direction: NudgeDirection::Right,
        step: NudgeStep::Normal,
    }));
    assert_eq!(position(&h, doc), before);
    assert!(!update.effects.is_empty(), "the refusal is reported");
}

#[test]
fn the_stored_preferences_can_be_read_before_any_controller_exists() {
    // The host needs the language to build its menus, before the first window.
    let fs = Arc::new(MemoryFileSystem::new());
    assert_eq!(
        burnburn_presentation::AppController::read_preferences(fs.as_ref(), Path::new(USER_DIR)),
        burnburn_presentation::Preferences::default(),
        "no file: defaults"
    );
    fs.insert(&user_file(), b"burnburn-preferences 1\nlanguage = en\nautosave_seconds = 120\n".to_vec());
    let prefs = burnburn_presentation::AppController::read_preferences(fs.as_ref(), Path::new(USER_DIR));
    assert_eq!((prefs.language, prefs.autosave_seconds), (LanguageChoice::English, 120));
    fs.insert(&user_file(), b"garbage".to_vec());
    assert_eq!(
        burnburn_presentation::AppController::read_preferences(fs.as_ref(), Path::new(USER_DIR)),
        burnburn_presentation::Preferences::default(),
        "a damaged file: defaults"
    );
}
