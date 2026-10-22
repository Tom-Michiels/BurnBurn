//! Archief (File), in the order of the macOS menu: new (and new from a template with the recent
//! templates), open (with the recent projects), import, export, bundles, close/save, save as
//! template. The two recent lists are dynamic submenus: the Win32 layer fills them from the stored
//! lists in every open window. The Files window is Windows-only and sits last.

use super::{MenuNode, cmd, host, nested, sep};
use crate::adapter::MenuItem;
use crate::host_panels::menu::Item;
use crate::l10n::{HostText, Locale, host_text};
use crate::l10n_files::{FilesText, files_text};
use burnburn_presentation::CommandKey;

/// A submenu whose entries come from a stored list (`id` names the list), ending in "clear".
fn recent(id: &'static str, title: &str, clear: Item) -> MenuNode {
    match nested(title, vec![sep(), host(clear)]) {
        MenuNode::Submenu(mut s) => {
            s.id = id;
            MenuNode::Submenu(s)
        }
        other => other,
    }
}

pub(super) fn items(locale: Locale) -> Vec<MenuNode> {
    vec![
        cmd(CommandKey::New),
        host(Item::NewTemplate),
        recent(
            "recent_templates",
            files_text(FilesText::MenuNewFromRecentTemplate, locale),
            Item::ClearRecentTemplates,
        ),
        MenuNode::Item(MenuItem::Open),
        recent("recent_projects", host_text(HostText::RecentProjects, locale), Item::ClearRecentProjects),
        host(Item::Import),
        nested(
            files_text(FilesText::MenuExportSubmenu, locale),
            vec![host(Item::ExportSvg), host(Item::ExportDxf), cmd(CommandKey::ExportGcode)],
        ),
        nested(
            files_text(FilesText::MenuBundleSubmenu, locale),
            vec![host(Item::BundleExport), host(Item::BundleImport)],
        ),
        cmd(CommandKey::OpenMachineProfile),
        sep(),
        cmd(CommandKey::Close),
        cmd(CommandKey::Save),
        cmd(CommandKey::SaveAs),
        host(Item::SaveTemplate),
        cmd(CommandKey::ShowNotes),
        sep(),
        host(Item::FilesPanel),
    ]
}
