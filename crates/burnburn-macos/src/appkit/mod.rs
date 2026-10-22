//! The thin AppKit layer (macOS only): application object, document windows, menu, sheets and the
//! canvas view with its Metal surface. Everything here runs on the main thread.
//!
//! Native callbacks (in `classes`) reduce events to [`NativeEvent`](crate::adapter::NativeEvent)s;
//! the headless [`adapter`](crate::adapter) decides what they mean; [`HostApp`] sends the
//! resulting intents to the one `AppController` and performs its `HostEffect`s.

mod app;
// `define_class!` and `msg_send!` need unsafe; the invariants are in `classes.rs`'s doc comment.
#[allow(unsafe_code)]
mod app_panels;
// AVFoundation camera adapter (IM27); unsafe FFI with the invariants in its module doc.
#[cfg(feature = "camera")]
#[allow(unsafe_code)]
pub mod camera;
mod canvas;
pub mod clipboard;
// `define_class!` and `msg_send!` need unsafe; the invariants are in the module's doc comment.
#[allow(unsafe_code)]
mod classes;
mod panels;
mod toolstrip;
mod window;

use std::process::ExitCode;
use std::sync::Arc;

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSEventMask, NSNormalWindowLevel, NSWindow};
use objc2_foundation::{NSDate, NSDefaultRunLoopMode};

pub use app::{CamPanelParts, HostApp, HostOptions, QuitOutcome, WindowParts};
pub use app::{MenuKeyReport, app_user_dir};
pub use canvas::{canvas_packet, canvas_packet_themed, canvas_packet_with, canvas_theme};
pub use panels::camera_window::CameraParts;
pub use panels::image::trace_batch_open_panel;
pub use panels::laser::LaserParts;
pub use toolstrip::{ToolStrip, image_pixels};
pub use window::CanvasStatus;

/// The UI language from the user's preferred languages (`NSLocale.preferredLanguages`), as menus
/// and sheets should follow the system language (IM20).
pub fn system_locale() -> crate::l10n::Locale {
    let preferred: Vec<String> =
        objc2_foundation::NSLocale::preferredLanguages().iter().map(|l| l.to_string()).collect();
    crate::l10n::locale_from_preferred(preferred.iter().map(String::as_str))
}

/// Run `body` (AppKit/Foundation calls that may raise); an Objective-C exception becomes `Err`
/// naming `what`, instead of aborting the process (Rust cannot unwind foreign exceptions).
/// `body` must not panic across this boundary: it only makes Objective-C calls (ADR 0002).
pub fn objc_try<T>(what: &str, body: impl FnOnce() -> T) -> Result<T, String> {
    objc2::exception::catch(std::panic::AssertUnwindSafe(body)).map_err(|e| match e {
        Some(exception) => format!("{what}: {exception:?}"),
        None => format!("{what}: exception without object"),
    })
}

/// Run `body` as an AppKit callback runs: a panic is caught at this boundary (logged, never
/// unwound into AppKit) and the host autosaves unsaved work into recovery files.
pub fn run_guarded(what: &str, body: impl FnOnce()) {
    classes::ffi_guard(what, (), body);
}

/// Run the real event loop for about `seconds`, dispatching every event (tests and smoke runs).
pub fn pump_events(mtm: MainThreadMarker, seconds: f64) {
    pump_until(mtm, seconds, || false);
}

/// Quiet native tests (`HostOptions::quiet_windows`): set once by the host, read by the event pump.
static QUIET_WINDOWS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Turns quiet mode on for every window this process shows from now on (tests only).
pub(crate) fn set_quiet_windows(quiet: bool) {
    QUIET_WINDOWS.store(quiet, std::sync::atomic::Ordering::Relaxed);
}

pub(crate) fn quiet_windows() -> bool {
    QUIET_WINDOWS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Makes `window` invisible to the user without closing it: fully transparent, clicks go through
/// to the apps below, normal level (never floating above the user's work).
pub(crate) fn quiet_window(window: &NSWindow) {
    window.setAlphaValue(0.0);
    window.setIgnoresMouseEvents(true);
    window.setLevel(NSNormalWindowLevel);
}

/// In quiet mode: every window of the application (panels, sheets, alerts the host opened since
/// the last pass) is made quiet.
fn quiet_all_windows(app: &NSApplication) {
    if quiet_windows() {
        for window in app.windows().iter() {
            if window.alphaValue() != 0.0 || !window.ignoresMouseEvents() {
                quiet_window(&window);
            }
        }
    }
}

/// Run the event loop until `done()` or `seconds` elapsed; returns whether `done()` became true.
#[allow(unsafe_code)]
pub fn pump_until(mtm: MainThreadMarker, seconds: f64, mut done: impl FnMut() -> bool) -> bool {
    let app = NSApplication::sharedApplication(mtm);
    let deadline = NSDate::dateWithTimeIntervalSinceNow(seconds);
    // SAFETY: reading an immutable Foundation constant.
    let mode = unsafe { NSDefaultRunLoopMode };
    while deadline.timeIntervalSinceNow() > 0.0 {
        quiet_all_windows(&app);
        if done() {
            return true;
        }
        let step = NSDate::dateWithTimeIntervalSinceNow(0.02);
        if let Some(event) =
            app.nextEventMatchingMask_untilDate_inMode_dequeue(NSEventMask::Any, Some(&step), mode, true)
        {
            // Quiet native tests: the user's own mouse and keys never reach the invisible windows.
            if !(quiet_windows() && crate::adapter::quiet_drops_queued_event(event.r#type().0, event.timestamp())) {
                app.sendEvent(&event);
            }
        }
        app.updateWindows();
    }
    done()
}

/// Prepare the shared application (activation policy, launch) before opening windows.
pub fn prepare_application(mtm: MainThreadMarker, policy: NSApplicationActivationPolicy) -> Retained<NSApplication> {
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(policy);
    app.finishLaunching();
    app
}

/// `burnburn`: the real event loop; the application delegate opens the files Finder passes at
/// launch, or one untitled window; quit when the last window closes. Projects are read and written
/// through the real filesystem (`burnburn_io::StdFileSystem`). Start-up opens no network
/// connection (offline, X01; test `offline_start`).
pub fn run_app() -> ExitCode {
    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("burnburn: must start on the main thread");
        return ExitCode::FAILURE;
    };
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    // The delegate is installed before `run` finishes launching, so it sees the launch's
    // `application:openURLs:` and `applicationDidFinishLaunching:`.
    // The language preference decides the menus' language, so it is read before the first menu is
    // built; the rest of the settings are loaded by the host right after.
    let user_dir = app_user_dir();
    let mut options = HostOptions::app();
    if let Some(dir) = &user_dir {
        use burnburn_presentation::LanguageChoice;
        let prefs = burnburn_presentation::AppController::read_preferences(&burnburn_io::std_fs::StdFileSystem, dir);
        options.locale = match prefs.language {
            LanguageChoice::Dutch => Some(crate::l10n::Locale::Nl),
            LanguageChoice::English => Some(crate::l10n::Locale::En),
            LanguageChoice::System => None,
        };
    }
    let host = HostApp::new(mtm, Arc::new(burnburn_io::std_fs::StdFileSystem), options);
    if let Some(dir) = user_dir {
        host.use_user_dir(dir);
    }
    // The bed camera is attached, never started: the window's camera section asks for access
    // and starts it only on the user's click.
    #[cfg(feature = "camera")]
    host.attach_camera(Arc::new(camera::AvCamera::new()));
    app.run();
    drop(host);
    ExitCode::SUCCESS
}
