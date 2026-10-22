//! The option rows of the Cut Settings sheet in the real window (C03, C04, C05, W2-L1-cut): the
//! rows of the chosen mode, greyed rows with their reason, a switch that enables its values, OK
//! sending the typed option text raw, and the mode pop-up swapping the Line rows for the Fill rows.
//!
//! The sheet's buttons and pop-ups are driven like the user does (`performClick:`, the control's
//! own action); the typed text is set on the real text fields.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("cut_settings_options: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("cut_settings_option_rows_drive_presentation", scenario())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn scenario() -> Result<(), String> {
    use burnburn_presentation::{CutChoice, CutField, CutValue, OperationKind, SelectMode, UiIntent};
    use common::expect;
    use objc2::runtime::AnyObject;
    use objc2::{Message, msg_send};
    use objc2_app_kit::{NSControlStateValueOn, NSPopUpButton};
    use objc2_foundation::{NSIndexSet, NSString};

    let s = common::start()?;
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (10.0, 10.0), (50.0, 30.0))?;
    let id = s.doc(doc).ok_or("no document")?.objects[0].id;
    s.app.dispatch(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    s.app.dispatch(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    s.pump(0.2);
    let table = s.app.with_window(doc, |w| w.operations_table.retain()).ok_or("no layer table")?;
    table.selectRowIndexes_byExtendingSelection(&NSIndexSet::indexSetWithIndex(0), false);
    let open = s.app.with_window(doc, |w| w.cut_settings_button.retain()).ok_or("no Cut Settings button")?;
    // SAFETY: clicking our own button with no sender, as the user does.
    unsafe { open.performClick(None) };
    s.pump(0.3);
    let window = s.app.window(doc).ok_or("no window")?;
    expect(|| "the sheet is attached".into(), window.attachedSheet().is_some())?;

    // Shown / hidden / enabled / note of one option row.
    let part = |field: CutField| -> Option<(bool, bool, String)> {
        s.app
            .with_window(doc, |w| {
                w.cut_options
                    .iter()
                    .find(|o| o.field == field)
                    .map(|o| (!o.control.isHidden(), o.control.isEnabled(), o.note.stringValue().to_string()))
            })
            .flatten()
    };
    let shown = |field| part(field).is_some_and(|p| p.0);
    let enabled = |field| part(field).is_some_and(|p| p.0 && p.1);
    expect(|| "the Line rows are on the sheet".into(), shown(CutField::TabsEnabled) && shown(CutField::Overcut))?;
    expect(|| "the Fill and Image rows are not".into(), !shown(CutField::FillAngle) && !shown(CutField::ImageMode))?;
    expect(
        || "tab values are greyed while tabs are off".into(),
        shown(CutField::TabsValue) && !enabled(CutField::TabsValue),
    )?;
    let dot = part(CutField::DotEnabled).ok_or("no dot row")?;
    expect(
        || format!("dot mode is visible, greyed, with its reason: {dot:?}"),
        dot.0 && !dot.1 && dot.2.to_lowercase().contains("ontbrekende capability"),
    )?;

    // G7-cam: the tab power row is there, and greyed with its reason (no proof for a power per movement).
    let power = part(CutField::TabsPower).ok_or("no tab power row")?;
    expect(
        || format!("tab power is visible, greyed, with its reason: {power:?}"),
        power.0 && !power.1 && power.2.to_lowercase().contains("ontbrekende capability"),
    )?;

    // Ticking the tabs switch enables the tab values (no intent: the sheet only re-lays out).
    s.app.take_dispatched();
    let tabs_switch = |on: bool| {
        // The action runs after the window borrow ends (it refreshes every window).
        let control = s.app.with_window(doc, |w| {
            w.cut_options.iter().find(|o| o.field == CutField::TabsEnabled).map(|o| o.control.retain())
        });
        if let Some(Some(control)) = control {
            control.setIntegerValue(isize::from(on));
            // SAFETY: the control's own action to its own target, as a click sends it.
            let _: bool =
                unsafe { msg_send![&*control, sendAction: control.action(), to: control.target().as_deref()] };
        }
    };
    tabs_switch(true);
    s.pump(0.2);
    expect(
        || "tab values editable once tabs are on".into(),
        enabled(CutField::TabsValue) && enabled(CutField::TabsWidth),
    )?;
    let sent = s.app.take_dispatched();
    expect(|| format!("ticking sends no intent, got {sent:?}"), sent.is_empty())?;
    let set_text = |field: CutField, text: &str| {
        s.app.with_window(doc, |w| {
            if let Some(o) = w.cut_options.iter().find(|o| o.field == field) {
                o.control.setStringValue(&NSString::from_str(text));
            }
        });
    };
    set_text(CutField::TabsValue, "3");
    set_text(CutField::TabsWidth, "1,5 mm");
    let revision = s.doc(doc).map(|d| d.revision);
    let ok = s.app.with_window(doc, |w| w.cut_ok.retain()).ok_or("no OK")?;
    // SAFETY: as above.
    unsafe { ok.performClick(None) };
    s.pump(0.3);
    let sent = s.app.take_dispatched();
    expect(
        || format!("OK sends the option text raw, got {sent:?}"),
        sent.iter().any(|i| {
            matches!(i, UiIntent::ApplyCutSettings { settings, .. }
                if settings.extras.flag(CutField::TabsEnabled)
                    && settings.extras.text(CutField::TabsValue) == "3"
                    && settings.extras.text(CutField::TabsWidth) == "1,5 mm"
                    && settings.extras.get(CutField::FillAngle).is_none())
        }),
    )?;
    expect(|| "the sheet closed".into(), window.attachedSheet().is_none())?;
    let after = s.doc(doc).map(|d| d.revision);
    expect(
        || format!("tabs are one command: {revision:?} -> {after:?}"),
        after.map(|r| r.raw()) == revision.map(|r| r.raw() + 1),
    )?;

    // Reopened, the sheet shows what was stored.
    // SAFETY: as above.
    unsafe { open.performClick(None) };
    s.pump(0.3);
    let text_of = |field: CutField| {
        s.app
            .with_window(doc, |w| {
                w.cut_options.iter().find(|o| o.field == field).map(|o| o.control.stringValue().to_string())
            })
            .flatten()
    };
    expect(
        || format!("stored tabs {:?} {:?}", text_of(CutField::TabsValue), text_of(CutField::TabsWidth)),
        text_of(CutField::TabsValue).as_deref() == Some("3") && text_of(CutField::TabsWidth).as_deref() == Some("1.5"),
    )?;
    expect(|| "and the tab values are editable".into(), enabled(CutField::TabsValue))?;

    // Choosing Fill swaps the rows; the ramp is greyed with its reason (RampLength is unverified).
    let popup = s.app.with_window(doc, |w| w.cut_mode.retain()).ok_or("no mode pop-up")?;
    popup.selectItemAtIndex(1);
    // SAFETY: the pop-up's own action to its own target, as a menu choice sends it.
    let _: bool = unsafe { msg_send![&*popup, sendAction: popup.action(), to: popup.target().as_deref()] };
    s.pump(0.2);
    expect(
        || "Fill rows replace the Line rows".into(),
        shown(CutField::FillAngle) && shown(CutField::FillCrossHatch) && !shown(CutField::TabsEnabled),
    )?;
    let ramp = part(CutField::FillRamp).ok_or("no ramp row")?;
    expect(
        || format!("ramp greyed with its reason: {ramp:?}"),
        ramp.0 && !ramp.1 && ramp.2.to_lowercase().contains("ontbrekende capability"),
    )?;
    expect(|| "the interval shows the Fill default".into(), text_of(CutField::FillInterval).as_deref() == Some("0.1"))?;
    // The grouping pop-up lists its three choices.
    let grouping_items = s
        .app
        .with_window(doc, |w| {
            w.cut_options.iter().find(|o| o.field == CutField::FillGrouping).and_then(|o| {
                let p: &AnyObject = o.control;
                p.downcast_ref::<NSPopUpButton>().map(|p| p.numberOfItems())
            })
        })
        .flatten();
    expect(|| format!("grouping choices {grouping_items:?}"), grouping_items == Some(3))?;
    s.app.with_window(doc, |w| {
        if let Some(o) = w.cut_options.iter().find(|o| o.field == CutField::FillCrossHatch) {
            o.control.setIntegerValue(isize::from(NSControlStateValueOn == 1));
        }
    });
    s.app.take_dispatched();
    // SAFETY: as above.
    unsafe { ok.performClick(None) };
    s.pump(0.3);
    let sent = s.app.take_dispatched();
    expect(
        || format!("Fill OK sends the fill options and no Line row, got {sent:?}"),
        sent.iter().any(|i| {
            matches!(i, UiIntent::ApplyCutSettings { settings, .. }
                if settings.mode == OperationKind::Fill
                    && settings.extras.flag(CutField::FillCrossHatch)
                    && settings.extras.get(CutField::TabsEnabled).is_none())
        }),
    )?;
    let row = s.doc(doc).and_then(|d| d.operations.first().cloned()).ok_or("no row")?;
    expect(|| format!("the layer is a Fill now: {:?}", row.kind), row.kind == OperationKind::Fill)?;
    // SAFETY: as above.
    unsafe { open.performClick(None) };
    s.pump(0.3);
    let cross = s
        .app
        .with_window(doc, |w| {
            w.cut_options.iter().find(|o| o.field == CutField::FillCrossHatch).map(|o| o.control.integerValue())
        })
        .flatten();
    expect(|| format!("cross-hatch stored: {cross:?}"), cross == Some(1))?;
    let _ = (CutChoice::Threshold, CutValue::Flag(true));
    s.wait_on_screen(doc)?;
    s.app.with_window(doc, |w| {
        if let Some(sheet) = w.cut_sheet {
            common::screenshot(sheet);
        }
    });
    Ok(())
}
