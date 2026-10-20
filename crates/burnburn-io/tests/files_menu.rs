//! W2-L6, File menu: recent projects/templates (M017/M018) and project templates (M024). Storage
//! goes through the injected `FileSystem`; the clock is a parameter.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers and let-else in tests

mod common;

use std::path::{Path, PathBuf};

use burnburn_document::process::OperationMode;
use burnburn_io::fs::{FileSystem, FsOp};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::recent::{RecentError, RecentIssue, RecentKind, RecentList, RecentStore};
use burnburn_io::template::{
    PROJECT_EXTENSION, TEMPLATE_EXTENSION, TemplateContent, TemplateError, new_from_template, save_template,
};
use burnburn_io::{Project, ProjectUuid, load_project, save_project};

fn p(s: &str) -> PathBuf {
    PathBuf::from(s)
}

// ---------------------------------------------------------------- recent lists

#[test]
fn touch_moves_to_front_dedupes_by_path_and_keeps_only_the_newest_max() {
    let mut list = RecentList::new(3);
    list.touch(&p("/w/a.burnburn"), "a", 10);
    list.touch(&p("/w/b.burnburn"), "b", 20);
    list.touch(&p("/w/c.burnburn"), "c", 30);
    list.touch(&p("/w/a.burnburn"), "a (renamed)", 40);
    let names: Vec<&str> = list.entries().iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["a (renamed)", "c", "b"], "re-opening moves to the front and updates name and time");
    assert_eq!(list.entries()[0].opened, 40);
    list.touch(&p("/w/d.burnburn"), "d", 50);
    let paths: Vec<&Path> = list.entries().iter().map(|e| e.path.as_path()).collect();
    assert_eq!(paths, [Path::new("/w/d.burnburn"), Path::new("/w/a.burnburn"), Path::new("/w/c.burnburn")]);
    assert!(list.remove(&p("/w/a.burnburn")) && !list.remove(&p("/w/a.burnburn")));
    assert_eq!(list.entries().len(), 2);
}

#[test]
fn the_list_persists_through_the_injected_filesystem_and_projects_and_templates_are_separate() {
    let fs = MemoryFileSystem::new();
    let dir = p("/user/BurnBurn");
    let store = RecentStore::new(&fs, &dir);
    let mut projects = RecentList::new(10);
    projects.touch(&p("/w/a.burnburn"), "Plaat", 100);
    projects.touch(&p("/w/b.burnburn"), "Doos", 200);
    store.save(RecentKind::Projects, &projects).unwrap();
    let mut templates = RecentList::new(10);
    templates.touch(&p("/t/basis.burnburntemplate"), "Basis", 300);
    store.save(RecentKind::Templates, &templates).unwrap();

    // A fresh store (a new process) sees the same lists.
    let again = RecentStore::new(&fs, &dir);
    let loaded = again.load(RecentKind::Projects);
    assert_eq!(loaded.issue, None);
    assert_eq!(loaded.list.entries(), projects.entries());
    assert_eq!(again.load(RecentKind::Templates).list.entries(), templates.entries());
    assert_ne!(again.file_path(RecentKind::Projects), again.file_path(RecentKind::Templates));
    // Nothing but those two files in the user directory (no temp files left behind).
    assert_eq!(fs.files().len(), 2, "{:?}", fs.files());
}

#[test]
fn a_missing_list_is_empty_and_prune_removes_only_files_that_are_gone() {
    let fs = MemoryFileSystem::new();
    let store = RecentStore::new(&fs, &p("/user"));
    let loaded = store.load(RecentKind::Projects);
    assert!(loaded.list.entries().is_empty() && loaded.issue.is_none(), "first start is not an error");

    fs.insert(&p("/w/here.burnburn"), vec![1]);
    let mut list = RecentList::new(10);
    list.touch(&p("/w/gone.burnburn"), "weg", 1);
    list.touch(&p("/w/here.burnburn"), "hier", 2);
    // Loading never touches the project files; pruning is an explicit step.
    let removed = list.prune_missing(&fs).unwrap();
    assert_eq!(removed.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["weg"]);
    assert_eq!(list.entries().len(), 1);
}

#[test]
fn a_corrupt_list_starts_empty_with_an_issue_and_is_only_replaced_by_an_explicit_save() {
    let fs = MemoryFileSystem::new();
    let store = RecentStore::new(&fs, &p("/user"));
    let path = store.file_path(RecentKind::Projects);
    fs.insert(&path, b"{ not json".to_vec());
    let loaded = store.load(RecentKind::Projects);
    assert!(loaded.list.entries().is_empty());
    assert!(matches!(loaded.issue, Some(RecentIssue::Corrupt(_))), "{:?}", loaded.issue);
    assert_eq!(fs.read(&path, 1 << 20).unwrap(), b"{ not json", "load does not rewrite the file");
    let mut list = RecentList::new(10);
    list.touch(&p("/w/a.burnburn"), "a", 1);
    store.save(RecentKind::Projects, &list).unwrap();
    assert_eq!(store.load(RecentKind::Projects).list.entries().len(), 1);
}

#[test]
fn a_list_from_a_newer_version_is_reported_and_never_overwritten() {
    let fs = MemoryFileSystem::new();
    let store = RecentStore::new(&fs, &p("/user"));
    let path = store.file_path(RecentKind::Projects);
    let newer = br#"{"schema":99,"entries":[],"future":true}"#.to_vec();
    fs.insert(&path, newer.clone());
    let loaded = store.load(RecentKind::Projects);
    assert_eq!(loaded.issue, Some(RecentIssue::NewerSchema { found: 99, supported: 1 }));
    let mut list = RecentList::new(10);
    list.touch(&p("/w/a.burnburn"), "a", 1);
    assert_eq!(store.save(RecentKind::Projects, &list), Err(RecentError::NewerSchema { found: 99, supported: 1 }));
    assert_eq!(fs.read(&path, 1 << 20).unwrap(), newer);
}

#[test]
fn a_failed_save_keeps_the_previous_list_readable() {
    let fs = MemoryFileSystem::new();
    let store = RecentStore::new(&fs, &p("/user"));
    let mut list = RecentList::new(10);
    list.touch(&p("/w/a.burnburn"), "a", 1);
    store.save(RecentKind::Projects, &list).unwrap();
    list.touch(&p("/w/b.burnburn"), "b", 2);
    fs.fail_on(FsOp::Replace, 0);
    assert!(matches!(store.save(RecentKind::Projects, &list), Err(RecentError::Fs(_))));
    let loaded = store.load(RecentKind::Projects);
    assert_eq!(loaded.issue, None);
    assert_eq!(loaded.list.entries().len(), 1, "the old list is intact after a failed replace");
}

// ------------------------------------------------------------------- templates

fn template_path() -> PathBuf {
    p(&format!("/t/basis.{TEMPLATE_EXTENSION}"))
}

#[test]
fn a_template_gives_a_new_project_with_a_fresh_identity_and_the_same_content() {
    let fs = MemoryFileSystem::new();
    let mut source = common::sample_project();
    source.freeze_revision("v1").unwrap();
    save_template(&fs, &template_path(), &source, TemplateContent::Everything).unwrap();
    let new_uuid = ProjectUuid::from_bytes(*b"brand-new-uuid!!");
    let fresh = new_from_template(&fs, &template_path(), new_uuid).unwrap();
    assert_eq!(fresh.uuid, new_uuid);
    assert_ne!(fresh.uuid, source.uuid, "a new project never shares the template's identity");
    assert!(fresh.revisions.is_empty(), "the source project's frozen revisions are not part of a template");
    assert!(fresh.document.same_content(&source.document));
    assert_eq!(fresh.assets.len(), source.assets.len());
}

#[test]
fn a_settings_only_template_keeps_operations_and_drops_geometry_and_unused_assets() {
    let fs = MemoryFileSystem::new();
    let source = common::sample_project();
    save_template(&fs, &template_path(), &source, TemplateContent::SettingsOnly).unwrap();
    let fresh = new_from_template(&fs, &template_path(), ProjectUuid::from_bytes(*b"another-uuid-ok!")).unwrap();
    assert_eq!(fresh.document.len(), 0, "no geometry");
    let ops: Vec<_> = fresh.document.process().operations().collect();
    let src_ops: Vec<_> = source.document.process().operations().collect();
    assert_eq!(ops.len(), 2);
    for (a, b) in ops.iter().zip(&src_ops) {
        assert_eq!(
            (a.name(), a.settings(), a.mode(), a.output_enabled()),
            (b.name(), b.settings(), b.mode(), b.output_enabled())
        );
        assert!(a.targets().is_empty(), "targets pointed at geometry that is not in the template");
    }
    assert!(matches!(ops[0].mode(), OperationMode::Line { .. }));
    assert!(fresh.assets.is_empty(), "the imported logo asset belonged to dropped geometry");
}

#[test]
fn using_a_template_never_writes_to_it() {
    let fs = MemoryFileSystem::new();
    save_template(&fs, &template_path(), &common::sample_project(), TemplateContent::Everything).unwrap();
    let before = fs.read(&template_path(), 1 << 30).unwrap();
    let writes_before = fs.log().iter().filter(|(op, _)| !matches!(op, FsOp::Read | FsOp::Exists)).count();
    let _ = new_from_template(&fs, &template_path(), ProjectUuid::from_bytes(*b"yet-another-uuid")).unwrap();
    assert_eq!(fs.read(&template_path(), 1 << 30).unwrap(), before);
    let writes_after = fs.log().iter().filter(|(op, _)| !matches!(op, FsOp::Read | FsOp::Exists)).count();
    assert_eq!(writes_before, writes_after, "new-from-template performs no write operation at all");
}

#[test]
fn template_and_project_extensions_are_not_interchangeable() {
    let fs = MemoryFileSystem::new();
    let project = common::sample_project();
    let as_project = p(&format!("/w/mijn.{PROJECT_EXTENSION}"));
    // Saving a template over a project file is refused, and a project is not opened as a template.
    assert_eq!(
        save_template(&fs, &as_project, &project, TemplateContent::Everything),
        Err(TemplateError::WrongExtension { expected: TEMPLATE_EXTENSION })
    );
    save_project(&fs, &as_project, &project).unwrap();
    assert_eq!(
        new_from_template(&fs, &as_project, ProjectUuid::from_bytes(*b"uuid-uuid-uuid-1")).unwrap_err(),
        TemplateError::WrongExtension { expected: TEMPLATE_EXTENSION }
    );
    assert!(fs.files().iter().all(|f| f != &template_path()));
}

#[test]
fn a_template_file_is_a_valid_container_that_a_project_reader_can_inspect() {
    let fs = MemoryFileSystem::new();
    save_template(&fs, &template_path(), &common::sample_project(), TemplateContent::Everything).unwrap();
    let as_project: Project = load_project(&fs, &template_path()).unwrap();
    assert_eq!(as_project.document.len(), 2);
}

#[test]
fn the_real_filesystem_adapter_keeps_the_lists_and_templates_in_a_user_directory() {
    use burnburn_io::std_fs::StdFileSystem;
    let tmp = tempfile::tempdir().unwrap();
    let fs = StdFileSystem;
    let user_dir = tmp.path().join("BurnBurn"); // does not exist yet: the first save creates it
    let store = RecentStore::new(&fs, &user_dir);
    let template = tmp.path().join(format!("basis.{TEMPLATE_EXTENSION}"));
    save_template(&fs, &template, &common::sample_project(), TemplateContent::Everything).unwrap();
    let mut list = RecentList::new(10);
    list.touch(&template, "Basis", 5);
    store.save(RecentKind::Templates, &list).unwrap();
    let loaded = RecentStore::new(&fs, &user_dir).load(RecentKind::Templates);
    assert_eq!(loaded.list.entries(), list.entries());
    let mut again = loaded.list;
    assert!(again.prune_missing(&fs).unwrap().is_empty(), "the template exists on disk");
}
