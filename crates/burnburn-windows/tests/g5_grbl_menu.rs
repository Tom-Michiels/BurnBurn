//! G5-grbl-ui on Windows, headless: Archief ▸ Exporteer G-code is in the file menu like on the Mac,
//! and its save dialog hands the chosen path to the same `ExportGcode` intent. The Windows host
//! has no GRBL window yet (the laser window of a GRBL machine is a Mac-first slice); the export
//! and the preflight refusals come from the shared presentation layer.

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
fn the_file_menu_has_export_gcode() {
    assert!(file_items().contains(&MenuItem::Command(CommandKey::ExportGcode)));
}

#[test]
fn the_gcode_effect_asks_for_a_gcode_file_and_the_answer_is_the_export() {
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
    let effect = HostEffect::ShowGcodeExportPanel { document: doc, suggested_name: "Plaat.gcode".into() };
    match effect_dialog(&effect, &inp) {
        Some(DialogRequest::Save { purpose, suggested_name, filters, .. }) => {
            assert_eq!(suggested_name, "Plaat.gcode");
            assert_eq!(filters[0].extensions, vec!["gcode"]);
            assert_eq!(purpose, FilePurpose::ExportGcode { document: doc });
            let path = PathBuf::from("C:/o/plaat.gcode");
            assert_eq!(
                on_files_file_chosen(&vm, &purpose, Some(path.clone())),
                PanelReaction::Intent(UiIntent::ExportGcode { document: doc, path })
            );
            assert_eq!(on_files_file_chosen(&vm, &purpose, None), PanelReaction::Nothing, "Cancel sends nothing");
        }
        other => panic!("{other:?}"),
    }
}
