//! IM20 / X01 offline: starting the app, drawing and saving open no network socket. Checked from
//! outside the process with `lsof` (internet sockets of this pid only).

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("offline_start: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("start_draw_and_save_open_no_network_socket", scenario())
}

#[cfg(target_os = "macos")]
fn scenario() -> Result<(), String> {
    use burnburn_presentation::UiIntent;
    use common::expect;

    let s = common::start()?;
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (0.0, 0.0), (40.0, 20.0))?;
    s.app.dispatch(UiIntent::Save { document: doc, path: Some(s.dir.join("offline.burnburn")) });
    s.wait(3.0, "the save", || s.doc(doc).is_some_and(|d| !d.dirty))?;
    s.pump(0.5);
    let pid = std::process::id().to_string();
    let out = std::process::Command::new("lsof")
        .args(["-nP", "-a", "-i", "-p", &pid])
        .output()
        .map_err(|e| format!("lsof: {e}"))?;
    let listing = String::from_utf8_lossy(&out.stdout).into_owned();
    // lsof exits 1 and prints nothing when the process has no internet sockets.
    expect(|| format!("internet sockets of pid {pid}:\n{listing}"), listing.trim().is_empty())?;
    Ok(())
}
