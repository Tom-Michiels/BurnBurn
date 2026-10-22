//! Shared helpers for the AppKit integration tests. Each test is its own process (`harness =
//! false`, own `main` on the main thread, one scenario per process; ADR 0002).
//!
//! The helpers act like a user: real `NSEvent`s through `NSWindow.sendEvent`, menu items through
//! `performActionForItemAtIndex`, sheet buttons through `performClick:`. Files only go to a fresh
//! directory under the system temp dir, never to user folders.

#![cfg(target_os = "macos")]
// Not every test process uses every helper.
#![allow(dead_code, unsafe_code)]

pub mod windows;

use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::Arc;

use burnburn_geometry::Point;
use burnburn_macos::adapter::{MenuItem, menu_tag};
use burnburn_macos::appkit::{HostApp, HostOptions, prepare_application, pump_until};
use burnburn_presentation::{CommandKey, DocumentViewModel, FieldKey, UiIntent};
use burnburn_types::DocumentId;
use objc2::rc::Retained;
use objc2::{MainThreadMarker, Message};
use objc2_app_kit::{NSApplicationActivationPolicy, NSEvent, NSEventModifierFlags, NSEventType, NSMenu, NSMenuItem};
use objc2_foundation::{NSPoint, NSString};

pub struct Session {
    pub mtm: MainThreadMarker,
    pub app: Rc<HostApp>,
    pub dir: PathBuf,
}

/// The app object with the real filesystem; files only in a fresh temp directory. Dutch UI
/// (explicit, so the tests do not depend on the machine's language).
pub fn start() -> Result<Session, String> {
    start_with(|_| {})
}

/// As [`start`], with changed host options (language, …).
pub fn start_with(change: impl FnOnce(&mut HostOptions)) -> Result<Session, String> {
    let mtm = MainThreadMarker::new().ok_or("not on the main thread")?;
    // `cargo xtask verify` runs the native tests quietly unless `--native` is given.
    let quiet = std::env::var_os("BURNBURN_NATIVE_QUIET").is_some_and(|v| v != "0");
    // Accessory: real windows without a Dock icon or menu-bar takeover during the test. Quiet:
    // Prohibited, so the system can never make a test process the active app (for example when
    // the previous test process quits and macOS hands the focus to the next app).
    let policy =
        if quiet { NSApplicationActivationPolicy::Prohibited } else { NSApplicationActivationPolicy::Accessory };
    prepare_application(mtm, policy);
    let dir = std::env::temp_dir().join(format!("burnburn-native-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut options = HostOptions {
        quit_when_last_window_closes: false,
        float_windows: !quiet,
        locale: Some(burnburn_macos::l10n::Locale::Nl),
        // Never let a test's quit flow end the test process: the host records the outcome.
        terminate_on_quit: false,
        // No autosave tick and no recovery offer unless a test asks (never the user's folder).
        recovery_dir: None,
        autosave_interval_s: None,
        // Never the user's clipboard or recent-projects list.
        private_pasteboard: true,
        recent_projects_file: None,
        quiet_windows: quiet,
    };
    change(&mut options);
    let app = HostApp::new(mtm, Arc::new(burnburn_io::std_fs::StdFileSystem), options);
    Ok(Session { mtm, app, dir })
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Session {
    pub fn pump(&self, seconds: f64) {
        pump_until(self.mtm, seconds, || false);
    }

    pub fn wait(&self, seconds: f64, what: &str, mut done: impl FnMut() -> bool) -> Result<(), String> {
        if pump_until(self.mtm, seconds, &mut done) { Ok(()) } else { Err(format!("timed out waiting for {what}")) }
    }

    pub fn doc(&self, document: DocumentId) -> Option<DocumentViewModel> {
        self.app.view().document(document).cloned()
    }

    /// Cmd+N through the menu; waits until the new window is on screen.
    pub fn new_document(&self) -> Result<DocumentId, String> {
        let before = self.app.open_documents();
        self.menu(MenuItem::Command(CommandKey::New))?;
        let mut found = None;
        self.wait(2.0, "the new window", || {
            found = self.app.open_documents().into_iter().find(|d| !before.contains(d));
            found.is_some_and(|d| self.app.with_window(d, |w| w.window.isVisible()).unwrap_or(false))
        })?;
        let doc = found.ok_or("no window")?;
        self.pump(0.2);
        Ok(doc)
    }

    /// Choose a main-menu item by its tag, as a click or its key equivalent would.
    pub fn menu(&self, item: MenuItem) -> Result<(), String> {
        let (menu, index) = find_item(self.app.menu(), menu_tag(item)).ok_or(format!("no menu item for {item:?}"))?;
        menu.update();
        let enabled = menu.itemAtIndex(index).is_some_and(|i| i.isEnabled());
        if !enabled {
            return Err(format!("menu item {item:?} is disabled"));
        }
        menu.performActionForItemAtIndex(index);
        Ok(())
    }

    /// Menu validation as AppKit does it before showing the menu (`validateMenuItem:`).
    pub fn menu_item_state(&self, item: MenuItem) -> Option<(bool, String)> {
        let (menu, index) = find_item(self.app.menu(), menu_tag(item))?;
        menu.update();
        let it: Retained<NSMenuItem> = menu.itemAtIndex(index)?;
        Some((it.isEnabled(), it.title().to_string()))
    }

    /// Whether the item shows a check mark after menu validation.
    pub fn menu_item_checked(&self, item: MenuItem) -> Option<bool> {
        let (menu, index) = find_item(self.app.menu(), menu_tag(item))?;
        menu.update();
        Some(menu.itemAtIndex(index)?.state() == objc2_app_kit::NSControlStateValueOn)
    }

    /// A mouse event at a canvas position in logical points (top-left origin), sent to the window.
    pub fn mouse(&self, document: DocumentId, kind: NSEventType, at_pt: [f64; 2]) -> Result<(), String> {
        let (window_point, number) = self
            .app
            .with_window(document, |w| {
                (w.canvas_view.convertPoint_toView(NSPoint::new(at_pt[0], at_pt[1]), None), w.window.windowNumber())
            })
            .ok_or("no window")?;
        let event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            kind,
            window_point,
            NSEventModifierFlags::empty(),
            0.0,
            number,
            None,
            0,
            1,
            1.0,
        )
        .ok_or("no mouse event")?;
        self.app.window(document).ok_or("no window")?.sendEvent(&event);
        Ok(())
    }

    /// Drag from one world position to another with the left button (through the window's camera).
    pub fn drag_mm(&self, document: DocumentId, from: (f64, f64), to: (f64, f64)) -> Result<(), String> {
        let pt = |mm: (f64, f64)| -> Result<[f64; 2], String> {
            let d = self.doc(document).ok_or("no document")?;
            Ok(d.camera.world_to_points(&d.viewport, Point::new(mm.0, mm.1)))
        };
        let (a, b) = (pt(from)?, pt(to)?);
        self.mouse(document, NSEventType::LeftMouseDown, a)?;
        self.mouse(document, NSEventType::LeftMouseDragged, [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0])?;
        self.mouse(document, NSEventType::LeftMouseDragged, b)?;
        self.mouse(document, NSEventType::LeftMouseUp, b)?;
        self.pump(0.1);
        Ok(())
    }

    /// Rectangle tool from the menu, then a native drag.
    pub fn draw_rectangle(&self, document: DocumentId, from: (f64, f64), to: (f64, f64)) -> Result<(), String> {
        self.menu(MenuItem::Command(CommandKey::ToolRectangle))?;
        self.drag_mm(document, from, to)
    }

    /// A key press sent to the window (it goes to the first responder, e.g. a field editor).
    pub fn key(&self, document: DocumentId, characters: &str, key_code: u16) -> Result<(), String> {
        let number = self.app.with_window(document, |w| w.window.windowNumber()).ok_or("no window")?;
        let chars = NSString::from_str(characters);
        for kind in [NSEventType::KeyDown, NSEventType::KeyUp] {
            let event = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
                kind,
                NSPoint::new(0.0, 0.0),
                NSEventModifierFlags::empty(),
                0.0,
                number,
                None,
                &chars,
                &chars,
                false,
                key_code,
            )
            .ok_or("no key event")?;
            self.app.window(document).ok_or("no window")?.sendEvent(&event);
        }
        Ok(())
    }

    /// Click into an inspector field, select its text and type `text` key by key.
    pub fn type_into_field(&self, document: DocumentId, field: FieldKey, text: &str) -> Result<(), String> {
        let f =
            self.app.with_window(document, |w| w.field(field).map(|f| f.retain())).flatten().ok_or("no such field")?;
        self.app.window(document).ok_or("no window")?.makeFirstResponder(Some(&f));
        // SAFETY: `selectText:` on our own text field with no sender.
        unsafe { f.selectText(None) };
        for c in text.chars() {
            self.key(document, &c.to_string(), 0)?;
        }
        Ok(())
    }

    pub fn commit_field_intents(&self) -> Vec<UiIntent> {
        self.app.take_dispatched().into_iter().filter(|i| matches!(i, UiIntent::CommitField { .. })).collect()
    }
}

/// The submenu and index of the item with `tag`, searching submenus at any depth.
fn find_item(menu: &NSMenu, tag: isize) -> Option<(Retained<NSMenu>, isize)> {
    for i in 0..menu.numberOfItems() {
        let sub = menu.itemAtIndex(i)?.submenu();
        if let Some(sub) = sub {
            let index = sub.indexOfItemWithTag(tag);
            if index >= 0 {
                return Some((sub, index));
            }
            if let Some(found) = find_item(&sub, tag) {
                return Some(found);
            }
        }
    }
    None
}

impl Session {
    /// Whether the window server shows the window (Metal presents nothing while it is occluded,
    /// e.g. when the screen is locked or the display sleeps).
    pub fn on_screen(&self, document: DocumentId) -> bool {
        self.app
            .with_window(document, |w| {
                w.window.occlusionState().contains(objc2_app_kit::NSWindowOcclusionState::Visible)
            })
            .unwrap_or(false)
    }

    /// Waits up to 2 s for the window to be on screen. `Ok(false)`: it never was (locked screen,
    /// sleeping display) — the caller reports SKIPPED for the on-screen part, which is not a pass.
    /// With `BURNBURN_REQUIRE_SCREEN=1` that is a failure instead.
    pub fn wait_on_screen(&self, document: DocumentId) -> Result<bool, String> {
        let visible = pump_until(self.mtm, 2.0, || self.on_screen(document));
        if !visible && std::env::var_os("BURNBURN_REQUIRE_SCREEN").is_some() {
            return Err("window never on screen (BURNBURN_REQUIRE_SCREEN=1)".into());
        }
        if !visible {
            println!("    SKIPPED on-screen checks (window occluded: screen locked or display asleep; not a pass)");
        }
        Ok(visible)
    }
}

impl Session {
    /// Click button `index` of the window's close sheet, as the user does.
    #[allow(unsafe_code)]
    pub fn click_close_alert(&self, document: DocumentId, index: usize) -> Result<(), String> {
        let button = self
            .app
            .with_window(document, |w| w.close_alert.and_then(|a| a.buttons().iter().nth(index)))
            .flatten()
            .ok_or("no close sheet button")?;
        // SAFETY: clicking our own alert's button with no sender, as the user does.
        unsafe { button.performClick(None) };
        Ok(())
    }

    /// Wait until `document`'s window shows its close sheet.
    pub fn wait_close_alert(&self, document: DocumentId) -> Result<(), String> {
        self.wait(2.0, "the save-changes sheet", || {
            self.app
                .with_window(document, |w| w.close_alert.is_some() && w.window.attachedSheet().is_some())
                .unwrap_or(false)
        })
    }
}

impl Session {
    /// Make `window` the key window. When the (background) test app cannot take key status, post
    /// the window's key notification so its delegate sees what a click into it would cause.
    #[allow(unsafe_code)]
    pub fn become_key(&self, window: &objc2_app_kit::NSWindow) {
        window.makeKeyWindow();
        if !window.isKeyWindow() {
            let center = objc2_foundation::NSNotificationCenter::defaultCenter();
            // SAFETY: AppKit's constant notification name, posted with the window as its object,
            // exactly as NSWindow posts it.
            unsafe {
                center.postNotificationName_object(objc2_app_kit::NSWindowDidBecomeKeyNotification, Some(window))
            };
        }
        self.pump(0.1);
    }
}

pub fn expect(what: impl FnOnce() -> String, ok: bool) -> Result<(), String> {
    if ok { Ok(()) } else { Err(what()) }
}

/// Capture only `window` (never the desktop) when `BURNBURN_NATIVE_SCREENSHOT` names a file.
pub fn screenshot(window: &objc2_app_kit::NSWindow) {
    if let Some(path) = std::env::var_os("BURNBURN_NATIVE_SCREENSHOT") {
        let id = window.windowNumber().to_string();
        let status = std::process::Command::new("screencapture").args(["-x", "-o", "-l", &id]).arg(&path).status();
        println!("    screenshot of window {id} to {}: {status:?}", path.to_string_lossy());
    }
}

/// Print libtest-like output for one scenario and turn its result into the exit code.
pub fn report(name: &str, result: Result<(), String>) -> ExitCode {
    match result {
        Ok(()) => {
            println!("test {name} ... ok\n\ntest result: ok. 1 passed; 0 failed");
            ExitCode::SUCCESS
        }
        Err(e) => {
            println!("test {name} ... FAILED\n    {e}\n\ntest result: FAILED. 0 passed; 1 failed");
            ExitCode::FAILURE
        }
    }
}
