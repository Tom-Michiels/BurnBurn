//! The image sheets natively (W2-L1-beeld): the bitmap import sheet with its DPI field, Adjust
//! Image with a live preview, and the tracer panel with its outline preview, on a real window with
//! the real controller. The file comes back the way NSOpenPanel's completion delivers it (its OK
//! button is in another process and cannot be pressed from a test: manual), and values are typed
//! the way a field's action delivers them.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("image_sheets: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("image_sheets_import_adjust_and_trace_on_a_real_window", scenario())
}

/// Type `text` into `field` and send its action, as Return does.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn type_into(field: &objc2_app_kit::NSTextField, text: &str) {
    field.setStringValue(&objc2_foundation::NSString::from_str(text));
    // SAFETY: the field's own action to its own target, as Return sends it.
    unsafe { field.sendAction_to(field.action(), field.target().as_deref()) };
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn scenario() -> Result<(), String> {
    use burnburn_macos::adapter::{MenuItem, NativeEvent};
    use burnburn_presentation::CommandKey;
    use common::expect;
    use objc2::Message;

    let s = common::start()?;
    let doc = s.new_document()?;
    let adjust_item = s.menu_item_state(MenuItem::Command(CommandKey::AdjustImage));
    expect(
        || format!("Beeld aanpassen… without an image {adjust_item:?}"),
        adjust_item == Some((false, "Beeld aanpassen…".to_owned())),
    )?;
    let trace_item = s.menu_item_state(MenuItem::TraceImage);
    expect(|| format!("Beeld tracen… {trace_item:?}"), trace_item == Some((true, "Beeld tracen…".to_owned())))?;

    // ---- Bitmap import: a 60 x 40 two-tone PNG without a resolution (254 dpi proposed).
    let pixels: Vec<u8> = (0..40).flat_map(|_| (0..60).map(|x| if x < 30 { 0u8 } else { 255 })).collect();
    let png = burnburn_image::encode_png_gray8(60, 40, &pixels).map_err(|e| format!("{e:?}"))?;
    let path = s.dir.join("twee-tinten.png");
    std::fs::write(&path, png).map_err(|e| e.to_string())?;
    s.app.native(NativeEvent::ImportFileChosen { document: doc, path: Some(path) });
    s.wait(5.0, "the bitmap import sheet", || {
        s.app
            .with_window(doc, |w| w.image_import_sheet.is_some() && w.window.attachedSheet().is_some())
            .unwrap_or(false)
    })?;
    s.pump(0.2);
    let (labels, dpi) = s
        .app
        .with_window(doc, |w| {
            (
                w.image_import_labels.iter().map(|l| l.stringValue().to_string()).collect::<Vec<_>>(),
                w.image_dpi.stringValue().to_string(),
            )
        })
        .unwrap_or_default();
    expect(|| format!("import labels {labels:?}"), labels[0] == "twee-tinten.png" && labels[1] == "60 × 40 pixels")?;
    // The controller leaves the field empty for a file without a resolution (its docs say 254):
    // either way Importeer follows the field, and typing 254 gives 0.1 mm per pixel.
    expect(|| format!("dpi {dpi:?}"), dpi.is_empty() || dpi == "254")?;
    s.wait_on_screen(doc)?;
    s.app.with_window(doc, |w| common::screenshot(w.window));
    let (field, commit) =
        s.app.with_window(doc, |w| (w.image_dpi.retain(), w.image_commit.retain())).ok_or("no sheet")?;
    expect(|| format!("Importeer follows the DPI field {dpi:?}"), commit.isEnabled() == !dpi.is_empty())?;
    type_into(&field, "254");
    s.pump(0.2);
    let size = s.app.with_window(doc, |w| w.image_import_labels[4].stringValue().to_string()).unwrap_or_default();
    expect(|| format!("size at 254 dpi {size:?}"), size == "Afmeting: 6 × 4 mm")?;
    type_into(&field, "abc");
    s.pump(0.2);
    let issue = s.app.with_window(doc, |w| w.image_import_labels[5].stringValue().to_string()).unwrap_or_default();
    expect(|| format!("issue {issue:?}"), issue == "Ongeldig getal (Resolutie)")?;
    expect(|| "Importeer disabled for an invalid DPI".into(), !commit.isEnabled())?;
    expect(|| "the typed text stays".into(), field.stringValue().to_string() == "abc")?;
    type_into(&field, "127");
    s.pump(0.2);
    let size = s.app.with_window(doc, |w| w.image_import_labels[4].stringValue().to_string()).unwrap_or_default();
    expect(|| format!("size at 127 dpi {size:?}"), size == "Afmeting: 12 × 8 mm")?;
    expect(|| "Importeer enabled again".into(), commit.isEnabled())?;
    // SAFETY: clicking our own button with no sender, as the user does.
    unsafe { commit.performClick(None) };
    s.wait(3.0, "the sheet to close", || {
        s.app.with_window(doc, |w| w.window.attachedSheet().is_none()).unwrap_or(false)
    })?;
    let objects = s.doc(doc).map(|d| d.objects.len()).unwrap_or(0);
    expect(|| format!("one imported image, got {objects}"), objects == 1)?;

    // ---- Adjust Image on the selected image.
    s.menu(MenuItem::Command(CommandKey::AdjustImage))?;
    s.wait(5.0, "the Adjust Image sheet", || {
        s.app.with_window(doc, |w| w.adjust_sheet.is_some() && w.window.attachedSheet().is_some()).unwrap_or(false)
    })?;
    s.wait(5.0, "the first preview", || {
        s.app.with_window(doc, |w| w.adjust_preview.image().is_some()).unwrap_or(false)
    })?;
    // W4-UI-machine: the same preview lies on the image's bounds on the canvas (a bitmap above the frame).
    let on_canvas = || -> Vec<(u32, u32)> {
        s.app
            .scene(doc)
            .map(|scene| {
                scene
                    .overlays
                    .iter()
                    .filter_map(|o| match o {
                        burnburn_editor::Overlay::ImagePreview { picture, .. } => Some((picture.width, picture.height)),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let drawn = on_canvas();
    expect(|| format!("the preview bitmap is in the canvas scene: {drawn:?}"), drawn.len() == 1 && drawn[0].0 > 0)?;
    let (source, gamma) = s
        .app
        .with_window(doc, |w| {
            (w.adjust_labels[1].stringValue().to_string(), w.adjust_fields[2].stringValue().to_string())
        })
        .unwrap_or_default();
    expect(|| format!("source {source:?}"), source == "Bron: 60 × 40 pixels")?;
    expect(|| format!("gamma {gamma:?}"), !gamma.is_empty())?;
    let (fields, apply) =
        s.app.with_window(doc, |w| (w.adjust_fields.map(|f| f.retain()), w.adjust_apply.retain())).ok_or("no sheet")?;
    type_into(&fields[2], "20");
    s.pump(0.2);
    let issue = s.app.with_window(doc, |w| w.adjust_labels[2].stringValue().to_string()).unwrap_or_default();
    expect(|| format!("gamma 20 is out of range: {issue:?}"), issue == "Waarde buiten bereik (Gamma)")?;
    expect(|| "the typed gamma stays".into(), fields[2].stringValue().to_string() == "20")?;
    type_into(&fields[2], "2");
    s.wait(5.0, "Pas toe", || apply.isEnabled())?;
    // SAFETY: clicking our own button with no sender, as the user does.
    unsafe { apply.performClick(None) };
    s.wait(3.0, "the sheet to close", || {
        s.app.with_window(doc, |w| w.window.attachedSheet().is_none()).unwrap_or(false)
    })?;
    let left = on_canvas();
    expect(|| format!("closing the sheet takes the preview off the canvas: {left:?}"), left.is_empty())?;

    // ---- Tracer on the same picture.
    let trace_path = s.dir.join("ring.png");
    let ring = format!("{}/../../test-data/trace/synthetic/ring.png", env!("CARGO_MANIFEST_DIR"));
    std::fs::copy(&ring, &trace_path).map_err(|e| format!("{ring}: {e}"))?;
    s.app.native(NativeEvent::TraceFileChosen { document: doc, path: Some(trace_path) });
    s.wait(5.0, "the tracer sheet", || {
        s.app.with_window(doc, |w| w.tracer_sheet.is_some() && w.window.attachedSheet().is_some()).unwrap_or(false)
    })?;
    // A picture without a resolution needs a scale first (T01): the hint says so, 300 dpi traces it.
    let hint = s.app.with_window(doc, |w| w.tracer_labels[3].stringValue().to_string()).unwrap_or_default();
    let dpi_field = s.app.with_window(doc, |w| w.tracer_fields[0].retain()).ok_or("no tracer")?;
    if dpi_field.stringValue().to_string().is_empty() {
        expect(
            || format!("scale hint {hint:?}"),
            hint == "Schaal ontbreekt: geef de resolutie (dpi) van het beeld op.",
        )?;
        type_into(&dpi_field, "300");
    }
    s.wait(5.0, "the outline preview", || {
        s.app.with_window(doc, |w| w.tracer_preview.image().is_some()).unwrap_or(false)
    })?;
    s.pump(0.3);
    s.app.with_window(doc, |w| common::screenshot(w.window));
    let summary = s.app.with_window(doc, |w| w.tracer_labels[1].stringValue().to_string()).unwrap_or_default();
    expect(|| format!("summary {summary:?}"), summary.starts_with("Delen: "))?;
    let (mode, recipe, close, fields) = s
        .app
        .with_window(doc, |w| {
            (
                w.tracer_mode.retain(),
                w.tracer_recipe.retain(),
                w.tracer_close.retain(),
                w.tracer_fields.map(|f| f.retain()),
            )
        })
        .ok_or("no tracer")?;
    expect(|| "negeer kleiner dan applies to contours".into(), fields[2].isEnabled())?;
    mode.selectItemAtIndex(1); // Middellijn
    // SAFETY: the pop-up's own action to its own target, as choosing an item sends it.
    unsafe { mode.sendAction_to(mode.action(), mode.target().as_deref()) };
    s.pump(0.3);
    expect(|| "negeer kleiner dan is greyed out for centre lines".into(), !fields[2].isEnabled())?;
    recipe.selectItemAtIndex(0); // Logo of silhouet: back to contours
    // SAFETY: as above.
    unsafe { recipe.sendAction_to(recipe.action(), recipe.target().as_deref()) };
    s.pump(0.3);
    expect(|| "the Logo recipe sets contours again".into(), fields[2].isEnabled() && mode.indexOfSelectedItem() == 0)?;
    // SAFETY: clicking our own button with no sender, as the user does.
    unsafe { close.performClick(None) };
    s.wait(3.0, "the tracer sheet to close", || {
        s.app.with_window(doc, |w| w.window.attachedSheet().is_none()).unwrap_or(false)
    })?;
    Ok(())
}
