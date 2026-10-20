//! A tiny single-stroke digit font for test labels. Seven-segment shapes in a box `0.5 h` wide
//! and `h` high, `0.8 h` per character, so a label is plain vector lines that any Line
//! operation can cut. It needs no font files and has no outlines to fill.

use burnburn_geometry::{Path, PathBuilder, Point};

const WIDTH: f64 = 0.5;
const ADVANCE: f64 = 0.8;
const DOT_WIDTH: f64 = 0.1;
const DOT_ADVANCE: f64 = 0.3;

/// Segments `a..g` as `(x0, y0, x1, y1)` in glyph units (`y` up, `0..1`).
const SEGMENTS: [(f64, f64, f64, f64); 7] = [
    (0.0, 1.0, WIDTH, 1.0),
    (WIDTH, 1.0, WIDTH, 0.5),
    (WIDTH, 0.5, WIDTH, 0.0),
    (0.0, 0.0, WIDTH, 0.0),
    (0.0, 0.5, 0.0, 0.0),
    (0.0, 1.0, 0.0, 0.5),
    (0.0, 0.5, WIDTH, 0.5),
];

/// Which of the segments `a..g` (bit 0 = a) each digit uses.
fn digit_segments(d: char) -> Option<u8> {
    let s = |letters: &str| letters.bytes().fold(0u8, |m, b| m | 1 << (b - b'a'));
    Some(match d {
        '0' => s("abcdef"),
        '1' => s("bc"),
        '2' => s("abged"),
        '3' => s("abgcd"),
        '4' => s("fgbc"),
        '5' => s("afgcd"),
        '6' => s("afgedc"),
        '7' => s("abc"),
        '8' => s("abcdefg"),
        '9' => s("abcdfg"),
        _ => return None,
    })
}

/// Width of `text` at character height `height_mm`; characters outside `0-9 . -` count as a space.
pub(crate) fn text_width(text: &str, height_mm: f64) -> f64 {
    let mut advance = 0.0;
    let mut last_width = 0.0;
    for c in text.chars() {
        let (a, w) = match c {
            '.' => (DOT_ADVANCE, DOT_WIDTH),
            _ => (ADVANCE, WIDTH),
        };
        advance += a;
        last_width = w;
    }
    if text.is_empty() {
        0.0
    } else {
        (advance - (if text.ends_with('.') { DOT_ADVANCE } else { ADVANCE }) + last_width) * height_mm
    }
}

/// The strokes of `text` with its bottom-left corner at `(x, y)`: one open subpath per stroke.
/// `None` when the text has no stroke at all.
pub(crate) fn text_path(text: &str, x: f64, y: f64, height_mm: f64) -> Option<Path> {
    let mut b = PathBuilder::new();
    let mut strokes = 0usize;
    let mut cursor = 0.0;
    let mut stroke = |b: PathBuilder, x0: f64, y0: f64, x1: f64, y1: f64| {
        strokes += 1;
        b.move_to(Point::new(x + x0 * height_mm, y + y0 * height_mm))
            .line_to(Point::new(x + x1 * height_mm, y + y1 * height_mm))
    };
    for c in text.chars() {
        match c {
            '.' => {
                b = stroke(b, cursor, 0.0, cursor + DOT_WIDTH, 0.0);
                cursor += DOT_ADVANCE;
            }
            '-' => {
                b = stroke(b, cursor, 0.5, cursor + WIDTH, 0.5);
                cursor += ADVANCE;
            }
            _ => {
                if let Some(mask) = digit_segments(c) {
                    for (i, (x0, y0, x1, y1)) in SEGMENTS.iter().enumerate() {
                        if mask & (1 << i) != 0 {
                            b = stroke(b, cursor + x0, *y0, cursor + x1, *y1);
                        }
                    }
                }
                cursor += ADVANCE;
            }
        }
    }
    if strokes == 0 { None } else { b.build().ok() }
}

/// A value as label text: at most three decimals, no trailing zeros.
pub(crate) fn format_value(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" { "0".into() } else { s.to_owned() }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn digits_are_seven_segment_strokes_inside_their_box() {
        let strokes = |t: &str| text_path(t, 0.0, 0.0, 3.0).map_or(0, |p| p.subpaths().len());
        assert_eq!(
            [strokes("8"), strokes("1"), strokes("7"), strokes("0"), strokes("-"), strokes(".")],
            [7, 2, 3, 6, 1, 1]
        );
        let b = text_path("8", 5.0, 7.0, 3.0).unwrap().bounds().unwrap();
        assert!((b.min().x - 5.0).abs() < 1e-12 && (b.min().y - 7.0).abs() < 1e-12);
        assert!((b.max().x - 6.5).abs() < 1e-12 && (b.max().y - 10.0).abs() < 1e-12, "0.5 h wide, h high");
        assert!(text_path("", 0.0, 0.0, 3.0).is_none());
        assert!(text_path("x", 0.0, 0.0, 3.0).is_none(), "unknown characters leave a gap, no stroke");
    }

    #[test]
    fn text_width_matches_the_drawn_extent() {
        for t in ["100", "12.5", "0.05", "-3", "7"] {
            let b = text_path(t, 0.0, 0.0, 3.0).unwrap().bounds().unwrap();
            assert!((text_width(t, 3.0) - b.max().x).abs() < 1e-12, "{t}: {} vs {}", text_width(t, 3.0), b.max().x);
        }
    }

    #[test]
    fn values_print_without_trailing_zeros() {
        assert_eq!(
            [100.0, 0.05, 12.5, 0.0, -0.5, 1.0 / 3.0].map(format_value),
            ["100", "0.05", "12.5", "0", "-0.5", "0.333"]
        );
    }
}
