//! G5-UI-machine: bewaren met herstelbare vorige versie (X07) en geheugenmelding van de
//! undo-geschiedenis (X04).

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use std::path::{Path as FsPath, PathBuf};

use burnburn_io::fs::FileSystem;
use burnburn_io::{SCHEMA_VERSION, container_schema};
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, HostEffect, MessageKey, UiIntent};
use burnburn_types::DocumentId;

fn v1_bytes() -> Vec<u8> {
    std::fs::read(FsPath::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/projects/v1/rectangle-120x80.burnburn"))
        .unwrap()
}

fn opened_v1() -> (HeadlessEditorHarness, DocumentId, PathBuf, Vec<u8>) {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    let path = PathBuf::from("/work/plaat.burnburn");
    let original = v1_bytes();
    h.fs().insert(&path, original.clone());
    h.send(UiIntent::Open { path: path.clone() });
    h.run_tasks();
    let doc = h.view().active.expect("the opened window is active");
    (h, doc, path, original)
}

fn dirty_it(h: &mut HeadlessEditorHarness, doc: DocumentId) {
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (10.0, 10.0));
    h.invoke(doc, CommandKey::ToolSelect);
}

/// X07: saving a project that was migrated on open keeps the previous file as it was and says where.
#[test]
fn saving_a_migrated_project_keeps_the_previous_version_and_says_where() {
    let (mut h, doc, path, original) = opened_v1();
    dirty_it(&mut h, doc);
    h.send(UiIntent::Save { document: doc, path: None });
    h.run_tasks();

    let backup = PathBuf::from("/work/plaat.burnburn.schema1.bak");
    assert_eq!(h.fs().read(&backup, 1 << 26).unwrap(), original, "the previous file is kept byte for byte");
    assert_eq!(container_schema(&h.fs().read(&path, 1 << 26).unwrap()).unwrap(), SCHEMA_VERSION);
    let view = h.document(doc).unwrap();
    assert!(!view.dirty);
    assert!(view.messages.iter().any(|m| m.key == MessageKey::PreviousVersionKept), "{:?}", view.messages);
    assert_eq!(h.controller().previous_version(doc), Some(backup));
}

/// A save over a file of the current schema keeps no further copy.
#[test]
fn saving_over_a_current_file_keeps_no_second_copy() {
    let (mut h, doc, _path, _) = opened_v1();
    dirty_it(&mut h, doc);
    h.send(UiIntent::Save { document: doc, path: None });
    h.run_tasks();
    h.invoke(doc, CommandKey::Undo);
    h.send(UiIntent::Save { document: doc, path: None });
    h.run_tasks();
    let files = h.fs().files();
    assert_eq!(files.iter().filter(|p| p.to_string_lossy().ends_with(".bak")).count(), 1, "{files:?}");
}

/// X07: a file written by a newer BurnBurn is not overwritten; the error names it.
#[test]
fn a_newer_file_on_disk_is_never_overwritten_and_is_said_in_words() {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(FsPath::new("/work"));
    let doc = h.new_document().unwrap();
    dirty_it(&mut h, doc);
    let path = PathBuf::from("/work/nieuwer.burnburn");
    h.send(UiIntent::Save { document: doc, path: Some(path.clone()) });
    h.run_tasks();
    // A newer-schema container: the real project, with its manifest claiming schema + 1.
    let newer = rewrite_schema(&h.fs().read(&path, 1 << 26).unwrap(), SCHEMA_VERSION + 1);
    h.fs().insert(&path, newer.clone());
    h.invoke(doc, CommandKey::Undo);
    h.send(UiIntent::Save { document: doc, path: Some(path.clone()) });
    let updates = h.run_tasks();
    let effects: Vec<HostEffect> = updates.iter().flat_map(|u| u.effects.clone()).collect();
    assert!(
        effects.iter().any(|e| matches!(e, HostEffect::ShowError { message: MessageKey::SaveNewerFileOnDisk, .. })),
        "{effects:?}"
    );
    assert_eq!(h.fs().read(&path, 1 << 26).unwrap(), newer, "the newer file is untouched");
    assert!(h.document(doc).unwrap().dirty, "nothing was saved");
}

fn rewrite_schema(bytes: &[u8], schema: u32) -> Vec<u8> {
    use std::io::{Cursor, Read, Write};
    let mut src = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..src.len() {
        let mut f = src.by_index(i).unwrap();
        let name = f.name().unwrap().into_owned();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        if name == "manifest.json" {
            let mut v: serde_json::Value = serde_json::from_slice(&data).unwrap();
            v["schema"] = serde_json::json!(schema);
            data = serde_json::to_vec(&v).unwrap();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(&data).unwrap();
    }
    out.finish().unwrap().into_inner()
}
