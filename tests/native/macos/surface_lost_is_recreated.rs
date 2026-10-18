//! When the OS takes the canvas surface away, the host recreates it and draws the same scene
//! again; the document (revision, objects, undo) is untouched.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("surface_lost_is_recreated: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("surface_lost_is_recreated_without_document_change", scenario())
}

#[cfg(target_os = "macos")]
fn scenario() -> Result<(), String> {
    use common::expect;

    let s = common::start()?;
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (0.0, 0.0), (120.0, 80.0))?;
    let on_screen = s.wait_on_screen(doc)?;
    if on_screen {
        s.wait(2.0, "a presented frame", || s.app.with_window(doc, |w| w.canvas_frames).unwrap_or(0) >= 1)?;
    }
    let before = s.doc(doc).ok_or("no document")?;
    let (frames, surfaces) = s.app.with_window(doc, |w| (w.canvas_frames, w.canvas_surfaces)).ok_or("no window")?;
    expect(|| format!("one surface so far, got {surfaces}"), surfaces == 1)?;
    s.app.take_dispatched();

    s.app.simulate_surface_loss(doc);
    s.pump(0.2);

    let (frames_after, surfaces_after) =
        s.app.with_window(doc, |w| (w.canvas_frames, w.canvas_surfaces)).ok_or("no window")?;
    expect(|| format!("surface recreated: {surfaces} -> {surfaces_after}"), surfaces_after == 2)?;
    if on_screen {
        expect(|| format!("drawing resumed: {frames} -> {frames_after}"), frames_after > frames)?;
    }
    let after = s.doc(doc).ok_or("no document")?;
    expect(|| "document changed by surface recovery".into(), after == before)?;
    let sent = s.app.take_dispatched();
    expect(|| format!("recovery sent intents: {sent:?}"), sent.is_empty())?;
    Ok(())
}
