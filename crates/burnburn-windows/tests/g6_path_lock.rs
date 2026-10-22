//! G6-machine, C06 on Windows, headless: Shapes ▸ Lock Start Point and Lock Direction are in the
//! Shapes menu like on the Mac.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use burnburn_presentation::CommandKey;
use burnburn_windows::adapter::MenuItem;
use burnburn_windows::l10n::Locale;
use burnburn_windows::menus::{MenuNode, menu_bar};

fn all_items(nodes: &[MenuNode], out: &mut Vec<MenuItem>) {
    for n in nodes {
        match n {
            MenuNode::Item(i) => out.push(*i),
            MenuNode::Separator => {}
            MenuNode::Submenu(s) => all_items(&s.items, out),
        }
    }
}

#[test]
fn the_shapes_menu_has_both_locks() {
    let bar = menu_bar(Locale::En);
    let mut out = Vec::new();
    for menu in &bar {
        all_items(&menu.items, &mut out);
    }
    for key in [CommandKey::LockPathStart, CommandKey::LockPathDirection] {
        assert_eq!(out.iter().filter(|i| **i == MenuItem::Command(key)).count(), 1, "{key:?} once in the menu bar");
    }
}
