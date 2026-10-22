//! C06 (G5-cam): hard order between layers from the window (Edit ▸ Cutting order…): "this layer
//! completely before that one". A cycle is refused naming the layers on it, the constraint beats
//! the list order in the compiled plan, and every change is one undo step.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use burnburn_document::UndoLabel;
use burnburn_document::process::ProcessLabel;
use burnburn_presentation::{
    CamDialog, CommandKey, DialogAction, DialogActionKind, DialogField, DialogNote, DialogValue, DialogViewModel,
    HeadlessEditorHarness, SelectMode, UiIntent,
};
use burnburn_types::{DocumentId, OperationId};

use common::{RUIDA, profile};

const ORDER: CamDialog = CamDialog::Order;

/// Three squares on layers C01, C02 and C03 (list order), left to right.
fn rig() -> (HeadlessEditorHarness, DocumentId, Vec<OperationId>) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    for (i, x) in [10.0, 100.0, 200.0].into_iter().enumerate() {
        h.invoke(doc, CommandKey::ToolRectangle);
        h.drag(doc, (x, 10.0), (x + 20.0, 30.0));
        h.invoke(doc, CommandKey::ToolSelect);
        let id = h.document(doc).unwrap().objects[i].id;
        h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
        h.send(UiIntent::AssignLayer { document: doc, slot: i as u8, add: false });
    }
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    let ops = h.document(doc).unwrap().operations.iter().map(|o| o.id).collect();
    (h, doc, ops)
}

fn open(h: &mut HeadlessEditorHarness, doc: DocumentId) -> DialogViewModel {
    h.send(UiIntent::OpenCamDialog { document: doc, dialog: ORDER });
    h.controller().cam_dialog(doc, ORDER).expect("the dialog is open")
}

fn vm(h: &mut HeadlessEditorHarness, doc: DocumentId) -> DialogViewModel {
    h.controller().cam_dialog(doc, ORDER).expect("the dialog is open")
}

fn choose(h: &mut HeadlessEditorHarness, doc: DocumentId, field: DialogField, index: usize) {
    h.send(UiIntent::SetCamField { document: doc, dialog: ORDER, field, value: DialogValue::Choice(index) });
}

fn press(h: &mut HeadlessEditorHarness, doc: DocumentId, action: DialogAction) {
    h.send(UiIntent::CamDialogAction { document: doc, dialog: ORDER, action });
}

fn existing(h: &mut HeadlessEditorHarness, doc: DocumentId) -> Vec<String> {
    vm(h, doc).row(DialogField::OrderExisting).unwrap().labels.iter().map(|l| l.text.clone()).collect()
}

/// The operations in the order the plan cuts them.
fn cut_order(h: &mut HeadlessEditorHarness, doc: DocumentId) -> Vec<OperationId> {
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    let plan = h.document(doc).unwrap().plan.unwrap();
    let mut order: Vec<OperationId> = Vec::new();
    for op in plan.preview.moves.iter().filter_map(|m| m.operation) {
        if !order.contains(&op) {
            order.push(op);
        }
    }
    order
}

#[test]
fn a_constraint_puts_one_layer_completely_before_another_in_the_plan() {
    let (mut h, doc, ops) = rig();
    assert_eq!(cut_order(&mut h, doc), ops, "list order without a constraint");
    let shown = open(&mut h, doc);
    assert_eq!(shown.row(DialogField::OrderBefore).unwrap().labels.len(), 3, "every layer can be picked");
    assert!(existing(&mut h, doc).is_empty());
    // C03 before C01.
    choose(&mut h, doc, DialogField::OrderBefore, 2);
    choose(&mut h, doc, DialogField::OrderAfter, 0);
    press(&mut h, doc, DialogAction::AddRow);
    assert_eq!(h.document(doc).unwrap().undo, Some(UndoLabel::Process(ProcessLabel::AddOrderConstraint)));
    assert_eq!(existing(&mut h, doc).len(), 1);
    // C03 before C01; C02 is free and keeps its place in the list order (stable ordering).
    assert_eq!(cut_order(&mut h, doc), vec![ops[1], ops[2], ops[0]], "the constraint beats the list order");
    // One undo takes it away again.
    h.invoke(doc, CommandKey::Undo);
    assert!(existing(&mut h, doc).is_empty());
    assert_eq!(cut_order(&mut h, doc), ops);
}

#[test]
fn a_cycle_is_refused_with_the_layers_on_it_and_nothing_changes() {
    let (mut h, doc, _) = rig();
    open(&mut h, doc);
    choose(&mut h, doc, DialogField::OrderBefore, 0);
    choose(&mut h, doc, DialogField::OrderAfter, 1);
    press(&mut h, doc, DialogAction::AddRow);
    let revision = h.document(doc).unwrap().revision;
    choose(&mut h, doc, DialogField::OrderBefore, 1);
    choose(&mut h, doc, DialogField::OrderAfter, 0);
    press(&mut h, doc, DialogAction::AddRow);
    assert_eq!(h.document(doc).unwrap().revision, revision, "the document is untouched");
    let shown = vm(&mut h, doc);
    assert!(shown.notes.contains(&DialogNote::OrderCycle));
    assert_eq!(shown.lines.len(), 2, "both layers of the cycle are named: {:?}", shown.lines);
    assert_eq!(existing(&mut h, doc).len(), 1);
}

#[test]
fn the_same_layer_twice_cannot_be_added_and_a_constraint_can_be_removed() {
    let (mut h, doc, _) = rig();
    open(&mut h, doc);
    choose(&mut h, doc, DialogField::OrderBefore, 1);
    choose(&mut h, doc, DialogField::OrderAfter, 1);
    let shown = vm(&mut h, doc);
    assert!(shown.notes.contains(&DialogNote::OrderSameLayer));
    assert!(!shown.action(DialogActionKind::AddRow).unwrap().enabled);

    choose(&mut h, doc, DialogField::OrderAfter, 2);
    press(&mut h, doc, DialogAction::AddRow);
    assert_eq!(existing(&mut h, doc).len(), 1);
    assert!(vm(&mut h, doc).action(DialogActionKind::RemoveRow).unwrap().enabled);
    press(&mut h, doc, DialogAction::RemoveRow);
    assert!(existing(&mut h, doc).is_empty());
    assert!(!vm(&mut h, doc).action(DialogActionKind::RemoveRow).unwrap().enabled, "nothing left to remove");
    assert_eq!(h.document(doc).unwrap().undo, Some(UndoLabel::Process(ProcessLabel::RemoveOrderConstraint)));
}

#[test]
fn a_duplicate_is_said_and_close_drops_the_dialog() {
    let (mut h, doc, _) = rig();
    open(&mut h, doc);
    choose(&mut h, doc, DialogField::OrderBefore, 0);
    choose(&mut h, doc, DialogField::OrderAfter, 2);
    press(&mut h, doc, DialogAction::AddRow);
    press(&mut h, doc, DialogAction::AddRow);
    assert!(vm(&mut h, doc).notes.contains(&DialogNote::OrderDuplicate));
    assert_eq!(existing(&mut h, doc).len(), 1);
    press(&mut h, doc, DialogAction::Close);
    assert!(h.controller().cam_dialog(doc, ORDER).is_none());
}
