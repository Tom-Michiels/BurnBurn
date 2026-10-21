//! Menu items, command ids, accelerators and enablement of the Windows host, without the `windows`
//! crate. The menu *structure* (which item sits in which popup) lives in the host menu tables; this file
//! says what an item is and which `WM_COMMAND` id it carries.

use burnburn_presentation::{AppViewModel, CamDialog, CommandKey, MessageKey};

use crate::l10n::{HostText, Locale, host_text, message_or_key};

/// A menu item: Open needs `IFileOpenDialog` first; the rest are presentation commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuItem {
    Command(CommandKey),
    Open,
    /// Bewerken / Laser: one of the CAM dialogs; presentation opens it for the active window.
    CamDialog(CamDialog),
    /// Schikken: shows or hides the Schikken options panel of the active window.
    SchikkenPanel,
}

/// Every menu item the host offers.
pub fn menu_items() -> Vec<MenuItem> {
    [MenuItem::Open, MenuItem::SchikkenPanel]
        .into_iter()
        .chain(CamDialog::ALL.iter().map(|d| MenuItem::CamDialog(*d)))
        .chain(CommandKey::ALL.iter().map(|k| MenuItem::Command(*k)))
        .collect()
}

/// Command ids: Open is 100, commands 101 + their index in `CommandKey::ALL`. The host items use
/// the macOS menu tags, so an id means the same item on both hosts: the Schikken panel is 1007 and
/// CAM dialog `n` (`CamDialog::ALL` order) is 4000 + `n`. Other lanes' host items take the other
/// macOS tags (1001–1008, 2000+, 3000+).
const FIRST_ID: u16 = 100;
const SCHIKKEN_PANEL_ID: u16 = 1007;
const CAM_ID_BASE: u16 = 4000;

/// `WM_COMMAND` id of a menu item (also used by the accelerator table).
pub fn command_id(item: MenuItem) -> u16 {
    match item {
        MenuItem::Open => FIRST_ID,
        MenuItem::SchikkenPanel => SCHIKKEN_PANEL_ID,
        MenuItem::CamDialog(dialog) => CamDialog::ALL
            .iter()
            .position(|d| *d == dialog)
            .and_then(|i| u16::try_from(i).ok())
            .map_or(0, |i| CAM_ID_BASE + i),
        MenuItem::Command(key) => CommandKey::ALL
            .iter()
            .position(|k| *k == key)
            .and_then(|i| u16::try_from(i).ok())
            .map_or(0, |i| FIRST_ID + 1 + i),
    }
}

pub fn menu_item(id: u16) -> Option<MenuItem> {
    if id == SCHIKKEN_PANEL_ID {
        return Some(MenuItem::SchikkenPanel);
    }
    if let Some(i) = id.checked_sub(CAM_ID_BASE) {
        return CamDialog::ALL.get(usize::from(i)).map(|d| MenuItem::CamDialog(*d));
    }
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

/// The accelerator table (Ctrl is the shortcut modifier on Windows; presentation-api.md). The keys
/// are the macOS menu's key equivalents with Cmd read as Ctrl; the `OEM` keys are the US-layout
/// names of `=`, `-`, `[` and `]` (`VK_OEM_PLUS`, `VK_OEM_MINUS`, `VK_OEM_4`, `VK_OEM_6`).
pub fn accelerators() -> Vec<Accelerator> {
    let c = MenuItem::Command;
    const OEM_PLUS: u8 = 0xBB;
    const OEM_MINUS: u8 = 0xBD;
    const OEM_4: u8 = 0xDB;
    const OEM_6: u8 = 0xDD;
    [
        (b'N', false, c(CommandKey::New)),
        (b'O', false, MenuItem::Open),
        (b'S', false, c(CommandKey::Save)),
        (b'S', true, c(CommandKey::SaveAs)),
        (b'W', false, c(CommandKey::Close)),
        (b'Z', false, c(CommandKey::Undo)),
        (b'Y', false, c(CommandKey::Redo)),
        (b'Z', true, c(CommandKey::Redo)),
        (b'X', false, c(CommandKey::Cut)),
        (b'C', false, c(CommandKey::Copy)),
        (b'V', false, c(CommandKey::Paste)),
        (b'V', true, c(CommandKey::PasteInPlace)),
        (b'D', false, c(CommandKey::Duplicate)),
        (b'A', false, c(CommandKey::SelectAll)),
        (b'G', false, c(CommandKey::Group)),
        (b'G', true, c(CommandKey::Ungroup)),
        (OEM_6, false, c(CommandKey::PushForward)),
        (OEM_4, false, c(CommandKey::PushBackward)),
        (OEM_6, true, c(CommandKey::PushToFront)),
        (OEM_4, true, c(CommandKey::PushToBack)),
        (b'0', false, c(CommandKey::ZoomPage)),
        (b'0', true, c(CommandKey::ZoomSelection)),
        (OEM_PLUS, false, c(CommandKey::ZoomIn)),
        (OEM_MINUS, false, c(CommandKey::ZoomOut)),
    ]
    .into_iter()
    .map(|(key, shift, item)| Accelerator { key: u16::from(key), ctrl: true, shift, id: command_id(item) })
    .collect()
}

/// The accelerator text of a menu item (`Ctrl+Shift+S`), for the right-hand column of the popup.
pub fn accelerator_label(item: MenuItem) -> Option<String> {
    let id = command_id(item);
    let a = accelerators().into_iter().find(|a| a.id == id)?;
    let key = match a.key {
        0xBB => "=".to_owned(),
        0xBD => "-".to_owned(),
        0xDB => "[".to_owned(),
        0xDD => "]".to_owned(),
        k => char::from(u8::try_from(k).ok()?).to_string(),
    };
    Some(format!("Ctrl+{}{key}", if a.shift { "Shift+" } else { "" }))
}

/// Menu enablement from the view model (`EnableMenuItem` on `WM_INITMENUPOPUP`).
pub fn menu_enabled(item: MenuItem, vm: &AppViewModel) -> bool {
    match item {
        MenuItem::Open => true,
        // A dialog or panel belongs to a window; its own greying says why in words.
        MenuItem::CamDialog(_) | MenuItem::SchikkenPanel => vm.active.is_some(),
        MenuItem::Command(key) => vm.commands.get(&key).is_some_and(|c| c.enabled),
    }
}

/// Check mark state (active tool, snapping toggles, wireframe).
pub fn menu_checked(item: MenuItem, vm: &AppViewModel) -> bool {
    match item {
        MenuItem::Command(key) => vm.commands.get(&key).is_some_and(|c| c.checked),
        MenuItem::Open | MenuItem::CamDialog(_) | MenuItem::SchikkenPanel => false,
    }
}

/// Menu title in `locale`, including the undo/redo action name from the view model.
pub fn menu_title(item: MenuItem, vm: &AppViewModel, locale: Locale) -> String {
    match item {
        MenuItem::Open => host_text(HostText::Open, locale).to_owned(),
        MenuItem::CamDialog(dialog) => crate::panels::cam_menu_title(dialog, locale),
        MenuItem::SchikkenPanel => crate::panels::schikken_text(crate::panels::SchikkenText::MenuPanel, locale),
        MenuItem::Command(key) => {
            let label = vm.commands.get(&key).map_or(MessageKey::Command(key), |c| c.label);
            message_or_key(label, locale)
        }
    }
}
