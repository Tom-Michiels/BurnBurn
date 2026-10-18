//! The thin AppKit layer (macOS only): application object, document windows, menu, sheets and the
//! canvas view with its Metal surface. Everything here runs on the main thread.
//!
//! Native callbacks (in `classes`) reduce events to [`NativeEvent`](crate::adapter::NativeEvent)s;
//! the headless [`adapter`](crate::adapter) decides what they mean; [`HostApp`] sends the
//! resulting intents to the one `AppController` and performs its `HostEffect`s.

mod app;
// AVFoundation camera adapter (IM27); unsafe FFI with the invariants in its module doc.
#[cfg(feature = "camera")]
#[allow(unsafe_code)]
pub mod camera;
mod canvas;
// `define_class!` and `msg_send!` need unsafe; the invariants are in the module's doc comment.
#[allow(unsafe_code)]
mod classes;
mod toolstrip;
mod window;

use std::process::ExitCode;
use std::sync::Arc;

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSEventMask};
use objc2_foundation::{NSDate, NSDefaultRunLoopMode};

pub use app::{HostApp, HostOptions, QuitOutcome, WindowParts};
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

/// Run the event loop until `done()` or `seconds` elapsed; returns whether `done()` became true.
#[allow(unsafe_code)]
pub fn pump_until(mtm: MainThreadMarker, seconds: f64, mut done: impl FnMut() -> bool) -> bool {
    let app = NSApplication::sharedApplication(mtm);
    let deadline = NSDate::dateWithTimeIntervalSinceNow(seconds);
    // SAFETY: reading an immutable Foundation constant.
    let mode = unsafe { NSDefaultRunLoopMode };
    while deadline.timeIntervalSinceNow() > 0.0 {
        if done() {
            return true;
        }
        let step = NSDate::dateWithTimeIntervalSinceNow(0.02);
        if let Some(event) =
            app.nextEventMatchingMask_untilDate_inMode_dequeue(NSEventMask::Any, Some(&step), mode, true)
        {
            app.sendEvent(&event);
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
    let host = HostApp::new(mtm, Arc::new(burnburn_io::std_fs::StdFileSystem), HostOptions::app());
    // The bed camera is attached, never started: the window's camera section asks for access
    // and starts it only on the user's click.
    #[cfg(feature = "camera")]
    host.attach_camera(Arc::new(camera::AvCamera::new()));
    app.run();
    drop(host);
    ExitCode::SUCCESS
}
