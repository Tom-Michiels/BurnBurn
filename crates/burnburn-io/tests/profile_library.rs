//! W4-UI-gaten (P07): the profiles a bundle brings in are kept outside the session in a profile
//! library file. Written atomically through the filesystem port, validated on load (the content
//! hash is recomputed), newer schemas reported, damaged files typed errors.

#![allow(clippy::unwrap_used)]

use std::path::Path;

use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::profile_library::{
    PROFILE_LIBRARY_SCHEMA, decode_profile_library, load_profile_library, save_profile_library,
};
use burnburn_io::{LoadError, fs::FileSystem};
use burnburn_job::{Feature, MachineCapabilities, MachineFrame, MachineProfile, OriginCorner, PowerRatio, Support};

fn profile(id: &str, speed: f64) -> MachineProfile {
    let frame = MachineFrame::new(1500.0, 900.0, OriginCorner::TopRight).unwrap();
    let caps = MachineCapabilities::conservative()
        .with(Feature::ZStep, Support::Unverified { reason: "nog niet gemeten".into() });
    MachineProfile::new(id, 3, frame, speed, 300.0, PowerRatio::new(0.1).unwrap(), PowerRatio::new(0.8).unwrap())
        .unwrap()
        .with_capabilities(caps)
        .unwrap()
}

#[test]
fn profiles_survive_a_save_and_load_with_identical_content() {
    let fs = MemoryFileSystem::new();
    let path = Path::new("/lib/profiles.json");
    let (a, b) = (profile("omtech-60w", 400.0), profile("werkplaats", 250.0));
    save_profile_library(&fs, path, &[a.clone(), b.clone()]).unwrap();
    let loaded = load_profile_library(&fs, path).unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].revision(), a.revision());
    assert_eq!(loaded[1].revision(), b.revision());
    assert_eq!(loaded[0], a);
}

#[test]
fn a_missing_file_is_an_fs_error_not_an_empty_library() {
    let fs = MemoryFileSystem::new();
    assert!(matches!(load_profile_library(&fs, Path::new("/nope.json")), Err(LoadError::Fs(_))));
}

#[test]
fn a_newer_schema_is_reported_and_a_foreign_or_damaged_file_is_refused() {
    let fs = MemoryFileSystem::new();
    let path = Path::new("/lib/profiles.json");
    save_profile_library(&fs, path, &[profile("a", 400.0)]).unwrap();
    let bytes = fs.read(path, 1 << 20).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    let newer = text.replace("\"schema\":1", &format!("\"schema\":{}", PROFILE_LIBRARY_SCHEMA + 1));
    assert_ne!(newer, text, "the fixture has a schema field to bump (compact JSON)");
    assert_eq!(
        decode_profile_library(newer.as_bytes()).unwrap_err(),
        LoadError::NewerSchema { found: PROFILE_LIBRARY_SCHEMA + 1, supported: PROFILE_LIBRARY_SCHEMA }
    );
    assert!(decode_profile_library(b"{\"format\":\"something-else\",\"schema\":1}").is_err());
    assert!(decode_profile_library(b"not json").is_err());
    // A stored value changed behind the content hash: refused, never silently adopted.
    let tampered = text.replace("400", "401");
    assert_ne!(tampered, text);
    assert!(decode_profile_library(tampered.as_bytes()).is_err());
}

#[test]
fn saving_replaces_the_previous_file_atomically_and_keeps_it_on_failure_of_validation() {
    let fs = MemoryFileSystem::new();
    let path = Path::new("/lib/profiles.json");
    save_profile_library(&fs, path, &[profile("a", 400.0)]).unwrap();
    save_profile_library(&fs, path, &[profile("a", 400.0), profile("b", 200.0)]).unwrap();
    assert_eq!(load_profile_library(&fs, path).unwrap().len(), 2);
    // Two profiles with the same id are one machine twice: refused, the old file stays.
    assert!(save_profile_library(&fs, path, &[profile("a", 400.0), profile("a", 300.0)]).is_err());
    assert_eq!(load_profile_library(&fs, path).unwrap().len(), 2);
}
