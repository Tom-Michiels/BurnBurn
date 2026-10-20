//! W2-L6, Art Library: vector assets in named groups with thumbnails; the library embeds the
//! source bytes (no file paths), so moving the library file or the original files breaks nothing.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers and let-else in tests

mod common;

use std::path::PathBuf;

use burnburn_io::art_library::{
    ART_LIBRARY_SCHEMA, ArtError, ArtFormat, ArtLibrary, NewArt, decode_art_library, encode_art_library,
    load_art_library, save_art_library,
};
use burnburn_io::fs::FileSystem;
use burnburn_io::import::{DxfScale, parse_svg};
use burnburn_io::memory::MemoryFileSystem;
use burnburn_io::{LoadError, asset_hash};
use burnburn_types::Unit;

const STAR: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20mm" height="20mm" viewBox="0 0 20 20">
  <path id="ster" d="M10 0 L12 8 L20 8 L14 13 L16 20 L10 15 L4 20 L6 13 L0 8 L8 8 Z"/>
  <circle id="oog" cx="10" cy="10" r="2"/></svg>"#;
const BOX: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="30mm" height="10mm" viewBox="0 0 30 10"><rect id="doos" x="0" y="0" width="30" height="10"/></svg>"#;

fn new<'a>(name: &'a str, group: &'a str, svg: &'a str) -> NewArt<'a> {
    NewArt { name, group, tags: vec![], format: ArtFormat::Svg, bytes: svg.as_bytes(), dxf_scale: None }
}

fn library() -> ArtLibrary {
    let mut lib = ArtLibrary::new("kunst");
    lib.add_group("Vormen").unwrap();
    lib.add_group("Verpakking").unwrap();
    let mut star = new("Ster", "Vormen", STAR);
    star.tags = vec!["kerst".into(), "decoratie".into()];
    lib.add_item(star).unwrap();
    lib.add_item(new("Doos", "Verpakking", BOX)).unwrap();
    lib
}

#[test]
fn items_are_validated_by_importing_them_and_placing_gives_the_import_at_physical_scale() {
    let lib = library();
    let star = lib.items().iter().find(|i| i.name() == "Ster").unwrap();
    assert_eq!(
        (star.group(), star.format(), star.tags()),
        ("Vormen", ArtFormat::Svg, ["kerst".to_owned(), "decoratie".to_owned()].as_slice())
    );
    assert_eq!(star.hash(), asset_hash(STAR.as_bytes()));
    let placed = lib.place(star.id()).unwrap();
    let direct = parse_svg(STAR.as_bytes()).unwrap();
    assert_eq!(placed.draft, direct, "placing is the normal import of the stored bytes");
    assert_eq!((placed.media, placed.hash), ("image/svg+xml", asset_hash(STAR.as_bytes())));
    assert_eq!(&*placed.bytes, STAR.as_bytes());
    // The draft commits like any import: one document command, source tracked by asset hash.
    assert!(placed.draft.into_command(placed.hash).is_ok());

    let mut lib = library();
    assert!(matches!(lib.add_item(new("Kapot", "Vormen", "<svg")), Err(ArtError::Import(_))));
    assert_eq!(
        lib.add_item(new("Leeg", "Vormen", r#"<svg xmlns="http://www.w3.org/2000/svg"/>"#)),
        Err(ArtError::NothingToImport)
    );
    assert_eq!(lib.add_item(new("X", "Bestaat niet", STAR)), Err(ArtError::UnknownGroup("Bestaat niet".into())));
    assert_eq!(lib.add_item(new("  ", "Vormen", STAR)), Err(ArtError::InvalidName));
    assert_eq!(lib.items().len(), 2, "nothing half-added");
}

#[test]
fn dxf_art_needs_a_scale_when_the_file_has_no_unit_and_keeps_it_afterwards() {
    let dxf =
        std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-data/import/unitless-lines.dxf"))
            .unwrap();
    let mut lib = ArtLibrary::new("k");
    lib.add_group("CAD").unwrap();
    let item = |scale| NewArt {
        name: "Hoek",
        group: "CAD",
        tags: vec![],
        format: ArtFormat::Dxf,
        bytes: &dxf,
        dxf_scale: scale,
    };
    assert_eq!(lib.add_item(item(None)), Err(ArtError::ScaleRequired));
    let id = lib.add_item(item(Some(DxfScale::unit(Unit::Inch)))).unwrap();
    let placed = lib.place(id).unwrap();
    // The fixture is 4 × 2 drawing units: with inch that is 101.6 × 50.8 mm.
    let b = placed.draft.items.iter().filter_map(|i| i.path.bounds()).reduce(burnburn_geometry::Bounds::union).unwrap();
    assert!((b.max().x - 101.6).abs() < 1e-9 && (b.max().y - 50.8).abs() < 1e-9, "{b:?}");
    // The choice survives a save and load.
    let back = decode_art_library(&encode_art_library(&lib).unwrap()).unwrap();
    let again = back.place(id).unwrap();
    assert_eq!(again.draft, placed.draft);
}

#[test]
fn plt_and_pdf_assets_are_accepted() {
    let plt = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-data/import/square-circle.plt"))
        .unwrap();
    let pdf = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-data/import/plate-layers.pdf"))
        .unwrap();
    let mut lib = ArtLibrary::new("k");
    lib.add_group("Import").unwrap();
    let a = lib
        .add_item(NewArt {
            name: "Plot",
            group: "Import",
            tags: vec![],
            format: ArtFormat::Hpgl,
            bytes: &plt,
            dxf_scale: None,
        })
        .unwrap();
    let b = lib
        .add_item(NewArt {
            name: "Plaat",
            group: "Import",
            tags: vec![],
            format: ArtFormat::Pdf,
            bytes: &pdf,
            dxf_scale: None,
        })
        .unwrap();
    assert_eq!(lib.place(a).unwrap().draft.items.len(), 3);
    assert_eq!(lib.place(b).unwrap().draft.items.len(), 2);
    assert_eq!(lib.place(b).unwrap().media, "application/pdf");
}

#[test]
fn thumbnails_are_small_derived_svgs_of_the_drawing() {
    let lib = library();
    let id = lib.items()[0].id();
    let svg = lib.thumbnail(id).unwrap();
    assert!(svg.contains(r#"width="64px""#) && svg.contains(r#"height="64px""#), "{svg}");
    assert!(svg.contains(r#"viewBox="0 0 20 20""#), "the star spans 20 × 20 mm: {svg}");
    assert_eq!(svg.matches("<path").count(), 2, "star and eye");
    assert!(svg.len() < 4096, "a thumbnail stays small: {} bytes", svg.len());
    assert_eq!(lib.thumbnail(999), Err(ArtError::UnknownItem(999)));
}

#[test]
fn groups_can_be_added_renamed_and_removed_without_losing_items() {
    let mut lib = library();
    assert_eq!(lib.groups(), ["Vormen", "Verpakking"]);
    assert_eq!(lib.add_group("Vormen"), Err(ArtError::DuplicateGroup("Vormen".into())));
    assert_eq!(lib.add_group(" "), Err(ArtError::InvalidName));
    lib.rename_group("Vormen", "Figuren").unwrap();
    assert_eq!(lib.groups(), ["Figuren", "Verpakking"], "order kept");
    assert_eq!(lib.items_in("Figuren").iter().map(|i| i.name()).collect::<Vec<_>>(), ["Ster"]);
    assert_eq!(lib.rename_group("Figuren", "Verpakking"), Err(ArtError::DuplicateGroup("Verpakking".into())));
    assert_eq!(lib.rename_group("Nope", "X"), Err(ArtError::UnknownGroup("Nope".into())));
    // A group with items is not removed silently.
    assert_eq!(lib.remove_group("Figuren", None), Err(ArtError::GroupNotEmpty("Figuren".into())));
    lib.remove_group("Figuren", Some("Verpakking")).unwrap();
    assert_eq!(lib.groups(), ["Verpakking"]);
    assert_eq!(lib.items_in("Verpakking").len(), 2, "items moved along");
    lib.add_group("Leeg").unwrap();
    lib.remove_group("Leeg", None).unwrap();
    assert_eq!(lib.remove_group("Verpakking", Some("Verpakking")), Err(ArtError::GroupNotEmpty("Verpakking".into())));
}

#[test]
fn items_move_rename_retag_and_are_found_by_name_tag_or_group() {
    let mut lib = library();
    let (star, boxx) = (lib.items()[0].id(), lib.items()[1].id());
    let names = |v: Vec<&burnburn_io::art_library::ArtItem>| v.iter().map(|i| i.name().to_owned()).collect::<Vec<_>>();
    assert_eq!(names(lib.search("KERST")), ["Ster"], "case-insensitive, tags count");
    assert_eq!(names(lib.search("verpak")), ["Doos"], "group names count");
    assert_eq!(names(lib.search("o")), ["Ster", "Doos"], "stable library order");
    assert!(lib.search("zzz").is_empty());
    lib.move_item(star, "Verpakking").unwrap();
    assert_eq!(names(lib.items_in("Verpakking")), ["Ster", "Doos"]);
    assert_eq!(lib.move_item(star, "Nope"), Err(ArtError::UnknownGroup("Nope".into())));
    lib.rename_item(boxx, "Doosje").unwrap();
    lib.set_tags(boxx, vec!["karton".into()]).unwrap();
    assert_eq!(names(lib.search("karton")), ["Doosje"]);
    assert_eq!(lib.rename_item(boxx, ""), Err(ArtError::InvalidName));
    assert!(lib.remove_item(star) && !lib.remove_item(star));
    let again = lib.add_item(new("Ster 2", "Verpakking", STAR)).unwrap();
    assert!(again != star && again != boxx, "ids are never reused");
}

#[test]
fn the_library_roundtrips_exactly_and_works_after_it_is_moved() {
    let lib = library();
    let fs = MemoryFileSystem::new();
    let old = PathBuf::from("/oud/kunst.burnburnart");
    save_art_library(&fs, &old, &lib).unwrap();
    let loaded = load_art_library(&fs, &old).unwrap();
    assert_eq!(loaded, lib);
    assert_eq!(encode_art_library(&loaded).unwrap(), encode_art_library(&lib).unwrap(), "deterministic bytes");
    // Path policy: nothing in the file refers to where it lives. Copy it elsewhere, load there.
    let new_path = PathBuf::from("/nieuw/ergens/anders.burnburnart");
    fs.insert(&new_path, fs.read(&old, 1 << 30).unwrap());
    assert_eq!(load_art_library(&fs, &new_path).unwrap(), lib);
}

#[test]
fn damaged_tampered_and_newer_libraries_are_typed_errors() {
    let good = encode_art_library(&library()).unwrap();
    // An item whose bytes were changed no longer matches its recorded hash.
    let tampered = common::rewrite_entry(&good, "items/1", |mut b| {
        b.push(b' ');
        b
    });
    assert!(
        matches!(decode_art_library(&tampered), Err(LoadError::AssetHashMismatch(_))),
        "{:?}",
        decode_art_library(&tampered)
    );
    let newer = common::rewrite_json(&good, "library.json", |v| v["schema"] = (ART_LIBRARY_SCHEMA + 1).into());
    assert_eq!(
        decode_art_library(&newer),
        Err(LoadError::NewerSchema { found: ART_LIBRARY_SCHEMA + 1, supported: ART_LIBRARY_SCHEMA })
    );
    let stray = common::add_entry(&good, "../evil", b"x");
    assert!(matches!(decode_art_library(&stray), Err(LoadError::UnexpectedEntry(_))));
    let orphan = common::add_entry(&good, "items/99", b"x");
    assert!(matches!(decode_art_library(&orphan), Err(LoadError::UnexpectedEntry(_))), "an item file without a record");
    let bad_group = common::rewrite_json(&good, "library.json", |v| v["items"][0]["group"] = "Onbekend".into());
    assert!(matches!(decode_art_library(&bad_group), Err(LoadError::InvalidValue { .. })));
    let dup = common::rewrite_json(&good, "library.json", |v| v["items"][1]["id"] = v["items"][0]["id"].clone());
    assert!(matches!(decode_art_library(&dup), Err(LoadError::InvalidValue { .. })));
    let unknown = common::rewrite_json(&good, "library.json", |v| v["extra"] = 1.into());
    assert!(matches!(decode_art_library(&unknown), Err(LoadError::UnknownField { .. })));
    assert!(decode_art_library(b"PK nonsense").is_err() && decode_art_library(b"").is_err());
}

#[test]
fn a_subset_keeps_ids_groups_of_the_chosen_items_only() {
    let lib = library();
    let boxx = lib.items()[1].id();
    let sub = lib.subset(&[boxx]);
    assert_eq!(sub.items().len(), 1);
    assert_eq!(sub.items()[0].id(), boxx);
    assert_eq!(sub.groups(), ["Verpakking"], "only groups that still hold an item");
    assert_eq!(sub.id(), lib.id());
}
