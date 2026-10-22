//! G8-trace, T04: the centre lines on the canvas carry node markers, so the user sees where the
//! strokes begin, end and bend before applying them. The open ends are drawn as selected (larger,
//! hollow) markers, the nodes between them as plain ones. Window aids, like the strokes themselves.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod g7_support;

use burnburn_editor::{NodeTarget, Overlay};
use burnburn_geometry::Point;
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, TraceMode};
use burnburn_types::DocumentId;
use g7_support::*;

/// A black bar of 40 × 6 px on white (64 × 16 px) at 0.1 mm per pixel.
fn bar() -> Vec<u8> {
    image(64, 16, |x, y| if (10..50).contains(&x) && (5..11).contains(&y) { 0 } else { 255 })
}

/// An "L" of two bars, 6 px thick: a vertical one (x 10…16, y 4…44) and a horizontal one (x 10…50,
/// y 38…44), on 64 × 48 px.
fn ell() -> Vec<u8> {
    image(64, 48, |x, y| {
        let vertical = (10..16).contains(&x) && (4..44).contains(&y);
        let horizontal = (10..50).contains(&x) && (38..44).contains(&y);
        if vertical || horizontal { 0 } else { 255 }
    })
}

fn nodes(h: &HeadlessEditorHarness, doc: DocumentId) -> Vec<(Point, bool)> {
    h.scene(doc)
        .expect("a scene")
        .overlays
        .iter()
        .filter_map(|o| match o {
            Overlay::Node { at, target: NodeTarget::Node(_), selected } => Some((*at, *selected)),
            _ => None,
        })
        .collect()
}

/// The nodes of the centre lines in force: (position, is an open end).
fn expected(h: &HeadlessEditorHarness, doc: DocumentId) -> Vec<(Point, bool)> {
    let preview = tracer(h, doc).result.expect("a result").preview;
    let mut out = Vec::new();
    for sub in preview.subpaths() {
        let ends: Vec<Point> = std::iter::once(sub.start()).chain(sub.segments().iter().map(|s| s.end())).collect();
        let last = ends.len() - 1;
        for (i, p) in ends.iter().enumerate() {
            out.push((*p, !sub.is_closed() && (i == 0 || i == last)));
        }
    }
    out
}

fn sorted(mut v: Vec<(Point, bool)>) -> Vec<(i64, i64, bool)> {
    let mut out: Vec<(i64, i64, bool)> =
        v.drain(..).map(|(p, e)| ((p.x * 1e6).round() as i64, (p.y * 1e6).round() as i64, e)).collect();
    out.sort_unstable();
    out
}

#[test]
fn a_stroke_has_a_marker_on_each_of_its_nodes_and_its_open_ends_stand_out() {
    let (mut h, doc) = opened_with(&bar());
    assert!(nodes(&h, doc).is_empty(), "a contour trace has no centre lines to mark");
    edit(&mut h, doc, |f| f.mode = TraceMode::Centerline);
    let got = nodes(&h, doc);
    assert!(got.len() >= 2, "at least both ends: {got:?}");
    assert_eq!(sorted(got.clone()), sorted(expected(&h, doc)), "one marker per node of the strokes, at the node");
    assert_eq!(got.iter().filter(|(_, end)| *end).count(), 2, "the two ends of the bar");
}

#[test]
fn a_bend_is_a_plain_marker_between_the_two_ends() {
    let (mut h, doc) = opened_with(&ell());
    edit(&mut h, doc, |f| f.mode = TraceMode::Centerline);
    let got = nodes(&h, doc);
    assert_eq!(sorted(got.clone()), sorted(expected(&h, doc)));
    let ends = got.iter().filter(|(_, end)| *end).count();
    let plain = got.iter().filter(|(_, end)| !*end).count();
    assert_eq!(ends, 2, "the L has two open ends");
    assert!(plain >= 1, "and a bend between them: {got:?}");
    // The corner of the L is near (1.3, 0.6) mm in the picture's frame (x 13 px, 48 − 41 px).
    assert!(
        got.iter().any(|(p, end)| !*end && (p.x - 1.3).abs() < 0.3 && (p.y - 0.7).abs() < 0.3),
        "a marker at the bend: {got:?}"
    );
}

#[test]
fn the_markers_go_with_the_centre_lines() {
    let (mut h, doc) = opened_with(&bar());
    edit(&mut h, doc, |f| f.mode = TraceMode::Centerline);
    assert!(!nodes(&h, doc).is_empty());
    edit(&mut h, doc, |f| f.mode = TraceMode::Contour);
    assert!(nodes(&h, doc).is_empty());
    edit(&mut h, doc, |f| f.mode = TraceMode::Centerline);
    h.invoke(doc, CommandKey::CloseTracer);
    assert!(nodes(&h, doc).is_empty());
    assert!(h.document(doc).unwrap().objects.is_empty(), "never document data");
}
