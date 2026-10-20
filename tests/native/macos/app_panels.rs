//! W3-UI-app, native: the application, Window and Help menus, the preferences window, the shortcut
//! editor, the undo history, the preview window, the RD inspector, the support dialog and the keys
//! on the canvas — real AppKit windows and events, files only in a fresh temp folder.
//!
//! Test names (one process, one scenario after the other): see `main`.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("app_panels: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    for (name, scenario) in [
        ("menus_follow_the_register_and_the_debug_log_switch", menus as fn() -> Result<(), String>),
        ("preferences_window_edits_apply_persist_and_refuse_bad_input", preferences),
        ("shortcut_editor_records_resolves_conflicts_and_resets", shortcuts),
        ("history_panel_click_goes_back_and_forward", history),
        ("preview_window_picks_a_move_and_plays_to_the_end", preview),
        ("rd_inspector_lists_the_current_plan_read_only", rd_inspector),
        ("support_dialog_shows_the_report_and_writes_exactly_that", support),
        ("inspector_captions_and_values_follow_the_display_unit", inspector_unit),
        ("canvas_keys_nudge_and_switch_tools", canvas_keys),
        ("app_panels_open_no_network_socket", offline),
    ] {
        let code = common::report(name, scenario());
        if code != ExitCode::SUCCESS {
            return code;
        }
    }
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
mod scenarios {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_presentation::{CommandKey, UiIntent};
    use burnburn_types::DocumentId;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2_app_kit::{
        NSAccessibility, NSButton, NSControl, NSEvent, NSEventModifierFlags, NSEventType, NSMenu, NSPopUpButton,
        NSTextField, NSView,
    };
    use objc2_foundation::{NSPoint, NSString};

    use super::common::Session;

    pub(crate) fn machine_profile() -> burnburn_job::MachineProfile {
        use burnburn_job::{MachineFrame, MachineProfile, OriginCorner, PowerRatio};
        MachineProfile::new(
            "research-1500x900",
            1,
            MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).unwrap(),
            1000.0,
            400.0,
            PowerRatio::new(0.0).unwrap(),
            PowerRatio::new(1.0).unwrap(),
        )
        .unwrap()
    }

    pub(crate) fn cut_draft() -> burnburn_document::process::OperationDraft {
        use burnburn_document::process::{
            Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessSettings, Speed,
        };
        let p = PowerPercent::new(20.0).unwrap();
        OperationDraft {
            name: "Snijden".into(),
            mode: OperationMode::Line { kerf: Kerf::NONE },
            settings: ProcessSettings {
                speed: Speed::mm_per_s(100.0).unwrap(),
                power: PowerRange::new(p, p).unwrap(),
                passes: Passes::new(1).unwrap(),
                air_assist: true,
            },
            output_enabled: true,
        }
    }

    /// A window with a drawn rectangle, a cut operation and a compiled plan.
    pub(crate) fn compiled(s: &Session) -> Result<DocumentId, String> {
        let doc = s.new_document()?;
        s.app.dispatch(UiIntent::SetMachineProfile {
            profile: Box::new(machine_profile()),
            ruida: burnburn_ruida::RuidaProfile {
                magic: burnburn_ruida::Magic::FIXTURE_0X88,
                overscan: burnburn_ruida::OverscanPolicy::ControllerManaged,
            },
        });
        s.draw_rectangle(doc, (10.0, 10.0), (130.0, 90.0))?;
        s.app.dispatch(UiIntent::AddOperation { document: doc, draft: cut_draft() });
        s.menu(MenuItem::Command(CommandKey::ToolSelect))?;
        s.menu(MenuItem::Command(CommandKey::Compile))?;
        s.wait(5.0, "the plan", || s.doc(doc).is_some_and(|d| d.plan.is_some()))?;
        Ok(doc)
    }

    /// Every control of a window's content, depth first.
    pub(crate) fn subviews(root: &NSView, out: &mut Vec<Retained<NSView>>) {
        for v in root.subviews().iter() {
            out.push(v.clone());
            subviews(&v, out);
        }
    }

    pub(crate) fn controls(window: &objc2_app_kit::NSWindow) -> Vec<Retained<NSView>> {
        let mut out = Vec::new();
        if let Some(content) = window.contentView() {
            subviews(&content, &mut out);
        }
        out
    }

    /// The control with this accessibility label.
    pub(crate) fn labelled(window: &objc2_app_kit::NSWindow, label: &str) -> Result<Retained<NSView>, String> {
        controls(window)
            .into_iter()
            .find(|v| v.accessibilityLabel().is_some_and(|l| l.to_string() == label))
            .ok_or_else(|| format!("no control labelled {label:?}"))
    }

    pub(crate) fn text_fields(window: &objc2_app_kit::NSWindow) -> Vec<String> {
        controls(window)
            .iter()
            .filter_map(|v| v.downcast_ref::<NSTextField>().map(|f| f.stringValue().to_string()))
            .collect()
    }

    pub(crate) fn button_titled(window: &objc2_app_kit::NSWindow, title: &str) -> Result<Retained<NSButton>, String> {
        controls(window)
            .into_iter()
            .filter_map(|v| v.downcast::<NSButton>().ok())
            .find(|b| b.title().to_string() == title)
            .ok_or_else(|| format!("no button titled {title:?}"))
    }

    /// Click a button as the user does.
    #[allow(unsafe_code)]
    pub(crate) fn click(button: &NSButton) {
        // SAFETY: `performClick:` on our own button with no sender.
        unsafe { button.performClick(None) };
    }

    /// Send the control's action to its target, as the user's click or Return would.
    #[allow(unsafe_code)]
    pub(crate) fn fire(control: &NSControl) -> Result<(), String> {
        // SAFETY: `target` and `action` are what the host wired; `sendAction:to:` only calls them.
        let target: Option<Retained<AnyObject>> = control.target();
        let action = control.action().ok_or("control has no action")?;
        // SAFETY: as above.
        let sent = unsafe { control.sendAction_to(Some(action), target.as_deref()) };
        if sent { Ok(()) } else { Err("the action was not sent".into()) }
    }

    pub(crate) fn find_menu_item(menu: &NSMenu, title: &str) -> Option<Retained<objc2_app_kit::NSMenuItem>> {
        for i in 0..menu.numberOfItems() {
            let item = menu.itemAtIndex(i)?;
            if item.title().to_string() == title {
                return Some(item);
            }
            if let Some(found) = item.submenu().and_then(|sub| find_menu_item(&sub, title)) {
                return Some(found);
            }
        }
        None
    }

    pub(crate) fn top_titles(menu: &NSMenu) -> Vec<String> {
        (0..menu.numberOfItems()).filter_map(|i| menu.itemAtIndex(i)).map(|i| i.title().to_string()).collect()
    }

    /// A key event for the window with modifiers.
    pub(crate) fn key_event(
        s: &Session,
        document: DocumentId,
        kind: NSEventType,
        chars: &str,
        code: u16,
        flags: NSEventModifierFlags,
    ) -> Result<Retained<NSEvent>, String> {
        let number = s.app.with_window(document, |w| w.window.windowNumber()).ok_or("no window")?;
        let chars = NSString::from_str(chars);
        NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            kind,
            NSPoint::new(0.0, 0.0),
            flags,
            0.0,
            number,
            None,
            &chars,
            &chars,
            false,
            code,
        )
        .ok_or_else(|| "no key event".to_owned())
    }

    /// A key press (down and up) sent to the window with modifiers.
    pub(crate) fn press(
        s: &Session,
        document: DocumentId,
        chars: &str,
        code: u16,
        flags: NSEventModifierFlags,
    ) -> Result<(), String> {
        let window = s.app.window(document).ok_or("no window")?;
        for kind in [NSEventType::KeyDown, NSEventType::KeyUp] {
            window.sendEvent(&*key_event(s, document, kind, chars, code, flags)?);
        }
        s.pump(0.05);
        Ok(())
    }

    pub(crate) fn popup(window: &objc2_app_kit::NSWindow, label: &str) -> Result<Retained<NSPopUpButton>, String> {
        labelled(window, label)?.downcast::<NSPopUpButton>().map_err(|_| format!("{label:?} is not a pop-up"))
    }

    pub(crate) fn field(window: &objc2_app_kit::NSWindow, label: &str) -> Result<Retained<NSTextField>, String> {
        labelled(window, label)?.downcast::<NSTextField>().map_err(|_| format!("{label:?} is not a field"))
    }
}

#[cfg(target_os = "macos")]
use burnburn_macos::adapter::MenuItem;
#[cfg(target_os = "macos")]
use burnburn_presentation::{CommandKey, UiIntent};
#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSButton, NSEvent, NSEventModifierFlags, NSEventType, NSView};
#[cfg(target_os = "macos")]
use objc2_foundation::{NSPoint, NSString};
#[cfg(target_os = "macos")]
use scenarios::*;

#[cfg(target_os = "macos")]
fn user_dir(s: &common::Session) -> std::path::PathBuf {
    s.dir.join("user")
}

// ---- menus ------------------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn menus() -> Result<(), String> {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_macos::app_menu::AppAction;
    use common::expect;

    let s = common::start()?;
    let menu = s.app.menu();
    let titles = top_titles(menu);
    expect(|| format!("menu bar {titles:?}"), titles.first().map(String::as_str) == Some("BurnBurn"))?;
    expect(
        || format!("Window and Help are the last two menus: {titles:?}"),
        titles.len() > 2 && titles[titles.len() - 2] == "Venster" && titles[titles.len() - 1] == "Help",
    )?;
    for title in [
        "Over BurnBurn",
        "Voorkeuren…",
        "Sneltoetsen…",
        "Diensten",
        "Verberg BurnBurn",
        "Verberg andere",
        "Toon alles",
        "Stop BurnBurn",
        "Voorbeeld",
        "RD inspecteren…",
        "Geschiedenis",
        "Minimaliseer",
        "Alles naar voren",
        "BurnBurn-handleiding",
        "Supportgegevens maken…",
        "Debuglog",
    ] {
        expect(|| format!("menu item {title:?} missing"), find_menu_item(menu, title).is_some())?;
    }
    let prefs = find_menu_item(menu, "Voorkeuren…").ok_or("no Preferences item")?;
    expect(
        || "Preferences has Cmd+,".into(),
        prefs.keyEquivalent().to_string() == "," && prefs.keyEquivalentModifierMask() == NSEventModifierFlags::Command,
    )?;
    let quit = find_menu_item(menu, "Stop BurnBurn").ok_or("no Quit item")?;
    expect(|| "Quit keeps Cmd+Q".into(), quit.keyEquivalent().to_string() == "q")?;

    // Every key in the menu is one the shortcut register knows and agrees with: a new menu item
    // with a key of its own has to be registered, or this fails.
    let report = s.app.menu_key_report();
    expect(|| format!("menu keys that the register does not cover: {report:?}"), report == Default::default())?;

    // Enablement: no window, no history, preview or RD; always the others.
    let state = |a| s.menu_item_state(MenuItem::App(a)).map(|(enabled, _)| enabled);
    expect(|| "About is always enabled".into(), state(AppAction::About) == Some(true))?;
    expect(|| "History needs a window".into(), state(AppAction::History) == Some(false))?;
    expect(|| "Preview needs a plan".into(), state(AppAction::Preview) == Some(false))?;
    expect(
        || "the manual is disabled while this build has no address".into(),
        state(AppAction::Manual) == Some(false),
    )?;
    let doc = s.new_document()?;
    expect(|| "History with a window".into(), state(AppAction::History) == Some(true))?;
    expect(|| "Preview still needs a plan".into(), state(AppAction::Preview) == Some(false))?;
    let _ = doc;

    // The debug log switch: off by default, on after the click, off after the second.
    expect(|| "debug log starts off".into(), s.menu_item_checked(MenuItem::App(AppAction::DebugLog)) == Some(false))?;
    s.menu(MenuItem::App(AppAction::DebugLog))?;
    expect(|| "debug log on".into(), s.menu_item_checked(MenuItem::App(AppAction::DebugLog)) == Some(true))?;
    s.menu(MenuItem::App(AppAction::DebugLog))?;
    expect(|| "debug log off again".into(), s.menu_item_checked(MenuItem::App(AppAction::DebugLog)) == Some(false))?;

    // About: name, version and the offline promise.
    let alert = s.app.about_alert();
    let info = alert.informativeText().to_string();
    expect(
        || format!("about text {info:?}"),
        alert.messageText().to_string() == "BurnBurn"
            && info.contains(env!("CARGO_PKG_VERSION"))
            && info.contains("geen netwerkverbinding"),
    )?;
    Ok(())
}

// ---- preferences -------------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn preferences() -> Result<(), String> {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_macos::app_menu::AppAction;
    use burnburn_presentation::LengthUnit;
    use common::expect;

    // The app's autosave tick is on in the app's own options; the tests' options leave it off.
    let s = common::start_with(|o| o.autosave_interval_s = Some(3600.0))?;
    s.app.use_user_dir(user_dir(&s));
    s.menu(MenuItem::App(AppAction::Preferences))?;
    let window = s.app.prefs_window().ok_or("no preferences window")?;
    expect(|| "the window is visible".into(), window.isVisible())?;

    // Units: inches. The physical values stay, the display follows.
    let units = popup(&window, "Eenheden")?;
    units.selectItemAtIndex(1);
    fire(&units)?;
    expect(|| format!("units {:?}", s.app.preferences().units), s.app.preferences().units == LengthUnit::Inches)?;

    // A bare number is inches now: 2.5 in = 63.5 mm. The field gets the raw text.
    let grid = field(&window, "Rasterafstand")?;
    grid.setStringValue(&NSString::from_str("2,5"));
    fire(&grid)?;
    let mm = s.app.preferences().grid_snap_mm;
    expect(|| format!("grid {mm} mm"), (mm - 63.5).abs() < 1e-9)?;
    expect(|| "the unit label follows".into(), text_fields(&window).iter().any(|t| t == "in"))?;

    // A refused value says why and changes nothing.
    grid.setStringValue(&NSString::from_str("abc"));
    fire(&grid)?;
    let texts = text_fields(&window);
    expect(|| format!("banner missing in {texts:?}"), texts.iter().any(|t| t == "Geen geldig getal."))?;
    expect(|| "value unchanged".into(), (s.app.preferences().grid_snap_mm - 63.5).abs() < 1e-9)?;

    // A checkbox and a pop-up with ids.
    let quality = popup(&window, "Weergavekwaliteit")?;
    quality.selectItemAtIndex(0);
    fire(&quality)?;
    expect(|| "coarse quality".into(), s.app.preferences().quality == burnburn_presentation::DisplayQuality::Coarse)?;
    let snap = labelled(&window, "Vastklikken op raster")?;
    let snap = snap.downcast::<NSButton>().map_err(|_| "snap is not a checkbox")?;
    snap.setState(objc2_app_kit::NSControlStateValueOn);
    fire(&snap)?;
    expect(|| "grid snapping on".into(), s.app.preferences().snap_to_grid)?;

    // Display quality reaches the canvas; autosave follows its interval (the app's host options
    // switch the tick on, the tests' do not: this scenario switches it on like the app does).
    expect(
        || format!("tolerance {}", s.app.display_tolerance_px()),
        (s.app.display_tolerance_px() - 1.0).abs() < 1e-9,
    )?;
    let autosave = field(&*prefs_window(&s)?, "Automatisch bewaren (s, 0 = uit)")?;
    autosave.setStringValue(&NSString::from_str("45"));
    fire(&autosave)?;
    expect(|| format!("autosave tick {:?}", s.app.autosave_interval_s()), s.app.autosave_interval_s() == Some(45.0))?;
    autosave.setStringValue(&NSString::from_str("0"));
    fire(&autosave)?;
    expect(|| "autosave off".into(), s.app.autosave_interval_s().is_none())?;

    // The settings file is written in the background, in the user's folder only.
    let file = user_dir(&s).join("preferences.conf");
    s.wait(3.0, "the settings file", || {
        std::fs::read_to_string(&file).is_ok_and(|t| t.contains("units = inch") && t.contains("quality = coarse"))
    })?;
    let text = std::fs::read_to_string(&file).map_err(|e| e.to_string())?;
    expect(
        || format!("file content:\n{text}"),
        text.contains("grid_snap_mm = 63.5") && text.contains("snap_to_grid = on"),
    )?;

    // Defaults again.
    click(&*button_titled(&window, "Standaardwaarden herstellen")?);
    expect(|| "defaults restored".into(), s.app.preferences() == burnburn_presentation::Preferences::default())?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn prefs_window(s: &common::Session) -> Result<Retained<objc2_app_kit::NSWindow>, String> {
    s.app.prefs_window().ok_or_else(|| "no preferences window".to_owned())
}

// ---- shortcut editor ------------------------------------------------------------------------------

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn shortcuts() -> Result<(), String> {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_macos::app_menu::AppAction;
    use common::expect;
    use objc2::msg_send;

    let s = common::start()?;
    s.app.use_user_dir(user_dir(&s));
    let doc = s.new_document()?;
    s.menu(MenuItem::App(AppAction::Shortcuts))?;
    let (window, table, search, buttons, recorder) = s.app.shortcut_widgets().ok_or("no shortcut editor")?;
    expect(|| "visible".into(), window.isVisible())?;
    let texts = s.app.shortcut_texts();
    expect(|| format!("{} lines", texts.len()), texts.len() > 100)?;
    expect(
        || "Herstel is listed with ⌘Z".into(),
        texts.iter().any(|(n, k, d)| n == "Herstel" && k == "⌘Z" && d == "⌘Z"),
    )?;

    // The filter keeps the names that contain the text.
    search.setStringValue(&NSString::from_str("herstel"));
    fire(&search)?;
    let texts = s.app.shortcut_texts();
    expect(
        || format!("filtered {texts:?}"),
        !texts.is_empty() && texts.len() < 6 && texts.iter().all(|(n, ..)| n.to_lowercase().contains("herstel")),
    )?;
    let row = texts.iter().position(|(n, ..)| n == "Herstel").ok_or("no Herstel line")?;
    table.selectRowIndexes_byExtendingSelection(&objc2_foundation::NSIndexSet::indexSetWithIndex(row), false);
    s.pump(0.1);

    // Record a free combination with a real key event.
    let record = |chars: &str, code: u16, flags: NSEventModifierFlags| -> Result<bool, String> {
        click(&buttons[0]);
        let event = key_event(&s, doc, NSEventType::KeyDown, chars, code, flags)?;
        // SAFETY: `performKeyEquivalent:` takes the event and returns BOOL, as the window sends it.
        let taken: bool = unsafe { msg_send![&*recorder, performKeyEquivalent: &*event] };
        s.pump(0.1);
        Ok(taken)
    };
    let cmd_opt = NSEventModifierFlags(NSEventModifierFlags::Command.0 | NSEventModifierFlags::Option.0);
    expect(|| "the armed recorder takes the key".into(), record("u", 32, cmd_opt)?)?;
    let texts = s.app.shortcut_texts();
    expect(
        || format!("Herstel now ⌥⌘U: {texts:?}"),
        texts.iter().any(|(n, k, d)| n == "Herstel" && k == "⌥⌘U" && d == "⌘Z"),
    )?;
    // The menu item follows at once.
    let undo = find_menu_item(s.app.menu(), "Herstel").ok_or("no Undo menu item")?;
    expect(
        || format!("Undo key {:?}", undo.keyEquivalent().to_string()),
        undo.keyEquivalent().to_string() == "u" && undo.keyEquivalentModifierMask() == cmd_opt,
    )?;

    // A key somebody else has: reported, and Replace takes it over.
    search.setStringValue(&NSString::from_str("bewaar"));
    fire(&search)?;
    let texts = s.app.shortcut_texts();
    let row = texts.iter().position(|(n, ..)| n == "Bewaar").ok_or("no Bewaar line")?;
    table.selectRowIndexes_byExtendingSelection(&objc2_foundation::NSIndexSet::indexSetWithIndex(row), false);
    s.pump(0.1);
    expect(|| "recorder takes it".into(), record("u", 32, cmd_opt)?)?;
    let notice = s.app.shortcut_notice().unwrap_or_default();
    expect(|| format!("notice {notice:?}"), notice == "⌥⌘U is al in gebruik door Herstel")?;
    expect(|| "Replace is offered".into(), !buttons[4].isHidden())?;
    expect(|| "nothing changed yet".into(), s.app.shortcut_texts().iter().any(|(n, k, _)| n == "Bewaar" && k == "⌘S"))?;
    click(&buttons[4]);
    s.pump(0.1);
    let texts = s.app.shortcut_texts();
    expect(|| format!("Bewaar took it: {texts:?}"), texts.iter().any(|(n, k, _)| n == "Bewaar" && k == "⌥⌘U"))?;
    expect(|| "the replaced menu item has no key".into(), undo.keyEquivalent().to_string().is_empty())?;
    // The command now runs from the new key, through the menu.
    let save = s.menu_item_state(MenuItem::Command(burnburn_presentation::CommandKey::Save));
    expect(|| format!("Save menu state {save:?}"), save.is_some())?;

    // The tool strip's tooltip shows the tool's own shortcut: choose H for the Select tool.
    search.setStringValue(&NSString::from_str("selecteren"));
    fire(&search)?;
    let texts = s.app.shortcut_texts();
    let row = texts
        .iter()
        .position(|(n, k, _)| k == "V" && n.to_lowercase().contains("selecteren"))
        .ok_or_else(|| format!("no tool line with V in {texts:?}"))?;
    table.selectRowIndexes_byExtendingSelection(&objc2_foundation::NSIndexSet::indexSetWithIndex(row), false);
    s.pump(0.1);
    expect(|| "recorder takes H".into(), record("h", 4, NSEventModifierFlags::empty())?)?;
    s.pump(0.1);
    let doc_window = s.app.window(doc).ok_or("no document window")?;
    let tips: Vec<String> = controls(&doc_window).iter().filter_map(|v| v.toolTip().map(|t| t.to_string())).collect();
    expect(
        || format!("the select tool's tooltip names H: {tips:?}"),
        tips.iter().any(|t| t.contains("(H)")) && !tips.iter().any(|t| t.contains("Selecteer (V)")),
    )?;

    // Reset all: the defaults are back, in the menu too, and the file has no overrides left.
    click(&buttons[3]);
    s.pump(0.2);
    expect(
        || "Undo key is Cmd+Z again".into(),
        undo.keyEquivalent().to_string() == "z" && undo.keyEquivalentModifierMask() == NSEventModifierFlags::Command,
    )?;
    let file = user_dir(&s).join("preferences.conf");
    s.wait(3.0, "the settings file", || std::fs::read_to_string(&file).is_ok_and(|t| !t.contains("shortcut.")))?;
    Ok(())
}

// ---- history -------------------------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn history() -> Result<(), String> {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_macos::app_menu::AppAction;
    use common::expect;

    let s = common::start()?;
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (0.0, 0.0), (40.0, 20.0))?;
    s.draw_rectangle(doc, (60.0, 0.0), (100.0, 20.0))?;
    s.menu(MenuItem::App(AppAction::History))?;
    let (window, table) = s.app.history_widgets().ok_or("no history window")?;
    expect(|| "visible".into(), window.isVisible())?;
    let texts = s.app.history_texts();
    expect(
        || format!("history {texts:?}"),
        texts == ["Begin (zoals geopend)", "Rechthoek tekenen", "Rechthoek tekenen ◀ huidige stand"],
    )?;
    expect(|| "the current row is selected".into(), table.selectedRow() == 2)?;
    let objects = || s.doc(doc).map_or(0, |d| d.objects.len());
    expect(|| "two rectangles".into(), objects() == 2)?;

    // Back one step, to the start, and forward to the end.
    s.app.history_row_clicked(1);
    expect(|| format!("one rectangle, got {}", objects()), objects() == 1)?;
    let texts = s.app.history_texts();
    expect(
        || format!("history {texts:?}"),
        texts == ["Begin (zoals geopend)", "Rechthoek tekenen ◀ huidige stand", "Rechthoek tekenen (ongedaan)"],
    )?;
    s.app.history_row_clicked(0);
    expect(|| "the start: nothing drawn".into(), objects() == 0)?;
    s.app.history_row_clicked(2);
    expect(|| "forward again".into(), objects() == 2)?;
    Ok(())
}

// ---- preview ---------------------------------------------------------------------------------------------

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn preview() -> Result<(), String> {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_macos::app_menu::AppAction;
    use burnburn_macos::app_view::PreviewFrame;
    use common::expect;

    let s = common::start()?;
    let doc = compiled(&s)?;
    s.menu(MenuItem::App(AppAction::Preview))?;
    let (window, image) = s.app.preview_widgets().ok_or("no preview window")?;
    expect(|| "visible".into(), window.isVisible())?;
    let (time, picked) = s.app.preview_texts().ok_or("no texts")?;
    expect(|| format!("time label {time:?}"), time.starts_with("Tijd 0:00 van ≈") && time.ends_with("(schatting)"))?;
    expect(|| format!("pick hint {picked:?}"), picked.contains("Klik op een beweging"))?;
    let shown = image.downcast_ref::<objc2_app_kit::NSImageView>().and_then(|v| v.image()).is_some();
    expect(|| "the preview bitmap is on screen".into(), shown)?;

    // A real click on a cut selects its source (the rectangle) and shows its operation.
    let view = s.app.preview_view(doc).ok_or("no preview model")?;
    let frame = PreviewFrame::for_view(&view, [760, 456], 8);
    let cut = view.preview.moves.iter().find(|m| m.kind == burnburn_presentation::MoveKind::Cut).ok_or("no cut")?;
    let mid = frame.to_px([(cut.from.x_mm() + cut.to.x_mm()) / 2.0, (cut.from.y_mm() + cut.to.y_mm()) / 2.0]);
    let height = image.bounds().size.height;
    let at_window = image.convertPoint_toView(NSPoint::new(mid[0], height - mid[1]), None);
    let number = window.windowNumber();
    let mouse = |kind: NSEventType| {
        NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            kind,
            at_window,
            NSEventModifierFlags::empty(),
            0.0,
            number,
            None,
            0,
            1,
            1.0,
        )
        .ok_or_else(|| "no mouse event".to_owned())
    };
    let before = s.doc(doc).map(|d| d.selection.len()).unwrap_or(0);
    window.sendEvent(&*mouse(NSEventType::LeftMouseDown)?);
    window.sendEvent(&*mouse(NSEventType::LeftMouseUp)?);
    s.pump(0.2);
    let rect = s.doc(doc).and_then(|d| d.objects.first().map(|o| o.id)).ok_or("no object")?;
    let selection = s.doc(doc).map(|d| d.selection).unwrap_or_default();
    expect(|| format!("selection {selection:?} (before: {before})"), selection == vec![rect])?;
    let (_, picked) = s.app.preview_texts().ok_or("no texts")?;
    expect(|| format!("pick line {picked:?}"), picked.starts_with("Gekozen: snijden") && picked.contains("bewerking"))?;

    // Play: the timer runs, the clock advances by what the timer reports, the end stops it.
    click(&*button_titled(&window, "Afspelen")?);
    expect(|| "the playback timer runs".into(), s.app.preview_timer_running())?;
    s.app.preview_advance(0.5);
    let (time, _) = s.app.preview_texts().ok_or("no texts")?;
    expect(|| format!("clock after 0.5 s: {time:?}"), time.starts_with("Tijd 0:00 van ≈"))?;
    s.app.preview_advance(1.0e6);
    let (time, _) = s.app.preview_texts().ok_or("no texts")?;
    let total = view.estimated_total_seconds;
    expect(
        || format!("end of the plan: {time:?}"),
        time.contains(&format!("Tijd {} van", burnburn_macos::app_view::clock_text(total))),
    )?;
    expect(|| "the end stops the timer".into(), !s.app.preview_timer_running())?;
    button_titled(&window, "Afspelen")?;

    // An edit makes the shown plan stale, in words.
    s.app.dispatch(UiIntent::Invoke { document: Some(doc), command: CommandKey::SelectAll });
    s.menu(MenuItem::Command(CommandKey::FlipHorizontal))?;
    s.pump(0.2);
    let texts = text_fields(&window);
    expect(|| format!("stale warning in {texts:?}"), texts.iter().any(|t| t.contains("verouderd")))?;
    Ok(())
}

// ---- RD inspector -----------------------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn rd_inspector() -> Result<(), String> {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_macos::app_menu::AppAction;
    use common::expect;

    let s = common::start()?;
    let doc = compiled(&s)?;
    s.menu(MenuItem::App(AppAction::InspectRd))?;
    let (window, table) = s.app.rd_widgets().ok_or("no RD window")?;
    expect(|| "visible".into(), window.isVisible())?;
    let rows = s.app.rd_rows();
    expect(|| format!("{} records", rows.len()), rows.len() > 10)?;
    expect(
        || format!("first row {:?}", rows[0]),
        rows[0][0].starts_with("0x") && !rows[0][1].is_empty() && !rows[0][3].is_empty(),
    )?;
    expect(|| "records have registry names".into(), rows.iter().any(|r| !r[2].is_empty()))?;
    // Read-only: no column is editable and the window offers no way to send anything.
    let editable = (0..table.numberOfColumns()).any(|i| table.tableColumns().objectAtIndex(i as usize).isEditable());
    expect(|| "no editable column".into(), !editable)?;
    let texts = text_fields(&window);
    expect(|| format!("read-only note in {texts:?}"), texts.iter().any(|t| t.contains("Alleen lezen")))?;
    expect(|| "records of the plan the preview shows".into(), s.app.preview_view(doc).is_some())?;
    Ok(())
}

// ---- support dialog -------------------------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn support() -> Result<(), String> {
    use burnburn_macos::adapter::{MenuItem, NativeEvent};
    use burnburn_macos::app_menu::AppAction;
    use burnburn_macos::app_menu::AppEvent;
    use burnburn_presentation::DiagnosticOptions;
    use common::expect;

    let s = common::start()?;
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (0.0, 0.0), (40.0, 20.0))?;
    let project = s.dir.join("klant-geheim.burnburn");
    s.app.dispatch(UiIntent::Save { document: doc, path: Some(project.clone()) });
    s.wait(3.0, "the save", || s.doc(doc).is_some_and(|d| !d.dirty))?;
    s.menu(MenuItem::App(AppAction::SupportData))?;
    let report = s.app.support_report_text().ok_or("no support dialog")?;
    expect(
        || format!("report:\n{report}"),
        report.contains("project names and paths: left out") && !report.contains("geheim"),
    )?;
    let [paths, _ids] = s.app.support_checkboxes().ok_or("no checkboxes")?;
    expect(|| "both privacy choices start off".into(), paths.state() == objc2_app_kit::NSControlStateValueOff)?;
    paths.setState(objc2_app_kit::NSControlStateValueOn);
    fire(&paths)?;
    let report = s.app.support_report_text().ok_or("no support dialog")?;
    expect(
        || "consent adds the path and says so".into(),
        report.contains("project names and paths: included") && report.contains("klant-geheim"),
    )?;

    // Finish as the save panel would: the file holds exactly what the dialog showed.
    let out = s.dir.join("support.txt");
    let options = DiagnosticOptions { include_project_paths: true, include_machine_ids: false };
    s.app.native(NativeEvent::App(AppEvent::SupportDialogFinished { options, path: Some(out.clone()) }));
    s.wait(3.0, "the support file", || out.exists())?;
    let written = std::fs::read_to_string(&out).map_err(|e| e.to_string())?;
    expect(|| "written text is the shown report".into(), written == report)?;
    Ok(())
}

// ---- keys on the canvas -----------------------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn canvas_keys() -> Result<(), String> {
    use burnburn_presentation::FieldKey;
    use common::expect;

    let s = common::start()?;
    s.app.use_user_dir(user_dir(&s));
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (10.0, 10.0), (50.0, 30.0))?;
    s.menu(MenuItem::Command(CommandKey::ToolSelect))?;
    use objc2::Message;
    let canvas: Retained<NSView> = s.app.with_window(doc, |w| w.canvas_view.retain()).ok_or("no canvas")?;
    s.app.window(doc).ok_or("no window")?.makeFirstResponder(Some(&canvas));
    let x = || -> f64 {
        s.doc(doc)
            .and_then(|d| d.inspector)
            .and_then(|i| i.field(FieldKey::X).map(|f| f.text.replace(',', ".")))
            .and_then(|t| t.parse().ok())
            .unwrap_or(f64::NAN)
    };
    let x0 = x();
    expect(|| format!("x0 {x0}"), x0.is_finite())?;
    let none = NSEventModifierFlags::empty();
    // Right arrow: the normal distance (1 mm); Ctrl: small (0.1 mm); Shift: large (10 mm).
    press(&s, doc, "\u{f703}", 124, none)?;
    expect(|| format!("normal nudge: {} -> {}", x0, x()), (x() - (x0 + 1.0)).abs() < 1e-9)?;
    press(&s, doc, "\u{f703}", 124, NSEventModifierFlags::Control)?;
    expect(|| format!("small nudge: {}", x()), (x() - (x0 + 1.1)).abs() < 1e-9)?;
    press(&s, doc, "\u{f703}", 124, NSEventModifierFlags::Shift)?;
    expect(|| format!("large nudge: {}", x()), (x() - (x0 + 11.1)).abs() < 1e-9)?;
    // One undo step per press.
    s.menu(MenuItem::Command(CommandKey::Undo))?;
    expect(|| format!("one nudge undone: {}", x()), (x() - (x0 + 1.1)).abs() < 1e-9)?;

    // A tool letter picks the tool; with the Text tool active letters are text.
    press(&s, doc, "e", 14, none)?;
    let tool = s.doc(doc).map(|d| d.tool);
    expect(|| format!("tool after E: {tool:?}"), tool == Some(burnburn_editor::ToolKind::EllipseAxes))?;
    press(&s, doc, "x", 7, none)?;
    expect(|| "X is the Text tool".into(), s.doc(doc).map(|d| d.tool) == Some(burnburn_editor::ToolKind::Text))?;
    press(&s, doc, "r", 15, none)?;
    expect(
        || "with the Text tool a plain letter is not a tool switch".into(),
        s.doc(doc).map(|d| d.tool) == Some(burnburn_editor::ToolKind::Text),
    )?;
    Ok(())
}

// ---- inspector unit ----------------------------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn inspector_unit() -> Result<(), String> {
    use burnburn_macos::app_menu::AppAction;
    use burnburn_presentation::FieldKey;
    use common::expect;

    let s = common::start()?;
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (10.0, 10.0), (130.0, 90.0))?;
    let window = s.app.window(doc).ok_or("no window")?;
    let x_value = || -> String {
        s.app
            .with_window(doc, |w| w.field(FieldKey::X).map(|f| f.stringValue().to_string()))
            .flatten()
            .unwrap_or_default()
    };
    let has_caption = |text: &str| text_fields(&window).iter().any(|t| t == text);
    expect(|| format!("X shows 10 mm, got {:?}", x_value()), x_value() == "10")?;
    expect(|| "caption in mm".into(), has_caption("X (mm)"))?;

    s.menu(MenuItem::App(AppAction::Preferences))?;
    let prefs = s.app.prefs_window().ok_or("no preferences window")?;
    let units = popup(&prefs, "Eenheden")?;
    units.selectItemAtIndex(1);
    fire(&units)?;
    s.pump(0.1);
    expect(|| format!("X shows 0.394 in, got {:?}", x_value()), x_value() == "0.394")?;
    expect(
        || "caption in inches".into(),
        has_caption("X (in)") && has_caption("Breedte (in)") && !has_caption("X (mm)"),
    )?;
    expect(|| "rotation and scale captions are unchanged".into(), has_caption("Rotatie (°)"))?;
    units.selectItemAtIndex(0);
    fire(&units)?;
    s.pump(0.1);
    expect(|| "back to millimetres".into(), x_value() == "10" && has_caption("X (mm)"))?;
    Ok(())
}

// ---- offline ------------------------------------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn offline() -> Result<(), String> {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_macos::app_menu::AppAction;
    use common::expect;

    let s = common::start()?;
    s.app.use_user_dir(user_dir(&s));
    let _doc = compiled(&s)?;
    for action in [
        AppAction::Preferences,
        AppAction::Shortcuts,
        AppAction::History,
        AppAction::Preview,
        AppAction::InspectRd,
        AppAction::SupportData,
    ] {
        s.menu(MenuItem::App(action))?;
    }
    s.pump(0.5);
    let pid = std::process::id().to_string();
    let out = std::process::Command::new("lsof")
        .args(["-nP", "-a", "-i", "-p", &pid])
        .output()
        .map_err(|e| format!("lsof: {e}"))?;
    let listing = String::from_utf8_lossy(&out.stdout).into_owned();
    expect(|| format!("internet sockets of pid {pid}:\n{listing}"), listing.trim().is_empty())?;
    Ok(())
}
