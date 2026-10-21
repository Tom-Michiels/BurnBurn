//! Menu items, command ids, accelerators and enablement of the Windows host, without the `windows`
//! crate. The menu *structure* (which item sits in which popup) lives in the host menu tables; this file
//! says what an item is and which `WM_COMMAND` id it carries.

use burnburn_presentation::{AppViewModel, CommandKey};

/// A menu item: Open needs `IFileOpenDialog` first; the rest are presentation commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuItem {
    Command(CommandKey),
    Open,
}

/// Every menu item the host offers.
pub fn menu_items() -> Vec<MenuItem> {
    std::iter::once(MenuItem::Open).chain(CommandKey::ALL.iter().map(|k| MenuItem::Command(*k))).collect()
}

/// Command ids: Open is 100, commands 101 + their index in `CommandKey::ALL`.
const FIRST_ID: u16 = 100;

/// `WM_COMMAND` id of a menu item (also used by the accelerator table).
pub fn command_id(item: MenuItem) -> u16 {
    match item {
        MenuItem::Open => FIRST_ID,
        MenuItem::Command(key) => CommandKey::ALL
            .iter()
            .position(|k| *k == key)
            .and_then(|i| u16::try_from(i).ok())
            .map_or(0, |i| FIRST_ID + 1 + i),
    }
}

pub fn menu_item(id: u16) -> Option<MenuItem> {
    match id.checked_sub(FIRST_ID)? {
        0 => Some(MenuItem::Open),
        n => CommandKey::ALL.get(usize::from(n - 1)).map(|k| MenuItem::Command(*k)),
    }
}

/// An accelerator: virtual key, Ctrl, Shift, command id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Accelerator {
    pub key: u16,
    pub ctrl: bool,
    pub shift: bool,
    pub id: u16,
}

/// The accelerator table (Ctrl is the shortcut modifier on Windows; presentation-api.md).
pub fn accelerators() -> Vec<Accelerator> {
    let c = MenuItem::Command;
    [
        (b'N', false, c(CommandKey::New)),
        (b'O', false, MenuItem::Open),
        (b'S', false, c(CommandKey::Save)),
        (b'S', true, c(CommandKey::SaveAs)),
        (b'W', false, c(CommandKey::Close)),
        (b'Z', false, c(CommandKey::Undo)),
        (b'Y', false, c(CommandKey::Redo)),
        (b'Z', true, c(CommandKey::Redo)),
        (b'A', false, c(CommandKey::SelectAll)),
    ]
    .into_iter()
    .map(|(key, shift, item)| Accelerator { key: u16::from(key), ctrl: true, shift, id: command_id(item) })
    .collect()
}

/// Menu enablement from the view model (`EnableMenuItem` on `WM_INITMENUPOPUP`).
pub fn menu_enabled(item: MenuItem, vm: &AppViewModel) -> bool {
    match item {
        MenuItem::Open => true,
        MenuItem::Command(key) => vm.commands.get(&key).is_some_and(|c| c.enabled),
    }
}
