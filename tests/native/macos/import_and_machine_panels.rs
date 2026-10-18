//! Poort C screens natively: the import dialog (a sheet on the document window) asks for the unit
//! of a unitless DXF before Importeer is possible; the inspector's machine section names the
//! machine state in words ("Geen machine verbonden" without a machine service).
//!
//! The file comes back the way NSOpenPanel's completion delivers it (its OK button is in another
//! process and cannot be pressed from a test: manual).

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("import_and_machine_panels: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("import_sheet_needs_a_unit_and_machine_section_names_the_state", scenario())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn scenario() -> Result<(), String> {
    use burnburn_macos::adapter::{MenuItem, NativeEvent};
    use common::expect;
    use objc2::Message;

    let s = common::start()?;
    let doc = s.new_document()?;
    let machine = s.app.with_window(doc, |w| w.machine_labels[0].stringValue().to_string()).unwrap_or_default();
    expect(|| format!("machine section {machine:?}"), machine == "Geen machine verbonden")?;
    let visible = s.app.with_window(doc, |w| w.machine_buttons.iter().filter(|b| !b.isHidden()).count()).unwrap_or(9);
    expect(|| format!("no machine controls without a machine, got {visible}"), visible == 0)?;

    let item = s.menu_item_state(MenuItem::Import);
    expect(|| format!("Archief › Importeer… {item:?}"), item == Some((true, "Importeer…".to_owned())))?;

    let src = format!("{}/../../test-data/import/unitless-lines.dxf", env!("CARGO_MANIFEST_DIR"));
    let path = s.dir.join("unitless-lines.dxf");
    std::fs::copy(&src, &path).map_err(|e| format!("{src}: {e}"))?;
    s.app.native(NativeEvent::ImportFileChosen { document: doc, path: Some(path) });
    s.wait(5.0, "the import sheet", || {
        s.app.with_window(doc, |w| w.import_sheet.is_some() && w.window.attachedSheet().is_some()).unwrap_or(false)
    })?;
    s.pump(0.2);
    let texts = s
        .app
        .with_window(doc, |w| w.import_labels.iter().map(|l| l.stringValue().to_string()).collect::<Vec<_>>())
        .unwrap_or_default();
    expect(|| format!("import texts {texts:?}"), texts.first().map(String::as_str) == Some("unitless-lines.dxf"))?;
    expect(
        || format!("unit prompt {texts:?}"),
        texts.get(1).map(String::as_str) == Some("Het bestand noemt geen eenheid: kies er een."),
    )?;
    s.wait_on_screen(doc)?;
    s.pump(0.3);
    s.app.with_window(doc, |w| common::screenshot(w.window));
    let commit = s.app.with_window(doc, |w| w.import_commit.retain()).ok_or("no Importeer button")?;
    expect(|| "Importeer disabled until a unit is chosen".into(), !commit.isEnabled())?;

    let popup = s.app.with_window(doc, |w| w.import_units.retain()).ok_or("no unit pop-up")?;
    popup.selectItemAtIndex(2); // inch
    // SAFETY: the pop-up's own action to its own target, as choosing an item sends it.
    unsafe { popup.sendAction_to(popup.action(), popup.target().as_deref()) };
    s.pump(0.2);
    let size = s.app.with_window(doc, |w| w.import_labels[2].stringValue().to_string()).unwrap_or_default();
    expect(|| format!("size {size:?}"), size == "Maat: 101.6 × 50.8 mm")?;
    expect(|| "Importeer enabled".into(), commit.isEnabled())?;
    // SAFETY: clicking our own button with no sender, as the user does.
    unsafe { commit.performClick(None) };
    s.wait(2.0, "the sheet to close", || {
        s.app.with_window(doc, |w| w.window.attachedSheet().is_none()).unwrap_or(false)
    })?;
    let objects = s.doc(doc).map(|d| d.objects.len()).unwrap_or(0);
    expect(|| format!("two imported lines, got {objects}"), objects == 2)?;
    Ok(())
}
