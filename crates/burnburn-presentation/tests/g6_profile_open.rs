//! G6-machine: a host opens a machine profile file (Ruida or GRBL). The file is read off the UI
//! thread, a usable one becomes the active machine with its dialect and the model and firmware it
//! declares, an unusable one changes nothing and says so.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::path::{Path as FsPath, PathBuf};

use burnburn_gcode::{PowerMode, WorkCoordinate};
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, HostEffect, MessageKey, UiIntent};
use common::{RUIDA, profile};

const GRBL_FILE: &str = r#"{
  "id": "grbl-diode-400x300", "revision": 1, "bed_mm": [400, 300], "origin": "bottom-left",
  "max_speed_mm_s": 200, "travel_speed_mm_s": 100, "power_range": [0, 1],
  "backend": "grbl",
  "grbl": { "max_spindle": 255, "power_mode": "constant", "work_coordinate": "G55", "air_assist": true },
  "identity": { "machine_id": "grbl-1", "model": "diode-5w", "firmware": "1.1h" }
}"#;

const RUIDA_FILE: &str = r#"{
  "id": "bench-600x400", "revision": 3, "bed_mm": [600, 400], "origin": "top-left",
  "max_speed_mm_s": 500, "travel_speed_mm_s": 300, "power_range": [0, 1],
  "ruida": { "magic": 136, "overscan": "controller-managed" }
}"#;

fn rig() -> HeadlessEditorHarness {
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(FsPath::new("/profielen"));
    h.fs().insert(FsPath::new("/profielen/grbl.json"), GRBL_FILE.as_bytes().to_vec());
    h.fs().insert(FsPath::new("/profielen/ruida.json"), RUIDA_FILE.as_bytes().to_vec());
    h.fs().insert(FsPath::new("/profielen/kapot.json"), b"{ not a profile".to_vec());
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    h
}

fn open(h: &mut HeadlessEditorHarness, name: &str) -> Vec<HostEffect> {
    let mut effects = h.send(UiIntent::OpenMachineProfile { path: PathBuf::from("/profielen").join(name) }).effects;
    for update in h.run_tasks() {
        effects.extend(update.effects);
    }
    effects
}

#[test]
fn a_grbl_file_becomes_the_active_grbl_machine_with_its_declared_identity() {
    let mut h = rig();
    let effects = open(&mut h, "grbl.json");
    assert!(!effects.iter().any(|e| matches!(e, HostEffect::ShowError { .. })), "{effects:?}");
    let c = h.controller();
    assert_eq!(c.machine_profile().unwrap().revision().id, "grbl-diode-400x300");
    let grbl = c.grbl_profile().expect("driven as GRBL");
    assert_eq!(
        (grbl.max_spindle, grbl.power_mode, grbl.work_coordinate),
        (255, PowerMode::Constant, Some(WorkCoordinate::G55))
    );
    assert_eq!(c.declared_identity(), (Some("diode-5w".into()), Some("1.1h".into())));
}

#[test]
fn a_ruida_file_becomes_the_active_ruida_machine_and_the_replaced_one_is_kept() {
    let mut h = rig();
    open(&mut h, "grbl.json");
    open(&mut h, "ruida.json");
    let c = h.controller();
    assert_eq!(c.machine_profile().unwrap().revision().id, "bench-600x400");
    assert_eq!(c.grbl_profile(), None);
    assert_eq!(c.declared_identity(), (None, None));
    let mut known: Vec<String> = c.known_profiles().iter().map(|p| p.revision().id.clone()).collect();
    assert_eq!(known[0], "bench-600x400", "the active machine is listed first");
    known.sort();
    assert_eq!(known, ["bench-600x400", "grbl-diode-400x300", "research-1500x900"], "replaced machines stay listed");
}

#[test]
fn a_damaged_or_missing_file_changes_nothing_and_says_so() {
    for name in ["kapot.json", "bestaat-niet.json"] {
        let mut h = rig();
        let effects = open(&mut h, name);
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, HostEffect::ShowError { message: MessageKey::MachineProfileFileUnusable, .. })),
            "{name}: {effects:?}"
        );
        assert_eq!(h.controller().machine_profile().unwrap().revision().id, "research-1500x900", "{name}");
    }
}

#[test]
fn the_command_asks_the_host_for_a_file_without_needing_a_window() {
    let mut h = HeadlessEditorHarness::new();
    let update = h.controller().handle(UiIntent::Invoke { document: None, command: CommandKey::OpenMachineProfile });
    assert!(update.effects.contains(&HostEffect::ShowMachineProfileOpenPanel), "{:?}", update.effects);
}
