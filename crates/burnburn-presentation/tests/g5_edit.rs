//! G5-UI-gaten, bewerken en documenten (P02, P04, P06, ...): bediening via de publieke route.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_presentation::{CommandKey, DocumentViewModel, HeadlessEditorHarness, SelectMode, UiIntent};
use burnburn_types::{DocumentId, GeometryId};

fn vm(h: &HeadlessEditorHarness, doc: DocumentId) -> DocumentViewModel {
    h.document(doc).unwrap()
}

fn two_rects() -> (HeadlessEditorHarness, DocumentId, [GeometryId; 2]) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (20.0, 10.0));
    h.drag(doc, (40.0, 0.0), (60.0, 10.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let ids: Vec<GeometryId> = vm(&h, doc).objects.iter().map(|o| o.id).collect();
    (h, doc, [ids[0], ids[1]])
}

fn select(h: &mut HeadlessEditorHarness, doc: DocumentId, ids: &[GeometryId]) {
    h.send(UiIntent::Select { document: doc, ids: ids.to_vec(), mode: SelectMode::Replace });
}

fn selected(h: &HeadlessEditorHarness, doc: DocumentId) -> Vec<GeometryId> {
    vm(h, doc).objects.iter().filter(|o| o.selected).map(|o| o.id).collect()
}

// ---------------------------------------------------------------------------------------------
// P02: selectieherstel bij undo/redo (undo-record bewaart de selectie)
// ---------------------------------------------------------------------------------------------

/// Undo of a command that made a new object brings back the selection from before it; redo selects
/// what the user ended with. Earlier the presentation could only derive this from `changed`.
#[test]
fn undo_restores_the_selection_from_before_the_command_and_redo_the_one_after() {
    let (mut h, doc, [a, b]) = two_rects();
    select(&mut h, doc, &[a]);
    h.invoke(doc, CommandKey::Duplicate);
    let after = selected(&h, doc);
    assert_eq!(after.len(), 1);
    assert!(after[0] != a && after[0] != b, "the copy is the selection");
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(selected(&h, doc), [a], "undo goes back to the selection before the duplicate");
    h.invoke(doc, CommandKey::Redo);
    assert_eq!(selected(&h, doc), after, "redo selects the copy again");
}

/// A selection the user changed between two commands is the one undo returns to.
#[test]
fn a_selection_change_between_commands_is_part_of_the_next_undo_record() {
    let (mut h, doc, [a, b]) = two_rects();
    select(&mut h, doc, &[a]);
    h.invoke(doc, CommandKey::Duplicate);
    select(&mut h, doc, &[b]);
    h.invoke(doc, CommandKey::Duplicate);
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(selected(&h, doc), [b]);
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(selected(&h, doc), [a]);
}
