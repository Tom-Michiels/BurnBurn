//! G8-trace: the overlays of the tracer on the canvas. A computed result that is not applied yet
//! (`ResultPreview`: centre lines, boolean preview) was never drawn by the renderer; and the two
//! variants of the A/B comparison (`CompareOutline`) each have a look of their own.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use std::sync::Arc;

use burnburn_editor::{Camera, CompareSide, Overlay, SceneSnapshot, Viewport};
use burnburn_geometry::{Path, Point, Rect};
use burnburn_render::{Primitive, Rgba, Theme, build_packet};
use burnburn_types::{DocumentId, Length, RevisionId, StableId};

fn theme() -> Theme {
    let mut t = Theme::light();
    t.grid = None;
    t
}

fn camera() -> Camera {
    Camera::new(Point::new(0.0, 0.0), 2.0).unwrap()
}

fn viewport() -> Viewport {
    Viewport::new(200.0, 200.0, 1.0).unwrap()
}

fn packet_of(overlays: Vec<Overlay>) -> Vec<Primitive> {
    let scene = SceneSnapshot {
        document: DocumentId::from_raw(1),
        revision: RevisionId::INITIAL,
        camera: camera(),
        viewport: viewport(),
        items: Vec::new(),
        overlays,
    };
    build_packet(&scene, &theme()).primitives
}

fn strokes(p: &[Primitive]) -> Vec<(&Vec<[f32; 2]>, bool, Rgba, f32)> {
    p.iter()
        .filter_map(|p| match p {
            Primitive::Stroke { points_px, closed, color, width_px } => Some((points_px, *closed, *color, *width_px)),
            _ => None,
        })
        .collect()
}

/// A 10 × 10 mm square at the origin.
fn square() -> Arc<Path> {
    let rect = Rect::new(Point::new(0.0, 0.0), Length::from_mm(10.0).unwrap(), Length::from_mm(10.0).unwrap()).unwrap();
    Arc::new(Path::rectangle(rect))
}

/// Whether `points` has a vertex within a quarter pixel of where the camera puts world point `p`.
fn has_vertex_at(points: &[[f32; 2]], p: Point) -> bool {
    let [x, y] = camera().world_to_points(&viewport(), p);
    points.iter().any(|q| (f64::from(q[0]) - x).abs() < 0.25 && (f64::from(q[1]) - y).abs() < 0.25)
}

#[test]
fn a_result_preview_is_drawn_as_an_outline_in_the_preview_colour_in_the_right_place() {
    let t = theme();
    let packet = packet_of(vec![Overlay::ResultPreview { path: square() }]);
    let s = strokes(&packet);
    assert_eq!(s.len(), 1, "one outline: {packet:?}");
    let (points, closed, color, width) = &s[0];
    assert!(*closed);
    assert_eq!(*color, t.preview);
    assert!(*width > 0.0);
    for corner in [Point::new(0.0, 0.0), Point::new(10.0, 0.0), Point::new(10.0, 10.0), Point::new(0.0, 10.0)] {
        assert!(has_vertex_at(points, corner), "{corner:?} in {points:?}");
    }
}

#[test]
fn an_open_result_preview_stays_open() {
    let line = Path::line(Point::new(0.0, 0.0), Point::new(10.0, 5.0)).unwrap();
    let packet = packet_of(vec![Overlay::ResultPreview { path: Arc::new(line) }]);
    let s = strokes(&packet);
    assert_eq!(s.len(), 1);
    assert!(!s[0].1, "a centre line is a stroke, not a ring");
}

#[test]
fn the_two_variants_of_a_comparison_have_looks_of_their_own() {
    let t = theme();
    assert_ne!(t.compare_a, t.compare_b);
    assert_ne!(t.compare_b, t.background);
    let packet = packet_of(vec![
        Overlay::CompareOutline { path: square(), side: CompareSide::A },
        Overlay::CompareOutline { path: square(), side: CompareSide::B },
    ]);
    let s = strokes(&packet);
    assert_eq!(s.len(), 2);
    assert_eq!(s[0].2, t.compare_a);
    assert_eq!(s[1].2, t.compare_b);
    assert!(s.iter().all(|(points, closed, ..)| *closed && has_vertex_at(points, Point::new(10.0, 10.0))));
}

#[test]
fn a_protected_outline_is_heavier_and_in_the_selection_colour() {
    let t = theme();
    let packet =
        packet_of(vec![Overlay::ResultPreview { path: square() }, Overlay::ProtectedOutline { path: square() }]);
    let s = strokes(&packet);
    assert_eq!(s.len(), 2);
    let (plain, protected) = (&s[0], &s[1]);
    assert_eq!(plain.2, t.preview);
    assert_eq!(protected.2, t.selection);
    assert!(protected.3 > plain.3, "{} against {}", protected.3, plain.3);
    assert!(protected.1 && has_vertex_at(protected.0, Point::new(10.0, 0.0)));
}
