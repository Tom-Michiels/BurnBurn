//! Menu items, command ids, accelerators and enablement of the Windows host, without the `windows`
//! crate. The menu *structure* (which item sits in which popup) lives in the host menu tables; this file
//! says what an item is and which `WM_COMMAND` id it carries.

use burnburn_presentation::{AppViewModel, CamDialog, CommandKey, MessageKey};

use crate::host_panels::menu::{Item as HostItem, MenuContext};
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
    /// Venster: shows or hides the offset and repair panel of the active window (G5-native).
    OffsetPanel,
    /// Venster: shows or hides the structure panel of the active window (G5-native).
    StructurePanel,
    /// Beeld: shows or hides the image preparation panel of the active window (G6-werk).
    TracePrepPanel,
    /// Venster: shows or hides the compact operator view of the active window (G6-werk).
    OperatorPanel,
    /// Venster: shows or hides the remnant library panel of the active window (G7-werk).
    RemnantsPanel,
    /// An item of the panel families (laser window, import, export, bundles, templates, images,
    /// camera, help; ids `6000..6500`): what it does is decided in [`crate::host_panels::menu`].
    Host(HostItem),
}

/// Every menu item the host offers. The recent-list entries are not in it: they come and go with
/// the stored lists (as `Recent(n)` on the Mac).
pub fn menu_items() -> Vec<MenuItem> {
    [
        MenuItem::Open,
        MenuItem::SchikkenPanel,
        MenuItem::OffsetPanel,
        MenuItem::StructurePanel,
        MenuItem::TracePrepPanel,
        MenuItem::OperatorPanel,
        MenuItem::RemnantsPanel,
    ]
    .into_iter()
    .chain(crate::host_panels::menu::items().into_iter().map(MenuItem::Host))
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
/// The golf-5 panels use the macOS tags too (G5-native).
const OFFSET_PANEL_ID: u16 = 1030;
const STRUCTURE_PANEL_ID: u16 = 1031;
/// The golf-6 workshop panels (G6-werk).
const TRACE_PREP_PANEL_ID: u16 = 1032;
const OPERATOR_PANEL_ID: u16 = 1033;
/// The golf-7 workshop panels (G7-werk), the macOS tag.
const REMNANTS_PANEL_ID: u16 = 1070;
const CAM_ID_BASE: u16 = 4000;

/// `WM_COMMAND` id of a menu item (also used by the accelerator table).
pub fn command_id(item: MenuItem) -> u16 {
    match item {
        MenuItem::Open => FIRST_ID,
        MenuItem::Host(item) => item.id(),
        MenuItem::SchikkenPanel => SCHIKKEN_PANEL_ID,
        MenuItem::OffsetPanel => OFFSET_PANEL_ID,
        MenuItem::StructurePanel => STRUCTURE_PANEL_ID,
        MenuItem::TracePrepPanel => TRACE_PREP_PANEL_ID,
        MenuItem::OperatorPanel => OPERATOR_PANEL_ID,
        MenuItem::RemnantsPanel => REMNANTS_PANEL_ID,
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
    if let Some(item) = HostItem::from_id(id) {
        return Some(MenuItem::Host(item));
    }
    if id == OFFSET_PANEL_ID {
        return Some(MenuItem::OffsetPanel);
    }
    if id == STRUCTURE_PANEL_ID {
        return Some(MenuItem::StructurePanel);
    }
    if id == TRACE_PREP_PANEL_ID {
        return Some(MenuItem::TracePrepPanel);
    }
    if id == OPERATOR_PANEL_ID {
        return Some(MenuItem::OperatorPanel);
    }
    if id == REMNANTS_PANEL_ID {
        return Some(MenuItem::RemnantsPanel);
    }
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

/// An accelerator: virtual key, Ctrl, Alt, Shift, command id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Accelerator {
    pub key: u16,
    pub ctrl: bool,
    pub alt: bool,
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
    let host = MenuItem::Host;
    // (key, Alt, Shift, item); Ctrl is always held.
    [
        (b'N', false, false, c(CommandKey::New)),
        (b'O', false, false, MenuItem::Open),
        (b'I', false, true, host(HostItem::Import)),
        (b'S', false, false, c(CommandKey::Save)),
        (b'S', false, true, c(CommandKey::SaveAs)),
        (b'W', false, false, c(CommandKey::Close)),
        (b'Z', false, false, c(CommandKey::Undo)),
        (b'Y', false, false, c(CommandKey::Redo)),
        (b'Z', false, true, c(CommandKey::Redo)),
        (b'X', false, false, c(CommandKey::Cut)),
        (b'C', false, false, c(CommandKey::Copy)),
        (b'V', false, false, c(CommandKey::Paste)),
        (b'V', false, true, c(CommandKey::PasteInPlace)),
        (b'D', false, false, c(CommandKey::Duplicate)),
        (b'A', false, false, c(CommandKey::SelectAll)),
        // Ctrl+Alt+I as on the Mac (Cmd+Option+I). Ctrl+Alt is AltGr on many layouts: only AltGr+I,
        // which no common layout maps to a character, shares the chord.
        (b'I', true, false, c(CommandKey::InvertSelection)),
        (b'G', false, false, c(CommandKey::Group)),
        (b'G', false, true, c(CommandKey::Ungroup)),
        (OEM_6, false, false, c(CommandKey::PushForward)),
        (OEM_4, false, false, c(CommandKey::PushBackward)),
        (OEM_6, false, true, c(CommandKey::PushToFront)),
        (OEM_4, false, true, c(CommandKey::PushToBack)),
        (b'0', false, false, c(CommandKey::ZoomPage)),
        (b'0', false, true, c(CommandKey::ZoomSelection)),
        (OEM_PLUS, false, false, c(CommandKey::ZoomIn)),
        (OEM_MINUS, false, false, c(CommandKey::ZoomOut)),
        (b'B', false, false, c(CommandKey::Compile)),
        (b'E', false, true, c(CommandKey::ExportRd)),
        (b'L', false, true, host(HostItem::LaserWindow)),
    ]
    .into_iter()
    .map(|(key, alt, shift, item)| Accelerator { key: u16::from(key), ctrl: true, alt, shift, id: command_id(item) })
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
    Some(format!("Ctrl+{}{}{key}", if a.alt { "Alt+" } else { "" }, if a.shift { "Shift+" } else { "" }))
}

/// Menu enablement from the view model (`EnableMenuItem` on `WM_INITMENUPOPUP`).
pub fn menu_enabled(item: MenuItem, vm: &AppViewModel) -> bool {
    match item {
        MenuItem::Open => true,
        MenuItem::Host(item) => item.enabled(&MenuContext {
            vm,
            locale: Locale::En,
            manual_known: true,
            recent_projects: &[],
            recent_templates: &[],
        }),
        // The preparation panel has something to show while a tracer is open in the window in front.
        MenuItem::TracePrepPanel => vm.active.and_then(|d| vm.document(d)).is_some_and(|d| d.trace_prep.is_some()),
        MenuItem::OperatorPanel => vm.active.and_then(|d| vm.document(d)).is_some_and(|d| d.operator.is_some()),
        // A dialog or panel belongs to a window; its own greying says why in words.
        MenuItem::CamDialog(_)
        | MenuItem::SchikkenPanel
        | MenuItem::OffsetPanel
        | MenuItem::StructurePanel
        | MenuItem::RemnantsPanel => vm.active.is_some(),
        MenuItem::Command(key) => vm.commands.get(&key).is_some_and(|c| c.enabled),
    }
}

/// Check mark state (active tool, snapping toggles, wireframe).
pub fn menu_checked(item: MenuItem, vm: &AppViewModel) -> bool {
    match item {
        MenuItem::Command(key) => vm.commands.get(&key).is_some_and(|c| c.checked),
        MenuItem::Open
        | MenuItem::CamDialog(_)
        | MenuItem::SchikkenPanel
        | MenuItem::OffsetPanel
        | MenuItem::StructurePanel
        | MenuItem::TracePrepPanel
        | MenuItem::OperatorPanel
        | MenuItem::RemnantsPanel
        | MenuItem::Host(_) => false,
    }
}

/// Menu title in `locale`, including the undo/redo action name from the view model.
pub fn menu_title(item: MenuItem, vm: &AppViewModel, locale: Locale) -> String {
    match item {
        MenuItem::Open => host_text(HostText::Open, locale).to_owned(),
        MenuItem::Host(item) => {
            item.title(&MenuContext { vm, locale, manual_known: true, recent_projects: &[], recent_templates: &[] })
        }
        MenuItem::CamDialog(dialog) => crate::panels::cam_menu_title(dialog, locale),
        MenuItem::SchikkenPanel => crate::panels::schikken_text(crate::panels::SchikkenText::MenuPanel, locale),
        MenuItem::OffsetPanel => crate::l10n_g5::g5_text(crate::l10n_g5::G5Text::OffsetMenu, locale).to_owned(),
        MenuItem::StructurePanel => crate::l10n_g5::g5_text(crate::l10n_g5::G5Text::StructureMenu, locale).to_owned(),
        MenuItem::TracePrepPanel => {
            crate::l10n_g6_werk::g6_text(crate::l10n_g6_werk::G6Text::TracePrepMenu, locale).to_owned()
        }
        MenuItem::OperatorPanel => {
            crate::l10n_g6_werk::g6_text(crate::l10n_g6_werk::G6Text::OperatorMenu, locale).to_owned()
        }
        MenuItem::RemnantsPanel => {
            crate::l10n_g7_werk::g7_text(crate::l10n_g7_werk::G7Text::RemnantsMenu, locale).to_owned()
        }
        MenuItem::Command(key) => {
            let label = vm.commands.get(&key).map_or(MessageKey::Command(key), |c| c.label);
            message_or_key(label, locale)
        }
    }
}
