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
use burnburn_windows::l10n::{HostText, Locale, host_text};
use burnburn_windows::menus::{MenuNode, Submenu, menu_bar};

const MAC_APP: &str = include_str!("../../burnburn-macos/src/appkit/app.rs");

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
        } else if rest.starts_with("MenuItem::SchikkenPanel") {
            out.push("Schikken".into());
            i += 20;
        } else if rest.starts_with("MenuItem::Open") {
            out.push("Open".into());
            i += 14;
        } else if rest.starts_with("MenuItem::") {
            let name = ident_at(i + 10);
            if !matches!(name.as_str(), "Command" | "CamDialog") {
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
    // The last popup's source ends where the function's closing assertion starts.
    if let Some(last) = parts.last_mut()
        && let Some(end) = last.find("debug_assert!")
    {
        *last = &last[..end];
    }
    parts
        .into_iter()
        .map(|p| {
            let mut lines = p.lines();
            let first = lines.next().unwrap_or("").trim().to_owned();
            let title = if first.is_empty() { lines.next().unwrap_or("").trim().to_owned() } else { first };
            (title, p.to_owned())
        })
        .collect()
}

fn win_tokens(menu: &Submenu) -> Vec<String> {
    fn walk(nodes: &[MenuNode], out: &mut Vec<String>) {
        for n in nodes {
            match n {
                MenuNode::Item(MenuItem::Command(k)) => out.push(format!("Cmd:{k:?}")),
                MenuNode::Item(MenuItem::Open) => out.push("Open".into()),
                MenuNode::Item(MenuItem::CamDialog(d)) => out.push(format!("Cam:{d:?}")),
                MenuNode::Item(MenuItem::SchikkenPanel) => out.push("Schikken".into()),
                // Items the other lane adds to `MenuItem` land here.
                #[allow(unreachable_patterns)]
                MenuNode::Item(other) => out.push(format!("Other:{other:?}")),
                MenuNode::Separator => {}
                MenuNode::Submenu(s) => walk(&s.items, out),
            }
        }
    }
    let mut out = Vec::new();
    walk(&menu.items, &mut out);
    out
}

/// Items that belong to the other Windows lane (files, laser window, images, production, libraries)
/// or to the macOS application menu: left out of the comparison.
fn other_lane(token: &str) -> bool {
    const OTHER: &[&str] = &[
        "NewFromTemplate",
        "SaveAsTemplate",
        "ExportSvg",
        "ExportDxf",
        "ExportBundle",
        "ImportBundle",
        "ShowMaterialLibrary",
        "ShowArtLibrary",
        "AdjustImage",
        "MaskImage",
        "ReplaceImage",
        "RefreshImage",
        "SaveProcessedBitmap",
        "Compile",
        "ExportRd",
        "ApplyPreview",
    ];
    token.starts_with("Other:") || token.strip_prefix("Cmd:").is_some_and(|n| OTHER.contains(&n))
}

/// Mac popup title source → Windows popup id, for the popups this lane owns.
const POPUPS: &[(&str, &str)] = &[
    ("HostText::MenuFile", "file"),
    ("HostText::MenuEdit", "edit"),
    ("HostText::MenuArrange", "arrange"),
    ("HostText::MenuTools", "tools"),
    ("HostText::MenuView", "view"),
    ("HostText::MenuShapes", "shapes"),
    ("TextText::MenuText", "text"),
    ("HostText::MenuSketch", "sketch"),
    ("LaserText::MenuLaser", "laser"),
];

fn popup<'a>(bar: &'a [Submenu], id: &str) -> &'a Submenu {
    bar.iter()
        .find(|m| m.id == id)
        .unwrap_or_else(|| panic!("menu {id} missing in {:?}", bar.iter().map(|m| m.id).collect::<Vec<_>>()))
}

#[test]
fn every_popup_this_lane_owns_has_the_macos_items_in_the_macos_order() {
    let bar = menu_bar(Locale::En);
    let popups = mac_popups();
    assert!(popups.len() >= 10, "the macOS menu source was parsed: {} popups", popups.len());
    for (token, id) in POPUPS {
        let (_, src) = popups.iter().find(|(t, _)| t.contains(token)).unwrap_or_else(|| panic!("mac popup {token}"));
        let mut expected: Vec<String> = mac_tokens(src);
        let mut got: Vec<String> = win_tokens(popup(&bar, id));
        // File: only the basic items; the rest is the files lane.
        if *id == "file" {
            let basic = |t: &String| matches!(t.as_str(), "Cmd:New" | "Open" | "Cmd:Close" | "Cmd:Save" | "Cmd:SaveAs");
            expected.retain(basic);
            got.retain(basic);
        }
        // Laser: the CAM tests, Print and Cut and the Center Finder; the window item is the laser lane.
        if *id == "laser" {
            let mine =
                |t: &String| t.starts_with("Cam:") || matches!(t.as_str(), "Cmd:PrintAndCut" | "Cmd:CenterFinder");
            expected.retain(mine);
            got.retain(mine);
        }
        expected.retain(|t| !other_lane(t));
        got.retain(|t| !other_lane(t));
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
            "Move Together"
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

/// `item(c(CommandKey::X), "k", shift)` and `chain(CommandKey::X, sel!(..), "k", CMD[_SHIFT])` in
/// the macOS menu source: `(command, key, shift)`.
fn mac_key_equivalents() -> Vec<(String, String, bool)> {
    let body = mac_main_menu();
    let mut out = Vec::new();
    for line in body.lines() {
        let l = line.trim();
        if let Some(rest) = l.strip_prefix("item(c(CommandKey::") {
            let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
            let args: Vec<&str> = rest.split(',').collect();
            if let (Some(key), Some(shift)) = (args.get(1), args.get(2)) {
                let key = key.trim().trim_matches('"').to_owned();
                if !key.is_empty() {
                    out.push((name, key, shift.contains("true")));
                }
            }
        } else if let Some(rest) = l.strip_prefix("chain(CommandKey::") {
            let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
            let args: Vec<&str> = rest.split(',').collect();
            if let (Some(key), Some(mask)) = (args.get(2), args.get(3)) {
                out.push((name, key.trim().trim_matches('"').to_owned(), mask.contains("CMD_SHIFT")));
            }
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

#[test]
fn every_macos_key_equivalent_of_this_lane_is_a_ctrl_accelerator_on_windows() {
    let table = accelerators();
    let equivalents = mac_key_equivalents();
    assert!(equivalents.len() >= 15, "parsed {} key equivalents", equivalents.len());
    for (name, key, shift) in equivalents {
        // Compile and Export RD belong to the preview lane.
        if matches!(name.as_str(), "Compile" | "ExportRd") {
            continue;
        }
        let command = MenuItem::Command(
            *CommandKey::ALL.iter().find(|k| format!("{k:?}") == name).unwrap_or_else(|| panic!("{name}")),
        );
        let vk = virtual_key(&key).unwrap_or_else(|| panic!("key {key}"));
        assert!(
            table.iter().any(|a| a.id == command_id(command) && a.key == vk && a.ctrl && a.shift == shift),
            "{name}: Ctrl{} + {key} missing",
            if shift { "+Shift" } else { "" }
        );
    }
}
