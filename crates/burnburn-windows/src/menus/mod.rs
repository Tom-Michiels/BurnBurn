//! The menu bar of the Windows host as data (no `windows` crate): which item sits in which popup.
//! `win32::menu` turns it into `HMENU`s; `tests/menus.rs` compares it with the macOS menu.
//!
//! One file per popup. Every popup lists the same items in the same order as `main_menu` in the
//! macOS host (`appkit/app.rs`), so a command reachable there is reachable here.
//!
//! The items of the panel families (laser window, import, export, bundles, templates, images,
//! camera, Window, Help) are [`MenuItem::Host`](crate::adapter::MenuItem::Host) entries in the same
//! popups as on the Mac (G5-windows); `tests/menus.rs` compares every popup with the macOS source.

use crate::adapter::MenuItem;
use crate::app_text::{AppText, app_text};
use crate::l10n::{HostText, Locale, host_text};
use burnburn_presentation::CommandKey;

mod app;
mod arrange;
mod edit;
mod file;
mod image;
mod laser;
pub mod mnemonics;
mod tools;
mod view;

/// One entry of a popup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuNode {
    Item(MenuItem),
    Separator,
    Submenu(Submenu),
}

/// A popup with its title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Submenu {
    /// Stable name of a top-level menu (`file`, `edit`, ...); empty for nested popups.
    pub id: &'static str,
    pub title: String,
    pub items: Vec<MenuNode>,
}

/// A command item.
pub(crate) fn cmd(key: CommandKey) -> MenuNode {
    MenuNode::Item(MenuItem::Command(key))
}

/// Several command items in a row.
pub(crate) fn cmds(keys: &[CommandKey]) -> Vec<MenuNode> {
    keys.iter().map(|k| cmd(*k)).collect()
}

pub(crate) fn sep() -> MenuNode {
    MenuNode::Separator
}

/// An item of the panel families.
pub(crate) fn host(item: crate::host_panels::menu::Item) -> MenuNode {
    MenuNode::Item(MenuItem::Host(item))
}

/// A nested popup.
pub(crate) fn nested(title: impl Into<String>, items: Vec<MenuNode>) -> MenuNode {
    MenuNode::Submenu(Submenu { id: "", title: title.into(), items })
}

/// A top-level popup.
pub(crate) fn popup(id: &'static str, title: impl Into<String>, items: Vec<MenuNode>) -> Submenu {
    Submenu { id, title: title.into(), items }
}

/// The menu bar, left to right (the order of the macOS menu).
pub fn menu_bar(locale: Locale) -> Vec<Submenu> {
    let t = |text: HostText| host_text(text, locale);
    vec![
        popup("file", t(HostText::MenuFile), file::items(locale)),
        popup("edit", t(HostText::MenuEdit), edit::items(locale)),
        popup("view", t(HostText::MenuView), view::items()),
        popup("tools", t(HostText::MenuTools), tools::tools_items(locale)),
        popup("arrange", t(HostText::MenuArrange), arrange::items(locale)),
        popup("text", crate::panels::text_text(crate::panels::TextText::MenuText, locale), tools::text_items()),
        popup("laser", crate::panels::laser_text(crate::panels::LaserText::MenuLaser, locale), laser::items(locale)),
        popup("window", app_text(AppText::MenuWindow, locale), app::window_items()),
        popup("help", app_text(AppText::MenuHelp, locale), app::help_items()),
    ]
}
