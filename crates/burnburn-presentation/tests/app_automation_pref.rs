//! API01 (G5-tekst-api): the desktop-automation preference is opt-in, persists in the user's
//! folder (never in a project) and is parsed like every other flag.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::PathBuf;
use std::sync::Arc;

use burnburn_io::memory::MemoryFileSystem;
use burnburn_presentation::{AppIntent, HeadlessEditorHarness, PrefKey, PrefProblem, PrefValue, Preferences, UiIntent};

const USER_DIR: &str = "/Users/ik/Library/Application Support/BurnBurn";

fn started(fs: Arc<MemoryFileSystem>) -> HeadlessEditorHarness {
    let mut h = HeadlessEditorHarness::with_fs(fs);
    h.controller().set_user_dir(PathBuf::from(USER_DIR));
    h
}

fn set(h: &mut HeadlessEditorHarness, text: &str) {
    h.send(UiIntent::App(AppIntent::SetPreference { key: PrefKey::DesktopAutomation, text: text.into() }));
    h.run_tasks();
}

#[test]
fn desktop_automation_is_off_until_the_user_turns_it_on() {
    assert!(!Preferences::default().desktop_automation);
    let mut h = started(Arc::new(MemoryFileSystem::new()));
    assert!(!h.controller().preferences().desktop_automation);
    let vm = h.controller().prefs_view();
    assert_eq!(vm.row(PrefKey::DesktopAutomation).unwrap().value, PrefValue::Flag(false));
}

#[test]
fn desktop_automation_survives_a_restart_and_can_be_turned_off_again() {
    let fs = Arc::new(MemoryFileSystem::new());
    let mut h = started(Arc::clone(&fs));
    set(&mut h, "on");
    assert!(h.controller().preferences().desktop_automation);
    assert_eq!(h.controller().prefs_view().row(PrefKey::DesktopAutomation).unwrap().value, PrefValue::Flag(true));

    let mut second = started(Arc::clone(&fs));
    assert!(second.controller().preferences().desktop_automation, "the next start reads the stored choice");
    set(&mut second, "off");
    let mut third = started(fs);
    assert!(!third.controller().preferences().desktop_automation);
}

#[test]
fn a_value_that_is_not_on_or_off_is_refused_and_changes_nothing() {
    let mut h = started(Arc::new(MemoryFileSystem::new()));
    set(&mut h, "maybe");
    assert!(!h.controller().preferences().desktop_automation);
    assert_eq!(
        h.controller().prefs_view().error.map(|e| (e.key, e.problem)),
        Some((PrefKey::DesktopAutomation, PrefProblem::UnknownChoice))
    );
}
