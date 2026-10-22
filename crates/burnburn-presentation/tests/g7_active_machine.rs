//! G7-werk, M01: the active machine is kept between starts. The machine in use is written to the
//! user's folder (next to the profile library) whenever it changes and is the machine of the next
//! start, with its GRBL dialect and declared identity. A machine chosen before the file was read
//! wins; a damaged file is left alone and the start is without a machine, as before.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_test_support::host::rig as common;

use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use burnburn_gcode::{GrblProfile, PowerMode, WorkCoordinate, grbl_capabilities};
use burnburn_io::fs::FileSystem;
use burnburn_io::memory::MemoryFileSystem;
use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
use burnburn_presentation::{HeadlessEditorHarness, UiIntent};
use common::RUIDA;

const FILE: &str = "/lib/active-machine.json";

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

/// One run of the app on the shared disk: the folder is set, no machine is chosen yet.
fn start(fs: &Arc<MemoryFileSystem>) -> HeadlessEditorHarness {
    let mut h = HeadlessEditorHarness::with_fs(Arc::clone(fs));
    h.fs().create_dir_all_for_test(FsPath::new("/lib"));
    h.controller().set_library_dir(Some(PathBuf::from("/lib")));
    h.run_tasks();
    h
}

fn activate(h: &mut HeadlessEditorHarness, p: MachineProfile) {
    h.send(UiIntent::SetMachineProfile { profile: Box::new(p), ruida: RUIDA });
    h.run_tasks();
}

fn active(h: &mut HeadlessEditorHarness) -> Option<(String, f64)> {
    h.controller().machine_profile().map(|p| (p.revision().id.clone(), p.frame().width_mm()))
}

fn grbl() -> GrblProfile {
    GrblProfile {
        max_spindle: 255,
        laser_mode: true,
        power_mode: PowerMode::Constant,
        air_assist: true,
        work_coordinate: Some(WorkCoordinate::G56),
        decimals: 2,
    }
}

#[test]
fn a_fresh_start_has_no_machine_and_no_file() {
    let fs = Arc::new(MemoryFileSystem::new());
    let mut h = start(&fs);
    assert_eq!(active(&mut h), None);
    assert!(!fs.exists(FsPath::new(FILE)).unwrap(), "nothing is written for a machine that is not there");
}

#[test]
fn the_machine_in_use_is_the_machine_of_the_next_start() {
    let fs = Arc::new(MemoryFileSystem::new());
    let mut first = start(&fs);
    activate(&mut first, machine("tweede-machine", 600.0));
    assert!(fs.exists(FsPath::new(FILE)).unwrap());
    drop(first);

    let mut second = start(&fs);
    assert_eq!(active(&mut second), Some(("tweede-machine".to_owned(), 600.0)));
}

#[test]
fn the_last_chosen_machine_wins_not_the_first_one() {
    let fs = Arc::new(MemoryFileSystem::new());
    let mut first = start(&fs);
    activate(&mut first, machine("eerste", 500.0));
    activate(&mut first, machine("tweede", 600.0));
    first.send(UiIntent::ChooseMachineProfile { id: "eerste".into() });
    first.run_tasks();
    assert_eq!(active(&mut first), Some(("eerste".to_owned(), 500.0)));
    drop(first);

    let mut second = start(&fs);
    assert_eq!(active(&mut second), Some(("eerste".to_owned(), 500.0)), "the choice is what is kept");
    let known: Vec<String> = second.controller().known_profiles().iter().map(|p| p.revision().id.clone()).collect();
    assert!(known.contains(&"tweede".to_owned()), "the other machine stays known: {known:?}");
}

#[test]
fn a_grbl_machine_comes_back_with_its_dialect_and_the_declared_identity() {
    let fs = Arc::new(MemoryFileSystem::new());
    let mut first = start(&fs);
    let profile = machine("graveur", 410.0).with_capabilities(grbl_capabilities(&grbl())).unwrap();
    first.send(UiIntent::SetGrblMachineProfile { profile: Box::new(profile), grbl: Box::new(grbl()) });
    first.run_tasks();
    drop(first);

    let mut second = start(&fs);
    assert_eq!(active(&mut second), Some(("graveur".to_owned(), 410.0)));
    assert_eq!(second.controller().grbl_profile(), Some(&grbl()), "it drives as GRBL again, with its settings");
}

#[test]
fn a_machine_chosen_before_the_file_was_read_wins_and_is_written() {
    let fs = Arc::new(MemoryFileSystem::new());
    let mut first = start(&fs);
    activate(&mut first, machine("oud", 500.0));
    drop(first);

    let mut second = HeadlessEditorHarness::with_fs(Arc::clone(&fs));
    second.fs().create_dir_all_for_test(FsPath::new("/lib"));
    second.controller().set_library_dir(Some(PathBuf::from("/lib")));
    // The host chooses before the first file job has run.
    second.send(UiIntent::SetMachineProfile { profile: Box::new(machine("nieuw", 700.0)), ruida: RUIDA });
    second.run_tasks();
    assert_eq!(active(&mut second), Some(("nieuw".to_owned(), 700.0)));
    drop(second);

    let mut third = start(&fs);
    assert_eq!(active(&mut third), Some(("nieuw".to_owned(), 700.0)));
}

#[test]
fn a_damaged_file_leaves_the_start_without_a_machine_and_is_not_overwritten() {
    let fs = Arc::new(MemoryFileSystem::new());
    fs.create_dir_all_for_test(FsPath::new("/lib"));
    fs.insert(FsPath::new(FILE), b"{ not json".to_vec());
    let mut h = start(&fs);
    assert_eq!(active(&mut h), None);
    activate(&mut h, machine("nieuw", 700.0));
    assert_eq!(active(&mut h), Some(("nieuw".to_owned(), 700.0)), "usable in this session");
    assert_eq!(fs.read(FsPath::new(FILE), 1 << 20).unwrap(), b"{ not json".to_vec());
}
