//! W2-L1 (bewerken), headless: the recent-projects menu items, the Cut/Copy/Paste decisions the
//! canvas makes from the pasteboard, and the stored recent-projects file. The view model and the
//! clipboard bytes come from the real `AppController` (through the headless harness).

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use std::path::PathBuf;

use burnburn_macos::adapter::{
    MenuItem, NativeEvent, Reaction, menu_enabled_with_recent, menu_item, menu_items, menu_tag, menu_title,
    recent_open_reaction, translate,
};
use burnburn_macos::l10n::Locale;
use burnburn_macos::pasteboard::{CLIPBOARD_TYPE, edit_item_enabled, paste_intent};
use burnburn_macos::recent::{read_recent_projects, recent_title, write_recent_projects};
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, HostEffect, UiIntent};
use burnburn_types::{DocumentId, StableId};

fn paths(names: &[&str]) -> Vec<PathBuf> {
    names.iter().map(|n| PathBuf::from(format!("/work/{n}.burnburn"))).collect()
}

/// A document with one selected 120 × 80 mm rectangle, drawn through the real controller.
fn one_rectangle() -> (HeadlessEditorHarness, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (120.0, 80.0));
    (h, doc)
}

// ---- recent projects in the menu ---------------------------------------------------------------

#[test]
fn recent_entries_have_unique_tags_of_their_own_and_clear_is_a_fixed_item() {
    let items = menu_items();
    assert!(items.contains(&MenuItem::ClearRecent), "Wis menu is always in the menu");
    let mut tags: Vec<isize> = items.iter().map(|i| menu_tag(*i)).collect();
    for i in 0..10 {
        let item = MenuItem::Recent(i);
        tags.push(menu_tag(item));
        assert_eq!(menu_item(menu_tag(item)), Some(item), "recent entry {i} round-trips through its tag");
    }
    let total = tags.len();
    tags.sort();
    tags.dedup();
    assert_eq!(tags.len(), total, "no tag is shared between commands and recent entries");
    assert_eq!(menu_item(menu_tag(MenuItem::ClearRecent)), Some(MenuItem::ClearRecent));
}

#[test]
fn choosing_a_recent_entry_opens_that_path_and_an_unknown_index_does_nothing() {
    let list = paths(&["a", "b", "c"]);
    assert_eq!(
        recent_open_reaction(MenuItem::Recent(1), &list),
        Reaction::Intent(UiIntent::Open { path: list[1].clone() })
    );
    assert_eq!(recent_open_reaction(MenuItem::Recent(3), &list), Reaction::Nothing, "stale index");
    assert_eq!(recent_open_reaction(MenuItem::Command(CommandKey::Save), &list), Reaction::Nothing);
}

#[test]
fn wis_menu_clears_the_list_and_is_only_enabled_when_there_is_something_to_clear() {
    let vm = HeadlessEditorHarness::new().view();
    assert_eq!(
        translate(NativeEvent::Menu(MenuItem::ClearRecent), &vm),
        Reaction::Intent(UiIntent::ClearRecentProjects)
    );
    assert!(!menu_enabled_with_recent(MenuItem::ClearRecent, &vm, 0));
    assert!(menu_enabled_with_recent(MenuItem::ClearRecent, &vm, 2));
    assert!(menu_enabled_with_recent(MenuItem::Recent(1), &vm, 2));
    assert!(!menu_enabled_with_recent(MenuItem::Recent(2), &vm, 2), "beyond the list");
    assert_eq!(menu_title(MenuItem::ClearRecent, &vm, Locale::Nl), "Wis menu");
    assert_eq!(menu_title(MenuItem::ClearRecent, &vm, Locale::En), "Clear Menu");
}

#[test]
fn a_recent_entry_shows_the_file_name_and_never_an_empty_title() {
    assert_eq!(recent_title(&PathBuf::from("/Users/tom/Plaat.burnburn")), "Plaat.burnburn");
    assert_eq!(recent_title(&PathBuf::from("/")), "/", "no file name: the path itself");
    assert!(!recent_title(&PathBuf::from("")).is_empty());
}

// ---- the stored recent-projects file -----------------------------------------------------------

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("burnburn-recent-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_list_survives_a_write_and_read_in_order() {
    let dir = temp_dir("roundtrip");
    let file = dir.join("BurnBurn").join("recent-projects.json");
    let list = paths(&["een", "twee", "drie met spatie en é"]);
    write_recent_projects(&file, &list).unwrap();
    assert_eq!(read_recent_projects(&file), list);
    // A second write replaces the content and leaves no temporary file next to it.
    write_recent_projects(&file, &list[..1]).unwrap();
    assert_eq!(read_recent_projects(&file), list[..1].to_vec());
    let names: Vec<String> = std::fs::read_dir(file.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into())
        .collect();
    assert_eq!(names, ["recent-projects.json"], "atomic write leaves only the final file");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_missing_corrupted_or_oversized_file_is_an_empty_list_never_a_crash() {
    let dir = temp_dir("corrupt");
    assert!(read_recent_projects(&dir.join("absent.json")).is_empty());
    for (name, content) in [
        ("truncated.json", "[\"/a/b.burnburn\", \"/c"),
        ("object.json", "{\"paths\": [\"/a.burnburn\"]}"),
        ("numbers.json", "[1, 2, 3]"),
        ("mixed.json", "[\"/a.burnburn\", 7]"),
        ("empty.json", ""),
        ("binary.json", "\u{0}\u{1}\u{2}"),
    ] {
        let file = dir.join(name);
        std::fs::write(&file, content).unwrap();
        assert!(read_recent_projects(&file).is_empty(), "{name} is rejected as a whole");
    }
    let big = dir.join("big.json");
    std::fs::write(&big, format!("[\"{}\"]", "x".repeat(2 * 1024 * 1024))).unwrap();
    assert!(read_recent_projects(&big).is_empty(), "an absurdly large file is not read");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn writing_into_a_folder_that_cannot_be_created_is_an_error_not_a_panic() {
    let dir = temp_dir("blocked");
    let blocker = dir.join("file");
    std::fs::write(&blocker, "x").unwrap();
    assert!(write_recent_projects(&blocker.join("sub").join("recent.json"), &paths(&["a"])).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

// ---- Cut / Copy / Paste -------------------------------------------------------------------------

#[test]
fn the_pasteboard_type_is_a_reverse_dns_identifier_of_our_own() {
    assert_eq!(CLIPBOARD_TYPE, "app.burnburn.clipboard.v1");
}

#[test]
fn paste_sends_the_pasteboard_bytes_when_we_own_the_type_and_the_controllers_clipboard_otherwise() {
    let doc = <DocumentId as StableId>::from_raw(3);
    let bytes = vec![1, 2, 3];
    assert_eq!(
        paste_intent(Some(doc), false, Some(bytes.clone())),
        UiIntent::PasteData { document: Some(doc), bytes: bytes.clone(), in_place: false }
    );
    assert_eq!(
        paste_intent(Some(doc), true, Some(bytes.clone())),
        UiIntent::PasteData { document: Some(doc), bytes, in_place: true }
    );
    assert_eq!(
        paste_intent(Some(doc), false, None),
        UiIntent::Invoke { document: Some(doc), command: CommandKey::Paste }
    );
    assert_eq!(
        paste_intent(Some(doc), true, None),
        UiIntent::Invoke { document: Some(doc), command: CommandKey::PasteInPlace }
    );
}

/// The whole route with the real controller: Copy puts bytes on the pasteboard (`SetClipboard`),
/// a paste of those bytes adds the copy as one undo step.
#[test]
fn a_copy_then_a_paste_of_the_pasteboard_bytes_duplicates_the_object() {
    let (mut h, doc) = one_rectangle();
    let update = h.send(UiIntent::Invoke { document: Some(doc), command: CommandKey::Copy });
    let bytes = update
        .effects
        .iter()
        .find_map(|e| match e {
            HostEffect::SetClipboard { bytes } => Some(bytes.clone()),
            _ => None,
        })
        .expect("Copy asks the host to fill the pasteboard");
    assert!(!bytes.is_empty());
    assert_eq!(h.document(doc).unwrap().objects.len(), 1, "Copy does not change the document");

    h.send(paste_intent(Some(doc), false, Some(bytes)));
    assert_eq!(h.document(doc).unwrap().objects.len(), 2);

    // Garbage on the pasteboard under our type is reported, not applied.
    let before = h.document(doc).unwrap().revision;
    h.send(paste_intent(Some(doc), false, Some(b"not a clipboard".to_vec())));
    assert_eq!(h.document(doc).unwrap().revision, before);
}

#[test]
fn edit_items_follow_the_view_model_and_paste_also_follows_the_pasteboard() {
    let (h, _) = one_rectangle();
    let vm = h.view();
    // With a document, the view model enables Paste (the controller may hold a clipboard of its own).
    assert!(edit_item_enabled(CommandKey::Paste, &vm, false));
    assert!(edit_item_enabled(CommandKey::Paste, &vm, true));
    assert!(edit_item_enabled(CommandKey::PasteInPlace, &vm, true));
    // Cut and Copy follow the selection only; the pasteboard does not matter.
    assert!(edit_item_enabled(CommandKey::Copy, &vm, false));
    assert!(edit_item_enabled(CommandKey::Cut, &vm, false));

    // Nothing selected: nothing to cut or copy, but another window's copy can still be pasted.
    let mut h = HeadlessEditorHarness::new();
    h.new_document().unwrap();
    let vm = h.view();
    assert!(!edit_item_enabled(CommandKey::Copy, &vm, true));
    assert!(!edit_item_enabled(CommandKey::Cut, &vm, true));
    assert!(edit_item_enabled(CommandKey::Paste, &vm, true));

    // Without a document there is nothing to paste into, whatever the pasteboard holds.
    let empty = HeadlessEditorHarness::new().view();
    assert!(!edit_item_enabled(CommandKey::Paste, &empty, true));
    assert!(!edit_item_enabled(CommandKey::PasteInPlace, &empty, true));
}
