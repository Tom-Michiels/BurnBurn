//! The system font source for canvas text input: the first readable face of a candidate list
//! (pure std file reads), never a silent substitute for a face that is missing or unreadable.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use burnburn_runtime::fonts::first_readable_font;

fn fonts_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-data/fonts")
}

#[test]
fn the_first_readable_candidate_becomes_the_default_face() {
    let candidates = [
        fonts_dir().join("does-not-exist.ttf"),
        // Readable bytes, but not a font.
        fonts_dir().join("manifest.json"),
        fonts_dir().join("LiberationSans-Regular.ttf"),
        fonts_dir().join("BIZUDPGothic-Regular.ttf"),
    ];
    let font = first_readable_font(&candidates).expect("Liberation Sans is readable");
    assert_eq!(font.font.family, "LiberationSans-Regular");
    let face = font.source.load(&font.font).expect("the source resolves its own reference");
    assert!(face.has_glyph('A') && !face.has_glyph('日'), "it is Liberation Sans, not the next candidate");
}

#[test]
fn no_readable_candidate_is_no_font() {
    let candidates = [fonts_dir().join("does-not-exist.ttf"), fonts_dir().join("manifest.json")];
    assert!(first_readable_font(&candidates).is_none());
    assert!(first_readable_font(&[]).is_none());
}

// ---------------------------------------------------------------------------------------------
// The catalogue (W3-UI-tekst): the text toolbar's font choice
// ---------------------------------------------------------------------------------------------

use std::sync::Arc;

use burnburn_runtime::fonts::SystemFontCatalog;
use burnburn_text::{FontError, FontRef, FontSource, FontStyle};

fn catalog_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for name in ["LiberationSans-Regular.ttf", "BIZUDPGothic-Regular.ttf"] {
        std::fs::copy(fonts_dir().join(name), dir.path().join(name)).unwrap();
    }
    // Neither a font nor a font extension: skipped without a word.
    std::fs::copy(fonts_dir().join("manifest.json"), dir.path().join("manifest.ttf")).unwrap();
    std::fs::write(dir.path().join("notes.txt"), b"hello").unwrap();
    dir
}

#[test]
fn the_catalogue_lists_the_families_the_files_declare_sorted_and_skips_what_is_no_font() {
    let dir = catalog_dir();
    let catalog = SystemFontCatalog::new(vec![dir.path().to_path_buf(), dir.path().join("missing")]);
    assert_eq!(catalog.families(), ["BIZ UDPGothic", "Liberation Sans"]);
    assert_eq!(catalog.available_styles("Liberation Sans"), [FontStyle::Regular]);
    assert!(catalog.available_styles("Comic Sans").is_empty());
}

#[test]
fn a_face_is_loaded_by_family_and_style_and_a_missing_one_is_never_replaced() {
    let dir = catalog_dir();
    let catalog = SystemFontCatalog::new(vec![dir.path().to_path_buf()]);
    let face = catalog.load(&FontRef::new("Liberation Sans", FontStyle::Regular)).expect("listed");
    assert!(face.has_glyph('A') && !face.has_glyph('日'), "Liberation Sans, not the next family");
    let again = catalog.load(&FontRef::new("Liberation Sans", FontStyle::Regular)).unwrap();
    assert!(Arc::ptr_eq(&face, &again), "a face is read once");

    // Bold is not in the folder: NotFound, not Regular with a bold label.
    let bold = FontRef::new("Liberation Sans", FontStyle::Bold);
    assert!(matches!(catalog.load(&bold), Err(FontError::NotFound(r)) if r == bold));
    // An asset reference resolves only to a face this catalogue loaded with that hash.
    assert!(
        face.hash()
            == catalog
                .load(&FontRef::new("Liberation Sans", FontStyle::Regular).with_asset(face.hash()))
                .unwrap()
                .hash()
    );
    let stranger = FontRef::new("Liberation Sans", FontStyle::Regular)
        .with_asset(burnburn_types::ContentHash::of(burnburn_types::HashDomain::Asset, b"not a font we loaded"));
    assert!(matches!(catalog.load(&stranger), Err(FontError::NotFound(_))));
}

#[test]
fn the_default_face_prefers_a_common_family_else_the_first_and_an_empty_catalogue_has_none() {
    let dir = catalog_dir();
    let catalog = SystemFontCatalog::new(vec![dir.path().to_path_buf()]);
    // Liberation Sans is on the preferred list, BIZ UDPGothic sorts first but is not.
    assert_eq!(catalog.default_font(), Some(FontRef::new("Liberation Sans", FontStyle::Regular)));
    let empty = tempfile::tempdir().unwrap();
    assert_eq!(SystemFontCatalog::new(vec![empty.path().to_path_buf()]).default_font(), None);
}

// ---------------------------------------------------------------------------------------------
// Font collections (.ttc): every face is listed (G9)
// ---------------------------------------------------------------------------------------------

use burnburn_text::shape;

/// The faces of `MuktaMahee.ttc`, in collection order, as the name table of each face gives them
/// (read from the file with plain `struct` unpacking: family = name ID 1, bold = head.macStyle bit 0).
const MUKTA_FACES: [(&str, FontStyle); 7] = [
    ("MuktaMahee Regular", FontStyle::Regular),
    ("MuktaMahee SemiBold", FontStyle::Regular),
    ("MuktaMahee Medium", FontStyle::Regular),
    ("MuktaMahee Light", FontStyle::Regular),
    ("MuktaMahee ExtraLight", FontStyle::Regular),
    ("MuktaMahee ExtraBold", FontStyle::Regular),
    ("MuktaMahee Bold", FontStyle::Bold),
];

fn collection_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::copy(fonts_dir().join("MuktaMahee.ttc"), dir.path().join("MuktaMahee.ttc")).unwrap();
    dir
}

#[test]
fn a_font_collection_lists_every_face_it_holds() {
    let dir = collection_dir();
    let catalog = SystemFontCatalog::new(vec![dir.path().to_path_buf()]);
    let mut expected: Vec<String> = MUKTA_FACES.iter().map(|(f, _)| (*f).to_owned()).collect();
    expected.sort();
    assert_eq!(catalog.families(), expected);
    for (family, style) in MUKTA_FACES {
        assert_eq!(catalog.available_styles(family), [style], "{family}");
    }
}

#[test]
fn each_face_of_a_collection_loads_as_itself_and_not_as_the_first_face() {
    let dir = collection_dir();
    let catalog = SystemFontCatalog::new(vec![dir.path().to_path_buf()]);
    let faces: Vec<_> =
        MUKTA_FACES.iter().map(|(f, s)| catalog.load(&FontRef::new(*f, *s)).expect("listed face")).collect();
    let hashes: std::collections::BTreeSet<_> = faces.iter().map(|f| f.hash()).collect();
    assert_eq!(hashes.len(), 7, "seven faces, seven asset identities");
    // The face is the one asked for: the weights set different advances (Gurmukhi letters).
    let width = |face: &Arc<burnburn_text::FontData>| -> f64 {
        let mut src = burnburn_text::MemoryFontSource::new();
        let r = src.insert("f", FontStyle::Regular, Arc::clone(face));
        let size = burnburn_text::FontSize::from_mm(burnburn_types::Length::from_mm(10.0).unwrap()).unwrap();
        let obj = burnburn_text::TextObject::new("\u{0A2E}\u{0A15}\u{0A3E}", r, size);
        shape(&obj, &src).unwrap().lines[0].width_mm
    };
    let (regular, extra_light, bold) = (width(&faces[0]), width(&faces[4]), width(&faces[6]));
    assert!(extra_light < regular && regular < bold, "ExtraLight {extra_light} < Regular {regular} < Bold {bold}");
}
