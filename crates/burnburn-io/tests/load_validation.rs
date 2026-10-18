//! Public contract: loading rejects damaged, foreign or newer files explicitly, with a reason.

#![allow(clippy::unwrap_used)] // test helpers outside #[test] fns

mod common;

use std::path::Path;

use burnburn_io::fs::FsOp;
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{LoadError, SCHEMA_VERSION, decode_container, encode_container, load_project, save_project};
use burnburn_types::{GeometryId, StableId};
use common::{add_entry, rewrite_entry, rewrite_json, sample_project};

fn sample_bytes() -> Vec<u8> {
    encode_container(&sample_project()).unwrap()
}

#[test]
fn non_finite_value_in_file_is_rejected_with_path() {
    // serde_json writes NaN as `null`; a damaged file may contain it.
    let bytes = rewrite_json(&sample_bytes(), "document.json", |v| {
        v["objects"][1]["subpaths"][0]["segments"][0]["c2"][1] = serde_json::Value::Null;
    });
    assert_eq!(
        decode_container(&bytes).unwrap_err(),
        LoadError::InvalidValue {
            path: "objects[1].subpaths[0].segments[0].c2[1]".into(),
            reason: "not a finite number".into()
        }
    );
    // A literal overflowing f64 is rejected by the parser (it never becomes Infinity).
    let overflow = rewrite_entry(&sample_bytes(), "document.json", |b| {
        String::from_utf8(b).unwrap().replacen("\"next_id\": 5", "\"next_id\": 5, \"x\": 1e999", 1).into_bytes()
    });
    assert!(matches!(decode_container(&overflow), Err(LoadError::Json { .. } | LoadError::UnknownField { .. })));
}

#[test]
fn unknown_field_in_current_schema_is_reported_not_dropped() {
    let bytes = rewrite_json(&sample_bytes(), "document.json", |v| {
        v["objects"][0]["layer"] = serde_json::json!("cut");
    });
    match decode_container(&bytes).unwrap_err() {
        LoadError::UnknownField { entry, detail } => {
            assert_eq!(entry, "document.json");
            assert!(detail.contains("layer"), "{detail}");
        }
        other => panic!("expected UnknownField, got {other:?}"),
    }
}

#[test]
fn newer_schema_is_reported_and_file_left_untouched() {
    let newer = rewrite_json(&sample_bytes(), "manifest.json", |v| {
        v["schema"] = serde_json::json!(SCHEMA_VERSION + 1);
        v["future_feature"] = serde_json::json!({"anything": true});
    });
    let fs = MemoryFileSystem::new();
    let path = Path::new("/p/newer.burnburn");
    fs.insert(path, newer.clone());
    assert_eq!(
        load_project(&fs, path).unwrap_err(),
        LoadError::NewerSchema { found: SCHEMA_VERSION + 1, supported: SCHEMA_VERSION }
    );
    assert_eq!(fs.files(), vec![path.to_path_buf()]);
    assert_eq!(fs.log().iter().filter(|(op, _)| *op != FsOp::Read).count(), 0, "opening never writes");
}

#[test]
fn corrupted_asset_is_rejected_by_hash() {
    let bytes = sample_bytes();
    let project = sample_project();
    let (hash, _) = project.assets.iter().next().unwrap();
    let entry = format!("assets/sha256-{}", hash.to_hex());
    let tampered = rewrite_entry(&bytes, &entry, |mut b| {
        b[0] ^= 1;
        b
    });
    assert_eq!(decode_container(&tampered).unwrap_err(), LoadError::AssetHashMismatch(*hash));
}

#[test]
fn unexpected_or_traversing_entries_are_rejected() {
    for name in ["../escape.txt", "/abs.txt", "assets/sub/x", "notes.txt"] {
        let bytes = add_entry(&sample_bytes(), name, b"x");
        assert_eq!(decode_container(&bytes).unwrap_err(), LoadError::UnexpectedEntry(name.into()), "{name}");
    }
    assert!(decode_container(b"not a zip").is_err());
    let foreign = rewrite_json(&sample_bytes(), "manifest.json", |v| v["format"] = serde_json::json!("other"));
    assert_eq!(decode_container(&foreign).unwrap_err(), LoadError::NotAProject);
}

#[test]
fn inconsistent_project_is_not_written() {
    let fs = MemoryFileSystem::new();
    fs.create_dir_all_for_test(Path::new("/p"));
    let path = Path::new("/p/a.burnburn");
    save_project(&fs, path, &sample_project()).unwrap();
    // Since IM13 operations live in the snapshot, whose constructor already rejects a dangling
    // reference or a clashing id: such a project cannot even be built, let alone written.
    let objects = || sample_project().document.objects().map(|(_, o)| o.clone()).collect::<Vec<_>>();
    let layer = |id, target| {
        let op = common::sample_operation(id, "x", vec![GeometryId::from_raw(target)]);
        burnburn_document::process::ProcessLayer::from_parts(vec![op], vec![], vec![]).unwrap()
    };
    let doc = || burnburn_document::DocumentSnapshot::from_parts(objects(), 5).unwrap();
    assert!(doc().with_process(layer(3, 4)).is_err(), "no such geometry");
    assert!(doc().with_process(layer(1, 1)).is_err(), "same raw id as geometry 1");
    assert_eq!(load_project(&fs, path).unwrap().document.process(), sample_project().document.process());
    assert_eq!(fs.files(), vec![path.to_path_buf()]);
}
