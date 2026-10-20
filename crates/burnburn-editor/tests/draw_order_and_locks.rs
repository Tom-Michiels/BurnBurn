//! W2-L1-bewerken: the canvas follows the document's draw order, locks and hidden flags.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use burnburn_document::{ArrangeCommand, CommandPayload, DocumentCommand, DocumentSession, OrderChange};
use burnburn_editor::{Camera, SceneRequest, Selection, ToolKind, Viewport, build_scene, hit_test, objects_inside};
use burnburn_geometry::{Bounds, Point, Rect};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn run(s: &mut DocumentSession, payload: CommandPayload) -> burnburn_document::CommandOutcome {
    s.apply(DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload })
        .unwrap()
}

fn rect(s: &mut DocumentSession, x: f64, y: f64, w: f64, h: f64) -> GeometryId {
    let rect = Rect::new(Point::new(x, y), mm(w), mm(h)).unwrap();
    run(s, CommandPayload::CreateRectangle { rect, name: "r".into() }).changes.created[0]
}

fn arrange(s: &mut DocumentSession, c: ArrangeCommand) {
    run(s, CommandPayload::Arrange(c));
}

/// Two overlapping squares sharing the same interior point (5, 5): `a` created first.
fn overlapping() -> (DocumentSession, GeometryId, GeometryId) {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let a = rect(&mut s, 0.0, 0.0, 10.0, 10.0);
    let b = rect(&mut s, 2.0, 2.0, 10.0, 10.0);
    (s, a, b)
}

#[test]
fn a_click_inside_two_equal_candidates_picks_the_one_drawn_on_top() {
    let (mut s, a, b) = overlapping();
    // Interiors of identical size compete equally; the topmost wins (b is created later).
    assert_eq!(hit_test(&s.snapshot(), Point::new(5.0, 5.0), 0.1)[0], b);
    arrange(&mut s, ArrangeCommand::Order { targets: vec![a], change: OrderChange::ToFront });
    assert_eq!(hit_test(&s.snapshot(), Point::new(5.0, 5.0), 0.1)[0], a, "pushed to the front, it is picked first");
}

#[test]
fn locked_and_hidden_objects_are_not_hit_and_not_caught_by_a_rubber_band() {
    let (mut s, a, b) = overlapping();
    arrange(&mut s, ArrangeCommand::SetLocked { targets: vec![b], locked: true });
    assert_eq!(hit_test(&s.snapshot(), Point::new(5.0, 5.0), 0.1), vec![a], "the click falls through a locked object");
    arrange(&mut s, ArrangeCommand::SetHidden { targets: vec![a], hidden: true });
    assert!(hit_test(&s.snapshot(), Point::new(5.0, 5.0), 0.1).is_empty());
    let all = Bounds::from_corners(Point::new(-1.0, -1.0), Point::new(20.0, 20.0));
    assert!(objects_inside(&s.snapshot(), all).is_empty());
}

#[test]
fn a_rubber_band_returns_objects_in_draw_order() {
    let (mut s, a, b) = overlapping();
    arrange(&mut s, ArrangeCommand::Order { targets: vec![a], change: OrderChange::ToFront });
    let all = Bounds::from_corners(Point::new(-1.0, -1.0), Point::new(20.0, 20.0));
    assert_eq!(objects_inside(&s.snapshot(), all), vec![b, a]);
}

#[test]
fn the_scene_draws_back_to_front_in_draw_order_and_leaves_hidden_objects_out() {
    let (mut s, a, b) = overlapping();
    let c = rect(&mut s, 30.0, 0.0, 5.0, 5.0);
    arrange(&mut s, ArrangeCommand::Order { targets: vec![a], change: OrderChange::ToFront });
    arrange(&mut s, ArrangeCommand::SetHidden { targets: vec![c], hidden: true });
    let snapshot = s.snapshot();
    let selection = Selection::default();
    let scene = build_scene(&SceneRequest {
        document: s.id(),
        snapshot: &snapshot,
        selection: &selection,
        tool: ToolKind::Select,
        preview: None,
        snap: None,
        issues: &[],
        extra: &[],
        result_preview: None,
        camera: Camera::new(Point::new(0.0, 0.0), 2.0).unwrap(),
        viewport: Viewport::new(800.0, 600.0, 1.0).unwrap(),
    });
    let drawn: Vec<GeometryId> = scene.items.iter().map(|i| i.source).collect();
    assert_eq!(drawn, vec![b, a]);
}
