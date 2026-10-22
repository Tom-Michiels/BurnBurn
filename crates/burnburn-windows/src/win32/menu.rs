//! The menu bar as Win32 menu handles. The structure comes from the headless tables in
//! [`crate::menus`], the titles, key hints, enablement and check marks from the adapter; this file
//! only creates the popups and items and keeps them in step with the view model.

use burnburn_presentation::AppViewModel;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CheckMenuItem, CreateMenu, CreatePopupMenu, EnableMenuItem, HMENU, MENU_ITEM_FLAGS, MF_BYCOMMAND,
    MF_CHECKED, MF_ENABLED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, MF_UNCHECKED, ModifyMenuW,
};
use windows::core::HSTRING;

use crate::adapter::{MenuItem, accelerator_label, command_id, menu_checked, menu_enabled, menu_items, menu_title};
use crate::l10n::Locale;
use crate::menus::{MenuNode, Submenu, menu_bar};

pub(super) fn new_menu() -> Option<HMENU> {
    // SAFETY: creates a menu this thread owns (handed to a window or destroyed with it).
    unsafe { CreateMenu() }.ok()
}

fn new_popup() -> Option<HMENU> {
    // SAFETY: creates a popup menu this thread owns (appended to its parent, destroyed with it).
    unsafe { CreatePopupMenu() }.ok()
}

pub(super) fn append(menu: HMENU, flags: MENU_ITEM_FLAGS, id: usize, title: &str) {
    let text = HSTRING::from(title);
    // SAFETY: our menu; for MF_POPUP `id` is a menu handle we created; the string outlives the call.
    let _ = unsafe { AppendMenuW(menu, flags, id, &text) };
}

/// The caption of an item: its title and, after a tab, its accelerator.
fn caption(item: MenuItem, vm: &AppViewModel, locale: Locale) -> String {
    let title = menu_title(item, vm, locale);
    match accelerator_label(item) {
        Some(keys) => format!("{title}\t{keys}"),
        None => title,
    }
}

fn fill(menu: HMENU, nodes: &[MenuNode], vm: &AppViewModel, locale: Locale) -> Option<()> {
    for node in nodes {
        match node {
            MenuNode::Separator => append(menu, MF_SEPARATOR, 0, ""),
            MenuNode::Item(item) => {
                append(menu, MF_STRING, usize::from(command_id(*item)), &caption(*item, vm, locale))
            }
            MenuNode::Submenu(sub) => {
                let popup = new_popup()?;
                fill(popup, &sub.items, vm, locale)?;
                append(menu, MF_POPUP, popup.0 as usize, &sub.title);
            }
        }
    }
    Some(())
}

/// The menu bar of a document window, in `locale`.
pub(super) fn build_menu(vm: &AppViewModel, locale: Locale) -> Option<HMENU> {
    let bar = new_menu()?;
    for Submenu { title, items, .. } in menu_bar(locale) {
        let popup = new_popup()?;
        fill(popup, &items, vm, locale)?;
        append(bar, MF_POPUP, popup.0 as usize, &title);
    }
    // W4-L8b: the panel families' popups (ids 6000..=7999).
    crate::win32_panels::manager::append_menus(bar);
    Some(bar)
}

/// `WM_INITMENUPOPUP`: titles (the undo name), enablement and check marks from the view model.
pub(super) fn refresh_popup(popup: HMENU, vm: &AppViewModel, locale: Locale) {
    for item in menu_items() {
        let id = u32::from(command_id(item));
        let enabled = if menu_enabled(item, vm) { MF_ENABLED } else { MF_GRAYED };
        let checked = if menu_checked(item, vm) { MF_CHECKED } else { MF_UNCHECKED };
        let text = HSTRING::from(caption(item, vm, locale));
        // SAFETY: the popup handle comes with the message; ids are ours; the string outlives the
        // call. Items that are not in this popup are ignored by Windows.
        let _ = unsafe { ModifyMenuW(popup, id, MF_BYCOMMAND | MF_STRING, usize::from(command_id(item)), &text) };
        // SAFETY: as above.
        let _ = unsafe { EnableMenuItem(popup, id, MF_BYCOMMAND | enabled) };
        // SAFETY: as above.
        let _ = unsafe { CheckMenuItem(popup, id, (MF_BYCOMMAND | checked).0) };
    }
}
