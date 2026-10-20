//! Warp/Deform handles and Tangent Circle contacts → packet: square markers in the selection
//! colour, the dragged one larger, an invalid set in the problem colour (never colour alone: the
//! marker stays the same shape, the status line says why).

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use burnburn_editor::{Camera, Overlay, SceneSnapshot, Viewport};
use burnburn_geometry::Point;
use burnburn_render::{Primitive, Rgba, Theme, build_packet};
use burnburn_types::{DocumentId, RevisionId, StableId};

fn theme() -> Theme {
    let mut t = Theme::light();
    t.grid = None;
    t
}

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

fn handle(active: bool, invalid: bool) -> Overlay {
    Overlay::ReshapeHandle { at: Point::new(0.0, 0.0), index: 0, active, invalid }
}

/// Width in device pixels of the first closed stroke (the marker's outline).
fn marker_width(p: &[Primitive]) -> f32 {
    p.iter()
        .find_map(|p| match p {
            Primitive::Stroke { points_px, closed: true, .. } => {
                let xs = points_px.iter().map(|q| q[0]);
                Some(xs.clone().fold(f32::MIN, f32::max) - xs.fold(f32::MAX, f32::min))
            }
            _ => None,
        })
        .expect("a marker is drawn")
}

fn stroke_colours(p: &[Primitive]) -> Vec<Rgba> {
    p.iter()
        .filter_map(|p| match p {
            Primitive::Stroke { color, .. } => Some(*color),
            _ => None,
        })
        .collect()
}

#[test]
fn a_handle_is_a_marker_in_the_selection_colour() {
    let t = theme();
    let p = packet_of(vec![handle(false, false)]);
    assert!(marker_width(&p) > 2.0);
    assert!(!stroke_colours(&p).is_empty());
    assert!(stroke_colours(&p).iter().all(|c| *c == t.selection));
}

#[test]
fn the_dragged_handle_is_larger() {
    assert!(marker_width(&packet_of(vec![handle(true, false)])) > marker_width(&packet_of(vec![handle(false, false)])));
}

#[test]
fn an_invalid_set_of_handles_uses_the_problem_colour() {
    let t = theme();
    let p = packet_of(vec![handle(false, true)]);
    assert!(!stroke_colours(&p).is_empty());
    assert!(stroke_colours(&p).iter().all(|c| *c == t.issue));
}

#[test]
fn a_tangent_contact_is_a_small_marker_in_the_snap_colour() {
    let t = theme();
    let p = packet_of(vec![Overlay::TangentContact { at: Point::new(0.0, 0.0) }]);
    assert!(!stroke_colours(&p).is_empty());
    assert!(stroke_colours(&p).iter().all(|c| *c == t.snap));
    assert!(marker_width(&p) < marker_width(&packet_of(vec![handle(false, false)])));
}

fn circle_path() -> std::sync::Arc<burnburn_geometry::Path> {
    let shape = burnburn_geometry::Shape::Circle {
        center: Point::new(0.0, 0.0),
        radius: burnburn_types::Length::from_mm(10.0).unwrap(),
    };
    std::sync::Arc::new(shape.to_path().unwrap())
}

fn stroke_widths(p: &[Primitive]) -> Vec<f32> {
    p.iter()
        .filter_map(|p| match p {
            Primitive::Stroke { width_px, .. } => Some(*width_px),
            _ => None,
        })
        .collect()
}

#[test]
fn the_chosen_tangent_solution_is_heavier_and_in_the_selection_colour() {
    let t = theme();
    let chosen = packet_of(vec![Overlay::TangentCandidate { path: circle_path(), chosen: true }]);
    let other = packet_of(vec![Overlay::TangentCandidate { path: circle_path(), chosen: false }]);
    assert!(!stroke_colours(&chosen).is_empty() && !stroke_colours(&other).is_empty());
    assert!(stroke_colours(&chosen).iter().all(|c| *c == t.selection));
    assert!(stroke_colours(&other).iter().all(|c| *c == t.preview));
    let (a, b) = (stroke_widths(&chosen), stroke_widths(&other));
    assert!(a.iter().cloned().fold(0.0, f32::max) > b.iter().cloned().fold(0.0, f32::max));
}
