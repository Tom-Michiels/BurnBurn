//! Bidirectional layout (Unicode Bidirectional Algorithm, UAX #9) against hand-derived visual
//! orders. The Liberation Sans fixture carries the Hebrew letters; Hebrew shin, lamed, vav and
//! final mem are U+05E9, U+05DC, U+05D5, U+05DD.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use burnburn_test_support::fixtures::read_fixture;
use burnburn_text::{FontData, FontRef, FontSize, FontStyle, MemoryFontSource, PlacedGlyph, TextObject, shape};
use burnburn_types::Length;

fn setup() -> (MemoryFontSource, FontRef) {
    let mut src = MemoryFontSource::new();
    let data = Arc::new(FontData::from_bytes(read_fixture("fonts", "LiberationSans-Regular.ttf").unwrap()).unwrap());
    let r = src.insert("Liberation Sans", FontStyle::Regular, data);
    (src, r)
}

fn size() -> FontSize {
    FontSize::from_mm(Length::from_mm(20.48).unwrap()).unwrap()
}

fn glyphs(content: &str) -> Vec<PlacedGlyph> {
    let (src, r) = setup();
    let shaped = shape(&TextObject::new(content, r, size()), &src).unwrap();
    assert!(shaped.missing.is_empty(), "{content:?}: {:?}", shaped.missing);
    shaped.glyphs
}

/// Byte offsets of the clusters from left to right.
fn visual_clusters(content: &str) -> Vec<usize> {
    let mut g = glyphs(content);
    g.sort_by(|a, b| a.origin.x.total_cmp(&b.origin.x));
    g.iter().map(|g| g.cluster).collect()
}

const SHIN: &str = "\u{5e9}";
const LAMED: &str = "\u{5dc}";
const VAV: &str = "\u{5d5}";
const MEM: &str = "\u{5dd}";

#[test]
fn a_hebrew_word_reads_from_right_to_left() {
    let word = format!("{SHIN}{LAMED}{VAV}{MEM}");
    // Each Hebrew letter is two bytes: logical clusters 0, 2, 4, 6; visually reversed.
    assert_eq!(visual_clusters(&word), [6, 4, 2, 0]);
}

#[test]
fn latin_runs_keep_their_order_around_a_hebrew_run() {
    // "ab SHIN LAMED cd": the paragraph is left-to-right (first strong is Latin); the Hebrew
    // pair is reversed in place between the two Latin words.
    let content = format!("ab {SHIN}{LAMED} cd");
    // bytes: a0 b1 ' '2 SHIN3-4 LAMED5-6 ' '7 c8 d9
    assert_eq!(visual_clusters(&content), [0, 1, 2, 5, 3, 7, 8, 9]);
}

#[test]
fn numbers_inside_right_to_left_text_stay_left_to_right() {
    // Right-to-left paragraph (first strong is Hebrew): "SHIN LAMED space 1 2".
    // Visual order, left to right: the number "12" (kept 1 then 2), space, LAMED, SHIN.
    let content = format!("{SHIN}{LAMED} 12");
    // bytes: SHIN0-1 LAMED2-3 ' '4 '1'5 '2'6
    assert_eq!(visual_clusters(&content), [5, 6, 4, 2, 0]);
}

#[test]
fn pure_left_to_right_text_is_unchanged() {
    assert_eq!(visual_clusters("Hello"), [0, 1, 2, 3, 4]);
}

#[test]
fn the_width_of_a_hebrew_word_is_the_sum_of_its_glyph_advances() {
    let (src, r) = setup();
    let shaped = shape(&TextObject::new(format!("{SHIN}{LAMED}{VAV}{MEM}"), r, size()), &src).unwrap();
    let sum: f64 = shaped.glyphs.iter().map(|g| g.advance_mm).sum();
    assert!((shaped.lines[0].width_mm - sum).abs() < 1e-9);
    // Glyphs sit side by side without overlap or gap: each pen starts where the previous ends.
    let mut g = shaped.glyphs.clone();
    g.sort_by(|a, b| a.origin.x.total_cmp(&b.origin.x));
    for pair in g.windows(2) {
        assert!((pair[0].origin.x + pair[0].advance_mm - pair[1].origin.x).abs() < 1e-9);
    }
}

#[test]
fn a_hebrew_line_in_a_multi_line_text_is_laid_out_per_line() {
    let content = format!("ab\n{SHIN}{LAMED}");
    let g = glyphs(&content);
    let second: Vec<_> = g.iter().filter(|g| g.line == 1).collect();
    let shin = second.iter().find(|g| g.cluster == 3).unwrap();
    let lamed = second.iter().find(|g| g.cluster == 5).unwrap();
    assert!(shin.origin.x > lamed.origin.x);
    let first: Vec<_> = g.iter().filter(|g| g.line == 0).collect();
    assert!(first[0].origin.x < first[1].origin.x);
}

#[test]
fn a_missing_character_inside_right_to_left_text_is_still_reported_in_logical_order() {
    let (src, r) = setup();
    // U+65E5 is not in the font; it sits between two Hebrew letters.
    let shaped = shape(&TextObject::new(format!("{SHIN}\u{65e5}{LAMED}\u{65e5}"), r, size()), &src).unwrap();
    let bytes: Vec<usize> = shaped.missing.iter().map(|m| m.byte).collect();
    assert_eq!(bytes, [2, 7]);
}
