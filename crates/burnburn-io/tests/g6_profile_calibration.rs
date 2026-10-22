//! G6-machine, W09: a profile with measured kerf and scan offsets is stored with them. The content
//! hash covers the measurements, so a changed value behind the hash is refused, and a library
//! with calibrated profiles says so in its schema.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::profile_library::{
    LibraryProfile, decode_profile_library, encode_profile_library, load_profile_library_entries,
    save_profile_library_entries,
};
use burnburn_io::{ProfileComparison, compare_profile};
use burnburn_job::{
    Calibration, KerfProof, MachineFrame, MachineProfile, OriginCorner, PowerRatio, ProofRef, ScanOffsetProof,
};

fn named(id: &str) -> MachineProfile {
    MachineProfile::new(
        id,
        2,
        MachineFrame::new(400.0, 300.0, OriginCorner::TopLeft).unwrap(),
        300.0,
        200.0,
        PowerRatio::new(0.0).unwrap(),
        PowerRatio::new(1.0).unwrap(),
    )
    .unwrap()
}

fn plain() -> MachineProfile {
    named("graveur")
}

fn calibration() -> Calibration {
    let proof = |id, date: &str| ProofRef { id, date: date.into() };
    Calibration::new(
        vec![KerfProof { material: "multiplex-3".into(), kerf_mm: 0.18, proof: proof(1, "2026-10-01") }],
        vec![
            ScanOffsetProof { speed_mm_s: 100.0, offset_mm: 0.10, proof: proof(2, "2026-10-02") },
            ScanOffsetProof { speed_mm_s: 300.0, offset_mm: 0.31, proof: proof(3, "2026-10-03") },
        ],
    )
    .unwrap()
}

fn calibrated() -> MachineProfile {
    plain().with_calibration(calibration()).unwrap()
}

fn entry(profile: MachineProfile) -> LibraryProfile {
    LibraryProfile { profile, grbl: None, model: None, firmware: None }
}

#[test]
fn the_measurements_of_a_profile_survive_the_library_file() {
    let fs = MemoryFileSystem::new();
    let path = Path::new("/lib/profiles.json");
    let other = named("zonder-metingen");
    save_profile_library_entries(&fs, path, &[entry(calibrated()), entry(other.clone())]).unwrap();
    let back = load_profile_library_entries(&fs, path).unwrap();
    assert_eq!(back[0].profile.calibration(), &calibration());
    assert_eq!(back[0].profile.revision(), calibrated().revision());
    assert!(back[1].profile.calibration().is_empty());
    assert_eq!(back[1].profile.revision(), other.revision(), "a profile without measurements keeps its revision");
}

#[test]
fn a_library_without_measurements_is_written_as_before_and_one_with_them_in_the_newer_schema() {
    let bytes = encode_profile_library(&[plain()]).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["schema"], 1);
    assert!(json["profiles"][0].get("calibration").is_none());

    let bytes = encode_profile_library(&[calibrated()]).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["schema"], 2, "an older app reports a newer file instead of dropping the measurements");
    assert_eq!(decode_profile_library(&bytes).unwrap()[0].calibration(), &calibration());
}

#[test]
fn a_hand_written_calibration_section_is_read_by_its_documented_names() {
    // Same values as `calibrated()`, typed out: the field names of the file format are pinned
    // independently of the encoder (the content hash must still match).
    let bytes = encode_profile_library(&[calibrated()]).unwrap();
    let mut json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    json["profiles"][0]["calibration"] = serde_json::json!({
        "kerf": [{"material": "multiplex-3", "kerf_mm": 0.18, "proof": 1, "date": "2026-10-01"}],
        "scan_offsets": [
            {"speed_mm_s": 100.0, "offset_mm": 0.10, "proof": 2, "date": "2026-10-02"},
            {"speed_mm_s": 300.0, "offset_mm": 0.31, "proof": 3, "date": "2026-10-03"}
        ]
    });
    let profiles = decode_profile_library(&serde_json::to_vec(&json).unwrap()).unwrap();
    assert_eq!(profiles[0].calibration(), &calibration());
}

#[test]
fn a_measurement_changed_behind_the_hash_is_refused() {
    let bytes = encode_profile_library(&[calibrated()]).unwrap();
    let mut json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    json["profiles"][0]["calibration"]["kerf"][0]["kerf_mm"] = serde_json::json!(0.30);
    assert!(decode_profile_library(&serde_json::to_vec(&json).unwrap()).is_err());
    // And a measurement that is not usable at all is a typed refusal, not a silent drop.
    json["profiles"][0]["calibration"]["kerf"][0]["kerf_mm"] = serde_json::json!(-1.0);
    assert!(decode_profile_library(&serde_json::to_vec(&json).unwrap()).is_err());
}

#[test]
fn comparing_a_frozen_profile_with_a_recalibrated_one_names_the_calibration() {
    let frozen = calibrated();
    let newer = plain().with_calibration(Calibration::default()).unwrap();
    assert_eq!(
        compare_profile(&frozen, &newer),
        ProfileComparison::Differs { fields: vec!["calibration"] },
        "the machine itself is the same, only its measurements differ"
    );
    assert_eq!(compare_profile(&frozen, &calibrated()), ProfileComparison::Identical);
}
