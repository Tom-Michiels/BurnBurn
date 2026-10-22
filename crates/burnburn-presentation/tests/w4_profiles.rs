//! W4-UI-gaten (P07): profiles brought in by a bundle are kept outside the session.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_test_support::host::rig as common;

use std::path::{Path as FsPath, PathBuf};

use burnburn_io::bundle::{BundleContents, encode_bundle};
use burnburn_io::fs::FileSystem;
use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
use burnburn_presentation::{FilesIntent, HeadlessEditorHarness, UiIntent};
use common::{RUIDA, profile};

fn new_machine() -> MachineProfile {
    MachineProfile::new(
        "nieuwe-machine",
        1,
        MachineFrame::new(600.0, 400.0, OriginCorner::BottomLeft).unwrap(),
        500.0,
        300.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
}

fn start(fs: std::sync::Arc<burnburn_io::memory::MemoryFileSystem>) -> HeadlessEditorHarness {
    let mut h = HeadlessEditorHarness::with_fs(fs);
    for dir in ["/lib", "/in"] {
        h.fs().create_dir_all_for_test(FsPath::new(dir));
    }
    h.controller().set_library_dir(Some(PathBuf::from("/lib")));
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    h
}

fn ids(h: &mut HeadlessEditorHarness) -> Vec<String> {
    h.controller().known_profiles().iter().map(|p| p.revision().id.clone()).collect()
}

/// A profile a bundle adds is written to the profile library and found again by a new session.
#[test]
fn profiles_from_a_bundle_are_kept_outside_the_session() {
    let mut h = start(std::sync::Arc::new(burnburn_io::memory::MemoryFileSystem::new()));
    let doc = h.new_document().unwrap();
    let profiles = [new_machine()];
    let bytes =
        encode_bundle(&BundleContents { materials: None, art: None, profiles: &profiles, templates: &[] }).unwrap();
    let path = PathBuf::from("/in/b.burnburnbundle");
    h.fs().insert(&path, bytes);
    h.send(UiIntent::Files(FilesIntent::OpenBundleImport { document: doc, path }));
    h.run_tasks();
    h.send(UiIntent::Files(FilesIntent::ApplyBundleImport { document: doc }));
    h.run_tasks();
    assert_eq!(ids(&mut h), ["research-1500x900", "nieuwe-machine"]);
    assert!(h.fs().exists(FsPath::new("/lib/profiles.json")).unwrap(), "the profile library file was written");

    // Another session on the same folder finds it again once the libraries are read.
    let mut again = start(h.shared_fs());
    let doc = again.new_document().unwrap();
    again.send(UiIntent::Files(FilesIntent::OpenMaterialLibrary { document: doc }));
    again.run_tasks();
    assert_eq!(ids(&mut again), ["research-1500x900", "nieuwe-machine"]);
}

/// A damaged profile library is reported and left as it is: the next import does not overwrite it.
#[test]
fn a_damaged_profile_library_is_kept_and_never_overwritten() {
    let fs = std::sync::Arc::new(burnburn_io::memory::MemoryFileSystem::new());
    let mut h = start(fs.clone());
    h.fs().insert(FsPath::new("/lib/profiles.json"), b"{ not json".to_vec());
    let doc = h.new_document().unwrap();
    h.send(UiIntent::Files(FilesIntent::OpenMaterialLibrary { document: doc }));
    h.run_tasks();
    assert_eq!(ids(&mut h), ["research-1500x900"]);
    let profiles = [new_machine()];
    let bytes =
        encode_bundle(&BundleContents { materials: None, art: None, profiles: &profiles, templates: &[] }).unwrap();
    let path = PathBuf::from("/in/b.burnburnbundle");
    h.fs().insert(&path, bytes);
    h.send(UiIntent::Files(FilesIntent::OpenBundleImport { document: doc, path }));
    h.run_tasks();
    h.send(UiIntent::Files(FilesIntent::ApplyBundleImport { document: doc }));
    h.run_tasks();
    assert_eq!(ids(&mut h), ["research-1500x900", "nieuwe-machine"], "still usable in this session");
    assert_eq!(h.fs().read(FsPath::new("/lib/profiles.json"), 1 << 20).unwrap(), b"{ not json".to_vec());
}
