//! LFC: the layer table is view-based. The layer column shows a 12 pt circle in the layer's colour
//! (drawn, no coloured cell) followed by the layer's name; the editable columns hold text fields
//! and Output / Show are check boxes whose actions reach presentation like the cell edits did.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("layer_cells: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("the_layer_table_cells_are_views_with_a_drawn_circle", scenario())
}

#[cfg(target_os = "macos")]
fn find<T: objc2::DowncastTarget>(view: &objc2_app_kit::NSView) -> Option<objc2::rc::Retained<T>> {
    for sub in view.subviews().iter() {
        if let Ok(found) = sub.clone().downcast::<T>() {
            return Some(found);
        }
        if let Some(found) = find::<T>(&sub) {
            return Some(found);
        }
    }
    None
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn scenario() -> Result<(), String> {
    use burnburn_presentation::{SelectMode, UiIntent};
    use common::expect;
    use objc2::Message;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSButton, NSTableCellView, NSTextField, NSView};
    use objc2_foundation::NSString;

    let s = common::start()?;
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (10.0, 10.0), (60.0, 40.0))?;
    let id = s.doc(doc).ok_or("no document")?.objects[0].id;
    s.app.dispatch(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    s.app.dispatch(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    s.pump(0.2);
    let table = s.app.with_window(doc, |w| w.operations_table.retain()).ok_or("no layer table")?;
    expect(|| format!("one layer row, got {}", table.numberOfRows()), table.numberOfRows() == 1)?;
    let column = |id: &str| -> Result<isize, String> {
        let col = table.tableColumnWithIdentifier(&NSString::from_str(id)).ok_or(format!("no column {id}"))?;
        Ok(table.columnWithIdentifier(&col.identifier()))
    };
    let cell = |id: &str| -> Result<Retained<NSView>, String> {
        table.viewAtColumn_row_makeIfNecessary(column(id)?, 0, true).ok_or(format!("no cell view in column {id}"))
    };

    // The layer column: a 12 pt circle drawn in the layer's colour, then the layer's name.
    let layer = cell("layer")?;
    expect(|| "the layer cell is a table cell view".into(), layer.downcast_ref::<NSTableCellView>().is_some())?;
    let dot = layer
        .subviews()
        .iter()
        .find(|v| v.class().name().to_str() == Ok("BurnBurnLayerDot"))
        .ok_or("no circle view in the layer cell")?;
    let size = dot.frame().size;
    expect(
        || format!("the circle is 12 x 12 pt, got {} x {}", size.width, size.height),
        size.width == 12.0 && size.height == 12.0,
    )?;
    let name = find::<NSTextField>(&layer).ok_or("no name in the layer cell")?;
    expect(|| format!("the layer's name, got {:?}", name.stringValue()), name.stringValue().to_string() == "C01")?;
    expect(|| "the name is read-only".into(), !name.isEditable())?;

    // Pixels: the centre is the layer colour, the corner (outside the circle) is transparent.
    let bounds = dot.bounds();
    let rep = dot.bitmapImageRepForCachingDisplayInRect(bounds).ok_or("no bitmap rep")?;
    dot.cacheDisplayInRect_toBitmapImageRep(bounds, &rep);
    let w = rep.pixelsWide() as f64;
    let at = |f: f64| (f * w) as isize;
    let want = burnburn_render::Theme::light().process_layers[1];
    let centre = rep.colorAtX_y(at(0.5), at(0.5)).ok_or("no centre pixel")?;
    let (r, g, b) = (centre.redComponent(), centre.greenComponent(), centre.blueComponent());
    expect(
        || format!("centre {r:.2} {g:.2} {b:.2}, layer colour {} {} {}", want.r, want.g, want.b),
        (r - f64::from(want.r)).abs() < 0.08
            && (g - f64::from(want.g)).abs() < 0.08
            && (b - f64::from(want.b)).abs() < 0.08
            && centre.alphaComponent() > 0.99,
    )?;
    let corner = rep.colorAtX_y(at(0.04), at(0.04)).ok_or("no corner pixel")?;
    expect(|| format!("corner alpha {}", corner.alphaComponent()), corner.alphaComponent() < 0.05)?;

    // The editable columns are text fields, Output and Show are check boxes.
    let speed = find::<NSTextField>(&*cell("speed")?).ok_or("no speed field")?;
    expect(|| "the speed field is editable".into(), speed.isEditable())?;
    let output = find::<NSButton>(&*cell("output")?).ok_or("no Output check box")?;
    expect(|| "Output is on".into(), output.state() == 1)?;
    let revision = s.doc(doc).map(|d| d.revision);
    // SAFETY: clicking our own check box with no sender, as the user does.
    unsafe { output.performClick(None) };
    s.pump(0.2);
    let off = s.doc(doc).is_some_and(|d| d.operations.iter().all(|o| !o.output_enabled));
    expect(|| "the click turned Output off".into(), off)?;
    expect(|| "Output off is a document change".into(), s.doc(doc).map(|d| d.revision) != revision)?;

    // An edit ends with the typed text going to presentation unparsed.
    let speed = find::<NSTextField>(&*cell("speed")?).ok_or("no speed field")?;
    speed.setStringValue(&NSString::from_str("250"));
    // SAFETY: the field's own action, as when the editor ends.
    unsafe { speed.sendAction_to(speed.action(), speed.target().as_deref()) };
    s.pump(0.2);
    let shown = s.doc(doc).and_then(|d| d.operations.first().map(|o| o.speed.clone()));
    expect(|| format!("speed after the edit {shown:?}"), shown.as_deref() == Some("250"))?;
    Ok(())
}
