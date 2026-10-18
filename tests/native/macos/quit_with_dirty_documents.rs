//! IM20: quitting (`applicationShouldTerminate:`) with dirty documents asks per document with the
//! same native sheet as the close button. Niet bewaren closes that one and asks the next;
//! Annuleer stops the quit and keeps every remaining document open and unchanged.
//!
//! The delegate method is called the way AppKit calls it; the test host records the outcome
//! instead of `replyToApplicationShouldTerminate:` so the test process is not ended (the real
//! Cmd-Q modal loop is a manual check, see docs/dev-notes/IM20.md).

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("quit_with_dirty_documents: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("quit_with_dirty_documents_asks_each_and_cancel_keeps_all_open", scenario())
}

#[cfg(target_os = "macos")]
fn scenario() -> Result<(), String> {
    use burnburn_macos::appkit::QuitOutcome;
    use common::expect;
    use objc2_app_kit::{NSApplication, NSApplicationDelegate, NSApplicationTerminateReply};

    let s = common::start()?;
    let a = s.new_document()?;
    s.draw_rectangle(a, (0.0, 0.0), (40.0, 20.0))?;
    let clean = s.new_document()?;
    let b = s.new_document()?;
    s.draw_rectangle(b, (0.0, 0.0), (30.0, 30.0))?;

    let app = NSApplication::sharedApplication(s.mtm);
    let delegate = app.delegate().ok_or("the host installs an application delegate")?;
    let reply = delegate.applicationShouldTerminate(&app);
    expect(
        || format!("dirty documents: terminate later, got {reply:?}"),
        reply == NSApplicationTerminateReply::TerminateLater,
    )?;

    s.wait_close_alert(a)?;
    expect(|| "only one sheet at a time".into(), s.app.with_window(b, |w| w.close_alert.is_none()).unwrap_or(false))?;
    s.click_close_alert(a, 2)?; // Niet bewaren
    s.wait(2.0, "the first window to close", || !s.app.open_documents().contains(&a))?;
    s.wait_close_alert(b)?;
    s.click_close_alert(b, 1)?; // Annuleer
    s.wait(2.0, "the quit to be cancelled", || !s.app.quit_outcomes().is_empty())?;
    let outcomes = s.app.quit_outcomes();
    expect(|| format!("quit cancelled, got {outcomes:?}"), outcomes == [QuitOutcome::Cancelled])?;
    expect(
        || "the cancelled document stays open and dirty".into(),
        s.doc(b).is_some_and(|d| d.dirty && d.objects.len() == 1),
    )?;
    expect(|| "its window stays".into(), s.app.with_window(b, |w| w.window.isVisible()).unwrap_or(false))?;
    expect(|| "the clean document stays".into(), s.doc(clean).is_some() && s.app.open_documents().contains(&clean))?;
    expect(|| "the discarded one is gone".into(), s.doc(a).is_none())?;

    // Quit again: only b is asked; Niet bewaren lets the app terminate.
    let reply = delegate.applicationShouldTerminate(&app);
    expect(|| format!("again later, got {reply:?}"), reply == NSApplicationTerminateReply::TerminateLater)?;
    s.wait_close_alert(b)?;
    s.click_close_alert(b, 2)?;
    s.wait(2.0, "the quit outcome", || s.app.quit_outcomes().len() == 2)?;
    let outcomes = s.app.quit_outcomes();
    expect(|| format!("then terminate, got {outcomes:?}"), outcomes.last() == Some(&QuitOutcome::Terminate))?;
    // Nothing dirty any more: terminate right away.
    let reply = delegate.applicationShouldTerminate(&app);
    expect(|| format!("clean: terminate now, got {reply:?}"), reply == NSApplicationTerminateReply::TerminateNow)?;
    let files = std::fs::read_dir(&s.dir).map(|d| d.count()).unwrap_or(0);
    expect(|| format!("nothing written, found {files} files"), files == 0)?;
    Ok(())
}
