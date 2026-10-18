//! The LightBurn-like layer UI in the real window (C01, C02, C03, C05): the colour palette above
//! the layer table assigns the selection (Shift-click adds a second treatment), the table's
//! Output and Show checkboxes are separate, and the Cut Settings sheet sends its typed text and
//! offers a mode the machine profile does not support only disabled, with the reason.
//!
//! Table checkboxes are driven through the data source (`tableView:setObjectValue:…`), as AppKit
//! calls it when a checkbox cell is clicked; the Shift-click is a real mouse-down/up pair.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("layer_palette: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("layer_palette_and_cut_settings_drive_presentation", scenario())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn scenario() -> Result<(), String> {
    use burnburn_job::{Feature, MachineCapabilities, MachineFrame, MachineProfile, OriginCorner, PowerRatio, Support};
    use burnburn_presentation::{OperationKind, SelectMode, UiIntent};
    use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};
    use common::expect;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{Message, msg_send};
    use objc2_app_kit::{NSApplication, NSEvent, NSEventModifierFlags, NSEventType};
    use objc2_foundation::{NSNumber, NSPoint, NSString};

    let s = common::start()?;
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (10.0, 10.0), (60.0, 40.0))?;
    s.draw_rectangle(doc, (80.0, 10.0), (130.0, 40.0))?;
    let ids: Vec<_> = s.doc(doc).ok_or("no document")?.objects.iter().map(|o| o.id).collect();
    expect(|| format!("two rectangles, got {ids:?}"), ids.len() == 2)?;
    let (a, b) = (ids[0], ids[1]);
    let select = |id| s.app.dispatch(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });

    // Palette: 30 colours, each named in its tooltip; a click with a selection makes layer C01.
    let palette =
        s.app.with_window(doc, |w| w.palette.iter().map(|b| b.retain()).collect::<Vec<_>>()).unwrap_or_default();
    expect(|| format!("30 palette buttons, got {}", palette.len()), palette.len() == 30)?;
    let tip = |i: usize| palette[i].toolTip().map(|t| t.to_string()).unwrap_or_default();
    expect(|| format!("tooltip of colour 1: {:?}", tip(1)), tip(1).starts_with("C01") && tip(1).contains("nog leeg"))?;
    select(b);
    s.app.take_dispatched();
    // SAFETY: clicking our own button with no sender, as the user does.
    unsafe { palette[1].performClick(None) };
    s.pump(0.2);
    let sent = s.app.take_dispatched();
    expect(
        || format!("plain click assigns, got {sent:?}"),
        sent.iter().any(|i| matches!(i, UiIntent::AssignLayer { slot: 1, add: false, .. })),
    )?;
    let table = s.app.with_window(doc, |w| w.operations_table.retain()).ok_or("no layer table")?;
    expect(|| format!("one layer row, got {}", table.numberOfRows()), table.numberOfRows() == 1)?;
    expect(|| format!("tooltip now {:?}", tip(1)), tip(1).contains("1 object"))?;

    let cell = |column: &str, row: isize| -> Option<Retained<AnyObject>> {
        let col = table.tableColumnWithIdentifier(&NSString::from_str(column))?;
        // SAFETY: reading the table's (weak) data source; the window keeps it alive.
        let source = unsafe { table.dataSource() }?;
        // SAFETY: the table's own data source, asked as NSTableView asks it.
        unsafe { msg_send![&*source, tableView: &*table, objectValueForTableColumn: &*col, row: row] }
    };
    let text =
        |column: &str, row: isize| cell(column, row).and_then(|v| v.downcast::<NSString>().ok()).map(|v| v.to_string());
    let checked =
        |column: &str, row: isize| cell(column, row).and_then(|v| v.downcast::<NSNumber>().ok()).map(|n| n.boolValue());
    expect(
        || format!("row {:?} {:?} {:?}", text("layer", 0), text("kind", 0), text("passes", 0)),
        text("layer", 0).as_deref() == Some("C01")
            && text("kind", 0).as_deref() == Some("Lijn")
            && text("passes", 0).as_deref() == Some("1"),
    )?;

    // C02: `a` into C01, then Shift-click C03: a second treatment of the same geometry.
    select(a);
    // SAFETY: as above.
    unsafe { palette[1].performClick(None) };
    s.pump(0.1);
    s.app.take_dispatched();
    let window = s.app.window(doc).ok_or("no window")?;
    let centre = palette[3].convertPoint_toView(NSPoint::new(9.0, 9.0), None);
    let click = |kind| {
        NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
            kind,
            centre,
            NSEventModifierFlags::Shift,
            0.0,
            window.windowNumber(),
            None,
            0,
            1,
            1.0,
        )
    };
    let (down, up) =
        (click(NSEventType::LeftMouseDown).ok_or("event")?, click(NSEventType::LeftMouseUp).ok_or("event")?);
    // The button tracks until the mouse-up, which it takes from the queue.
    NSApplication::sharedApplication(s.mtm).postEvent_atStart(&up, false);
    window.sendEvent(&down);
    s.pump(0.2);
    let sent = s.app.take_dispatched();
    expect(
        || format!("Shift-click adds, got {sent:?}"),
        sent.iter().any(|i| matches!(i, UiIntent::AssignLayer { slot: 3, add: true, .. })),
    )?;
    let v = s.doc(doc).ok_or("no document")?;
    let count = |slot: u8| v.operations.iter().find(|o| o.slot == slot).map(|o| o.targets);
    expect(
        || format!("C01 {:?} objects, C03 {:?}, {} objects", count(1), count(3), v.objects.len()),
        count(1) == Some(2) && count(3) == Some(1) && v.objects.len() == 2,
    )?;

    // C01: Show and Output are separate checkboxes.
    let row1 = s.doc(doc).and_then(|d| d.operations.iter().position(|o| o.slot == 1)).ok_or("no C01 row")? as isize;
    let set = |column: &str, on: bool| -> Result<(), String> {
        let col = table.tableColumnWithIdentifier(&NSString::from_str(column)).ok_or("no column")?;
        // SAFETY: as above.
        let source = unsafe { table.dataSource() }.ok_or("no data source")?;
        let value = NSNumber::numberWithBool(on);
        // SAFETY: as AppKit calls it when a checkbox cell is clicked.
        let _: () = unsafe {
            msg_send![&*source, tableView: &*table, setObjectValue: &*value, forTableColumn: &*col, row: row1]
        };
        s.pump(0.2);
        Ok(())
    };
    s.wait_on_screen(doc)?;
    let revision = s.doc(doc).map(|d| d.revision);
    let frames = s.app.with_window(doc, |w| w.canvas_frames).unwrap_or(0);
    set("visible", false)?;
    expect(
        || format!("hidden: output {:?} show {:?}", checked("output", row1), checked("visible", row1)),
        checked("output", row1) == Some(true) && checked("visible", row1) == Some(false),
    )?;
    expect(|| "showing is no document change".into(), s.doc(doc).map(|d| d.revision) == revision)?;
    // Metal presents only on screen; an occluded window skips this check (not a pass).
    if s.wait_on_screen(doc)? {
        s.wait(2.0, "a redraw without the hidden layer", || {
            s.app.with_window(doc, |w| w.canvas_frames).unwrap_or(0) > frames
        })?;
    }
    set("output", false)?;
    expect(
        || format!("output off: output {:?} show {:?}", checked("output", row1), checked("visible", row1)),
        checked("output", row1) == Some(false) && checked("visible", row1) == Some(false),
    )?;
    expect(|| "output is one document change".into(), s.doc(doc).map(|d| d.revision) != revision)?;
    set("visible", true)?;
    set("output", true)?;

    // Cut Settings: the button opens the sheet for the selected row, filled from the layer.
    s.app.take_dispatched();
    let set_rows = objc2_foundation::NSIndexSet::indexSetWithIndex(row1 as usize);
    table.selectRowIndexes_byExtendingSelection(&set_rows, false);
    let open = s.app.with_window(doc, |w| w.cut_settings_button.retain()).ok_or("no Cut Settings button")?;
    // SAFETY: as above.
    unsafe { open.performClick(None) };
    s.pump(0.3);
    let sheet = s.app.with_window(doc, |w| w.cut_sheet.map(|w| w.retain())).flatten().ok_or("no sheet shown")?;
    expect(
        || "sheet attached to the window".into(),
        window.attachedSheet().is_some_and(|w| std::ptr::eq(&*w, &*sheet)),
    )?;
    let field = |i: usize| s.app.with_window(doc, |w| w.cut_fields[i].stringValue().to_string()).unwrap_or_default();
    expect(
        || format!("fields {:?}", (0..5).map(field).collect::<Vec<_>>()),
        (0..5).map(field).collect::<Vec<_>>() == ["100", "20", "20", "1", "0"],
    )?;
    let title = s.app.with_window(doc, |w| w.cut_title.stringValue().to_string()).unwrap_or_default();
    expect(|| format!("sheet title {title:?}"), title.contains("C01"))?;

    // C03: kerf as typed; OK sends the raw text and closes the sheet.
    s.app.with_window(doc, |w| {
        w.cut_fields[0].setStringValue(&NSString::from_str("250 mm/s"));
        w.cut_fields[4].setStringValue(&NSString::from_str("0,15 mm"));
    });
    let ok = s.app.with_window(doc, |w| w.cut_ok.retain()).ok_or("no OK")?;
    // SAFETY: as above.
    unsafe { ok.performClick(None) };
    s.pump(0.3);
    let sent = s.app.take_dispatched();
    expect(
        || format!("OK sends raw text, got {sent:?}"),
        sent.iter().any(|i| {
            matches!(i, UiIntent::ApplyCutSettings { settings, .. }
            if settings.speed == "250 mm/s" && settings.kerf == "0,15 mm" && settings.mode == OperationKind::Line)
        }),
    )?;
    expect(|| "sheet closed after a valid OK".into(), window.attachedSheet().is_none())?;
    let row = s.doc(doc).and_then(|d| d.operations.get(row1 as usize).cloned()).ok_or("no row")?;
    expect(|| format!("stored kerf {:?}, speed {:?}", row.kerf, row.speed), row.kerf == "0.15" && row.speed == "250")?;

    // C05: with a profile without Offset fill, that mode is listed but disabled, with the reason.
    let profile = MachineProfile::new(
        "research-1500x900",
        1,
        MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).map_err(|e| e.to_string())?,
        1000.0,
        400.0,
        PowerRatio::new(0.0).map_err(|e| e.to_string())?,
        PowerRatio::new(1.0).map_err(|e| e.to_string())?,
    )
    .and_then(|p| {
        p.with_capabilities(MachineCapabilities::conservative().with(Feature::OffsetFill, Support::Unsupported))
    })
    .map_err(|e| e.to_string())?;
    s.app.dispatch(UiIntent::SetMachineProfile {
        profile: Box::new(profile),
        ruida: RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged },
    });
    // SAFETY: as above.
    unsafe { open.performClick(None) };
    s.pump(0.3);
    let popup = s.app.with_window(doc, |w| w.cut_mode.retain()).ok_or("no mode pop-up")?;
    let item = popup.itemAtIndex(2).ok_or("no third mode")?;
    expect(
        || format!("offset fill item {:?} enabled {}", item.title().to_string(), item.isEnabled()),
        !item.isEnabled() && item.title().to_string().to_lowercase().contains("ontbrekende capability"),
    )?;
    expect(|| "Line and Fill stay available".into(), popup.itemAtIndex(1).is_some_and(|i| i.isEnabled()))?;
    // Choosing Fill disables the kerf field (kerf is Line only).
    popup.selectItemAtIndex(1);
    // SAFETY: the pop-up's own action to its own target, as a menu choice sends it.
    let _: bool = unsafe { msg_send![&*popup, sendAction: popup.action(), to: popup.target().as_deref()] };
    s.pump(0.1);
    let kerf_on = s.app.with_window(doc, |w| w.cut_fields[4].isEnabled()).unwrap_or(true);
    expect(|| "kerf disabled for Fill".into(), !kerf_on)?;
    // An invalid field keeps the sheet open with the message and the typed text.
    s.app.with_window(doc, |w| w.cut_fields[3].setStringValue(&NSString::from_str("0")));
    // SAFETY: as above.
    unsafe { ok.performClick(None) };
    s.pump(0.3);
    let (error, passes) = s
        .app
        .with_window(doc, |w| (w.cut_error.stringValue().to_string(), w.cut_fields[3].stringValue().to_string()))
        .unwrap_or_default();
    expect(|| format!("error {error:?}, passes {passes:?}"), !error.is_empty() && passes == "0")?;
    expect(|| "still open after an error".into(), window.attachedSheet().is_some())?;
    s.app.take_dispatched();
    let cancel = s.app.with_window(doc, |w| w.cut_cancel.retain()).ok_or("no Cancel")?;
    // SAFETY: as above.
    unsafe { cancel.performClick(None) };
    s.pump(0.3);
    let sent = s.app.take_dispatched();
    expect(|| format!("cancel sends {sent:?}"), sent.iter().all(|i| matches!(i, UiIntent::CancelCutSettings { .. })))?;
    expect(|| "sheet closed after Cancel".into(), window.attachedSheet().is_none())?;
    let row = s.doc(doc).and_then(|d| d.operations.get(row1 as usize).cloned()).ok_or("no row")?;
    expect(
        || format!("cancel kept {:?} {:?}", row.kind, row.passes),
        row.kind == OperationKind::Line && row.passes == "1",
    )?;

    // The swatches use the canvas theme's colours.
    let theme = burnburn_render::Theme::light();
    let tint = palette[2].contentTintColor().ok_or("no swatch colour")?;
    let rgb = tint.colorUsingColorSpace(&objc2_app_kit::NSColorSpace::sRGBColorSpace()).ok_or("no sRGB")?;
    let want = theme.process_layers[2];
    expect(
        || format!("swatch 2 {:.2} {:.2} {:.2}", rgb.redComponent(), rgb.greenComponent(), rgb.blueComponent()),
        (rgb.redComponent() - f64::from(want.r)).abs() < 0.01
            && (rgb.greenComponent() - f64::from(want.g)).abs() < 0.01
            && (rgb.blueComponent() - f64::from(want.b)).abs() < 0.01,
    )?;

    s.wait_on_screen(doc)?;
    s.app.with_window(doc, |w| common::screenshot(w.window));
    Ok(())
}
