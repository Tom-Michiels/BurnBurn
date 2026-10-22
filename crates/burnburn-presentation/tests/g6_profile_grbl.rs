//! G6-machine: the profile library keeps the GRBL dialect settings of a machine, and choosing a
//! GRBL profile from the library drives it as GRBL again (it used to fall back to Ruida).

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod common;

use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use burnburn_gcode::{GrblProfile, PowerMode, WorkCoordinate, grbl_capabilities};
use burnburn_io::fs::FileSystem;
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::profile_library::{LibraryProfile, decode_profile_library_entries, save_profile_library_entries};
use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
use burnburn_presentation::{HeadlessEditorHarness, UiIntent};
use common::{RUIDA, profile};

fn grbl_settings() -> GrblProfile {
    GrblProfile {
        max_spindle: 255,
        laser_mode: true,
        power_mode: PowerMode::Constant,
        air_assist: true,
        work_coordinate: Some(WorkCoordinate::G56),
        decimals: 2,
    }
}

fn grbl_machine(id: &str) -> MachineProfile {
    MachineProfile::new(
        id,
        1,
        MachineFrame::new(410.0, 400.0, OriginCorner::BottomLeft).unwrap(),
        400.0,
        300.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
    .with_capabilities(grbl_capabilities(&grbl_settings()))
    .unwrap()
}

fn start(fs: Arc<MemoryFileSystem>) -> HeadlessEditorHarness {
    let mut h = HeadlessEditorHarness::with_fs(fs);
    h.fs().create_dir_all_for_test(FsPath::new("/lib"));
    h.controller().set_library_dir(Some(PathBuf::from("/lib")));
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    h
}

fn use_grbl(h: &mut HeadlessEditorHarness, id: &str, grbl: GrblProfile) {
    h.send(UiIntent::SetGrblMachineProfile { profile: Box::new(grbl_machine(id)), grbl: Box::new(grbl) });
    h.run_tasks();
}

fn stored(h: &HeadlessEditorHarness) -> Vec<LibraryProfile> {
    let bytes = h.fs().read(FsPath::new("/lib/profiles.json"), 1 << 24).unwrap();
    decode_profile_library_entries(&bytes).unwrap()
}

#[test]
fn a_replaced_grbl_machine_is_kept_with_its_dialect_settings() {
    let mut h = start(Arc::new(MemoryFileSystem::new()));
    use_grbl(&mut h, "graveur", grbl_settings());
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    h.run_tasks();
    let kept = stored(&h);
    let graveur = kept.iter().find(|e| e.profile.revision().id == "graveur").expect("the GRBL machine is kept");
    assert_eq!(graveur.grbl, Some(grbl_settings()));
    let other = kept.iter().find(|e| e.profile.revision().id != "graveur");
    assert!(other.is_none_or(|e| e.grbl.is_none()), "a Ruida machine is stored without dialect settings");
}

#[test]
fn choosing_a_grbl_profile_from_the_library_drives_it_as_grbl_with_its_settings() {
    let mut h = start(Arc::new(MemoryFileSystem::new()));
    use_grbl(&mut h, "graveur", grbl_settings());
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    h.run_tasks();
    assert_eq!(h.controller().grbl_profile(), None);
    h.send(UiIntent::ChooseMachineProfile { id: "graveur".into() });
    h.run_tasks();
    assert_eq!(h.controller().grbl_profile(), Some(&grbl_settings()), "dialect restored, not a Ruida fallback");
    assert_eq!(h.controller().known_profiles()[0].revision().id, "graveur");
}

#[test]
fn the_dialect_is_still_there_in_a_new_session_on_the_same_library_file() {
    let fs = Arc::new(MemoryFileSystem::new());
    let mut first = start(Arc::clone(&fs));
    use_grbl(&mut first, "graveur", grbl_settings());
    first.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    first.run_tasks();

    let mut second = start(Arc::clone(&fs));
    second.run_tasks();
    second.send(UiIntent::ChooseMachineProfile { id: "graveur".into() });
    second.run_tasks();
    assert_eq!(second.controller().grbl_profile(), Some(&grbl_settings()));
}

#[test]
fn choosing_a_ruida_profile_after_a_grbl_one_leaves_no_dialect_behind() {
    let mut h = start(Arc::new(MemoryFileSystem::new()));
    use_grbl(&mut h, "graveur", grbl_settings());
    h.send(UiIntent::ChooseMachineProfile { id: "research-1500x900".into() });
    h.run_tasks();
    assert_eq!(h.controller().grbl_profile(), None);
    let kept = stored(&h);
    assert_eq!(
        kept.iter().find(|e| e.profile.revision().id == "graveur").and_then(|e| e.grbl.clone()),
        Some(grbl_settings())
    );
}

#[test]
fn a_library_written_before_the_dialects_existed_chooses_as_ruida() {
    let fs = Arc::new(MemoryFileSystem::new());
    fs.create_dir_all_for_test(FsPath::new("/lib"));
    save_profile_library_entries(
        fs.as_ref(),
        FsPath::new("/lib/profiles.json"),
        &[LibraryProfile { profile: profile(1), grbl: None, model: None, firmware: None }],
    )
    .unwrap();
    let mut h = HeadlessEditorHarness::with_fs(fs);
    h.controller().set_library_dir(Some(PathBuf::from("/lib")));
    h.send(UiIntent::SetGrblMachineProfile {
        profile: Box::new(grbl_machine("graveur")),
        grbl: Box::new(grbl_settings()),
    });
    h.run_tasks();
    h.send(UiIntent::ChooseMachineProfile { id: "research-1500x900".into() });
    h.run_tasks();
    assert_eq!(h.controller().grbl_profile(), None);
}

#[test]
fn changing_the_settings_of_the_active_grbl_machine_keeps_the_previous_settings_with_the_old_revision() {
    // A new revision of the same machine replaces the old one in the library: the library then
    // holds the settings that went with the revision that was replaced.
    let mut h = start(Arc::new(MemoryFileSystem::new()));
    use_grbl(&mut h, "graveur", grbl_settings());
    let mut changed = grbl_settings();
    changed.max_spindle = 1000;
    let mut p2 = grbl_machine("graveur");
    p2 = MachineProfile::new(
        "graveur",
        2,
        *p2.frame(),
        p2.max_speed_mm_s(),
        p2.travel_speed_mm_s(),
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
    .with_capabilities(grbl_capabilities(&changed))
    .unwrap();
    h.send(UiIntent::SetGrblMachineProfile { profile: Box::new(p2), grbl: Box::new(changed.clone()) });
    h.run_tasks();
    let kept = stored(&h);
    let old = kept.iter().find(|e| e.profile.revision().id == "graveur").unwrap();
    assert_eq!(old.profile.revision().revision, 1);
    assert_eq!(old.grbl, Some(grbl_settings()));
    assert_eq!(h.controller().grbl_profile(), Some(&changed));
}
