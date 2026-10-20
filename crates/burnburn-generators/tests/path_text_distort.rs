//! Text on a path with the Distort choice (W1-L5b): rigid glyphs versus outlines that bend.
//!
//! Oracle for the bend: on a circle of radius R (counter-clockwise guide, left normal towards the
//! centre) a point at height h above the baseline lies at distance R − offset − h from the centre,
//! whatever its position along the text.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use burnburn_generators::{Distort, PathTextParams, text_on_path};
use burnburn_geometry::{Path, Point, ProductionTolerance, Winding};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_text::{FontData, FontSize, FontStyle, MemoryFontSource, TextObject, shape};
use burnburn_types::Length;

fn tol(mm: f64) -> ProductionTolerance {
    ProductionTolerance::new(Length::from_mm(mm).unwrap()).unwrap()
}

fn setup(content: &str) -> (MemoryFontSource, TextObject) {
    let mut src = MemoryFontSource::new();
    let data = Arc::new(FontData::from_bytes(read_fixture("fonts", "LiberationSans-Regular.ttf").unwrap()).unwrap());
    let r = src.insert("Liberation Sans", FontStyle::Regular, data);
    (src, TextObject::new(content, r, FontSize::from_mm(Length::from_mm(10.0).unwrap()).unwrap()))
}

fn params(distort: Distort, offset: f64) -> PathTextParams {
    PathTextParams { distort, start: 8.0, offset, reverse: false, tolerance: tol(0.001) }
}

fn vertices(p: &Path) -> Vec<Point> {
    p.flatten(&tol(0.001)).into_iter().flat_map(|pl| pl.points).collect()
}

#[test]
fn warped_text_on_a_circle_puts_height_at_radius_minus_offset_minus_height() {
    let (src, obj) = setup("HOH");
    let r = 50.0;
    let circle = Path::circle(Point::ORIGIN, r).unwrap();
    let offset = 1.5;
    let plain = shape(&obj, &src).unwrap();
    let (cap_min, cap_max) = (plain.outlines.bounds().unwrap().min().y, plain.outlines.bounds().unwrap().max().y);
    let warped = text_on_path(&obj, &src, &circle, &params(Distort::Warp, offset)).unwrap();
    let radii: Vec<f64> = vertices(&warped.path).iter().map(|q| q.to_vector().length()).collect();
    assert!(radii.len() > vertices(&plain.outlines).len(), "straight edges are subdivided so they can bend");
    let (lo, hi) = (radii.iter().copied().fold(f64::INFINITY, f64::min), radii.iter().copied().fold(0.0, f64::max));
    // Heights span [cap_min, cap_max] (the 'O' overshoots the baseline slightly).
    assert!((hi - (r - offset - cap_min)).abs() < 2e-3, "{hi}");
    assert!((lo - (r - offset - cap_max)).abs() < 2e-3, "{lo}");
    // Every vertex height corresponds to a height of the plain glyph outline or lies between two of its
    // vertices on a straight edge: it is never outside the glyph's height range.
    assert!(radii.iter().all(|&d| d >= lo - 1e-9 && d <= hi + 1e-9));
    // The flat tops of the stems (constant height) become arcs of constant radius: many vertices at `lo`.
    let h_top = plain.glyph_outlines[0].bounds().unwrap().max().y; // 1409 font units of 2048 at 10 mm
    let top = radii.iter().filter(|&&d| (d - (r - offset - h_top)).abs() < 2e-3).count();
    assert!(top > 12, "{top} vertices on the cap line");
}

#[test]
fn warped_contours_stay_closed_and_counters_stay_counters() {
    let (src, obj) = setup("O");
    let circle = Path::circle(Point::ORIGIN, 40.0).unwrap();
    let t = text_on_path(&obj, &src, &circle, &params(Distort::Warp, 0.0)).unwrap();
    let subs = t.path.subpaths();
    assert_eq!(subs.len(), 2);
    assert!(subs.iter().all(|s| s.is_closed()));
    let w: Vec<_> = subs.iter().map(|s| s.winding()).collect();
    assert!(w.contains(&Some(Winding::Clockwise)) && w.contains(&Some(Winding::CounterClockwise)), "{w:?}");
}

#[test]
fn warped_text_on_a_straight_line_is_plain_text_translated() {
    let (src, obj) = setup("Hi");
    let line = Path::line(Point::new(0.0, 0.0), Point::new(200.0, 0.0)).unwrap();
    let plain = shape(&obj, &src).unwrap().outlines;
    let t = text_on_path(&obj, &src, &line, &params(Distort::Warp, 0.0)).unwrap();
    let (a, b) = (plain.bounds().unwrap(), t.path.bounds().unwrap());
    assert!((b.min().x - (a.min().x + 8.0)).abs() < 1e-6 && (b.max().x - (a.max().x + 8.0)).abs() < 1e-6);
    assert!((b.min().y - a.min().y).abs() < 1e-6 && (b.max().y - a.max().y).abs() < 1e-6);
    assert!((t.path.signed_area().abs() - plain.signed_area().abs()).abs() < 1e-6);
}

#[test]
fn rigid_and_warp_differ_on_a_curve_and_agree_on_the_glyph_frames() {
    let (src, obj) = setup("HOH");
    let circle = Path::circle(Point::ORIGIN, 50.0).unwrap();
    let rigid = text_on_path(&obj, &src, &circle, &params(Distort::Rigid, 0.0)).unwrap();
    let warp = text_on_path(&obj, &src, &circle, &params(Distort::Warp, 0.0)).unwrap();
    assert_ne!(rigid.path, warp.path);
    assert_eq!(rigid.frames, warp.frames, "the frames report where each glyph is anchored in both modes");
}
