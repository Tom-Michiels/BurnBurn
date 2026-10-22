//! G5-UI-gaten, bewerken en documenten (P02, P04, P06, ...): bediening via de publieke route.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_document::process::ProcessLabel;
use burnburn_document::UndoLabel;
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

// ---------------------------------------------------------------------------------------------
// P04: Bestand > Notities tonen
// ---------------------------------------------------------------------------------------------

/// Show Notes opens the panel with the stored text; committing changes the document in one undo
/// step and marks it dirty; an unchanged text is no new step.
#[test]
fn show_notes_opens_the_stored_text_and_a_commit_is_one_undo_step() {
    let (mut h, doc, _) = two_rects();
    assert!(vm(&h, doc).notes.is_none());
    assert!(h.view().commands[&CommandKey::ShowNotes].enabled);
    h.invoke(doc, CommandKey::ShowNotes);
    assert_eq!(vm(&h, doc).notes.as_ref().unwrap().text, "");
    assert!(h.view().commands[&CommandKey::ShowNotes].checked);
    let revision = vm(&h, doc).revision;
    h.send(UiIntent::SetNotes { document: doc, text: "Klant Jansen".into() });
    let now = vm(&h, doc);
    assert!(now.revision != revision);
    assert_eq!(now.notes.as_ref().unwrap().text, "Klant Jansen");
    assert_eq!(now.undo, Some(UndoLabel::Process(ProcessLabel::SetNotes)));
    let again = now.revision;
    h.send(UiIntent::SetNotes { document: doc, text: "Klant Jansen".into() });
    assert_eq!(vm(&h, doc).revision, again, "the same text is no new step");
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(vm(&h, doc).notes.as_ref().unwrap().text, "", "undo takes the note back");
    h.send(UiIntent::CloseNotes { document: doc });
    assert!(vm(&h, doc).notes.is_none());
    assert!(!h.view().commands[&CommandKey::ShowNotes].checked);
}

/// A text beyond the limit is refused and the panel keeps the stored note.
#[test]
fn notes_beyond_the_limit_are_refused_without_touching_the_document() {
    let (mut h, doc, _) = two_rects();
    h.invoke(doc, CommandKey::ShowNotes);
    let revision = vm(&h, doc).revision;
    h.send(UiIntent::SetNotes { document: doc, text: "a".repeat(20_001) });
    assert_eq!(vm(&h, doc).revision, revision);
    assert!(vm(&h, doc).notes.is_some(), "the panel stays open");
}

// ---------------------------------------------------------------------------------------------
// P04: LBRN2-import als een undo-stap
// ---------------------------------------------------------------------------------------------

fn lbrn2_session(name: &str) -> (HeadlessEditorHarness, DocumentId) {
    use burnburn_io::fs::FileSystem;
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../research/lightburn/fixtures").join(name),
    )
    .unwrap();
    let mut h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(std::path::Path::new("/in"));
    let path = std::path::PathBuf::from(format!("/in/{name}"));
    let temp = h.fs().create_temp_in(std::path::Path::new("/in"), ".fixture").unwrap();
    h.fs().write_all(&temp, &bytes).unwrap();
    h.fs().replace(&temp, &path).unwrap();
    let doc = h.new_document().unwrap();
    h.send(UiIntent::Import { document: doc, path });
    h.run_tasks();
    (h, doc)
}

/// A LightBurn project with shapes, a layer operation and an embedded bitmap is one history entry:
/// one undo takes everything away, one redo brings everything back.
#[test]
fn a_lbrn2_import_with_a_bitmap_is_one_undo_step() {
    let (mut h, doc) = lbrn2_session("gray-ramp-grayscale.lbrn2");
    h.invoke(doc, CommandKey::CommitImport);
    assert_eq!(vm(&h, doc).objects.len(), 1);
    assert_eq!(vm(&h, doc).operations.len(), 1);
    assert_eq!(vm(&h, doc).undo, Some(UndoLabel::Compound(burnburn_document::CompoundLabel::ImportProject)));
    h.invoke(doc, CommandKey::Undo);
    assert!(vm(&h, doc).objects.is_empty(), "one undo removes the frame");
    assert!(vm(&h, doc).operations.is_empty(), "and the operation");
    assert!(vm(&h, doc).undo.is_none(), "nothing of the import is left on the undo stack");
    h.invoke(doc, CommandKey::Redo);
    assert_eq!(vm(&h, doc).objects.len(), 1);
    assert_eq!(vm(&h, doc).operations.len(), 1);
}

// ---------------------------------------------------------------------------------------------
// E05: offsetvoorbeeld als taak en offsetopties
// ---------------------------------------------------------------------------------------------

/// The preview of an offset is computed by a task on the current revision; it never changes the
/// document and shows the size of the result before anything is applied. Round corners cut a
/// little area off the mitred ones: 20 x 10 grown by 2 mm is 336 mm2 mitred and 200 + 120 + 4 pi
/// = 332.57 mm2 rounded.
#[test]
fn the_offset_preview_is_a_task_that_never_changes_the_document() {
    let (mut h, doc, [a, _]) = two_rects();
    select(&mut h, doc, &[a]);
    let (revision, undo) = (vm(&h, doc).revision, vm(&h, doc).undo);
    h.send(UiIntent::PreviewOffset { document: doc, distance: "2 mm".into() });
    assert!(vm(&h, doc).offset.preview.is_none(), "nothing until the worker has answered");
    h.run_tasks();
    let preview = vm(&h, doc).offset.preview.expect("the preview arrived");
    assert_eq!((preview.width.as_str(), preview.height.as_str()), ("24", "14"));
    assert!((preview.area_mm2 - 332.57).abs() < 0.5, "round corners: {}", preview.area_mm2);
    assert_eq!((vm(&h, doc).revision, vm(&h, doc).undo), (revision, undo), "the document is untouched");

    // A changed join recomputes the open preview.
    h.send(UiIntent::SetOffsetOptions {
        document: doc,
        options: burnburn_presentation::OffsetOptions { join: burnburn_presentation::OffsetJoin::Miter, keep_sources: true },
    });
    h.run_tasks();
    let mitred = vm(&h, doc).offset.preview.expect("recomputed");
    assert!((mitred.area_mm2 - 336.0).abs() < 0.01, "mitred corners: {}", mitred.area_mm2);
}

/// A preview made for an older revision is never shown, also when the worker was too late to be
/// cancelled.
#[test]
fn an_offset_preview_of_an_older_revision_is_dropped() {
    let (mut h, doc, [a, _]) = two_rects();
    select(&mut h, doc, &[a]);
    h.send(UiIntent::PreviewOffset { document: doc, distance: "2 mm".into() });
    let late = h.take_task_requests();
    h.invoke(doc, CommandKey::Duplicate);
    for r in late {
        h.complete_despite_cancel(r);
    }
    assert!(vm(&h, doc).offset.preview.is_none());
}

/// A preview that cannot be made says where and why, like applying it would.
#[test]
fn an_impossible_offset_preview_names_the_problem_and_shows_nothing() {
    let (mut h, doc, [a, _]) = two_rects();
    select(&mut h, doc, &[a]);
    h.send(UiIntent::PreviewOffset { document: doc, distance: "-6 mm".into() });
    h.run_tasks();
    let v = vm(&h, doc);
    assert!(v.offset.preview.is_none());
    assert_eq!(v.problem.map(|p| p.message), Some(burnburn_presentation::MessageKey::OffsetImpossible));
}

/// The chosen join and the "keep sources" choice decide what Offset applies; the preview closes
/// once it is applied.
#[test]
fn applying_an_offset_uses_the_chosen_options() {
    let (mut h, doc, [a, _]) = two_rects();
    select(&mut h, doc, &[a]);
    h.send(UiIntent::SetOffsetOptions {
        document: doc,
        options: burnburn_presentation::OffsetOptions { join: burnburn_presentation::OffsetJoin::Miter, keep_sources: false },
    });
    h.send(UiIntent::PreviewOffset { document: doc, distance: "2 mm".into() });
    h.run_tasks();
    assert!(vm(&h, doc).offset.preview.is_some());
    h.send(UiIntent::Offset { document: doc, distance: "2 mm".into() });
    let v = vm(&h, doc);
    assert_eq!(v.objects.len(), 2, "the source was replaced by its offset: two objects remain");
    assert!(v.offset.preview.is_none(), "applied: the preview is gone");
    let new = v.objects.iter().find(|o| o.selected).expect("the result is selected");
    let area = h.scene(doc).unwrap().items.iter().find(|i| i.source == new.id).map(|i| i.path.signed_area().abs()).unwrap();
    assert!((area - 336.0).abs() < 0.01, "mitred: {area}");
}
