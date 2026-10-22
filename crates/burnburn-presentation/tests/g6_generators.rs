//! G6-editor (E10): the generator dialog for boxes and living hinges, headless. Parameters are
//! typed, the preview shows what the domain makes, Insert is one history entry, a generated object
//! opens again with its parameters, and the material thickness never turns into kerf.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_document::UndoLabel;
use burnburn_editor::Overlay;
use burnburn_presentation::{
    CommandKey, DocumentViewModel, GeneratorEdit, GeneratorField, GeneratorIssue, GeneratorKind, HeadlessEditorHarness,
    SelectMode, UiIntent,
};
use burnburn_types::{DocumentId, GeometryId};

fn open() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::GeneratorNew);
    (h, doc)
}

fn edit(h: &mut HeadlessEditorHarness, doc: DocumentId, edit: GeneratorEdit) {
    h.send(UiIntent::Generator { document: doc, edit });
}

fn field(h: &mut HeadlessEditorHarness, doc: DocumentId, field: GeneratorField, text: &str) {
    edit(h, doc, GeneratorEdit::Field(field, text.to_owned()));
}

fn vm(h: &HeadlessEditorHarness, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).unwrap()
}

fn text_of(h: &HeadlessEditorHarness, doc: DocumentId, f: GeneratorField) -> String {
    vm(h, doc).generator_dialog.unwrap().fields.iter().find(|r| r.field == f).unwrap().text.clone()
}

fn problem(h: &HeadlessEditorHarness, doc: DocumentId, f: GeneratorField) -> Option<GeneratorIssue> {
    vm(h, doc).generator_dialog.unwrap().problems.iter().find(|p| p.field == f).map(|p| p.issue)
}

fn object_ids(h: &HeadlessEditorHarness, doc: DocumentId) -> Vec<GeometryId> {
    vm(h, doc).objects.iter().map(|o| o.id).collect()
}

fn select(h: &mut HeadlessEditorHarness, doc: DocumentId, id: GeometryId) {
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
}

/// Width and height of the object `id` in the scene.
fn size_of(h: &HeadlessEditorHarness, doc: DocumentId, id: GeometryId) -> (f64, f64) {
    let scene = h.scene(doc).unwrap();
    let item = scene.items.iter().find(|i| i.source == id).unwrap();
    let b = item.path.bounds().unwrap();
    (b.width(), b.height())
}

#[test]
fn the_dialog_opens_with_a_box_previews_its_six_panels_and_cancel_changes_nothing() {
    let (mut h, doc) = open();
    let d = vm(&h, doc).generator_dialog.expect("open");
    assert_eq!(d.kind, GeneratorKind::FingerBox);
    assert!(!d.editing);
    assert_eq!(d.lid_open, Some(false));
    let preview = d.preview.expect("the defaults make a box");
    assert_eq!(preview.parts, 6);
    assert!(d.can_insert && d.problems.is_empty());
    let scene = h.scene(doc).unwrap();
    let shown = scene.overlays.iter().filter(|o| matches!(o, Overlay::DragPreview { .. })).count();
    assert_eq!(shown, 6, "the canvas shows the panels before they are inserted");
    let revision = vm(&h, doc).revision;
    edit(&mut h, doc, GeneratorEdit::Cancel);
    assert!(vm(&h, doc).generator_dialog.is_none());
    assert_eq!((vm(&h, doc).revision, vm(&h, doc).objects.len()), (revision, 0), "nothing was made");
    let scene = h.scene(doc).unwrap();
    assert!(scene.overlays.iter().all(|o| !matches!(o, Overlay::DragPreview { .. })), "and the preview is gone");
}

#[test]
fn inserting_a_box_makes_its_panels_in_one_undo_step() {
    let (mut h, doc) = open();
    field(&mut h, doc, GeneratorField::Width, "80");
    field(&mut h, doc, GeneratorField::Depth, "50 mm");
    field(&mut h, doc, GeneratorField::Height, "30");
    edit(&mut h, doc, GeneratorEdit::Insert);
    let v = vm(&h, doc);
    assert!(v.generator_dialog.is_none(), "the dialog is done");
    assert_eq!(v.objects.len(), 6, "six panels, ordinary objects of one generator");
    assert!(matches!(v.undo, Some(UndoLabel::Generator(_))), "{:?}", v.undo);
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(vm(&h, doc).objects.len(), 0, "one undo removes the whole box");
    h.invoke(doc, CommandKey::Redo);
    assert_eq!(vm(&h, doc).objects.len(), 6);
}

#[test]
fn an_open_lid_leaves_five_panels() {
    let (mut h, doc) = open();
    edit(&mut h, doc, GeneratorEdit::LidOpen(true));
    assert_eq!(vm(&h, doc).generator_dialog.unwrap().preview.unwrap().parts, 5);
    edit(&mut h, doc, GeneratorEdit::Insert);
    assert_eq!(vm(&h, doc).objects.len(), 5);
}

#[test]
fn each_refusal_is_named_at_its_field_and_nothing_is_inserted() {
    let (mut h, doc) = open();
    field(&mut h, doc, GeneratorField::Width, "veel");
    assert_eq!(problem(&h, doc, GeneratorField::Width), Some(GeneratorIssue::InvalidNumber));
    field(&mut h, doc, GeneratorField::Width, "100");
    field(&mut h, doc, GeneratorField::Height, "-5");
    assert!(
        matches!(problem(&h, doc, GeneratorField::Height), Some(GeneratorIssue::Negative | GeneratorIssue::NotPositive)),
        "{:?}",
        problem(&h, doc, GeneratorField::Height)
    );
    field(&mut h, doc, GeneratorField::Height, "40");
    // A finger no wider than the material cannot be cut: the limit is named, not clamped.
    field(&mut h, doc, GeneratorField::FingerWidth, "2");
    match problem(&h, doc, GeneratorField::FingerWidth) {
        Some(GeneratorIssue::TooSmall { min_mm, value_mm }) => assert!((min_mm - 3.0).abs() < 1e-9 && value_mm == 2.0),
        other => panic!("{other:?}"),
    }
    let d = vm(&h, doc).generator_dialog.unwrap();
    assert!(!d.can_insert && d.preview.is_none(), "{d:?}");
    assert_eq!(text_of(&h, doc, GeneratorField::FingerWidth), "2", "what was typed stays");
    let revision = vm(&h, doc).revision;
    edit(&mut h, doc, GeneratorEdit::Insert);
    assert_eq!(vm(&h, doc).revision, revision, "Insert does nothing while a field is refused");
    assert!(vm(&h, doc).generator_dialog.is_some());
}

#[test]
fn a_generated_object_opens_again_and_a_thickness_change_is_one_undo_with_the_same_ids() {
    let (mut h, doc) = open();
    edit(&mut h, doc, GeneratorEdit::Insert);
    let ids = object_ids(&h, doc);
    assert_eq!(ids.len(), 6);
    assert!(h.view().commands[&CommandKey::GeneratorEdit].enabled, "the new panels are selected, all of one generator");
    h.send(UiIntent::Select { document: doc, ids: vec![], mode: SelectMode::Replace });
    assert!(!h.view().commands[&CommandKey::GeneratorEdit].enabled, "nothing selected");
    select(&mut h, doc, ids[0]);
    assert!(h.view().commands[&CommandKey::GeneratorEdit].enabled);
    h.invoke(doc, CommandKey::GeneratorEdit);
    let d = vm(&h, doc).generator_dialog.expect("opened with its parameters");
    assert!(d.editing);
    assert_eq!(text_of(&h, doc, GeneratorField::Thickness), "3");
    assert_eq!(text_of(&h, doc, GeneratorField::Width), "100");
    let before: Vec<(f64, f64)> = ids.iter().map(|id| size_of(&h, doc, *id)).collect();
    field(&mut h, doc, GeneratorField::Thickness, "4");
    edit(&mut h, doc, GeneratorEdit::Insert);
    assert!(vm(&h, doc).generator_dialog.is_none());
    assert_eq!(object_ids(&h, doc), ids, "the panels keep their ids, so operations keep pointing at them");
    // The material thickness changes the joints, not the outer sizes: no kerf, no double kerf.
    let after: Vec<(f64, f64)> = ids.iter().map(|id| size_of(&h, doc, *id)).collect();
    assert_eq!(after, before);
    h.invoke(doc, CommandKey::Undo);
    select(&mut h, doc, ids[0]);
    h.invoke(doc, CommandKey::GeneratorEdit);
    assert_eq!(text_of(&h, doc, GeneratorField::Thickness), "3", "one undo went back to the old thickness");
    assert_eq!(object_ids(&h, doc).len(), 6, "the box was not undone with it");
}

#[test]
fn the_joints_follow_the_thickness_nominally() {
    // Bottom panel: 100 x 60 outer; its slots are as deep as the material is thick.
    let (mut h, doc) = open();
    edit(&mut h, doc, GeneratorEdit::Insert);
    let ids = object_ids(&h, doc);
    let scene = h.scene(doc).unwrap();
    let area = |id: GeometryId| scene.items.iter().find(|i| i.source == id).unwrap().path.signed_area().abs();
    let thin: Vec<f64> = ids.iter().map(|id| area(*id)).collect();
    select(&mut h, doc, ids[0]);
    h.invoke(doc, CommandKey::GeneratorEdit);
    field(&mut h, doc, GeneratorField::Thickness, "5");
    edit(&mut h, doc, GeneratorEdit::Insert);
    let scene = h.scene(doc).unwrap();
    let thick: Vec<f64> = ids.iter().map(|id| scene.items.iter().find(|i| i.source == *id).unwrap().path.signed_area().abs()).collect();
    assert!(thin.iter().zip(&thick).any(|(a, b)| (a - b).abs() > 1.0), "the joints changed: {thin:?} -> {thick:?}");
}

#[test]
fn a_living_hinge_is_one_object_and_a_too_short_one_is_refused_at_its_height() {
    let (mut h, doc) = open();
    edit(&mut h, doc, GeneratorEdit::Kind(GeneratorKind::LivingHinge));
    let d = vm(&h, doc).generator_dialog.unwrap();
    assert_eq!(d.kind, GeneratorKind::LivingHinge);
    assert_eq!(d.lid_open, None);
    assert!(d.preview.as_ref().unwrap().cuts > 10, "{:?}", d.preview);
    field(&mut h, doc, GeneratorField::Height, "3");
    assert!(matches!(problem(&h, doc, GeneratorField::Height), Some(GeneratorIssue::TooSmall { .. })));
    field(&mut h, doc, GeneratorField::Height, "40");
    edit(&mut h, doc, GeneratorEdit::Insert);
    assert_eq!(vm(&h, doc).objects.len(), 1, "every cut in one object");
}

/// C-8 keeps derived geometry from being edited directly, so moving a generated object edits its
/// placement: the whole box moves as one object (one undo step); one panel alone does not move.
#[test]
fn a_whole_generated_box_moves_through_its_placement_and_one_panel_alone_does_not() {
    use burnburn_presentation::FieldKey;
    let (mut h, doc) = open();
    edit(&mut h, doc, GeneratorEdit::Insert);
    let ids = object_ids(&h, doc);
    let min_x = |h: &HeadlessEditorHarness| {
        let scene = h.scene(doc).unwrap();
        ids.iter().map(|id| scene.items.iter().find(|i| i.source == *id).unwrap().path.bounds().unwrap().min().x).fold(f64::MAX, f64::min)
    };
    let before = min_x(&h);
    assert!(vm(&h, doc).selection.len() == 6, "the new box is selected");
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::X, text: "250".into() });
    assert!((min_x(&h) - 250.0).abs() < 1e-9, "{} -> {}", before, min_x(&h));
    assert_eq!(object_ids(&h, doc), ids, "the same objects moved");
    h.invoke(doc, CommandKey::Undo);
    assert!((min_x(&h) - before).abs() < 1e-9, "one undo step moves it back");
    // The dialog shows the same parameters after a move.
    h.invoke(doc, CommandKey::GeneratorEdit);
    assert_eq!(text_of(&h, doc, GeneratorField::Width), "100");
    edit(&mut h, doc, GeneratorEdit::Cancel);

    select(&mut h, doc, ids[0]);
    let revision = vm(&h, doc).revision;
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::X, text: "400".into() });
    assert_eq!(vm(&h, doc).revision, revision, "a single panel is regenerated from the box, not moved");
}

/// A generated object turns and moves as a whole, but is not scaled behind its parameters' back:
/// its size is its parameters (a barcode's module, a box's millimetres), edited in the dialog.
#[test]
fn a_generated_box_rotates_with_its_placement_but_is_not_scaled() {
    use burnburn_presentation::FieldKey;
    let (mut h, doc) = open();
    edit(&mut h, doc, GeneratorEdit::Insert);
    let ids = object_ids(&h, doc);
    let extent = |h: &HeadlessEditorHarness| {
        let scene = h.scene(doc).unwrap();
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for id in &ids {
            let b = scene.items.iter().find(|i| i.source == *id).unwrap().path.bounds().unwrap();
            (x0, y0, x1, y1) = (x0.min(b.min().x), y0.min(b.min().y), x1.max(b.max().x), y1.max(b.max().y));
        }
        (x1 - x0, y1 - y0)
    };
    let (w, hgt) = extent(&h);
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::Rotation, text: "90".into() });
    let (w2, h2) = extent(&h);
    assert!((w2 - hgt).abs() < 1e-6 && (h2 - w).abs() < 1e-6, "turned a quarter: {w}x{hgt} -> {w2}x{h2}");
    h.invoke(doc, CommandKey::GeneratorEdit);
    assert_eq!(text_of(&h, doc, GeneratorField::Width), "100", "the parameters did not change");
    edit(&mut h, doc, GeneratorEdit::Cancel);
    let revision = vm(&h, doc).revision;
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::ScaleX, text: "50".into() });
    assert_eq!(vm(&h, doc).revision, revision, "scaling is refused; the parameters are the size");
    assert_eq!(extent(&h), (w2, h2));
}

#[test]
fn only_a_generated_object_can_be_edited_as_a_generator() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (10.0, 10.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let id = object_ids(&h, doc)[0];
    select(&mut h, doc, id);
    assert!(!h.view().commands[&CommandKey::GeneratorEdit].enabled);
    h.invoke(doc, CommandKey::GeneratorEdit);
    assert!(vm(&h, doc).generator_dialog.is_none());
}
