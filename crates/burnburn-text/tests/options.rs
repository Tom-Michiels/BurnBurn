//! Text options (W1-L5b): bold/italic face choice, uppercase, vertical alignment, VSpace, welded.
//!
//! Reference numbers from `test-data/fonts/reference_metrics.py` and the hhea table of Liberation
//! Sans (read with `struct`): upem 2048, hhea ascender 1854, descender -434; 'I' bbox
//! (189, 0, 380, 1409), advance 569, a plain rectangle; 'O' has an outer contour and a counter.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test helpers outside #[test] fns

use std::sync::Arc;

use burnburn_geometry::Winding;
use burnburn_test_support::fixtures::read_fixture;
use burnburn_text::{
    FontData, FontError, FontRef, FontSize, FontSource, FontStyle, MemoryFontSource, TextError, TextMode, TextObject,
    VAlign, shape, switch_style,
};
use burnburn_types::Length;

const EPS: f64 = 1e-9;

fn font(name: &str) -> Arc<FontData> {
    Arc::new(FontData::from_bytes(read_fixture("fonts", name).unwrap()).unwrap())
}

fn liberation() -> (MemoryFontSource, FontRef) {
    let mut src = MemoryFontSource::new();
    let r = src.insert("Test Sans", FontStyle::Regular, font("LiberationSans-Regular.ttf"));
    (src, r)
}

/// Em of 20.48 mm: one font unit is exactly 0.01 mm.
fn text(content: &str, font: &FontRef) -> TextObject {
    TextObject::new(content, font.clone(), FontSize::from_mm(Length::from_mm(20.48).unwrap()).unwrap())
}

#[test]
fn bold_and_italic_flags_name_the_four_styles() {
    for (bold, italic, style) in [
        (false, false, FontStyle::Regular),
        (true, false, FontStyle::Bold),
        (false, true, FontStyle::Italic),
        (true, true, FontStyle::BoldItalic),
    ] {
        assert_eq!(FontStyle::from_flags(bold, italic), style);
        assert_eq!((style.is_bold(), style.is_italic()), (bold, italic));
    }
}

#[test]
fn a_style_switch_names_another_face_and_drops_the_old_embedded_asset() {
    let (_, regular) = liberation();
    let embedded = regular.clone().with_asset(font("LiberationSans-Regular.ttf").hash());
    let bold = switch_style(&embedded, true, false);
    assert_eq!((bold.family.as_str(), bold.style, bold.asset), ("Test Sans", FontStyle::Bold, None));
    let back = switch_style(&bold, false, false);
    assert_eq!(back.style, FontStyle::Regular);
}

#[test]
fn bold_selects_the_registered_bold_face_and_a_missing_style_is_never_substituted() {
    let mut src = MemoryFontSource::new();
    let regular = src.insert("Test Sans", FontStyle::Regular, font("LiberationSans-Regular.ttf"));
    // Stand-in bytes for the Bold face (the repository has no bold fixture): what matters is that
    // the face registered under (family, Bold) is the one that draws.
    let bold_data = font("BIZUDPGothic-Regular.ttf");
    src.insert("Test Sans", FontStyle::Bold, bold_data.clone());
    assert_eq!(src.available_styles("Test Sans"), [FontStyle::Regular, FontStyle::Bold]);
    assert!(src.available_styles("Nope").is_empty());

    let mut obj = text("H", &regular);
    obj.font = switch_style(&regular, true, false);
    let shaped = shape(&obj, &src).unwrap();
    assert_eq!(shaped.used_fonts, [bold_data.hash()]);

    obj.font = switch_style(&regular, false, true);
    let err = shape(&obj, &src).unwrap_err();
    assert!(matches!(err, TextError::Font(FontError::NotFound(ref r)) if r.style == FontStyle::Italic), "{err:?}");
}

#[test]
fn uppercase_draws_the_capital_forms_and_keeps_source_offsets() {
    let (src, r) = liberation();
    let mut lower = text("abß", &r);
    lower.layout.uppercase = true;
    let upper = text("ABSS", &r);
    let (a, b) = (shape(&lower, &src).unwrap(), shape(&upper, &src).unwrap());
    assert_eq!(a.outlines, b.outlines, "same outlines as the capitals typed out");
    assert_eq!(
        a.glyphs.iter().map(|g| g.glyph_id).collect::<Vec<_>>(),
        b.glyphs.iter().map(|g| g.glyph_id).collect::<Vec<_>>()
    );
    // Clusters point into the *source* text: a = 0, b = 1, ß (two bytes) = 2 for both S glyphs.
    assert_eq!(a.glyphs.iter().map(|g| g.cluster).collect::<Vec<_>>(), [0, 1, 2, 2]);

    lower.layout.uppercase = false;
    assert_ne!(shape(&lower, &src).unwrap().outlines, b.outlines, "lower case stays lower case");

    // A character no font can draw is reported at its source byte, also after upper-casing.
    let mut missing = text("a日", &r);
    missing.layout.uppercase = true;
    let m = shape(&missing, &src).unwrap().missing;
    assert_eq!(m.len(), 1);
    assert_eq!((m[0].ch, m[0].byte, m[0].line), ('日', 1, 0));
}

#[test]
fn vertical_alignment_hangs_the_block_on_the_origin() {
    let (src, r) = liberation();
    let (asc, desc) = (18.54, 4.34); // hhea 1854 / 434 font units at 0.01 mm
    let baselines = |valign, content: &str| {
        let mut obj = text(content, &r);
        obj.layout.valign = valign;
        shape(&obj, &src).unwrap().lines.iter().map(|l| l.baseline_y).collect::<Vec<_>>()
    };
    assert_eq!(baselines(VAlign::Baseline, "H"), [0.0]);
    let top = baselines(VAlign::Top, "H");
    assert!((top[0] + asc).abs() < EPS, "{top:?}");
    let middle = baselines(VAlign::Middle, "H");
    assert!((middle[0] - ((asc + desc) / 2.0 - asc)).abs() < EPS, "{middle:?}");
    let bottom = baselines(VAlign::Bottom, "H");
    assert!((bottom[0] - desc).abs() < EPS, "{bottom:?}");

    // Two lines: the bottom of the *last* line's descent is at y = 0.
    let two = baselines(VAlign::Bottom, "H\nH");
    assert!((two[1] - desc).abs() < EPS && (two[0] - (desc + 1.2 * 20.48)).abs() < EPS, "{two:?}");
    let two_top = baselines(VAlign::Top, "H\nH");
    assert!((two_top[0] + asc).abs() < EPS && (two_top[1] + asc + 1.2 * 20.48).abs() < EPS, "{two_top:?}");

    // The outline follows: with Top the cap height (14.09) ends 4.45 mm below the origin.
    let mut obj = text("H", &r);
    obj.layout.valign = VAlign::Top;
    let top_y = shape(&obj, &src).unwrap().outlines.bounds().unwrap().max().y;
    assert!((top_y - (14.09 - asc)).abs() < EPS, "{top_y}");
}

#[test]
fn single_line_text_hangs_on_its_cap_height() {
    let (src, r) = liberation();
    let mut obj = text("H", &r);
    obj.mode = TextMode::SingleLine;
    obj.layout.valign = VAlign::Top;
    let shaped = shape(&obj, &src).unwrap();
    assert!((shaped.lines[0].baseline_y + 0.7 * 20.48).abs() < EPS, "cap = 0.7 em");
    assert!(shaped.outlines.bounds().unwrap().max().y.abs() < 1e-6, "the top of the capital is on the origin");
}

#[test]
fn v_space_adds_millimetres_to_the_baseline_pitch() {
    let (src, r) = liberation();
    let mut obj = text("a\nb\nc", &r);
    obj.layout.v_space_mm = 3.0;
    let ys: Vec<f64> = shape(&obj, &src).unwrap().lines.iter().map(|l| l.baseline_y).collect();
    let pitch = 1.2 * 20.48 + 3.0;
    assert!((ys[1] + pitch).abs() < EPS && (ys[2] + 2.0 * pitch).abs() < EPS, "{ys:?}");
    obj.layout.v_space_mm = f64::NAN;
    assert_eq!(shape(&obj, &src).unwrap_err(), TextError::InvalidLayout);
}

#[test]
fn welded_text_merges_overlapping_glyphs_into_one_contour() {
    let (src, r) = liberation();
    // Two 'I' (rectangles 1.91 × 14.09 mm) whose origins are 1.19 mm apart: they overlap by 0.72 mm.
    let mut obj = text("II", &r);
    obj.layout.letter_spacing_mm = -4.5;
    let loose = shape(&obj, &src).unwrap();
    assert_eq!(loose.outlines.subpaths().len(), 2);
    obj.layout.welded = true;
    let welded = shape(&obj, &src).unwrap();
    assert_eq!(welded.outlines.subpaths().len(), 1, "one merged contour");
    // Union area = (499 − 189) × 1409 font units² = 3.10 mm × 14.09 mm.
    assert!((welded.outlines.signed_area().abs() - 3.10 * 14.09).abs() < 1e-3, "{}", welded.outlines.signed_area());
    assert_eq!(welded.line_outlines.len(), 1);
    assert_eq!(welded.glyphs.len(), 2, "glyph bookkeeping is unchanged");

    // Welding never fills a counter: 'O' keeps its hole, wound opposite to its outer contour.
    let mut o = text("O", &r);
    let unwelded = shape(&o, &src).unwrap().outlines.signed_area();
    o.layout.welded = true;
    let w = shape(&o, &src).unwrap();
    assert_eq!(w.outlines.subpaths().len(), 2);
    let windings: Vec<_> = w.outlines.subpaths().iter().map(|s| s.winding()).collect();
    assert!(windings.contains(&Some(Winding::CounterClockwise)) && windings.contains(&Some(Winding::Clockwise)));
    assert!(
        (w.outlines.signed_area().abs() - unwelded.abs()).abs() < 0.1, // polygonal within em/2000 = 0.01 mm
        "{} vs {}",
        w.outlines.signed_area(),
        unwelded
    );

    // Stroke text is never welded (open paths).
    let mut single = text("II", &r);
    single.mode = TextMode::SingleLine;
    single.layout.welded = true;
    assert!(shape(&single, &src).unwrap().outlines.subpaths().iter().all(|s| !s.is_closed()));
}

#[test]
fn a_font_source_lists_its_families_sorted_and_once_and_a_bare_source_lists_none() {
    // The text toolbar's font choice is this list (W3-UI-tekst).
    let mut src = MemoryFontSource::new();
    let data = font("LiberationSans-Regular.ttf");
    src.insert("Zeta Sans", FontStyle::Regular, data.clone());
    src.insert("Alpha Sans", FontStyle::Bold, data.clone());
    src.insert("Alpha Sans", FontStyle::Regular, data);
    assert_eq!(src.families(), ["Alpha Sans", "Zeta Sans"]);
    assert_eq!(src.available_styles("Alpha Sans"), [FontStyle::Regular, FontStyle::Bold]);

    struct Bare;
    impl FontSource for Bare {
        fn load(&self, f: &FontRef) -> Result<Arc<FontData>, FontError> {
            Err(FontError::NotFound(f.clone()))
        }
    }
    assert!(Bare.families().is_empty(), "a source that cannot enumerate says so; nothing is invented");
}
