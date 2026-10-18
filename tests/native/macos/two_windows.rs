//! IM20 multiple windows: each document window keeps its own selection; the menu acts on the key
//! window's document only.
//!
//! A test process cannot activate itself over the user's app, so `makeKeyWindow` may not take;
//! then the window's own `NSWindowDidBecomeKeyNotification` is posted (the route AppKit uses to
//! reach the window delegate). Switching windows by clicking is a manual check.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("two_windows: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("two_windows_keep_their_selection_and_the_menu_acts_on_the_key_window", scenario())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn scenario() -> Result<(), String> {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_presentation::CommandKey;
    use common::expect;
    use objc2::Message;

    let s = common::start()?;
    let a = s.new_document()?;
    s.draw_rectangle(a, (0.0, 0.0), (40.0, 20.0))?;
    let b = s.new_document()?;
    s.draw_rectangle(b, (10.0, 10.0), (30.0, 30.0))?;
    expect(
        || "both drawings are selected in their window".into(),
        s.doc(a).is_some_and(|d| d.selection.len() == 1) && s.doc(b).is_some_and(|d| d.selection.len() == 1),
    )?;

    // Clearing the table selection in a changes a only.
    let table = s.app.with_window(a, |w| w.table.retain()).ok_or("no table")?;
    // SAFETY: deselecting our own table with no sender, as Cmd-click on the last row does.
    unsafe { table.deselectAll(None) };
    s.pump(0.1);
    expect(|| "a deselected".into(), s.doc(a).is_some_and(|d| d.selection.is_empty()))?;
    expect(|| "b keeps its selection".into(), s.doc(b).is_some_and(|d| d.selection.len() == 1))?;
    let rows = s.app.with_window(b, |w| w.table.selectedRow()).unwrap_or(-1);
    expect(|| format!("b's table row stays selected, got {rows}"), rows == 0)?;

    // Window b becomes key: the menu's Delete removes b's object, never a's.
    let window_b = s.app.window(b).ok_or("no window b")?;
    s.become_key(&window_b);
    s.wait(1.0, "b to become the active document", || s.app.view().active == Some(b))?;
    s.menu(MenuItem::Command(CommandKey::Delete))?;
    s.pump(0.1);
    expect(|| "b's object deleted".into(), s.doc(b).is_some_and(|d| d.objects.is_empty()))?;
    expect(|| "a untouched".into(), s.doc(a).is_some_and(|d| d.objects.len() == 1 && d.selection.is_empty()))?;

    // And back: a key, undo is a's (nothing to delete there, nothing selected).
    let window_a = s.app.window(a).ok_or("no window a")?;
    s.become_key(&window_a);
    s.wait(1.0, "a to become active", || s.app.view().active == Some(a))?;
    let delete = s.menu_item_state(MenuItem::Command(CommandKey::Delete)).map(|(e, _)| e);
    expect(|| format!("Delete disabled for a without selection, got {delete:?}"), delete == Some(false))?;
    let _ = window_a.retain();
    Ok(())
}
