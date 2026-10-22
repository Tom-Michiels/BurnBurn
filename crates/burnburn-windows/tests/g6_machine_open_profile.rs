//! G6-machine on Windows, headless: Archief ▸ Machineprofiel openen… is in the file menu like on
//! the Mac, its open dialog filters JSON and the chosen path is the same `OpenMachineProfile` intent.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use burnburn_presentation::{CommandKey, HeadlessEditorHarness, HostEffect, UiIntent};
use burnburn_windows::adapter::MenuItem;
use burnburn_windows::host_panels::files::{FilesInput, effect_dialog, on_files_file_chosen};
use burnburn_windows::host_panels::{DialogRequest, FilePurpose, PanelReaction};
use burnburn_windows::l10n::Locale;
use burnburn_windows::menus::{MenuNode, menu_bar};

fn file_items() -> Vec<MenuItem> {
    fn walk(nodes: &[MenuNode], out: &mut Vec<MenuItem>) {
        for n in nodes {
            match n {
                MenuNode::Item(i) => out.push(*i),
                MenuNode::Separator => {}
                MenuNode::Submenu(s) => walk(&s.items, out),
            }
        }
    }
    let bar = menu_bar(Locale::En);
    let file = bar.iter().find(|m| m.id == "file").expect("a file menu");
    let mut out = Vec::new();
    walk(&file.items, &mut out);
    out
}

#[test]
fn the_file_menu_has_open_machine_profile() {
    assert!(file_items().contains(&MenuItem::Command(CommandKey::OpenMachineProfile)));
}

#[test]
fn the_effect_asks_for_a_json_file_and_the_answer_is_the_open_intent() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let vm = h.view();
    let inp = FilesInput {
        document: doc,
        vm: &vm,
        locale: Locale::Nl,
        recent_projects: &[],
        recent_templates: &[],
        template_scope: 0,
        recent_row: None,
        template_row: None,
    };
    match effect_dialog(&HostEffect::ShowMachineProfileOpenPanel, &inp) {
        Some(DialogRequest::Open { purpose, title, filters }) => {
            assert_eq!(purpose, FilePurpose::MachineProfileOpen);
            assert_eq!(title, "Kies een machineprofiel (JSON)");
            assert_eq!(filters[0].extensions, vec!["json"]);
            let path = PathBuf::from("C:/profielen/graveur.json");
            assert_eq!(
                on_files_file_chosen(&vm, &purpose, Some(path.clone())),
                PanelReaction::Intent(UiIntent::OpenMachineProfile { path })
            );
            assert_eq!(on_files_file_chosen(&vm, &purpose, None), PanelReaction::Nothing, "Cancel sends nothing");
        }
        other => panic!("{other:?}"),
    }
}
