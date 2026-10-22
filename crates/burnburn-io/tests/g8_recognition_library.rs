//! G8-cam-werk, W06: learned recognition templates survive a restart. The library keeps the grey
//! pixels of a template with the exact camera configuration it was learned in (a template is only
//! used with that configuration), written atomically; a newer schema, a damaged file or a template
//! that means nothing is a typed error and never an empty library; ids are never reused.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use burnburn_image::calib::{CameraConfigKey, CameraDeviceId, CameraMode, CameraPixelFormat};
use burnburn_io::fs::{FileSystem, FsOp};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::recognition_library::{
    RECOGNITION_LIBRARY_SCHEMA, RecognitionLibrary, TemplateProblem, TemplateRecord, decode_recognition_library,
    encode_recognition_library, load_recognition_library, save_recognition_library,
};
use burnburn_io::{LoadError, SaveError};

fn config(device: &str) -> CameraConfigKey {
    CameraConfigKey {
        device: CameraDeviceId(device.into()),
        mode: CameraMode { width: 1280, height: 720, pixel_format: CameraPixelFormat::Bgra8, frame_rate_mhz: 30_000 },
        lens: "none".into(),
    }
}

/// A 6 x 5 template with a pattern (a diagonal ramp): not flat.
fn pattern() -> Vec<u8> {
    (0..30u32).map(|i| ((i % 6) * 40 + (i / 6) * 9) as u8).collect()
}

fn record(id: u64, name: &str) -> TemplateRecord {
    TemplateRecord { id, name: name.into(), config: config("cam-1"), width: 6, height: 5, gray8: pattern() }
}

fn library(templates: Vec<TemplateRecord>) -> RecognitionLibrary {
    let next_id = templates.iter().map(|t| t.id).max().map_or(1, |m| m + 1);
    RecognitionLibrary { templates, next_id }
}

#[test]
fn templates_survive_a_save_and_load_exactly_with_their_next_id() {
    let fs = MemoryFileSystem::new();
    let path = Path::new("/lib/recognition-templates.json");
    let lib = RecognitionLibrary { templates: vec![record(1, "Sleutelhanger"), record(4, "Plaatje")], next_id: 9 };
    save_recognition_library(&fs, path, &lib).unwrap();
    assert_eq!(load_recognition_library(&fs, path).unwrap(), lib, "also the next id: a removed template's id stays spent");
}

#[test]
fn the_file_uses_its_documented_names() {
    // Typed out by hand: the names of the format are pinned independently of the encoder. The grey
    // pixels are two hex digits each, row by row.
    let bytes = br#"{"format":"burnburn-recognition-templates","schema":1,"next_id":8,"templates":[
        {"id":7,"name":"Hoek","config":{"device":"cam-1","width":1280,"height":720,
         "pixel_format":{"kind":"bgra8"},"frame_rate_mhz":30000,"lens":"none"},
         "width":4,"height":4,"gray8_hex":"00102030405060708090a0b0c0d0e0ff"}]}"#;
    let lib = decode_recognition_library(bytes).unwrap();
    assert_eq!(lib.next_id, 8);
    let t = &lib.templates[0];
    assert_eq!((t.id, t.name.as_str(), t.width, t.height), (7, "Hoek", 4, 4));
    assert_eq!(t.config, config("cam-1"));
    assert_eq!(t.gray8, vec![0x00, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80, 0x90, 0xa0, 0xb0, 0xc0, 0xd0, 0xe0, 0xff]);
}

#[test]
fn a_newer_schema_a_foreign_file_and_damage_are_refused_not_read_as_empty() {
    let text = String::from_utf8(encode_recognition_library(&library(vec![record(1, "A")])).unwrap()).unwrap();
    assert!(text.contains("\"templates\""), "the encoder writes the templates");
    let newer = text.replace(
        &format!("\"schema\":{RECOGNITION_LIBRARY_SCHEMA}"),
        &format!("\"schema\":{}", RECOGNITION_LIBRARY_SCHEMA + 1),
    );
    assert_ne!(newer, text);
    assert_eq!(
        decode_recognition_library(newer.as_bytes()).unwrap_err(),
        LoadError::NewerSchema { found: RECOGNITION_LIBRARY_SCHEMA + 1, supported: RECOGNITION_LIBRARY_SCHEMA }
    );
    assert!(decode_recognition_library(br#"{"format":"something-else","schema":1}"#).is_err());
    assert!(decode_recognition_library(b"not json").is_err());
    assert!(decode_recognition_library(b"").is_err());
    assert!(decode_recognition_library(&text.as_bytes()[..text.len() / 2]).is_err());
    let unknown = text.replacen("\"templates\"", "\"surprise\":1,\"templates\"", 1);
    assert!(matches!(decode_recognition_library(unknown.as_bytes()), Err(LoadError::UnknownField { .. })));
}

fn refused(t: TemplateRecord) -> bool {
    match encode_recognition_library(&library(vec![t])) {
        Err(_) => true,
        Ok(b) => decode_recognition_library(&b).is_err(),
    }
}

#[test]
fn templates_that_say_nothing_usable_are_refused() {
    assert!(!refused(record(1, "goed")), "the fixture itself is valid");
    assert!(refused(TemplateRecord { name: "  ".into(), ..record(1, "x") }), "a template has a name");
    assert!(refused(TemplateRecord { width: 3, height: 10, gray8: vec![0; 30], ..record(1, "x") }), "too small");
    assert!(refused(TemplateRecord { gray8: vec![7; 30], ..record(1, "x") }), "a flat template has no pattern");
    assert!(refused(TemplateRecord { gray8: pattern()[..29].to_vec(), ..record(1, "x") }), "pixels missing");
    assert!(refused(TemplateRecord { width: 6000, height: 6000, gray8: Vec::new(), ..record(1, "x") }), "too large");
    let mut c = config("cam-1");
    c.device = CameraDeviceId(String::new());
    assert!(refused(TemplateRecord { config: c, ..record(1, "x") }), "a template belongs to a camera");
    let mut c = config("cam-1");
    c.mode.width = 0;
    assert!(refused(TemplateRecord { config: c, ..record(1, "x") }), "a picture has a size");
}

#[test]
fn an_odd_length_or_non_hex_pixel_string_is_refused_on_load() {
    let good = String::from_utf8(encode_recognition_library(&library(vec![record(1, "A")])).unwrap()).unwrap();
    let hex: String = pattern().iter().map(|b| format!("{b:02x}")).collect();
    assert!(good.contains(&hex), "the pixels are written as hex");
    assert!(decode_recognition_library(good.replace(&hex, &hex[1..]).as_bytes()).is_err(), "odd length");
    assert!(decode_recognition_library(good.replace(&hex, &hex.replacen('0', "z", 1)).as_bytes()).is_err());
}

#[test]
fn two_templates_with_one_name_or_id_are_refused_and_the_old_file_stays() {
    let fs = MemoryFileSystem::new();
    let path = Path::new("/lib/recognition-templates.json");
    save_recognition_library(&fs, path, &library(vec![record(1, "A")])).unwrap();
    let before = fs.read(path, 1 << 20).unwrap();
    let same_name = library(vec![record(1, "Plaatje"), record(2, " plaatje ")]);
    assert!(matches!(save_recognition_library(&fs, path, &same_name), Err(SaveError::Invalid(_))));
    let same_id = RecognitionLibrary { templates: vec![record(1, "A"), record(1, "B")], next_id: 5 };
    assert!(matches!(save_recognition_library(&fs, path, &same_id), Err(SaveError::Invalid(_))));
    let reused = RecognitionLibrary { templates: vec![record(3, "A")], next_id: 3 };
    assert!(matches!(save_recognition_library(&fs, path, &reused), Err(SaveError::Invalid(_))), "next id in use");
    assert_eq!(fs.read(path, 1 << 20).unwrap(), before, "the old file is untouched");
}

#[test]
fn a_missing_file_is_a_typed_error_and_a_failed_write_keeps_the_old_file() {
    let fs = MemoryFileSystem::new();
    let path = Path::new("/lib/recognition-templates.json");
    assert!(matches!(load_recognition_library(&fs, path), Err(LoadError::Fs(_))));
    save_recognition_library(&fs, path, &library(vec![record(1, "A")])).unwrap();
    let before = fs.read(path, 1 << 20).unwrap();
    fs.fail_on(FsOp::Replace, 0);
    assert!(save_recognition_library(&fs, path, &library(vec![record(1, "A"), record(2, "B")])).is_err());
    assert_eq!(fs.read(path, 1 << 20).unwrap(), before);
}

#[test]
fn learning_a_template_with_a_name_in_use_replaces_it_and_keeps_its_id() {
    let mut lib = RecognitionLibrary::empty();
    let a = lib.upsert("Plaatje", config("cam-1"), 6, 5, pattern()).unwrap();
    let b = lib.upsert("Andere", config("cam-1"), 6, 5, pattern()).unwrap();
    assert_ne!(a, b);
    let mut other = pattern();
    other[0] = 255;
    let again = lib.upsert(" plaatje ", config("cam-2"), 6, 5, other.clone()).unwrap();
    assert_eq!(again, a, "same name: same template, re-taught");
    assert_eq!(lib.templates.len(), 2);
    let t = lib.find(a).unwrap();
    assert_eq!((t.config.device.0.as_str(), t.gray8[0], t.name.as_str()), ("cam-2", 255, "plaatje"));
    assert!(lib.remove(a));
    assert!(!lib.remove(a));
    let c = lib.upsert("Nieuw", config("cam-1"), 6, 5, pattern()).unwrap();
    assert!(c > b, "an id is not used again after a removal: {c} after {b}");
}

#[test]
fn upsert_refuses_what_a_file_would_refuse() {
    let mut lib = RecognitionLibrary::empty();
    assert_eq!(lib.upsert(" ", config("cam-1"), 6, 5, pattern()), Err(TemplateProblem::NoName));
    assert_eq!(lib.upsert("x", config("cam-1"), 6, 5, vec![7; 30]), Err(TemplateProblem::Flat));
    assert_eq!(lib.upsert("x", config("cam-1"), 6, 5, vec![0; 3]), Err(TemplateProblem::SizeMismatch));
    assert_eq!(lib.upsert("x", config("cam-1"), 2, 2, vec![0, 255, 255, 0]), Err(TemplateProblem::TooSmall));
    assert!(lib.templates.is_empty());
}
