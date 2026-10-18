//! The pen tool in the real window: moving the mouse between clicks (no button) reaches the tool as
//! hover, so the rubber-band preview follows the pointer; Return on the canvas finishes an open
//! path as one object.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("pen_tool: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("pen_hover_follows_the_mouse_and_return_finishes_an_open_path", scenario())
}

#[cfg(target_os = "macos")]
fn scenario() -> Result<(), String> {
    use burnburn_geometry::Point;
    use burnburn_macos::adapter::MenuItem;
    use burnburn_presentation::{CommandKey, UiIntent};
    use common::expect;
    use objc2::Message;
    use objc2_app_kit::NSEventType;

    let s = common::start()?;
    let doc = s.new_document()?;
    s.menu(MenuItem::Command(CommandKey::ToolPen))?;
    let canvas = s.app.with_window(doc, |w| w.canvas_view.retain()).ok_or("no canvas")?;
    s.app.window(doc).ok_or("no window")?.makeFirstResponder(Some(&canvas));
    let pt = |x: f64, y: f64| -> Result<[f64; 2], String> {
        let d = s.doc(doc).ok_or("no document")?;
        Ok(d.camera.world_to_points(&d.viewport, Point::new(x, y)))
    };
    for (x, y) in [(10.0, 10.0), (60.0, 10.0)] {
        s.mouse(doc, NSEventType::LeftMouseDown, pt(x, y)?)?;
        s.mouse(doc, NSEventType::LeftMouseUp, pt(x, y)?)?;
    }
    s.pump(0.1);
    s.app.take_dispatched();

    // Hover between clicks: a button-less move reaches presentation.
    s.mouse(doc, NSEventType::MouseMoved, pt(60.0, 50.0)?)?;
    s.pump(0.1);
    let sent = s.app.take_dispatched();
    let hover = sent.iter().any(|i| matches!(i, UiIntent::Pointer { event, .. } if event.button.is_none()));
    expect(|| format!("mouse move without button reaches the pen, got {sent:?}"), hover)?;

    // Return finishes the open path: one new object.
    s.mouse(doc, NSEventType::LeftMouseDown, pt(60.0, 50.0)?)?;
    s.mouse(doc, NSEventType::LeftMouseUp, pt(60.0, 50.0)?)?;
    s.key(doc, "\r", 36)?;
    s.pump(0.2);
    let objects = s.doc(doc).map(|d| d.objects.len()).unwrap_or(0);
    expect(|| format!("Return finishes the pen path: {objects} objects"), objects == 1)?;
    Ok(())
}
