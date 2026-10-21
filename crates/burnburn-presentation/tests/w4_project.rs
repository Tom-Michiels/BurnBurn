//! W4-UI-gaten, projectfamilie (P01–P08): gaten uit de storydekking.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::io::{Cursor, Read, Write};
use std::path::{Path as FsPath, PathBuf};

use burnburn_io::fs::FileSystem;
use burnburn_io::{Project, ProjectUuid, SCHEMA_VERSION, save_project};
use burnburn_presentation::{HeadlessEditorHarness, HostEffect, MessageKey, UiIntent};
use burnburn_types::StableId;

/// A valid empty project whose manifest claims schema `SCHEMA_VERSION + 1`.
fn newer_project(h: &HeadlessEditorHarness) -> (PathBuf, Vec<u8>) {
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    let ok = PathBuf::from("/work/ok.burnburn");
    let snapshot = burnburn_document::DocumentSession::new(burnburn_types::DocumentId::from_raw(1)).snapshot();
    save_project(h.fs(), &ok, &Project::new(ProjectUuid::from_bytes([3; 16]), (*snapshot).clone())).unwrap();
    let bytes = h.fs().read(&ok, 1 << 24).unwrap();
    let mut src = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..src.len() {
        let mut f = src.by_index(i).unwrap();
        let name = f.name().unwrap().into_owned();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        if name == "manifest.json" {
            let mut v: serde_json::Value = serde_json::from_slice(&data).unwrap();
            v["schema"] = serde_json::json!(SCHEMA_VERSION + 1);
            data = serde_json::to_vec(&v).unwrap();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(&data).unwrap();
    }
    let newer = out.finish().unwrap().into_inner();
    let path = PathBuf::from("/work/newer.burnburn");
    let temp = h.fs().create_temp_in(FsPath::new("/work"), ".fixture").unwrap();
    h.fs().write_all(&temp, &newer).unwrap();
    h.fs().replace(&temp, &path).unwrap();
    (path, newer)
}

/// P01: a project from a newer BurnBurn is refused with its own message (not the generic
/// "opening failed"), no window opens, and the file is not touched.
#[test]
fn a_project_from_a_newer_version_says_so_and_stays_untouched() {
    let mut h = HeadlessEditorHarness::new();
    let (path, bytes) = newer_project(&h);
    h.send(UiIntent::Open { path: path.clone() });
    let updates = h.run_tasks();
    let effects: Vec<HostEffect> = updates.iter().flat_map(|u| u.effects.clone()).collect();
    assert!(effects.iter().all(|e| !matches!(e, HostEffect::OpenWindow { .. })), "{effects:?}");
    assert!(
        effects.iter().any(|e| matches!(e, HostEffect::ShowError { message: MessageKey::OpenNewerSchema, .. })),
        "{effects:?}"
    );
    assert_eq!(h.fs().read(&path, 1 << 24).unwrap(), bytes, "the file is left as it was");
}
