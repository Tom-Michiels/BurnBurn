//! E03/X02 (W2-L1-bewerken): cut, copy, paste, paste in place, duplicate and delete duplicates.
//! Every document change is one undo entry; copying changes no document; the clipboard payload
//! is handed to the host as bytes for the OS pasteboard.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod edit_rig;

use burnburn_document::{ArrangeLabel, UndoLabel};
use burnburn_io::clipboard::decode_clipboard;
use burnburn_presentation::{CommandKey, FieldKey, HostEffect, MessageKey, UiIntent};
use edit_rig::{opened_with, select, square, view};

fn clipboard_bytes(effects: &[HostEffect]) -> Option<Vec<u8>> {
    effects.iter().find_map(|e| match e {
        HostEffect::SetClipboard { bytes } => Some(bytes.clone()),
        _ => None,
    })
}

fn field(h: &burnburn_presentation::HeadlessEditorHarness, doc: burnburn_types::DocumentId, key: FieldKey) -> String {
    view(h, doc).inspector.expect("inspector").field(key).unwrap().text.clone()
}

#[test]
fn copy_hands_the_selection_to_the_host_and_changes_no_document() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(30.0, 0.0, 10.0)]);
    select(&mut h, doc, &[ids[1]]);
    let before = view(&h, doc);
    let update = h.invoke(doc, CommandKey::Copy);
    let bytes = clipboard_bytes(&update.effects).expect("a clipboard effect");
    let clip = decode_clipboard(&bytes).unwrap();
    assert_eq!(clip.items.len(), 1);
    assert_eq!(clip.items[0].name, "Pad 2");
    assert_eq!(clip.items[0].path.bounds().unwrap().min().x, 30.0);
    let after = view(&h, doc);
    assert_eq!((after.revision, after.undo), (before.revision, before.undo), "no history entry, no new revision");
    assert!(!after.dirty);
}

#[test]
fn copy_with_nothing_selected_does_nothing() {
    let (mut h, doc, _) = opened_with(vec![square(0.0, 0.0, 10.0)]);
    let update = h.invoke(doc, CommandKey::Copy);
    assert!(clipboard_bytes(&update.effects).is_none());
}

#[test]
fn paste_in_place_adds_copies_at_the_same_position_as_one_undo_entry_and_selects_them() {
    let (mut h, doc, ids) = opened_with(vec![square(5.0, 7.0, 10.0), square(30.0, 0.0, 10.0)]);
    select(&mut h, doc, &[ids[0]]);
    h.invoke(doc, CommandKey::Copy);
    h.invoke(doc, CommandKey::PasteInPlace);
    let vm = view(&h, doc);
    assert_eq!(vm.objects.len(), 3);
    assert_eq!(vm.undo, Some(UndoLabel::Arrange(ArrangeLabel::Paste)));
    assert_eq!(vm.selection.len(), 1);
    assert!(!ids.contains(&vm.selection[0]), "the copy has its own identity");
    assert_eq!((field(&h, doc, FieldKey::X), field(&h, doc, FieldKey::Y)), ("5".to_owned(), "7".to_owned()));
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(view(&h, doc).objects.len(), 2, "one undo removes the whole paste");
}

#[test]
fn paste_puts_the_copies_in_the_middle_of_the_view() {
    let (mut h, doc, ids) = opened_with(vec![square(100.0, 100.0, 10.0)]);
    select(&mut h, doc, &[ids[0]]);
    h.invoke(doc, CommandKey::Copy);
    h.invoke(doc, CommandKey::Paste);
    let vm = view(&h, doc);
    let centre = vm.camera.points_to_world(&vm.viewport, [vm.viewport.width_pt() / 2.0, vm.viewport.height_pt() / 2.0]);
    // A 10 mm square centred on the view centre starts 5 mm left of and below it.
    let (x, y) = (field(&h, doc, FieldKey::X).replace(',', ".").parse::<f64>().unwrap(), field(&h, doc, FieldKey::Y).replace(',', ".").parse::<f64>().unwrap());
    assert!((x - (centre.x - 5.0)).abs() < 1e-9 && (y - (centre.y - 5.0)).abs() < 1e-9, "({x}, {y}) vs {centre:?}");
}

#[test]
fn paste_keeps_the_relative_positions_of_several_objects() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(30.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids);
    h.invoke(doc, CommandKey::Copy);
    h.invoke(doc, CommandKey::Paste);
    let vm = view(&h, doc);
    assert_eq!(vm.objects.len(), 4);
    assert_eq!(vm.selection.len(), 2);
    assert!(vm.selection.iter().all(|id| !ids.contains(id)), "the selection is the pasted pair");
    // The pasted pair keeps its 30 mm pitch; the selection's overall width is the pair's width, 40 mm.
    let bounds = h.scene(doc).unwrap();
    let selected: Vec<_> = bounds.items.iter().filter(|i| vm.selection.contains(&i.source)).collect();
    let xs: Vec<f64> = selected.iter().map(|i| i.path.bounds().unwrap().min().x).collect();
    assert_eq!((xs[1] - xs[0]).abs(), 30.0);
}

#[test]
fn cut_removes_the_selection_in_one_undo_entry_and_paste_brings_it_back() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(30.0, 0.0, 10.0)]);
    select(&mut h, doc, &[ids[0]]);
    let update = h.invoke(doc, CommandKey::Cut);
    assert!(clipboard_bytes(&update.effects).is_some());
    let vm = view(&h, doc);
    assert_eq!(vm.objects.len(), 1);
    assert_eq!(vm.undo, Some(UndoLabel::Delete));
    h.invoke(doc, CommandKey::PasteInPlace);
    assert_eq!(view(&h, doc).objects.len(), 2);
    h.invoke(doc, CommandKey::Undo);
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(view(&h, doc).objects.len(), 2, "undoing the cut restores the original");
}

#[test]
fn duplicate_copies_in_place_and_leaves_the_clipboard_alone() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(30.0, 0.0, 10.0)]);
    select(&mut h, doc, &[ids[0]]);
    h.invoke(doc, CommandKey::Copy);
    select(&mut h, doc, &[ids[1]]);
    let update = h.invoke(doc, CommandKey::Duplicate);
    assert!(clipboard_bytes(&update.effects).is_none(), "duplicate does not touch the clipboard");
    let vm = view(&h, doc);
    assert_eq!(vm.objects.len(), 3);
    assert_eq!(vm.undo, Some(UndoLabel::Arrange(ArrangeLabel::Paste)));
    assert_eq!(field(&h, doc, FieldKey::X), "30");
    h.invoke(doc, CommandKey::PasteInPlace);
    assert_eq!(view(&h, doc).objects.len(), 4);
    assert_eq!(field(&h, doc, FieldKey::X), "0", "paste still brings the earlier copy, not the duplicate");
}

#[test]
fn pasted_data_from_the_os_pasteboard_works_in_another_document() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids);
    let bytes = clipboard_bytes(&h.invoke(doc, CommandKey::Copy).effects).unwrap();
    let other = h.new_document().expect("second window");
    h.send(UiIntent::PasteData { document: Some(other), bytes, in_place: true });
    let vm = view(&h, other);
    assert_eq!(vm.objects.len(), 1);
    assert_eq!(vm.undo, Some(UndoLabel::Arrange(ArrangeLabel::Paste)));
    assert_eq!(view(&h, doc).objects.len(), 1, "the source document is untouched");
}

#[test]
fn unreadable_pasteboard_data_is_reported_and_changes_nothing() {
    let (mut h, doc, _) = opened_with(vec![square(0.0, 0.0, 10.0)]);
    let before = view(&h, doc).revision;
    for junk in [b"hello".to_vec(), Vec::new(), br#"{"format":"lightburn","version":1}"#.to_vec()] {
        let update = h.send(UiIntent::PasteData { document: Some(doc), bytes: junk, in_place: false });
        assert!(
            update.effects.iter().any(|e| matches!(e, HostEffect::ShowError { message: MessageKey::ClipboardUnreadable, .. })),
            "{:?}",
            update.effects
        );
    }
    assert_eq!(view(&h, doc).revision, before);
}

#[test]
fn paste_with_an_empty_clipboard_is_a_no_op() {
    let (mut h, doc, _) = opened_with(vec![square(0.0, 0.0, 10.0)]);
    let before = view(&h, doc).revision;
    h.invoke(doc, CommandKey::Paste);
    h.invoke(doc, CommandKey::PasteInPlace);
    assert_eq!(view(&h, doc).revision, before);
}

#[test]
fn pasted_group_members_form_a_new_group_and_the_original_group_is_untouched() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(30.0, 0.0, 10.0), square(60.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids[..2]);
    h.invoke(doc, CommandKey::Group);
    let original: Vec<_> = view(&h, doc).objects.iter().map(|o| o.groups.clone()).collect();
    assert_eq!(original[0], original[1]);
    assert!(original[2].is_empty());
    select(&mut h, doc, &ids[..2]);
    h.invoke(doc, CommandKey::Copy);
    h.invoke(doc, CommandKey::PasteInPlace);
    let rows = view(&h, doc).objects;
    assert_eq!(rows.len(), 5);
    let copies: Vec<_> = rows.iter().filter(|r| !ids.contains(&r.id)).collect();
    assert_eq!(copies[0].groups.len(), 1);
    assert_eq!(copies[0].groups, copies[1].groups, "the copies belong to one group");
    assert_ne!(copies[0].groups, original[0], "a different group than the originals");
    assert_eq!(rows[0].groups, original[0]);
}

#[test]
fn delete_duplicates_keeps_the_first_of_identical_stacked_shapes_in_one_entry() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(30.0, 0.0, 10.0)]);
    select(&mut h, doc, &[ids[0]]);
    h.invoke(doc, CommandKey::Duplicate);
    h.invoke(doc, CommandKey::Duplicate);
    assert_eq!(view(&h, doc).objects.len(), 4);
    h.invoke(doc, CommandKey::SelectAll);
    h.invoke(doc, CommandKey::DeleteDuplicates);
    let vm = view(&h, doc);
    assert_eq!(vm.objects.iter().map(|o| o.id).collect::<Vec<_>>(), ids, "the first of each stack survives");
    assert_eq!(vm.undo, Some(UndoLabel::Delete));
    h.invoke(doc, CommandKey::Undo);
    assert_eq!(view(&h, doc).objects.len(), 4);
}

#[test]
fn delete_duplicates_without_duplicates_says_so_and_changes_nothing() {
    let (mut h, doc, ids) = opened_with(vec![square(0.0, 0.0, 10.0), square(30.0, 0.0, 10.0)]);
    select(&mut h, doc, &ids);
    let before = view(&h, doc).revision;
    h.invoke(doc, CommandKey::DeleteDuplicates);
    let vm = view(&h, doc);
    assert_eq!(vm.revision, before);
    assert!(vm.messages.iter().any(|m| m.key == MessageKey::NoDuplicatesFound));
}
