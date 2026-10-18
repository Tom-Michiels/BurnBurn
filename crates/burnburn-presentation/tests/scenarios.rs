//! Headless user scenarios (docs/user-flows/gate-a.md). They drive the real AppController with
//! the same intents a native host sends, real editor tools and real document commands.

#![allow(clippy::expect_used)] // test helpers outside #[test] fns

use burnburn_document::UndoLabel;
use burnburn_editor::ToolKind;
use burnburn_presentation::{CommandKey, HeadlessEditorHarness};

/// Flow A2, cancel path (P02, E01): Esc mid-drag leaves document, history and view unchanged.
#[test]
fn drag_cancel_leaves_document_and_history_unchanged() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window opened");
    h.invoke(doc, CommandKey::ToolRectangle);
    let before = h.document(doc).expect("document view");

    h.begin_drag(doc, (0.0, 0.0), (120.0, 80.0));
    let during = h.document(doc).expect("document view");
    assert!(during.preview.is_some(), "the drag reached the rectangle tool and shows a preview");
    assert_eq!(during.revision, before.revision, "a preview is not a document change");

    h.escape(doc);
    let after = h.document(doc).expect("document view");
    assert_eq!(after, before, "document, history (undo/redo), selection and view are exactly as before");
    assert!(after.objects.is_empty());
    assert_eq!((after.undo, after.redo), (None, None));
    assert!(!after.dirty);
    assert_eq!(after.tool, ToolKind::Rectangle, "Esc cancels the gesture, not the tool");
    assert!(!h.view().commands[&CommandKey::Undo].enabled);
    let _ = UndoLabel::CreateRectangle;
}

/// Flow A2/A4 (P02): a cancelled drag adds no history entry next to an existing one.
#[test]
fn cancelled_drag_creates_no_history_entry() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window opened");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (120.0, 80.0));
    let committed = h.document(doc).expect("view");
    assert_eq!(committed.objects.len(), 1);
    assert_eq!(committed.undo, Some(UndoLabel::CreateRectangle));

    h.begin_drag(doc, (200.0, 0.0), (260.0, 40.0));
    h.pointer(doc, burnburn_editor::PointerPhase::Cancel, (260.0, 40.0), Default::default());
    let after_cancel = h.document(doc).expect("view");
    assert_eq!(after_cancel.revision, committed.revision);
    assert_eq!(after_cancel.objects, committed.objects);

    // Exactly one entry: one undo empties the document, a second has nothing left.
    h.invoke(doc, CommandKey::Undo);
    let undone = h.document(doc).expect("view");
    assert!(undone.objects.is_empty());
    assert_eq!(undone.undo, None);
    assert!(!h.view().commands[&CommandKey::Undo].enabled);
    assert_eq!(undone.redo, Some(UndoLabel::CreateRectangle));
}

/// Flow A8 (X04 basis): a worker result for an older revision never changes the newer one.
#[test]
fn stale_task_result_does_not_change_newer_revision() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window opened");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (120.0, 80.0));
    h.invoke(doc, CommandKey::Measure);
    let old = h.take_task_requests();
    assert_eq!(old.len(), 1);

    // The user keeps editing before the worker finishes.
    h.send(burnburn_presentation::UiIntent::CommitField {
        document: doc,
        field: burnburn_presentation::FieldKey::Width,
        text: "150".into(),
    });
    let newer = h.document(doc).expect("view");
    assert!(newer.revision > old[0].source);

    // Decisive rule: even a worker that raced past the cancellation and returns Ok is rejected
    // on revision (cancellation is only best effort).
    assert!(old[0].cancel.is_cancelled(), "the edit cancels the outdated task");
    for request in old {
        h.complete_despite_cancel(request);
    }
    let after = h.document(doc).expect("view");
    assert_eq!(after.revision, newer.revision);
    assert_eq!(after.objects, newer.objects);
    assert_eq!(after.measure, None, "the 120 mm result of the old revision is not shown for the 150 mm document");
    assert!(after.tasks.is_empty());

    // A fresh request for the current revision is applied.
    h.invoke(doc, CommandKey::Measure);
    h.run_tasks();
    let measured = h.document(doc).expect("view").measure.expect("current measure applied");
    assert_eq!((measured.revision, measured.width_text.as_deref()), (newer.revision, Some("150")));
}

/// Flow A1 (X02, E03): two windows keep separate selections, even with identical object ids.
#[test]
fn two_windows_do_not_share_selection() {
    let mut h = HeadlessEditorHarness::new();
    let a = h.new_document().expect("window A");
    let b = h.new_document().expect("window B");
    for doc in [a, b] {
        h.invoke(doc, CommandKey::ToolRectangle);
        h.drag(doc, (0.0, 0.0), (50.0, 50.0));
        h.invoke(doc, CommandKey::ToolSelect);
        // Click on empty canvas clears the selection the create left behind.
        h.click(doc, (300.0, 300.0), Default::default());
    }
    let (va, vb) = (h.document(a).expect("A"), h.document(b).expect("B"));
    assert_eq!(va.objects[0].id, vb.objects[0].id, "ids are per document, so a global selection would leak");
    assert!(va.selection.is_empty() && vb.selection.is_empty());

    h.pointer(a, burnburn_editor::PointerPhase::Down, (25.0, 25.0), Default::default());
    let (va, vb) = (h.document(a).expect("A"), h.document(b).expect("B"));
    assert_eq!(va.selection, vec![va.objects[0].id]);
    assert!(va.objects[0].selected && va.inspector.is_some());
    assert!(vb.selection.is_empty(), "selecting in A does not select in B");
    assert!(!vb.objects[0].selected && vb.inspector.is_none());
}

/// Flow A7: after a window closes, late worker results are dropped without any callback.
#[test]
fn closed_session_receives_no_callback() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window opened");
    let other = h.new_document().expect("second window");
    h.invoke(doc, CommandKey::Measure);
    let pending = h.take_task_requests();
    assert_eq!(pending.len(), 1);

    let closed = h.invoke(doc, CommandKey::Close);
    assert!(closed.effects.contains(&burnburn_presentation::HostEffect::CloseWindow { document: doc }));
    assert!(pending[0].cancel.is_cancelled(), "closing cancels the session's running tasks");
    let before = h.view();

    for request in pending {
        let update = h.complete(request);
        assert_eq!(update, Default::default(), "no effect, no redraw, no view change for a closed session");
    }
    assert_eq!(*h.view(), *before);
    assert!(h.view().document(doc).is_none());
    assert!(h.view().document(other).is_some());
}

fn rect_size(h: &HeadlessEditorHarness, doc: burnburn_types::DocumentId) -> (String, String) {
    let vm = h.document(doc).expect("view");
    let inspector = vm.inspector.expect("single selection shows the inspector");
    let text = |k| inspector.field(k).expect("field").text.clone();
    (text(burnburn_presentation::FieldKey::Width), text(burnburn_presentation::FieldKey::Height))
}

/// Flow A2–A4 (E01, P02, X02): the executable part of gate A before storage exists.
#[test]
fn headless_gate_a_rectangle_resize_undo_redo() {
    use burnburn_presentation::{FieldKey, UiIntent};
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window opened");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (120.0, 80.0));
    let created = h.document(doc).expect("view");
    let id = created.objects[0].id;
    assert_eq!(created.selection, vec![id], "the new rectangle is selected");
    assert_eq!(rect_size(&h, doc), ("120".into(), "80".into()));

    h.send(UiIntent::CommitField { document: doc, field: FieldKey::Width, text: "150".into() });
    assert_eq!(rect_size(&h, doc), ("150".into(), "80".into()));
    assert_eq!(h.document(doc).expect("view").undo, Some(UndoLabel::SetDimension));

    h.invoke(doc, CommandKey::Undo);
    let undone = h.document(doc).expect("view");
    assert_eq!(undone.objects.len(), 1);
    assert_eq!(undone.objects[0].id, id, "undo keeps the stable id");
    assert_eq!(undone.selection, vec![id], "undo restores the selection of the changed object");
    assert_eq!(rect_size(&h, doc), ("120".into(), "80".into()));
    assert!(undone.dirty);

    h.invoke(doc, CommandKey::Redo);
    assert_eq!(rect_size(&h, doc), ("150".into(), "80".into()));
}

/// Flow A3 error path (E01, X03): invalid text shows a field error and sends no command;
/// comma and inch input give the same physical value as point and millimetres.
#[test]
fn invalid_dimension_text_shows_field_error_and_sends_no_command() {
    use burnburn_presentation::{FieldError, FieldKey, MessageKey, UiIntent};
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window opened");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (120.0, 80.0));
    let before = h.document(doc).expect("view");

    for (text, reason) in [("abc", FieldError::NotANumber), ("0", FieldError::NotPositive), ("", FieldError::Empty)] {
        h.send(UiIntent::CommitField { document: doc, field: FieldKey::Width, text: text.into() });
        let vm = h.document(doc).expect("view");
        assert_eq!(vm.revision, before.revision, "{text:?} sent no command");
        let field = vm.inspector.as_ref().and_then(|i| i.field(FieldKey::Width).cloned()).expect("width field");
        assert_eq!(field.error, Some(MessageKey::InvalidLength(reason)), "{text:?}");
        assert_eq!(field.text, "120", "the field keeps showing the document value");
    }

    h.send(UiIntent::CommitField { document: doc, field: FieldKey::Width, text: "5,9 in".into() });
    let vm = h.document(doc).expect("view");
    let field = vm.inspector.as_ref().and_then(|i| i.field(FieldKey::Width).cloned()).expect("width field");
    assert_eq!((field.text.as_str(), field.error), ("149.86", None));
}

fn opened_window(updates: &[burnburn_presentation::AppUpdate]) -> Option<burnburn_types::DocumentId> {
    updates.iter().flat_map(|u| u.effects.iter()).find_map(|e| match e {
        burnburn_presentation::HostEffect::OpenWindow { document } => Some(*document),
        _ => None,
    })
}

fn harness_with_workdir() -> HeadlessEditorHarness {
    let h = HeadlessEditorHarness::new();
    h.fs().create_dir_all_for_test(std::path::Path::new("/work"));
    h
}

/// Integration gate A (headless route): draw 120 × 80, set width 150, undo, save, reopen.
#[test]
fn headless_gate_a_rectangle_resize_undo_save_reopen() {
    use burnburn_presentation::{FieldKey, HostEffect, UiIntent};
    let mut h = harness_with_workdir();
    let doc = h.new_document().expect("window opened");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (120.0, 80.0));
    let id = h.document(doc).expect("view").objects[0].id;
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::Width, text: "150".into() });
    h.invoke(doc, CommandKey::Undo);

    // Cmd+S on an untitled document asks for a location (native save sheet).
    let ask = h.invoke(doc, CommandKey::Save);
    assert!(ask.effects.iter().any(|e| matches!(e, HostEffect::ShowSavePanel { document, .. } if *document == doc)));
    let path = std::path::PathBuf::from("/work/plaat.burnburn");
    h.send(UiIntent::Save { document: doc, path: Some(path.clone()) });
    assert!(h.document(doc).expect("view").dirty, "dirty until the save task reports success");
    h.run_tasks();
    let saved = h.document(doc).expect("view");
    assert!(!saved.dirty, "saved");
    assert_eq!(saved.title, "plaat.burnburn");

    h.send(UiIntent::Open { path });
    let updates = h.run_tasks();
    let reopened = opened_window(&updates).expect("reopened window");
    assert_ne!(reopened, doc, "a new session");
    let vm = h.document(reopened).expect("view");
    assert_eq!(vm.objects.len(), 1);
    assert_eq!(vm.objects[0].id, id, "stable id survives save and reopen");
    assert!(!vm.dirty);
    assert_eq!(vm.title, "plaat.burnburn");
    assert_eq!(vm.undo, None, "history is not persisted");
    h.send(UiIntent::Select { document: reopened, ids: vec![id], mode: burnburn_presentation::SelectMode::Replace });
    assert_eq!(rect_size(&h, reopened), ("120".into(), "80".into()));
}

/// Flow A5 error path (P01): a failed write reports an error, keeps the document dirty and
/// leaves the previously saved file readable.
#[test]
fn failed_save_keeps_document_dirty_and_previous_file_readable() {
    use burnburn_presentation::{FieldKey, HostEffect, UiIntent};
    let mut h = harness_with_workdir();
    let doc = h.new_document().expect("window opened");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (120.0, 80.0));
    let path = std::path::PathBuf::from("/work/plaat.burnburn");
    h.send(UiIntent::Save { document: doc, path: Some(path.clone()) });
    h.run_tasks();
    assert!(!h.document(doc).expect("view").dirty);

    h.send(UiIntent::CommitField { document: doc, field: FieldKey::Width, text: "150".into() });
    h.fs().fail_on(burnburn_io::fs::FsOp::Replace, 0);
    h.invoke(doc, CommandKey::Save);
    let updates = h.run_tasks();
    assert!(
        updates
            .iter()
            .flat_map(|u| &u.effects)
            .any(|e| matches!(e, HostEffect::ShowError { document: Some(d), .. } if *d == doc)),
        "the failure is reported: {updates:?}"
    );
    assert!(h.document(doc).expect("view").dirty, "still dirty after a failed save");
    let on_disk = burnburn_io::load_project(h.fs(), &path).expect("previous file still readable");
    let (_, obj) = on_disk.document.objects().next().expect("object");
    assert_eq!(obj.bounds().expect("bounds").width(), 120.0);
}

/// Flow A5 (P01): a save that finishes after a further edit records only the saved revision.
#[test]
fn save_finishing_after_further_edit_leaves_document_dirty() {
    use burnburn_presentation::{FieldKey, UiIntent};
    let mut h = harness_with_workdir();
    let doc = h.new_document().expect("window opened");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (120.0, 80.0));
    h.send(UiIntent::Save { document: doc, path: Some("/work/plaat.burnburn".into()) });
    let save = h.take_task_requests();
    h.send(UiIntent::CommitField { document: doc, field: FieldKey::Width, text: "150".into() });
    assert!(!save[0].cancel.is_cancelled(), "an edit does not cancel a save in progress");
    for r in save {
        h.complete(r);
    }
    let vm = h.document(doc).expect("view");
    assert!(vm.dirty, "the 150 mm revision was never written");
    assert_eq!(vm.title, "plaat.burnburn", "the file location is known now");
}

/// Flow A5/A7 cancel paths: cancelling the save sheet or the close sheet changes nothing.
#[test]
fn cancelled_save_panel_and_close_sheet_change_nothing() {
    use burnburn_presentation::{CloseChoice, DialogKind, HostEffect, UiIntent};
    let mut h = harness_with_workdir();
    let doc = h.new_document().expect("window opened");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (120.0, 80.0));
    let before = h.document(doc).expect("view");
    h.invoke(doc, CommandKey::Save);
    h.send(UiIntent::DialogCancelled { document: doc, dialog: DialogKind::SavePanel });
    let close = h.invoke(doc, CommandKey::Close);
    assert_eq!(close.effects, vec![HostEffect::ConfirmClose { document: doc }]);
    h.send(UiIntent::ConfirmClose { document: doc, choice: CloseChoice::Cancel });
    assert!(h.take_task_requests().is_empty());
    assert_eq!(h.document(doc).expect("view"), before);
    assert!(h.fs().files().is_empty(), "nothing was written");
}

/// Flow A5 (P01, Design answer to core.md #4): replace succeeded but the directory sync failed.
/// The new content is on disk: the document is clean and the user gets a non-blocking warning,
/// not an error dialog.
#[test]
fn unconfirmed_durable_save_is_clean_with_a_warning() {
    use burnburn_presentation::{HostEffect, MessageKey, UiIntent};
    let mut h = harness_with_workdir();
    let doc = h.new_document().expect("window opened");
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (120.0, 80.0));
    h.fs().fail_on(burnburn_io::fs::FsOp::SyncDir, 0);
    h.send(UiIntent::Save { document: doc, path: Some("/work/plaat.burnburn".into()) });
    let updates = h.run_tasks();
    let vm = h.document(doc).expect("view");
    assert!(!vm.dirty, "the replaced file holds this revision");
    assert!(vm.messages.iter().any(|m| m.key == MessageKey::SaveNotConfirmedDurable));
    assert!(
        !updates.iter().flat_map(|u| &u.effects).any(|e| matches!(e, HostEffect::ShowError { .. })),
        "no error dialog"
    );
}
