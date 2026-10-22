//! Productie (Production), Venster (Window) and Help. Windows has no application menu: the Mac
//! keeps About, Preferences and Shortcuts there, here they close the Help popup.

use super::{MenuNode, cmd, host, sep};
use crate::adapter::MenuItem;
use crate::app_menu::AppAction;
use crate::host_panels::menu::Item;
use burnburn_presentation::CommandKey;

pub(super) fn production_items() -> Vec<MenuNode> {
    vec![cmd(CommandKey::Compile), cmd(CommandKey::ExportRd)]
}

/// The libraries, then Preview, Inspect RD and the undo history. (The Mac adds its standard window
/// commands and the list of windows; Windows has the taskbar for that.)
pub(super) fn window_items() -> Vec<MenuNode> {
    vec![
        host(Item::Material),
        host(Item::Art),
        MenuNode::Item(MenuItem::OffsetPanel),
        MenuNode::Item(MenuItem::StructurePanel),
        MenuNode::Item(MenuItem::OperatorPanel),
        MenuNode::Item(MenuItem::RemnantsPanel),
        MenuNode::Item(MenuItem::RecognitionPanel),
        sep(),
        host(Item::App(AppAction::Preview)),
        host(Item::App(AppAction::InspectRd)),
        host(Item::App(AppAction::History)),
        host(Item::App(AppAction::MachineHub)),
    ]
}

pub(super) fn help_items() -> Vec<MenuNode> {
    vec![
        host(Item::App(AppAction::Manual)),
        sep(),
        host(Item::App(AppAction::SupportData)),
        host(Item::ExportDiagnostics),
        sep(),
        host(Item::App(AppAction::DebugLog)),
        sep(),
        host(Item::App(AppAction::Preferences)),
        host(Item::App(AppAction::Shortcuts)),
        host(Item::App(AppAction::About)),
    ]
}
