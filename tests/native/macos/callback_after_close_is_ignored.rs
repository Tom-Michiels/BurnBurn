//! After a window closed, late callbacks from its widgets reach nothing: no intent, no frame.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("callback_after_close_is_ignored: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("callback_after_close_is_ignored", scenario())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn scenario() -> Result<(), String> {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_presentation::CommandKey;
    use common::expect;
    use objc2::Message;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSTextField, NSView, NSWindow};

    let s = common::start()?;
    let doc = s.new_document()?;
    // Keep our own references to the widgets, as a stray AppKit callback would have them.
    let (window, field, canvas): (Retained<NSWindow>, Retained<NSTextField>, Retained<NSView>) = s
        .app
        .with_window(doc, |w| {
            w.field(burnburn_presentation::FieldKey::Width)
                .map(|f| (w.window.retain(), f.retain(), w.canvas_view.retain()))
        })
        .flatten()
        .ok_or("no window")?;
    // A clean document closes without asking (Cmd+W).
    s.menu(MenuItem::Command(CommandKey::Close))?;
    s.wait(2.0, "the window to close", || !window.isVisible())?;
    expect(|| "session still open after close".into(), s.doc(doc).is_none())?;
    expect(|| "native window still registered".into(), s.app.open_documents().is_empty())?;
    s.app.take_dispatched();

    field.setStringValue(&objc2_foundation::NSString::from_str("200"));
    // SAFETY: sending the field's action with its (now cleared) target, as AppKit would.
    unsafe { field.sendAction_to(field.action(), field.target().as_deref()) };
    s.mouse_on(&window, &canvas)?;
    s.pump(0.1);
    let got = s.app.take_dispatched();
    expect(|| format!("callbacks after close reached the controller: {got:?}"), got.is_empty())?;
    expect(|| "no canvas after close".into(), s.app.with_window(doc, |w| w.canvas_frames).is_none())?;
    Ok(())
}

#[cfg(target_os = "macos")]
trait MouseOn {
    fn mouse_on(&self, window: &objc2_app_kit::NSWindow, view: &objc2_app_kit::NSView) -> Result<(), String>;
}

#[cfg(target_os = "macos")]
impl MouseOn for common::Session {
    /// A mouse-down straight into a closed window's canvas view (its controller is gone).
    fn mouse_on(&self, window: &objc2_app_kit::NSWindow, view: &objc2_app_kit::NSView) -> Result<(), String> {
        use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};
        let at = view.convertPoint_toView(objc2_foundation::NSPoint::new(10.0, 10.0), None);
        let event = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            NSEventType::LeftMouseDown, at, NSEventModifierFlags::empty(), 0.0, window.windowNumber(), None, 0, 1, 1.0,
        )
        .ok_or("no event")?;
        view.mouseDown(&event);
        Ok(())
    }
}
