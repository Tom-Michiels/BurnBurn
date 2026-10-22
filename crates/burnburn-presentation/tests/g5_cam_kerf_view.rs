//! C03 (G5-cam): the side of the kerf is visible on the canvas. A selected shape on a Line layer
//! with kerf shows the path the beam really follows (half the kerf into the waste: outwards round
//! an outer contour, inwards in a hole), so the part keeps its nominal size and you can see it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_editor::Overlay;
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, SelectMode, UiIntent};
use burnburn_types::{DocumentId, GeometryId, OperationId};

fn rig() -> (HeadlessEditorHarness, DocumentId, GeometryId, OperationId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (50.0, 30.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let id = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    let op = h.document(doc).unwrap().operations[0].id;
    (h, doc, id, op)
}

fn set_kerf(h: &mut HeadlessEditorHarness, doc: DocumentId, op: OperationId, text: &str) {
    h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
    let mut input = h.document(doc).unwrap().cut_settings.unwrap().input();
    input.kerf = text.into();
    h.send(UiIntent::ApplyCutSettings { document: doc, operation: op, settings: input });
}

fn ghosts(h: &HeadlessEditorHarness, doc: DocumentId) -> Vec<((f64, f64), (f64, f64))> {
    h.scene(doc)
        .unwrap()
        .overlays
        .iter()
        .filter_map(|o| match o {
            Overlay::DragPreview { path, .. } => {
                path.bounds().map(|b| ((b.min().x, b.min().y), (b.max().x, b.max().y)))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_selected_shape_with_kerf_shows_the_path_the_beam_follows() {
    let (mut h, doc, _, op) = rig();
    assert!(ghosts(&h, doc).is_empty(), "no kerf, nothing to show");
    set_kerf(&mut h, doc, op, "0,2 mm");
    let g = ghosts(&h, doc);
    assert_eq!(g.len(), 1, "{g:?}");
    let ((x0, y0), (x1, y1)) = g[0];
    // The outer contour grows by half the kerf on every side.
    assert!((x0 - 9.9).abs() < 1e-6 && (y0 - 9.9).abs() < 1e-6, "{g:?}");
    assert!((x1 - 50.1).abs() < 1e-6 && (y1 - 30.1).abs() < 1e-6, "{g:?}");
}

#[test]
fn nothing_is_shown_when_the_shape_is_not_selected_or_the_kerf_is_removed() {
    let (mut h, doc, _, op) = rig();
    set_kerf(&mut h, doc, op, "0,2 mm");
    h.send(UiIntent::Select { document: doc, ids: vec![], mode: SelectMode::Replace });
    assert!(ghosts(&h, doc).is_empty(), "only the selection is shown");
    let id = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    assert_eq!(ghosts(&h, doc).len(), 1);
    set_kerf(&mut h, doc, op, "0");
    assert!(ghosts(&h, doc).is_empty());
}
