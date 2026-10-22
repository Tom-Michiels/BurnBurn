//! G5-windows: menu mnemonics, the moved panel-family items, the dynamic recent submenus and the
//! accelerators the Mac menu has (Ctrl+Alt+I, Compile, Export RD), headless.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashSet;
use std::path::PathBuf;

use burnburn_presentation::{CommandKey, HeadlessEditorHarness, UiIntent};
use burnburn_windows::adapter::{
    MenuItem, accelerator_label, accelerators, command_id, menu_item, menu_items, menu_title,
};
use burnburn_windows::host_panels::PanelReaction;
use burnburn_windows::host_panels::menu::{Item, MenuContext, reaction};
use burnburn_windows::l10n::Locale;
use burnburn_windows::menus::mnemonics::{assign, numbered, reapply};
use burnburn_windows::menus::{MenuNode, Submenu, menu_bar};

fn s(list: &[&str]) -> Vec<String> {
    list.iter().map(|t| (*t).to_owned()).collect()
}

/// The letter after the single `&` of a caption (`&&` is a literal ampersand).
fn mnemonic(caption: &str) -> Option<char> {
    let mut chars = caption.chars();
    while let Some(c) = chars.next() {
        if c == '&' {
            match chars.next() {
                Some('&') => continue,
                other => return other.map(|c| c.to_ascii_lowercase()),
            }
        }
    }
    None
}

#[test]
fn a_mnemonic_letter_is_used_once_per_menu_level() {
    let got = assign(&s(&["Open", "Opslaan", "Opslaan als…", "Sluiten"]));
    assert_eq!(got.len(), 4);
    let letters: Vec<char> = got.iter().filter_map(|c| mnemonic(c)).collect();
    assert_eq!(letters.len(), 4, "{got:?}");
    assert_eq!(letters.iter().collect::<HashSet<_>>().len(), 4, "{got:?}");
    assert_eq!(got[0], "&Open", "the first title takes its first letter");
}

#[test]
fn the_first_letter_of_a_later_word_is_preferred_over_a_letter_inside_a_word() {
    assert_eq!(assign(&s(&["Save As", "Select All"])), ["&Save As", "Select &All"]);
}

#[test]
fn a_literal_ampersand_in_a_title_is_doubled_and_never_the_mnemonic() {
    assert_eq!(assign(&s(&["Fill & Cut"])), ["&Fill && Cut"]);
    assert_eq!(mnemonic("&Fill && Cut"), Some('f'));
}

#[test]
fn a_title_without_a_free_letter_gets_no_mnemonic_and_stays_readable() {
    let got = assign(&s(&["aa", "aa", "…"]));
    assert_eq!(got, ["&aa", "aa", "…"]);
}

#[test]
fn a_changing_title_keeps_the_letter_that_was_underlined() {
    assert_eq!(reapply("&Ongedaan maken", "Ongedaan maken: Verplaats"), "&Ongedaan maken: Verplaats");
    assert_eq!(reapply("Ongedaan &maken", "Ongedaan maken: Verplaats"), "Ongedaan &maken: Verplaats");
    assert_eq!(reapply("&Redo", "Opnieuw"), "Opnieuw", "the letter is gone from the new title");
    assert_eq!(reapply("Plain", "Other"), "Other");
    assert_eq!(reapply("Fill && &Cut", "Fill & Cut now"), "Fill && &Cut now");
}

fn level_titles(nodes: &[MenuNode], vm: &burnburn_presentation::AppViewModel, locale: Locale) -> Vec<String> {
    nodes
        .iter()
        .filter_map(|n| match n {
            MenuNode::Item(item) => Some(menu_title(*item, vm, locale)),
            MenuNode::Submenu(sub) => Some(sub.title.clone()),
            MenuNode::Separator => None,
        })
        .collect()
}

fn check_levels(nodes: &[MenuNode], vm: &burnburn_presentation::AppViewModel, locale: Locale, path: &str) {
    let titles = level_titles(nodes, vm, locale);
    let captions = assign(&titles);
    assert_eq!(captions.len(), titles.len(), "{path}");
    let letters: Vec<char> = captions.iter().filter_map(|c| mnemonic(c)).collect();
    assert_eq!(letters.iter().collect::<HashSet<_>>().len(), letters.len(), "{path}: {captions:?}");
    assert!(letters.len() * 10 >= captions.len() * 8, "{path}: most items have a mnemonic: {captions:?}");
    for (title, caption) in titles.iter().zip(&captions) {
        assert_eq!(&caption.replace("&&", "\u{0}").replace('&', "").replace('\u{0}', "&"), title, "{path}");
    }
    for node in nodes {
        if let MenuNode::Submenu(sub) = node {
            check_levels(&sub.items, vm, locale, &format!("{path}/{}", sub.title));
        }
    }
}

#[test]
fn every_level_of_the_real_menu_bar_has_distinct_mnemonics_in_both_languages() {
    let vm = HeadlessEditorHarness::new().view();
    for locale in [Locale::Nl, Locale::En] {
        let bar = menu_bar(locale);
        let tops: Vec<MenuNode> = bar
            .iter()
            .map(|p| MenuNode::Submenu(Submenu { id: p.id, title: p.title.clone(), items: p.items.clone() }))
            .collect();
        check_levels(&tops, &vm, locale, "bar");
    }
}

#[test]
fn the_production_popup_has_compile_and_export_rd_with_the_mac_shortcuts() {
    let bar = menu_bar(Locale::En);
    let production = bar.iter().find(|p| p.id == "production").expect("production popup");
    let items: Vec<&MenuNode> = production.items.iter().collect();
    assert_eq!(
        items,
        [
            &MenuNode::Item(MenuItem::Command(CommandKey::Compile)),
            &MenuNode::Item(MenuItem::Command(CommandKey::ExportRd))
        ]
    );
    assert_eq!(accelerator_label(MenuItem::Command(CommandKey::Compile)).as_deref(), Some("Ctrl+B"));
    assert_eq!(accelerator_label(MenuItem::Command(CommandKey::ExportRd)).as_deref(), Some("Ctrl+Shift+E"));
}

#[test]
fn invert_selection_is_ctrl_alt_i_and_the_label_says_so() {
    let invert = MenuItem::Command(CommandKey::InvertSelection);
    let a = accelerators().into_iter().find(|a| a.id == command_id(invert)).expect("accelerator");
    assert!(a.ctrl && a.alt && !a.shift && a.key == u16::from(b'I'), "{a:?}");
    assert_eq!(accelerator_label(invert).as_deref(), Some("Ctrl+Alt+I"));
    // No other accelerator uses the same chord.
    let same = accelerators().into_iter().filter(|b| (b.key, b.ctrl, b.alt, b.shift) == (a.key, true, true, false));
    assert_eq!(same.count(), 1);
}

#[test]
fn no_two_accelerators_share_a_chord() {
    let mut seen = HashSet::new();
    for a in accelerators() {
        assert!(seen.insert((a.key, a.ctrl, a.alt, a.shift)), "{a:?} twice");
    }
}

#[test]
fn the_panel_family_items_sit_in_the_file_laser_image_window_and_help_popups() {
    let bar = menu_bar(Locale::En);
    let flat = |id: &str| -> Vec<MenuItem> {
        fn walk(nodes: &[MenuNode], out: &mut Vec<MenuItem>) {
            for n in nodes {
                match n {
                    MenuNode::Item(i) => out.push(*i),
                    MenuNode::Submenu(s) => walk(&s.items, out),
                    MenuNode::Separator => {}
                }
            }
        }
        let mut out = Vec::new();
        walk(&bar.iter().find(|p| p.id == id).unwrap().items, &mut out);
        out
    };
    let host = MenuItem::Host;
    assert!(flat("file").contains(&host(Item::Import)));
    assert!(flat("file").contains(&host(Item::BundleExport)));
    assert!(flat("laser").contains(&host(Item::LaserWindow)));
    assert!(flat("laser").contains(&host(Item::CameraWindow)));
    assert!(flat("image").contains(&host(Item::ImportImage)));
    assert!(flat("image").contains(&host(Item::TraceImage)));
    assert!(flat("window").contains(&host(Item::App(burnburn_windows::app_menu::AppAction::Preview))));
    assert!(flat("help").contains(&host(Item::ExportDiagnostics)));
    assert!(flat("help").contains(&host(Item::App(burnburn_windows::app_menu::AppAction::About))));
}

#[test]
fn the_file_popup_has_both_recent_lists_as_dynamic_submenus() {
    let bar = menu_bar(Locale::Nl);
    let file = bar.iter().find(|p| p.id == "file").unwrap();
    let dynamic: Vec<(&str, &str)> = file
        .items
        .iter()
        .filter_map(|n| match n {
            MenuNode::Submenu(s) if s.id.starts_with("recent_") => Some((s.id, s.title.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(dynamic.len(), 2, "{dynamic:?}");
    assert_eq!(dynamic[0].0, "recent_templates");
    assert_eq!(dynamic[1].0, "recent_projects");
    assert!(dynamic.iter().all(|(_, title)| !title.is_empty()));
}

#[test]
fn every_host_item_id_maps_back_and_none_collides_with_a_command() {
    let items = menu_items();
    let mut ids = HashSet::new();
    for item in &items {
        assert!(ids.insert(command_id(*item)), "{item:?}");
        assert_eq!(menu_item(command_id(*item)), Some(*item));
    }
    assert!(items.contains(&MenuItem::Host(Item::ClearRecentProjects)));
}

#[test]
fn clearing_the_recent_projects_sends_the_intent_and_is_off_for_an_empty_list() {
    let mut h = HeadlessEditorHarness::new();
    h.new_document().unwrap();
    let vm = h.view();
    let recent = [PathBuf::from("C:/a.burnburn")];
    let none: [PathBuf; 0] = [];
    fn ctx<'a>(vm: &'a burnburn_presentation::AppViewModel, recent: &'a [PathBuf]) -> MenuContext<'a> {
        MenuContext { vm, locale: Locale::En, manual_known: true, recent_projects: recent, recent_templates: &[] }
    }
    assert!(Item::ClearRecentProjects.enabled(&ctx(&vm, &recent)));
    assert!(!Item::ClearRecentProjects.enabled(&ctx(&vm, &none)));
    assert_eq!(
        reaction(Item::ClearRecentProjects, &ctx(&vm, &recent)),
        PanelReaction::Intent(UiIntent::ClearRecentProjects)
    );
    assert_eq!(reaction(Item::ClearRecentProjects, &ctx(&vm, &none)), PanelReaction::Nothing);
}

#[test]
fn recent_entries_are_numbered_with_the_digit_as_mnemonic_for_the_first_nine() {
    assert_eq!(numbered(0, "plaat.burnburn"), "&1 plaat.burnburn");
    assert_eq!(numbered(8, "x"), "&9 x");
    assert_eq!(numbered(9, "x"), "10 x", "no single-digit mnemonic for the tenth");
    assert_eq!(numbered(0, "A&B.burnburn"), "&1 A&&B.burnburn", "a literal ampersand is doubled");
}
