//! Arabic: contextual forms, ligatures and right-to-left order (G7-trace, text). The oracle is the
//! font file itself, read by `test-data/fonts/reference_metrics.py` with plain `struct` (no
//! skrifa, no harfrust): the glyph ids and advances of the Arabic Presentation Forms-B code
//! points, which DejaVu Sans maps to the isolated, initial, medial and final shapes.
//!
//! Arabic letters used: beh U+0628, lam U+0644, alef U+0627, meem U+0645, fatha U+064E.
//! Reference values (units per em 2048, so 1 unit = 0.01 mm at a 20.48 mm em):
//! beh isolated/initial/medial/final = gid 1365/5256/5257/5255 (advance 1928/570/618/2011; the isolated
//! shape is the base cmap glyph, with the same advance and box as U+FE8F);
//! lam-alef ligature (isolated, final) = gid 5362/5363; meem isolated = 1389 (as U+FEE1: advance 1268).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use burnburn_test_support::fixtures::read_fixture;
use burnburn_text::{FontData, FontRef, FontSize, FontStyle, MemoryFontSource, PlacedGlyph, TextObject, shape};
use burnburn_types::Length;

const BEH_ISOLATED: u32 = 1365;
const BEH_FINAL: u32 = 5255;
const BEH_INITIAL: u32 = 5256;
const BEH_MEDIAL: u32 = 5257;
const LAM_ALEF_ISOLATED: u32 = 5362;
const LAM_ALEF_FINAL: u32 = 5363;
const MEEM_ISOLATED: u32 = 1389;

const BEH: &str = "\u{628}";
const LAM: &str = "\u{644}";
const ALEF: &str = "\u{627}";
const MEEM: &str = "\u{645}";
const FATHA: &str = "\u{64e}";

fn setup() -> (MemoryFontSource, FontRef) {
    let mut src = MemoryFontSource::new();
    let data = Arc::new(FontData::from_bytes(read_fixture("fonts", "DejaVuSans.ttf").unwrap()).unwrap());
    let r = src.insert("DejaVu Sans", FontStyle::Regular, data);
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

/// Glyphs from left to right on the line.
fn visual(content: &str) -> Vec<PlacedGlyph> {
    let mut g = glyphs(content);
    g.sort_by(|a, b| a.origin.x.total_cmp(&b.origin.x));
    g
}

fn visual_ids(content: &str) -> Vec<u32> {
    visual(content).iter().map(|g| g.glyph_id).collect()
}

#[test]
fn a_lone_letter_takes_its_isolated_form() {
    assert_eq!(visual_ids(BEH), [BEH_ISOLATED]);
}

#[test]
fn a_letter_takes_the_initial_medial_or_final_form_from_its_neighbours() {
    // Logical order beh beh beh = initial, medial, final; drawn right to left, so the final form
    // is leftmost.
    assert_eq!(visual_ids(&BEH.repeat(3)), [BEH_FINAL, BEH_MEDIAL, BEH_INITIAL]);
    assert_eq!(visual_ids(&BEH.repeat(2)), [BEH_FINAL, BEH_INITIAL]);
}

#[test]
fn lam_followed_by_alef_is_one_ligature_glyph() {
    let g = glyphs(&format!("{LAM}{ALEF}"));
    assert_eq!(g.iter().map(|g| g.glyph_id).collect::<Vec<_>>(), [LAM_ALEF_ISOLATED]);
    assert_eq!(g[0].cluster, 0, "the ligature starts at the lam");
    // After a joining letter the ligature takes its final shape.
    assert_eq!(visual_ids(&format!("{BEH}{LAM}{ALEF}")), [LAM_ALEF_FINAL, BEH_INITIAL]);
}

#[test]
fn alef_does_not_join_the_letter_after_it() {
    // lam alef meem: the ligature closes the word, the meem after it starts afresh (isolated).
    assert_eq!(visual_ids(&format!("{LAM}{ALEF}{MEEM}")), [MEEM_ISOLATED, LAM_ALEF_ISOLATED]);
}

#[test]
fn the_advance_of_a_shaped_word_is_the_sum_of_the_contextual_glyph_advances() {
    // beh final 2011 + beh initial 570 units of 2048 per em at a 20.48 mm em.
    let width: f64 = glyphs(&BEH.repeat(2)).iter().map(|g| g.advance_mm).sum();
    assert!((width - (2011.0 + 570.0) / 100.0).abs() < 1e-6, "{width}");
}

#[test]
fn an_arabic_word_between_latin_words_keeps_the_latin_order() {
    // "ab " + beh beh + " cd": the paragraph is left-to-right; the Arabic run is reversed inside.
    // Byte offsets: a0 b1 space2, beh 3..5, beh 5..7, space7 c8 d9.
    let g = visual(&format!("ab {BEH}{BEH} cd"));
    let clusters: Vec<usize> = g.iter().map(|g| g.cluster).collect();
    assert_eq!(clusters, [0, 1, 2, 5, 3, 7, 8, 9]);
    assert_eq!((g[3].glyph_id, g[4].glyph_id), (BEH_FINAL, BEH_INITIAL));
}

#[test]
fn a_combining_mark_stays_with_its_base_letter() {
    // beh + fatha: the mark has no advance of its own and shares the cluster of the beh.
    let g = glyphs(&format!("{BEH}{FATHA}"));
    assert_eq!(g.len(), 2);
    assert!(g.iter().all(|g| g.cluster == 0), "{:?}", g.iter().map(|g| g.cluster).collect::<Vec<_>>());
    assert!(g.iter().any(|g| g.glyph_id == BEH_ISOLATED));
    assert!(g.iter().any(|g| g.advance_mm == 0.0), "the mark does not advance the pen");
}

#[test]
fn arabic_text_becomes_outlines() {
    let (src, r) = setup();
    let size = FontSize::from_mm(Length::from_mm(10.0).unwrap()).unwrap();
    let shaped = shape(&TextObject::new(format!("{LAM}{ALEF}{MEEM}"), r, size), &src).unwrap();
    assert!(!shaped.outlines.subpaths().is_empty());
}
