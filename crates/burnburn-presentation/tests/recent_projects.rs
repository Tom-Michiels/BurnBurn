//! W2-L1-bewerken: the Archief ▸ Recente projecten list. The controller keeps it (most recent
//! first, no duplicates, at most ten), tells the host when it changes so the host can persist it,
//! and the host hands the stored list back at start.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path as FsPath, PathBuf};

use burnburn_document::DocumentSnapshot;
use burnburn_io::{Project, ProjectUuid, save_project};
use burnburn_presentation::{HeadlessEditorHarness, HostEffect, UiIntent};

fn fixture(h: &HeadlessEditorHarness, name: &str) -> PathBuf {
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    let path = PathBuf::from(format!("/work/{name}.burnburn"));
    let snapshot = DocumentSnapshot::from_parts(Vec::new(), 1).unwrap();
    save_project(h.fs(), &path, &Project::new(ProjectUuid::from_bytes([3; 16]), snapshot)).unwrap();
    path
}

fn open(h: &mut HeadlessEditorHarness, path: &FsPath) -> Vec<Vec<PathBuf>> {
    h.send(UiIntent::Open { path: path.to_path_buf() });
    h.run_tasks()
        .iter()
        .flat_map(|u| u.effects.clone())
        .filter_map(|e| match e {
            HostEffect::RecentProjectsChanged { paths } => Some(paths),
            _ => None,
        })
        .collect()
}

#[test]
fn an_opened_project_becomes_the_most_recent_and_the_host_is_told_to_persist_the_list() {
    let mut h = HeadlessEditorHarness::new();
    let (a, b) = (fixture(&h, "a"), fixture(&h, "b"));
    assert_eq!(open(&mut h, &a), vec![vec![a.clone()]]);
    assert_eq!(open(&mut h, &b), vec![vec![b.clone(), a.clone()]]);
    assert_eq!(open(&mut h, &a), vec![vec![a.clone(), b.clone()]], "reopening moves it to the front, no duplicate");
    assert_eq!(h.controller().recent_projects(), vec![a, b]);
}

#[test]
fn only_ten_are_kept() {
    let mut h = HeadlessEditorHarness::new();
    let paths: Vec<PathBuf> = (0..12).map(|i| fixture(&h, &format!("p{i}"))).collect();
    for p in &paths {
        open(&mut h, p);
    }
    let recent = h.controller().recent_projects();
    assert_eq!(recent.len(), 10);
    assert_eq!(recent[0], paths[11]);
    assert_eq!(recent[9], paths[2]);
}

#[test]
fn a_saved_project_is_recent_too_but_a_recovered_one_is_not() {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    let doc = h.new_document().unwrap();
    let target = PathBuf::from("/work/saved.burnburn");
    h.send(UiIntent::Save { document: doc, path: Some(target.clone()) });
    let effects: Vec<_> = h.run_tasks().iter().flat_map(|u| u.effects.clone()).collect();
    assert!(effects.contains(&HostEffect::RecentProjectsChanged { paths: vec![target.clone()] }), "{effects:?}");
    assert_eq!(h.controller().recent_projects(), vec![target]);
}

#[test]
fn the_host_restores_the_stored_list_and_a_clear_empties_it() {
    let mut h = HeadlessEditorHarness::new();
    let list: Vec<PathBuf> = ["/x/a.burnburn", "/x/b.burnburn", "/x/a.burnburn"].iter().map(PathBuf::from).collect();
    let update = h.send(UiIntent::SetRecentProjects { paths: list });
    assert!(update.effects.is_empty(), "loading the stored list is not a change to persist");
    assert_eq!(h.controller().recent_projects(), vec![PathBuf::from("/x/a.burnburn"), PathBuf::from("/x/b.burnburn")]);
    let update = h.send(UiIntent::ClearRecentProjects);
    assert_eq!(update.effects, vec![HostEffect::RecentProjectsChanged { paths: vec![] }]);
    assert!(h.controller().recent_projects().is_empty());
}

#[test]
fn a_project_that_cannot_be_opened_is_forgotten() {
    let mut h = HeadlessEditorHarness::new();
    let gone = PathBuf::from("/x/gone.burnburn");
    h.send(UiIntent::SetRecentProjects { paths: vec![gone.clone()] });
    h.send(UiIntent::Open { path: gone });
    let effects: Vec<_> = h.run_tasks().iter().flat_map(|u| u.effects.clone()).collect();
    assert!(effects.contains(&HostEffect::RecentProjectsChanged { paths: vec![] }), "{effects:?}");
    assert!(h.controller().recent_projects().is_empty());
}
