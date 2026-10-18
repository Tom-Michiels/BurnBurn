//! What the modify/measure/text tools report back: a refused gesture is a status message with its
//! reason, a measurement is a readout in the view model, and a Text click anchors the next text.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use burnburn_editor::Modifiers;
use burnburn_presentation::{FieldKey, HeadlessEditorHarness, MessageKey, ToolMeasurement, ToolRejection, UiIntent};
use burnburn_test_support::fixtures::read_fixture;
use burnburn_text::{FontData, FontStyle, MemoryFontSource};
use burnburn_types::DocumentId;

fn window() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    (h, doc)
}

fn choose(h: &mut HeadlessEditorHarness, doc: DocumentId, id: &str) {
    h.send(UiIntent::SelectTool { document: Some(doc), id: id.to_owned() });
}

#[test]
fn a_refused_gesture_is_a_status_message_with_its_reason() {
    let (mut h, doc) = window();
    choose(&mut h, doc, "sketch.offset");
    let revision = h.document(doc).unwrap().revision;
    h.click(doc, (200.0, 200.0), Modifiers::default());
    let vm = h.document(doc).unwrap();
    assert_eq!(vm.revision, revision, "a refusal changes no document");
    assert!(
        vm.messages.iter().any(|m| m.key == MessageKey::ToolRejected(ToolRejection::NothingPicked)),
        "messages: {:?}",
        vm.messages
    );
}

#[test]
fn choosing_another_tool_clears_the_refusal() {
    let (mut h, doc) = window();
    choose(&mut h, doc, "sketch.offset");
    h.click(doc, (200.0, 200.0), Modifiers::default());
    choose(&mut h, doc, "select");
    assert!(h.document(doc).unwrap().messages.is_empty());
}

#[test]
fn the_dimension_tool_outside_a_sketch_reads_out_the_distance() {
    let (mut h, doc) = window();
    choose(&mut h, doc, "sketch.dimension");
    h.click(doc, (0.0, 0.0), Modifiers::default());
    h.click(doc, (30.0, 40.0), Modifiers::default());
    let vm = h.document(doc).unwrap();
    assert_eq!(
        vm.tool_measurement,
        Some(ToolMeasurement { distance_text: "50".into(), dx_text: "30".into(), dy_text: "40".into() })
    );
    choose(&mut h, doc, "select");
    assert_eq!(h.document(doc).unwrap().tool_measurement, None, "a tool choice clears the readout");
}

#[test]
fn text_is_placed_where_the_text_tool_was_clicked() {
    let (mut h, doc) = window();
    let mut src = MemoryFontSource::new();
    let data = Arc::new(FontData::from_bytes(read_fixture("fonts", "LiberationSans-Regular.ttf").unwrap()).unwrap());
    let font = src.insert("Liberation Sans", FontStyle::Regular, data);
    h.controller().attach_fonts(Arc::new(src), font);
    choose(&mut h, doc, "sketch.text");
    h.click(doc, (80.0, 60.0), Modifiers::default());
    h.send(UiIntent::InsertText { document: doc, text: "Hallo".into() });
    let vm = h.document(doc).unwrap();
    let x: f64 = vm.inspector.as_ref().unwrap().field(FieldKey::X).unwrap().text.parse().unwrap();
    let y: f64 = vm.inspector.as_ref().unwrap().field(FieldKey::Y).unwrap().text.parse().unwrap();
    assert!((75.0..90.0).contains(&x), "text starts at the clicked x, got {x}");
    assert!((55.0..75.0).contains(&y), "text sits at the clicked y, got {y}");
}
