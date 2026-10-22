//! G6-machine, C06 on the Mac: Lock Start Point and Lock Direction are menu items with words in
//! both languages, on only while shapes are selected, checked when the whole selection has the
//! lock, and their undo step has a name.

#![allow(clippy::unwrap_used, clippy::panic)]

use burnburn_macos::adapter::{MenuItem, menu_checked, menu_enabled, menu_item, menu_items, menu_tag, menu_title};
use burnburn_macos::l10n::Locale;
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, MessageKey, SelectMode, UiIntent};

#[test]
fn both_locks_are_menu_items_with_words_in_both_languages() {
    let h = HeadlessEditorHarness::new();
    for (key, nl, en) in [
        (CommandKey::LockPathStart, "Startpunt vergrendelen", "Lock Start Point"),
        (CommandKey::LockPathDirection, "Richting vergrendelen", "Lock Direction"),
    ] {
        let item = MenuItem::Command(key);
        assert!(menu_items().contains(&item));
        assert_eq!(menu_item(menu_tag(item)), Some(item));
        assert_eq!(menu_title(item, &h.view(), Locale::Nl), nl);
        assert_eq!(menu_title(item, &h.view(), Locale::En), en);
    }
}

#[test]
fn the_items_follow_the_selection_and_the_locks() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (20.0, 10.0));
    h.invoke(doc, CommandKey::ToolSelect);
    let start = MenuItem::Command(CommandKey::LockPathStart);
    h.send(UiIntent::Select { document: doc, ids: vec![], mode: SelectMode::Replace });
    assert!(!menu_enabled(start, &h.view()));
    let id = h.document(doc).unwrap().objects[0].id;
    h.send(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    assert!(menu_enabled(start, &h.view()));
    assert!(!menu_checked(start, &h.view()));
    h.invoke(doc, CommandKey::LockPathStart);
    assert!(menu_checked(start, &h.view()), "the check mark shows the lock");
    assert!(!menu_checked(MenuItem::Command(CommandKey::LockPathDirection), &h.view()));
}

#[test]
fn the_undo_step_of_a_lock_has_a_name() {
    use burnburn_document::UndoLabel;
    use burnburn_document::process::ProcessLabel;
    let label = MessageKey::UndoAction(UndoLabel::Process(ProcessLabel::SetPathLock));
    assert_eq!(burnburn_macos::adapter::message_text(label, Locale::Nl), "Herstel Startpunt of richting vergrendelen");
    assert_eq!(burnburn_macos::adapter::message_text(label, Locale::En), "Undo Lock Start Point or Direction");
}
