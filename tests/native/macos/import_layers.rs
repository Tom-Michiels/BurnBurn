//! The import sheet's check for the layers a plotter file proposes, natively (G7-trace, P04): on a
//! real window with the real controller. The check is hidden for a file without a proposal, shown
//! for a plotter file, and clicking it makes Importeer create one operation per pen. The file panel
//! itself runs in another process; its completion is driven through the intent it sends.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("import_layers: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("import_sheet_accepts_the_proposed_layers", scenario())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn scenario() -> Result<(), String> {
    use burnburn_macos::adapter::NativeEvent;
    use common::expect;
    use objc2::Message;

    let s = common::start()?;
    let doc = s.new_document()?;

    // A DXF has layers of its own: no proposal, no check.
    let dxf = s.dir.join("leeg.dxf");
    std::fs::write(&dxf, "0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n").map_err(|e| e.to_string())?;

    // A plotter file with two pens: a square with pen 1 and a circle with pen 2.
    let plt = s.dir.join("pennen.plt");
    std::fs::write(&plt, "IN;SP1;PU0,0;PD400,0,400,400,0,400,0,0;PU;SP2;PU200,200;CI100;PU;SP0;")
        .map_err(|e| e.to_string())?;
    s.app.native(NativeEvent::ImportFileChosen { document: doc, path: Some(plt) });
    s.wait(5.0, "the import sheet", || {
        s.app.with_window(doc, |w| w.import_sheet.is_some() && w.window.attachedSheet().is_some()).unwrap_or(false)
    })?;
    s.pump(0.3);
    let (layers, commit) =
        s.app.with_window(doc, |w| (w.import_layers.retain(), w.import_commit.retain())).ok_or("no sheet")?;
    expect(|| "the check is shown for a plotter file".into(), !layers.isHidden())?;
    expect(|| "and it starts unchecked".into(), layers.state() == objc2_app_kit::NSControlStateValueOff)?;
    let title = layers.title().to_string();
    expect(|| format!("check title {title:?}"), title == "Maak voor elke van deze lagen een bewerking aan")?;

    // SAFETY: clicking our own checkbox as the user does.
    unsafe { layers.performClick(None) };
    s.wait(3.0, "the acceptance to reach presentation", || {
        s.app.with_window(doc, |w| w.import_layers.state() == objc2_app_kit::NSControlStateValueOn).unwrap_or(false)
            && s.doc(doc).and_then(|d| d.import.map(|i| i.layers_accepted)).unwrap_or(false)
    })?;
    // SAFETY: clicking our own button as the user does.
    unsafe { commit.performClick(None) };
    s.wait(3.0, "the sheet to close", || {
        s.app.with_window(doc, |w| w.window.attachedSheet().is_none()).unwrap_or(false)
    })?;
    let operations = s.doc(doc).map(|d| d.operations.len()).unwrap_or(0);
    expect(|| format!("one operation per pen, got {operations}"), operations == 2)?;

    // The DXF afterwards: no proposal, so no check.
    s.app.native(NativeEvent::ImportFileChosen { document: doc, path: Some(dxf) });
    s.wait(5.0, "the second import sheet", || {
        s.app.with_window(doc, |w| w.import_sheet.is_some() && w.window.attachedSheet().is_some()).unwrap_or(false)
    })?;
    s.pump(0.3);
    let hidden = s.app.with_window(doc, |w| w.import_layers.isHidden()).ok_or("no sheet")?;
    expect(|| "no check for a DXF".into(), hidden)?;
    Ok(())
}
