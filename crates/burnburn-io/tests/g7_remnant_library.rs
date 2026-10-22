//! G7-werk, W07: the remnant library keeps leftover sheets between sessions: material, thickness,
//! contour and exclusion zones. Written atomically; a newer schema is reported with its version, a
//! damaged or contradictory file is a typed error and never an empty library; ids are never reused.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use burnburn_io::fs::{FileSystem, FsOp};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::remnant_library::{
    REMNANT_LIBRARY_SCHEMA, RemnantLibrary, RemnantRecord, decode_remnant_library, encode_remnant_library,
    load_remnant_library, save_remnant_library,
};
use burnburn_io::{LoadError, SaveError};

fn square(x: f64, y: f64, side: f64) -> Vec<[f64; 2]> {
    vec![[x, y], [x + side, y], [x + side, y + side], [x, y + side]]
}

fn remnant(id: u64, name: &str) -> RemnantRecord {
    RemnantRecord {
        id,
        name: name.into(),
        material: "multiplex".into(),
        thickness_mm: 3.0,
        // An L-shaped offcut with a hole-ish zone that stays out of use.
        contour: vec![[0.0, 0.0], [300.5, 0.0], [300.5, 120.25], [100.0, 120.25], [100.0, 250.0], [0.0, 250.0]],
        exclusions: vec![square(10.0, 10.0, 20.0)],
        note: "rand verbrand".into(),
        date: "2026-10-22".into(),
    }
}

fn library(remnants: Vec<RemnantRecord>) -> RemnantLibrary {
    let next_id = remnants.iter().map(|r| r.id).max().map_or(1, |m| m + 1);
    RemnantLibrary { remnants, next_id }
}

#[test]
fn remnants_survive_a_save_and_load_exactly_with_their_next_id() {
    let fs = MemoryFileSystem::new();
    let path = Path::new("/lib/remnants.json");
    let lib = RemnantLibrary { remnants: vec![remnant(1, "Restplaat A"), remnant(4, "Restplaat B")], next_id: 9 };
    save_remnant_library(&fs, path, &lib).unwrap();
    assert_eq!(load_remnant_library(&fs, path).unwrap(), lib, "also the next id: a removed remnant's id stays spent");
}

#[test]
fn the_file_uses_its_documented_names() {
    // Typed out by hand: the names of the format are pinned independently of the encoder.
    let bytes = br#"{"format":"burnburn-remnant-library","schema":1,"next_id":8,"remnants":[
        {"id":7,"name":"Hoek","material":"acryl","thickness_mm":5.0,
         "contour":[[0.0,0.0],[100.0,0.0],[100.0,50.0],[0.0,50.0]],
         "exclusions":[[[5.0,5.0],[15.0,5.0],[15.0,15.0],[5.0,15.0]]],
         "note":"","date":"2026-01-31"}]}"#;
    let lib = decode_remnant_library(bytes).unwrap();
    assert_eq!(lib.remnants.len(), 1);
    let r = &lib.remnants[0];
    assert_eq!((r.id, r.name.as_str(), r.material.as_str(), r.thickness_mm), (7, "Hoek", "acryl", 5.0));
    assert_eq!((r.contour.len(), r.exclusions.len(), r.exclusions[0].len()), (4, 1, 4));
    assert_eq!(lib.next_id, 8);
}

#[test]
fn a_newer_schema_a_foreign_file_and_damage_are_refused_not_read_as_empty() {
    let text = String::from_utf8(encode_remnant_library(&library(vec![remnant(1, "A")])).unwrap()).unwrap();
    assert!(text.contains("\"remnants\""), "the encoder writes the remnants");
    let newer = text.replace(
        &format!("\"schema\":{REMNANT_LIBRARY_SCHEMA}"),
        &format!("\"schema\":{}", REMNANT_LIBRARY_SCHEMA + 1),
    );
    assert_ne!(newer, text);
    assert_eq!(
        decode_remnant_library(newer.as_bytes()).unwrap_err(),
        LoadError::NewerSchema { found: REMNANT_LIBRARY_SCHEMA + 1, supported: REMNANT_LIBRARY_SCHEMA }
    );
    assert!(decode_remnant_library(br#"{"format":"something-else","schema":1}"#).is_err());
    assert!(decode_remnant_library(b"not json").is_err());
    assert!(decode_remnant_library(b"").is_err());
    assert!(decode_remnant_library(&text.as_bytes()[..text.len() / 2]).is_err());
    let unknown = text.replacen("\"remnants\"", "\"surprise\":1,\"remnants\"", 1);
    assert!(matches!(decode_remnant_library(unknown.as_bytes()), Err(LoadError::UnknownField { .. })));
}

fn refused(r: RemnantRecord) -> bool {
    let bytes = encode_remnant_library(&library(vec![r]));
    match bytes {
        Err(_) => true,
        Ok(b) => decode_remnant_library(&b).is_err(),
    }
}

#[test]
fn remnants_that_say_nothing_usable_are_refused() {
    assert!(!refused(remnant(1, "goed")), "the fixture itself is valid");
    assert!(refused(RemnantRecord { name: "  ".into(), ..remnant(1, "x") }), "a remnant has a name");
    assert!(refused(RemnantRecord { material: String::new(), ..remnant(1, "x") }), "a remnant has a material");
    for t in [0.0, -1.0, f64::NAN, f64::INFINITY, 500.0] {
        assert!(refused(RemnantRecord { thickness_mm: t, ..remnant(1, "x") }), "thickness {t}");
    }
    assert!(refused(RemnantRecord { contour: vec![[0.0, 0.0], [1.0, 0.0]], ..remnant(1, "x") }), "two points");
    assert!(
        refused(RemnantRecord { contour: vec![[0.0, 0.0], [5.0, 0.0], [10.0, 0.0]], ..remnant(1, "x") }),
        "no area"
    );
    assert!(
        refused(RemnantRecord { contour: vec![[0.0, 0.0], [10.0, 10.0], [10.0, 0.0], [0.0, 10.0]], ..remnant(1, "x") }),
        "a bow tie is not a sheet"
    );
    assert!(refused(RemnantRecord { contour: vec![[0.0, f64::NAN], [5.0, 0.0], [5.0, 5.0]], ..remnant(1, "x") }));
    assert!(
        refused(RemnantRecord { exclusions: vec![square(900.0, 900.0, 10.0)], ..remnant(1, "x") }),
        "a zone outside the sheet is nonsense"
    );
    assert!(refused(RemnantRecord { exclusions: vec![vec![[10.0, 10.0], [20.0, 10.0]]], ..remnant(1, "x") }));
    assert!(refused(RemnantRecord { date: "22-10-2026".into(), ..remnant(1, "x") }), "dates are YYYY-MM-DD");
}

#[test]
fn two_remnants_with_one_name_or_id_are_refused_and_the_old_file_stays() {
    let fs = MemoryFileSystem::new();
    let path = Path::new("/lib/remnants.json");
    save_remnant_library(&fs, path, &library(vec![remnant(1, "A")])).unwrap();
    let before = fs.read(path, 1 << 20).unwrap();
    let same_name = library(vec![remnant(1, "Restplaat"), remnant(2, " restplaat ")]);
    assert!(matches!(save_remnant_library(&fs, path, &same_name), Err(SaveError::Invalid(_))));
    let same_id = RemnantLibrary { remnants: vec![remnant(1, "A"), remnant(1, "B")], next_id: 5 };
    assert!(matches!(save_remnant_library(&fs, path, &same_id), Err(SaveError::Invalid(_))));
    let reused = RemnantLibrary { remnants: vec![remnant(3, "A")], next_id: 3 };
    assert!(matches!(save_remnant_library(&fs, path, &reused), Err(SaveError::Invalid(_))), "next id in use");
    assert_eq!(fs.read(path, 1 << 20).unwrap(), before, "the old file is untouched");
}

#[test]
fn a_missing_file_is_a_typed_error_and_a_failed_write_keeps_the_old_file() {
    let fs = MemoryFileSystem::new();
    let path = Path::new("/lib/remnants.json");
    assert!(matches!(load_remnant_library(&fs, path), Err(LoadError::Fs(_))));
    save_remnant_library(&fs, path, &library(vec![remnant(1, "A")])).unwrap();
    let before = fs.read(path, 1 << 20).unwrap();
    fs.fail_on(FsOp::Replace, 0);
    assert!(save_remnant_library(&fs, path, &library(vec![remnant(1, "A"), remnant(2, "B")])).is_err());
    assert_eq!(fs.read(path, 1 << 20).unwrap(), before);
}
