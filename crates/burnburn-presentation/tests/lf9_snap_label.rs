//! LF9: the snap target under the pointer gets a label in the view model (`MessageKey::Snap`), and
//! the selection grip under the pointer is marked in the scene.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

use burnburn_editor::{Handle, Overlay, ScaleGrip, SnapKind};
use burnburn_geometry::Point;
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, MessageKey, SelectMode, UiIntent};
use burnburn_types::DocumentId;

fn window_with_rectangle() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    h.invoke(doc, CommandKey::SnapToObjects); // the fixture must not snap to earlier objects
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.invoke(doc, CommandKey::SnapToObjects);
    (h, doc)
}

#[test]
fn a_snap_target_gives_a_label_in_the_view_model() {
    let (mut h, doc) = window_with_rectangle();
    h.invoke(doc, CommandKey::ToolRectangle);
    assert_eq!(h.document(doc).unwrap().snap_label, None, "nothing under the pointer yet");
    h.hover(doc, (40.8, 20.6));
    let vm = h.document(doc).unwrap();
    assert_eq!(vm.snap.map(|s| s.kind), Some(SnapKind::Endpoint));
    assert_eq!(vm.snap_label, Some(MessageKey::Snap(SnapKind::Endpoint)));
    h.hover(doc, (200.0, 150.0));
    let vm = h.document(doc).unwrap();
    assert_eq!((vm.snap, vm.snap_label), (None, None), "the label goes with the target");
}

#[test]
fn the_grip_under_the_pointer_is_marked_in_the_scene() {
    let (mut h, doc) = window_with_rectangle();
    h.invoke(doc, CommandKey::ToolSelect);
    let id = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    let hovered = |h: &HeadlessEditorHarness| -> Vec<(Handle, Point)> {
        h.scene(doc)
            .unwrap()
            .overlays
            .iter()
            .filter_map(|o| match o {
                Overlay::Handle { handle, at, hovered: true, .. } => Some((*handle, *at)),
                _ => None,
            })
            .collect()
    };
    assert!(hovered(&h).is_empty());
    // Right edge midpoint of the rectangle (0,0)-(40,20).
    h.hover(doc, (40.0, 10.0));
    assert_eq!(hovered(&h), vec![(Handle::Scale(ScaleGrip::Right), Point::new(40.0, 10.0))]);
    h.hover(doc, (20.0, 10.0));
    assert!(hovered(&h).is_empty(), "moved off the grip");
}
