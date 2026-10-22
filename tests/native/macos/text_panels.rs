//! The text panels in the real window (W3-UI-tekst): the text toolbar follows the selected text and
//! lists the installed system fonts, its controls are driven like a user (clicks, typing, pop-up
//! choices) and end up in the document; the variable-text panel counts and bakes; the barcode sheet
//! lists the 29 types with the unsupported ones greyed out, encodes while typing and creates the
//! symbol; Tekst ▸ Tekst op pad… opens its sheet and bends the text along a circle.
//!
//! Manual (not covered here): how the panels look in light and dark, their position beside the
//! window, and the CSV file panel (the file sheet is AppKit's own; its result goes through the
//! tested `SlotValue::Csv` path).

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("text_panels: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("text_panels_work_in_the_real_window", scenario())
}

/// Writes the window's content view as a PNG into the folder named by `BURNBURN_NATIVE_VIEW_DIR`
/// (drawn by AppKit offscreen, so it needs no screen-recording permission); a no-op without it.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn snapshot(window: &objc2_app_kit::NSWindow, name: &str) {
    use objc2_app_kit::NSBitmapImageFileType;
    use objc2_foundation::NSDictionary;
    let Some(dir) = std::env::var_os("BURNBURN_NATIVE_VIEW_DIR") else { return };
    let Some(view) = window.contentView() else { return };
    let bounds = view.bounds();
    let Some(rep) = view.bitmapImageRepForCachingDisplayInRect(bounds) else { return };
    view.cacheDisplayInRect_toBitmapImageRep(bounds, &rep);
    // SAFETY: `representationUsingType:properties:` on our own bitmap with an empty property dictionary.
    let bytes = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new()) };
    if let Some(data) = bytes {
        let path = std::path::Path::new(&dir).join(format!("{name}.png"));
        let _ = std::fs::write(&path, data.to_vec());
        println!("    view snapshot {}", path.display());
    }
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn scenario() -> Result<(), String> {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_presentation::{CommandKey, SelectMode, UiIntent};
    use common::expect;
    use objc2::Message;
    use objc2_foundation::NSString;

    let s = common::start()?;
    let doc = s.new_document()?;
    let c = MenuItem::Command;
    let text_of = |s: &common::Session| s.doc(doc).and_then(|d| d.text_toolbar);

    // --- the toolbar follows the selected text
    expect(|| "no toolbar without text".into(), !s.app.with_text_widgets(doc, |p| p.toolbar_shown).unwrap_or(true))?;
    s.app.dispatch(UiIntent::InsertText { document: doc, text: "HIT".into() });
    s.pump(0.3);
    let bar = text_of(&s).ok_or("no toolbar for the new text")?;
    expect(|| format!("{} system font families", bar.families.len()), bar.families.len() > 5)?;
    expect(|| format!("family {:?}", bar.family), !bar.family.is_empty() && bar.families.contains(&bar.family))?;
    let shown = s.app.with_text_widgets(doc, |p| p.toolbar_shown).unwrap_or(false);
    expect(|| "the toolbar panel is on screen".into(), shown)?;
    let items = s.app.with_text_widgets(doc, |p| p.text.family.numberOfItems()).unwrap_or(0);
    expect(|| format!("{items} entries in the font pop-up"), items as usize == bar.families.len())?;
    let selected = s
        .app
        .with_text_widgets(doc, |p| p.text.family.titleOfSelectedItem().map(|t| t.to_string()))
        .flatten()
        .unwrap_or_default();
    expect(|| format!("selected family {selected:?}"), selected == bar.family)?;

    s.app.with_text_widgets(doc, |p| snapshot(p.text.panel, "toolbar"));

    // --- typing a height like a user; a bad value stays in the field with its reason
    let height = s.app.with_text_widgets(doc, |p| p.text.height.retain()).ok_or("no height field")?;
    height.setStringValue(&NSString::from_str("20"));
    // SAFETY: sending the field's own action as AppKit does after Return.
    unsafe { height.sendAction_to(height.action(), height.target().as_deref()) };
    s.pump(0.2);
    let bar = text_of(&s).ok_or("toolbar gone")?;
    expect(|| format!("height {:?}", bar.height), bar.height == "20")?;
    height.setStringValue(&NSString::from_str("veel"));
    // SAFETY: as above.
    unsafe { height.sendAction_to(height.action(), height.target().as_deref()) };
    s.pump(0.2);
    let (typed, note) = s
        .app
        .with_text_widgets(doc, |p| (p.text.height.stringValue().to_string(), p.text.note.stringValue().to_string()))
        .unwrap_or_default();
    expect(
        || format!("the refused text stays ({typed:?}) and says why ({note:?})"),
        typed == "veel" && !note.is_empty(),
    )?;
    expect(|| "the document kept 20".into(), text_of(&s).is_some_and(|b| b.height == "20"))?;

    // --- Uppercase and an alignment through real clicks and a segment choice
    let (upper, align) =
        s.app.with_text_widgets(doc, |p| (p.text.uppercase.retain(), p.text.align.retain())).ok_or("no widgets")?;
    // SAFETY: clicking our own checkbox.
    unsafe { upper.performClick(None) };
    s.pump(0.2);
    expect(|| "Uppercase is on".into(), text_of(&s).is_some_and(|b| b.uppercase))?;
    align.setSelectedSegment(1);
    // SAFETY: sending the control's own action.
    unsafe { align.sendAction_to(align.action(), align.target().as_deref()) };
    s.pump(0.2);
    expect(|| "centred".into(), text_of(&s).is_some_and(|b| b.align == burnburn_presentation::Align::Center))?;

    // --- Convert to Path through the Tekst menu and the toolbar button; undo brings the text back
    let convert = s.app.with_text_widgets(doc, |p| p.text.convert.retain()).ok_or("no convert button")?;
    // SAFETY: clicking our own button.
    unsafe { convert.performClick(None) };
    s.pump(0.3);
    let after = s.doc(doc).ok_or("no document")?;
    expect(
        || format!("objects {} toolbar {:?}", after.objects.len(), after.text_toolbar.is_some()),
        !after.objects.is_empty() && after.text_toolbar.is_none(),
    )?;
    let shown = s.app.with_text_widgets(doc, |p| p.toolbar_shown).unwrap_or(true);
    expect(|| "the toolbar panel is gone".into(), !shown)?;
    s.menu(c(CommandKey::Undo))?;
    s.pump(0.3);
    expect(|| "undo brings the text and its toolbar back".into(), text_of(&s).is_some())?;

    // --- the variable-text panel: Variable, a series, Bake (a committed text extends the selected one: deselect)
    s.app.dispatch(UiIntent::Select { document: doc, ids: vec![], mode: SelectMode::Replace });
    s.app.dispatch(UiIntent::InsertText { document: doc, text: "SN-{serial}".into() });
    s.pump(0.2);
    let kind = s.app.with_text_widgets(doc, |p| p.variable.kind.retain()).ok_or("no kind control")?;
    kind.setSelectedSegment(1);
    // SAFETY: sending the control's own action.
    unsafe { kind.sendAction_to(kind.action(), kind.target().as_deref()) };
    s.pump(0.2);
    let panel = s.doc(doc).and_then(|d| d.variable_panel).ok_or("no variable panel")?;
    expect(|| format!("variable {:?} preview {:?}", panel.variable, panel.preview), panel.variable)?;
    for (field, text) in [("start", "10"), ("end", "95"), ("by", "5")] {
        let f = s
            .app
            .with_text_widgets(doc, |p| match field {
                "start" => p.variable.start.retain(),
                "end" => p.variable.end.retain(),
                _ => p.variable.advance_by.retain(),
            })
            .ok_or("no serial field")?;
        f.setStringValue(&NSString::from_str(text));
        // SAFETY: as above.
        unsafe { f.sendAction_to(f.action(), f.target().as_deref()) };
        s.pump(0.1);
    }
    let shown = s.app.with_text_widgets(doc, |p| p.variable_shown).unwrap_or(false);
    expect(|| "the variable panel is on screen".into(), shown)?;
    s.app.with_text_widgets(doc, |p| snapshot(p.variable.panel, "variable"));
    let preview = s.app.with_text_widgets(doc, |p| p.variable.preview.stringValue().to_string()).unwrap_or_default();
    expect(|| format!("preview {preview:?}"), preview == "SN-10")?;
    // The outlines still show the template until the text is baked; baking draws the resolved text.
    let width = |s: &common::Session| -> f64 {
        s.doc(doc)
            .and_then(|d| d.inspector)
            .and_then(|i| {
                i.fields.iter().find(|f| f.key == burnburn_presentation::FieldKey::Width).map(|f| f.text.clone())
            })
            .and_then(|t| t.parse().ok())
            .unwrap_or(0.0)
    };
    let template_width = width(&s);
    let bake = s.app.with_text_widgets(doc, |p| p.variable.bake.retain()).ok_or("no bake button")?;
    // SAFETY: clicking our own button.
    unsafe { bake.performClick(None) };
    s.pump(0.3);
    let baked = s.doc(doc).ok_or("no document")?;
    let baked_width = width(&s);
    expect(
        || {
            format!(
                "baked: variable {:?}, width {template_width} -> {baked_width}",
                baked.variable_panel.as_ref().map(|p| p.variable)
            )
        },
        baked.variable_panel.as_ref().is_some_and(|p| !p.variable) && baked_width > 0.0 && baked_width < template_width,
    )?;

    // --- the barcode sheet
    s.app.dispatch(UiIntent::Select { document: doc, ids: vec![], mode: SelectMode::Replace });
    s.menu(c(CommandKey::BarcodeNew))?;
    s.pump(0.3);
    let up = s.app.with_text_widgets(doc, |p| p.barcode_shown).unwrap_or(false);
    expect(|| "the barcode sheet is open".into(), up)?;
    let (items, grey) = s
        .app
        .with_text_widgets(doc, |p| {
            let n = p.barcode.types.numberOfItems();
            let grey: Vec<(String, String)> = (0..n)
                .filter_map(|i| p.barcode.types.itemAtIndex(i))
                .filter(|it| !it.isEnabled())
                .map(|it| (it.title().to_string(), it.toolTip().map(|t| t.to_string()).unwrap_or_default()))
                .collect();
            (n, grey)
        })
        .unwrap_or_default();
    expect(|| format!("{items} types"), items == 29)?;
    expect(
        || format!("greyed: {:?}", grey.iter().map(|g| g.0.clone()).collect::<Vec<_>>()),
        grey.len() == 2 && grey.iter().all(|(_, tip)| !tip.is_empty()),
    )?;
    let payload = s.app.with_text_widgets(doc, |p| p.barcode.payload.retain()).ok_or("no payload field")?;
    payload.setStringValue(&NSString::from_str("BURNBURN"));
    // SAFETY: sending the field's own action as AppKit does after Return.
    unsafe { payload.sendAction_to(payload.action(), payload.target().as_deref()) };
    s.pump(0.2);
    let (size, create_on) = s
        .app
        .with_text_widgets(doc, |p| (p.barcode.size_line.stringValue().to_string(), p.barcode.create.isEnabled()))
        .unwrap_or_default();
    expect(|| format!("size line {size:?}"), size.contains("Afmeting") && create_on)?;
    // A module below the minimum: the warning shows with both numbers and Maak is off.
    let module = s.app.with_text_widgets(doc, |p| p.barcode.module.retain()).ok_or("no module field")?;
    module.setStringValue(&NSString::from_str("0,05"));
    // SAFETY: as above.
    unsafe { module.sendAction_to(module.action(), module.target().as_deref()) };
    s.pump(0.2);
    let (warning, create_on) = s
        .app
        .with_text_widgets(doc, |p| (p.barcode.readability.stringValue().to_string(), p.barcode.create.isEnabled()))
        .unwrap_or_default();
    expect(|| format!("readability {warning:?}"), warning.contains("0.05") && warning.contains("0.2") && !create_on)?;
    module.setStringValue(&NSString::from_str("0,5"));
    // SAFETY: as above.
    unsafe { module.sendAction_to(module.action(), module.target().as_deref()) };
    s.pump(0.2);
    s.app.with_text_widgets(doc, |p| snapshot(p.barcode.sheet, "barcode"));
    let objects_before = s.doc(doc).ok_or("no document")?.objects.len();
    let create = s.app.with_text_widgets(doc, |p| p.barcode.create.retain()).ok_or("no create button")?;
    // SAFETY: clicking our own button.
    unsafe { create.performClick(None) };
    s.pump(0.3);
    let after = s.doc(doc).ok_or("no document")?;
    expect(
        || format!("{} objects, last {:?}", after.objects.len(), after.objects.last().map(|o| o.label.clone())),
        after.objects.len() == objects_before + 1
            && after.objects.last().is_some_and(|o| o.label == "QR Code: BURNBURN"),
    )?;
    let closed = s.app.with_text_widgets(doc, |p| !p.barcode_shown).unwrap_or(false);
    expect(|| "the sheet is closed".into(), closed)?;

    // --- text on a path: a text and a circle, the Tekst menu, the sheet, Warp, Apply
    s.app.dispatch(UiIntent::Select { document: doc, ids: vec![], mode: SelectMode::Replace });
    s.app.dispatch(UiIntent::InsertText { document: doc, text: "RONDOM".into() });
    s.pump(0.2);
    let text_ids: Vec<_> = s.doc(doc).ok_or("no document")?.selection.clone();
    s.app.dispatch(UiIntent::Invoke { document: Some(doc), command: CommandKey::ToolCircle });
    s.drag_mm(doc, (100.0, 100.0), (140.0, 100.0))?;
    s.menu(c(CommandKey::ToolSelect))?;
    let circle = s.doc(doc).ok_or("no document")?.selection.clone();
    let mut both = text_ids;
    both.extend(circle);
    s.app.dispatch(UiIntent::Select { document: doc, ids: both, mode: SelectMode::Replace });
    s.pump(0.2);
    s.menu(c(CommandKey::TextOnPath))?;
    s.pump(0.3);
    let sheet_up = s.app.with_text_widgets(doc, |p| p.path_sheet_shown).unwrap_or(false);
    expect(|| "the text-on-path sheet is open".into(), sheet_up)?;
    let distort = s.app.with_text_widgets(doc, |p| p.text.path_distort.retain()).ok_or("no distort pop-up")?;
    distort.selectItemAtIndex(1);
    // SAFETY: sending the control's own action.
    unsafe { distort.sendAction_to(distort.action(), distort.target().as_deref()) };
    s.pump(0.2);
    expect(
        || "Warp is chosen in the dialog".into(),
        s.doc(doc).and_then(|d| d.text_on_path).is_some_and(|d| d.distort == burnburn_presentation::Distort::Warp),
    )?;
    let live = s
        .app
        .with_text_widgets(doc, |p| p.text.path_live.state() == objc2_app_kit::NSControlStateValueOn)
        .unwrap_or(false);
    expect(|| "the sheet keeps the text following the path by default".into(), live)?;
    let count = s.doc(doc).ok_or("no document")?.objects.len();
    let apply = s.app.with_text_widgets(doc, |p| p.text.path_apply.retain()).ok_or("no apply button")?;
    // SAFETY: clicking our own button.
    unsafe { apply.performClick(None) };
    s.pump(0.3);
    let after = s.doc(doc).ok_or("no document")?;
    expect(
        || format!("{} objects (was {count}), dialog {:?}", after.objects.len(), after.text_on_path),
        after.objects.len() == count && after.text_on_path.is_none(),
    )?;
    let closed = s.app.with_text_widgets(doc, |p| !p.path_sheet_shown).unwrap_or(false);
    expect(|| "the sheet is closed".into(), closed)?;

    s.app.with_window(doc, |w| common::screenshot(w.window));
    Ok(())
}
