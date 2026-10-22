//! G6-machine on the Mac: Archief ▸ Machineprofiel openen… is a menu item that needs no window,
//! and the profile file chosen in the open panel is sent as `OpenMachineProfile`.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::path::PathBuf;

use burnburn_macos::adapter::{MenuItem, menu_enabled, menu_item, menu_items, menu_tag, menu_title};
use burnburn_macos::l10n::Locale;
use burnburn_macos::l10n_files::{FilesText, files_text};
use burnburn_macos::panels_files::machine_profile_open_intent;
use burnburn_presentation::{CommandKey, HeadlessEditorHarness, MessageKey, UiIntent};

#[test]
fn the_command_is_a_menu_item_in_both_languages_that_is_on_without_a_window() {
    let item = MenuItem::Command(CommandKey::OpenMachineProfile);
    assert!(menu_items().contains(&item));
    assert_eq!(menu_item(menu_tag(item)), Some(item));
    let h = HeadlessEditorHarness::new();
    assert!(menu_enabled(item, &h.view()), "choosing a machine does not need an open project");
    assert_eq!(menu_title(item, &h.view(), Locale::Nl), "Machineprofiel openen…");
    assert_eq!(menu_title(item, &h.view(), Locale::En), "Open Machine Profile…");
}

#[test]
fn the_chosen_file_is_sent_as_it_is() {
    let path = PathBuf::from("/tmp/graveur.json");
    assert_eq!(machine_profile_open_intent(path.clone()), UiIntent::OpenMachineProfile { path });
}

#[test]
fn the_panel_and_the_refusal_are_said_in_both_languages() {
    assert_eq!(files_text(FilesText::PanelMachineProfileOpen, Locale::Nl), "Kies een machineprofiel (JSON)");
    assert_eq!(files_text(FilesText::PanelMachineProfileOpen, Locale::En), "Choose a machine profile (JSON)");
    let nl = burnburn_macos::adapter::message_text(MessageKey::MachineProfileFileUnusable, Locale::Nl);
    assert!(nl.contains("geen bruikbaar machineprofiel"), "{nl}");
    let en = burnburn_macos::adapter::message_text(MessageKey::MachineProfileFileUnusable, Locale::En);
    assert!(en.contains("not a usable machine profile"), "{en}");
}
