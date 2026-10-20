//! Optimization Settings dialog and cut-order priority (W3-UI-cam, C06): the dialog edits a draft,
//! Apply is one `SetOptimization` (one undo step), a field outside its range keeps the dialog open
//! and sends nothing, and the priority field of the inspector is one `SetPriority` per shape.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use burnburn_document::UndoLabel;
use burnburn_document::process::{OrderKey, ProcessLabel};
use burnburn_presentation::{
    CamDialog, CommandKey, DialogAction, DialogActionKind, DialogError, DialogField, DialogNote, DialogValue,
    DialogViewModel, FieldKey, HeadlessEditorHarness, MessageKey, SelectMode, UiIntent,
};
use burnburn_types::{DocumentId, GeometryId};

use common::{RUIDA, profile};

const OPT: CamDialog = CamDialog::Optimization;

/// Two rectangles, each on its own layer (colours 1 and 2): returns the document and the shapes.
fn two_layers(h: &mut HeadlessEditorHarness) -> (DocumentId, [GeometryId; 2]) {
    let doc = h.new_document().expect("window");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (40.0, 20.0));
    h.drag(doc, (100.0, 0.0), (140.0, 20.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let ids: Vec<GeometryId> = h.document(doc).unwrap().objects.iter().map(|o| o.id).collect();
    for (id, slot) in ids.iter().zip([1, 2]) {
        h.send(UiIntent::Select { document: doc, ids: vec![*id], mode: SelectMode::Replace });
        h.send(UiIntent::AssignLayer { document: doc, slot, add: false });
    }
    (doc, [ids[0], ids[1]])
}

fn open(h: &mut HeadlessEditorHarness, doc: DocumentId) -> DialogViewModel {
    h.send(UiIntent::OpenCamDialog { document: doc, dialog: OPT });
    h.controller().cam_dialog(doc, OPT).expect("dialog open")
}

fn dialog(h: &mut HeadlessEditorHarness, doc: DocumentId) -> Option<DialogViewModel> {
    h.controller().cam_dialog(doc, OPT)
}

fn set(h: &mut HeadlessEditorHarness, doc: DocumentId, field: DialogField, value: DialogValue) {
    h.send(UiIntent::SetCamField { document: doc, dialog: OPT, field, value });
}

fn press(h: &mut HeadlessEditorHarness, doc: DocumentId, action: DialogAction) {
    h.send(UiIntent::CamDialogAction { document: doc, dialog: OPT, action });
}

fn revision(h: &HeadlessEditorHarness, doc: DocumentId) -> u64 {
    h.document(doc).unwrap().revision.raw()
}

#[test]
fn opening_shows_the_documents_current_settings() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, _) = two_layers(&mut h);
    let before = revision(&h, doc);
    let vm = open(&mut h, doc);
    assert_eq!(
        vm.order(),
        vec![(OrderKey::Layer, true), (OrderKey::Group, false), (OrderKey::Priority, false)],
        "the stored default is Layer only; the other levels are listed switched off"
    );
    for field in
        [DialogField::InnerFirst, DialogField::ReduceTravel, DialogField::BestStart, DialogField::BestDirection]
    {
        assert!(vm.flag(field), "{field:?} is on in the default planner");
    }
    assert!(!vm.flag(DialogField::HideBacklash));
    assert!(!vm.flag(DialogField::RemoveOverlapping));
    assert_eq!(vm.text(DialogField::OverlapTolerance), "0.1");
    assert!(matches!(
        vm.row(DialogField::OverlapTolerance).unwrap().state,
        burnburn_presentation::FieldState::Disabled(None)
    ));
    // Nothing happened to the document by opening it.
    assert_eq!(revision(&h, doc), before);
}

#[test]
fn apply_is_one_set_optimization_with_the_order_of_the_list() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, _) = two_layers(&mut h);
    let before = revision(&h, doc);
    open(&mut h, doc);
    set(&mut h, doc, DialogField::OrderEnabled(OrderKey::Priority), DialogValue::Flag(true));
    // Priority is third; two steps up put it before Layer.
    press(&mut h, doc, DialogAction::MoveUp(OrderKey::Priority));
    press(&mut h, doc, DialogAction::MoveUp(OrderKey::Priority));
    let vm = dialog(&mut h, doc).expect("still open");
    assert_eq!(vm.order()[0], (OrderKey::Priority, true));
    assert_eq!(revision(&h, doc), before, "the draft is not a document change");

    press(&mut h, doc, DialogAction::Apply);
    assert!(dialog(&mut h, doc).is_none(), "Apply closes the dialog");
    assert_eq!(revision(&h, doc), before + 1, "one command");
    let stored = h.document(doc).unwrap();
    assert_eq!(stored.undo, Some(UndoLabel::Process(ProcessLabel::SetOptimization)));
    let reopened = open(&mut h, doc);
    assert_eq!(
        reopened.order(),
        vec![(OrderKey::Priority, true), (OrderKey::Layer, true), (OrderKey::Group, false)],
        "Priority → Layer, as listed"
    );

    // One undo brings the previous settings back.
    press(&mut h, doc, DialogAction::Close);
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(revision(&h, doc), before + 2, "undo is a revision of its own");
    let after_undo = open(&mut h, doc);
    assert_eq!(after_undo.order()[0], (OrderKey::Layer, true));
}

#[test]
fn switches_and_the_overlap_tolerance_reach_the_stored_settings() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, _) = two_layers(&mut h);
    open(&mut h, doc);
    set(&mut h, doc, DialogField::InnerFirst, DialogValue::Flag(false));
    set(&mut h, doc, DialogField::HideBacklash, DialogValue::Flag(true));
    set(&mut h, doc, DialogField::RemoveOverlapping, DialogValue::Flag(true));
    set(&mut h, doc, DialogField::OverlapTolerance, DialogValue::Text("0,25".into()));
    press(&mut h, doc, DialogAction::Apply);
    let vm = open(&mut h, doc);
    assert!(!vm.flag(DialogField::InnerFirst));
    assert!(vm.flag(DialogField::HideBacklash));
    assert!(vm.flag(DialogField::RemoveOverlapping));
    assert_eq!(vm.text(DialogField::OverlapTolerance), "0.25");
}

#[test]
fn a_tolerance_outside_its_range_keeps_the_dialog_open_and_sends_nothing() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, _) = two_layers(&mut h);
    let before = revision(&h, doc);
    open(&mut h, doc);
    set(&mut h, doc, DialogField::RemoveOverlapping, DialogValue::Flag(true));
    for (text, why) in [("6", DialogError::OutOfRange), ("abc", DialogError::Invalid), ("0", DialogError::Invalid)] {
        set(&mut h, doc, DialogField::OverlapTolerance, DialogValue::Text(text.into()));
        press(&mut h, doc, DialogAction::Apply);
        let vm = dialog(&mut h, doc).expect("a refused Apply leaves the dialog open");
        assert_eq!(vm.row(DialogField::OverlapTolerance).unwrap().error, Some(why), "{text:?}");
        assert_eq!(vm.text(DialogField::OverlapTolerance), text, "what the user typed stays");
        assert_eq!(revision(&h, doc), before, "{text:?} sends no command");
    }
    // Editing the field clears its error.
    set(&mut h, doc, DialogField::OverlapTolerance, DialogValue::Text("1".into()));
    assert_eq!(dialog(&mut h, doc).unwrap().row(DialogField::OverlapTolerance).unwrap().error, None);
}

#[test]
fn an_unchanged_dialog_applies_nothing_and_cancel_drops_the_draft() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, _) = two_layers(&mut h);
    let before = revision(&h, doc);
    open(&mut h, doc);
    press(&mut h, doc, DialogAction::Apply);
    assert!(dialog(&mut h, doc).is_none());
    assert_eq!(revision(&h, doc), before, "equal settings are not a command");

    open(&mut h, doc);
    set(&mut h, doc, DialogField::ReduceTravel, DialogValue::Flag(false));
    press(&mut h, doc, DialogAction::Close);
    assert_eq!(revision(&h, doc), before);
    assert!(open(&mut h, doc).flag(DialogField::ReduceTravel), "a fresh draft starts from the document");
}

#[test]
fn moving_the_first_level_up_or_the_last_down_changes_nothing() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, _) = two_layers(&mut h);
    let vm = open(&mut h, doc);
    assert_eq!(vm.row(DialogField::OrderEnabled(OrderKey::Layer)).unwrap().reorder, Some((false, true)));
    assert_eq!(vm.row(DialogField::OrderEnabled(OrderKey::Priority)).unwrap().reorder, Some((true, false)));
    press(&mut h, doc, DialogAction::MoveUp(OrderKey::Layer));
    press(&mut h, doc, DialogAction::MoveDown(OrderKey::Priority));
    assert_eq!(dialog(&mut h, doc).unwrap().order(), vm.order());
}

/// The order decides the plan: Layer→Priority and Priority→Layer are different plans of the same
/// document (the hash names the plan, so the preview, the RD file and the upload follow it).
#[test]
fn the_order_by_list_changes_the_compiled_plan() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, [first, second]) = two_layers(&mut h);
    h.send(UiIntent::SetMachineProfile { profile: Box::new(profile(1)), ruida: RUIDA });
    // The first shape (layer 1) is cut last by priority.
    h.send(UiIntent::Select { document: doc, ids: vec![first], mode: SelectMode::Replace });
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::Priority, text: "5".into() });
    h.send(UiIntent::Select { document: doc, ids: vec![second], mode: SelectMode::Replace });
    let _ = second;

    let plan_with = |h: &mut HeadlessEditorHarness, order: &[OrderKey]| {
        open(h, doc);
        for key in [OrderKey::Layer, OrderKey::Group, OrderKey::Priority] {
            set(h, doc, DialogField::OrderEnabled(key), DialogValue::Flag(order.contains(&key)));
        }
        // Bring the wanted order about by moving each level to its place from the end.
        for key in order.iter().rev() {
            for _ in 0..3 {
                press(h, doc, DialogAction::MoveUp(*key));
            }
        }
        press(h, doc, DialogAction::Apply);
        h.invoke(doc, CommandKey::Compile);
        h.run_tasks();
        h.document(doc).unwrap().plan.expect("compiled").id
    };
    let layer_first = plan_with(&mut h, &[OrderKey::Layer, OrderKey::Priority]);
    let priority_first = plan_with(&mut h, &[OrderKey::Priority, OrderKey::Layer]);
    assert_ne!(layer_first, priority_first);
}

#[test]
fn the_dialog_says_when_priorities_exist_but_priority_is_not_a_level() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, [first, _]) = two_layers(&mut h);
    assert!(!open(&mut h, doc).notes.contains(&DialogNote::PriorityNeedsOrderLevel), "no priorities, no note");
    press(&mut h, doc, DialogAction::Close);
    h.send(UiIntent::Select { document: doc, ids: vec![first], mode: SelectMode::Replace });
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::Priority, text: "3".into() });
    let vm = open(&mut h, doc);
    assert!(vm.notes.contains(&DialogNote::PriorityNeedsOrderLevel));
    set(&mut h, doc, DialogField::OrderEnabled(OrderKey::Priority), DialogValue::Flag(true));
    assert!(!dialog(&mut h, doc).unwrap().notes.contains(&DialogNote::PriorityNeedsOrderLevel));
    assert!(vm.action(DialogActionKind::Apply).is_some_and(|a| a.enabled));
}

// ---- the priority field of the inspector -------------------------------------------------------

fn select(h: &mut HeadlessEditorHarness, doc: DocumentId, ids: &[GeometryId]) {
    h.send(UiIntent::Select { document: doc, ids: ids.to_vec(), mode: SelectMode::Replace });
}

fn priority_text(h: &HeadlessEditorHarness, doc: DocumentId) -> Option<String> {
    h.document(doc).unwrap().inspector.and_then(|i| i.field(FieldKey::Priority).map(|f| f.text.clone()))
}

#[test]
fn the_priority_field_is_one_set_priority_with_one_undo_step() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, [first, _]) = two_layers(&mut h);
    select(&mut h, doc, &[first]);
    assert_eq!(priority_text(&h, doc).as_deref(), Some("0"), "the default priority is shown");
    let before = revision(&h, doc);
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::Priority, text: "5".into() });
    assert_eq!(revision(&h, doc), before + 1);
    assert_eq!(priority_text(&h, doc).as_deref(), Some("5"));
    assert_eq!(h.document(doc).unwrap().undo, Some(UndoLabel::Process(ProcessLabel::SetPriority)));
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(priority_text(&h, doc).as_deref(), Some("0"));
}

#[test]
fn an_invalid_or_unchanged_priority_sends_nothing() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, [first, _]) = two_layers(&mut h);
    select(&mut h, doc, &[first]);
    let before = revision(&h, doc);
    for text in ["20000", "-10001", "1,5", "abc", ""] {
        h.send(UiIntent::CommitField { document: doc, field: FieldKey::Priority, text: text.into() });
        assert_eq!(revision(&h, doc), before, "{text:?}");
        let field = h.document(doc).unwrap().inspector.unwrap().field(FieldKey::Priority).cloned().unwrap();
        assert!(matches!(field.error, Some(MessageKey::InvalidLength(_))), "{text:?} is said next to the field");
    }
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::Priority, text: "0".into() });
    assert_eq!(revision(&h, doc), before, "the stored value again is not a command");
    // The edges are valid.
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::Priority, text: "-10000".into() });
    assert_eq!(priority_text(&h, doc).as_deref(), Some("-10000"));
}

#[test]
fn a_mixed_selection_shows_an_empty_priority_and_sets_every_shape() {
    let mut h = HeadlessEditorHarness::new();
    let (doc, [first, second]) = two_layers(&mut h);
    select(&mut h, doc, &[first]);
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::Priority, text: "2".into() });
    select(&mut h, doc, &[first, second]);
    assert_eq!(priority_text(&h, doc).as_deref(), Some(""), "the two shapes differ");
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::Priority, text: "7".into() });
    assert_eq!(priority_text(&h, doc).as_deref(), Some("7"));
    select(&mut h, doc, &[second]);
    assert_eq!(priority_text(&h, doc).as_deref(), Some("7"));
}
