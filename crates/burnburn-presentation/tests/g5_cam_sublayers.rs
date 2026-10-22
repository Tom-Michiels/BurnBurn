//! C11 / LY01 (G5-cam): sub-layers from the window (Edit ▸ Sub-layers…): add, edit and remove the
//! sub-layers of a layer and put them in the order they run. Every change is one `SetAdvanced`
//! and one undo step; the compiled plan follows the list.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_test_support::host::rig as common;

use burnburn_document::UndoLabel;
use burnburn_document::process::ProcessLabel;
use burnburn_presentation::{
    CamDialog, CommandKey, DialogAction, DialogActionKind, DialogChoice, DialogError, DialogField, DialogNote,
    DialogValue, DialogViewModel, HeadlessEditorHarness, MoveKind, SelectMode, UiIntent,
};
use burnburn_types::DocumentId;

use common::{RUIDA, profile};

const SUB: CamDialog = CamDialog::Sublayers;

fn rig() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (50.0, 30.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let id = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    h.send(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    h.send(UiIntent::AssignLayer { document: doc, slot: 30, add: true }); // also on T1: never listed
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    (h, doc)
}

fn open(h: &mut HeadlessEditorHarness, doc: DocumentId) -> DialogViewModel {
    h.send(UiIntent::OpenCamDialog { document: doc, dialog: SUB });
    h.controller().cam_dialog(doc, SUB).expect("the dialog is open")
}

fn vm(h: &mut HeadlessEditorHarness, doc: DocumentId) -> DialogViewModel {
    h.controller().cam_dialog(doc, SUB).expect("the dialog is open")
}

fn press(h: &mut HeadlessEditorHarness, doc: DocumentId, action: DialogAction) {
    h.send(UiIntent::CamDialogAction { document: doc, dialog: SUB, action });
}

fn text(h: &mut HeadlessEditorHarness, doc: DocumentId, field: DialogField, value: &str) {
    h.send(UiIntent::SetCamField { document: doc, dialog: SUB, field, value: DialogValue::Text(value.into()) });
}

fn choose(h: &mut HeadlessEditorHarness, doc: DocumentId, field: DialogField, index: usize) {
    h.send(UiIntent::SetCamField { document: doc, dialog: SUB, field, value: DialogValue::Choice(index) });
}

fn mode(h: &mut HeadlessEditorHarness, doc: DocumentId, which: DialogChoice) {
    let index = DialogField::SubMode.choices().iter().position(|c| *c == which).unwrap();
    choose(h, doc, DialogField::SubMode, index);
}

/// The kinds of burning motion the plan has, in order, with repeats merged.
fn burn_kinds(h: &mut HeadlessEditorHarness, doc: DocumentId) -> Vec<MoveKind> {
    h.invoke(doc, CommandKey::Compile);
    h.run_tasks();
    let plan = h.document(doc).unwrap().plan.unwrap();
    let mut kinds: Vec<MoveKind> =
        plan.preview.moves.iter().map(|m| m.kind).filter(|k| matches!(k, MoveKind::Cut | MoveKind::Scan)).collect();
    kinds.dedup();
    kinds
}

fn count(vm: &DialogViewModel) -> usize {
    vm.row(DialogField::SubIndex).unwrap().labels.len()
}

#[test]
fn the_dialog_lists_only_layers_that_are_cut_and_starts_without_sub_layers() {
    let (mut h, doc) = rig();
    let shown = open(&mut h, doc);
    let layers: Vec<String> = shown.row(DialogField::SubLayer).unwrap().labels.iter().map(|l| l.text.clone()).collect();
    assert_eq!(layers, vec!["C01".to_owned()], "the tool layer T1 has no sub-layers");
    assert_eq!(count(&shown), 0);
    assert!(shown.notes.contains(&DialogNote::SublayersNone));
    assert!(shown.action(DialogActionKind::AddRow).unwrap().enabled);
    assert!(!shown.action(DialogActionKind::RemoveRow).unwrap().enabled);
    assert_eq!(burn_kinds(&mut h, doc), vec![MoveKind::Cut]);
}

#[test]
fn a_sub_layer_copies_its_layer_and_each_change_is_one_undo_step() {
    let (mut h, doc) = rig();
    open(&mut h, doc);
    let revision = h.document(doc).unwrap().revision;
    press(&mut h, doc, DialogAction::AddRow);
    assert_eq!(h.document(doc).unwrap().revision.raw(), revision.raw() + 1);
    assert_eq!(h.document(doc).unwrap().undo, Some(UndoLabel::Process(ProcessLabel::SetAdvanced)));
    let shown = vm(&mut h, doc);
    assert_eq!(count(&shown), 1);
    assert_eq!(shown.text(DialogField::SubSpeed), "100", "it starts as a copy of its layer");
    assert_eq!(shown.choice(DialogField::SubMode), Some(DialogChoice::SubLine));

    mode(&mut h, doc, DialogChoice::SubFill);
    assert_eq!(burn_kinds(&mut h, doc), vec![MoveKind::Cut, MoveKind::Scan]);
    text(&mut h, doc, DialogField::SubSpeed, "50");
    text(&mut h, doc, DialogField::SubInterval, "0,2");
    let shown = vm(&mut h, doc);
    assert_eq!((shown.text(DialogField::SubSpeed), shown.text(DialogField::SubInterval)), ("50", "0.2"));
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(vm(&mut h, doc).text(DialogField::SubInterval), "0.1", "back one step: the interval before 0,2");
}

#[test]
fn the_order_of_the_list_is_the_order_they_run_in() {
    let (mut h, doc) = rig();
    open(&mut h, doc);
    press(&mut h, doc, DialogAction::AddRow);
    mode(&mut h, doc, DialogChoice::SubFill);
    press(&mut h, doc, DialogAction::AddRow);
    // Two sub-layers: [Fill, Line]; the second one is chosen after adding.
    assert_eq!(count(&vm(&mut h, doc)), 2);
    assert_eq!(burn_kinds(&mut h, doc), vec![MoveKind::Cut, MoveKind::Scan, MoveKind::Cut]);
    choose(&mut h, doc, DialogField::SubIndex, 1);
    assert!(vm(&mut h, doc).action(DialogActionKind::MoveRowUp).unwrap().enabled);
    assert!(!vm(&mut h, doc).action(DialogActionKind::MoveRowDown).unwrap().enabled, "last in the list");
    press(&mut h, doc, DialogAction::MoveRowUp);
    assert_eq!(burn_kinds(&mut h, doc), vec![MoveKind::Cut, MoveKind::Scan], "the line cut now runs before the fill");
    assert_eq!(vm(&mut h, doc).choice(DialogField::SubMode), Some(DialogChoice::SubLine), "the moved one stays chosen");
}

#[test]
fn bad_values_are_said_on_their_row_and_send_nothing_and_remove_takes_one_away() {
    let (mut h, doc) = rig();
    open(&mut h, doc);
    press(&mut h, doc, DialogAction::AddRow);
    let revision = h.document(doc).unwrap().revision;
    text(&mut h, doc, DialogField::SubSpeed, "abc");
    text(&mut h, doc, DialogField::SubPower, "150");
    text(&mut h, doc, DialogField::SubPasses, "0");
    let shown = vm(&mut h, doc);
    assert_eq!(shown.row(DialogField::SubSpeed).unwrap().error, Some(DialogError::Invalid));
    assert_eq!(shown.row(DialogField::SubPower).unwrap().error, Some(DialogError::OutOfRange));
    assert_eq!(shown.row(DialogField::SubPasses).unwrap().error, Some(DialogError::OutOfRange));
    assert_eq!(h.document(doc).unwrap().revision, revision, "nothing was sent");

    press(&mut h, doc, DialogAction::RemoveRow);
    let shown = vm(&mut h, doc);
    assert_eq!(count(&shown), 0);
    assert!(shown.notes.contains(&DialogNote::SublayersNone));
    assert_eq!(h.document(doc).unwrap().undo, Some(UndoLabel::Process(ProcessLabel::SetAdvanced)));
}

#[test]
fn at_most_sixteen_sub_layers() {
    let (mut h, doc) = rig();
    open(&mut h, doc);
    for _ in 0..16 {
        press(&mut h, doc, DialogAction::AddRow);
    }
    let shown = vm(&mut h, doc);
    assert_eq!(count(&shown), 16);
    assert!(shown.notes.contains(&DialogNote::SublayersFull));
    assert!(!shown.action(DialogActionKind::AddRow).unwrap().enabled);
}
