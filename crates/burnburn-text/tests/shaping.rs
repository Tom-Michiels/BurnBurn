//! Shaping, outlines and layout against independent references.
//!
//! Reference numbers (units per em, advances, glyf bounding boxes) were read from the fixture
//! bytes with `test-data/fonts/reference_metrics.py` (a `struct`-only parser, not skrifa):
//! Liberation Sans: upem 2048; 'H' advance 1479, bbox (168, 0, 1312, 1409); 'A' and 'V' advance
//! 1366; 'f' 569, 'i' 455; ' ' 569; '日' not mapped (gid 0).
//! BIZ UDPGothic: '日' gid 4248, advance 2048, bbox (274, -154, 1769, 1634).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test helpers outside #[test] fns

use std::sync::Arc;

use burnburn_geometry::{Affine, Path, Vector};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_text::{
    Align, FontData, FontError, FontRef, FontSize, FontStyle, MM_PER_POINT, MemoryFontSource, TextError, TextMode,
    TextObject, shape,
};
use burnburn_types::{ContentHash, HashDomain, Length};

const UPEM: f64 = 2048.0;
const EPS: f64 = 1e-9;

fn font(name: &str) -> Arc<FontData> {
    let bytes = read_fixture("fonts", name).unwrap();
    Arc::new(FontData::from_bytes(bytes).unwrap())
}

fn liberation() -> (MemoryFontSource, FontRef) {
    let mut src = MemoryFontSource::new();
    let r = src.insert("Liberation Sans", FontStyle::Regular, font("LiberationSans-Regular.ttf"));
    (src, r)
}

/// Em of 20.48 mm: one font unit is exactly 0.01 mm.
fn em_2048() -> FontSize {
    FontSize::from_mm(Length::from_mm(20.48).unwrap()).unwrap()
}

fn text(content: &str, font: &FontRef) -> TextObject {
    TextObject::new(content, font.clone(), em_2048())
}

fn bounds(p: &Path) -> (f64, f64, f64, f64) {
    let b = p.bounds().unwrap();
    (b.min().x, b.min().y, b.max().x, b.max().y)
}

#[test]
fn font_loads_from_bytes_and_garbage_is_a_typed_error() {
    let bytes = read_fixture("fonts", "LiberationSans-Regular.ttf").unwrap();
    let data = FontData::from_bytes(bytes.clone()).unwrap();
    assert_eq!(data.units_per_em(), 2048);
    assert_eq!(data.hash(), ContentHash::of(HashDomain::Asset, &bytes));
    assert!(data.has_glyph('H'));
    assert!(!data.has_glyph('日'));
    assert_eq!(FontData::from_bytes(b"not a font at all".to_vec()).unwrap_err(), FontError::Unreadable);
    assert_eq!(FontData::from_bytes(Vec::new()).unwrap_err(), FontError::Unreadable);
}

#[test]
fn glyph_outline_matches_reference_bbox_in_millimetres() {
    let (src, r) = liberation();
    let shaped = shape(&text("H", &r), &src).unwrap();
    let (x0, y0, x1, y1) = bounds(&shaped.outlines);
    // bbox (168, 0, 1312, 1409) font units × 0.01 mm.
    assert!((x0 - 1.68).abs() < EPS && (y0 - 0.0).abs() < EPS, "{x0} {y0}");
    assert!((x1 - 13.12).abs() < EPS && (y1 - 14.09).abs() < EPS, "{x1} {y1}");
    assert!(shaped.outlines.subpaths().iter().all(|s| s.is_closed()), "outline text is closed");
    assert_eq!(shaped.glyphs.len(), 1);
    assert!((shaped.glyphs[0].advance_mm - 14.79).abs() < EPS);
    assert!(shaped.missing.is_empty());
}

#[test]
fn points_and_millimetres_are_explicit_size_units() {
    let pt = FontSize::from_points(72.0).unwrap();
    assert!((pt.em_mm() - 25.4).abs() < EPS);
    assert!((MM_PER_POINT * 72.0 - 25.4).abs() < EPS);
    assert!(FontSize::from_points(0.0).is_err());
    assert!(FontSize::from_mm(Length::from_mm(-1.0).unwrap()).is_err());

    let (src, r) = liberation();
    let mut obj = text("H", &r);
    obj.size = pt;
    let (_, _, _, y1) = bounds(&shape(&obj, &src).unwrap().outlines);
    assert!((y1 - 1409.0 / UPEM * 25.4).abs() < EPS, "{y1}");
}

#[test]
fn kerning_tightens_av_and_can_be_switched_off() {
    let (src, r) = liberation();
    let mut obj = text("AV", &r);
    let kerned = shape(&obj, &src).unwrap();
    obj.layout.kerning = false;
    let plain = shape(&obj, &src).unwrap();
    // Without kerning the pen advances by the hmtx widths: 13.66 + 13.66 mm.
    assert!((plain.lines[0].width_mm - 27.32).abs() < EPS, "{}", plain.lines[0].width_mm);
    assert!(kerned.lines[0].width_mm < plain.lines[0].width_mm - 0.5, "AV is a kerning pair in Liberation Sans");
    assert!(kerned.glyphs[1].origin.x < plain.glyphs[1].origin.x);
}

/// Liberation Sans has no Latin ligatures (GSUB latn: ccmp, subs, sups; its dlig is Hebrew-only —
/// read with a struct parser over ScriptList/FeatureList). BIZ UDPGothic has `liga` for latn.
/// Advances from its hmtx (struct parser): f 1024, i 655, glyph 12864 1491 font units.
#[test]
fn fi_ligature_is_one_glyph_and_can_be_switched_off() {
    let mut src = MemoryFontSource::new();
    let r = src.insert("BIZ UDPGothic", FontStyle::Regular, font("BIZUDPGothic-Regular.ttf"));
    let mut obj = text("fi", &r);
    let liga = shape(&obj, &src).unwrap();
    assert_eq!(liga.glyphs.len(), 1, "one ligature glyph");
    assert_eq!((liga.glyphs[0].glyph_id, liga.glyphs[0].cluster), (12864, 0));
    assert!((liga.lines[0].width_mm - 14.91).abs() < EPS);

    obj.layout.ligatures = false;
    let plain = shape(&obj, &src).unwrap();
    assert_eq!(plain.glyphs.iter().map(|g| g.cluster).collect::<Vec<_>>(), [0, 1]);
    assert!((plain.lines[0].width_mm - (10.24 + 6.55)).abs() < EPS);

    // Letter spacing goes between clusters: a ligature is one cluster, so it gets none inside.
    obj.layout.ligatures = true;
    obj.layout.letter_spacing_mm = 3.0;
    assert!((shape(&obj, &src).unwrap().lines[0].width_mm - 14.91).abs() < EPS);
}

#[test]
fn lines_stack_by_line_spacing_and_align_left_center_right() {
    let (src, r) = liberation();
    let mut obj = text("H\nHH", &r);
    obj.layout.line_spacing = 1.5;
    obj.layout.kerning = false;
    for (align, expected_start) in
        [(Align::Left, [0.0, 0.0]), (Align::Center, [-7.395, -14.79]), (Align::Right, [-14.79, -29.58])]
    {
        obj.layout.align = align;
        let s = shape(&obj, &src).unwrap();
        assert_eq!(s.lines.len(), 2);
        assert_eq!(s.line_outlines.len(), 2);
        assert!((s.lines[1].baseline_y - (-1.5 * 20.48)).abs() < EPS);
        for (line, start) in s.lines.iter().zip(expected_start) {
            assert!((line.start_x - start).abs() < EPS, "{align:?} {}", line.start_x);
        }
        // Second line's glyph sits on its baseline: bbox y from -30.72 to -30.72 + 14.09.
        let (_, y0, _, y1) = bounds(&s.line_outlines[1]);
        assert!((y0 + 30.72).abs() < EPS && (y1 - (14.09 - 30.72)).abs() < EPS);
    }
}

#[test]
fn letter_spacing_is_added_between_clusters_only() {
    let (src, r) = liberation();
    let mut obj = text("HHH", &r);
    obj.layout.kerning = false;
    obj.layout.letter_spacing_mm = 2.0;
    let s = shape(&obj, &src).unwrap();
    assert!((s.lines[0].width_mm - (3.0 * 14.79 + 2.0 * 2.0)).abs() < EPS);
    assert!((s.glyphs[2].origin.x - 2.0 * (14.79 + 2.0)).abs() < EPS);
    obj.layout.letter_spacing_mm = f64::NAN;
    assert_eq!(shape(&obj, &src).unwrap_err(), TextError::InvalidLayout);
    obj.layout.letter_spacing_mm = 0.0;
    obj.layout.line_spacing = 0.0;
    assert_eq!(shape(&obj, &src).unwrap_err(), TextError::InvalidLayout);
}

#[test]
fn placement_maps_text_space_to_document_millimetres() {
    let (src, r) = liberation();
    let mut obj = text("H", &r);
    obj.placement = Affine::translate(Vector::new(100.0, 50.0)).unwrap();
    let (x0, y0, x1, y1) = bounds(&shape(&obj, &src).unwrap().outlines);
    assert!((x0 - 101.68).abs() < EPS && (y0 - 50.0).abs() < EPS);
    assert!((x1 - 113.12).abs() < EPS && (y1 - 64.09).abs() < EPS);
}

#[test]
fn fallback_chain_draws_missing_characters_from_the_next_font() {
    let (mut src, r) = liberation();
    let jp = src.insert("BIZ UDPGothic", FontStyle::Regular, font("BIZUDPGothic-Regular.ttf"));
    let mut obj = text("H日", &r);
    obj.layout.kerning = false;
    obj.fallback = vec![jp.clone()];
    let s = shape(&obj, &src).unwrap();
    assert!(s.missing.is_empty());
    assert_eq!(s.glyphs.iter().map(|g| (g.font, g.glyph_id)).collect::<Vec<_>>(), [(0, 43), (1, 4248)]);
    assert_eq!(s.used_fonts, [r.asset.unwrap(), jp.asset.unwrap()]);
    // '日' starts after 'H' (14.79 mm), bbox x 274..1769, y -154..1634 font units.
    let (x0, y0, x1, y1) = bounds(&s.outlines);
    assert!((x0 - 1.68).abs() < EPS && (y0 + 1.54).abs() < EPS);
    assert!((x1 - (14.79 + 17.69)).abs() < EPS && (y1 - 16.34).abs() < EPS);
    assert!((s.lines[0].width_mm - (14.79 + 20.48)).abs() < EPS);
}

#[test]
fn missing_glyph_is_reported_and_never_drawn_as_tofu() {
    let (src, r) = liberation();
    let mut obj = text("H日H", &r);
    obj.layout.kerning = false;
    let s = shape(&obj, &src).unwrap();
    assert_eq!(s.missing.len(), 1);
    assert_eq!((s.missing[0].ch, s.missing[0].byte, s.missing[0].line), ('日', 1, 0));
    // Only the two H glyphs: no .notdef box. The missing character takes no space.
    assert_eq!(s.glyphs.len(), 2);
    assert!(s.glyphs.iter().all(|g| g.glyph_id == 43));
    assert!((s.glyphs[1].origin.x - 14.79).abs() < EPS);
    let one_h = shape(&text("H", &r), &src).unwrap().outlines.subpaths().len();
    assert_eq!(s.outlines.subpaths().len(), 2 * one_h);
}

#[test]
fn missing_font_in_chain_is_an_error_not_a_silent_substitute() {
    let (src, r) = liberation();
    let mut obj = text("H", &r);
    let absent =
        FontRef::new("Liberation Sans", FontStyle::Regular).with_asset(ContentHash::of(HashDomain::Asset, b"x"));
    obj.font = absent.clone();
    assert_eq!(shape(&obj, &src).unwrap_err(), TextError::Font(FontError::NotFound(absent.clone())));
    obj.font = r;
    obj.fallback = vec![absent.clone()];
    assert_eq!(shape(&obj, &src).unwrap_err(), TextError::Font(FontError::NotFound(absent)));
}

#[test]
fn empty_and_whitespace_text_give_no_geometry() {
    let (src, r) = liberation();
    let s = shape(&text("", &r), &src).unwrap();
    assert!(s.outlines.is_empty() && s.glyphs.is_empty() && s.missing.is_empty());
    let s = shape(&text(" \n ", &r), &src).unwrap();
    assert!(s.outlines.is_empty());
    assert_eq!(s.lines.len(), 2);
    assert!((s.lines[0].width_mm - 5.69).abs() < EPS);
}

#[test]
fn shaping_is_deterministic() {
    let (src, r) = liberation();
    let obj = text("Hoofdletters AV fi\nTweede regel", &r);
    assert_eq!(shape(&obj, &src).unwrap(), shape(&obj, &src).unwrap());
}

#[test]
fn single_line_mode_gives_open_strokes_and_reports_unsupported_characters() {
    let (src, r) = liberation();
    let mut obj = text("A1-Z", &r);
    obj.mode = TextMode::SingleLine;
    let s = shape(&obj, &src).unwrap();
    assert!(s.missing.is_empty());
    assert!(!s.outlines.is_empty());
    assert!(s.outlines.subpaths().iter().all(|p| !p.is_closed()), "single-line paths are never closed");
    assert!(s.used_fonts.is_empty(), "single-line text needs no font asset");
    // Cap height of the stroke set is 0.7 em.
    let (_, y0, _, y1) = bounds(&s.outlines);
    assert!(y0.abs() < EPS && (y1 - 0.7 * 20.48).abs() < EPS, "{y0} {y1}");

    obj.content = "Ab".into();
    let s = shape(&obj, &src).unwrap();
    assert_eq!(s.missing.iter().map(|m| m.ch).collect::<String>(), "b");
}

#[test]
fn glyph_outlines_are_per_glyph_in_text_space() {
    let (src, r) = liberation();
    let mut obj = text("HH", &r);
    obj.layout.kerning = false;
    obj.placement = Affine::translate(Vector::new(100.0, 0.0)).unwrap();
    let s = shape(&obj, &src).unwrap();
    assert_eq!(s.glyph_outlines.len(), 2);
    let (x0, _, x1, _) = bounds(&s.glyph_outlines[1]);
    // Second H in text space (placement not applied): 14.79 + 1.68 .. 14.79 + 13.12.
    assert!((x0 - 16.47).abs() < EPS && (x1 - 27.91).abs() < EPS, "{x0} {x1}");
}
