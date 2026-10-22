//! G5-machine-rest (P07 rest): a replaced active machine profile is kept in the profile library,
//! and the machine the user works with is chosen from the known profiles.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_test_support::host::rig as common;

use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use burnburn_io::fs::FileSystem;
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::profile_library::{decode_profile_library, save_profile_library};
use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
use burnburn_presentation::{HeadlessEditorHarness, MessageKey, UiIntent};
use common::{RUIDA, profile};

fn machine(id: &str, width_mm: f64) -> MachineProfile {
    MachineProfile::new(
        id,
        1,
        MachineFrame::new(width_mm, 400.0, OriginCorner::BottomLeft).unwrap(),
        500.0,
        300.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
}

fn start(fs: Arc<MemoryFileSystem>) -> HeadlessEditorHarness {
    let mut h = HeadlessEditorHarness::with_fs(fs);
    h.fs().create_dir_all_for_test(FsPath::new("/lib"));
    h.controller().set_library_dir(Some(PathBuf::from("/lib")));
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    h
}

fn activate(h: &mut HeadlessEditorHarness, p: MachineProfile) {
    h.send(UiIntent::SetMachineProfile { profile: Box::new(p), ruida: RUIDA });
    h.run_tasks();
}

fn library(h: &HeadlessEditorHarness) -> Vec<(String, u32)> {
    let bytes = h.fs().read(FsPath::new("/lib/profiles.json"), 1 << 24).unwrap();
    decode_profile_library(&bytes).unwrap().iter().map(|p| (p.revision().id.clone(), p.revision().revision)).collect()
}

fn known(h: &mut HeadlessEditorHarness) -> Vec<String> {
    h.controller().known_profiles().iter().map(|p| p.revision().id.clone()).collect()
}

#[test]
fn a_replaced_active_profile_is_kept_in_the_profile_library() {
    let mut h = start(Arc::new(MemoryFileSystem::new()));
    activate(&mut h, machine("tweede-machine", 600.0));
    assert_eq!(library(&h), [("research-1500x900".to_owned(), 1)], "the machine that was replaced is kept");
    assert_eq!(known(&mut h), ["tweede-machine", "research-1500x900"], "active first, then the kept one");
}

#[test]
fn what_the_library_file_already_holds_survives_a_replacement_before_it_was_read() {
    let fs = Arc::new(MemoryFileSystem::new());
    fs.create_dir_all_for_test(FsPath::new("/lib"));
    save_profile_library(fs.as_ref(), FsPath::new("/lib/profiles.json"), &[machine("oude-bekende", 800.0)]).unwrap();
    let mut h = start(fs);
    activate(&mut h, machine("tweede-machine", 600.0));
    let mut ids: Vec<String> = library(&h).into_iter().map(|(id, _)| id).collect();
    ids.sort();
    assert_eq!(ids, ["oude-bekende", "research-1500x900"], "the file is merged, never replaced by this session's list");
}

#[test]
fn replacing_a_machine_by_a_newer_revision_of_itself_keeps_the_older_revision() {
    let mut h = start(Arc::new(MemoryFileSystem::new()));
    activate(&mut h, profile(2));
    assert_eq!(library(&h), [("research-1500x900".to_owned(), 1)]);
    assert_eq!(known(&mut h), ["research-1500x900"], "one machine is listed once, with the active revision");
}

#[test]
fn the_same_profile_again_replaces_nothing_and_writes_nothing() {
    let mut h = start(Arc::new(MemoryFileSystem::new()));
    activate(&mut h, profile(1));
    assert!(!h.fs().exists(FsPath::new("/lib/profiles.json")).unwrap());
}

#[test]
fn a_damaged_library_file_is_left_alone_when_a_profile_is_replaced() {
    let mut h = start(Arc::new(MemoryFileSystem::new()));
    h.fs().insert(FsPath::new("/lib/profiles.json"), b"{ not json".to_vec());
    activate(&mut h, machine("tweede-machine", 600.0));
    assert_eq!(h.fs().read(FsPath::new("/lib/profiles.json"), 1 << 20).unwrap(), b"{ not json".to_vec());
    assert_eq!(known(&mut h), ["tweede-machine", "research-1500x900"], "still usable in this session");
}

#[test]
fn a_known_profile_is_chosen_by_id_and_the_previous_one_is_kept() {
    let mut h = start(Arc::new(MemoryFileSystem::new()));
    activate(&mut h, machine("tweede-machine", 600.0));
    h.send(UiIntent::ChooseMachineProfile { id: "research-1500x900".into() });
    h.run_tasks();
    assert_eq!(known(&mut h), ["research-1500x900", "tweede-machine"]);
    let mut ids: Vec<String> = library(&h).into_iter().map(|(id, _)| id).collect();
    ids.sort();
    assert_eq!(ids, ["research-1500x900", "tweede-machine"]);
}

#[test]
fn choosing_an_unknown_profile_changes_nothing_and_says_so() {
    let mut h = start(Arc::new(MemoryFileSystem::new()));
    let update = h.send(UiIntent::ChooseMachineProfile { id: "bestaat-niet".into() });
    h.run_tasks();
    assert_eq!(known(&mut h), ["research-1500x900"]);
    assert!(
        update.effects.iter().any(|e| matches!(
            e,
            burnburn_presentation::HostEffect::ShowError { message: MessageKey::UnknownMachineProfile, .. }
        )),
        "{:?}",
        update.effects
    );
    assert!(!h.fs().exists(FsPath::new("/lib/profiles.json")).unwrap());
}

/// Replacing the active machine through a bundle import keeps the revision that was replaced.
#[test]
fn a_bundle_that_replaces_the_active_profile_leaves_the_replaced_revision_in_the_library() {
    use burnburn_io::bundle::{BundleContents, encode_bundle};
    use burnburn_presentation::FilesIntent;
    let mut h = start(Arc::new(MemoryFileSystem::new()));
    h.fs().create_dir_all_for_test(FsPath::new("/in"));
    let doc = h.new_document().unwrap();
    let faster = MachineProfile::new(
        "research-1500x900",
        2,
        MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap(),
        600.0,
        400.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap();
    let bytes =
        encode_bundle(&BundleContents { materials: None, art: None, profiles: &[faster], templates: &[] }).unwrap();
    let path = PathBuf::from("/in/b.burnburnbundle");
    h.fs().insert(&path, bytes);
    h.send(UiIntent::Files(FilesIntent::OpenBundleImport { document: doc, path }));
    h.run_tasks();
    h.send(UiIntent::Files(FilesIntent::SetBundleProfileReplace {
        document: doc,
        profile: "research-1500x900".into(),
        replace: true,
    }));
    h.send(UiIntent::Files(FilesIntent::ApplyBundleImport { document: doc }));
    h.run_tasks();
    assert_eq!(h.controller().known_profiles()[0].revision().revision, 2, "the bundle's revision is active");
    assert_eq!(library(&h), [("research-1500x900".to_owned(), 1)], "the replaced revision is kept");
}
