//! X07 / W4-release: saving over a project written by an older (or unreadable) build keeps a
//! recoverable copy of the previous file; a newer file is never overwritten by an older build.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

mod common;

use std::path::{Path, PathBuf};

use burnburn_io::fs::{FileSystem, FsOp};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{
    LoadError, SCHEMA_VERSION, SaveError, container_schema, encode_container, load_project,
    save_project_keeping_previous,
};
use common::{rewrite_json, sample_project};

const LIMIT: u64 = 64 * 1024 * 1024;

fn v1_bytes() -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/projects/v1/rectangle-120x80.burnburn"))
        .unwrap()
}

fn target() -> PathBuf {
    PathBuf::from("/work/plaat.burnburn")
}

fn disk_bytes(fs: &MemoryFileSystem, path: &Path) -> Vec<u8> {
    fs.read(path, LIMIT).unwrap()
}

#[test]
fn container_schema_reads_the_manifest_without_loading_the_project() {
    assert_eq!(container_schema(&v1_bytes()).unwrap(), 1);
    assert_eq!(container_schema(&encode_container(&sample_project()).unwrap()).unwrap(), SCHEMA_VERSION);
    assert!(container_schema(b"not a zip").is_err());
}

#[test]
fn saving_over_an_older_schema_file_keeps_a_byte_identical_copy_first() {
    let fs = MemoryFileSystem::new();
    let original = v1_bytes();
    fs.insert(&target(), original.clone());
    let project = load_project(&fs, &target()).unwrap();

    let outcome = save_project_keeping_previous(&fs, &target(), &project).unwrap();

    let backup = outcome.backup.expect("an older file must be backed up");
    assert_eq!(backup, PathBuf::from("/work/plaat.burnburn.schema1.bak"));
    assert_eq!(disk_bytes(&fs, &backup), original);
    assert_eq!(container_schema(&disk_bytes(&fs, &target())).unwrap(), SCHEMA_VERSION);
    // The copy is a complete, loadable project of its own schema: the user can go back.
    assert_eq!(load_project(&fs, &backup).unwrap().uuid, project.uuid);
    // And it survives a power cut right after the save.
    fs.simulate_power_loss();
    assert_eq!(disk_bytes(&fs, &backup), original);
}

#[test]
fn a_file_of_the_current_schema_and_a_first_save_need_no_backup() {
    let fs = MemoryFileSystem::new();
    let project = sample_project();
    let first = save_project_keeping_previous(&fs, &target(), &project).unwrap();
    assert_eq!(first.backup, None);
    let second = save_project_keeping_previous(&fs, &target(), &project).unwrap();
    assert_eq!(second.backup, None);
    assert_eq!(fs.files(), vec![target()]);
}

#[test]
fn an_existing_backup_is_never_overwritten() {
    let fs = MemoryFileSystem::new();
    let project = sample_project();
    let first_old = v1_bytes();
    fs.insert(&target(), first_old.clone());
    let first = save_project_keeping_previous(&fs, &target(), &project).unwrap().backup.unwrap();

    // Another old file lands at the same path (restored from an archive, say) and is saved over too.
    let second_old = rewrite_json(&first_old, "manifest.json", |v| v["app"] = serde_json::json!("other"));
    fs.insert(&target(), second_old.clone());
    let second = save_project_keeping_previous(&fs, &target(), &project).unwrap().backup.unwrap();

    assert_ne!(first, second);
    assert_eq!(disk_bytes(&fs, &first), first_old, "the first backup is untouched");
    assert_eq!(disk_bytes(&fs, &second), second_old);
}

#[test]
fn when_the_backup_cannot_be_written_the_original_stays_and_nothing_is_saved() {
    let fs = MemoryFileSystem::new();
    let original = v1_bytes();
    fs.insert(&target(), original.clone());
    let project = load_project(&fs, &target()).unwrap();
    fs.fail_on(FsOp::Write, 0);

    let err = save_project_keeping_previous(&fs, &target(), &project).unwrap_err();

    assert!(matches!(err, SaveError::Fs(_)), "{err:?}");
    assert_eq!(disk_bytes(&fs, &target()), original);
    assert_eq!(fs.files(), vec![target()], "no half-written backup or temp file is left");
}

#[test]
fn a_newer_file_on_disk_is_refused_and_left_alone() {
    let fs = MemoryFileSystem::new();
    let newer = rewrite_json(&encode_container(&sample_project()).unwrap(), "manifest.json", |v| {
        v["schema"] = serde_json::json!(SCHEMA_VERSION + 1);
    });
    fs.insert(&target(), newer.clone());

    let err = save_project_keeping_previous(&fs, &target(), &sample_project()).unwrap_err();

    assert_eq!(err, SaveError::NewerFileOnDisk { found: SCHEMA_VERSION + 1, supported: SCHEMA_VERSION });
    assert!(err.to_string().contains("newer"), "{err}");
    assert_eq!(disk_bytes(&fs, &target()), newer);
    assert_eq!(fs.files(), vec![target()]);
}

#[test]
fn an_unreadable_previous_file_is_kept_too() {
    let fs = MemoryFileSystem::new();
    fs.insert(&target(), b"half a project".to_vec());

    let outcome = save_project_keeping_previous(&fs, &target(), &sample_project()).unwrap();

    let backup = outcome.backup.unwrap();
    assert_eq!(backup, PathBuf::from("/work/plaat.burnburn.unreadable.bak"));
    assert_eq!(disk_bytes(&fs, &backup), b"half a project");
    assert_eq!(container_schema(&disk_bytes(&fs, &target())).unwrap(), SCHEMA_VERSION);
}

#[test]
fn opening_a_newer_project_names_the_problem_and_the_way_out() {
    let newer = rewrite_json(&encode_container(&sample_project()).unwrap(), "manifest.json", |v| {
        v["schema"] = serde_json::json!(SCHEMA_VERSION + 1);
    });
    let fs = MemoryFileSystem::new();
    fs.insert(&target(), newer);
    let message = load_project(&fs, &target()).unwrap_err().to_string();
    assert!(message.contains(&format!("{}", SCHEMA_VERSION + 1)), "{message}");
    assert!(message.contains("newer"), "{message}");
    assert!(message.to_lowercase().contains("update burnburn"), "{message}");
    let _ = LoadError::NotAProject;
}
