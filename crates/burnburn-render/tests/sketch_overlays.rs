//! Sketch-mode overlays → packet: relation glyphs, freedom markers, dimensions with their digits
//! and dashed suggestions. Conflicts never rely on colour alone (X03): they add a cross.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use std::sync::Arc;

use burnburn_editor::{Camera, Overlay, PointFreedom, SceneSnapshot, SketchGlyph, SketchMark, Viewport};
use burnburn_geometry::{Path, Point};
use burnburn_render::{Primitive, Rgba, Theme, build_packet};
use burnburn_types::{DocumentId, RevisionId, StableId};

fn theme() -> Theme {
    let mut t = Theme::light();
    t.grid = None;
    t
}

/// 2 pt/mm, 200 × 200 pt, camera at the origin: world (x, y) is device (100 + 2x, 100 - 2y)?
/// Only the centre is relied on: the world point under the camera centre.
fn packet_of(overlays: Vec<Overlay>) -> Vec<Primitive> {
    let scene = SceneSnapshot {
        document: DocumentId::from_raw(1),
        revision: RevisionId::INITIAL,
        camera: Camera::new(Point::new(0.0, 0.0), 2.0).unwrap(),
        viewport: Viewport::new(200.0, 200.0, 1.0).unwrap(),
        items: Vec::new(),
        overlays,
    };
    build_packet(&scene, &theme()).primitives
}

fn strokes(p: &[Primitive]) -> Vec<(&Vec<[f32; 2]>, bool, Rgba)> {
    p.iter()
        .filter_map(|p| match p {
            Primitive::Stroke { points_px, closed, color, .. } => Some((points_px, *closed, *color)),
            _ => None,
        })
        .collect()
}

fn fills(p: &[Primitive]) -> Vec<(&Vec<Vec<[f32; 2]>>, Rgba)> {
    p.iter()
        .filter_map(|p| match p {
            Primitive::Fill { contours_px, color } => Some((contours_px, *color)),
            _ => None,
        })
        .collect()
}

fn glyph(glyph: SketchGlyph, mark: SketchMark) -> Overlay {
    Overlay::SketchGlyph { at: Point::new(0.0, 0.0), glyph, mark }
}

#[test]
fn a_glyph_is_drawn_in_the_sketch_colour_within_a_small_box_around_its_position() {
    let t = theme();
    for g in [
        SketchGlyph::Horizontal,
        SketchGlyph::Vertical,
        SketchGlyph::Parallel,
        SketchGlyph::Perpendicular,
        SketchGlyph::Equal,
        SketchGlyph::Tangent,
        SketchGlyph::Coincident,
        SketchGlyph::Fixed,
    ] {
        let primitives = packet_of(vec![glyph(g, SketchMark::Normal)]);
        let drawn: Vec<_> = strokes(&primitives).into_iter().filter(|(_, _, c)| *c == t.sketch).collect();
        assert!(!drawn.is_empty(), "{g:?} draws something in the sketch colour");
        for (points, _, _) in drawn {
            for [x, y] in points {
                assert!((x - 100.0).abs() <= 9.0 && (y - 100.0).abs() <= 9.0, "{g:?}: ({x}, {y}) is near (100, 100)");
            }
        }
    }
}

#[test]
fn the_glyphs_differ_from_each_other() {
    let shape = |g| {
        let p = packet_of(vec![glyph(g, SketchMark::Normal)]);
        format!("{:?}", strokes(&p).iter().map(|(pts, closed, _)| ((*pts).clone(), *closed)).collect::<Vec<_>>())
    };
    let all = [
        SketchGlyph::Horizontal,
        SketchGlyph::Vertical,
        SketchGlyph::Parallel,
        SketchGlyph::Perpendicular,
        SketchGlyph::Equal,
        SketchGlyph::Tangent,
        SketchGlyph::Coincident,
        SketchGlyph::Fixed,
    ];
    for (i, a) in all.iter().enumerate() {
        for b in &all[i + 1..] {
            assert_ne!(shape(*a), shape(*b), "{a:?} and {b:?} look alike");
        }
    }
}

#[test]
fn a_conflict_has_the_conflict_colour_and_a_cross_so_it_is_not_colour_only() {
    let t = theme();
    let normal = packet_of(vec![glyph(SketchGlyph::Horizontal, SketchMark::Normal)]);
    let conflict = packet_of(vec![glyph(SketchGlyph::Horizontal, SketchMark::Conflict)]);
    assert!(strokes(&conflict).iter().all(|(_, _, c)| *c == t.issue), "all of it in the conflict colour");
    assert!(strokes(&conflict).len() >= strokes(&normal).len() + 2, "the cross adds two strokes");
}

#[test]
fn a_selected_glyph_uses_the_selection_colour() {
    let t = theme();
    let selected = packet_of(vec![glyph(SketchGlyph::Vertical, SketchMark::Selected)]);
    assert!(!strokes(&selected).is_empty(), "something is drawn");
    assert!(strokes(&selected).iter().all(|(_, _, c)| *c == t.selection));
}

#[test]
fn freedom_shows_as_a_hollow_ring_a_filled_dot_and_a_pinned_triangle() {
    let at = Point::new(0.0, 0.0);
    let free = packet_of(vec![Overlay::SketchPoint { at, freedom: PointFreedom::Free, selected: false }]);
    let determined = packet_of(vec![Overlay::SketchPoint { at, freedom: PointFreedom::Determined, selected: false }]);
    let fixed = packet_of(vec![Overlay::SketchPoint { at, freedom: PointFreedom::Fixed, selected: false }]);
    assert!(fills(&free).is_empty() && strokes(&free).iter().any(|(_, closed, _)| *closed), "hollow ring");
    assert_eq!(fills(&determined).len(), 1, "filled dot");
    let triangle = fills(&fixed);
    assert_eq!(triangle.len(), 1);
    assert_eq!(triangle[0].0[0].len(), 3, "a triangle marks a fixed point");
}

fn dimension(text: &str, mark: SketchMark) -> Overlay {
    Overlay::SketchDimension {
        lines: vec![
            [Point::new(-20.0, 0.0), Point::new(-20.0, 10.0)],
            [Point::new(20.0, 0.0), Point::new(20.0, 10.0)],
            [Point::new(-20.0, 10.0), Point::new(20.0, 10.0)],
        ],
        label_at: Point::new(0.0, 10.0),
        text: text.to_owned(),
        driven: false,
        mark,
    }
}

#[test]
fn a_dimension_draws_its_lines_and_the_strokes_of_its_digits() {
    let t = theme();
    let lines_only = packet_of(vec![dimension("", SketchMark::Normal)]);
    let eight = packet_of(vec![dimension("8", SketchMark::Normal)]);
    let one = packet_of(vec![dimension("1", SketchMark::Normal)]);
    assert!(strokes(&lines_only).len() >= 3, "the three lines");
    assert!(strokes(&eight).iter().all(|(_, _, c)| *c == t.sketch_dimension));
    let digit_strokes = |p: &[Primitive]| strokes(p).len() - strokes(&lines_only).len();
    assert!(digit_strokes(&eight) > digit_strokes(&one), "an 8 has more strokes than a 1");
    assert!(digit_strokes(&one) >= 1);
    let label_box = |p: &[Primitive]| {
        strokes(p)
            .iter()
            .skip(strokes(&lines_only).len())
            .flat_map(|(pts, _, _)| pts.iter().copied())
            .fold((f32::MAX, f32::MIN), |(lo, hi), [x, _]| (lo.min(x), hi.max(x)))
    };
    let (lo, hi) = label_box(&eight);
    assert!(lo > 100.0 - 16.0 && hi < 100.0 + 16.0, "the digit sits around the label position: {lo}..{hi}");
}

#[test]
fn longer_numbers_are_wider_and_every_digit_has_its_own_shape() {
    let width = |text: &str| {
        let p = packet_of(vec![dimension(text, SketchMark::Normal)]);
        let s = strokes(&p);
        let xs: Vec<f32> = s.iter().skip(3).flat_map(|(pts, _, _)| pts.iter().map(|p| p[0])).collect();
        xs.iter().cloned().fold(f32::MIN, f32::max) - xs.iter().cloned().fold(f32::MAX, f32::min)
    };
    assert!(width("120") > width("12"));
    let shape = |d: char| format!("{:?}", strokes(&packet_of(vec![dimension(&d.to_string(), SketchMark::Normal)])));
    let digits: Vec<String> = "0123456789".chars().map(shape).collect();
    for (i, a) in digits.iter().enumerate() {
        for b in &digits[i + 1..] {
            assert_ne!(a, b);
        }
    }
    assert_ne!(shape('.'), shape('-'));
    assert_ne!(shape('R'), shape('\u{00D8}'));
}

#[test]
fn a_conflicting_dimension_gets_a_cross_too() {
    let t = theme();
    let normal = packet_of(vec![dimension("5", SketchMark::Normal)]);
    let conflict = packet_of(vec![dimension("5", SketchMark::Conflict)]);
    assert!(strokes(&conflict).iter().all(|(_, _, c)| *c == t.issue));
    assert!(strokes(&conflict).len() >= strokes(&normal).len() + 2);
}

#[test]
fn a_suggestion_is_dashed_and_so_different_from_an_applied_relation() {
    let t = theme();
    let applied = packet_of(vec![glyph(SketchGlyph::Horizontal, SketchMark::Normal)]);
    let suggested =
        packet_of(vec![Overlay::SketchSuggestion { at: Point::new(0.0, 0.0), glyph: SketchGlyph::Horizontal }]);
    let dashes = strokes(&suggested).iter().filter(|(_, _, c)| *c == t.preview).count();
    assert!(dashes >= 6, "a dashed frame is many short strokes: {dashes}");
    assert!(strokes(&suggested).len() > strokes(&applied).len());
}

#[test]
fn a_selected_or_conflicting_entity_is_drawn_heavier_than_a_plain_one() {
    let t = theme();
    let path = Arc::new(Path::line(Point::new(-10.0, 0.0), Point::new(10.0, 0.0)).unwrap());
    let packet = packet_of(vec![Overlay::SketchEntity { path, mark: SketchMark::Conflict }]);
    let widths: Vec<(f32, Rgba)> = packet
        .iter()
        .filter_map(|p| match p {
            Primitive::Stroke { width_px, color, .. } => Some((*width_px, *color)),
            _ => None,
        })
        .collect();
    assert_eq!(widths.len(), 1);
    assert!(widths[0].0 >= 2.0);
    assert_eq!(widths[0].1, t.issue);
}
