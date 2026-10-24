//! The Windows menu bar against the macOS menu (W4-L8a): the same commands in the same popups,
//! the same accelerators, the same intents. The macOS menu is read from its source text
//! (`appkit/app.rs::main_menu`), so no AppKit is linked and the oracle is independent of the
//! Windows tables.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_presentation::{CamDialog, CommandKey, HeadlessEditorHarness, UiIntent};
use burnburn_windows::adapter::{
    Control, MenuItem, MessageContext, Reaction, accelerators, command_id, menu_checked, menu_enabled, menu_item,
    menu_items, menu_title, msg, notify, on_message,
};
use burnburn_windows::host_panels::menu::Item;
use burnburn_windows::l10n::{HostText, Locale, host_text};
use burnburn_windows::menus::{MenuNode, Submenu, menu_bar};

const MAC_APP: &str = include_str!("../../burnburn-macos/src/appkit/app.rs");
/// The macOS Window and Help menus are built in the app host.
const MAC_APP_HOST: &str = include_str!("../../burnburn-macos/src/appkit/app/app_host.rs");

/// The text of `fn main_menu` in the macOS host.
fn mac_main_menu() -> &'static str {
    let start = MAC_APP.find("fn main_menu(").expect("main_menu exists");
    &MAC_APP[start..]
}

/// Menu items named in a piece of macOS menu source, in order: `Cmd:Name`, `Cam:Name`, `Open`,
/// `Schikken`, or `Other:Name` for the host-only items.
fn mac_tokens(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    let ident_at = |from: usize| -> String {
        src[from..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect()
    };
    while i < src.len() {
        if !src.is_char_boundary(i) {
            i += 1;
            continue;
        }
        let rest = &src[i..];
        if rest.starts_with("CommandKey::") {
            let name = ident_at(i + 12);
            // `CommandKey::ALL` is a list, not an item.
            if name != "ALL" {
                out.push(format!("Cmd:{name}"));
            }
            i += 12;
        } else if rest.starts_with("CamDialog::") {
            out.push(format!("Cam:{}", ident_at(i + 11)));
            i += 11;
        } else if rest.starts_with("AppAction::") {
            out.push(format!("App:{}", ident_at(i + 11)));
            i += 11;
        } else if rest.starts_with("recent_templates_holder,") {
            out.push("Dyn:recent_templates".into());
            i += 24;
        } else if rest.starts_with("recent_holder,") {
            out.push("Dyn:recent_projects".into());
            i += 14;
        } else if rest.starts_with("MenuItem::SchikkenPanel") {
            out.push("Schikken".into());
            i += 20;
        } else if rest.starts_with("MenuItem::Open") {
            out.push("Open".into());
            i += 14;
        } else if rest.starts_with("MenuItem::") {
            let name = ident_at(i + 10);
            if !matches!(name.as_str(), "Command" | "CamDialog" | "App" | "separatorItem") {
                out.push(format!("Other:{name}"));
            }
            i += 10;
        } else {
            i += 1;
        }
    }
    out
}

/// The top-level popups of the macOS menu: `(title source, items source)`.
fn mac_popups() -> Vec<(String, String)> {
    let body = mac_main_menu();
    let marker = "main.addItem(&submenu(";
    let mut parts: Vec<&str> = body.split(marker).skip(1).collect();
    // The last popups' source ends where the Window menu (built by the app host) starts.
    for part in parts.iter_mut() {
        if let Some(end) = part.find("main.addItem(&app_host::") {
            *part = &part[..end];
        }
    }
    let mut out: Vec<(String, String)> = parts
        .into_iter()
        .map(|p| {
            let mut lines = p.lines();
            let first = lines.next().unwrap_or("").trim().to_owned();
            let title = if first.is_empty() { lines.next().unwrap_or("").trim().to_owned() } else { first };
            (title, p.to_owned())
        })
        .collect();
    // Window: the library panels main_menu hands over, then the app host's own items.
    let window_args = &body[body.find("app_host::window_menu(").unwrap()..];
    let window_args = &window_args[..window_args.find("main.addItem(&app_host::help_menu").unwrap()];
    out.push(("AppText::MenuWindow".into(), format!("{window_args}\n{}", fn_body(MAC_APP_HOST, "fn window_menu("))));
    out.push(("AppText::MenuHelp".into(), fn_body(MAC_APP_HOST, "fn help_menu(")));
    out
}

/// The source of the function that starts with `signature`, up to its closing brace.
fn fn_body(src: &str, signature: &str) -> String {
    let start = src.find(signature).unwrap_or_else(|| panic!("{signature} exists"));
    let rest = &src[start..];
    let end = rest[signature.len()..].find("\n}\n").map_or(rest.len(), |e| e + signature.len());
    rest[..end].to_owned()
}

/// The macOS token of a menu item of the Windows menu.
fn token_of(item: &MenuItem) -> String {
    match item {
        MenuItem::Command(k) => format!("Cmd:{k:?}"),
        MenuItem::Open => "Open".into(),
        MenuItem::CamDialog(d) => format!("Cam:{d:?}"),
        MenuItem::SchikkenPanel => "Schikken".into(),
        MenuItem::OffsetPanel => "Other:OffsetPanel".into(),
        MenuItem::StructurePanel => "Other:StructurePanel".into(),
        MenuItem::TracePrepPanel => "Other:TracePrepPanel".into(),
        MenuItem::OperatorPanel => "Other:OperatorPanel".into(),
        MenuItem::RemnantsPanel => "Other:RemnantsPanel".into(),
        MenuItem::RecognitionPanel => "Other:RecognitionPanel".into(),
        MenuItem::Host(item) => host_token(*item),
    }
}

/// Every item of a popup with its macOS token, nested popups flattened; the dynamic recent lists
/// are one `Dyn:` entry without a menu item.
fn win_entries(menu: &Submenu) -> Vec<(String, Option<MenuItem>)> {
    fn walk(nodes: &[MenuNode], out: &mut Vec<(String, Option<MenuItem>)>) {
        for n in nodes {
            match n {
                MenuNode::Item(item) => out.push((token_of(item), Some(*item))),
                MenuNode::Separator => {}
                MenuNode::Submenu(s) if s.id.starts_with("recent_") => out.push((format!("Dyn:{}", s.id), None)),
                MenuNode::Submenu(s) => walk(&s.items, out),
            }
        }
    }
    let mut out = Vec::new();
    walk(&menu.items, &mut out);
    out
}

fn win_tokens(menu: &Submenu) -> Vec<String> {
    win_entries(menu).into_iter().map(|(token, _)| token).collect()
}

/// The macOS token of a panel-family item: commands are commands, host items keep their name.
fn host_token(item: Item) -> String {
    if let Some(command) = item.command() {
        return format!("Cmd:{command:?}");
    }
    match item {
        Item::App(action) => format!("App:{action:?}"),
        Item::Import | Item::ImportImage | Item::TraceImage | Item::LaserWindow | Item::ExportDiagnostics => {
            format!("Other:{item:?}")
        }
        other => format!("Win:{other:?}"),
    }
}

/// Entries only the Windows menu has: the family windows that the Mac opens from elsewhere and the
/// application-menu items (the Mac has an application menu; Windows puts them at the end of Help).
fn windows_only(token: &str) -> bool {
    token.starts_with("Win:") || matches!(token, "App:About" | "App:Preferences" | "App:Shortcuts")
}

/// Mac popup title source → Windows popup id, for the popups this lane owns.
const POPUPS: &[(&str, &str)] = &[
    ("HostText::MenuFile", "file"),
    ("HostText::MenuEdit", "edit"),
    ("HostText::MenuView", "view"),
    ("HostText::MenuTools", "tools"),
    ("HostText::MenuArrange", "arrange"),
    ("TextText::MenuText", "text"),
    ("LaserText::MenuLaser", "laser"),
    ("AppText::MenuWindow", "window"),
    ("AppText::MenuHelp", "help"),
];

fn popup<'a>(bar: &'a [Submenu], id: &str) -> &'a Submenu {
    bar.iter()
        .find(|m| m.id == id)
        .unwrap_or_else(|| panic!("menu {id} missing in {:?}", bar.iter().map(|m| m.id).collect::<Vec<_>>()))
}

#[test]
fn every_popup_has_the_macos_items_in_the_macos_order() {
    let bar = menu_bar(Locale::En);
    let popups = mac_popups();
    assert!(popups.len() >= 9, "the macOS menu source was parsed: {} popups", popups.len());
    for (token, id) in POPUPS {
        let (_, src) = popups.iter().find(|(t, _)| t.contains(token)).unwrap_or_else(|| panic!("mac popup {token}"));
        let mut expected: Vec<String> = mac_tokens(src);
        let mut got: Vec<String> = win_tokens(popup(&bar, id));
        expected.retain(|t| !windows_only(t));
        // The Mac builds the diagnostics item before the list it sits in (third entry of Help).
        if *id == "help" {
            expected.retain(|t| t != "Other:ExportDiagnostics");
            let at = expected.iter().position(|t| t == "App:SupportData").map_or(expected.len(), |p| p + 1);
            expected.insert(at, "Other:ExportDiagnostics".into());
        }
        got.retain(|t| !windows_only(t));
        assert!(!expected.is_empty(), "popup {id} has items in the macOS menu");
        assert_eq!(got, expected, "popup {id}");
    }
}

#[test]
fn the_arrange_popup_nests_align_distribute_dock_and_move_together_like_the_mac() {
    let bar = menu_bar(Locale::En);
    let arrange = popup(&bar, "arrange");
    let subs: Vec<&Submenu> =
        arrange.items.iter().filter_map(|n| if let MenuNode::Submenu(s) = n { Some(s) } else { None }).collect();
    let titles: Vec<&str> = subs.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(
        titles,
        [
            host_text(HostText::MenuAlign, Locale::En),
            host_text(HostText::MenuDistribute, Locale::En),
            "Dock",
            "Move Together",
            // LF16: drawing order and lock/hide are submenus so the popup stays short.
            host_text(HostText::MenuOrder, Locale::En),
            host_text(HostText::MenuLockHide, Locale::En)
        ]
    );
    assert_eq!(win_tokens(subs[0]).len(), 6, "six alignments");
    assert_eq!(win_tokens(subs[2]), ["Cmd:DockLeft", "Cmd:DockRight", "Cmd:DockUp", "Cmd:DockDown"]);
    // Separators group the popup as on the Mac.
    assert!(arrange.items.iter().filter(|n| matches!(n, MenuNode::Separator)).count() >= 4);
}

#[test]
fn the_menu_bar_follows_the_window_language() {
    let nl = menu_bar(Locale::Nl);
    let en = menu_bar(Locale::En);
    assert_eq!(popup(&nl, "edit").title, "Wijzig");
    assert_eq!(popup(&en, "edit").title, "Edit");
    assert_eq!(popup(&nl, "arrange").title, "Schikken");
}

#[test]
fn every_menu_item_has_a_unique_command_id_that_maps_back() {
    let items = menu_items();
    assert!(items.contains(&MenuItem::SchikkenPanel));
    for d in CamDialog::ALL {
        assert!(items.contains(&MenuItem::CamDialog(d)), "{d:?}");
    }
    let mut ids: Vec<u16> = items.iter().map(|i| command_id(*i)).collect();
    for item in &items {
        assert_eq!(menu_item(command_id(*item)), Some(*item), "{item:?}");
    }
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), items.len(), "ids are unique");
    assert!(ids.iter().all(|id| *id >= 100));
    // The numbers of the host items are the macOS tags, so a menu id means the same on both hosts.
    assert_eq!(command_id(MenuItem::SchikkenPanel), 1007);
    assert_eq!(command_id(MenuItem::CamDialog(CamDialog::Optimization)), 4000);
    assert_eq!(command_id(MenuItem::CamDialog(CamDialog::Device)), 4004);
}

#[test]
fn every_item_of_the_menu_bar_is_a_known_item_and_none_appears_twice_in_one_popup() {
    let known = menu_items();
    fn walk(nodes: &[MenuNode], seen: &mut Vec<MenuItem>) {
        for n in nodes {
            match n {
                MenuNode::Item(i) => seen.push(*i),
                MenuNode::Submenu(s) => walk(&s.items, seen),
                MenuNode::Separator => {}
            }
        }
    }
    let bar = menu_bar(Locale::En);
    assert!(bar.len() >= 9, "{} popups", bar.len());
    for menu in bar {
        let mut seen = Vec::new();
        walk(&menu.items, &mut seen);
        assert!(!seen.is_empty(), "{} has items", menu.id);
        for i in &seen {
            assert!(known.contains(i), "{i:?} in {}", menu.id);
            assert_eq!(seen.iter().filter(|x| *x == i).count(), 1, "{i:?} twice in {}", menu.id);
        }
    }
}

fn wm_command(id: u16) -> usize {
    (id as usize) | ((notify::MENU as usize) << 16)
}

fn frame_ctx(document: burnburn_types::DocumentId) -> MessageContext<'static> {
    MessageContext {
        document,
        control: Control::Frame,
        dpi: 96,
        text: "",
        selected_rows: &[],
        alt: false,
        cursor_px: [0, 0],
        time_ms: 0,
    }
}

#[test]
fn a_cam_dialog_item_opens_that_dialog_and_the_schikken_item_toggles_its_panel() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let vm = h.view();
    for d in CamDialog::ALL {
        let id = command_id(MenuItem::CamDialog(d));
        assert_eq!(
            on_message(&frame_ctx(doc), msg::WM_COMMAND, wm_command(id), 0, &vm),
            Reaction::Intent(UiIntent::OpenCamDialog { document: doc, dialog: d }),
            "{d:?}"
        );
    }
    assert_eq!(
        on_message(&frame_ctx(doc), msg::WM_COMMAND, wm_command(command_id(MenuItem::SchikkenPanel)), 0, &vm),
        Reaction::ToggleSchikkenPanel
    );
}

#[test]
fn dialog_and_panel_items_need_a_window_and_titles_and_checks_come_from_the_view_model() {
    let mut h = HeadlessEditorHarness::new();
    let empty = h.view();
    // No window yet: nothing to open a dialog for.
    assert!(!menu_enabled(MenuItem::SchikkenPanel, &empty));
    assert!(!menu_enabled(MenuItem::CamDialog(CamDialog::Device), &empty));
    let doc = h.new_document().unwrap();
    let vm = h.view();
    assert!(menu_enabled(MenuItem::SchikkenPanel, &vm));
    assert!(menu_enabled(MenuItem::CamDialog(CamDialog::Optimization), &vm));
    assert_eq!(menu_title(MenuItem::CamDialog(CamDialog::Optimization), &vm, Locale::En), "Optimization Settings…");
    assert_eq!(menu_title(MenuItem::CamDialog(CamDialog::Device), &vm, Locale::Nl), "Apparaatinstellingen…");
    assert_eq!(menu_title(MenuItem::SchikkenPanel, &vm, Locale::Nl), "Schikkenpaneel");
    assert_eq!(menu_title(MenuItem::Open, &vm, Locale::En), host_text(HostText::Open, Locale::En));
    // Undo names the last edit, as on the Mac.
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (0.0, 0.0), (10.0, 10.0));
    let title = menu_title(MenuItem::Command(CommandKey::Undo), &h.view(), Locale::En);
    assert!(title.starts_with("Undo"), "{title}");
    // The active tool shows a check mark.
    assert!(menu_checked(MenuItem::Command(CommandKey::ToolRectangle), &h.view()));
    assert!(!menu_checked(MenuItem::Command(CommandKey::ToolEllipse), &h.view()));
    assert!(!menu_checked(MenuItem::Open, &h.view()));
}

/// A key equivalent of the macOS menu: the item, the key, Shift and Option.
struct MacKey {
    /// `Cmd:Name` for a command, `Other:Name` for a host item.
    token: String,
    key: String,
    shift: bool,
    option: bool,
}

/// `item(c(CommandKey::X), "k", shift)`, `item(MenuItem::X, "k", shift)`, `keyed(c(CommandKey::X), "k",
/// CMD_OPT)` and `chain(CommandKey::X, sel!(..), "k", CMD[_SHIFT])` in the macOS menu source.
fn mac_key_equivalents() -> Vec<MacKey> {
    let body = mac_main_menu();
    let mut out = Vec::new();
    let name_of = |rest: &str| -> String { rest.chars().take_while(|c| c.is_ascii_alphanumeric()).collect() };
    for line in body.lines() {
        let l = line.trim();
        let args: Vec<&str> = l.split(',').collect();
        let clean = |a: Option<&&str>| a.map(|a| a.trim().trim_matches('"').to_owned()).unwrap_or_default();
        if let Some(rest) = l.strip_prefix("item(c(CommandKey::") {
            let key = clean(args.get(1));
            if !key.is_empty() {
                out.push(MacKey {
                    token: format!("Cmd:{}", name_of(rest)),
                    key,
                    shift: clean(args.get(2)).contains("true"),
                    option: false,
                });
            }
        } else if let Some(rest) = l.strip_prefix("item(MenuItem::") {
            let key = clean(args.get(1));
            if !key.is_empty() {
                out.push(MacKey {
                    token: format!("Other:{}", name_of(rest)),
                    key,
                    shift: clean(args.get(2)).contains("true"),
                    option: false,
                });
            }
        } else if let Some(rest) = l.strip_prefix("keyed(c(CommandKey::") {
            let mask = clean(args.get(2));
            out.push(MacKey {
                token: format!("Cmd:{}", name_of(rest)),
                key: clean(args.get(1)),
                shift: mask.contains("SHIFT"),
                option: mask.contains("OPT"),
            });
        } else if let Some(rest) = l.strip_prefix("chain(CommandKey::") {
            out.push(MacKey {
                token: format!("Cmd:{}", name_of(rest)),
                key: clean(args.get(2)),
                shift: clean(args.get(3)).contains("CMD_SHIFT"),
                option: false,
            });
        }
    }
    out
}

fn virtual_key(key: &str) -> Option<u16> {
    match key {
        "=" => Some(0xBB),
        "-" => Some(0xBD),
        "[" => Some(0xDB),
        "]" => Some(0xDD),
        k if k.len() == 1 => Some(u16::from(k.to_ascii_uppercase().as_bytes()[0])),
        _ => None,
    }
}

/// The menu item a token of the macOS menu stands for.
fn item_of(token: &str) -> MenuItem {
    if let Some(name) = token.strip_prefix("Cmd:") {
        return MenuItem::Command(
            *CommandKey::ALL.iter().find(|k| format!("{k:?}") == name).unwrap_or_else(|| panic!("{name}")),
        );
    }
    let name = token.strip_prefix("Other:").unwrap_or(token);
    if name == "Open" {
        return MenuItem::Open;
    }
    MenuItem::Host(
        burnburn_windows::host_panels::menu::items()
            .into_iter()
            .find(|i| format!("{i:?}") == name)
            .unwrap_or_else(|| panic!("{name}")),
    )
}

#[test]
fn every_macos_key_equivalent_is_a_ctrl_accelerator_on_windows() {
    let table = accelerators();
    let equivalents = mac_key_equivalents();
    assert!(equivalents.len() >= 20, "parsed {} key equivalents", equivalents.len());
    for MacKey { token, key, shift, option } in equivalents {
        let item = item_of(&token);
        let vk = virtual_key(&key).unwrap_or_else(|| panic!("key {key}"));
        assert!(
            table.iter().any(|a| a.id == command_id(item)
                && a.key == vk
                && a.ctrl
                && a.shift == shift
                && a.alt == option),
            "{token}: Ctrl{}{} + {key} missing",
            if option { "+Alt" } else { "" },
            if shift { "+Shift" } else { "" }
        );
    }
}

// ---- LF16: the menu structure rules --------------------------------------------------------------

include!("common/menu_rules.rs");

#[test]
fn the_menu_bar_is_the_ten_menus_of_the_mac_guidelines_without_the_application_menu() {
    let bar = menu_bar(Locale::Nl);
    let ids: Vec<&str> = bar.iter().map(|m| m.id).collect();
    assert_eq!(ids, ["file", "edit", "view", "tools", "arrange", "text", "laser", "window", "help"]);
    let titles: Vec<&str> = bar.iter().map(|m| m.title.as_str()).collect();
    assert_eq!(
        titles,
        ["Archief", "Wijzig", "Weergave", "Gereedschap", "Schikken", "Tekst", "Laser", "Venster", "Help"]
    );
}

#[test]
fn the_macos_menu_has_the_same_popups_in_the_same_order() {
    let titles: Vec<String> = mac_popups().into_iter().map(|(title, _)| title).collect();
    let expected = POPUPS.iter().map(|(token, _)| *token).collect::<Vec<_>>();
    assert_eq!(titles.len(), expected.len(), "popups of the macOS menu: {titles:?}");
    for (title, token) in titles.iter().zip(expected) {
        assert!(title.contains(token), "{title} should be {token}");
    }
}

#[test]
fn production_and_the_cam_settings_live_in_the_laser_menu() {
    let bar = menu_bar(Locale::En);
    let laser = win_tokens(popup(&bar, "laser"));
    for token in ["Cmd:Compile", "Cmd:ExportRd", "Cam:Optimization", "Cam:Order", "Cam:Sublayers", "Cam:Device"] {
        assert!(laser.contains(&token.to_owned()), "{token} in Laser: {laser:?}");
    }
    let edit = win_tokens(popup(&bar, "edit"));
    assert!(!edit.iter().any(|t| t.starts_with("Cam:")), "no CAM dialog is left in Edit: {edit:?}");
    // The four settings sit together in one submenu named Operations.
    let laser_menu = popup(&bar, "laser");
    let operations = laser_menu
        .items
        .iter()
        .find_map(|n| match n {
            MenuNode::Submenu(s) if s.title == "Operations" => Some(s),
            _ => None,
        })
        .expect("a submenu Operations in Laser");
    assert_eq!(win_tokens(operations), ["Cam:Optimization", "Cam:Order", "Cam:Sublayers", "Cam:Device"]);
    let nl = menu_bar(Locale::Nl);
    assert!(popup(&nl, "laser").items.iter().any(|n| matches!(n, MenuNode::Submenu(s) if s.title == "Bewerkingen")));
}

#[test]
fn shapes_sketch_and_image_are_submenus_of_the_tools_menu() {
    let bar = menu_bar(Locale::Nl);
    let tools = popup(&bar, "tools");
    let subs: Vec<&str> = tools
        .items
        .iter()
        .filter_map(|n| if let MenuNode::Submenu(s) = n { Some(s.title.as_str()) } else { None })
        .collect();
    assert_eq!(subs, ["Vormen", "Knopen", "Schets", "Beeld"]);
}

#[test]
fn no_item_appears_twice_in_the_whole_menu_bar() {
    for locale in [Locale::En, Locale::Nl] {
        let mut seen: Vec<String> = Vec::new();
        for menu in menu_bar(locale) {
            for token in win_tokens(&menu) {
                assert!(!seen.contains(&token), "{token} appears twice (second time in {})", menu.id);
                seen.push(token);
            }
        }
    }
}

#[test]
fn the_macos_menu_source_has_no_item_twice_and_no_key_twice() {
    let mut seen: Vec<String> = Vec::new();
    for (title, src) in mac_popups() {
        for token in mac_tokens(&src) {
            assert!(!seen.contains(&token), "{token} appears twice in the macOS menu (second time in {title})");
            seen.push(token);
        }
    }
    let mut keys: Vec<(String, bool, bool)> = Vec::new();
    for MacKey { token, key, shift, option } in mac_key_equivalents() {
        let combo = (key.clone(), shift, option);
        assert!(!keys.contains(&combo), "{token}: shortcut {combo:?} is used twice");
        keys.push(combo);
    }
}

#[test]
fn an_item_that_opens_a_dialog_ends_in_an_ellipsis_and_no_other_item_does() {
    let ellipsis = '\u{2026}';
    for locale in [Locale::En, Locale::Nl] {
        let vm = HeadlessEditorHarness::new().view();
        for menu in menu_bar(locale) {
            for (token, item) in win_entries(&menu) {
                let Some(item) = item else { continue };
                let title = menu_title(item, &vm, locale);
                let ends = title.ends_with(ellipsis);
                match opens(&token) {
                    Opens::Dialog => {
                        assert!(ends, "{token} opens a dialog: {title:?} should end in an ellipsis ({locale:?})")
                    }
                    Opens::Window | Opens::Nothing => {
                        assert!(!ends, "{token} opens no dialog: {title:?} must not end in an ellipsis ({locale:?})")
                    }
                }
            }
        }
    }
}

#[test]
fn no_menu_lists_more_than_25_entries_without_grouping_them_in_submenus() {
    fn check(id: &str, nodes: &[MenuNode]) {
        let entries = nodes.iter().filter(|n| !matches!(n, MenuNode::Separator)).count();
        assert!(entries <= 25, "{id} lists {entries} entries");
        for n in nodes {
            if let MenuNode::Submenu(s) = n {
                check(&format!("{id} > {}", s.title), &s.items);
            }
        }
    }
    for locale in [Locale::En, Locale::Nl] {
        for menu in menu_bar(locale) {
            check(menu.id, &menu.items);
        }
    }
}

#[test]
fn the_edit_menu_groups_the_select_commands_in_a_submenu() {
    let bar = menu_bar(Locale::En);
    let edit = popup(&bar, "edit");
    let select = edit
        .items
        .iter()
        .find_map(|n| match n {
            MenuNode::Submenu(s) => Some(s),
            _ => None,
        })
        .expect("Edit has a submenu");
    assert_eq!(select.title, "Select");
    assert!(win_tokens(select).contains(&"Cmd:SelectCircles".to_owned()));
}

#[test]
fn the_shortcut_ids_do_not_depend_on_the_menu_position() {
    // The register and the Windows accelerators name an item by its command, never by the popup it
    // sits in: moving Optimization Settings from Edit to Laser keeps its id and its accelerators.
    let item = MenuItem::CamDialog(CamDialog::Optimization);
    assert_eq!(command_id(item), 4000);
    let bar = menu_bar(Locale::En);
    let at = bar.iter().find(|m| win_tokens(m).contains(&"Cam:Optimization".to_owned())).map(|m| m.id);
    assert_eq!(at, Some("laser"));
    // Every accelerator still points at an item of the bar, wherever the item sits now.
    let known: Vec<u16> = bar.iter().flat_map(win_entries).filter_map(|(_, i)| i).map(command_id).collect();
    for a in accelerators() {
        assert!(known.contains(&a.id), "accelerator for id {} has no menu item", a.id);
    }
}
