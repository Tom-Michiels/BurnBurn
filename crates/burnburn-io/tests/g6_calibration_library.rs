//! G6-machine, W09: the calibration proofs (kerf, scan offset, pointer offset) are kept in a
//! library file next to the other libraries, so a measurement outlives the session. Written
//! atomically through the filesystem port; a newer schema is reported with its version, a damaged
//! file is a typed error and never an empty list.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use burnburn_io::calibration_library::{
    CALIBRATION_LIBRARY_SCHEMA, CalibrationLibrary, ProofKindRecord, ProofRecord, decode_calibration_library,
    encode_calibration_library, load_calibration_library, save_calibration_library,
};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{LoadError, fs::FileSystem};

fn kerf(id: u64, machine: &str, material: &str, mm: f64) -> ProofRecord {
    ProofRecord {
        id,
        kind: ProofKindRecord::Kerf,
        machine: machine.into(),
        material: material.into(),
        speed_mm_s: None,
        value_mm: mm,
        value_y_mm: None,
        method: "5 vierkanten van 20 mm gemeten met schuifmaat".into(),
        date: "2026-10-01".into(),
    }
}

fn scan(id: u64, speed: f64, mm: f64) -> ProofRecord {
    ProofRecord {
        id,
        kind: ProofKindRecord::ScanOffset,
        machine: "graveur".into(),
        material: String::new(),
        speed_mm_s: Some(speed),
        value_mm: mm,
        value_y_mm: None,
        method: "scanproef 4 snelheden".into(),
        date: "2026-10-02".into(),
    }
}

fn pointer(id: u64) -> ProofRecord {
    ProofRecord {
        id,
        kind: ProofKindRecord::PointerOffset,
        machine: "graveur".into(),
        material: String::new(),
        speed_mm_s: None,
        value_mm: 1.5,
        value_y_mm: Some(-2.25),
        method: "kruis gebrand, stip erop gelegd".into(),
        date: "2026-10-03".into(),
    }
}

#[test]
fn every_kind_of_proof_survives_a_save_and_load() {
    let fs = MemoryFileSystem::new();
    let path = Path::new("/lib/calibration.json");
    let proofs = vec![kerf(1, "graveur", "multiplex-3", 0.18), scan(2, 100.0, 0.1), scan(3, 300.0, 0.3), pointer(4)];
    let library = CalibrationLibrary { proofs, next_id: 9 };
    save_calibration_library(&fs, path, &library).unwrap();
    assert_eq!(load_calibration_library(&fs, path).unwrap(), library, "also the next id: a removed proof's id stays spent");
}

fn lib(proofs: Vec<ProofRecord>) -> CalibrationLibrary {
    let next_id = proofs.iter().map(|p| p.id).max().map_or(1, |m| m + 1);
    CalibrationLibrary { proofs, next_id }
}

#[test]
fn the_file_uses_its_documented_names() {
    // Typed out by hand: the field names of the format are pinned independently of the encoder.
    let bytes = br#"{"format":"burnburn-calibration-library","schema":1,"proofs":[
        {"id":9,"kind":"scan-offset","machine":"graveur","material":"","speed_mm_s":250.0,"value_mm":0.27,
         "value_y_mm":null,"method":"m","date":"2026-01-31"}]}"#;
    let library = decode_calibration_library(bytes).unwrap();
    assert_eq!(library.proofs.len(), 1);
    assert_eq!(library.proofs[0].kind, ProofKindRecord::ScanOffset);
    assert_eq!((library.proofs[0].speed_mm_s, library.proofs[0].value_mm, library.proofs[0].id), (Some(250.0), 0.27, 9));
    assert_eq!(library.next_id, 10, "a file without a next id continues after the highest id it holds");
}

#[test]
fn a_newer_schema_a_foreign_file_and_damage_are_refused_not_read_as_empty() {
    let good = encode_calibration_library(&lib(vec![kerf(1, "g", "m", 0.1)])).unwrap();
    let text = String::from_utf8(good).unwrap();
    let newer = text.replace(
        &format!("\"schema\":{CALIBRATION_LIBRARY_SCHEMA}"),
        &format!("\"schema\":{}", CALIBRATION_LIBRARY_SCHEMA + 1),
    );
    assert_ne!(newer, text);
    assert_eq!(
        decode_calibration_library(newer.as_bytes()).unwrap_err(),
        LoadError::NewerSchema { found: CALIBRATION_LIBRARY_SCHEMA + 1, supported: CALIBRATION_LIBRARY_SCHEMA }
    );
    assert!(decode_calibration_library(b"{\"format\":\"something-else\",\"schema\":1}").is_err());
    assert!(decode_calibration_library(b"not json").is_err());
    assert!(decode_calibration_library(b"").is_err());
}

#[test]
fn proofs_that_say_nothing_usable_are_refused() {
    let bad = |p: ProofRecord| encode_calibration_library(&lib(vec![p])).and_then(|b| {
        decode_calibration_library(&b).map_err(|e| burnburn_io::SaveError::Invalid(e.to_string()))
    });
    assert!(bad(kerf(1, "g", "m", -0.1)).is_err(), "negative kerf");
    assert!(bad(kerf(1, "g", "m", f64::NAN)).is_err(), "not a number");
    assert!(bad(kerf(1, "g", "  ", 0.1)).is_err(), "a kerf belongs to a material");
    assert!(bad(scan(1, 0.0, 0.1)).is_err(), "a scan offset is measured at a speed above zero");
    assert!(bad(ProofRecord { speed_mm_s: None, ..scan(1, 100.0, 0.1) }).is_err(), "a scan offset needs its speed");
    assert!(bad(ProofRecord { value_y_mm: None, ..pointer(1) }).is_err(), "a pointer offset has x and y");
    assert!(bad(ProofRecord { date: "1-10-2026".into(), ..kerf(1, "g", "m", 0.1) }).is_err(), "dates are YYYY-MM-DD");
    assert!(bad(ProofRecord { method: " ".into(), ..kerf(1, "g", "m", 0.1) }).is_err(), "the method is on record");
    assert!(bad(ProofRecord { machine: String::new(), ..kerf(1, "g", "m", 0.1) }).is_err(), "a proof belongs to a machine");
}

#[test]
fn two_proofs_with_one_id_are_refused_and_the_old_file_stays() {
    let fs = MemoryFileSystem::new();
    let path = Path::new("/lib/calibration.json");
    save_calibration_library(&fs, path, &lib(vec![kerf(1, "g", "m", 0.1)])).unwrap();
    let before = fs.read(path, 1 << 20).unwrap();
    assert!(save_calibration_library(&fs, path, &lib(vec![kerf(1, "g", "m", 0.1), kerf(1, "g", "n", 0.2)])).is_err());
    assert_eq!(fs.read(path, 1 << 20).unwrap(), before, "a refused list writes nothing");
}

#[test]
fn a_next_id_that_would_hand_out_an_id_in_use_is_refused() {
    let fs = MemoryFileSystem::new();
    let path = Path::new("/lib/calibration.json");
    let bad = CalibrationLibrary { proofs: vec![kerf(5, "g", "m", 0.1)], next_id: 5 };
    assert!(save_calibration_library(&fs, path, &bad).is_err());
    assert!(!fs.exists(path).unwrap());
}

#[test]
fn a_missing_file_is_an_fs_error_not_an_empty_list() {
    let fs = MemoryFileSystem::new();
    assert!(matches!(load_calibration_library(&fs, Path::new("/nope.json")), Err(LoadError::Fs(_))));
}
