//! The file panel of the tracer's batch, natively (G8-trace, T10): the "Choose files…" button opens a
//! real NSOpenPanel sheet that allows multiple selection of pictures; the files it returns (the panel
//! runs in another process, its OK button cannot be pressed from a test: manual) start one batch with
//! a report line per file.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("trace_batch_panel: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("trace_batch_panel_allows_multiple_selection_and_starts_one_batch", scenario())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn scenario() -> Result<(), String> {
    use burnburn_macos::adapter::NativeEvent;
    use burnburn_macos::panels::{G5Control, G5Panel};
    use common::expect;

    let s = common::start()?;
    let doc = s.new_document()?;

    // The panel itself, not shown: files only, several of them, the pictures the tracer reads.
    let panel = burnburn_macos::appkit::trace_batch_open_panel(s.mtm);
    expect(|| "multiple selection".into(), panel.allowsMultipleSelection())?;
    expect(|| "files can be chosen".into(), panel.canChooseFiles())?;
    expect(|| "folders cannot".into(), !panel.canChooseDirectories())?;
    #[allow(deprecated)]
    let types: Vec<String> =
        panel.allowedFileTypes().map(|t| t.iter().map(|s| s.to_string()).collect()).unwrap_or_default();
    expect(|| format!("allowed types {types:?}"), ["png", "jpg", "jpeg"].iter().all(|t| types.iter().any(|x| x == t)))?;

    // Two pictures and a tracer on the first.
    let pixels: Vec<u8> = (0..40).flat_map(|_| (0..60).map(|x| if x < 30 { 0u8 } else { 255 })).collect();
    let png = burnburn_image::encode_png_gray8(60, 40, &pixels).map_err(|e| format!("{e:?}"))?;
    let one = s.dir.join("een.png");
    let two = s.dir.join("twee.png");
    std::fs::write(&one, &png).map_err(|e| e.to_string())?;
    std::fs::write(&two, &png).map_err(|e| e.to_string())?;
    s.app.native(NativeEvent::TraceFileChosen { document: doc, path: Some(one.clone()) });
    s.wait(10.0, "the tracer", || s.doc(doc).is_some_and(|d| d.tracer.is_some()))?;

    // The button opens a sheet that is an open panel with multiple selection.
    s.app.native(NativeEvent::G5Control {
        document: doc,
        panel: G5Panel::TracePrep,
        control: G5Control::BatchChoose,
        values: Vec::new(),
    });
    s.wait(5.0, "the file sheet", || s.app.with_window(doc, |w| w.window.attachedSheet().is_some()).unwrap_or(false))?;
    let shown = s.app.trace_batch_panel();
    expect(|| "the host keeps the batch panel while it is open".into(), shown.is_some())?;
    let multiple = shown.as_ref().map(|p| p.allowsMultipleSelection());
    expect(|| format!("the open panel allows multiple selection: {multiple:?}"), multiple == Some(true))?;
    // Cancel it the way the panel's Cancel does: the completion sends no files and nothing starts.
    s.app.with_window(doc, |w| {
        if let Some(sheet) = w.window.attachedSheet() {
            w.window.endSheet_returnCode(&sheet, objc2_app_kit::NSModalResponseCancel);
        }
    });
    s.wait(5.0, "the sheet to close", || {
        s.app.with_window(doc, |w| w.window.attachedSheet().is_none()).unwrap_or(false)
    })?;
    s.pump(0.3);
    let started = s.doc(doc).and_then(|d| d.trace_recipes).is_some_and(|r| r.batch.is_some());
    expect(|| "cancelling starts no batch".into(), !started)?;

    // The files the panel returns start one batch with a line per file, in the order given.
    s.app.native(NativeEvent::TraceBatchFilesChosen { document: doc, paths: vec![two, one] });
    s.wait(15.0, "the batch report", || {
        s.doc(doc).and_then(|d| d.trace_recipes).and_then(|r| r.batch).is_some_and(|b| !b.running && b.rows.len() == 2)
    })?;
    let names: Vec<String> = s
        .doc(doc)
        .and_then(|d| d.trace_recipes)
        .and_then(|r| r.batch)
        .map(|b| b.rows.into_iter().map(|r| r.name).collect())
        .unwrap_or_default();
    expect(|| format!("report order {names:?}"), names == ["twee.png", "een.png"])?;
    Ok(())
}
