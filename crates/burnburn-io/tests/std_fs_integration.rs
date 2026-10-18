//! Real filesystem integration in a temporary directory (never in user projects).
//! Verified on macOS arm64; Windows replace/sync semantics need their own runner.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

mod common;

use std::path::Path;

use burnburn_io::std_fs::StdFileSystem;
use burnburn_io::{SaveError, load_project, read_recovery, save_project, write_recovery};
use common::sample_project;

fn entries(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> =
        std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    v.sort();
    v
}

#[test]
fn std_save_reopen_and_failed_replace_in_temp_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let path = dir.join("a.burnburn");
    let original = sample_project();

    save_project(&StdFileSystem, &path, &original).unwrap();
    assert_eq!(entries(dir), ["a.burnburn"], "no temp file left after a successful save");
    // A fresh adapter value: nothing but the bytes on disk is shared.
    let reopened = load_project(&StdFileSystem, &path).unwrap();
    assert_eq!(reopened.document.process(), original.document.process());
    assert_eq!(
        reopened.document.objects().map(|(_, o)| o.clone()).collect::<Vec<_>>(),
        original.document.objects().map(|(_, o)| o.clone()).collect::<Vec<_>>()
    );

    // Overwrite an existing file through the same route.
    save_project(&StdFileSystem, &path, &reopened).unwrap();
    assert_eq!(entries(dir), ["a.burnburn"]);

    // Replace fails: the target is a non-empty directory. The temp file is cleaned up.
    let blocked = dir.join("blocked.burnburn");
    std::fs::create_dir(&blocked).unwrap();
    std::fs::write(blocked.join("keep.txt"), b"x").unwrap();
    let err = save_project(&StdFileSystem, &blocked, &original).unwrap_err();
    assert!(matches!(err, SaveError::Fs(_)), "{err:?}");
    assert_eq!(entries(dir), ["a.burnburn", "blocked.burnburn"]);
    assert_eq!(entries(&blocked), ["keep.txt"]);
    assert!(load_project(&StdFileSystem, &path).is_ok(), "the earlier project is still readable");

    // Recovery lives in its own directory and reads back after "restart".
    let recovery = dir.join("Recovery");
    std::fs::create_dir(&recovery).unwrap();
    write_recovery(&StdFileSystem, &recovery, &original).unwrap();
    let back = read_recovery(&StdFileSystem, &recovery, original.uuid).unwrap().unwrap();
    assert_eq!(back.document.process(), original.document.process());
    assert_eq!(entries(dir), ["Recovery", "a.burnburn", "blocked.burnburn"]);
}
