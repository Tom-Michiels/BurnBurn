#![allow(clippy::unwrap_used)] // filesystem helpers outside #[test] fns

use std::fs;
use std::path::{Path, PathBuf};

use burnburn_test_support::fixtures::{
    FixtureError, fixture_dir, read_fixture, read_ruida_fixture, repo_root, ruida_dir, sha256_hex, test_data_dir,
    unlisted_files, verify_manifest,
};

#[test]
fn sha256_matches_published_test_vector() {
    // FIPS 180-2 example "abc".
    assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
}

#[test]
fn ruida_fixture_hashes_match_manifest() {
    let verified = verify_manifest(&ruida_dir()).unwrap_or_else(|e| panic!("{e}"));
    let rd: Vec<_> = verified.iter().filter(|f| f.role == "rd").map(|f| f.path.as_str()).collect();
    assert_eq!(rd, ["rectangle-default.rd", "rectangle-fill.rd", "gray-ramp-grayscale.rd"]);
    assert_eq!(verified.iter().filter(|f| f.role == "source").count(), 4);
    assert_eq!(verified.iter().filter(|f| f.role == "oracle").count(), 3);
}

/// The copies must be byte-identical to the research evidence they claim to come from.
#[test]
fn ruida_fixture_manifest_agrees_with_research_evidence() {
    let research: serde_json::Value = serde_json::from_slice(
        &fs::read(repo_root().join("research/lightburn/evidence/fixtures-manifest.json")).unwrap(),
    )
    .unwrap();
    let ours: serde_json::Value =
        serde_json::from_slice(&fs::read(ruida_dir().join("manifest.json")).unwrap()).unwrap();
    let mut compared = 0;
    for file in ours["files"].as_array().unwrap() {
        let copied_from = file["copied_from"].as_str().unwrap();
        let Some(key) = copied_from.strip_prefix("research/lightburn/") else { panic!("{copied_from}") };
        if let Some(entry) = research.as_array().unwrap().iter().find(|e| e["path"] == key) {
            assert_eq!(entry["sha256"], file["sha256"], "{key}");
            assert_eq!(entry["bytes"], file["bytes"], "{key}");
            compared += 1;
        }
        let original = fs::read(repo_root().join(copied_from)).unwrap();
        assert_eq!(sha256_hex(&original), file["sha256"].as_str().unwrap(), "{copied_from}");
    }
    assert_eq!(compared, 7, "every research fixture is cross-checked");
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    copy_tree(&ruida_dir(), &dir);
    dir
}

#[test]
fn tampered_fixture_byte_is_reported() {
    let dir = scratch("tampered-fixture");
    let path = dir.join("rectangle-default.rd");
    let mut bytes = fs::read(&path).unwrap();
    bytes[100] ^= 0x01;
    fs::write(&path, bytes).unwrap();
    let err = verify_manifest(&dir).unwrap_err();
    assert!(matches!(&err, FixtureError::HashMismatch { path, .. } if path.ends_with("rectangle-default.rd")), "{err}");
}

#[test]
fn truncated_fixture_is_reported_as_size_mismatch() {
    let dir = scratch("truncated-fixture");
    let path = dir.join("rectangle-fill.rd");
    let bytes = fs::read(&path).unwrap();
    fs::write(&path, &bytes[..bytes.len() - 1]).unwrap();
    assert!(matches!(verify_manifest(&dir), Err(FixtureError::SizeMismatch { expected: 2572, actual: 2571, .. })));
}

#[test]
fn missing_fixture_is_an_io_error() {
    let dir = scratch("missing-fixture");
    fs::remove_file(dir.join("gray-ramp-grayscale.rd")).unwrap();
    assert!(matches!(verify_manifest(&dir), Err(FixtureError::Io { .. })));
}

#[test]
fn read_ruida_fixture_returns_checked_bytes() {
    let bytes = read_ruida_fixture("rectangle-default.rd").unwrap();
    assert_eq!(bytes.len(), 525);
    // 04-exportanalyse.md: the last *unscrambled* byte is D7 (EOF). The file is scrambled with
    // magic 0x88; scrambling D7 gives 0x60 (swap bits 0/7 of D7 = D7, ^0x88 = 0x5F, +1 = 0x60).
    assert_eq!(bytes.last(), Some(&0x60));
    assert!(matches!(read_ruida_fixture("unknown.rd"), Err(FixtureError::NotInManifest(_))));
}

/// Kern's project manifest uses `fixtures[].file` instead of `files[].path`; both are checked.
#[test]
fn project_fixture_hashes_match_manifest() {
    let verified = verify_manifest(&fixture_dir("projects")).unwrap_or_else(|e| panic!("{e}"));
    let paths: Vec<_> = verified.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["v1/rectangle-120x80.burnburn", "v2/plate-operations.burnburn", "v2/recipes-in-rows.burnburn"]);
    assert_eq!(verified[0].sha256, "9f0b355a2bedad4d06abec316613641a5df546376dc492e0370ed6a8ed9b651d");
    assert_eq!(verified[1].sha256, "2ec762d383f5da1652ddfa529367059fd65f7ef3962aaaa02d057f3fa063088d");
    // G7-cam: written by hand, recorded once.
    assert_eq!(verified[2].sha256, "25a65667bdfd0a788bcbc690ee32d4746c7828ea4bd5f0370a5731c5c792efe1");
}

#[test]
fn read_fixture_checks_any_set() {
    let bytes = read_fixture("projects", "v1/rectangle-120x80.burnburn").unwrap();
    assert_eq!(bytes.len(), 573);
    assert_eq!(&bytes[..4], b"PK\x03\x04", "a zip container starts with a local file header");
    assert_eq!(read_fixture("ruida", "rectangle-fill.rd").unwrap().len(), 2572);
    assert!(matches!(read_fixture("projects", "v1/missing.burnburn"), Err(FixtureError::NotInManifest(_))));
}

#[test]
fn unlisted_fixture_is_reported() {
    let dir = scratch("unlisted-fixture");
    assert_eq!(unlisted_files(&dir).unwrap(), Vec::<String>::new());
    fs::write(dir.join("oracle").join("stray.json"), b"{}").unwrap();
    assert_eq!(unlisted_files(&dir).unwrap(), ["oracle/stray.json"]);
}

/// Every directory under `test-data/` has a manifest, every listed file matches its size and
/// SHA-256, and no file sits there unlisted.
#[test]
fn fixtures_match_manifests() {
    let mut sets = Vec::new();
    for entry in fs::read_dir(test_data_dir()).unwrap() {
        let entry = entry.unwrap();
        if !entry.file_type().unwrap().is_dir() {
            continue;
        }
        let dir = entry.path();
        let name = entry.file_name().into_string().unwrap();
        assert!(dir.join("manifest.json").is_file(), "test-data/{name} has no manifest.json");
        let verified = verify_manifest(&dir).unwrap_or_else(|e| panic!("test-data/{name}: {e}"));
        assert!(!verified.is_empty(), "test-data/{name}: manifest lists no files");
        assert_eq!(
            unlisted_files(&dir).unwrap(),
            Vec::<String>::new(),
            "test-data/{name}: files missing from manifest"
        );
        sets.push(name);
    }
    sets.sort();
    for required in ["geometry", "import", "projects", "ruida", "trace"] {
        assert!(sets.iter().any(|s| s == required), "test-data/{required} missing: {sets:?}");
    }
}
