//! The menu bar as Win32 menu handles. The structure comes from the headless tables in
//! [`crate::menus`], the titles, key hints, enablement and check marks from the adapter, the
//! mnemonics from [`crate::menus::mnemonics`]; this file only creates the popups and items and
//! keeps them in step with the view model.

use burnburn_presentation::AppViewModel;
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CheckMenuItem, CreateMenu, CreatePopupMenu, EnableMenuItem, GetMenuStringW, HMENU, MENU_ITEM_FLAGS,
    MF_BYCOMMAND, MF_CHECKED, MF_ENABLED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, MF_UNCHECKED, ModifyMenuW,
};
use windows::core::HSTRING;

use crate::adapter::{MenuItem, accelerator_label, command_id, menu_checked, menu_enabled, menu_items, menu_title};
use crate::l10n::Locale;
use crate::menus::mnemonics::{assign, reapply};
use crate::menus::{MenuNode, Submenu, menu_bar};

/// A built menu bar and the two recent-list popups inside it (filled by the panel manager).
pub(super) struct BuiltMenu {
    pub bar: HMENU,
    pub recent_projects: Option<HMENU>,
    pub recent_templates: Option<HMENU>,
}

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

/// The caption of an item: its (mnemonic) title and, after a tab, its accelerator.
fn caption(item: MenuItem, title: &str) -> String {
    match accelerator_label(item) {
        Some(keys) => format!("{title}\t{keys}"),
        None => title.to_owned(),
    }
}

/// The title of a node as the level shows it before mnemonics.
fn node_title(node: &MenuNode, vm: &AppViewModel, locale: Locale) -> Option<String> {
    match node {
        MenuNode::Item(item) => Some(menu_title(*item, vm, locale)),
        MenuNode::Submenu(sub) => Some(sub.title.clone()),
        MenuNode::Separator => None,
    }
}

fn fill(menu: HMENU, nodes: &[MenuNode], vm: &AppViewModel, locale: Locale, built: &mut BuiltMenu) -> Option<()> {
    let titles: Vec<String> = nodes.iter().filter_map(|n| node_title(n, vm, locale)).collect();
    let mut captions = assign(&titles).into_iter();
    for node in nodes {
        match node {
            MenuNode::Separator => append(menu, MF_SEPARATOR, 0, ""),
            MenuNode::Item(item) => {
                let title = captions.next().unwrap_or_default();
                append(menu, MF_STRING, usize::from(command_id(*item)), &caption(*item, &title));
            }
            MenuNode::Submenu(sub) => {
                let title = captions.next().unwrap_or_default();
                let popup = new_popup()?;
                fill(popup, &sub.items, vm, locale, built)?;
                match sub.id {
                    "recent_projects" => built.recent_projects = Some(popup),
                    "recent_templates" => built.recent_templates = Some(popup),
                    _ => {}
                }
                append(menu, MF_POPUP, popup.0 as usize, &title);
            }
        }
    }
    Some(())
}

/// The menu bar of a document window, in `locale`.
pub(super) fn build_menu(vm: &AppViewModel, locale: Locale) -> Option<BuiltMenu> {
    let bar = new_menu()?;
    let mut built = BuiltMenu { bar, recent_projects: None, recent_templates: None };
    let popups = menu_bar(locale);
    let titles: Vec<String> = popups.iter().map(|p| p.title.clone()).collect();
    for (Submenu { items, .. }, title) in popups.into_iter().zip(assign(&titles)) {
        let popup = new_popup()?;
        fill(popup, &items, vm, locale, &mut built)?;
        append(bar, MF_POPUP, popup.0 as usize, &title);
    }
    Some(built)
}

/// The caption an item has now, if it is in `popup`.
fn current_caption(popup: HMENU, id: u32) -> Option<String> {
    let mut buffer = [0u16; 256];
    // SAFETY: a writable buffer of the stated length; the id is looked up in our own popup.
    let len = unsafe { GetMenuStringW(popup, id, Some(&mut buffer), MF_BYCOMMAND) };
    let len = usize::try_from(len).ok().filter(|l| *l > 0)?;
    Some(String::from_utf16_lossy(&buffer[..len.min(buffer.len())]))
}

/// `WM_INITMENUPOPUP`: titles (the undo name), enablement and check marks from the view model.
/// The panel families' items are refreshed by the panel manager.
pub(super) fn refresh_popup(popup: HMENU, vm: &AppViewModel, locale: Locale) {
    for item in menu_items().into_iter().filter(|i| !matches!(i, MenuItem::Host(_))) {
        let id = u32::from(command_id(item));
        let Some(old) = current_caption(popup, id) else { continue };
        let enabled = if menu_enabled(item, vm) { MF_ENABLED } else { MF_GRAYED };
        let checked = if menu_checked(item, vm) { MF_CHECKED } else { MF_UNCHECKED };
        // The caption keeps its underlined letter and its accelerator text.
        let previous_title = old.split('\t').next().unwrap_or_default();
        let text = HSTRING::from(caption(item, &reapply(previous_title, &menu_title(item, vm, locale))));
        // SAFETY: the popup handle comes with the message; ids are ours; the string outlives the call.
        let _ = unsafe { ModifyMenuW(popup, id, MF_BYCOMMAND | MF_STRING, usize::from(command_id(item)), &text) };
        // SAFETY: as above.
        let _ = unsafe { EnableMenuItem(popup, id, MF_BYCOMMAND | enabled) };
        // SAFETY: as above.
        let _ = unsafe { CheckMenuItem(popup, id, (MF_BYCOMMAND | checked).0) };
    }
}
