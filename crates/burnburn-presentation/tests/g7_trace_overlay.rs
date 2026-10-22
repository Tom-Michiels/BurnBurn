//! G7-trace, T04: the centre lines of the open tracer are drawn on the canvas as strokes, where
//! applying them would put them, so the user judges the trace against the sheet. The overlay is a
//! window aid: it follows the result in force, is gone when the tracer closes, and is never
//! document data.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod g7_support;

use burnburn_editor::Overlay;
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, TraceMode};
use burnburn_types::DocumentId;
use g7_support::*;

/// A black bar of 40 × 6 px on white (64 × 16 px) at 0.1 mm per pixel: 4.0 × 0.6 mm.
fn bar() -> Vec<u8> {
    image(64, 16, |x, y| if (10..50).contains(&x) && (5..11).contains(&y) { 0 } else { 255 })
}

fn strokes_on_canvas(h: &HeadlessEditorHarness, doc: DocumentId) -> Vec<std::sync::Arc<burnburn_geometry::Path>> {
    h.scene(doc)
        .expect("a scene")
        .overlays
        .iter()
        .filter_map(|o| match o {
            Overlay::ResultPreview { path } => Some(std::sync::Arc::clone(path)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_centre_lines_are_on_the_canvas_as_open_strokes_where_applying_would_put_them() {
    let (mut h, doc) = opened_with(&bar());
    assert!(strokes_on_canvas(&h, doc).is_empty(), "a contour trace is not drawn on the canvas");
    edit(&mut h, doc, |f| f.mode = TraceMode::Centerline);
    let on_canvas = strokes_on_canvas(&h, doc);
    assert_eq!(on_canvas.len(), 1, "one overlay for the centre lines");
    let path = &on_canvas[0];
    assert_eq!(path.subpaths().len(), 1, "one stroke");
    assert!(!path.subpaths()[0].is_closed(), "a stroke is open, not an outline");
    let b = path.bounds().unwrap();
    // The bar is 4.0 mm long and 0.6 mm thick: its centre line runs along y = (16 - 8) × 0.1 = 0.8 mm.
    assert!((b.min().y - 0.8).abs() < 0.12 && (b.max().y - 0.8).abs() < 0.12, "{b:?}");
    assert!(b.width() > 3.0 && b.width() < 4.1 && b.min().x > 0.9 && b.max().x < 5.1, "{b:?}");
    // It is the very path of the panel's preview: what is judged is what is applied.
    let shown = tracer(&h, doc).result.expect("a result").preview;
    assert_eq!(**path, *shown);
    // And applying puts the strokes exactly there (identity placement): the item on the sheet has
    // the very same geometry as the overlay.
    h.invoke(doc, CommandKey::ApplyTrace);
    let scene = h.scene(doc).expect("a scene");
    let item = scene.items.last().expect("the applied strokes");
    assert_eq!(item.transform, burnburn_geometry::Affine::IDENTITY);
    assert_eq!(*item.path, **path);
}

#[test]
fn the_overlay_is_gone_with_the_tracer_or_in_contour_mode_and_adds_nothing_to_the_document() {
    let (mut h, doc) = opened_with(&bar());
    edit(&mut h, doc, |f| f.mode = TraceMode::Centerline);
    assert_eq!(strokes_on_canvas(&h, doc).len(), 1);
    let revision = vm(&h, doc).revision;
    edit(&mut h, doc, |f| f.mode = TraceMode::Contour);
    assert!(strokes_on_canvas(&h, doc).is_empty());
    edit(&mut h, doc, |f| f.mode = TraceMode::Centerline);
    assert_eq!(strokes_on_canvas(&h, doc).len(), 1);
    h.invoke(doc, CommandKey::CloseTracer);
    assert!(strokes_on_canvas(&h, doc).is_empty());
    assert_eq!(vm(&h, doc).revision, revision, "looking never edits");
}

#[test]
fn a_trace_of_several_strokes_is_one_overlay_with_all_of_them() {
    // A plus sign: one junction in the middle that has to be resolved.
    let plus = image(64, 64, |x, y| {
        if ((28..36).contains(&x) && (8..56).contains(&y)) || ((8..56).contains(&x) && (28..36).contains(&y)) {
            0
        } else {
            255
        }
    });
    let (mut h, doc) = opened_with(&plus);
    edit(&mut h, doc, |f| f.mode = TraceMode::Centerline);
    let before = strokes_on_canvas(&h, doc);
    assert_eq!(before.len(), 1);
    assert!(before[0].subpaths().len() >= 2, "a plus sign has more than one stroke");
    let shown = tracer(&h, doc).result.expect("a result").preview;
    assert_eq!(*before[0], *shown, "also with several strokes at once");
}
