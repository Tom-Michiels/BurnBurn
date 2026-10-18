//! Sketch mode in the real window (W1-L1b, B8, S01–S04): Schets ▸ Nieuwe schets opens the sketch
//! panel; a nearly horizontal line drawn with real mouse events gets its suggested relation; a
//! picked line gets a dimension through the menu, whose value cell is edited like AppKit's table
//! does (`tableView:setObjectValue:…`), and the geometry follows; relations are removed and
//! undone; the conflict message selects what it names; leaving the sketch hides the panel.
//!
//! Manual (not covered here): how the glyphs and dimensions look on the canvas, and typing into the
//! value cell with the keyboard (set `BURNBURN_NATIVE_SCREENSHOT` for a window capture).

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("sketch_mode: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("sketch_mode_draws_relates_dimensions_and_edits_through_the_real_window", scenario())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn scenario() -> Result<(), String> {
    use burnburn_editor::Overlay;
    use burnburn_macos::adapter::MenuItem;
    use burnburn_presentation::{CommandKey, SketchStatus, UiIntent};
    use common::expect;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{Message, msg_send};
    use objc2_foundation::{NSIndexSet, NSString};

    let s = common::start()?;
    let doc = s.new_document()?;
    let c = MenuItem::Command;

    // Schets ▸ Nieuwe schets: sketch mode, the panel is up, the toolbar can finish the sketch.
    s.menu(c(CommandKey::SketchNew))?;
    s.pump(0.2);
    let mode = s.doc(doc).and_then(|d| d.sketch_mode).ok_or("no sketch mode after Nieuwe schets")?;
    expect(|| format!("sketch {:?} is empty", mode.name), mode.rows.is_empty() && mode.name == "Schets 1")?;
    let shown = s.app.with_window(doc, |w| w.sketch_panel_shown).unwrap_or(false);
    expect(|| "the sketch panel is shown".into(), shown)?;
    let finish_enabled = s.app.with_window(doc, |w| w.sketch_toolbar.isEnabledForSegment(3)).ok_or("no window")?;
    expect(|| "the toolbar's Sluit schets applies".into(), finish_enabled)?;
    let (enabled, title) = s.menu_item_state(c(CommandKey::SketchFinish)).ok_or("no Schets menu")?;
    expect(|| format!("menu: Sluit schets enabled {enabled} title {title:?}"), enabled && title == "Sluit schets")?;

    // S01: two lines drawn with real mouse events; the nearly horizontal one gets its relation.
    let table = s.app.with_window(doc, |w| w.sketch_table.retain()).ok_or("no constraint table")?;
    let text = |column: &str, row: isize| -> Option<String> {
        let col = table.tableColumnWithIdentifier(&NSString::from_str(column))?;
        // SAFETY: reading the table's (weak) data source; the window keeps it alive.
        let source = unsafe { table.dataSource() }?;
        // SAFETY: the table's own data source, asked as NSTableView asks it.
        let value: Option<Retained<AnyObject>> =
            unsafe { msg_send![&*source, tableView: &*table, objectValueForTableColumn: &*col, row: row] };
        value.and_then(|v| v.downcast::<NSString>().ok()).map(|v| v.to_string())
    };
    s.menu(c(CommandKey::ToolLine))?;
    s.drag_mm(doc, (10.0, 10.0), (60.0, 10.5))?;
    s.drag_mm(doc, (10.0, 40.0), (60.0, 60.0))?;
    expect(|| format!("one relation row, got {}", table.numberOfRows()), table.numberOfRows() == 1)?;
    expect(|| format!("kind {:?}", text("sketch-kind", 0)), text("sketch-kind", 0).as_deref() == Some("Horizontaal"))?;
    expect(
        || format!("status {:?}", text("sketch-status", 0)),
        text("sketch-status", 0).is_some_and(|t| t.contains("voldaan")),
    )?;
    let freedom = s.app.with_window(doc, |w| w.sketch_freedom.stringValue().to_string()).unwrap_or_default();
    expect(|| format!("freedom label {freedom:?}"), freedom.contains("Vrijheidsgraden"))?;

    // Pick the oblique line with a real click; Schets ▸ Maat adds its length and starts editing it.
    s.menu(c(CommandKey::ToolSelect))?;
    s.drag_mm(doc, (35.0, 50.0), (35.0, 50.0))?;
    let selected = s.doc(doc).and_then(|d| d.sketch_mode).map(|m| m.selected_entities.len());
    expect(|| format!("one entity selected, got {selected:?}"), selected == Some(1))?;
    s.menu(c(CommandKey::SketchDimension))?;
    s.pump(0.3);
    expect(|| format!("two rows, got {}", table.numberOfRows()), table.numberOfRows() == 2)?;
    expect(|| format!("length {:?}", text("sketch-kind", 1)), text("sketch-kind", 1).as_deref() == Some("Lengte"))?;
    expect(|| format!("value {:?}", text("sketch-value", 1)), text("sketch-value", 1).as_deref() == Some("53.852 mm"))?;
    // The value cell is in edit mode (AppKit's editColumn:row:); Return would end it with the text.
    println!("    editing row {} column {}", table.editedRow(), table.editedColumn());

    let set_value = |row: isize, new: &str| -> Result<(), String> {
        let col = table.tableColumnWithIdentifier(&NSString::from_str("sketch-value")).ok_or("no value column")?;
        // SAFETY: as above.
        let source = unsafe { table.dataSource() }.ok_or("no data source")?;
        let value = NSString::from_str(new);
        // SAFETY: as AppKit calls it when a cell edit ends.
        let _: () = unsafe {
            msg_send![&*source, tableView: &*table, setObjectValue: &*value, forTableColumn: &*col, row: row]
        };
        s.pump(0.2);
        Ok(())
    };
    // The user types into the field editor of the cell being edited and leaves the cell.
    s.app.take_dispatched();
    let editor = table.currentEditor().ok_or("the value cell is not being edited")?;
    editor.setString(&NSString::from_str("70 mm"));
    table.window().ok_or("no panel")?.makeFirstResponder(None);
    s.pump(0.2);
    let sent = s.app.take_dispatched();
    expect(
        || format!("one SetSketchDimension with the typed text, got {sent:?}"),
        matches!(sent.as_slice(), [UiIntent::SetSketchDimension { text, .. }] if text == "70 mm"),
    )?;
    let m = s.doc(doc).and_then(|d| d.sketch_mode).ok_or("no sketch mode")?;
    expect(|| format!("solved value {:?}", m.rows[1].value), m.rows[1].value.as_deref() == Some("70"))?;
    let scene = s.app.scene(doc).ok_or("no scene")?;
    let lengths: Vec<f64> =
        scene.items.iter().filter_map(|i| i.path.bounds()).map(|b| b.width().hypot(b.height())).collect();
    expect(
        || format!("the oblique line is 70 mm now, contours {lengths:?}"),
        lengths.iter().any(|l| (l - 70.0).abs() < 1e-6),
    )?;
    // The same text again is no new command.
    s.app.take_dispatched();
    set_value(1, "70 mm")?;
    expect(|| "unchanged text sends nothing".into(), s.app.take_dispatched().is_empty())?;
    // A dimension label is on the canvas: dimension overlay with the number.
    let label = scene.overlays.iter().find_map(|o| match o {
        Overlay::SketchDimension { text, label_at, .. } => Some((text.clone(), *label_at)),
        _ => None,
    });
    expect(|| format!("dimension overlay {label:?}"), label.as_ref().is_some_and(|(t, _)| t == "70"))?;

    // Double-click on the label starts editing again (like the panel's own double-click).
    let (_, label_at) = label.ok_or("no label")?;
    let pt = {
        let d = s.doc(doc).ok_or("no document")?;
        d.camera.world_to_points(&d.viewport, label_at)
    };
    let window = s.app.window(doc).ok_or("no window")?;
    let view_point = s
        .app
        .with_window(doc, |w| w.canvas_view.convertPoint_toView(objc2_foundation::NSPoint::new(pt[0], pt[1]), None))
        .ok_or("no view")?;
    for kind in [objc2_app_kit::NSEventType::LeftMouseDown, objc2_app_kit::NSEventType::LeftMouseUp] {
        let event = objc2_app_kit::NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            kind,
            view_point,
            objc2_app_kit::NSEventModifierFlags::empty(),
            0.0,
            window.windowNumber(),
            None,
            0,
            2,
            1.0,
        )
        .ok_or("event")?;
        window.sendEvent(&event);
    }
    s.pump(0.3);
    let sel = s.doc(doc).and_then(|d| d.sketch_mode).map(|m| m.selected_constraints.len());
    expect(|| format!("the double-click selects the dimension, got {sel:?}"), sel == Some(1))?;
    println!("    after double-click editing row {} column {}", table.editedRow(), table.editedColumn());

    // S03: Meten/Sturen on the selected dimension; the row says "gemeten".
    let role = s.app.with_window(doc, |w| w.sketch_role.retain()).ok_or("no role button")?;
    expect(|| "Meten/Sturen applies to the selected dimension".into(), role.isEnabled())?;
    // SAFETY: clicking our own button with no sender, as the user does.
    unsafe { role.performClick(None) };
    s.pump(0.2);
    expect(
        || format!("status {:?}", text("sketch-status", 1)),
        text("sketch-status", 1).is_some_and(|t| t.contains("gemeten")),
    )?;

    // Remove the horizontal relation through the panel (select its row, press Verwijder relatie),
    // then undo through the Wijzig menu.
    let rows = NSIndexSet::indexSetWithIndex(0);
    table.selectRowIndexes_byExtendingSelection(&rows, false);
    s.pump(0.2);
    let remove = s
        .app
        .with_window(doc, |w| {
            w.sketch_buttons.iter().find(|(k, _)| *k == CommandKey::SketchRemoveConstraint).map(|(_, b)| b.retain())
        })
        .flatten()
        .ok_or("no remove button")?;
    expect(|| "Verwijder relatie applies to the selected row".into(), remove.isEnabled())?;
    // SAFETY: as above.
    unsafe { remove.performClick(None) };
    s.pump(0.2);
    expect(|| format!("one row left, got {}", table.numberOfRows()), table.numberOfRows() == 1)?;
    s.menu(c(CommandKey::Undo))?;
    s.pump(0.2);
    expect(|| format!("undo brings it back, got {}", table.numberOfRows()), table.numberOfRows() == 2)?;

    // S02: a parameter from the panel's fields, raw text.
    s.app.take_dispatched();
    s.app.with_window(doc, |w| {
        w.sketch_param_inputs[0].setStringValue(&NSString::from_str("dikte"));
        w.sketch_param_inputs[1].setStringValue(&NSString::from_str("3.1 mm"));
    });
    let set = s.app.with_window(doc, |w| w.sketch_param_set.retain()).ok_or("no parameter button")?;
    // SAFETY: as above.
    unsafe { set.performClick(None) };
    s.pump(0.2);
    let sent = s.app.take_dispatched();
    expect(
        || format!("raw parameter text, got {sent:?}"),
        sent.iter().any(
            |i| matches!(i, UiIntent::SetSketchParameter { name, text, .. } if name == "dikte" && text == "3.1 mm"),
        ),
    )?;
    let params = s.app.with_window(doc, |w| w.sketch_params_table.numberOfRows()).unwrap_or(0);
    expect(|| format!("one parameter row, got {params}"), params == 1)?;

    // S04: two contradicting lengths: the sketch fails, the message says so and a click on it
    // selects the constraints involved.
    s.menu(c(CommandKey::ToolSelect))?;
    s.drag_mm(doc, (35.0, 50.0), (35.0, 50.0))?;
    for text in ["40 mm", "90 mm"] {
        s.app.dispatch(UiIntent::AddSketchDimension {
            document: doc,
            kind: burnburn_presentation::SketchDimensionKind::Auto,
            text: text.into(),
        });
    }
    s.pump(0.3);
    let m = s.doc(doc).and_then(|d| d.sketch_mode).ok_or("no sketch mode")?;
    expect(|| format!("failed sketch, got {:?}", m.status), m.status == SketchStatus::Failed)?;
    let problem = s.app.with_window(doc, |w| w.sketch_problem.retain()).ok_or("no problem button")?;
    let message = problem.title().to_string();
    expect(|| format!("conflict in words: {message:?}"), message.contains("tegenstrijdig") && !problem.isHidden())?;
    // SAFETY: as above.
    unsafe { problem.performClick(None) };
    s.pump(0.2);
    let m = s.doc(doc).and_then(|d| d.sketch_mode).ok_or("no sketch mode")?;
    expect(
        || format!("selected {:?} vs named {:?}", m.selected_constraints, m.problem_constraints),
        !m.problem_constraints.is_empty() && m.selected_constraints == m.problem_constraints,
    )?;
    s.menu(c(CommandKey::Undo))?;
    s.pump(0.2);
    let m = s.doc(doc).and_then(|d| d.sketch_mode).ok_or("no sketch mode")?;
    expect(|| "undo restores a valid sketch".into(), m.status == SketchStatus::Valid)?;

    common::screenshot(&window);

    // Schets ▸ Sluit schets: the panel goes, the sketch stays in the document.
    s.menu(c(CommandKey::SketchFinish))?;
    s.pump(0.2);
    expect(
        || "sketch mode is off".into(),
        s.doc(doc).is_some_and(|d| d.sketch_mode.is_none() && d.sketches.len() == 1),
    )?;
    let shown = s.app.with_window(doc, |w| w.sketch_panel_shown).unwrap_or(true);
    expect(|| "the panel is hidden again".into(), !shown)?;
    Ok(())
}
