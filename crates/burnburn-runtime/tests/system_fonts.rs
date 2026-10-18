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
