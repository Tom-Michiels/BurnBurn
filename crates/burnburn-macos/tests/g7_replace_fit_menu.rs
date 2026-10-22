//! Beeld ▸ Beeld vervangen en passen… (G7-trace, T09): the menu item, its words, the file panel
//! and the intent a chosen file becomes, headless. The NSOpenPanel itself is the same sheet as that
//! of Replace Image.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use burnburn_macos::adapter::{MenuItem, NativeEvent, Reaction, menu_enabled, menu_title, translate};
use burnburn_macos::l10n::Locale;
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, UiIntent};

#[test]
fn the_menu_item_opens_the_file_panel_and_a_chosen_file_becomes_replace_to_fit() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().expect("window");
    let vm = h.view();
    assert_eq!(
        translate(NativeEvent::Menu(MenuItem::Command(CommandKey::ReplaceImageToFit)), &vm),
        Reaction::ShowReplaceToFitPanel
    );
    let path = PathBuf::from("/tmp/foto.png");
    assert_eq!(
        translate(NativeEvent::ReplaceToFitFileChosen { document: doc, path: Some(path.clone()) }, &vm),
        Reaction::Intent(UiIntent::ReplaceImageToFit { document: doc, path })
    );
    assert_eq!(translate(NativeEvent::ReplaceToFitFileChosen { document: doc, path: None }, &vm), Reaction::Nothing);
}

#[test]
fn the_item_has_its_words_in_both_languages_and_follows_the_view_model() {
    let mut h = HeadlessEditorHarness::new();
    h.new_document().expect("window");
    let vm = h.view();
    let item = MenuItem::Command(CommandKey::ReplaceImageToFit);
    assert_eq!(menu_title(item, &vm, Locale::Nl), "Beeld vervangen en passen…");
    assert_eq!(menu_title(item, &vm, Locale::En), "Replace to Fit…");
    assert!(!menu_enabled(item, &vm), "no image selected");
    assert_eq!(menu_enabled(item, &vm), vm.commands.get(&CommandKey::ReplaceImageToFit).is_some_and(|c| c.enabled));
}
