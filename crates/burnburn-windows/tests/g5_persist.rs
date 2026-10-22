//! G5-windows: the recent projects and templates on disk, headless (a temporary folder, never the
//! user's BurnBurn folder).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use burnburn_presentation::{FilesIntent, HeadlessEditorHarness, HostEffect, UiIntent};
use burnburn_windows::persist::{load_intents, persist_effect, recent_projects_file, recent_templates_file};

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        let dir = std::env::temp_dir().join(format!("burnburn-g5-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn paths(list: &[&str]) -> Vec<PathBuf> {
    list.iter().map(PathBuf::from).collect()
}

#[test]
fn a_changed_template_list_is_written_and_comes_back_as_the_set_intent() {
    let dir = TempDir::new("templates");
    let list = paths(&["C:\\t\\a.burnburntemplate", "C:\\t\\b.burnburntemplate"]);
    let result = persist_effect(dir.path(), &HostEffect::RecentTemplatesChanged { paths: list.clone() });
    assert!(matches!(result, Some(Ok(()))), "{result:?}");
    assert!(recent_templates_file(dir.path()).is_file());
    assert!(!recent_projects_file(dir.path()).exists(), "the project list is untouched");
    assert!(load_intents(dir.path()).contains(&UiIntent::Files(FilesIntent::SetRecentTemplates { paths: list })));
}

#[test]
fn a_changed_project_list_is_written_and_comes_back_as_the_set_intent() {
    let dir = TempDir::new("projects");
    let list = paths(&["C:\\w\\x.burnburn"]);
    let result = persist_effect(dir.path(), &HostEffect::RecentProjectsChanged { paths: list.clone() });
    assert!(matches!(result, Some(Ok(()))), "{result:?}");
    assert!(load_intents(dir.path()).contains(&UiIntent::SetRecentProjects { paths: list }));
}

#[test]
fn clearing_a_list_persists_the_empty_list() {
    let dir = TempDir::new("clear");
    persist_effect(dir.path(), &HostEffect::RecentTemplatesChanged { paths: paths(&["C:\\t\\a.burnburntemplate"]) });
    persist_effect(dir.path(), &HostEffect::RecentTemplatesChanged { paths: Vec::new() });
    assert!(load_intents(dir.path()).contains(&UiIntent::Files(FilesIntent::SetRecentTemplates { paths: Vec::new() })));
}

#[test]
fn other_effects_are_not_persisted() {
    let dir = TempDir::new("other");
    assert!(persist_effect(dir.path(), &HostEffect::ShowTemplateOpenPanel).is_none());
    assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none(), "nothing was written");
}

#[test]
fn missing_and_damaged_files_are_empty_lists_not_errors() {
    let dir = TempDir::new("damaged");
    let none = UiIntent::Files(FilesIntent::SetRecentTemplates { paths: Vec::new() });
    assert!(load_intents(dir.path()).contains(&none), "missing file");
    std::fs::write(recent_templates_file(dir.path()), b"{ not json").unwrap();
    std::fs::write(recent_projects_file(dir.path()), [0xff, 0xfe, 0x00]).unwrap();
    let intents = load_intents(dir.path());
    assert!(intents.contains(&none), "damaged templates file");
    assert!(intents.contains(&UiIntent::SetRecentProjects { paths: Vec::new() }), "damaged projects file");
}

#[test]
fn the_stored_lists_survive_a_restart_through_a_real_controller() {
    // Controller 1 notes a template; its effect is stored; a new controller restores it.
    let dir = TempDir::new("restart");
    let list = paths(&["C:\\t\\vorm.burnburntemplate"]);
    persist_effect(dir.path(), &HostEffect::RecentTemplatesChanged { paths: list.clone() });
    let mut harness = HeadlessEditorHarness::new();
    for intent in load_intents(dir.path()) {
        harness.send(intent);
    }
    assert_eq!(harness.controller().recent_templates(), list);
}
