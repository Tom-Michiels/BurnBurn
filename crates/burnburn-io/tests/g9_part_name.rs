//! G9-cam-werk, W01/W02: a project can carry an explicit part name, separate from the names of the
//! drawn objects, so that an order can name the product. Additive: a project without a name keeps
//! its file form and its hash, a file from before the field loads without a name, and the name is
//! production bookkeeping that the project hash does not see.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use burnburn_io::{LoadError, decode_container, encode_container, project_hash};
use common::{add_entry, sample_project};

fn named(name: &str) -> burnburn_io::Project {
    let mut p = sample_project();
    p.part_name = Some(name.to_owned());
    p
}

#[test]
fn a_part_name_survives_save_and_reopen_byte_for_byte() {
    let name = "Houder \"links\" — 日本 ø";
    let p = named(name);
    let bytes = encode_container(&p).unwrap();
    let back = decode_container(&bytes).unwrap();
    assert_eq!(back.part_name.as_deref(), Some(name));
    assert_eq!(encode_container(&back).unwrap(), bytes, "the second save is the same file");
}

#[test]
fn the_part_name_is_not_in_the_project_hash_and_a_project_without_one_keeps_its_file_form() {
    let plain = sample_project();
    assert_eq!(project_hash(&named("Houder")).unwrap(), project_hash(&plain).unwrap());
    let text = String::from_utf8_lossy(&encode_container(&plain).unwrap()).into_owned();
    assert!(!text.contains("part-name"), "no entry for a project without a name");
    let text = String::from_utf8_lossy(&encode_container(&named("Houder")).unwrap()).into_owned();
    assert!(text.contains("part-name"), "the name has its own entry");
}

#[test]
fn a_file_from_before_the_field_loads_without_a_name() {
    // The same project, written by the encoder without a name, is what older builds wrote.
    let back = decode_container(&encode_container(&sample_project()).unwrap()).unwrap();
    assert_eq!(back.part_name, None);
}

#[test]
fn a_blank_or_oversized_or_unknown_part_name_entry_is_refused_not_repaired() {
    let base = encode_container(&sample_project()).unwrap();
    for (label, json) in [
        ("blank", br#"{"name":"   "}"#.to_vec()),
        ("empty", br#"{"name":""}"#.to_vec()),
        ("long", format!(r#"{{"name":"{}"}}"#, "a".repeat(201)).into_bytes()),
        ("unknown field", br#"{"name":"Houder","extra":1}"#.to_vec()),
        ("not an object", br#""Houder""#.to_vec()),
    ] {
        let with = add_entry(&base, "part-name.json", &json);
        assert!(decode_container(&with).is_err(), "{label}");
    }
    let ok = add_entry(&base, "part-name.json", br#"{"name":"Houder"}"#);
    assert_eq!(decode_container(&ok).unwrap().part_name.as_deref(), Some("Houder"));
    // Twice is a damaged file.
    let twice = add_entry(&ok, "part-name.json", br#"{"name":"Ander"}"#);
    assert!(matches!(decode_container(&twice), Err(LoadError::UnexpectedEntry(_))));
}

#[test]
fn a_name_that_is_only_blank_is_not_written() {
    let mut p = sample_project();
    p.part_name = Some("  ".to_owned());
    let text = String::from_utf8_lossy(&encode_container(&p).unwrap()).into_owned();
    assert!(!text.contains("part-name"));
    // And a name with edge spaces is kept trimmed.
    p.part_name = Some("  Houder  ".to_owned());
    let back = decode_container(&encode_container(&p).unwrap()).unwrap();
    assert_eq!(back.part_name.as_deref(), Some("Houder"));
}
