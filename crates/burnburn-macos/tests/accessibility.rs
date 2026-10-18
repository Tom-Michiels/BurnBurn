//! IM20 accessibility, headless part: canvas objects are accessibility elements with the object's
//! name, its physical size and its frame in canvas points (VoiceOver focus ring), from the same
//! scene the GPU draws. The AppKit layer only wraps these in `NSAccessibilityElement`s.

#![allow(clippy::unwrap_used)]

use burnburn_geometry::Point;
use burnburn_macos::accessibility::canvas_elements;
use burnburn_macos::l10n::Locale;
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, SelectMode, UiIntent};

#[test]
fn canvas_objects_are_elements_with_name_size_and_frame_in_points() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 20.0), (130.0, 100.0));
    h.invoke(doc, CommandKey::ToolEllipse);
    h.drag(doc, (200.0, 0.0), (250.0, 30.0));
    let vm = h.view();
    let scene = h.scene(doc).unwrap();
    let d = vm.document(doc).unwrap();

    let elements = canvas_elements(&scene, &vm, doc, Locale::Nl);
    assert_eq!(elements.len(), 2);
    let rect = elements.iter().find(|e| e.label == "Rechthoek 1").expect("rectangle element");
    assert_eq!(rect.value, "120 × 80 mm");
    // Independent oracle: the camera maps the world corners; y up in the world, y down on screen.
    let top_left = d.camera.world_to_points(&d.viewport, Point::new(10.0, 100.0));
    let bottom_right = d.camera.world_to_points(&d.viewport, Point::new(130.0, 20.0));
    let expected = [top_left[0], top_left[1], bottom_right[0] - top_left[0], bottom_right[1] - top_left[1]];
    for (got, want) in rect.frame_pt.iter().zip(expected) {
        assert!((got - want).abs() < 1e-6, "frame {:?} vs {expected:?}", rect.frame_pt);
    }
    assert!((rect.frame_pt[2] - 240.0).abs() < 1e-6, "120 mm at 2 pt/mm");
    let ellipse = elements.iter().find(|e| e.label.starts_with("Ellips")).expect("ellipse element");
    assert_eq!(ellipse.value, "50 × 30 mm");
    // Selection is announced, not only drawn in blue (X03).
    assert!(ellipse.selected && !rect.selected);

    h.send(UiIntent::Select { document: doc, ids: vec![rect.object], mode: SelectMode::Replace });
    let english = canvas_elements(&h.scene(doc).unwrap(), &h.view(), doc, Locale::En);
    assert!(english.iter().any(|e| e.object == rect.object && e.selected));
}

#[test]
fn an_empty_canvas_has_no_elements() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    assert!(canvas_elements(&h.scene(doc).unwrap(), &h.view(), doc, Locale::Nl).is_empty());
}
