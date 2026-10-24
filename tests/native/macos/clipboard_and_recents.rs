//! W2-L1 (bewerken), native: Cut / Copy / Paste through the responder chain onto an NSPasteboard
//! under BurnBurn's own type, and Archief ▸ Recente projecten (menu entries, stored file, Wis menu).
//!
//! Safety: the app uses a private, uniquely named pasteboard and no recent-projects file other than
//! a fresh temp folder; the test checks that the user's general pasteboard was not touched.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

// Which items open a dialog and which only show a window (the ellipsis rule), shared with the
// Windows menu test.
#[cfg(target_os = "macos")]
include!("../../../crates/burnburn-windows/tests/common/menu_rules.rs");

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("clipboard_and_recents: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    let a = common::report("pasteboard_roundtrip_on_a_private_pasteboard", pasteboard_roundtrip());
    if a != ExitCode::SUCCESS {
        return a;
    }
    let b = common::report("edit_menu_acts_through_the_responder_chain_on_the_private_pasteboard", edit_chain());
    if b != ExitCode::SUCCESS {
        return b;
    }
    let c = common::report("recent_projects_menu_follows_the_stored_list", recents());
    if c != ExitCode::SUCCESS {
        return c;
    }
    let d = common::report("arrange_and_view_menus_act_on_the_selection_and_the_view", arrange_and_view());
    if d != ExitCode::SUCCESS {
        return d;
    }
    let e = common::report("menu_structure_follows_the_mac_guidelines", menu_structure());
    if e != ExitCode::SUCCESS {
        return e;
    }
    common::report("saved_shortcuts_follow_their_items_to_the_new_menus", saved_shortcuts_follow_moved_items())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn pasteboard_roundtrip() -> Result<(), String> {
    use burnburn_macos::appkit::clipboard::{
        has_clipboard, private_pasteboard, read_clipboard, read_clipboard_limited, write_clipboard,
    };
    use common::expect;
    use objc2_app_kit::NSPasteboardTypeString;

    let (a, b) = (private_pasteboard(), private_pasteboard());
    expect(|| "a fresh pasteboard holds nothing of ours".into(), !has_clipboard(&a) && read_clipboard(&a).is_none())?;
    let payload = vec![0u8, 1, 2, 255, 128, 0];
    write_clipboard(&a, &payload)?;
    expect(|| "has our type after a write".into(), has_clipboard(&a))?;
    expect(|| "the bytes survive, zeros included".into(), read_clipboard(&a).as_deref() == Some(payload.as_slice()))?;
    expect(|| "another pasteboard is independent".into(), !has_clipboard(&b))?;

    write_clipboard(&a, &[9])?;
    expect(|| "a second write replaces the first".into(), read_clipboard(&a) == Some(vec![9]))?;

    write_clipboard(&a, &[7u8; 16])?;
    expect(
        || "a payload over the limit comes back empty, not truncated".into(),
        read_clipboard_limited(&a, 8) == Some(Vec::new()),
    )?;
    expect(|| "at the limit it is read".into(), read_clipboard_limited(&a, 16).map(|v| v.len()) == Some(16))?;

    // Text from another app is not ours.
    a.clearContents();
    // SAFETY: AppKit's constant pasteboard type name.
    let string_type = unsafe { NSPasteboardTypeString };
    a.setString_forType(&objc2_foundation::NSString::from_str("hello"), string_type);
    expect(|| "plain text is not a BurnBurn clipboard".into(), !has_clipboard(&a) && read_clipboard(&a).is_none())?;
    Ok(())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn edit_chain() -> Result<(), String> {
    use burnburn_macos::appkit::clipboard::{general_pasteboard, has_clipboard, read_clipboard, write_clipboard};
    use burnburn_presentation::MessageKey;
    use common::expect;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{Message, msg_send, sel};
    use objc2_app_kit::{NSMenuItem, NSView};

    let general_before = general_pasteboard().changeCount();
    let s = common::start()?;
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (0.0, 0.0), (120.0, 80.0))?;
    let count = |s: &common::Session| s.doc(doc).map_or(0, |d| d.objects.len());
    expect(|| "one rectangle".into(), count(&s) == 1)?;
    let canvas: Retained<NSView> = s.app.with_window(doc, |w| w.canvas_view.retain()).ok_or("no canvas")?;
    let window = s.app.window(doc).ok_or("no window")?;
    window.makeFirstResponder(Some(&canvas));

    // The Edit items have no target of their own: they reach the first responder.
    let item = |action| -> Result<Retained<NSMenuItem>, String> {
        let menu = s.app.menu();
        for i in 0..menu.numberOfItems() {
            let Some(sub) = menu.itemAtIndex(i).and_then(|i| i.submenu()) else { continue };
            // SAFETY: a plain search of the menu's items by action selector (no target).
            let index = unsafe { sub.indexOfItemWithTarget_andAction(None, Some(action)) };
            if index >= 0 {
                return sub.itemAtIndex(index).ok_or_else(|| "item vanished".to_owned());
            }
        }
        Err(format!("no Edit item for {action:?}"))
    };
    let valid = |action| -> Result<bool, String> {
        let it = item(action)?;
        // SAFETY: `validateMenuItem:` takes the item and returns BOOL, as NSMenu calls it.
        Ok(unsafe { msg_send![&*canvas, validateMenuItem: &*it] })
    };
    let perform = |action| -> bool {
        // SAFETY: `tryToPerform:with:` sends the action to the responder if it responds.
        unsafe { msg_send![&*canvas, tryToPerform: action, with: None::<&AnyObject>] }
    };
    expect(|| "Copy is enabled with a selection".into(), valid(sel!(copy:))? && valid(sel!(cut:))?)?;
    expect(|| "Paste is enabled".into(), valid(sel!(paste:))? && valid(sel!(pasteInPlace:))?)?;

    // Copy: the bytes land on the (private) pasteboard, the document is unchanged.
    expect(|| "copy: reaches the canvas".into(), perform(sel!(copy:)))?;
    let bytes = read_clipboard(s.app.pasteboard()).ok_or("Copy wrote nothing on the pasteboard")?;
    expect(|| "non-empty payload".into(), !bytes.is_empty())?;
    expect(|| "Copy does not change the document".into(), count(&s) == 1)?;

    // Paste reads the pasteboard: one more object, one undo step.
    expect(|| "paste: reaches the canvas".into(), perform(sel!(paste:)))?;
    s.pump(0.1);
    expect(|| format!("two objects after Paste, got {}", count(&s)), count(&s) == 2)?;
    expect(|| "paste in place".into(), perform(sel!(pasteInPlace:)))?;
    s.pump(0.1);
    expect(|| format!("three objects after Paste in Place, got {}", count(&s)), count(&s) == 3)?;

    // Another process may have put something else under our type: reported, nothing applied.
    write_clipboard(s.app.pasteboard(), b"not a burnburn clipboard")?;
    let before = s.doc(doc).ok_or("no document")?.revision;
    expect(|| "paste with damaged data".into(), perform(sel!(paste:)))?;
    s.pump(0.1);
    let after = s.doc(doc).ok_or("no document")?;
    expect(|| "a damaged payload changes nothing".into(), after.revision == before && count(&s) == 3)?;
    let status = s.app.with_window(doc, |w| w.status_label.stringValue().to_string()).unwrap_or_default();
    expect(
        || format!("the unreadable clipboard is reported in the window, status {status:?}"),
        // The status line leads with the symbol of its tone (an error here), then the message.
        status
            == format!(
                "\u{2716} {}",
                burnburn_macos::l10n::message(MessageKey::ClipboardUnreadable, burnburn_macos::l10n::Locale::Nl)
                    .unwrap_or_default()
            ),
    )?;

    // Cut removes the selection and fills the pasteboard again.
    expect(|| "cut:".into(), perform(sel!(cut:)))?;
    s.pump(0.1);
    expect(|| format!("two objects after Cut, got {}", count(&s)), count(&s) == 2)?;
    expect(|| "Cut refilled the pasteboard".into(), read_clipboard(s.app.pasteboard()).is_some_and(|b| b.len() > 24))?;

    // Without our type on the pasteboard, Paste uses the last copy the controller holds.
    s.app.pasteboard().clearContents();
    expect(|| "pasteboard is empty".into(), !has_clipboard(s.app.pasteboard()))?;
    expect(|| "paste from the controller's copy".into(), perform(sel!(paste:)))?;
    s.pump(0.1);
    expect(|| format!("three objects again, got {}", count(&s)), count(&s) == 3)?;

    expect(
        || "the user's general pasteboard was never touched".into(),
        general_pasteboard().changeCount() == general_before,
    )?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn recents() -> Result<(), String> {
    use burnburn_macos::adapter::{MenuItem, menu_tag};
    use burnburn_macos::recent::{read_recent_projects, write_recent_projects};
    use common::expect;
    use std::path::PathBuf;

    let dir = std::env::temp_dir().join(format!("burnburn-recents-native-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let file = dir.join("BurnBurn").join("recent-projects.json");
    let stored: Vec<PathBuf> = vec![dir.join("gone-a.burnburn"), dir.join("gone-b.burnburn")];
    write_recent_projects(&file, &stored).map_err(|e| e.to_string())?;

    let f = file.clone();
    let s = common::start_with(move |o| o.recent_projects_file = Some(f))?;

    // The list read at start is in the menu, most recent first, with Wis menu below.
    let titles = |s: &common::Session| -> Vec<(String, bool)> {
        let menu = s.app.recent_menu();
        (0..menu.numberOfItems())
            .filter_map(|i| menu.itemAtIndex(i))
            .filter(|i| !i.isSeparatorItem())
            .map(|i| (i.title().to_string(), i.isEnabled()))
            .collect()
    };
    let (_, _) = (menu_tag(MenuItem::ClearRecent), 0);
    s.pump(0.1);
    s.app.recent_menu().update();
    let shown = titles(&s);
    expect(
        || format!("menu shows both files and Wis menu, got {shown:?}"),
        shown == [("gone-a.burnburn".into(), true), ("gone-b.burnburn".into(), true), ("Wis menu".into(), true)],
    )?;

    // Choosing an entry whose file is gone forgets it: menu and stored file follow.
    let doc_before = s.app.open_documents().len();
    s.menu(MenuItem::Recent(0))?;
    s.wait(3.0, "the missing project to be forgotten", || titles(&s).len() == 2)?;
    s.app.recent_menu().update();
    let shown = titles(&s);
    expect(|| format!("one file left, got {shown:?}"), shown[0].0 == "gone-b.burnburn")?;
    let on_disk = read_recent_projects(&file);
    expect(|| format!("stored list follows: {on_disk:?}"), on_disk == [dir.join("gone-b.burnburn")])?;
    expect(|| "no window opened for a missing file".into(), s.app.open_documents().len() == doc_before)?;

    // Wis menu empties both and disables itself.
    s.menu(MenuItem::ClearRecent)?;
    s.pump(0.1);
    s.app.recent_menu().update();
    let shown = titles(&s);
    expect(|| format!("only a disabled Wis menu, got {shown:?}"), shown == [("Wis menu".to_owned(), false)])?;
    expect(|| "stored list is empty".into(), read_recent_projects(&file).is_empty())?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// The menu bar of this round: headings, no shortcut used twice, Arrange acting on the selection
/// through the real menu, and the checked Weergave ▸ Draadmodel redrawing the canvas.
#[cfg(target_os = "macos")]
fn arrange_and_view() -> Result<(), String> {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_presentation::CommandKey;
    use common::expect;
    use std::collections::HashSet;

    let s = common::start()?;
    let headings: Vec<String> = (0..s.app.menu().numberOfItems())
        .filter_map(|i| s.app.menu().itemAtIndex(i))
        .map(|i| i.title().to_string())
        .collect();
    expect(
        || format!("menu bar {headings:?}"),
        headings
            == [
                "BurnBurn",
                "Archief",
                "Wijzig",
                "Weergave",
                "Gereedschap",
                "Schikken",
                "Tekst",
                "Laser",
                "Venster",
                "Help",
            ]
            .map(String::from),
    )?;

    // Every key equivalent is used once (a duplicate would silently shadow an item).
    let mut seen = HashSet::new();
    let mut stack = vec![s.app.menu().itemArray()];
    while let Some(items) = stack.pop() {
        for item in items.iter() {
            if let Some(sub) = item.submenu() {
                stack.push(sub.itemArray());
            }
            let key = item.keyEquivalent().to_string();
            if key.is_empty() {
                continue;
            }
            let combo = (key, item.keyEquivalentModifierMask().0);
            let described = format!("shortcut {combo:?} used twice (second: {:?})", item.title());
            expect(|| described, seen.insert(combo))?;
        }
    }

    let doc = s.new_document()?;
    s.draw_rectangle(doc, (0.0, 0.0), (40.0, 30.0))?;
    s.draw_rectangle(doc, (60.0, 0.0), (100.0, 30.0))?;
    s.menu(MenuItem::Command(CommandKey::ToolSelect))?;
    let enabled = |key| s.menu_item_state(MenuItem::Command(key)).map(|(e, _)| e);
    expect(|| "Group needs two selected objects".into(), enabled(CommandKey::Group) == Some(false))?;
    s.app.dispatch(burnburn_presentation::UiIntent::Invoke { document: Some(doc), command: CommandKey::SelectAll });
    s.pump(0.1);
    expect(
        || "with two selected: Group, Align and Distribute-capable commands are on".into(),
        enabled(CommandKey::Group) == Some(true) && enabled(CommandKey::AlignLeft) == Some(true),
    )?;
    let title = s.menu_item_state(MenuItem::Command(CommandKey::Group)).map(|(_, t)| t);
    expect(|| format!("Group title {title:?}"), title.as_deref() == Some("Groepeer"))?;
    s.menu(MenuItem::Command(CommandKey::Group))?;
    s.pump(0.1);
    let grouped = s.doc(doc).ok_or("no document")?.objects.iter().all(|o| !o.groups.is_empty());
    expect(|| "the menu grouped both rectangles".into(), grouped)?;
    expect(|| "Ungroup is now on".into(), enabled(CommandKey::Ungroup) == Some(true))?;

    // Weergave ▸ Draadmodel: a check mark, a redraw, no document change.
    let wire = MenuItem::Command(CommandKey::ViewWireframe);
    expect(|| "wireframe starts off".into(), s.menu_item_checked(wire) == Some(false))?;
    let (frames, revision) =
        (s.app.with_window(doc, |w| w.canvas_frames).unwrap_or(0), s.doc(doc).ok_or("no document")?.revision);
    s.menu(wire)?;
    s.pump(0.2);
    expect(|| "wireframe is checked".into(), s.menu_item_checked(wire) == Some(true))?;
    expect(|| "view model says wireframe".into(), s.doc(doc).is_some_and(|d| d.wireframe))?;
    // Metal presents nothing while the window is occluded (locked screen): then only report it.
    let on_screen = s.wait_on_screen(doc)?;
    if on_screen {
        let after = s.app.with_window(doc, |w| w.canvas_frames).unwrap_or(0);
        expect(|| format!("the canvas drew again ({frames} to {after} frames)"), after > frames)?;
    }
    expect(|| "a view setting is no document change".into(), s.doc(doc).is_some_and(|d| d.revision == revision))?;
    s.menu(wire)?;
    expect(|| "wireframe off again".into(), s.menu_item_checked(wire) == Some(false))?;
    Ok(())
}

/// The rule token of a menu item (the scheme of `menu_rules.rs`).
#[cfg(target_os = "macos")]
fn rule_token(item: burnburn_macos::adapter::MenuItem) -> String {
    use burnburn_macos::adapter::MenuItem as M;
    match item {
        M::Command(k) => format!("Cmd:{k:?}"),
        M::Open => "Open".into(),
        M::CamDialog(d) => format!("Cam:{d:?}"),
        M::SchikkenPanel => "Schikken".into(),
        M::App(a) => format!("App:{a:?}"),
        other => format!("Other:{other:?}"),
    }
}

/// The menus of LF16: ten menus in the order of the Mac guidelines, the system's Help, Window and
/// Services menus registered, the toolbar items, and the rules that hold for the whole bar.
#[cfg(target_os = "macos")]
fn menu_structure() -> Result<(), String> {
    use burnburn_macos::adapter::menu_item;
    use common::expect;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSApplication, NSMenu};
    use std::collections::HashSet;

    let s = common::start()?;
    let bar = s.app.menu();
    let top = |menu: &NSMenu, title: &str| -> Option<Retained<NSMenu>> {
        (0..menu.numberOfItems())
            .filter_map(|i| menu.itemAtIndex(i))
            .find(|i| i.title().to_string() == title)?
            .submenu()
    };

    // The system knows which menus are the Help, Window and Services menus.
    let app = NSApplication::sharedApplication(s.mtm);
    let title_of = |m: Option<Retained<NSMenu>>| m.map(|m| m.title().to_string());
    expect(|| "Help is registered with setHelpMenu".into(), title_of(app.helpMenu()).as_deref() == Some("Help"))?;
    expect(|| "Window is registered".into(), title_of(app.windowsMenu()).as_deref() == Some("Venster"))?;
    expect(|| "Services is registered".into(), title_of(app.servicesMenu()).as_deref() == Some("Diensten"))?;

    // View has the system's toolbar switches, answered through the responder chain.
    let view = top(bar, "Weergave").ok_or("no Weergave menu")?;
    let action_of = |title: &str| -> Result<Option<objc2::runtime::Sel>, String> {
        let item = (0..view.numberOfItems())
            .filter_map(|i| view.itemAtIndex(i))
            .find(|i| i.title().to_string() == title)
            .ok_or_else(|| format!("Weergave has no {title:?}"))?;
        Ok(item.action())
    };
    expect(
        || "Toon werkbalk toggles the toolbar".into(),
        action_of("Toon werkbalk")? == Some(objc2::sel!(toggleToolbarShown:)),
    )?;
    expect(
        || "Pas werkbalk aan… opens the palette".into(),
        action_of("Pas werkbalk aan…")? == Some(objc2::sel!(runToolbarCustomizationPalette:)),
    )?;

    // Walk every menu of the bar below the application menu.
    let mut tags = HashSet::new();
    let mut converted = 0;
    let mut stack: Vec<(String, Retained<NSMenu>)> = Vec::new();
    for i in 1..bar.numberOfItems() {
        let item = bar.itemAtIndex(i).ok_or("menu item")?;
        stack.push((item.title().to_string(), item.submenu().ok_or("a top menu without items")?));
    }
    while let Some((path, menu)) = stack.pop() {
        let entries: Vec<_> = (0..menu.numberOfItems()).filter_map(|i| menu.itemAtIndex(i)).collect();
        let counted = entries.iter().filter(|e| !e.isSeparatorItem()).count();
        // The Window menu lists the open windows after the standard commands: AppKit adds those.
        expect(|| format!("{path} lists {counted} entries (at most 25)"), counted <= 25)?;
        for entry in entries {
            if let Some(sub) = entry.submenu() {
                stack.push((format!("{path} > {}", entry.title()), sub));
            }
            let Some(menu_item) = menu_item(entry.tag()) else { continue };
            expect(|| format!("{path}: {:?} appears twice", entry.title()), tags.insert(entry.tag()))?;
            // The dynamic recent entries carry file names; only fixed items follow the rule.
            let token = rule_token(menu_item);
            let title = entry.title().to_string();
            let ends = title.ends_with('\u{2026}');
            let wants = opens(&token) == Opens::Dialog;
            expect(
                || format!("{path}: {title:?} ({token}) {} an ellipsis", if wants { "needs" } else { "must not have" }),
                ends == wants
                    || matches!(
                        menu_item,
                        burnburn_macos::adapter::MenuItem::Recent(_)
                            | burnburn_macos::adapter::MenuItem::RecentTemplate(_)
                    ),
            )?;
            converted += 1;
        }
    }
    expect(|| format!("only {converted} items were checked"), converted > 150)?;

    // Nothing is left of the menus that were merged.
    let titles: Vec<String> =
        (0..bar.numberOfItems()).filter_map(|i| bar.itemAtIndex(i)).map(|i| i.title().to_string()).collect();
    for gone in ["Productie", "Vormen", "Schets", "Beeld"] {
        expect(|| format!("{gone} is no top-level menu any more: {titles:?}"), !titles.iter().any(|t| t == gone))?;
    }
    let laser = top(bar, "Laser").ok_or("no Laser menu")?;
    for (title, what) in [("Compileer plan", "Compile"), ("Bewerkingen", "the CAM settings submenu")] {
        let found =
            (0..laser.numberOfItems()).filter_map(|i| laser.itemAtIndex(i)).any(|i| i.title().to_string() == title);
        expect(|| format!("Laser has {what} ({title:?})"), found)?;
    }
    Ok(())
}

/// Shortcuts are stored by the name of the command, not by the menu position: a shortcut saved
/// for an item that LF16 moved (Compile and Export RD from Productie to Laser, Push Forward into
/// Volgorde) applies to the item in its new place after a restart.
#[cfg(target_os = "macos")]
fn saved_shortcuts_follow_moved_items() -> Result<(), String> {
    use burnburn_macos::adapter::{MenuItem, menu_item};
    use burnburn_presentation::{AppIntent, CommandKey, KeyBinding, UiIntent};
    use common::expect;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSEventModifierFlags, NSMenu};

    fn find(menu: &NSMenu, wanted: MenuItem) -> Option<Retained<objc2_app_kit::NSMenuItem>> {
        for i in 0..menu.numberOfItems() {
            let item = menu.itemAtIndex(i)?;
            if menu_item(item.tag()) == Some(wanted) {
                return Some(item);
            }
            if let Some(found) = item.submenu().and_then(|sub| find(&sub, wanted)) {
                return Some(found);
            }
        }
        None
    }
    let compile = MenuItem::Command(CommandKey::Compile);
    let forward = MenuItem::Command(CommandKey::PushForward);
    let cmd_opt = NSEventModifierFlags(NSEventModifierFlags::Command.0 | NSEventModifierFlags::Option.0);

    // First run: the user picks new keys; the settings file is written.
    let saved = {
        let s = common::start()?;
        let dir = s.dir.join("user");
        s.app.use_user_dir(dir.clone());
        for (id, key) in [("cmd.Compile", "Cmd+Opt+k"), ("cmd.PushForward", "Cmd+Opt+u")] {
            let binding = KeyBinding::parse(key).ok_or("binding")?;
            s.app.dispatch(UiIntent::App(AppIntent::SetShortcut { id: id.into(), binding, replace: true }));
        }
        let file = dir.join("preferences.conf");
        s.wait(3.0, "the settings file", || {
            std::fs::read_to_string(&file).is_ok_and(|t| t.contains("shortcut.cmd.Compile"))
        })?;
        std::fs::read_to_string(&file).map_err(|e| e.to_string())?
    };

    // Second run: a fresh app reads the file; the keys sit on the items in their new menus.
    let s = common::start()?;
    let dir = s.dir.join("user");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("preferences.conf"), saved).map_err(|e| e.to_string())?;
    s.app.use_user_dir(dir);
    for (item, key) in [(compile, "k"), (forward, "u")] {
        let found = find(s.app.menu(), item).ok_or_else(|| format!("{item:?} is not in the menu"))?;
        expect(
            || {
                format!(
                    "{item:?} has key {:?} / {:?}",
                    found.keyEquivalent().to_string(),
                    found.keyEquivalentModifierMask()
                )
            },
            found.keyEquivalent().to_string() == key && found.keyEquivalentModifierMask() == cmd_opt,
        )?;
    }
    let report = s.app.menu_key_report();
    expect(|| format!("the register covers every key of the moved menus: {report:?}"), report.unregistered.is_empty())?;
    Ok(())
}
