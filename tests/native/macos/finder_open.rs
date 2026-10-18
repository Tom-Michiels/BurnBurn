//! IM20: a `.burnburn` opened from Finder or the Dock (`application:openURLs:`) opens its own
//! document window; other files are ignored. A launch with a file opens no extra untitled window.
//! The delegate method is called as AppKit calls it after the Apple Event; a real Finder
//! double-click on the bundled app is a manual check.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("finder_open: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("finder_open_request_opens_the_project_in_a_window", scenario())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn scenario() -> Result<(), String> {
    use burnburn_presentation::UiIntent;
    use common::expect;
    use objc2_app_kit::{NSApplication, NSApplicationDelegate};
    use objc2_foundation::{NSArray, NSNotification, NSString, NSURL};

    let s = common::start()?;
    // A project on disk, written through the app itself.
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (0.0, 0.0), (70.0, 35.0))?;
    let path = s.dir.join("Plaat.burnburn");
    s.app.dispatch(UiIntent::Save { document: doc, path: Some(path.clone()) });
    s.wait(3.0, "the save", || s.doc(doc).is_some_and(|d| !d.dirty))?;
    s.app.dispatch(UiIntent::Close { document: doc });
    s.wait(2.0, "the window to close", || s.app.open_documents().is_empty())?;
    std::fs::write(s.dir.join("foto.png"), b"not a project").map_err(|e| e.to_string())?;

    let app = NSApplication::sharedApplication(s.mtm);
    let delegate = app.delegate().ok_or("no application delegate")?;
    let url = |p: &std::path::Path| NSURL::fileURLWithPath(&NSString::from_str(&p.to_string_lossy()));
    let urls = NSArray::from_retained_slice(&[url(&path), url(&s.dir.join("foto.png"))]);
    delegate.application_openURLs(&app, &urls);
    s.wait(3.0, "the opened window", || s.app.open_documents().len() == 1)?;
    s.pump(0.3);
    let opened = s.app.open_documents()[0];
    let d = s.doc(opened).ok_or("no document")?;
    expect(|| format!("title {:?}", d.title), d.title == "Plaat.burnburn")?;
    expect(|| format!("one object, got {}", d.objects.len()), d.objects.len() == 1 && !d.dirty)?;
    // The image was not opened as a project, and no error sheet came up for it.
    expect(|| "only the project opened".into(), s.app.open_documents().len() == 1)?;

    // Finishing the launch after a file was opened does not add an untitled window.
    // SAFETY: a notification without object, as AppKit posts it.
    let launched = unsafe {
        NSNotification::notificationWithName_object(
            &NSString::from_str("NSApplicationDidFinishLaunchingNotification"),
            None,
        )
    };
    delegate.applicationDidFinishLaunching(&launched);
    s.pump(0.3);
    expect(|| format!("still one window, got {:?}", s.app.open_documents()), s.app.open_documents().len() == 1)?;
    Ok(())
}
