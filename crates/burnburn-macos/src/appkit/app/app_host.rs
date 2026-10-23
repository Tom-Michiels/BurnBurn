//! The host's part of the app-level panels (W3-UI-app): the application, Window and Help menus,
//! showing and refreshing the five panel windows, the shortcut keys, the preview timer and the
//! user's preferences. The widgets themselves are in `appkit::app_panels`.
//!
//! The panels borrow `HostApp::app_panels` only for short stretches and never across a dispatch:
//! a dispatch refreshes the windows, which borrows the panels again.

use std::path::PathBuf;
use std::time::Instant;

use block2::RcBlock;
use burnburn_presentation::{DiagnosticOptions, KeyBinding, PrefsViewModel};
use burnburn_types::DocumentId;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{AnyThread, MainThreadMarker, MainThreadOnly, sel};
use objc2_app_kit::{
    NSApplication, NSControlStateValueOn, NSEventModifierFlags, NSImage, NSMenu, NSMenuItem, NSModalResponse,
    NSSavePanel, NSWorkspace,
};
use objc2_foundation::{NSAttributedString, NSData, NSDictionary, NSIndexSet, NSString, NSTimer, NSURL};

use super::{HostApp, command_item};
use crate::adapter::{MenuItem, NativeEvent, menu_item};
use crate::app_menu::{AppAction, AppEvent, ArrowKey, arrow_key, binding_from_event, binding_glyphs};
use crate::app_text::{AppText, app_text};
use crate::app_view::{
    PickNames, PreviewFrame, history_lines, prefs_panel, preview_panel_named, rd_panel, render_preview, shortcut_lines,
    shortcut_problem_text,
};
use crate::appkit::app_panels::windows::{self, PREVIEW_SIZE, present};
use crate::appkit::app_panels::{AppPanels, SupportDialog};
use crate::appkit::objc_try;
use crate::l10n::HostText;

/// Redraw rate of the preview while it plays.
const PREVIEW_TICK_S: f64 = 1.0 / 30.0;

/// Where the app keeps the user's settings (`~/Library/Application Support/BurnBurn`): never a
/// project folder.
pub fn app_user_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library").join("Application Support").join("BurnBurn"))
}

// ---- app icon ----------------------------------------------------------------------------------

/// The BurnBurn icon as a 512 px PNG: `cargo xtask icon` renders it from `assets/app-icon/burnburn.svg`
/// (a test in xtask keeps it current). Bundled runs get the full `.icns` from the bundle instead.
const APP_ICON_PNG: &[u8] = include_bytes!("../../../../../assets/app-icon/burnburn-512.png");

/// The embedded icon as an image (None only if AppKit cannot decode the PNG).
fn app_icon_image() -> Option<Retained<NSImage>> {
    NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(APP_ICON_PNG))
}

/// Whether the running executable sits in an `.app` bundle (then macOS already shows its `.icns`).
fn running_from_bundle() -> bool {
    std::env::current_exe().is_ok_and(|p| p.to_string_lossy().contains(".app/Contents/MacOS/"))
}

/// Gives an unbundled run (`cargo run`, the native tests) the BurnBurn icon in the Dock and in the
/// application switcher instead of the generic executable icon. Does nothing inside a bundle.
#[allow(unsafe_code)]
pub fn install_app_icon(mtm: MainThreadMarker) {
    if running_from_bundle() {
        return;
    }
    if let Some(icon) = app_icon_image() {
        // SAFETY: called on the main thread (the marker), with an image that outlives the call;
        // NSApplication retains it.
        unsafe { NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&icon)) };
    }
}

// ---- menus -------------------------------------------------------------------------------------

fn raw_item(mtm: MainThreadMarker, title: &str, action: Option<Sel>, key: &str) -> Retained<NSMenuItem> {
    // SAFETY: a freshly allocated item with an AppKit action (nil target: the responder chain /
    // NSApplication) and no target retained.
    #[allow(unsafe_code)]
    unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            action,
            &NSString::from_str(key),
        )
    }
}

fn submenu_holder(
    mtm: MainThreadMarker,
    title: &str,
    items: &[Retained<NSMenuItem>],
) -> (Retained<NSMenuItem>, Retained<NSMenu>) {
    let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(title));
    for item in items {
        menu.addItem(item);
    }
    let holder = raw_item(mtm, title, None, "");
    holder.setSubmenu(Some(&menu));
    (holder, menu)
}

/// BurnBurn menu: About, Preferences, Shortcuts, Services, Hide…, Quit. The keys of About,
/// Preferences and Shortcuts come from the shortcut register (`apply_menu_shortcuts`); the system
/// items keep their fixed keys.
pub(super) fn application_menu(
    mtm: MainThreadMarker,
    target: &crate::appkit::classes::MenuTarget,
    locale: crate::l10n::Locale,
) -> Retained<NSMenuItem> {
    let t = |text| app_text(text, locale);
    let app = |a: AppAction| command_item(mtm, target, MenuItem::App(a), a.title(locale));
    let sep = || NSMenuItem::separatorItem(mtm);
    let (services_holder, services) = submenu_holder(mtm, t(AppText::Services), &[]);
    NSApplication::sharedApplication(mtm).setServicesMenu(Some(&services));
    let items = [
        app(AppAction::About),
        sep(),
        app(AppAction::Preferences),
        app(AppAction::Shortcuts),
        sep(),
        services_holder,
        sep(),
        raw_item(mtm, t(AppText::HideApp), Some(sel!(hide:)), "h"),
        raw_item(mtm, t(AppText::HideOthers), Some(sel!(hideOtherApplications:)), "h"),
        raw_item(mtm, t(AppText::ShowAll), Some(sel!(unhideAllApplications:)), ""),
        sep(),
        raw_item(mtm, t(AppText::QuitApp), Some(sel!(terminate:)), "q"),
    ];
    items[8].setKeyEquivalentModifierMask(NSEventModifierFlags(
        NSEventModifierFlags::Command.0 | NSEventModifierFlags::Option.0,
    ));
    let (holder, _) = submenu_holder(mtm, host_title(locale), &items);
    holder
}

fn host_title(locale: crate::l10n::Locale) -> &'static str {
    crate::l10n::host_text(HostText::AppMenu, locale)
}

/// Window menu: `panels` (the library panels), Preview, Inspect RD, History and the standard window
/// commands; AppKit adds the list of open windows to it (`setWindowsMenu`). One Window menu only.
pub(super) fn window_menu(
    mtm: MainThreadMarker,
    target: &crate::appkit::classes::MenuTarget,
    locale: crate::l10n::Locale,
    panels: Vec<Retained<NSMenuItem>>,
) -> Retained<NSMenuItem> {
    let t = |text| app_text(text, locale);
    let app = |a: AppAction| command_item(mtm, target, MenuItem::App(a), a.title(locale));
    let sep = || NSMenuItem::separatorItem(mtm);
    let mut items = panels;
    if !items.is_empty() {
        items.push(sep());
    }
    items.extend([
        app(AppAction::Preview),
        app(AppAction::InspectRd),
        app(AppAction::History),
        app(AppAction::MachineHub),
        sep(),
        raw_item(mtm, t(AppText::Minimize), Some(sel!(performMiniaturize:)), "m"),
        raw_item(mtm, t(AppText::ZoomWindow), Some(sel!(performZoom:)), ""),
        sep(),
        raw_item(mtm, t(AppText::BringAllToFront), Some(sel!(arrangeInFront:)), ""),
    ]);
    let (holder, menu) = submenu_holder(mtm, t(AppText::MenuWindow), &items);
    NSApplication::sharedApplication(mtm).setWindowsMenu(Some(&menu));
    holder
}

/// Help menu: manual, support data, diagnostic export and the debug log switch.
pub(super) fn help_menu(
    mtm: MainThreadMarker,
    target: &crate::appkit::classes::MenuTarget,
    locale: crate::l10n::Locale,
) -> Retained<NSMenuItem> {
    let app = |a: AppAction| command_item(mtm, target, MenuItem::App(a), a.title(locale));
    let diagnostics = command_item(
        mtm,
        target,
        MenuItem::ExportDiagnostics,
        crate::panels::laser_text(crate::panels::LaserText::DiagnosticsItem, locale),
    );
    let items = [
        app(AppAction::Manual),
        NSMenuItem::separatorItem(mtm),
        app(AppAction::SupportData),
        diagnostics,
        NSMenuItem::separatorItem(mtm),
        app(AppAction::DebugLog),
    ];
    submenu_holder(mtm, app_text(AppText::MenuHelp, locale), &items).0
}

/// The key string AppKit wants for a binding's key.
fn key_equivalent(key: &str) -> String {
    let function = |code: u32| char::from_u32(code).map(String::from).unwrap_or_default();
    match key {
        "Tab" => "\t".into(),
        "Return" => "\r".into(),
        "Space" => " ".into(),
        "Delete" => "\u{8}".into(),
        "Escape" => "\u{1b}".into(),
        "Up" => function(0xF700),
        "Down" => function(0xF701),
        "Left" => function(0xF702),
        "Right" => function(0xF703),
        k if k.len() > 1 && k.starts_with('F') => k[1..]
            .parse::<u32>()
            .ok()
            .filter(|n| (1..=12).contains(n))
            .map_or_else(String::new, |n| function(0xF703 + n)),
        k => k.to_owned(),
    }
}

/// The register id of a menu item, if it is one of the editable ones. Items that go through the
/// responder chain have no tag; their action names the command.
fn shortcut_id_of(item: &NSMenuItem) -> Option<String> {
    if let Some(menu_item) = menu_item(item.tag()) {
        return match menu_item {
            MenuItem::Command(key) => Some(format!("cmd.{key:?}")),
            MenuItem::Open => Some("host.open".into()),
            MenuItem::Import => Some("host.import".into()),
            MenuItem::ImportImage => Some("host.import_image".into()),
            MenuItem::TraceImage => Some("host.trace_image".into()),
            MenuItem::LaserWindow => Some("host.laser_window".into()),
            MenuItem::ExportDiagnostics => Some("host.export_diagnostics".into()),
            MenuItem::App(a) => Some(a.shortcut_id().to_owned()),
            MenuItem::CamDialog(dialog) => Some(format!("host.cam.{dialog:?}")),
            MenuItem::SchikkenPanel => Some("host.schikken_panel".into()),
            MenuItem::OffsetPanel => Some("host.offset_panel".into()),
            MenuItem::StructurePanel => Some("host.structure_panel".into()),
            MenuItem::TracePrepPanel => Some("host.trace_prep_panel".into()),
            MenuItem::OperatorPanel => Some("host.operator_panel".into()),
            MenuItem::RemnantsPanel => Some("host.remnants_panel".into()),
            MenuItem::RecognitionPanel => Some("host.recognition_panel".into()),
            MenuItem::Recent(_)
            | MenuItem::ClearRecent
            | MenuItem::RecentTemplate(_)
            | MenuItem::ClearRecentTemplates => None,
        };
    }
    let action = item.action()?;
    let command = if action == sel!(cut:) {
        "Cut"
    } else if action == sel!(copy:) {
        "Copy"
    } else if action == sel!(paste:) {
        "Paste"
    } else if action == sel!(pasteInPlace:) {
        "PasteInPlace"
    } else if action == sel!(selectAll:) {
        "SelectAll"
    } else {
        return None;
    };
    Some(format!("cmd.{command}"))
}

/// What applying the register to the menu found that does not agree with it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MenuKeyReport {
    /// Items with a key of their own that the register does not know (not editable).
    pub unregistered: Vec<String>,
    /// Items whose built-in key differs from the register's default.
    pub mismatched: Vec<String>,
}

fn apply_to_menu(
    check: bool,
    menu: &NSMenu,
    bindings: &std::collections::BTreeMap<String, Option<KeyBinding>>,
    defaults: &std::collections::BTreeMap<String, Option<KeyBinding>>,
    report: &mut MenuKeyReport,
) {
    for i in 0..menu.numberOfItems() {
        let Some(item) = menu.itemAtIndex(i) else { continue };
        if let Some(sub) = item.submenu() {
            apply_to_menu(check, &sub, bindings, defaults, report);
            continue;
        }
        let builtin = item.keyEquivalent().to_string();
        match shortcut_id_of(&item) {
            Some(id) => {
                let Some(binding) = bindings.get(&id) else { continue };
                // The built-in key (written when the menu was built) must be the register's default.
                if check && !builtin.is_empty() {
                    let mask = item.keyEquivalentModifierMask();
                    let built_in = builtin_binding(&builtin, mask);
                    if defaults.get(&id).is_none_or(|d| *d != built_in) {
                        report.mismatched.push(format!("{id}: {builtin:?}"));
                    }
                }
                match binding {
                    Some(b) => {
                        item.setKeyEquivalent(&NSString::from_str(&key_equivalent(&b.key)));
                        item.setKeyEquivalentModifierMask(modifier_mask(b));
                    }
                    None => item.setKeyEquivalent(&NSString::from_str("")),
                }
            }
            None => {
                // Items AppKit or the system put in (Dictation, Emoji, hide, quit …) keep their keys
                // and are not ours; one of our own items without a register entry is reported.
                let ours = item.action().is_some_and(|a| a == sel!(burnburnMenu:));
                if !builtin.is_empty() && ours {
                    report.unregistered.push(format!("{}: {builtin:?}", item.title()));
                }
            }
        }
    }
}

fn modifier_mask(b: &KeyBinding) -> NSEventModifierFlags {
    let mut mask = 0;
    for (on, flag) in [
        (b.command, NSEventModifierFlags::Command),
        (b.control, NSEventModifierFlags::Control),
        (b.option, NSEventModifierFlags::Option),
        (b.shift, NSEventModifierFlags::Shift),
    ] {
        if on {
            mask |= flag.0;
        }
    }
    NSEventModifierFlags(mask)
}

/// The binding a menu item's key string and mask stand for (what the menu was built with).
fn builtin_binding(key: &str, mask: NSEventModifierFlags) -> Option<KeyBinding> {
    let has = |flag: NSEventModifierFlags| mask.0 & flag.0 != 0;
    let name = match key {
        "\t" => "Tab".to_owned(),
        "\r" => "Return".to_owned(),
        " " => "Space".to_owned(),
        other => other.to_lowercase(),
    };
    let text = format!(
        "{}{}{}{}{name}",
        if has(NSEventModifierFlags::Command) { "Cmd+" } else { "" },
        if has(NSEventModifierFlags::Control) { "Ctrl+" } else { "" },
        if has(NSEventModifierFlags::Option) { "Opt+" } else { "" },
        if has(NSEventModifierFlags::Shift) { "Shift+" } else { "" },
    );
    KeyBinding::parse(&text)
}

impl HostApp {
    /// Writes the effective bindings of the shortcut register into the menu items. Returns what did
    /// not agree with the register (tests assert it is empty: a new menu item must be registered).
    pub fn apply_menu_shortcuts(&self) -> MenuKeyReport {
        self.apply_menu_shortcuts_inner(true)
    }

    fn apply_menu_shortcuts_inner(&self, check: bool) -> MenuKeyReport {
        let (bindings, defaults) = match self.controller.try_borrow() {
            Ok(c) => {
                self.applied_shortcut_revision.set(c.shortcuts_revision());
                let view = c.shortcuts_view();
                let map = |f: fn(&burnburn_presentation::ShortcutRow) -> Option<KeyBinding>| {
                    view.rows.iter().map(|r| (r.id.clone(), f(r))).collect::<std::collections::BTreeMap<_, _>>()
                };
                (map(|r| r.binding.clone()), map(|r| r.default.clone()))
            }
            Err(_) => return MenuKeyReport::default(),
        };
        let mut report = MenuKeyReport::default();
        apply_to_menu(check, &self.menu, &bindings, &defaults, &mut report);
        report
    }
}

// ---- performing the items -----------------------------------------------------------------------

impl HostApp {
    /// A menu item that opens an app-level panel or dialog.
    pub(crate) fn perform_app_action(&self, action: AppAction) {
        match action {
            AppAction::About => self.show_about(),
            AppAction::Preferences => self.show_prefs(),
            AppAction::Shortcuts => self.show_shortcuts(),
            AppAction::History => self.show_history(),
            AppAction::MachineHub => self.show_machine_hub(),
            AppAction::Preview => self.show_preview(),
            AppAction::InspectRd => self.show_rd(),
            AppAction::Manual => self.open_manual(),
            AppAction::SupportData => self.show_support_dialog(),
            AppAction::DebugLog => {
                let enabled = self.controller.try_borrow().map(|c| c.debug_log_view().enabled).unwrap_or(false);
                self.native(NativeEvent::App(AppEvent::DebugLogSwitched { enabled: !enabled }));
            }
        }
    }

    /// Check mark of the items whose state lives outside the view model.
    pub(crate) fn app_menu_checked(&self, item: MenuItem) -> bool {
        matches!(item, MenuItem::App(AppAction::DebugLog))
            && self.controller.try_borrow().map(|c| c.debug_log_view().enabled).unwrap_or(false)
    }

    /// The tool shortcuts the user changed (tool id → shortcut text, `None` = removed), for the
    /// tool strip's tooltips.
    pub(super) fn tool_shortcuts(&self) -> std::collections::BTreeMap<String, Option<String>> {
        let Ok(c) = self.controller.try_borrow() else { return Default::default() };
        c.shortcuts_view()
            .rows
            .iter()
            .filter(|r| r.customized)
            .filter_map(|r| match r.target {
                burnburn_presentation::ShortcutTarget::Tool(id) => {
                    Some((id.to_owned(), r.binding.as_ref().map(KeyBinding::display)))
                }
                _ => None,
            })
            .collect()
    }

    /// The preferences in force (tests).
    pub fn preferences(&self) -> burnburn_presentation::Preferences {
        self.controller.try_borrow().map(|c| *c.preferences()).unwrap_or_default()
    }

    /// The interval of the running autosave tick, `None` while it is off (tests).
    pub fn autosave_interval_s(&self) -> Option<f64> {
        self.autosave_timer.try_borrow().ok()?.as_ref().map(|t| t.timeInterval())
    }

    /// The curve tolerance the canvas is drawn with now (tests): the display-quality preference.
    pub fn display_tolerance_px(&self) -> f64 {
        self.display_tolerance_px.get()
    }

    /// The preview model of `document` (tests).
    pub fn preview_view(&self, document: DocumentId) -> Option<burnburn_presentation::PreviewViewModel> {
        self.controller.try_borrow().ok()?.preview_view(document)
    }

    pub(super) fn float(&self) -> bool {
        self.options.float_windows
    }

    pub(super) fn with_panels<T>(&self, f: impl FnOnce(&mut AppPanels) -> T) -> Option<T> {
        self.app_panels.try_borrow_mut().ok().map(|mut p| f(&mut p))
    }

    pub(super) fn active_document(&self) -> Option<DocumentId> {
        self.view().active
    }

    // ---- About and manual ------------------------------------------------------------------------

    /// Options of the standard About panel (not shown): application name, version, credits (platform
    /// and the offline promise) and the BurnBurn icon. The panel itself supplies the layout, the
    /// "Version" wording in the system language and the dark-mode handling.
    pub fn about_panel_options(&self) -> Retained<NSDictionary<NSString, AnyObject>> {
        let info = burnburn_presentation::AppController::help_info();
        let t = |text| app_text(text, self.locale);
        let credits = format!("{} {} {}\n{}", t(AppText::AboutPlatform), info.os, info.arch, t(AppText::AboutOffline));
        let credits = NSAttributedString::initWithString(NSAttributedString::alloc(), &NSString::from_str(&credits));
        let text = |s: &str| Retained::into_super(Retained::into_super(NSString::from_str(s)));
        let mut keys: Vec<Retained<NSString>> = Vec::new();
        let mut values: Vec<Retained<AnyObject>> = Vec::new();
        // An empty build number: the version line then reads "Version 0.1.0", not "0.1.0 (0.1.0)".
        for (key, value) in [
            ("ApplicationName", text(info.app_name)),
            ("ApplicationVersion", text(info.version)),
            ("Version", text("")),
            ("Credits", Retained::into_super(Retained::into_super(credits))),
        ] {
            keys.push(NSString::from_str(key));
            values.push(value);
        }
        if let Some(icon) = app_icon_image() {
            keys.push(NSString::from_str("ApplicationIcon"));
            values.push(Retained::into_super(Retained::into_super(icon)));
        }
        let key_refs: Vec<&NSString> = keys.iter().map(|k| &**k).collect();
        NSDictionary::from_retained_objects(&key_refs, &values)
    }

    #[allow(unsafe_code)]
    fn show_about(&self) {
        let options = self.about_panel_options();
        let app = NSApplication::sharedApplication(self.mtm);
        // SAFETY: the dictionary maps NSString keys to NSString / NSAttributedString / NSImage
        // objects, the types `orderFrontStandardAboutPanelWithOptions:` documents for these keys.
        let shown =
            objc_try("showing the About panel", || unsafe { app.orderFrontStandardAboutPanelWithOptions(&options) });
        if let Err(e) = shown {
            eprintln!("burnburn: {e}");
        }
    }

    /// The manual, in the user's browser: only on this click, never at start, and only when the
    /// build knows an address.
    fn open_manual(&self) {
        let Some(url) = burnburn_presentation::AppController::help_info().manual_url else { return };
        if let Some(url) = NSURL::URLWithString(&NSString::from_str(url)) {
            NSWorkspace::sharedWorkspace().openURL(&url);
        }
    }

    // ---- preferences -------------------------------------------------------------------------------

    fn prefs_view(&self) -> Option<PrefsViewModel> {
        self.controller.try_borrow().ok().map(|c| c.prefs_view())
    }

    pub fn show_prefs(&self) {
        let Some(view) = self.prefs_view() else { return };
        let panel = prefs_panel(&view, self.locale);
        let (mtm, locale, float) = (self.mtm, self.locale, self.float());
        self.with_panels(|p| {
            if p.prefs.is_none() {
                p.prefs = Some(windows::build_prefs(mtm, &p.target, locale, &panel, float));
            }
            if let Some(w) = p.prefs.as_mut() {
                windows::refresh_prefs(w, &panel);
                present(&w.window, float);
            }
        });
    }

    fn refresh_prefs(&self) {
        let Some(view) = self.prefs_view() else { return };
        let panel = prefs_panel(&view, self.locale);
        self.with_panels(|p| {
            if let Some(w) = p.prefs.as_mut() {
                windows::refresh_prefs(w, &panel);
            }
        });
    }

    /// Opens the tab called `title` of the preferences window, for tests that drive it like a user.
    pub fn select_prefs_tab(&self, title: &str) -> bool {
        self.app_panels.try_borrow().ok().and_then(|p| p.prefs.as_ref().map(|w| windows::select_prefs_tab(w, title))).unwrap_or(false)
    }

    /// The window that shows the preferences, for tests.
    pub fn prefs_window(&self) -> Option<Retained<objc2_app_kit::NSWindow>> {
        self.app_panels.try_borrow().ok()?.prefs.as_ref().map(|w| w.window.clone())
    }

    /// Read the user's settings from `dir` through the controller's filesystem port and apply them.
    /// Call once at start (the app) or with a temporary folder (tests); never a project folder.
    pub fn use_user_dir(&self, dir: PathBuf) {
        if let Ok(mut c) = self.controller.try_borrow_mut() {
            c.set_user_dir(dir);
        }
        self.apply_preferences();
        self.apply_menu_shortcuts();
        self.refresh();
    }

    /// Put what the preferences say into effect where the host owns it: the autosave tick and the
    /// canvas quality. Snapping, nudge distances and units are presentation's.
    fn apply_preferences(&self) {
        let Some(prefs) = self.controller.try_borrow().ok().map(|c| *c.preferences()) else { return };
        // Desktop automation listens exactly while its preference is on (opt-in, loopback only);
        // idempotent, and retried by the next refresh when the pump was busy.
        self.sync_automation(prefs.desktop_automation);
        if self.applied_prefs.get() == Some(prefs) {
            return;
        }
        let before = self.applied_prefs.replace(Some(prefs));
        if before.is_none_or(|b| b.autosave_seconds != prefs.autosave_seconds)
            && self.options.autosave_interval_s.is_some()
        {
            self.set_autosave_interval((prefs.autosave_seconds > 0).then_some(f64::from(prefs.autosave_seconds)));
        }
        if before.is_none_or(|b| b.quality != prefs.quality) {
            self.display_tolerance_px.set(prefs.quality.tolerance_px());
            let documents = self.open_documents();
            for d in documents {
                self.redraw(d);
            }
        }
    }

    /// (Re)start the autosave tick; `None` switches it off.
    #[allow(unsafe_code)]
    pub(super) fn set_autosave_interval(&self, seconds: Option<f64>) {
        if let Ok(mut slot) = self.autosave_timer.try_borrow_mut() {
            if let Some(old) = slot.take() {
                old.invalidate();
            }
            *slot = seconds.map(|s| self.make_timer(s, HostApp::autosave_now));
        }
    }

    /// A repeating main-thread timer calling `tick`; the caller owns (and invalidates) it.
    #[allow(unsafe_code)]
    fn make_timer(&self, interval_s: f64, tick: fn(&HostApp)) -> Retained<NSTimer> {
        let me = self.me.clone();
        let block = RcBlock::new(move |_timer: std::ptr::NonNull<NSTimer>| {
            crate::appkit::classes::ffi_guard("timer", (), || {
                if let Some(app) = me.upgrade() {
                    tick(&app);
                }
            });
        });
        // SAFETY: a block-based timer on the main run loop; the block only upgrades a `Weak` and
        // calls the host on the main thread; the owner invalidates it before the host drops.
        unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(interval_s, true, &block) }
    }

    // ---- shortcut editor ---------------------------------------------------------------------------

    pub fn show_shortcuts(&self) {
        let (mtm, locale, float, me) = (self.mtm, self.locale, self.float(), self.me.clone());
        self.with_panels(|p| {
            if p.shortcuts.is_none() {
                p.shortcuts = Some(windows::build_shortcuts(mtm, &p.target, me, locale, float));
            }
            if let Some(w) = &p.shortcuts {
                present(&w.window, float);
            }
        });
        self.refresh_shortcuts();
    }

    fn refresh_shortcuts(&self) {
        let Some(view) = self.controller.try_borrow().ok().map(|c| c.shortcuts_view()) else { return };
        let locale = self.locale;
        self.with_panels(|p| {
            let lines = shortcut_lines(&view, &p.shortcut_filter, locale);
            if let Ok(mut data) = p.target.ivars_data().try_borrow_mut() {
                data.shortcuts = lines;
            }
            let Some(w) = &p.shortcuts else { return };
            if let Err(e) = objc_try("reloading the shortcut table", || w.table.reloadData()) {
                eprintln!("burnburn: {e}");
            }
            let selected = w.table.selectedRow() >= 0;
            w.buttons[0].setEnabled(selected);
            w.buttons[1].setEnabled(selected);
            w.buttons[2].setEnabled(selected);
            let attempted = p.pending_conflict.as_ref().map(|(_, b)| binding_glyphs(b)).unwrap_or_default();
            let text = view
                .error
                .as_ref()
                .map(|e| shortcut_problem_text(&e.problem, &attempted, &view, locale))
                .unwrap_or_default();
            windows::set_notice(w, &text, p.pending_conflict.is_some());
            if !p.recording {
                windows::set_prompt(w, "");
            }
        });
    }

    fn selected_shortcut_id(&self) -> Option<String> {
        let panels = self.app_panels.try_borrow().ok()?;
        let w = panels.shortcuts.as_ref()?;
        let row = usize::try_from(w.table.selectedRow()).ok()?;
        let data = panels.target.ivars_data().try_borrow().ok()?;
        data.shortcuts.get(row).map(|l| l.id.clone())
    }

    pub(crate) fn shortcut_filter_changed(&self, text: &str) {
        self.with_panels(|p| p.shortcut_filter = text.to_owned());
        self.refresh_shortcuts();
    }

    pub(crate) fn shortcut_selection_changed(&self) {
        self.refresh_shortcuts();
    }

    /// Record…, Clear, Default, Reset all, Replace.
    pub(crate) fn shortcut_button(&self, tag: isize) {
        let id = self.selected_shortcut_id();
        match (tag, id) {
            (0, Some(_)) => self.arm_shortcut_recorder(),
            (1, Some(id)) => {
                self.native(NativeEvent::App(AppEvent::ShortcutCleared { id }));
            }
            (2, Some(id)) => {
                self.native(NativeEvent::App(AppEvent::ShortcutReset { id }));
            }
            (3, _) => self.native(NativeEvent::App(AppEvent::ShortcutsResetAll)),
            (4, _) => {
                let pending = self.with_panels(|p| p.pending_conflict.take()).flatten();
                if let Some((id, binding)) = pending {
                    self.native(NativeEvent::App(AppEvent::ShortcutRecorded { id, binding, replace: true }));
                }
            }
            _ => {}
        }
        self.refresh_shortcuts();
    }

    fn arm_shortcut_recorder(&self) {
        let locale = self.locale;
        self.with_panels(|p| {
            p.recording = true;
            p.pending_conflict = None;
            if let Some(w) = &p.shortcuts {
                w.recorder.set_armed(true);
                windows::set_prompt(w, app_text(AppText::ShortcutRecording, locale));
                w.window.makeFirstResponder(Some(&w.recorder));
            }
        });
    }

    /// The recorder captured a key combination (`None`: not a key we can name).
    pub(crate) fn shortcut_recorded(&self, binding: Option<KeyBinding>) {
        self.with_panels(|p| {
            p.recording = false;
            if let Some(w) = &p.shortcuts {
                w.recorder.set_armed(false);
                windows::set_prompt(w, "");
            }
        });
        let Some(binding) = binding else { return };
        let cancelled = binding.key == "Escape" && !binding.has_command_modifier() && !binding.shift;
        let Some(id) = self.selected_shortcut_id().filter(|_| !cancelled) else {
            self.refresh_shortcuts();
            return;
        };
        self.native(NativeEvent::App(AppEvent::ShortcutRecorded {
            id: id.clone(),
            binding: binding.clone(),
            replace: false,
        }));
        let conflict = self
            .controller
            .try_borrow()
            .ok()
            .and_then(|c| c.shortcuts_view().error)
            .is_some_and(|e| matches!(e.problem, burnburn_presentation::ShortcutProblem::Conflict { .. }));
        self.with_panels(|p| p.pending_conflict = conflict.then_some((id, binding)));
        self.refresh_shortcuts();
    }

    /// A key on the canvas that is not one of the fixed canvas keys: a tool or command shortcut.
    pub(crate) fn canvas_shortcut(&self, document: DocumentId, chars: &str, code: u16, flags: usize) -> bool {
        let Some(binding) = binding_from_event(chars, code, flags) else { return false };
        let intent = self.controller.try_borrow().ok().and_then(|c| c.shortcut_intent(document, &binding));
        match intent {
            Some(intent) => {
                self.dispatch(intent);
                true
            }
            None => false,
        }
    }

    /// An arrow key on the canvas nudges the selection.
    pub(crate) fn canvas_arrow(&self, document: DocumentId, key: ArrowKey, flags: usize) {
        self.native(NativeEvent::App(AppEvent::ArrowKey { document, key, flags }));
    }

    pub(crate) fn arrow_for(&self, code: u16) -> Option<ArrowKey> {
        arrow_key(code)
    }

    /// The widgets of the shortcut editor, for tests: window, table, search field, buttons
    /// (Record, Clear, Default, Reset all, Replace) and the recorder.
    #[allow(clippy::type_complexity)]
    pub fn shortcut_widgets(
        &self,
    ) -> Option<(
        Retained<objc2_app_kit::NSWindow>,
        Retained<objc2_app_kit::NSTableView>,
        Retained<objc2_app_kit::NSTextField>,
        Vec<Retained<objc2_app_kit::NSButton>>,
        Retained<objc2_app_kit::NSView>,
    )> {
        let panels = self.app_panels.try_borrow().ok()?;
        let w = panels.shortcuts.as_ref()?;
        let recorder: Retained<objc2_app_kit::NSView> = Retained::into_super(w.recorder.clone());
        Some((w.window.clone(), w.table.clone(), w.search.clone(), w.buttons.to_vec(), recorder))
    }

    /// The text of the shortcut editor's notice line (tests).
    pub fn shortcut_notice(&self) -> Option<String> {
        self.app_panels.try_borrow().ok()?.shortcuts.as_ref().map(windows::notice_text)
    }

    // ---- history ---------------------------------------------------------------------------------------

    pub fn show_history(&self) {
        let (mtm, locale, float) = (self.mtm, self.locale, self.float());
        self.with_panels(|p| {
            if p.history.is_none() {
                p.history = Some(windows::build_history(mtm, &p.target, locale, float));
            }
            if let Some(w) = &p.history {
                present(&w.window, float);
            }
        });
        self.refresh_history();
    }

    fn refresh_history(&self) {
        let locale = self.locale;
        let lines = self
            .active_document()
            .and_then(|d| self.controller.try_borrow().ok().and_then(|c| c.history_view(d)))
            .map(|v| (history_lines(&v, locale), v.position));
        self.with_panels(|p| {
            let (lines, position) = lines.unwrap_or_default();
            if let Ok(mut data) = p.target.ivars_data().try_borrow_mut() {
                data.history = lines;
            }
            let Some(w) = &p.history else { return };
            if let Err(e) = objc_try("reloading the history table", || {
                w.table.reloadData();
                w.table.selectRowIndexes_byExtendingSelection(&NSIndexSet::indexSetWithIndex(position), false);
                w.table.scrollRowToVisible(position as isize);
            }) {
                eprintln!("burnburn: {e}");
            }
        });
    }

    /// A click on row `row` of the history table (row 0 is the start).
    pub fn history_row_clicked(&self, row: usize) {
        if let Some(document) = self.active_document() {
            self.native(NativeEvent::App(AppEvent::HistoryRowClicked { document, row }));
        }
    }

    /// The history window and its table, for tests.
    pub fn history_widgets(&self) -> Option<(Retained<objc2_app_kit::NSWindow>, Retained<objc2_app_kit::NSTableView>)> {
        let panels = self.app_panels.try_borrow().ok()?;
        let w = panels.history.as_ref()?;
        Some((w.window.clone(), w.table.clone()))
    }

    // ---- preview ---------------------------------------------------------------------------------------

    pub fn show_preview(&self) {
        let (mtm, locale, float, me) = (self.mtm, self.locale, self.float(), self.me.clone());
        let titles: Vec<String> =
            burnburn_presentation::PlaybackSpeed::ALL.iter().map(|s| format!("{}×", s.factor())).collect();
        self.with_panels(|p| {
            if p.preview.is_none() {
                p.preview = Some(windows::build_preview(mtm, &p.target, me, locale, &titles, float));
            }
            if let Some(w) = &p.preview {
                present(&w.window, float);
            }
        });
        self.refresh_preview();
    }

    fn refresh_preview(&self) {
        let (locale, vm) = (self.locale, self.view());
        let view = vm.active.and_then(|d| self.controller.try_borrow().ok().and_then(|c| c.preview_view(d)));
        let names = view.as_ref().and_then(|v| {
            let doc = vm.document(vm.active?)?;
            let source = v.selected.and_then(|s| s.source)?;
            Some(PickNames {
                geometry: doc.objects.iter().find(|o| o.id == source.geometry).map(|o| o.label.clone()),
                operation: doc.operations.iter().find(|o| o.id == source.operation).map(|o| o.name.clone()),
            })
        });
        let panel = view.as_ref().map(|v| preview_panel_named(v, &names.unwrap_or_default(), locale));
        let frame = view.as_ref().map(|v| PreviewFrame::for_view(v, PREVIEW_SIZE, 8));
        let bitmap = view.as_ref().zip(frame.as_ref()).map(|(v, f)| render_preview(v, f));
        let playing = view.as_ref().is_some_and(|v| v.playing);
        let shown = self.with_panels(|p| {
            p.preview_frame = frame;
            match &mut p.preview {
                Some(w) => {
                    windows::refresh_preview(w, panel.as_ref(), bitmap.as_ref());
                    true
                }
                None => false,
            }
        });
        if shown == Some(true) {
            self.sync_preview_timer(playing);
        }
    }

    /// Run the playback tick while the plan plays; stop it when it does not.
    fn sync_preview_timer(&self, playing: bool) {
        let running = self.app_panels.try_borrow().map(|p| p.preview_timer.is_some()).unwrap_or(true);
        if playing && !running {
            let timer = self.make_timer(PREVIEW_TICK_S, HostApp::preview_tick);
            self.with_panels(|p| {
                p.preview_timer = Some(timer);
                p.last_tick = Some(Instant::now());
            });
        } else if !playing && running {
            self.with_panels(|p| {
                if let Some(timer) = p.preview_timer.take() {
                    timer.invalidate();
                }
                p.last_tick = None;
            });
        }
    }

    /// The playback timer: the wall time since the last tick goes to presentation, which owns the
    /// position. Only the preview refreshes (not every window).
    pub fn preview_tick(&self) {
        let seconds = self
            .with_panels(|p| {
                let now = Instant::now();
                let dt = p.last_tick.map_or(0.0, |t| now.duration_since(t).as_secs_f64());
                p.last_tick = Some(now);
                dt
            })
            .unwrap_or(0.0);
        self.preview_advance(seconds);
    }

    /// Advance the playback by `seconds` of wall time (the timer's body; tests call it directly).
    pub fn preview_advance(&self, seconds: f64) {
        let Some(document) = self.active_document() else { return };
        let update = self.dispatch_without_apply(burnburn_presentation::UiIntent::App(
            burnburn_presentation::AppIntent::Preview(burnburn_presentation::PreviewIntent::Tick { document, seconds }),
        ));
        let _ = update;
        self.refresh_preview();
    }

    pub(crate) fn preview_button(&self, tag: isize, on: bool) {
        let Some(document) = self.active_document() else { return };
        let view = self.controller.try_borrow().ok().and_then(|c| c.preview_view(document));
        let Some(view) = view else { return };
        match tag {
            0 => self.native(NativeEvent::App(AppEvent::PreviewPlayToggled { document, playing: !view.playing })),
            9 => self.native(NativeEvent::App(AppEvent::PreviewTravelToggled { document, show: on })),
            step @ (-1 | 1) => {
                let count = view.preview.moves.len();
                if count == 0 {
                    return;
                }
                let index = match (view.selected.map(|s| s.index), step) {
                    (Some(i), 1) => (i + 1).min(count - 1),
                    (Some(i), _) => i.saturating_sub(1),
                    (None, 1) => 0,
                    (None, _) => count - 1,
                };
                self.native(NativeEvent::App(AppEvent::PreviewMoveChosen { document, index }));
            }
            _ => {}
        }
    }

    pub(crate) fn preview_seek(&self, seconds: f64) {
        if let Some(document) = self.active_document() {
            self.native(NativeEvent::App(AppEvent::PreviewSeeked { document, seconds }));
        }
    }

    pub(crate) fn preview_speed(&self, index: usize) {
        if let Some(document) = self.active_document() {
            self.native(NativeEvent::App(AppEvent::PreviewSpeedChosen { document, index }));
        }
    }

    /// A click on the preview image at `px` (pixels from the top left of the image).
    pub(crate) fn preview_clicked(&self, px: [f64; 2]) {
        let Some(document) = self.active_document() else { return };
        let Some(frame) = self.app_panels.try_borrow().ok().and_then(|p| p.preview_frame) else { return };
        let (at_mm, tolerance_mm) = (frame.to_mm(px), frame.hit_radius_mm(4.0));
        self.native(NativeEvent::App(AppEvent::PreviewClicked { document, at_mm, tolerance_mm }));
    }

    /// The preview window and its image view, for tests.
    pub fn preview_widgets(&self) -> Option<(Retained<objc2_app_kit::NSWindow>, Retained<objc2_app_kit::NSView>)> {
        let panels = self.app_panels.try_borrow().ok()?;
        let w = panels.preview.as_ref()?;
        Some((w.window.clone(), Retained::into_super(Retained::into_super(Retained::into_super(w.image())))))
    }

    /// The preview's time label and the pick line (tests).
    pub fn preview_texts(&self) -> Option<(String, String)> {
        let panels = self.app_panels.try_borrow().ok()?;
        panels.preview.as_ref().map(windows::preview_texts)
    }

    /// Whether the playback timer is running (tests).
    pub fn preview_timer_running(&self) -> bool {
        self.app_panels.try_borrow().map(|p| p.preview_timer.is_some()).unwrap_or(false)
    }

    // ---- RD inspector ----------------------------------------------------------------------------------

    pub fn show_rd(&self) {
        let (mtm, locale, float) = (self.mtm, self.locale, self.float());
        self.rd_refresh();
        self.with_panels(|p| {
            if p.rd.is_none() {
                p.rd = Some(windows::build_rd(mtm, &p.target, locale, float));
            }
            if let Some(w) = &p.rd {
                present(&w.window, float);
            }
        });
        self.refresh_rd();
    }

    /// Decode the current plan again (after a new compile) and show the list.
    pub(crate) fn rd_refresh(&self) {
        if let Some(document) = self.active_document() {
            self.native(NativeEvent::App(AppEvent::RdInspectRequested { document }));
        }
    }

    fn refresh_rd(&self) {
        let locale = self.locale;
        let panel = self
            .active_document()
            .and_then(|d| self.controller.try_borrow().ok().and_then(|c| c.rd_inspect_view(d)))
            .map(|v| rd_panel(&v, locale));
        self.with_panels(|p| {
            if let Ok(mut data) = p.target.ivars_data().try_borrow_mut() {
                data.rd = panel.as_ref().map(|p| p.rows.clone()).unwrap_or_default();
            }
            let Some(w) = &p.rd else { return };
            if let Err(e) = objc_try("reloading the RD table", || w.table.reloadData()) {
                eprintln!("burnburn: {e}");
            }
            windows::refresh_rd(w, panel.as_ref(), locale);
        });
    }

    /// The RD window and its table, for tests.
    pub fn rd_widgets(&self) -> Option<(Retained<objc2_app_kit::NSWindow>, Retained<objc2_app_kit::NSTableView>)> {
        let panels = self.app_panels.try_borrow().ok()?;
        let w = panels.rd.as_ref()?;
        Some((w.window.clone(), w.table.clone()))
    }

    /// The rows the RD table shows (tests): offset, bytes, name, decoded.
    pub fn rd_rows(&self) -> Vec<[String; 4]> {
        self.app_panels
            .try_borrow()
            .ok()
            .and_then(|p| p.target.ivars_data().try_borrow().ok().map(|d| d.rd.clone()))
            .unwrap_or_default()
    }

    /// The lines the history table shows (tests).
    pub fn history_texts(&self) -> Vec<String> {
        self.app_panels
            .try_borrow()
            .ok()
            .and_then(|p| {
                p.target.ivars_data().try_borrow().ok().map(|d| d.history.iter().map(|l| l.text.clone()).collect())
            })
            .unwrap_or_default()
    }

    /// The lines the shortcut table shows (tests): name, key, default.
    pub fn shortcut_texts(&self) -> Vec<(String, String, String)> {
        self.app_panels
            .try_borrow()
            .ok()
            .and_then(|p| {
                p.target.ivars_data().try_borrow().ok().map(|d| {
                    d.shortcuts.iter().map(|l| (l.name.clone(), l.key_text.clone(), l.default_text.clone())).collect()
                })
            })
            .unwrap_or_default()
    }

    // ---- support data ----------------------------------------------------------------------------------

    fn support_options(&self) -> DiagnosticOptions {
        let on = |b: &objc2_app_kit::NSButton| b.state() == NSControlStateValueOn;
        self.app_panels
            .try_borrow()
            .ok()
            .and_then(|p| {
                p.support
                    .as_ref()
                    .map(|s| DiagnosticOptions { include_project_paths: on(&s.paths), include_machine_ids: on(&s.ids) })
            })
            .unwrap_or_default()
    }

    /// Rewrite the report in the dialog after a checkbox changed.
    pub(crate) fn support_option_changed(&self) {
        let options = self.support_options();
        let report = self.controller.try_borrow().map(|c| c.support_report(&options)).unwrap_or_default();
        self.with_panels(|p| {
            if let Some(s) = &p.support {
                s.report.setString(&NSString::from_str(&report));
            }
        });
    }

    /// The text the support dialog shows now (tests).
    pub fn support_report_text(&self) -> Option<String> {
        self.app_panels.try_borrow().ok()?.support.as_ref().map(|s| s.report.string().to_string())
    }

    /// The two checkboxes of the support dialog (tests): paths, machine identifiers.
    pub fn support_checkboxes(&self) -> Option<[Retained<objc2_app_kit::NSButton>; 2]> {
        let panels = self.app_panels.try_borrow().ok()?;
        let s = panels.support.as_ref()?;
        Some([s.paths.clone(), s.ids.clone()])
    }

    /// Help ▸ Supportgegevens maken…: a save panel whose accessory shows the exact report and the
    /// two privacy choices (both off). Nothing is written until the user saves.
    #[allow(unsafe_code)]
    pub fn show_support_dialog(&self) {
        let locale = self.locale;
        let t = |text| app_text(text, locale);
        let panel = NSSavePanel::savePanel(self.mtm);
        panel.setNameFieldStringValue(&NSString::from_str("burnburn-support.txt"));
        panel.setTitle(Some(&NSString::from_str(t(AppText::SupportSavePanelTitle))));
        panel.setMessage(Some(&NSString::from_str(t(AppText::SupportMessage))));
        let (paths, ids, report, note, accessory) =
            windows::support_accessory(self.mtm, &self.app_panels_target(), locale);
        let initial =
            self.controller.try_borrow().map(|c| c.support_report(&DiagnosticOptions::default())).unwrap_or_default();
        report.setString(&NSString::from_str(&initial));
        panel.setAccessoryView(Some(&accessory));
        self.with_panels(|p| p.support = Some(SupportDialog { paths, ids, report, note }));
        let me = self.me.clone();
        let reader = panel.clone();
        let handler = RcBlock::new(move |response: NSModalResponse| {
            crate::appkit::classes::ffi_guard("support panel completion", (), || {
                let path: Option<PathBuf> = (response == crate::adapter::response::OK)
                    .then(|| reader.URL())
                    .flatten()
                    .and_then(|u| u.to_file_path());
                if let Some(app) = me.upgrade() {
                    let options = app.support_options();
                    app.with_panels(|p| p.support = None);
                    app.native(NativeEvent::App(AppEvent::SupportDialogFinished { options, path }));
                }
            });
        });
        let window = self.active_document().and_then(|d| self.window(d));
        let shown = match &window {
            Some(w) => {
                objc_try("showing the support panel", || panel.beginSheetModalForWindow_completionHandler(w, &handler))
            }
            None => objc_try("showing the support panel", || panel.beginWithCompletionHandler(&handler)),
        };
        if let Err(e) = shown {
            eprintln!("burnburn: {e}");
            self.with_panels(|p| p.support = None);
        }
    }

    fn app_panels_target(&self) -> Retained<crate::appkit::app_panels::AppPanelTarget> {
        match self.app_panels.try_borrow() {
            Ok(p) => p.target.clone(),
            Err(_) => unreachable_target(self.mtm, self.me.clone()),
        }
    }

    // ---- refresh -----------------------------------------------------------------------------------------

    /// Called after every refresh: the open app panels follow the controller.
    pub(crate) fn refresh_app_panels(&self) {
        self.apply_preferences();
        // A shortcut the user changed (or a settings file that was read) goes into the menu items.
        let revision = self.controller.try_borrow().map(|c| c.shortcuts_revision()).unwrap_or(0);
        if revision != self.applied_shortcut_revision.get() {
            self.apply_menu_shortcuts_inner(false);
        }
        let open = |f: fn(&AppPanels) -> bool| self.app_panels.try_borrow().map(|p| f(&p)).unwrap_or(false);
        if open(|p| p.prefs.is_some()) {
            self.refresh_prefs();
        }
        if open(|p| p.shortcuts.is_some()) {
            self.refresh_shortcuts();
        }
        if open(|p| p.history.is_some()) {
            self.refresh_history();
        }
        if open(|p| p.preview.is_some()) {
            self.refresh_preview();
        }
        if open(|p| p.rd.is_some()) {
            self.refresh_rd();
        }
        if open(|p| p.hub.is_some()) {
            self.refresh_hub();
        }
    }
}

/// A target for the (rare) moment the panels are borrowed elsewhere: a fresh one is enough for
/// building an accessory view, because the dialog's checkboxes only need *a* target.
fn unreachable_target(
    mtm: MainThreadMarker,
    app: std::rc::Weak<HostApp>,
) -> Retained<crate::appkit::app_panels::AppPanelTarget> {
    crate::appkit::app_panels::AppPanelTarget::new(mtm, app)
}
