//! G5-grbl-ui in the real AppKit app, against a fake GRBL controller behind the serial-port trait:
//! Device Settings picks the GRBL backend, the plan shows what the backend refuses (a pull-down
//! whose rows select the object), Archief ▸ Exporteer G-code opens its save sheet, and Laser ▸
//! Laservenster opens the GRBL window: status read through the timer, console with history on the
//! arrow keys, a macro that is saved, run line by line and kept in a file, jog, and a job start
//! that stays off with the missing evidence as its tooltip.
//!
//! No real serial device and no real laser: the port is an in-process fake. Hardware behaviour is
//! unverified (`docs/hardware/evidence.md`).
//!
//! Manual (not covered here): how the window looks (set `BURNBURN_NATIVE_SCREENSHOT` for a window
//! capture), typing with a real keyboard (the arrow keys are sent to the field editor as the
//! commands AppKit turns them into), and the OK button of the save sheet.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;
#[cfg(target_os = "macos")]
#[path = "common/grbl_fake.rs"]
mod grbl_fake;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("grbl_window: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("grbl_window_end_to_end_against_a_fake_controller", scenario())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn scenario() -> Result<(), String> {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_presentation::{CamDialog, CommandKey, DialogActionKind, DialogField, SelectMode, UiIntent};
    use common::expect;
    use grbl_fake::FakeGrbl;
    use objc2::rc::Retained;
    use objc2::{Message, msg_send};
    use objc2_app_kit::{NSButton, NSControl, NSPopUpButton, NSStandardKeyBindingResponding};
    use objc2_foundation::NSString;

    let temp = std::env::temp_dir().join(format!("burnburn-native-{}", std::process::id()));
    let recent = temp.join("recent-projects.txt");
    let s = common::start_with(|o| o.recent_projects_file = Some(recent))?;
    let doc = s.new_document()?;
    let fake = FakeGrbl::new();

    // ---- Device Settings: the GRBL backend with air assist on.
    let dev = CamDialog::Device;
    s.menu(MenuItem::CamDialog(dev))?;
    s.pump(0.3);
    let row = |field: DialogField| -> Option<Retained<NSControl>> {
        s.app.with_cam_panel(doc, dev, |p| p.rows.iter().find(|r| r.0 == field).map(|r| r.1.retain())).flatten()
    };
    let choose = |c: &NSControl, index: isize| {
        let object: &objc2::runtime::AnyObject = c.as_ref();
        if let Some(popup) = object.downcast_ref::<NSPopUpButton>() {
            popup.selectItemAtIndex(index);
        }
        // SAFETY: the control's own action to its own target, as a click sends it.
        let _: bool = unsafe { msg_send![c, sendAction: c.action(), to: c.target().as_deref()] };
    };
    let backend = row(DialogField::Backend).ok_or("the dialog has no machine type row")?;
    choose(&backend, 1);
    s.pump(0.1);
    let air = row(DialogField::GrblAirAssist).ok_or("no air assist row")?;
    expect(|| "the GRBL rows follow the choice".into(), air.isEnabled())?;
    air.setIntegerValue(1);
    // SAFETY: as above.
    let _: bool = unsafe { msg_send![&*air, sendAction: air.action(), to: air.target().as_deref()] };
    let apply: Retained<NSButton> = s
        .app
        .with_cam_panel(doc, dev, |p| p.buttons.iter().find(|b| b.0 == DialogActionKind::Apply).map(|b| b.1.retain()))
        .flatten()
        .ok_or("no Apply button")?;
    // SAFETY: clicking our own button with no sender, as the user does.
    unsafe { apply.performClick(None) };
    s.pump(0.2);
    s.menu(MenuItem::CamDialog(dev))?;
    s.pump(0.3);
    let shown = row(DialogField::Backend).map(|c| c.integerValue());
    expect(|| format!("the profile keeps the GRBL backend: {shown:?}"), shown == Some(1))?;
    let close: Retained<NSButton> = s
        .app
        .with_cam_panel(doc, dev, |p| p.buttons.iter().find(|b| b.0 == DialogActionKind::Close).map(|b| b.1.retain()))
        .flatten()
        .ok_or("no Close button")?;
    // SAFETY: as above.
    unsafe { close.performClick(None) };

    // ---- A plan: the backend accepts it, then refuses its air assist.
    s.app.attach_grbl_port(Box::new(fake.clone()));
    s.draw_rectangle(doc, (10.0, 10.0), (50.0, 30.0))?;
    let object = s.doc(doc).and_then(|d| d.objects.first().map(|o| o.id)).ok_or("no object")?;
    s.app.dispatch(UiIntent::Select { document: doc, ids: vec![object], mode: SelectMode::Replace });
    s.app.dispatch(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    s.menu(MenuItem::Command(CommandKey::Compile))?;
    s.wait(5.0, "the compiled plan", || s.doc(doc).is_some_and(|d| d.plan.as_ref().is_some_and(|p| !p.stale)))?;
    s.pump(0.2);
    let plan = s.doc(doc).and_then(|d| d.plan).ok_or("no plan")?;
    expect(|| format!("a clean GRBL plan: {} errors, {:?}", plan.errors, plan.grbl), plan.errors == 0)?;
    let hidden = s.app.with_window(doc, |w| w.grbl_refusals.isHidden()).unwrap_or(false);
    expect(|| "no GRBL refusals are listed".into(), hidden)?;

    // Export G-code: the item is on, the sheet opens; cancelling writes nothing.
    let (on, title) =
        s.menu_item_state(MenuItem::Command(CommandKey::ExportGcode)).ok_or("no Exporteer G-code item")?;
    expect(|| format!("menu: enabled {on}, title {title:?}"), on && title == "Exporteer G-code…")?;
    s.menu(MenuItem::Command(CommandKey::ExportGcode))?;
    s.wait(2.0, "the G-code sheet", || {
        s.app.with_window(doc, |w| w.export_panel.is_some() && w.window.attachedSheet().is_some()).unwrap_or(false)
    })?;
    let panel = s.app.with_window(doc, |w| w.export_panel.map(|p| p.retain())).flatten().ok_or("no panel")?;
    let suggested = panel.nameFieldStringValue().to_string();
    expect(|| format!("suggested name {suggested:?}"), suggested.ends_with(".gcode"))?;
    // SAFETY: the panel's own cancel action, as the Cancel button sends it.
    unsafe { panel.cancel(None) };
    s.wait(2.0, "the sheet to close", || {
        s.app.with_window(doc, |w| w.window.attachedSheet().is_none()).unwrap_or(false)
    })?;
    // The machine in use is kept in the user's folder (M01); that is not an export.
    let files = std::fs::read_dir(&s.dir)
        .map(|d| d.filter_map(Result::ok).filter(|e| e.file_name() != "active-machine.json").count())
        .unwrap_or(0);
    expect(|| format!("nothing exported, found {files} files"), files == 0)?;

    // Air assist off: a new profile revision, and the plan names what the backend refuses.
    s.menu(MenuItem::CamDialog(dev))?;
    s.pump(0.3);
    let air = row(DialogField::GrblAirAssist).ok_or("no air assist row")?;
    air.setIntegerValue(0);
    // SAFETY: as above.
    let _: bool = unsafe { msg_send![&*air, sendAction: air.action(), to: air.target().as_deref()] };
    let apply: Retained<NSButton> = s
        .app
        .with_cam_panel(doc, dev, |p| p.buttons.iter().find(|b| b.0 == DialogActionKind::Apply).map(|b| b.1.retain()))
        .flatten()
        .ok_or("no Apply button")?;
    // SAFETY: as above.
    unsafe { apply.performClick(None) };
    s.pump(0.2);
    s.menu(MenuItem::Command(CommandKey::Compile))?;
    s.wait(5.0, "the plan for the new profile", || {
        s.doc(doc).is_some_and(|d| d.plan.as_ref().is_some_and(|p| !p.stale && !p.grbl.is_empty()))
    })?;
    s.pump(0.2);
    let (hidden, titles) = s
        .app
        .with_window(doc, |w| {
            (w.grbl_refusals.isHidden(), w.grbl_refusals.itemTitles().iter().map(|t| t.to_string()).collect::<Vec<_>>())
        })
        .ok_or("no window")?;
    expect(|| format!("refusals shown: hidden {hidden}, items {titles:?}"), !hidden && titles.len() >= 2)?;
    expect(|| format!("the reason is in words: {titles:?}"), titles[1].contains("M8"))?;
    expect(|| format!("the heading counts them: {titles:?}"), titles[0].contains("GRBL") && titles[0].contains('1'))?;
    s.app.dispatch(UiIntent::Select { document: doc, ids: Vec::new(), mode: SelectMode::Replace });
    let popup = s.app.with_window(doc, |w| w.grbl_refusals.retain()).ok_or("no window")?;
    popup.selectItemAtIndex(1);
    // SAFETY: the control's own action to its own target, as choosing the row sends it.
    let _: bool = unsafe { msg_send![&*popup, sendAction: popup.action(), to: popup.target().as_deref()] };
    s.pump(0.1);
    let selected = s.doc(doc).map(|d| d.selection).unwrap_or_default();
    expect(|| format!("choosing the refusal selects its object: {selected:?}"), selected == vec![object])?;

    // ---- Laser ▸ Laservenster is the GRBL window on a GRBL machine.
    s.menu(MenuItem::LaserWindow)?;
    s.pump(0.2);
    expect(|| "the GRBL window is shown".into(), s.app.grbl_window_shown())?;
    expect(|| "the Ruida laser panel stays off".into(), !s.app.laser_panel_shown(doc))?;
    let title = s.app.with_grbl_window(|p| p.panel.title().to_string()).unwrap_or_default();
    expect(|| format!("window title {title:?}"), title == "GRBL-laservenster")?;
    // The timer reads the controller; until then the status is unknown, never idle.
    s.wait(5.0, "the status through the timer", || {
        s.app.with_grbl_window(|p| p.status.stringValue().to_string().contains("Rust")).unwrap_or(false)
    })?;
    let start = s
        .app
        .with_grbl_window(|p| {
            p.buttons
                .iter()
                .find(|b| b.0 == burnburn_presentation::GrblControlKind::StartJob)
                .map(|b| (b.1.isEnabled(), b.1.toolTip().map(|t| t.to_string()).unwrap_or_default()))
        })
        .flatten()
        .ok_or("no start button")?;
    expect(|| format!("start job: {start:?}"), !start.0 && start.1.contains("GRBL"))?;
    let notes = s.app.with_grbl_window(|p| p.notes.stringValue().to_string()).unwrap_or_default();
    expect(|| format!("the unverified note is there: {notes:?}"), notes.contains("nagebootste"))?;

    // ---- Console: a line, its answer, and the history on the arrow keys.
    let input = s.app.with_grbl_window(|p| p.input.retain()).ok_or("no window")?;
    let send = s.app.with_grbl_window(|p| p.send.retain()).ok_or("no window")?;
    input.setStringValue(&NSString::from_str("G0 X5"));
    // SAFETY: clicking our own button with no sender, as the user does.
    unsafe { send.performClick(None) };
    s.wait(5.0, "the answer in the transcript", || {
        s.app.with_grbl_window(|p| p.transcript.string().to_string().contains("< ok")).unwrap_or(false)
    })?;
    expect(|| format!("the controller got the line: {:?}", fake.received()), fake.received() == ["G0 X5"])?;
    let transcript = s.app.with_grbl_window(|p| p.transcript.string().to_string()).unwrap_or_default();
    expect(|| format!("transcript {transcript:?}"), transcript.lines().any(|l| l == "> G0 X5"))?;
    expect(|| "a sent line empties the input".into(), input.stringValue().to_string().is_empty())?;
    let window = s.app.with_grbl_window(|p| p.panel.retain()).ok_or("no window")?;
    s.become_key(&window);
    window.makeFirstResponder(Some(&input));
    // SAFETY: the window's own field editor for our field; the commands are what AppKit's key
    // bindings send an editing field for the arrow keys.
    let editor = unsafe { window.fieldEditor_forObject(true, Some(&input)) }.ok_or("no field editor")?;
    // SAFETY: as above.
    unsafe { editor.doCommandBySelector(objc2::sel!(moveUp:)) };
    s.pump(0.1);
    let recalled = input.stringValue().to_string();
    expect(|| format!("up arrow recalls the last command: {recalled:?}"), recalled == "G0 X5")?;
    // SAFETY: as above.
    unsafe { editor.doCommandBySelector(objc2::sel!(moveDown:)) };
    s.pump(0.1);
    expect(|| "down arrow returns to the empty line".into(), input.stringValue().to_string().is_empty())?;

    // ---- Macros: saved, listed, run line by line, kept in a file.
    let (name, program, save, run, table) = s
        .app
        .with_grbl_window(|p| {
            (
                p.macro_name.retain(),
                p.macro_program.retain(),
                p.macro_buttons[1].retain(),
                p.macro_buttons[0].retain(),
                p.macro_table.retain(),
            )
        })
        .ok_or("no window")?;
    name.setStringValue(&NSString::from_str("Nulpunt"));
    program.setString(&NSString::from_str("G92 X0 Y0 ; zero\nG0 X1"));
    // SAFETY: as above.
    unsafe { save.performClick(None) };
    s.pump(0.2);
    let rows = table.numberOfRows();
    expect(|| format!("one macro in the list, got {rows}"), rows == 1)?;
    table.selectRowIndexes_byExtendingSelection(&objc2_foundation::NSIndexSet::indexSetWithIndex(0), false);
    s.pump(0.2);
    expect(|| "Voer uit is on for a selected macro".into(), run.isEnabled())?;
    // SAFETY: as above.
    unsafe { run.performClick(None) };
    s.wait(5.0, "the macro to finish", || fake.received().len() == 3)?;
    expect(
        || format!("the macro lines in order after the console line: {:?}", fake.received()),
        fake.received() == ["G0 X5", "G92 X0 Y0", "G0 X1"],
    )?;
    let file = std::fs::read_to_string(s.dir.join("grbl-macros.txt")).unwrap_or_default();
    expect(|| format!("the macro file: {file:?}"), file.contains("@macro Nulpunt") && file.contains("G92 X0 Y0"))?;

    // ---- Jog: one relative move of the typed step.
    let left = s
        .app
        .with_grbl_window(|p| {
            p.buttons
                .iter()
                .find(|b| {
                    b.0 == burnburn_presentation::GrblControlKind::Jog(burnburn_presentation::StepDirection::Left)
                })
                .map(|b| b.1.retain())
        })
        .flatten()
        .ok_or("no jog button")?;
    s.wait(5.0, "jog left to come on (the controller is idle again)", || left.isEnabled())?;
    // SAFETY: as above.
    unsafe { left.performClick(None) };
    s.pump(0.2);
    let jog = fake.received().last().cloned().unwrap_or_default();
    expect(|| format!("jog line {jog:?}"), jog.starts_with("$J=") && jog.contains("G91") && jog.contains("X-1"))?;

    common::screenshot(&window);
    Ok(())
}
