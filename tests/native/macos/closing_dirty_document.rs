//! IM20 route, already here: closing a dirty document asks natively (Bewaar / Annuleer / Niet
//! bewaren). Annuleer keeps the session and window open; Niet bewaren then closes without saving.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("closing_dirty_document: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("closing_dirty_native_document_then_cancel_keeps_session_open", scenario())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn scenario() -> Result<(), String> {
    use common::expect;

    let s = common::start()?;
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (0.0, 0.0), (120.0, 80.0))?;
    expect(|| "drawing made the document dirty".into(), s.doc(doc).is_some_and(|d| d.dirty))?;
    let edited = s.app.with_window(doc, |w| w.window.isDocumentEdited()).unwrap_or(false);
    expect(|| "close button shows the edited dot".into(), edited)?;

    // The red close button.
    s.app.window(doc).ok_or("no window")?.performClose(None);
    s.wait(2.0, "the save-changes sheet", || {
        s.app.with_window(doc, |w| w.close_alert.is_some() && w.window.attachedSheet().is_some()).unwrap_or(false)
    })?;
    let titles = s
        .app
        .with_window(doc, |w| {
            w.close_alert.map(|a| a.buttons().iter().map(|b| b.title().to_string()).collect::<Vec<_>>())
        })
        .flatten()
        .ok_or("no alert")?;
    expect(|| format!("buttons {titles:?}"), titles == ["Bewaar", "Annuleer", "Niet bewaren"])?;
    let click = |index: usize| {
        let button = s.app.with_window(doc, |w| w.close_alert.and_then(|a| a.buttons().iter().nth(index))).flatten();
        if let Some(button) = button {
            // SAFETY: clicking our own alert's button with no sender, as the user does.
            unsafe { button.performClick(None) };
        }
    };
    click(1); // Annuleer
    s.wait(2.0, "the sheet to go away", || {
        s.app.with_window(doc, |w| w.window.attachedSheet().is_none()).unwrap_or(false)
    })?;
    s.pump(0.2);
    expect(|| "Annuleer closed the session".into(), s.doc(doc).is_some_and(|d| d.dirty && d.objects.len() == 1))?;
    expect(|| "Annuleer closed the window".into(), s.app.with_window(doc, |w| w.window.isVisible()).unwrap_or(false))?;

    // Again, now Niet bewaren: the window closes, nothing is written.
    s.app.window(doc).ok_or("no window")?.performClose(None);
    s.wait(2.0, "the sheet again", || s.app.with_window(doc, |w| w.close_alert.is_some()).unwrap_or(false))?;
    click(2);
    s.wait(2.0, "the window to close", || s.app.open_documents().is_empty())?;
    expect(|| "session gone after Niet bewaren".into(), s.doc(doc).is_none())?;
    let files = std::fs::read_dir(&s.dir).map(|d| d.count()).unwrap_or(0);
    expect(|| format!("nothing written, found {files} files"), files == 0)?;
    Ok(())
}
