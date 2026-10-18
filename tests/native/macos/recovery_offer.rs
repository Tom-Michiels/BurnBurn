//! Autosave and recovery in the real app object: the autosave tick writes a recovery file into
//! the recovery folder (never the project file, the document stays dirty); at start the host
//! offers each recovery file in an alert: Herstel opens it as a new unsaved window, Verwerp
//! removes it. A panic caught at an AppKit callback boundary asks for an autosave as well.
//!
//! The recovery folder is a fresh temp directory, never the user's Application Support.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("recovery_offer: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("autosave_writes_recovery_and_the_start_offer_restores_or_discards", scenario())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn scenario() -> Result<(), String> {
    use std::path::PathBuf;

    use common::expect;

    let recovery = std::env::temp_dir().join(format!("burnburn-recovery-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&recovery);
    let r = recovery.clone();
    let s = common::start_with(move |o| o.recovery_dir = Some(r))?;
    let files = || -> Vec<PathBuf> {
        std::fs::read_dir(&recovery)
            .map(|d| {
                d.filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| p.extension().is_some_and(|x| x == "burnburn"))
                    .collect()
            })
            .unwrap_or_default()
    };

    let doc = s.new_document()?;
    s.draw_rectangle(doc, (10.0, 10.0), (130.0, 90.0))?;
    s.app.autosave_now();
    s.wait(5.0, "a recovery file", || files().len() == 1)?;
    expect(|| "autosave keeps the document dirty".into(), s.doc(doc).is_some_and(|d| d.dirty))?;

    // Start offer: one alert per file; Herstel opens a new, unsaved window.
    let before = s.app.open_documents();
    s.app.check_recovery();
    s.wait(5.0, "the recovery alert", || s.app.recovery_alert().is_some())?;
    let alert = s.app.recovery_alert().ok_or("no alert")?;
    let title = alert.messageText().to_string();
    expect(|| format!("alert title {title:?}"), title == "Herstel een niet-bewaarde versie? (1 van 1)")?;
    let buttons: Vec<String> = alert.buttons().iter().map(|b| b.title().to_string()).collect();
    expect(|| format!("buttons {buttons:?}"), buttons == ["Herstel", "Later", "Verwerp"])?;
    let restore = alert.buttons().iter().next().ok_or("no restore button")?;
    // SAFETY: clicking our own alert's button with no sender, as the user does.
    unsafe { restore.performClick(None) };
    let mut restored = None;
    s.wait(5.0, "the restored window", || {
        restored = s.app.open_documents().into_iter().find(|d| !before.contains(d));
        restored.is_some()
    })?;
    let restored = restored.ok_or("no restored window")?;
    s.pump(0.2);
    let rd = s.doc(restored).ok_or("restored document")?;
    expect(
        || format!("restored: {} objects, dirty {}", rd.objects.len(), rd.dirty),
        rd.objects.len() == 1 && rd.dirty,
    )?;

    // Verwerp removes the file.
    s.app.check_recovery();
    s.wait(5.0, "the second recovery alert", || s.app.recovery_alert().is_some())?;
    let discard = s.app.recovery_alert().and_then(|a| a.buttons().iter().nth(2)).ok_or("no discard button")?;
    // SAFETY: as above.
    unsafe { discard.performClick(None) };
    s.wait(5.0, "the recovery file to go", || files().is_empty())?;

    // A panic caught at a callback boundary asks for an autosave of what changed since.
    s.type_into_field(doc, burnburn_presentation::FieldKey::Width, "50")?;
    s.key(doc, "\r", 36)?;
    s.pump(0.2);
    #[allow(clippy::panic)]
    burnburn_macos::appkit::run_guarded("a test callback", || panic!("simulated callback panic"));
    s.wait(5.0, "a recovery file after the panic", || !files().is_empty())?;
    let _ = std::fs::remove_dir_all(&recovery);
    Ok(())
}
