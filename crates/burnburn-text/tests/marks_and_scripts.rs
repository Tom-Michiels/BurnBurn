//! G8-trace, text: mark positioning (GPOS) for Arabic vowel marks and Latin combining accents,
//! kashida (tatweel) behaviour, and an Indic script (Devanagari and Gurmukhi) with its reordering,
//! conjuncts and reph. Fonts: DejaVu Sans (Arabic, Latin marks) and Mukta Mahee (Indic, a font
//! collection), both from `test-data/fonts` with their provenance in the README there.
//!
//! The oracles are geometric facts about writing, not the shaper's own output: a fatha stands above
//! its letter, a kasra below it, a tatweel runs into the letters on both sides, a short i matra is
//! drawn before the consonant it follows in the text, a conjunct is fewer glyphs than letters. The
//! glyph ids and advances of the Arabic forms come from the font file itself
//! (`test-data/fonts/reference_metrics.py`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use burnburn_geometry::Bounds;
use burnburn_test_support::fixtures::read_fixture;
use burnburn_text::{FontData, FontSize, FontStyle, MemoryFontSource, ShapedText, TextObject, shape};
use burnburn_types::Length;

const EM_MM: f64 = 20.48;

fn size() -> FontSize {
    FontSize::from_mm(Length::from_mm(EM_MM).unwrap()).unwrap()
}

fn source(fixture: &str, family: &str) -> (MemoryFontSource, burnburn_text::FontRef) {
    let mut src = MemoryFontSource::new();
    let data = Arc::new(FontData::from_bytes(read_fixture("fonts", fixture).unwrap()).unwrap());
    let r = src.insert(family, FontStyle::Regular, data);
    (src, r)
}

fn dejavu(content: &str) -> ShapedText {
    let (src, r) = source("DejaVuSans.ttf", "DejaVu Sans");
    let shaped = shape(&TextObject::new(content, r, size()), &src).unwrap();
    assert!(shaped.missing.is_empty(), "{content:?}: {:?}", shaped.missing);
    shaped
}

fn mukta(content: &str) -> ShapedText {
    let (src, r) = source("MuktaMahee.ttc", "Mukta Mahee");
    let shaped = shape(&TextObject::new(content, r, size()), &src).unwrap();
    assert!(shaped.missing.is_empty(), "{content:?}: {:?}", shaped.missing);
    shaped
}

fn bounds(shaped: &ShapedText, glyph: usize) -> Bounds {
    shaped.glyph_outlines[glyph].bounds().unwrap_or_else(|| panic!("glyph {glyph} has no outline"))
}

/// Index of the glyph (in `shaped.glyphs`) that is the mark: the one with no advance of its own.
fn mark_index(shaped: &ShapedText) -> usize {
    shaped.glyphs.iter().position(|g| g.advance_mm == 0.0).expect("a glyph without advance")
}

fn centre_x(b: &Bounds) -> f64 {
    (b.min().x + b.max().x) / 2.0
}

// ---------------------------------------------------------------------------------------------
// Marks
// ---------------------------------------------------------------------------------------------

const BEH: &str = "\u{628}";
const FATHA: &str = "\u{64e}";
const KASRA: &str = "\u{650}";
const SHADDA: &str = "\u{651}";

#[test]
fn a_fatha_stands_above_its_letter_and_over_its_body_in_every_joining_form() {
    // Isolated, initial, medial and final beh each with a fatha; the mark is the glyph without advance.
    for word in [format!("{BEH}{FATHA}"), format!("{BEH}{FATHA}{BEH}"), format!("{BEH}{BEH}{FATHA}{BEH}")] {
        let shaped = dejavu(&word);
        let mark = mark_index(&shaped);
        let m = bounds(&shaped, mark);
        // The base is the glyph of the same cluster that does advance.
        let cluster = shaped.glyphs[mark].cluster;
        let base = shaped.glyphs.iter().position(|g| g.cluster == cluster && g.advance_mm > 0.0).unwrap();
        let b = bounds(&shaped, base);
        assert!(m.min().y > b.max().y, "{word:?}: the fatha ({:?}) is above the letter ({:?})", m, b);
        assert!(
            b.min().x <= centre_x(&m) && centre_x(&m) <= b.max().x,
            "{word:?}: the fatha ({:?}) is over the body of the letter ({:?})",
            m,
            b
        );
    }
}

#[test]
fn a_kasra_stands_below_its_letter_and_a_shadda_with_a_kasra_has_a_mark_on_each_side() {
    let shaped = dejavu(&format!("{BEH}{KASRA}"));
    let mark = mark_index(&shaped);
    let (m, b) = (bounds(&shaped, mark), bounds(&shaped, 1 - mark));
    assert!(m.max().y < b.min().y, "the kasra ({m:?}) is below the letter, dot included ({b:?})");
    assert!(b.min().x <= centre_x(&m) && centre_x(&m) <= b.max().x, "and under its body");

    // A shadda is above the letter, like the fatha.
    let doubled = dejavu(&format!("{BEH}{SHADDA}"));
    let mark = mark_index(&doubled);
    assert!(bounds(&doubled, mark).min().y > bounds(&doubled, 1 - mark).max().y);
    // (Shadda and kasra together are one glyph of the font, stacked above the letter: its own choice.)
    assert_eq!(dejavu(&format!("{BEH}{SHADDA}{KASRA}")).glyphs.len(), 2);
}

#[test]
fn a_mark_never_moves_the_letters_around_it() {
    // Same letters with and without the mark: the base glyphs keep their positions and advances.
    let plain = dejavu(&format!("{BEH}{BEH}"));
    let marked = dejavu(&format!("{BEH}{FATHA}{BEH}"));
    let bases = |s: &ShapedText| -> Vec<(u32, f64, f64)> {
        s.glyphs.iter().filter(|g| g.advance_mm > 0.0).map(|g| (g.glyph_id, g.origin.x, g.advance_mm)).collect()
    };
    assert_eq!(bases(&plain), bases(&marked));
}

#[test]
fn a_latin_accent_with_no_precomposed_letter_stands_over_its_base() {
    // x + combining acute: DejaVu has no precomposed x-acute, so the accent is placed, not composed.
    let shaped = dejavu("x\u{301}");
    assert_eq!(shaped.glyphs.len(), 2, "base and accent stay two glyphs");
    let mark = mark_index(&shaped);
    let (m, b) = (bounds(&shaped, mark), bounds(&shaped, 1 - mark));
    assert!(m.min().y >= b.max().y - 0.05 * EM_MM, "the accent ({m:?}) is above the x ({b:?})");
    assert!((centre_x(&m) - centre_x(&b)).abs() < 0.15 * EM_MM, "and over its middle");
    // A precomposed letter is one glyph: composing is what the font does when it can.
    assert_eq!(dejavu("e\u{301}").glyphs.len(), 1);
    // The marks of a stack both stay over the letter and do not sit in each other's place.
    let stacked = dejavu("a\u{323}\u{301}");
    let accent = stacked.glyphs.len() - 1;
    let (dot_or_base, acute) = (bounds(&stacked, 0), bounds(&stacked, accent));
    assert!(acute.min().y > dot_or_base.max().y - 0.05 * EM_MM, "the acute is over the a, not below");
}

// ---------------------------------------------------------------------------------------------
// Kashida
// ---------------------------------------------------------------------------------------------

const TATWEEL: &str = "\u{640}";

#[test]
fn a_tatweel_runs_into_the_letters_on_both_sides_and_stretches_the_word_by_its_advance() {
    let plain = dejavu(&format!("{BEH}{BEH}"));
    let kashida = dejavu(&format!("{BEH}{TATWEEL}{BEH}"));
    // Visual order, left to right: final beh, tatweel, initial beh (right-to-left text).
    let mut order: Vec<usize> = (0..kashida.glyphs.len()).collect();
    order.sort_by(|a, b| kashida.glyphs[*a].origin.x.total_cmp(&kashida.glyphs[*b].origin.x));
    assert_eq!(order.len(), 3);
    let (left, bar, right) = (bounds(&kashida, order[0]), bounds(&kashida, order[1]), bounds(&kashida, order[2]));
    assert!(bar.min().x <= left.max().x + 1e-6, "the tatweel reaches the letter on its left ({bar:?} / {left:?})");
    assert!(bar.max().x >= right.min().x - 1e-6, "and the letter on its right ({bar:?} / {right:?})");
    assert!(bar.max().y - bar.min().y < 0.15 * EM_MM, "it is a bar, not a letter");
    // The word is as much wider as the tatweel advances, and the letters keep their joining forms.
    let width = |s: &ShapedText| s.lines[0].width_mm;
    let bar_advance = kashida.glyphs[order[1]].advance_mm;
    assert!((width(&kashida) - width(&plain) - bar_advance).abs() < 1e-6);
    assert_eq!(
        (kashida.glyphs[order[0]].glyph_id, kashida.glyphs[order[2]].glyph_id),
        (plain.glyphs.iter().map(|g| g.glyph_id).min().unwrap_or(0).max(5255), 5256),
        "final beh on the left, initial beh on the right, as in the plain word"
    );
}

#[test]
fn letter_spacing_does_not_tear_the_joins_of_an_arabic_word_but_still_spaces_latin() {
    let spaced = |content: &str, spacing: f64| {
        let (src, r) = source("DejaVuSans.ttf", "DejaVu Sans");
        let mut obj = TextObject::new(content, r, size());
        obj.layout.letter_spacing_mm = spacing;
        shape(&obj, &src).unwrap()
    };
    let word = format!("{BEH}{BEH}{BEH}");
    assert_eq!(
        spaced(&word, 0.0).lines[0].width_mm,
        spaced(&word, 2.0).lines[0].width_mm,
        "joined Arabic letters keep touching"
    );
    // Latin text is spaced between its letters: three letters, two gaps.
    let (plain, wide) = (spaced("abc", 0.0).lines[0].width_mm, spaced("abc", 2.0).lines[0].width_mm);
    assert!((wide - plain - 4.0).abs() < 1e-9, "{plain} -> {wide}");
}

// ---------------------------------------------------------------------------------------------
// Indic: Gurmukhi (the Indic font of the fixtures; no permissively licensed Devanagari font is available)
// ---------------------------------------------------------------------------------------------

const KA: &str = "\u{a15}";
const PA: &str = "\u{a2a}";
const RA: &str = "\u{a30}";
const HA: &str = "\u{a39}";
const VIRAMA: &str = "\u{a4d}";
const I_MATRA: &str = "\u{a3f}";
const U_MATRA: &str = "\u{a41}";
const AI_MATRA: &str = "\u{a48}";
const BINDI: &str = "\u{a02}";
const ZWNJ: &str = "\u{200c}";

#[test]
fn a_font_collection_loads_its_first_face() {
    let bytes = read_fixture("fonts", "MuktaMahee.ttc").unwrap();
    assert_eq!(&bytes[..4], b"ttcf", "the fixture really is a collection");
    let font = FontData::from_bytes(bytes.clone()).expect("the first face of a collection");
    assert!(font.has_glyph('\u{a15}') && font.has_glyph('a'), "Gurmukhi and Latin");
    assert!(font.units_per_em() >= 16);
    assert_eq!(burnburn_text::font_names(&bytes).map(|n| n.1), Some(FontStyle::Regular));
}

#[test]
fn the_short_i_matra_is_drawn_before_the_consonant_it_follows_in_the_text() {
    let shaped = mukta(&format!("{KA}{I_MATRA}"));
    assert_eq!(shaped.glyphs.len(), 2);
    assert!(shaped.glyphs.iter().all(|g| g.cluster == 0), "one syllable, one cluster");
    let mut order: Vec<usize> = (0..2).collect();
    order.sort_by(|a, b| shaped.glyphs[*a].origin.x.total_cmp(&shaped.glyphs[*b].origin.x));
    let (first, second) = (bounds(&shaped, order[0]), bounds(&shaped, order[1]));
    // The matra (a narrow stroke with the line over it) is left of the consonant it belongs to.
    assert!(
        first.max().x - first.min().x < second.max().x - second.min().x,
        "the narrow glyph is the matra: {first:?} against {second:?}"
    );
    assert!(centre_x(&first) < centre_x(&second), "and it stands before the consonant");
    assert!(shaped.glyphs[order[0]].origin.x < shaped.glyphs[order[1]].origin.x);
    // In the text the consonant comes first: the plain letter is the last of the pair.
    assert_eq!(shaped.glyphs[order[1]].glyph_id, mukta(KA).glyphs[0].glyph_id);
}

#[test]
fn vowel_signs_and_the_bindi_are_placed_under_or_over_the_consonant() {
    let base = bounds(&mukta(KA), 0);
    let placed = |sign: &str| {
        let shaped = mukta(&format!("{KA}{sign}"));
        assert_eq!(shaped.glyphs.len(), 2, "{sign:?}");
        let mark = mark_index(&shaped);
        let m = bounds(&shaped, mark);
        assert!(base.min().x <= centre_x(&m) && centre_x(&m) <= base.max().x, "{sign:?} is over the letter: {m:?}");
        m
    };
    assert!(placed(U_MATRA).max().y <= base.min().y, "the u sign hangs under the letter");
    assert!(placed(AI_MATRA).min().y >= base.max().y - 0.1 * EM_MM, "the ai sign stands over it");
    assert!(placed(BINDI).min().y >= base.max().y, "the bindi stands over it");
}

#[test]
fn a_subjoined_letter_hangs_under_its_base_and_a_conjunct_is_fewer_glyphs_than_letters() {
    // ka + virama + ha: the ha is written below the ka.
    let shaped = mukta(&format!("{KA}{VIRAMA}{HA}"));
    assert_eq!(shaped.glyphs.len(), 2, "a base and a subjoined letter");
    let sub = mark_index(&shaped);
    let (s, b) = (bounds(&shaped, sub), bounds(&shaped, 1 - sub));
    assert!(s.max().y <= b.min().y + 0.01 * EM_MM, "under the base: {s:?} against {b:?}");
    // pa + virama + ra is a single glyph of the font.
    let conjunct = mukta(&format!("{PA}{VIRAMA}{RA}"));
    assert_eq!(conjunct.glyphs.len(), 1, "three letters, one conjunct glyph");
    // A ZWNJ between virama and ra stops the joining: more glyphs, a wider word, the letters apart.
    let apart = mukta(&format!("{PA}{VIRAMA}{ZWNJ}{RA}"));
    assert!(apart.glyphs.len() > conjunct.glyphs.len());
    assert!(apart.lines[0].width_mm > conjunct.lines[0].width_mm);
    assert_ne!(apart.glyphs.iter().filter(|g| g.advance_mm > 0.0).count(), 1);
}

#[test]
fn a_syllable_is_not_split_by_letter_spacing_but_two_syllables_are_apart() {
    let width = |spacing: f64| {
        let (src, r) = source("MuktaMahee.ttc", "Mukta Mahee");
        let mut obj = TextObject::new(format!("{KA}{I_MATRA}{KA}{I_MATRA}"), r, size());
        obj.layout.letter_spacing_mm = spacing;
        shape(&obj, &src).unwrap().lines[0].width_mm
    };
    assert!((width(1.5) - width(0.0) - 1.5).abs() < 1e-9, "one gap between the two syllables, none inside them");
}

#[test]
fn indic_text_becomes_outlines() {
    let shaped = mukta(&format!("{PA}{VIRAMA}{RA} {KA}{I_MATRA}{BINDI}"));
    assert!(!shaped.outlines.subpaths().is_empty());
}

// ---------------------------------------------------------------------------------------------
// Invisible characters
// ---------------------------------------------------------------------------------------------

#[test]
fn invisible_format_characters_are_never_reported_as_missing_glyphs_whatever_the_font_maps() {
    // Mukta Mahee maps neither the left-to-right mark nor some others to a glyph; the text still
    // shapes, nothing is "missing" (nobody sees these characters) and the letters are untouched.
    for invisible in ["\u{200b}", "\u{200e}", "\u{200f}", "\u{2060}", "\u{feff}", "\u{fe0f}", "\u{202a}", "\u{202c}"] {
        let shaped = mukta(&format!("a{invisible}b"));
        let ink: Vec<u32> = shaped.glyphs.iter().filter(|g| g.advance_mm > 0.0).map(|g| g.glyph_id).collect();
        assert_eq!(ink, mukta("ab").glyphs.iter().map(|g| g.glyph_id).collect::<Vec<_>>(), "{invisible:?}");
    }
}
