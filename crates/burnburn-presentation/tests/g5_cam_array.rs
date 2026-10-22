//! AR02 (G5-cam): the array dialog (Arrange ▸ Array…) with grid, circular and along-a-path
//! arrays. The dialog is a draft with a live preview on the canvas; the document changes only on
//! Apply, as one modifier chain and one undo step.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod edit_rig;

use burnburn_editor::Overlay;
use burnburn_geometry::{Path, Point};
use burnburn_presentation::{
    CamDialog, DialogAction, DialogActionKind, DialogChoice, DialogError, DialogField, DialogNote, DialogValue,
    DialogViewModel, FieldState, HeadlessEditorHarness, UiIntent,
};
use burnburn_types::{DocumentId, GeometryId};
use edit_rig::{opened_with, select, square, view};

const ARRAY: CamDialog = CamDialog::Array;

fn rig() -> (HeadlessEditorHarness, DocumentId, Vec<GeometryId>) {
    // A 10 mm square at the origin and a 60 mm path along x for the path array.
    let pad = Path::polyline(&[Point::new(0.0, 0.0), Point::new(60.0, 0.0)], false).unwrap();
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), pad]);
    select(&mut h, doc, &ids[..1]);
    (h, doc, ids)
}

fn open(h: &mut HeadlessEditorHarness, doc: DocumentId) -> DialogViewModel {
    h.send(UiIntent::OpenCamDialog { document: doc, dialog: ARRAY });
    h.controller().cam_dialog(doc, ARRAY).expect("the dialog is open")
}

fn vm(h: &mut HeadlessEditorHarness, doc: DocumentId) -> DialogViewModel {
    h.controller().cam_dialog(doc, ARRAY).expect("the dialog is open")
}

fn text(h: &mut HeadlessEditorHarness, doc: DocumentId, field: DialogField, value: &str) {
    h.send(UiIntent::SetCamField { document: doc, dialog: ARRAY, field, value: DialogValue::Text(value.into()) });
}

fn flag(h: &mut HeadlessEditorHarness, doc: DocumentId, field: DialogField, on: bool) {
    h.send(UiIntent::SetCamField { document: doc, dialog: ARRAY, field, value: DialogValue::Flag(on) });
}

fn kind(h: &mut HeadlessEditorHarness, doc: DocumentId, which: DialogChoice) {
    let index = DialogField::ArrayKind.choices().iter().position(|c| *c == which).expect("an array kind");
    h.send(UiIntent::SetCamField {
        document: doc,
        dialog: ARRAY,
        field: DialogField::ArrayKind,
        value: DialogValue::Choice(index),
    });
}

fn apply(h: &mut HeadlessEditorHarness, doc: DocumentId) {
    h.send(UiIntent::CamDialogAction { document: doc, dialog: ARRAY, action: DialogAction::Apply });
}

fn previews(h: &HeadlessEditorHarness, doc: DocumentId) -> usize {
    h.scene(doc).unwrap().overlays.iter().filter(|o| matches!(o, Overlay::DragPreview { .. })).count()
}

/// The object the array made: the one id that was not there before.
fn made(h: &HeadlessEditorHarness, doc: DocumentId, before: &[GeometryId]) -> GeometryId {
    let after: Vec<GeometryId> = view(h, doc).objects.iter().map(|o| o.id).collect();
    let new: Vec<GeometryId> = after.into_iter().filter(|id| !before.contains(id)).collect();
    assert_eq!(new.len(), 1, "exactly one object was made: {new:?}");
    new[0]
}

fn bounds_of(h: &HeadlessEditorHarness, doc: DocumentId, id: GeometryId) -> ((f64, f64), (f64, f64)) {
    let scene = h.scene(doc).unwrap();
    let b = scene.items.iter().find(|i| i.source == id).unwrap().path.bounds().unwrap();
    ((b.min().x, b.min().y), (b.max().x, b.max().y))
}

#[test]
fn a_grid_array_is_a_preview_until_apply_and_then_one_modifier_with_one_undo_step() {
    let (mut h, doc, ids) = rig();
    let opened = open(&mut h, doc);
    assert_eq!(opened.dialog, ARRAY);
    assert_eq!(opened.choice(DialogField::ArrayKind), Some(DialogChoice::ArrayGrid));
    // Defaults leave a gap between copies: pitch = object size + 5 mm.
    assert_eq!(opened.text(DialogField::GridPitchX), "15");
    assert_eq!(opened.text(DialogField::GridPitchY), "15");

    text(&mut h, doc, DialogField::GridColumns, "4");
    text(&mut h, doc, DialogField::GridRows, "2");
    text(&mut h, doc, DialogField::GridPitchX, "25");
    text(&mut h, doc, DialogField::GridPitchY, "18");
    let revision = view(&h, doc).revision;
    let live = vm(&mut h, doc);
    // 8 copies, the original stays where it is: 7 ghosts on the canvas, the document untouched.
    assert_eq!(previews(&h, doc), 7);
    assert_eq!(view(&h, doc).revision, revision, "editing the dialog changes no document");
    assert!(live.notes.contains(&DialogNote::ArrayExtent { copies: 8, width_um: 85_000, height_um: 28_000 }));

    let before: Vec<GeometryId> = view(&h, doc).objects.iter().map(|o| o.id).collect();
    apply(&mut h, doc);
    let array = made(&h, doc, &before);
    assert_eq!(bounds_of(&h, doc, array), ((0.0, 0.0), (85.0, 28.0)));
    assert!(h.controller().cam_dialog(doc, ARRAY).is_none(), "Apply closes the dialog");
    assert_eq!(previews(&h, doc), 0);
    h.invoke(doc, burnburn_presentation::CommandKey::Undo);
    assert!(view(&h, doc).objects.iter().all(|o| o.id != array), "one undo takes the whole array away");
    assert!(view(&h, doc).objects.iter().any(|o| o.id == ids[0]), "the source is untouched");
}

#[test]
fn a_circular_array_turns_each_copy_about_the_centre() {
    let (mut h, doc, _) = rig();
    open(&mut h, doc);
    kind(&mut h, doc, DialogChoice::ArrayCircular);
    let shown = vm(&mut h, doc);
    assert_eq!(shown.row(DialogField::GridColumns).unwrap().state, FieldState::Hidden);
    assert_eq!(shown.row(DialogField::CircularCount).unwrap().state, FieldState::Editable);
    text(&mut h, doc, DialogField::CircularCount, "4");
    text(&mut h, doc, DialogField::CircularCentreX, "0");
    text(&mut h, doc, DialogField::CircularCentreY, "0");
    text(&mut h, doc, DialogField::CircularSweep, "360");
    flag(&mut h, doc, DialogField::CircularRotate, true);
    assert_eq!(previews(&h, doc), 3);
    let before: Vec<GeometryId> = view(&h, doc).objects.iter().map(|o| o.id).collect();
    apply(&mut h, doc);
    let array = made(&h, doc, &before);
    // Four quarter turns of a square at the corner of the centre: the union reaches ±10 each way.
    assert_eq!(bounds_of(&h, doc, array), ((-10.0, -10.0), (10.0, 10.0)));
}

#[test]
fn a_path_array_lists_the_other_paths_and_follows_the_chosen_one() {
    let (mut h, doc, ids) = rig();
    open(&mut h, doc);
    kind(&mut h, doc, DialogChoice::ArrayPath);
    let shown = vm(&mut h, doc);
    let along = shown.row(DialogField::PathAlong).unwrap();
    assert_eq!(along.labels, vec!["Pad 2".to_owned()], "the source itself is not a path to follow");
    assert_eq!(along.state, FieldState::Editable);
    text(&mut h, doc, DialogField::PathCount, "4");
    flag(&mut h, doc, DialogField::PathOrient, false);
    assert_eq!(previews(&h, doc), 3);
    let before: Vec<GeometryId> = view(&h, doc).objects.iter().map(|o| o.id).collect();
    apply(&mut h, doc);
    let array = made(&h, doc, &before);
    assert_eq!(bounds_of(&h, doc, array), ((0.0, 0.0), (70.0, 10.0)), "4 squares 20 mm apart along the path");
    let _ = ids;
}

#[test]
fn a_path_array_without_a_path_says_so_and_cannot_be_applied() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids);
    open(&mut h, doc);
    kind(&mut h, doc, DialogChoice::ArrayPath);
    let shown = vm(&mut h, doc);
    assert!(shown.row(DialogField::PathAlong).unwrap().labels.is_empty());
    assert_eq!(shown.row(DialogField::PathAlong).unwrap().error, Some(DialogError::NoPath));
    assert!(!shown.action(DialogActionKind::Apply).unwrap().enabled);
    let rev = view(&h, doc).revision;
    apply(&mut h, doc);
    assert_eq!(view(&h, doc).revision, rev);
    assert!(h.controller().cam_dialog(doc, ARRAY).is_some(), "the dialog stays open with its message");
}

#[test]
fn bad_values_are_said_on_their_row_and_nothing_is_previewed_or_applied() {
    let (mut h, doc, _) = rig();
    open(&mut h, doc);
    text(&mut h, doc, DialogField::GridColumns, "0");
    text(&mut h, doc, DialogField::GridRows, "2,5");
    text(&mut h, doc, DialogField::GridPitchX, "abc");
    let shown = vm(&mut h, doc);
    assert_eq!(shown.row(DialogField::GridColumns).unwrap().error, Some(DialogError::TooManyCopies));
    assert_eq!(shown.row(DialogField::GridRows).unwrap().error, Some(DialogError::NotWhole));
    assert_eq!(shown.row(DialogField::GridPitchX).unwrap().error, Some(DialogError::Invalid));
    assert_eq!(previews(&h, doc), 0, "an invalid draft shows no preview");
    assert!(!shown.action(DialogActionKind::Apply).unwrap().enabled);
    text(&mut h, doc, DialogField::GridColumns, "100001");
    assert_eq!(
        vm(&mut h, doc).row(DialogField::GridColumns).unwrap().error,
        Some(DialogError::TooManyCopies),
        "more copies than the document allows"
    );
}

#[test]
fn the_array_needs_exactly_one_object_selected() {
    let (mut h, doc, ids) = rig();
    select(&mut h, doc, &ids);
    let shown = open(&mut h, doc);
    assert!(shown.notes.contains(&DialogNote::ArrayNeedsOneObject));
    assert!(!shown.action(DialogActionKind::Apply).unwrap().enabled);
    assert_eq!(previews(&h, doc), 0);
    select(&mut h, doc, &ids[..1]);
    let shown = vm(&mut h, doc);
    assert!(!shown.notes.contains(&DialogNote::ArrayNeedsOneObject), "the dialog follows the selection");
    assert!(shown.action(DialogActionKind::Apply).unwrap().enabled);
    select(&mut h, doc, &[]);
    assert!(vm(&mut h, doc).notes.contains(&DialogNote::ArrayNeedsOneObject));
}

#[test]
fn close_drops_the_draft_and_the_preview() {
    let (mut h, doc, _) = rig();
    open(&mut h, doc);
    text(&mut h, doc, DialogField::GridColumns, "5");
    assert_eq!(previews(&h, doc), 5 * 3 - 1);
    h.send(UiIntent::CamDialogAction { document: doc, dialog: ARRAY, action: DialogAction::Close });
    assert!(h.controller().cam_dialog(doc, ARRAY).is_none());
    assert_eq!(previews(&h, doc), 0);
    // Opening again starts from the defaults, not from the dropped draft.
    assert_eq!(open(&mut h, doc).text(DialogField::GridColumns), "3");
}
