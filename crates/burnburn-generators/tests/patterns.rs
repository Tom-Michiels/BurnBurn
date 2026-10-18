//! Living hinge, stencil bridges and text on a path against analytical oracles.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use burnburn_generators::{GenError, LivingHinge, PathTextParams, living_hinge, stencil_bridges, text_on_path};
use burnburn_geometry::{Path, Point, ProductionTolerance, Rect, Subpath, Vector};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_text::{FontData, FontSize, FontStyle, MemoryFontSource, TextObject};
use burnburn_types::Length;

const EPS: f64 = 1e-9;

fn live() -> burnburn_types::CancellationToken {
    burnburn_types::CancellationSource::new().token()
}

fn tol(mm: f64) -> ProductionTolerance {
    ProductionTolerance::new(Length::from_mm(mm).unwrap()).unwrap()
}

fn hinge() -> LivingHinge {
    LivingHinge { width: 20.0, height: 50.0, cut_length: 10.0, bridge: 2.0, spacing: 2.0 }
}

fn cut(p: &Path) -> (f64, f64, f64) {
    let s = &p.subpaths()[0];
    (s.start().x, s.start().y.min(s.end().y), s.start().y.max(s.end().y))
}

#[test]
fn living_hinge_columns_fill_the_height_with_exact_bridges() {
    let cuts = living_hinge(&hinge()).unwrap();
    assert!(cuts.iter().all(|c| c.subpaths().len() == 1 && !c.subpaths()[0].is_closed()), "open cut lines");
    let mut columns: Vec<f64> = cuts.iter().map(|c| cut(c).0).collect();
    columns.dedup();
    // 20 / 2 → 11 columns at x = 0, 2, …, 20.
    assert_eq!(columns.len(), 11);
    for (i, x) in columns.iter().enumerate() {
        assert!((x - 2.0 * i as f64).abs() < EPS);
    }
    // Even columns: k = round((50 - 2) / 12) = 4 cuts of (50 - 5·2)/4 = 10 mm, bridges of 2 mm
    // at both ends and between.
    let even: Vec<_> = cuts.iter().map(cut).filter(|c| c.0.abs() < EPS).collect();
    assert_eq!(even.len(), 4);
    for (j, (_, y0, y1)) in even.iter().enumerate() {
        assert!((y0 - (2.0 + 12.0 * j as f64)).abs() < EPS && (y1 - y0 - 10.0).abs() < EPS, "{y0} {y1}");
    }
    // Odd columns: staggered, centred on the even column's bridges, kept 2 mm from the ends.
    let odd: Vec<_> = cuts.iter().map(cut).filter(|c| (c.0 - 2.0).abs() < EPS).collect();
    assert_eq!(odd.len(), 5);
    assert!((odd[0].1 - 2.0).abs() < EPS && (odd[0].2 - 6.0).abs() < EPS);
    assert!((odd[1].1 - 8.0).abs() < EPS && (odd[1].2 - 18.0).abs() < EPS);
    assert!((odd[4].2 - 48.0).abs() < EPS);
    for w in odd.windows(2) {
        assert!((w[1].1 - w[0].2 - 2.0).abs() < EPS, "bridge between odd cuts");
    }
}

#[test]
fn living_hinge_rejects_impossible_patterns() {
    let err = |f: fn(&mut LivingHinge)| {
        let mut h = hinge();
        f(&mut h);
        living_hinge(&h).unwrap_err()
    };
    assert_eq!(err(|h| h.spacing = 0.0), GenError::NotPositive { param: "spacing" });
    assert_eq!(err(|h| h.bridge = -1.0), GenError::NotPositive { param: "bridge" });
    assert!(matches!(err(|h| h.cut_length = 1.5), GenError::TooSmall { param: "cut_length", .. }));
    assert!(matches!(err(|h| h.height = 5.0), GenError::TooSmall { param: "height", .. }));
}

fn square(x0: f64, y0: f64, s: f64) -> Subpath {
    Path::rectangle(Rect::new(Point::new(x0, y0), Length::from_mm(s).unwrap(), Length::from_mm(s).unwrap()).unwrap())
        .subpaths()[0]
        .clone()
}

/// Two contours at nesting depth 0 and 1 of an even-odd region (no nested pairs left = no island).
fn nested_pairs(p: &Path) -> usize {
    let polys: Vec<Vec<Point>> = p
        .subpaths()
        .iter()
        .map(|s| {
            let mut v = vec![s.start()];
            v.extend(s.segments().iter().map(|g| g.end()));
            v
        })
        .collect();
    let inside = |pt: Point, poly: &[Point]| {
        let mut c = false;
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            if (a.y > pt.y) != (b.y > pt.y) && pt.x < a.x + (pt.y - a.y) * (b.x - a.x) / (b.y - a.y) {
                c = !c;
            }
        }
        c
    };
    let mut n = 0;
    for (i, a) in polys.iter().enumerate() {
        for (j, b) in polys.iter().enumerate() {
            if i != j && a.iter().all(|pt| inside(*pt, b)) {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn stencil_bridge_frees_the_island_of_a_square_ring_with_exact_area() {
    // Ring: 20×20 outer, 10×10 hole centred; ring area 300 mm².
    let ring = Path::from_subpaths(vec![square(0.0, 0.0, 20.0), square(5.0, 5.0, 10.0)]);
    assert_eq!(nested_pairs(&ring), 1);
    let s = stencil_bridges(&ring, 2.0, tol(0.01), &live()).unwrap();
    assert_eq!((s.bridges, s.islands_left), (1, 0));
    assert_eq!(nested_pairs(&s.path), 0, "the counter is connected to the sheet");
    // A vertical 2 mm bridge through x = 10 removes 2 × 5 mm at the top and at the bottom.
    assert!((s.path.signed_area().abs() - (300.0 - 20.0)).abs() < 1e-6, "{}", s.path.signed_area());
    let b = s.path.bounds().unwrap();
    assert!((b.width() - 20.0).abs() < 1e-6 && (b.height() - 20.0).abs() < 1e-6);
}

#[test]
fn stencil_without_holes_is_unchanged_and_bad_width_is_refused() {
    let solid = Path::from_subpaths(vec![square(0.0, 0.0, 20.0)]);
    let s = stencil_bridges(&solid, 2.0, tol(0.01), &live()).unwrap();
    assert_eq!(s.bridges, 0);
    assert!((s.path.signed_area().abs() - 400.0).abs() < 1e-6);
    let ring = Path::from_subpaths(vec![square(0.0, 0.0, 20.0), square(5.0, 5.0, 10.0)]);
    assert_eq!(stencil_bridges(&ring, 0.0, tol(0.01), &live()).unwrap_err(), GenError::NotPositive { param: "width" });
    assert!(matches!(
        stencil_bridges(&ring, 10.0, tol(0.01), &live()).unwrap_err(),
        GenError::TooSmall { param: "width", .. }
    ));
    let source = burnburn_types::CancellationSource::new();
    source.cancel();
    assert_eq!(
        stencil_bridges(&ring, 2.0, tol(0.01), &source.token()).unwrap_err(),
        GenError::Boolean(burnburn_geometry::BooleanError::Cancelled)
    );
}

fn liberation() -> (MemoryFontSource, TextObject) {
    let mut src = MemoryFontSource::new();
    let data = Arc::new(FontData::from_bytes(read_fixture("fonts", "LiberationSans-Regular.ttf").unwrap()).unwrap());
    let r = src.insert("Liberation Sans", FontStyle::Regular, data);
    let mut obj = TextObject::new("OBO", r, FontSize::from_mm(Length::from_mm(10.0).unwrap()).unwrap());
    obj.layout.kerning = false;
    (src, obj)
}

#[test]
fn stencil_bridges_letter_o_glyph() {
    let (src, obj) = liberation();
    let shaped = burnburn_text::shape(&obj, &src).unwrap();
    assert!(nested_pairs(&shaped.outlines) >= 3, "O, B (2 counters), O");
    let s = stencil_bridges(&shaped.outlines, 0.8, tol(0.01), &live()).unwrap();
    assert_eq!(s.bridges, 4);
    assert_eq!(s.islands_left, 0);
}

#[test]
fn text_on_circle_sits_at_radius_minus_offset_and_follows_the_tangent() {
    let (src, obj) = liberation();
    let r = 50.0;
    let circle = Path::circle(Point::ORIGIN, r).unwrap();
    let shaped = burnburn_text::shape(&obj, &src).unwrap();
    let advances: Vec<f64> = shaped.glyphs.iter().map(|g| g.advance_mm).collect();
    for (offset, reverse, radius) in [(2.0, false, r - 2.0), (2.0, true, r + 2.0), (0.0, false, r)] {
        let p = PathTextParams { start: 10.0, offset, reverse, tolerance: tol(0.00001) };
        let t = text_on_path(&obj, &src, &circle, &p).unwrap();
        assert_eq!(t.frames.len(), 3);
        for f in &t.frames {
            let d = f.centre.to_vector().length();
            assert!((d - radius).abs() < 2e-3, "reverse={reverse}: {d}");
            // Unit tangent of a CCW circle at c is (−c.y, c.x)/|c|; reversed: (c.y, −c.x)/|c|.
            let c = f.centre.to_vector() * (1.0 / d);
            let expected = if reverse { Vector::new(c.y, -c.x) } else { Vector::new(-c.y, c.x) };
            assert!((f.direction - expected).length() < 1e-3, "{:?} vs {expected:?}", f.direction);
            assert!((f.direction.length() - 1.0).abs() < 1e-12);
        }
        // Glyph centres are spaced by half the sum of neighbouring advances, along the guide.
        assert!((t.frames[0].at - (10.0 + advances[0] / 2.0)).abs() < 1e-9);
        assert!((t.frames[1].at - t.frames[0].at - (advances[0] + advances[1]) / 2.0).abs() < 1e-9);
        assert!(!t.path.is_empty());
    }
}

#[test]
fn text_on_path_reports_overflow_and_multiline() {
    let (src, mut obj) = liberation();
    let short = Path::line(Point::ORIGIN, Point::new(5.0, 0.0)).unwrap();
    let p = PathTextParams { start: 0.0, offset: 0.0, reverse: false, tolerance: tol(0.01) };
    assert!(matches!(text_on_path(&obj, &src, &short, &p).unwrap_err(), GenError::TextLongerThanPath { .. }));
    obj.content = "a\nb".into();
    let long = Path::line(Point::ORIGIN, Point::new(500.0, 0.0)).unwrap();
    assert_eq!(text_on_path(&obj, &src, &long, &p).unwrap_err(), GenError::MultiLineText);
}

#[test]
fn text_on_a_straight_line_equals_plain_text_translated() {
    let (src, mut obj) = liberation();
    obj.content = "H".into();
    let line = Path::line(Point::new(0.0, 0.0), Point::new(200.0, 0.0)).unwrap();
    let p = PathTextParams { start: 0.0, offset: 0.0, reverse: false, tolerance: tol(0.01) };
    let t = text_on_path(&obj, &src, &line, &p).unwrap();
    let plain = burnburn_text::shape(&obj, &src).unwrap().outlines.bounds().unwrap();
    let on = t.path.bounds().unwrap();
    assert!((on.min().x - plain.min().x).abs() < 1e-9 && (on.max().y - plain.max().y).abs() < 1e-9);
}
