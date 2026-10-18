//! The canvas NSView gets a wgpu Metal surface and presents frames of the document's scene; a
//! window resize reaches presentation as a new viewport (in points, with the backing scale).
//!
//! Optional evidence: `BURNBURN_NATIVE_SCREENSHOT=<file.png>` captures this window only
//! (`screencapture -l <window number>`), never the rest of the desktop.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("native_canvas_surface: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("native_canvas_presents_metal_frame_and_follows_resize", scenario())
}

#[cfg(target_os = "macos")]
fn scenario() -> Result<(), String> {
    use burnburn_macos::appkit::CanvasStatus;
    use common::expect;
    use objc2_foundation::NSSize;

    let s = common::start()?;
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (0.0, 0.0), (120.0, 80.0))?;
    let status = s.app.with_window(doc, |w| format!("{:?}", w.canvas_status)).unwrap_or_default();
    expect(|| format!("Metal surface: {status}"), status == format!("{:?}", CanvasStatus::Attached))?;
    let on_screen = s.wait_on_screen(doc)?;
    if on_screen {
        s.wait(2.0, "a presented frame", || s.app.with_window(doc, |w| w.canvas_frames).unwrap_or(0) >= 1)?;
        s.pump(0.3);
        s.app.with_window(doc, |w| common::screenshot(w.window));
    }

    // Resize: the canvas view's new size and the backing scale arrive as the controller's viewport.
    let frames = s.app.with_window(doc, |w| w.canvas_frames).unwrap_or(0);
    s.app.window(doc).ok_or("no window")?.setContentSize(NSSize::new(900.0, 560.0));
    s.pump(0.3);
    let (bounds, scale) =
        s.app.with_window(doc, |w| (w.canvas_view.bounds().size, w.window.backingScaleFactor())).ok_or("no window")?;
    let vp = s.app.viewport(doc).ok_or("no viewport")?;
    expect(
        || format!("viewport {vp:?} vs canvas {bounds:?} @ {scale}"),
        vp.width_pt() == bounds.width && vp.height_pt() == bounds.height && vp.backing_scale() == scale,
    )?;
    let after = s.app.with_window(doc, |w| w.canvas_frames).unwrap_or(0);
    if on_screen {
        expect(|| format!("redrawn after resize: {frames} -> {after}"), after > frames)?;
    }
    Ok(())
}
