//! LY01 (G5-cam): the tool layers T1 and T2 at the end of the colour palette. They hold guides and
//! registration marks: drawn in the window and kept in the project, never in a job; their output
//! cannot be switched on, and an object on one is not "forgotten".

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use burnburn_presentation::{
    CommandKey, DocumentViewModel, HeadlessEditorHarness, LAYER_SLOTS, MessageKey, PALETTE_SLOTS, SelectMode,
    TOOL_SLOT_T1, TOOL_SLOT_T2, UiIntent,
};
use burnburn_types::{DocumentId, GeometryId};

use common::{RUIDA, profile};

fn rig() -> (HeadlessEditorHarness, DocumentId, GeometryId, GeometryId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (40.0, 30.0));
    h.drag(doc, (60.0, 10.0), (90.0, 30.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let ids: Vec<GeometryId> = h.document(doc).unwrap().objects.iter().map(|o| o.id).collect();
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    (h, doc, ids[0], ids[1])
}

fn select(h: &mut HeadlessEditorHarness, doc: DocumentId, ids: &[GeometryId]) {
    h.send(UiIntent::Select { document: doc, ids: ids.to_vec(), mode: SelectMode::Replace });
}

fn view(h: &HeadlessEditorHarness, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).unwrap()
}

#[test]
fn the_palette_ends_with_t1_and_t2() {
    assert_eq!((LAYER_SLOTS, TOOL_SLOT_T1, TOOL_SLOT_T2, PALETTE_SLOTS), (30, 30, 31, 32));
}

#[test]
fn a_click_on_t1_makes_a_tool_layer_that_is_never_output() {
    let (mut h, doc, a, b) = rig();
    select(&mut h, doc, &[a]);
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    select(&mut h, doc, &[b]);
    h.send(UiIntent::AssignLayer { document: doc, slot: TOOL_SLOT_T1, add: false });
    let ops = view(&h, doc).operations;
    let tool = ops.iter().find(|o| o.slot == TOOL_SLOT_T1).expect("the T1 layer");
    assert!(tool.tool && !tool.output_enabled);
    assert_eq!(tool.name, "T1");
    assert_eq!(tool.targets, 1);

    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    let plan = view(&h, doc).plan.expect("a plan");
    assert_eq!(plan.findings.len(), 0, "the guide is neither forgotten nor outside anything: {:?}", plan.findings);
    let cut_operations: std::collections::BTreeSet<_> = plan.preview.moves.iter().filter_map(|m| m.operation).collect();
    assert_eq!(cut_operations.len(), 1, "only the ordinary layer is cut");
    assert!(!cut_operations.contains(&tool.id));
}

#[test]
fn output_cannot_be_switched_on_for_a_tool_layer() {
    let (mut h, doc, a, _) = rig();
    select(&mut h, doc, &[a]);
    h.send(UiIntent::AssignLayer { document: doc, slot: TOOL_SLOT_T2, add: false });
    let op = view(&h, doc).operations.into_iter().find(|o| o.slot == TOOL_SLOT_T2).unwrap();
    let revision = view(&h, doc).revision;
    h.send(UiIntent::SetOperationOutput { document: doc, operation: op.id, enabled: true });
    assert_eq!(view(&h, doc).revision, revision, "nothing changed");
    assert!(view(&h, doc).messages.iter().any(|m| m.key == MessageKey::ToolLayerNoOutput));
    assert!(!view(&h, doc).operations.iter().find(|o| o.id == op.id).unwrap().output_enabled);
    // It can still be hidden on the canvas.
    h.send(UiIntent::SetOperationVisible { document: doc, operation: op.id, visible: false });
    assert!(!view(&h, doc).operations.iter().find(|o| o.id == op.id).unwrap().visible);
}

#[test]
fn a_second_object_joins_the_tool_layer_and_leaving_it_is_one_undo_step() {
    let (mut h, doc, a, b) = rig();
    select(&mut h, doc, &[a]);
    h.send(UiIntent::AssignLayer { document: doc, slot: TOOL_SLOT_T1, add: false });
    select(&mut h, doc, &[b]);
    h.send(UiIntent::AssignLayer { document: doc, slot: TOOL_SLOT_T1, add: true });
    assert_eq!(view(&h, doc).operations.iter().find(|o| o.slot == TOOL_SLOT_T1).unwrap().targets, 2);
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(view(&h, doc).operations.iter().find(|o| o.slot == TOOL_SLOT_T1).unwrap().targets, 1);
}
