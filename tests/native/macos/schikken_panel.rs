//! Schikken in the real window (W3-UI-schikken): the toolbar's Dock and Move Together segments act
//! on the selection, Warp is driven with real mouse events and applied from the panel, the nest
//! panel runs the cancellable task on the real worker pool and applies its result, Print and Cut
//! and the Center Finder take typed values, and the Resize Slots dialog shows why it did nothing.
//!
//! Manual (not covered here): how the panel looks next to the window in light and dark, the icons
//! of the new toolbar segments at 1x/2x, and a real laser reading for Print and Cut (set
//! `BURNBURN_NATIVE_SCREENSHOT` for the screenshot).

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("schikken_panel: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    let a = common::report("toolbar_dock_and_warp_with_the_panel", dock_and_warp());
    if a != ExitCode::SUCCESS {
        return a;
    }
    common::report("nest_registration_finder_and_slots_panels", flows())
}

/// Two rectangles at (10,10)–(30,30) and (50,10)–(70,30), both selected.
#[cfg(target_os = "macos")]
fn two_rectangles(s: &common::Session) -> Result<burnburn_types::DocumentId, String> {
    use burnburn_presentation::{CommandKey, UiIntent};
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (10.0, 10.0), (30.0, 30.0))?;
    s.draw_rectangle(doc, (50.0, 10.0), (70.0, 30.0))?;
    s.app.dispatch(UiIntent::Invoke { document: Some(doc), command: CommandKey::SelectAll });
    s.pump(0.1);
    Ok(doc)
}

/// A user's click on a control (AppKit's own `performClick:`).
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn click(view: &objc2_app_kit::NSView) -> Result<(), String> {
    use objc2::Message;
    let any: &objc2::runtime::AnyObject = view.as_ref();
    let control = any.downcast_ref::<objc2_app_kit::NSControl>().ok_or("not a control")?;
    // SAFETY: `performClick:` on a live control on the main thread, with no sender.
    unsafe { control.retain().performClick(None) };
    Ok(())
}

/// A user types `text` into a field of the panel and leaves it (Return / end of editing).
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn type_into(
    s: &common::Session,
    doc: burnburn_types::DocumentId,
    control: burnburn_macos::panels::SchikkenControl,
    text: &str,
) -> Result<(), String> {
    use objc2::Message;
    use objc2_foundation::NSString;
    let view = s.app.schikken_control(doc, control).ok_or(format!("no control {control:?}"))?;
    let any: &objc2::runtime::AnyObject = view.as_ref();
    let field = any.downcast_ref::<objc2_app_kit::NSTextField>().ok_or("not a field")?.retain();
    field.setStringValue(&NSString::from_str(text));
    // SAFETY: sending the field's own action as AppKit does after Return.
    unsafe { field.sendAction_to(field.action(), field.target().as_deref()) };
    s.pump(0.1);
    Ok(())
}

#[cfg(target_os = "macos")]
fn press(
    s: &common::Session,
    doc: burnburn_types::DocumentId,
    control: burnburn_macos::panels::SchikkenControl,
) -> Result<(), String> {
    let view = s.app.schikken_control(doc, control).ok_or(format!("no control {control:?}"))?;
    click(&view)?;
    s.pump(0.1);
    Ok(())
}

#[cfg(target_os = "macos")]
fn labels(s: &common::Session, doc: burnburn_types::DocumentId) -> Vec<String> {
    s.app.schikken_labels(doc)
}

#[cfg(target_os = "macos")]
fn dock_and_warp() -> Result<(), String> {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_macos::panels::SchikkenControl as C;
    use burnburn_presentation::{CommandKey, FieldKey};
    use common::expect;
    use objc2::Message;

    let s = common::start()?;
    let doc = two_rectangles(&s)?;

    // The Uitlijnen control: segment 8 is Dock Left. The first selected rectangle stays; the second
    // slides against it: the selection is 40 wide afterwards (10…50).
    let control = s.app.with_window(doc, |w| w.arrange_toolbar.retain()).ok_or("no window")?;
    control.setSelectedSegment(8);
    click(&control)?;
    s.pump(0.1);
    let width = s.app.with_window(doc, |w| w.field(FieldKey::Width).map(|f| f.stringValue().to_string())).flatten();
    expect(|| format!("docked: selection width {width:?}"), width.as_deref() == Some("40"))?;
    let x = s.app.with_window(doc, |w| w.field(FieldKey::X).map(|f| f.stringValue().to_string())).flatten();
    expect(|| format!("the reference stayed: x {x:?}"), x.as_deref() == Some("10"))?;

    // Gereedschap ▸ Vervorm selectie (4 punten): the panel opens by itself with the hint.
    s.menu(MenuItem::Command(CommandKey::ToolWarp))?;
    s.pump(0.2);
    expect(|| "the Schikken panel is shown for Warp".into(), s.app.schikken_panel_shown(doc))?;
    let all = labels(&s, doc);
    expect(|| format!("heading and hint, got {all:?}"), all.iter().any(|l| l.contains("Vervorm selectie (4 punten)")))?;
    let apply = s.app.schikken_control(doc, C::ReshapeApply).ok_or("no apply button")?;
    let enabled = |v: &objc2_app_kit::NSView| {
        let any: &objc2::runtime::AnyObject = v.as_ref();
        any.downcast_ref::<objc2_app_kit::NSControl>().is_some_and(|c| c.isEnabled())
    };
    expect(|| "Apply waits for a change".into(), !enabled(&apply))?;

    // Drag the top-right handle (50,30) with real mouse events: the panel enables Apply.
    s.drag_mm(doc, (50.0, 30.0), (45.0, 45.0))?;
    s.pump(0.2);
    let apply = s.app.schikken_control(doc, C::ReshapeApply).ok_or("no apply button")?;
    expect(|| "Apply is enabled after a drag".into(), enabled(&apply))?;
    s.app.with_window(doc, |w| common::screenshot(w.window));
    click(&apply)?;
    s.pump(0.2);
    let vm = s.doc(doc).ok_or("no document")?;
    expect(
        || format!("one warp in the history, got {:?}", vm.undo),
        vm.undo == Some(burnburn_document::UndoLabel::Edit(burnburn_document::EditLabel::Warp)),
    )?;
    expect(|| "the panel closes with the tool".into(), !s.app.schikken_panel_shown(doc))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn flows() -> Result<(), String> {
    use burnburn_document::{EditLabel, UndoLabel};
    use burnburn_macos::adapter::MenuItem;
    use burnburn_macos::panels::SchikkenControl as C;
    use burnburn_presentation::{CommandKey, NestRun};
    use common::expect;

    let s = common::start()?;
    let doc = two_rectangles(&s)?;
    let c = MenuItem::Command;

    // Quick Nest: the panel, the real worker pool, then Apply.
    s.menu(c(CommandKey::QuickNest))?;
    s.pump(0.2);
    expect(|| "the nest panel is shown".into(), s.app.schikken_panel_shown(doc))?;
    expect(|| format!("heading, got {:?}", labels(&s, doc)), labels(&s, doc).iter().any(|l| l == "Snel nesten"))?;
    press(&s, doc, C::NestStart)?;
    s.wait(10.0, "the nest result", || {
        s.doc(doc).and_then(|d| d.schikken.nest).is_some_and(|n| n.run == NestRun::Done)
    })?;
    s.pump(0.2);
    let all = labels(&s, doc);
    expect(|| format!("the report names the placements, got {all:?}"), all.iter().any(|l| l.contains("2 geplaatst")))?;
    press(&s, doc, C::NestApply)?;
    let vm = s.doc(doc).ok_or("no document")?;
    expect(
        || format!("one nest in the history, got {:?}", vm.undo),
        vm.undo == Some(UndoLabel::Edit(EditLabel::Nest)),
    )?;
    press(&s, doc, C::NestClose)?;
    expect(|| "the nest panel closes".into(), !s.app.schikken_panel_shown(doc))?;

    // Resize Slots: rectangles have none of that width; the dialog says why and stays.
    s.menu(c(CommandKey::ResizeSlots))?;
    s.pump(0.2);
    type_into(&s, doc, C::SlotsFrom, "3")?;
    type_into(&s, doc, C::SlotsTo, "3,4")?;
    press(&s, doc, C::SlotsOk)?;
    let vm = s.doc(doc).ok_or("no document")?;
    expect(
        || "the slots dialog stays open with its reason".into(),
        vm.schikken.slots.is_some_and(|d| d.error.is_some()),
    )?;
    press(&s, doc, C::SlotsCancel)?;

    // Print and Cut with typed values: two exact marks give the exact rotation.
    s.menu(c(CommandKey::PrintAndCut))?;
    s.pump(0.2);
    // New design targets drop the measurements, so the targets come first.
    let marks = [("0", "0", "100", "50"), ("100", "0", "100", "150")];
    for (i, (x, y, _, _)) in marks.iter().enumerate() {
        type_into(&s, doc, C::PcX(i), x)?;
        type_into(&s, doc, C::PcY(i), y)?;
    }
    for (i, (_, _, mx, my)) in marks.iter().enumerate() {
        type_into(&s, doc, C::PcMeasuredX(i), mx)?;
        type_into(&s, doc, C::PcMeasuredY(i), my)?;
    }
    press(&s, doc, C::PcRegister)?;
    let all = labels(&s, doc);
    expect(|| format!("registered at 90°, got {all:?}"), all.iter().any(|l| l.contains("hoek 90,00°")))?;
    press(&s, doc, C::PcReset)?;
    let vm = s.doc(doc).ok_or("no document")?;
    expect(|| "reset leaves no registration".into(), vm.schikken.print_and_cut.is_some_and(|p| p.result.is_none()))?;
    press(&s, doc, C::PcClose)?;

    // Center Finder: three typed edge points.
    s.menu(c(CommandKey::CenterFinder))?;
    s.pump(0.2);
    for (x, y) in [("70", "40"), ("50", "60"), ("30", "40")] {
        type_into(&s, doc, C::CfX, x)?;
        type_into(&s, doc, C::CfY, y)?;
        press(&s, doc, C::CfAdd)?;
    }
    let all = labels(&s, doc);
    expect(
        || format!("centre and radius, got {all:?}"),
        all.iter().any(|l| l.contains("Middelpunt (50, 40) mm, straal 20 mm")),
    )?;
    press(&s, doc, C::CfClose)?;
    Ok(())
}
