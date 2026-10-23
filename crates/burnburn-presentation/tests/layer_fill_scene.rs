//! LFB-rest: the scene tells the canvas which objects an operation fills (Fill / Offset Fill), by
//! layer slot; line-only objects and hidden layers carry no fill. Headless, through the controller.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_presentation::{CommandKey, CutSettingsInput, HeadlessEditorHarness, OperationKind, SelectMode, UiIntent};
use burnburn_types::{DocumentId, GeometryId, OperationId};

fn two_rectangles(h: &mut HeadlessEditorHarness) -> (DocumentId, GeometryId, GeometryId) {
    let doc = h.new_document().expect("window");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.drag(doc, (60.0, 0.0), (100.0, 20.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let ids: Vec<_> = h.document(doc).unwrap().objects.iter().map(|o| o.id).collect();
    (doc, ids[0], ids[1])
}

fn assign(h: &mut HeadlessEditorHarness, doc: DocumentId, id: GeometryId, slot: u8) -> OperationId {
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    h.send(UiIntent::AssignLayer { document: doc, slot, add: false });
    h.document(doc).unwrap().operations.iter().find(|o| o.slot == slot).expect("layer").id
}

fn set_mode(h: &mut HeadlessEditorHarness, doc: DocumentId, op: OperationId, mode: OperationKind) {
    h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
    let mut input: CutSettingsInput = h.document(doc).unwrap().cut_settings.expect("sheet").input();
    input.mode = mode;
    h.send(UiIntent::ApplyCutSettings { document: doc, operation: op, settings: input });
}

fn fill_slots(h: &HeadlessEditorHarness, doc: DocumentId) -> Vec<(GeometryId, Option<u16>)> {
    h.scene(doc).unwrap().items.iter().map(|i| (i.source, i.fill_slot)).collect()
}

#[test]
fn only_objects_of_a_fill_or_offset_fill_operation_carry_a_fill_slot() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, a, b) = two_rectangles(&mut h);
    let op_a = assign(&mut h, doc, a, 3);
    let _op_b = assign(&mut h, doc, b, 5);
    assert_eq!(fill_slots(&h, doc), vec![(a, None), (b, None)], "line operations do not fill");

    set_mode(&mut h, doc, op_a, OperationKind::Fill);
    assert_eq!(fill_slots(&h, doc), vec![(a, Some(3)), (b, None)], "the Fill operation of slot 3 fills a");
}

#[test]
fn a_hidden_fill_layer_leaves_no_fill_and_a_second_line_layer_keeps_the_fill() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, a, _) = two_rectangles(&mut h);
    let op = assign(&mut h, doc, a, 3);
    set_mode(&mut h, doc, op, OperationKind::Fill);
    // A second, line-only treatment (Shift-click) on the same geometry: the fill stays in the colour of the fill layer.
    h.send(UiIntent::Select { document: doc, ids: vec![a], mode: SelectMode::Replace });
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: true });
    assert_eq!(fill_slots(&h, doc)[0], (a, Some(3)));
    // Hiding the fill layer removes the fill.
    h.send(UiIntent::OpenCutSettings { document: doc, operation: op });
    let mut input = h.document(doc).unwrap().cut_settings.unwrap().input();
    input.visible = false;
    h.send(UiIntent::ApplyCutSettings { document: doc, operation: op, settings: input });
    assert_eq!(fill_slots(&h, doc)[0], (a, None));
}
