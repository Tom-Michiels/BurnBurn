//! The menu bar as Win32 menu handles. The structure comes from the headless tables in
//! [`crate::menus`]; this file only creates the popups and items.

use windows::Win32::UI::WindowsAndMessaging::{AppendMenuW, CreateMenu, HMENU, MF_POPUP, MF_SEPARATOR, MF_STRING};
use windows::core::HSTRING;

use crate::adapter::{MenuItem, command_id, menu_items};
use burnburn_presentation::CommandKey;

pub(super) fn new_menu() -> Option<HMENU> {
    // SAFETY: creates a menu this thread owns (handed to a window or destroyed with it).
    unsafe { CreateMenu() }.ok()
}

pub(super) fn append(
    menu: HMENU,
    flags: windows::Win32::UI::WindowsAndMessaging::MENU_ITEM_FLAGS,
    id: usize,
    title: &str,
) {
    let text = HSTRING::from(title);
    // SAFETY: our menu; for MF_POPUP `id` is a menu handle we created; the string outlives the call.
    let _ = unsafe { AppendMenuW(menu, flags, id, &text) };
}

pub(super) fn build_menu() -> Option<HMENU> {
    {
        let bar = new_menu()?;
        let add =
            |menu: HMENU, item: MenuItem, title: &str| append(menu, MF_STRING, usize::from(command_id(item)), title);
        let c = MenuItem::Command;
        let file = new_menu()?;
        add(file, c(CommandKey::New), "&Nieuw\tCtrl+N");
        add(file, MenuItem::Open, "&Open…\tCtrl+O");
        append(file, MF_SEPARATOR, 0, "");
        add(file, c(CommandKey::Save), "&Bewaar\tCtrl+S");
        add(file, c(CommandKey::SaveAs), "Bewaar &als…\tCtrl+Shift+S");
        add(file, c(CommandKey::Close), "&Sluit\tCtrl+W");
        let edit = new_menu()?;
        add(edit, c(CommandKey::Undo), "&Herstel\tCtrl+Z");
        add(edit, c(CommandKey::Redo), "&Opnieuw\tCtrl+Y");
        add(edit, c(CommandKey::Delete), "&Verwijder\tDel");
        add(edit, c(CommandKey::SelectAll), "Selecteer &alles");
        let tools = new_menu()?;
        for item in menu_items() {
            if let MenuItem::Command(
                k @ (CommandKey::ToolSelect
                | CommandKey::ToolRectangle
                | CommandKey::ToolEllipse
                | CommandKey::ToolLine),
            ) = item
            {
                add(tools, item, &format!("{k:?}"));
            }
        }
        append(bar, MF_POPUP, file.0 as usize, "&Archief");
        append(bar, MF_POPUP, edit.0 as usize, "&Wijzig");
        append(bar, MF_POPUP, tools.0 as usize, "&Gereedschap");
        Some(bar)
    }
}
