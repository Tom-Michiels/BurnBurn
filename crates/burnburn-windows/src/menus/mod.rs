//! The menu bar of the Windows host as data (no `windows` crate): which item sits in which popup.
//! `win32::menu` turns it into `HMENU`s; `tests/menus.rs` compares it with the macOS menu.
//!
//! One file per popup. Every popup lists the same items in the same order as `main_menu` in the
//! macOS host (`appkit/app.rs`), so a command reachable there is reachable here.
//!
//! Insertion points for the other lane (W4-L8b): `file` (import, export, bundles, templates,
//! recent), `laser` (the laser window), and new popups `image`, `production`, `window`, `help` in
//! [`menu_bar`] — each as a new line, nothing here needs to move.

use crate::adapter::MenuItem;
use crate::l10n::{HostText, Locale, host_text};
use burnburn_presentation::CommandKey;

mod arrange;
mod edit;
mod file;
mod laser;
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
        popup("file", t(HostText::MenuFile), file::items()),
        popup("edit", t(HostText::MenuEdit), edit::items()),
        popup("arrange", t(HostText::MenuArrange), arrange::items(locale)),
        popup("tools", t(HostText::MenuTools), tools::tools_items()),
        popup("view", t(HostText::MenuView), view::items()),
        popup("shapes", t(HostText::MenuShapes), tools::shapes_items()),
        popup("text", crate::panels::text_text(crate::panels::TextText::MenuText, locale), tools::text_items()),
        popup("sketch", t(HostText::MenuSketch), tools::sketch_items()),
        popup("laser", crate::panels::laser_text(crate::panels::LaserText::MenuLaser, locale), laser::items()),
    ]
}
