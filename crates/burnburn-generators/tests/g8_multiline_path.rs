//! G8-editor E08: text of several lines on a path, and the two helpers the window needs to move a
//! bound text (the guide's frame at an arc length, and how far the text may start). Oracles: on a
//! straight guide the result is the plain text shifted; on a circle the lines are concentric with
//! a spacing equal to the baseline distance of the shaped text.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use burnburn_generators::{Distort, GenError, PathTextParams, guide_frame_at, text_on_path, text_room};
use burnburn_geometry::{Path, Point, ProductionTolerance};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_text::{Align, FontData, FontSize, FontStyle, MemoryFontSource, TextObject, shape};
use burnburn_types::Length;

fn tol(mm: f64) -> ProductionTolerance {
    ProductionTolerance::new(Length::from_mm(mm).unwrap()).unwrap()
}

fn text(content: &str) -> (MemoryFontSource, TextObject) {
    let mut src = MemoryFontSource::new();
    let data = Arc::new(FontData::from_bytes(read_fixture("fonts", "LiberationSans-Regular.ttf").unwrap()).unwrap());
    let font = src.insert("Liberation Sans", FontStyle::Regular, data);
    (src, TextObject::new(content, font, FontSize::from_mm(Length::from_mm(10.0).unwrap()).unwrap()))
}

fn params(distort: Distort, start: f64, offset: f64, reverse: bool) -> PathTextParams {
    PathTextParams { distort, start, offset, reverse, tolerance: tol(0.01) }
}

fn area(p: &Path) -> f64 {
    p.subpaths().iter().map(|s| s.signed_area().unwrap_or(0.0).abs()).sum()
}

fn straight() -> Path {
    Path::line(Point::new(0.0, 0.0), Point::new(500.0, 0.0)).unwrap()
}

#[test]
fn two_lines_on_a_straight_guide_are_the_plain_text_shifted_by_the_start() {
    let (src, obj) = text("HI\nIH");
    let plain = shape(&obj, &src).unwrap();
    for distort in [Distort::Rigid, Distort::Warp] {
        let on = text_on_path(&obj, &src, &straight(), &params(distort, 10.0, 0.0, false)).unwrap();
        let (a, b) = (on.path.bounds().unwrap(), plain.outlines.bounds().unwrap());
        assert!((a.min().x - b.min().x - 10.0).abs() < 1e-6 && (a.min().y - b.min().y).abs() < 1e-6, "{distort:?}");
        assert!((a.max().x - b.max().x - 10.0).abs() < 1e-6 && (a.max().y - b.max().y).abs() < 1e-6, "{distort:?}");
        assert!((area(&on.path) - area(&plain.outlines)).abs() < 1e-6, "{distort:?}: same letters");
        assert_eq!(on.frames.len(), plain.glyphs.len(), "a frame per glyph, both lines");
    }
}

#[test]
fn centred_lines_keep_their_alignment_to_each_other() {
    let (src, mut obj) = text("H\nHHHH");
    obj.layout.align = Align::Center;
    let plain = shape(&obj, &src).unwrap();
    let x0 = plain.lines.iter().map(|l| l.start_x).fold(f64::INFINITY, f64::min);
    assert!(x0 < -1.0, "the wide line starts left of the alignment point: {x0}");
    let on = text_on_path(&obj, &src, &straight(), &params(Distort::Rigid, 20.0, 0.0, false)).unwrap();
    let (a, b) = (on.path.bounds().unwrap(), plain.outlines.bounds().unwrap());
    // The block's left edge (the widest line) starts at `start`; the short line stays centred over it.
    assert!((a.min().x - b.min().x - (20.0 - x0)).abs() < 1e-6, "{} vs {}", a.min().x, b.min().x);
    assert!((a.max().x - b.max().x - (20.0 - x0)).abs() < 1e-6);
}

#[test]
fn on_a_circle_the_lines_are_concentric_with_the_spacing_of_the_text() {
    let (src, obj) = text("HH\nHH");
    let plain = shape(&obj, &src).unwrap();
    let spacing = plain.lines[0].baseline_y - plain.lines[1].baseline_y;
    assert!(spacing > 5.0, "{spacing}");
    let circle = Path::circle(Point::new(0.0, 0.0), 60.0).unwrap();
    let on = text_on_path(&obj, &src, &circle, &params(Distort::Rigid, 0.0, 0.0, false)).unwrap();
    let radius = |i: usize| on.frames[i].centre.distance(Point::new(0.0, 0.0));
    let line_of = |i: usize| plain.glyphs[i].line;
    let (first, second): (Vec<usize>, Vec<usize>) = (0..on.frames.len()).partition(|i| line_of(*i) == 0);
    assert_eq!((first.len(), second.len()), (2, 2));
    for i in &first {
        assert!((radius(*i) - 60.0).abs() < 0.02, "the first line is on the guide: {}", radius(*i));
    }
    // The second line lies `spacing` further out or in, the same way for both its glyphs.
    let shifts: Vec<f64> = second.iter().map(|i| radius(*i) - 60.0).collect();
    for s in &shifts {
        assert!((s.abs() - spacing).abs() < 0.02, "{s} vs {spacing}");
        assert!(s.signum() == shifts[0].signum());
    }
}

#[test]
fn the_longest_line_decides_whether_a_multi_line_text_fits() {
    let (src, obj) = text("H\nHHHHHHHH");
    let plain = shape(&obj, &src).unwrap();
    let widest = plain.lines.iter().map(|l| l.width_mm).fold(0.0, f64::max);
    let guide = Path::line(Point::new(0.0, 0.0), Point::new(widest + 5.0, 0.0)).unwrap();
    assert!(text_on_path(&obj, &src, &guide, &params(Distort::Rigid, 4.0, 0.0, false)).is_ok());
    let err = text_on_path(&obj, &src, &guide, &params(Distort::Rigid, 6.0, 0.0, false)).unwrap_err();
    let GenError::TextLongerThanPath { needed_mm, available_mm } = err else { panic!("{err:?}") };
    assert!((needed_mm - (6.0 + widest)).abs() < 1e-6 && (available_mm - (widest + 5.0)).abs() < 1e-6);
}

#[test]
fn the_frame_of_the_guide_at_an_arc_length_follows_the_travel_direction() {
    let line = Path::line(Point::new(0.0, 0.0), Point::new(100.0, 0.0)).unwrap();
    let (p, d) = guide_frame_at(&line, &tol(0.01), false, 30.0).unwrap();
    assert!(p.distance(Point::new(30.0, 0.0)) < 1e-9 && (d.x - 1.0).abs() < 1e-9 && d.y.abs() < 1e-9);
    let (p, d) = guide_frame_at(&line, &tol(0.01), true, 30.0).unwrap();
    assert!(p.distance(Point::new(70.0, 0.0)) < 1e-9 && (d.x + 1.0).abs() < 1e-9, "reversed: from the other end");
    // A quarter of a circle of radius 50 further, the point is a quarter turn round.
    let circle = Path::circle(Point::new(0.0, 0.0), 50.0).unwrap();
    let quarter = std::f64::consts::FRAC_PI_2 * 50.0;
    let (p0, d0) = guide_frame_at(&circle, &tol(0.01), false, 0.0).unwrap();
    let (p1, d1) = guide_frame_at(&circle, &tol(0.01), false, quarter).unwrap();
    assert!(
        (p0.distance(Point::new(0.0, 0.0)) - 50.0).abs() < 0.02
            && (p1.distance(Point::new(0.0, 0.0)) - 50.0).abs() < 0.02
    );
    assert!((p0.x * p1.x + p0.y * p1.y).abs() / 2500.0 < 1e-3, "a right angle apart: {p0:?} {p1:?}");
    assert!((d0.x * d1.x + d0.y * d1.y).abs() < 0.01, "and so are the tangents");
    // A chord of a 0.01 mm flattening of radius 50 is 2 mm long: its tilt to the tangent is 0.02 rad.
    assert!((d0.x * p0.x + d0.y * p0.y).abs() / 50.0 < 0.03, "the tangent is across the radius");
    // Beyond the guide there is no frame.
    assert!(guide_frame_at(&line, &tol(0.01), false, 120.0).is_ok(), "the frame continues past the end");
}

#[test]
fn the_room_is_what_is_left_of_the_guide_after_the_widest_line() {
    let (src, obj) = text("HI\nHIHI");
    let plain = shape(&obj, &src).unwrap();
    let widest = plain.lines.iter().map(|l| l.width_mm).fold(0.0, f64::max);
    let guide = Path::line(Point::new(0.0, 0.0), Point::new(200.0, 0.0)).unwrap();
    let room = text_room(&obj, &src, &guide, &tol(0.01), false).unwrap();
    assert!((room - (200.0 - widest)).abs() < 1e-6, "{room}");
    // At the end of the room the text still fits, a hair past it does not.
    assert!(text_on_path(&obj, &src, &guide, &params(Distort::Rigid, room, 0.0, false)).is_ok());
    assert!(matches!(
        text_on_path(&obj, &src, &guide, &params(Distort::Rigid, room + 0.01, 0.0, false)),
        Err(GenError::TextLongerThanPath { .. })
    ));
    let short = Path::line(Point::new(0.0, 0.0), Point::new(widest - 1.0, 0.0)).unwrap();
    assert!(matches!(text_room(&obj, &src, &short, &tol(0.01), false), Err(GenError::TextLongerThanPath { .. })));
}
