//! G9-trace, text: justified text (`Align::Justify`). Every line but the last is stretched to the width of
//! the widest line, by the gaps between words, and in Arabic first by kashida (tatweel) at the joins
//! where a stretch is allowed. The oracle is arithmetic on the shaped lines (widths, pen positions,
//! glyph counts) with DejaVu Sans, whose Arabic glyph ids are in `arabic.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use burnburn_test_support::fixtures::read_fixture;
use burnburn_text::{Align, FontData, FontRef, FontSize, FontStyle, MemoryFontSource, ShapedText, TextObject, shape};
use burnburn_types::Length;

const BEH: &str = "\u{628}";
const LAM: &str = "\u{644}";
const ALEF: &str = "\u{627}";

fn setup() -> (MemoryFontSource, FontRef) {
    let mut src = MemoryFontSource::new();
    let data = Arc::new(FontData::from_bytes(read_fixture("fonts", "DejaVuSans.ttf").unwrap()).unwrap());
    let r = src.insert("DejaVu Sans", FontStyle::Regular, data);
    (src, r)
}

fn shaped(content: &str, align: Align) -> ShapedText {
    let (src, font) = setup();
    let mut obj = TextObject::new(content, font, FontSize::from_mm(Length::from_mm(10.0).unwrap()).unwrap());
    obj.layout.align = align;
    let s = shape(&obj, &src).unwrap();
    assert!(s.missing.is_empty(), "{:?}", s.missing);
    s
}

/// Right edge (pen after the last glyph) of every line, from the glyphs themselves.
fn right_edges(s: &ShapedText) -> Vec<f64> {
    (0..s.lines.len())
        .map(|line| {
            s.glyphs
                .iter()
                .filter(|g| g.line == line)
                .map(|g| g.origin.x + g.advance_mm)
                .fold(f64::NEG_INFINITY, f64::max)
        })
        .collect()
}

fn glyph_count(s: &ShapedText, line: usize) -> usize {
    s.glyphs.iter().filter(|g| g.line == line).count()
}

const LATIN: &str = "the quick brown fox\nsmall words\nxyz\nlast";

#[test]
fn every_line_but_the_last_is_stretched_to_the_widest_line() {
    let natural = shaped(LATIN, Align::Left);
    let widest = natural.lines.iter().map(|l| l.width_mm).fold(0.0, f64::max);
    let justified = shaped(LATIN, Align::Justify);
    let edges = right_edges(&justified);
    for line in 0..3 {
        assert!((edges[line] - widest).abs() < 1e-6, "line {line}: right edge {} for {widest}", edges[line]);
        assert!((justified.lines[line].width_mm - widest).abs() < 1e-6);
        assert_eq!(justified.lines[line].start_x, 0.0);
    }
    // The last line keeps its natural width.
    assert!((edges[3] - natural.lines[3].width_mm).abs() < 1e-9);
}

#[test]
fn the_extra_width_goes_between_the_words_and_leaves_each_word_alone() {
    let natural = shaped(LATIN, Align::Left);
    let justified = shaped(LATIN, Align::Justify);
    // Line 1, "small words": the letters of "small" (5 glyphs) keep their pen distances.
    let of = |s: &ShapedText| s.glyphs.iter().filter(|g| g.line == 1).map(|g| g.origin.x).collect::<Vec<_>>();
    let (n, j) = (of(&natural), of(&justified));
    assert_eq!(n.len(), j.len(), "no glyph is added to a Latin line");
    for i in 1..5 {
        assert!(((j[i] - j[0]) - (n[i] - n[0])).abs() < 1e-9, "inside the first word");
    }
    for i in 6..n.len() {
        assert!(((j[i] - j[6]) - (n[i] - n[6])).abs() < 1e-9, "inside the second word");
    }
    assert!(j[6] - n[6] > 0.5, "the second word moved right: {}", j[6] - n[6]);
}

#[test]
fn a_line_without_a_gap_is_spread_over_its_letters() {
    let natural = shaped(LATIN, Align::Left);
    let widest = natural.lines.iter().map(|l| l.width_mm).fold(0.0, f64::max);
    let justified = shaped(LATIN, Align::Justify);
    // "xyz": no space, so the letter spacing takes the stretch, equally.
    let glyphs: Vec<_> = justified.glyphs.iter().filter(|g| g.line == 2).collect();
    let gaps: Vec<f64> = glyphs.windows(2).map(|w| w[1].origin.x - w[0].origin.x - w[0].advance_mm).collect();
    assert!((gaps[0] - gaps[1]).abs() < 1e-9 && gaps[0] > 1.0, "{gaps:?}");
    assert!((right_edges(&justified)[2] - widest).abs() < 1e-6);
}

#[test]
fn one_line_and_the_last_line_are_not_justified() {
    for content in ["alone", "short\nanother one that is longer"] {
        let left = shaped(content, Align::Left);
        let justified = shaped(content, Align::Justify);
        let last = left.lines.len() - 1;
        assert_eq!(right_edges(&left)[last], right_edges(&justified)[last], "{content:?}");
        assert_eq!(
            left.glyphs.iter().filter(|g| g.line == last).map(|g| g.origin.x).collect::<Vec<_>>(),
            justified.glyphs.iter().filter(|g| g.line == last).map(|g| g.origin.x).collect::<Vec<_>>()
        );
    }
}

// ---- kashida ----

fn arabic_word(n: usize) -> String {
    BEH.repeat(n)
}

fn arabic_lines() -> String {
    // Line 0 is the widest; line 1 is two short words (spaces and joins to stretch); line 2 is the last.
    format!("{} {} {}\n{} {}\n{}", arabic_word(5), arabic_word(5), arabic_word(5), arabic_word(3), arabic_word(3), BEH)
}

#[test]
fn an_arabic_line_is_stretched_by_kashida_first_and_the_words_stay_connected() {
    let natural = shaped(&arabic_lines(), Align::Left);
    let widest = natural.lines.iter().map(|l| l.width_mm).fold(0.0, f64::max);
    let justified = shaped(&arabic_lines(), Align::Justify);
    assert!(glyph_count(&justified, 1) > glyph_count(&natural, 1), "tatweels were added to line 1");
    assert!((right_edges(&justified)[1] - widest).abs() < 1e-6);
    // The kashida took part of the stretch: the word gap grew by less than the whole stretch.
    let gap_growth = {
        let x = |s: &ShapedText| {
            let mut xs: Vec<f64> = s.glyphs.iter().filter(|g| g.line == 1).map(|g| g.origin.x).collect();
            xs.sort_by(f64::total_cmp);
            xs
        };
        let (n, j) = (x(&natural), x(&justified));
        // The space is the glyph with the widest pen jump at the middle of the line (3 beh + space + 3 beh).
        let mid = n.len() / 2;
        (j[mid + 1] - j[mid]) - (n[mid + 1] - n[mid])
    };
    let stretch = widest - natural.lines[1].width_mm;
    assert!(gap_growth < stretch - 0.5, "gap grew {gap_growth} of {stretch}");
    // Every glyph's cluster is a character boundary of the content (added tatweels point into the text).
    let content = arabic_lines();
    assert!(justified.glyphs.iter().all(|g| content.is_char_boundary(g.cluster)));
    // The last line is left as it was.
    assert!((right_edges(&justified)[2] - natural.lines[2].width_mm).abs() < 1e-9);
}

#[test]
fn kashida_never_splits_the_lam_alef_ligature_and_never_enters_a_letter_that_does_not_join() {
    // "لا" is one ligature glyph; alef joins nothing after it.
    let line = format!("{LAM}{ALEF} {LAM}{ALEF} {LAM}{ALEF}");
    let content = format!("{} {} {}\n{line} {LAM}{ALEF}\nx", arabic_word(6), arabic_word(6), arabic_word(6));
    let natural = shaped(&content, Align::Left);
    let justified = shaped(&content, Align::Justify);
    assert_eq!(glyph_count(&justified, 1), glyph_count(&natural, 1), "nothing to stretch but the spaces");
    let widest = natural.lines.iter().map(|l| l.width_mm).fold(0.0, f64::max);
    assert!((right_edges(&justified)[1] - widest).abs() < 1e-6, "the spaces took it all");
}

#[test]
fn an_arabic_word_without_gap_or_join_stays_as_it_is() {
    // Alefs join nothing and a single word has no gap: there is nothing to stretch honestly, and letter
    // spacing would tear a cursive word apart.
    let content = format!("{} {}\n{}\nz", arabic_word(6), arabic_word(6), ALEF.repeat(3));
    let plain = shaped(&content, Align::Left);
    let justified = shaped(&content, Align::Justify);
    assert!((justified.lines[1].width_mm - plain.lines[1].width_mm).abs() < 1e-9);
    assert_eq!(glyph_count(&justified, 1), glyph_count(&plain, 1));
}
