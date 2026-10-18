//! Save and open through the native panels and the real filesystem (a fresh temp directory only):
//! NSSavePanel cancel changes nothing; the OK route writes the project (worker thread, main-queue
//! hand-back);
//! NSOpenPanel cancel opens nothing; reopening the file gives the same document (gate A on Mac).

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("native_file_panels: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("native_save_and_open_panels_round_trip_and_cancel_changes_nothing", scenario())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn scenario() -> Result<(), String> {
    use burnburn_macos::adapter::{MenuItem, NativeEvent};
    use burnburn_presentation::{CommandKey, FieldKey};
    use common::expect;
    use objc2::Message;

    let s = common::start()?;
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (0.0, 0.0), (120.0, 80.0))?;
    s.type_into_field(doc, FieldKey::Width, "150")?;
    s.key(doc, "\r", 36)?;
    s.pump(0.1);
    let before = s.doc(doc).ok_or("no document")?;

    // Cmd+S on an untitled document: the save sheet. Cancel: nothing changes.
    s.menu(MenuItem::Command(CommandKey::Save))?;
    s.wait(2.0, "the save sheet", || s.app.with_window(doc, |w| w.save_panel.is_some()).unwrap_or(false))?;
    let panel = s.app.with_window(doc, |w| w.save_panel.map(|p| p.retain())).flatten().ok_or("no save panel")?;
    // SAFETY: the panel's own Cancel action, as its button sends it.
    unsafe { panel.cancel(None) };
    s.wait(2.0, "the sheet to close", || s.app.with_window(doc, |w| w.save_panel.is_none()).unwrap_or(false))?;
    s.pump(0.2);
    let after_cancel = s.doc(doc).ok_or("no document")?;
    expect(|| "cancelled save changed the document".into(), after_cancel == before && after_cancel.dirty)?;
    expect(|| "cancelled save wrote a file".into(), std::fs::read_dir(&s.dir).map(|d| d.count()).unwrap_or(1) == 0)?;

    // OK: macOS hosts NSSavePanel out of process (view bridge); it refuses a programmatic `ok:`
    // (raises) and late `setDirectoryURL:`. So the OK button itself is a manual check; here the
    // panel's completion route is driven with the path it would deliver, through the same
    // adapter → presentation → worker thread → main-queue hand-back → real filesystem.
    let file = s.dir.join("plaat.burnburn");
    s.app.native(NativeEvent::SavePanelFinished { document: doc, path: Some(file.clone()) });
    s.wait(5.0, "the saved document", || s.doc(doc).is_some_and(|d| !d.dirty))?;
    expect(|| format!("{} written", file.display()), file.is_file())?;
    let title = s.app.with_window(doc, |w| w.window.title().to_string()).unwrap_or_default();
    expect(|| format!("title follows the file, got {title:?}"), title == "plaat.burnburn")?;

    // Cmd+O, then Cancel: no document is opened.
    let open_before = s.app.open_documents();
    s.menu(MenuItem::Open)?;
    s.wait(2.0, "the open panel", || s.app.open_panel().is_some())?;
    if let Some(p) = s.app.open_panel() {
        // SAFETY: the panel's own Cancel action.
        unsafe { p.cancel(None) };
    }
    s.wait(2.0, "the open panel to close", || s.app.open_panel().is_none())?;
    s.pump(0.3);
    expect(|| "cancelled open opened a window".into(), s.app.open_documents() == open_before)?;

    // The panel's OK route with the saved file: a second window with the same document content.
    s.app.native(NativeEvent::OpenPanelFinished { path: Some(file.clone()) });
    let mut reopened = None;
    s.wait(5.0, "the reopened document", || {
        reopened = s.app.open_documents().into_iter().find(|d| !open_before.contains(d));
        reopened.is_some()
    })?;
    let r = s.doc(reopened.ok_or("no reopened document")?).ok_or("no view model")?;
    let width = r.inspector.as_ref().and_then(|i| i.field(FieldKey::Width)).map(|f| f.text.clone());
    expect(
        || format!("reopened objects {:?}", r.objects),
        r.objects.len() == 1 && r.objects[0].label == before.objects[0].label,
    )?;
    expect(|| format!("reopened: not dirty, titled {:?}", r.title), !r.dirty && r.title == "plaat.burnburn")?;
    // The reopened rectangle is not selected yet; select it from the object list to read its size.
    s.app.native(NativeEvent::TableSelectionChanged { document: r.id, rows: vec![0] });
    let r = s.doc(r.id).ok_or("no view model")?;
    let width = r.inspector.as_ref().and_then(|i| i.field(FieldKey::Width)).map(|f| f.text.clone()).or(width);
    expect(|| format!("reopened width {width:?}"), width.as_deref() == Some("150"))?;
    Ok(())
}
