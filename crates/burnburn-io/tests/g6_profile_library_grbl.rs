//! G6-machine: the profile library keeps the GRBL dialect settings of a machine next to its
//! profile. The file format stays additive: a library without GRBL machines is byte-identical to
//! what earlier versions wrote; one with GRBL machines says so in a higher schema, so an older app
//! reports "newer file" instead of silently dropping the settings.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use burnburn_gcode::{GrblProfile, PowerMode, WorkCoordinate, grbl_capabilities};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::profile_library::{
    LibraryProfile, decode_profile_library, decode_profile_library_entries, encode_profile_library,
    encode_profile_library_entries, load_profile_library_entries, save_profile_library_entries,
};
use burnburn_io::{LoadError, fs::FileSystem};
use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};

fn base(id: &str) -> MachineProfile {
    MachineProfile::new(
        id,
        1,
        MachineFrame::new(400.0, 400.0, OriginCorner::BottomLeft).unwrap(),
        300.0,
        200.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
}

fn unusual_grbl() -> GrblProfile {
    GrblProfile {
        max_spindle: 255,
        laser_mode: true,
        power_mode: PowerMode::Constant,
        air_assist: true,
        work_coordinate: Some(WorkCoordinate::G55),
        decimals: 2,
    }
}

fn grbl_entry(id: &str, g: GrblProfile) -> LibraryProfile {
    let profile = base(id).with_capabilities(grbl_capabilities(&g)).unwrap();
    LibraryProfile { profile, grbl: Some(g) }
}

#[test]
fn the_dialect_settings_of_a_grbl_machine_survive_the_library_file() {
    let fs = MemoryFileSystem::new();
    let path = Path::new("/lib/profiles.json");
    let entries = [grbl_entry("graveur", unusual_grbl()), LibraryProfile { profile: base("ruida-bed"), grbl: None }];
    save_profile_library_entries(&fs, path, &entries).unwrap();
    let back = load_profile_library_entries(&fs, path).unwrap();
    let key = |v: &[LibraryProfile]| -> Vec<_> { v.iter().map(|e| (e.profile.revision().clone(), e.grbl.clone())).collect() };
    assert_eq!(key(&back), key(&entries), "settings of every field come back, a Ruida machine stays without");
}

#[test]
fn a_library_without_grbl_machines_is_written_exactly_as_before() {
    let plain = [base("a"), base("b")];
    let entries: Vec<LibraryProfile> =
        plain.iter().cloned().map(|profile| LibraryProfile { profile, grbl: None }).collect();
    assert_eq!(encode_profile_library_entries(&entries).unwrap(), encode_profile_library(&plain).unwrap());
}

#[test]
fn a_file_with_grbl_machines_has_a_higher_schema_that_is_still_read_by_the_profile_only_reader() {
    let entries = [grbl_entry("graveur", unusual_grbl())];
    let bytes = encode_profile_library_entries(&entries).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["schema"], 2, "an older app sees a newer schema instead of dropping the settings");
    // The profile-only reader of this version still lists the machine.
    let plain = decode_profile_library(&bytes).unwrap();
    assert_eq!(plain.len(), 1);
    assert_eq!(plain[0].revision().id, "graveur");
}

#[test]
fn a_hand_written_dialect_section_is_read_by_its_documented_names() {
    // The profile part is written by the library; the dialect section is typed by hand here, so
    // the field names of the file format are pinned independently of the encoder.
    let bytes = encode_profile_library(&[base("graveur")]).unwrap();
    let mut json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    json["schema"] = 2.into();
    json["dialects"] = serde_json::json!([{
        "id": "graveur",
        "grbl": {
            "max_spindle": 1000, "laser_mode": false, "power_mode": "dynamic",
            "air_assist": false, "work_coordinate": "G59", "decimals": 4
        }
    }]);
    let entries = decode_profile_library_entries(&serde_json::to_vec(&json).unwrap()).unwrap();
    let grbl = entries[0].grbl.clone().expect("the dialect is attached to the machine with that id");
    assert_eq!(
        grbl,
        GrblProfile {
            max_spindle: 1000,
            laser_mode: false,
            power_mode: PowerMode::Dynamic,
            air_assist: false,
            work_coordinate: Some(WorkCoordinate::G59),
            decimals: 4
        }
    );
}

#[test]
fn damaged_dialect_sections_are_typed_errors_and_never_adopted() {
    let bytes = encode_profile_library(&[base("graveur")]).unwrap();
    let with = |dialects: serde_json::Value| {
        let mut json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        json["schema"] = 2.into();
        json["dialects"] = dialects;
        serde_json::to_vec(&json).unwrap()
    };
    let grbl = |patch: serde_json::Value| {
        let mut g = serde_json::json!({"max_spindle": 1000, "laser_mode": true, "power_mode": "dynamic",
            "air_assist": false, "work_coordinate": null, "decimals": 3});
        for (k, v) in patch.as_object().unwrap() {
            g[k] = v.clone();
        }
        g
    };
    let load = |patch: serde_json::Value| {
        decode_profile_library_entries(&with(serde_json::json!([{"id": "graveur", "grbl": grbl(patch)}])))
    };
    // $30 = 0 and decimals outside 1..=4 are refused like everywhere else.
    for patch in [serde_json::json!({"max_spindle": 0}), serde_json::json!({"decimals": 9})] {
        let err = load(patch).unwrap_err();
        assert!(matches!(err, LoadError::InvalidValue { .. }), "{err:?}");
    }
    // Unknown power mode / work coordinate.
    for patch in [serde_json::json!({"power_mode": "turbo"}), serde_json::json!({"work_coordinate": "G60"})] {
        let err = load(patch).unwrap_err();
        assert!(matches!(err, LoadError::InvalidValue { .. }), "{err:?}");
    }
    // A dialect for a machine that is not in the file, and two for one machine.
    let ok = grbl(serde_json::json!({}));
    assert!(decode_profile_library_entries(&with(serde_json::json!([{"id": "spook", "grbl": ok.clone()}]))).is_err());
    let twice = serde_json::json!([{"id": "graveur", "grbl": ok.clone()}, {"id": "graveur", "grbl": ok}]);
    assert!(decode_profile_library_entries(&with(twice)).is_err());
    // Sanity: the same section with sound values loads.
    assert!(load(serde_json::json!({})).is_ok());
}

#[test]
fn saving_an_invalid_dialect_writes_nothing_and_keeps_the_old_file() {
    let fs = MemoryFileSystem::new();
    let path = Path::new("/lib/profiles.json");
    save_profile_library_entries(&fs, path, &[grbl_entry("graveur", unusual_grbl())]).unwrap();
    let before = fs.read(path, 1 << 20).unwrap();
    let mut bad = unusual_grbl();
    bad.max_spindle = 0;
    assert!(save_profile_library_entries(&fs, path, &[grbl_entry("graveur", bad)]).is_err());
    assert_eq!(fs.read(path, 1 << 20).unwrap(), before);
}
