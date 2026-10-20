//! Shapes and node tools in the real window (W2-L1-tekenen): the strip has the Vormen and Knopen
//! panels with their buttons, a star dragged with real mouse events appears in the shape panel with
//! its parameters (edited like a user does: typing into the field), Convert to Path hides the
//! panel again, and the join dialog opens as a sheet from the Vormen menu, shows a refused value,
//! and joins two lines with the typed tolerance.
//!
//! Manual (not covered here): how the new icons look at 1x/2x in light and dark, and the position
//! of the floating shape panel relative to the window (set `BURNBURN_NATIVE_SCREENSHOT`).

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("shape_tools: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("shapes_and_node_tools_work_in_the_real_window", scenario())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn scenario() -> Result<(), String> {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_macos::toolbar::tag_for_id;
    use burnburn_presentation::ToolCatalog;
    use burnburn_presentation::{CommandKey, ParamKey, UiIntent};
    use common::expect;
    use objc2::Message;
    use objc2_foundation::NSString;

    let s = common::start()?;
    let doc = s.new_document()?;
    let c = MenuItem::Command;

    // The strip: the two new panels and their buttons, with a reason when a button cannot act.
    let titles = s.app.with_window(doc, |w| w.strip.panel_titles()).ok_or("no strip")?;
    expect(
        || format!("panel titles {titles:?}"),
        titles.iter().any(|t| t == "VORMEN") && titles.iter().any(|t| t == "KNOPEN"),
    )?;
    // A single tool is a button; a variant is an entry of its group's drop-down.
    let in_strip = |id: &str| -> bool {
        let group = ToolCatalog::standard().get(id).and_then(|spec| spec.group);
        let tag = tag_for_id(id);
        s.app
            .with_window(doc, |w| {
                w.strip.button(id).is_some()
                    || group
                        .and_then(|g| w.strip.menu(g))
                        .is_some_and(|m| tag.is_some_and(|t| m.indexOfItemWithTag(t) >= 0))
            })
            .unwrap_or(false)
    };
    for id in [
        "shape.star",
        "shape.double_star",
        "geometry.join",
        "geometry.cut",
        "geometry.round_corners",
        "node.delete",
        "node.close_path",
        "node.smooth",
        "sketch.polygon.inscribed",
        "sketch.polygon.edge",
    ] {
        expect(|| format!("the strip has no entry for {id}"), in_strip(id))?;
    }
    let tip = s
        .app
        .with_window(doc, |w| w.strip.button("node.delete").and_then(|b| b.toolTip()).map(|t| t.to_string()))
        .flatten()
        .unwrap_or_default();
    expect(|| format!("a node button without nodes says why: {tip:?}"), tip.contains("knooppunt"))?;

    // A star by dragging with real mouse events: it is a named shape and the panel shows its parameters.
    let menu = s.app.with_window(doc, |w| w.strip.menu("shape")).flatten().ok_or("no shapes menu")?;
    let index = menu.indexOfItemWithTag(tag_for_id("shape.star").ok_or("no tag")?);
    expect(|| format!("star entry index {index}"), index >= 0)?;
    menu.performActionForItemAtIndex(index);
    s.pump(0.1);
    s.drag_mm(doc, (10.0, 10.0), (50.0, 50.0))?;
    s.pump(0.2);
    let vm = s.doc(doc).ok_or("no document")?;
    expect(
        || format!("objects {:?}", vm.objects),
        vm.objects.iter().map(|o| o.label.as_str()).collect::<Vec<_>>() == ["Ster 1"],
    )?;
    let shown = s.app.with_window(doc, |w| w.shapes_panel_shown).unwrap_or(false);
    expect(|| "the shape panel is shown".into(), shown)?;
    let rows = s
        .app
        .with_window(doc, |w| {
            w.shapes_rows
                .iter()
                .filter(|(caption, _, _)| !caption.isHidden())
                .map(|(caption, field, _)| (caption.stringValue().to_string(), field.stringValue().to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    expect(
        || format!("rows {rows:?}"),
        rows.len() == 4
            && rows[0] == ("Punten".into(), "5".into())
            && rows[1].0 == "Verhouding"
            && rows[2].0 == "Punten",
    )?;

    s.app.with_window(doc, |w| common::screenshot(w.window));

    // The user types 7 into the Points field of the shape section and leaves it.
    s.app.take_dispatched();
    let field = s.app.with_window(doc, |w| w.shapes_rows[0].1.retain()).ok_or("no field")?;
    field.setStringValue(&NSString::from_str("7"));
    // SAFETY: sending the field's own action as AppKit does after Return.
    unsafe { field.sendAction_to(field.action(), field.target().as_deref()) };
    s.pump(0.2);
    let sent = s.app.take_dispatched();
    expect(
        || format!("one CommitShapeParam with the typed text, got {sent:?}"),
        matches!(sent.as_slice(), [UiIntent::CommitShapeParam { key: ParamKey::Points, text, .. }] if text == "7"),
    )?;
    let after = s.doc(doc).ok_or("no document")?;
    expect(
        || format!("shape rows {:?}", after.shape_panel),
        after.shape_panel.as_ref().is_some_and(|p| p.rows[0].text == "7"),
    )?;

    // Vormen ▸ Omzetten naar pad: the parameters are gone and so is the shape section.
    s.menu(c(CommandKey::ShapesConvertToPath))?;
    s.pump(0.2);
    let after = s.doc(doc).ok_or("no document")?;
    expect(|| "the shape has no definition any more".into(), after.shape_panel.is_none())?;
    let (enabled, _) = s.menu_item_state(c(CommandKey::ShapesConvertToPath)).ok_or("no menu")?;
    expect(|| "Omzetten naar pad is off for a plain path".into(), !enabled)?;

    // Two lines with a gap, selected through the table; Vormen ▸ Verbinden opens the sheet.
    let line = s.app.with_window(doc, |w| w.strip.button("sketch.line")).flatten().ok_or("no line button")?;
    // SAFETY: as above.
    unsafe { line.performClick(None) };
    s.drag_mm(doc, (100.0, 10.0), (130.0, 10.0))?;
    s.drag_mm(doc, (130.3, 10.0), (160.0, 10.0))?;
    let ids: Vec<_> = s.doc(doc).ok_or("no document")?.objects.iter().skip(1).map(|o| o.id).collect();
    s.app.dispatch(UiIntent::Select { document: doc, ids, mode: burnburn_presentation::SelectMode::Replace });
    s.pump(0.2);
    s.menu(c(CommandKey::ShapesJoin))?;
    s.pump(0.3);
    let sheet_up = s.app.with_window(doc, |w| w.geometry_sheet.is_some()).unwrap_or(false);
    expect(|| "the join dialog is a sheet on the window".into(), sheet_up)?;
    let default = s.app.with_window(doc, |w| w.geometry_field.stringValue().to_string()).unwrap_or_default();
    expect(|| format!("default tolerance {default:?}"), default == "0.05")?;

    // A value that is not a length keeps the sheet and shows why; the typed text stays.
    s.app.with_window(doc, |w| w.geometry_field.setStringValue(&NSString::from_str("abc")));
    let ok = s.app.with_window(doc, |w| w.geometry_ok.retain()).ok_or("no ok")?;
    // SAFETY: as above.
    unsafe { ok.performClick(None) };
    s.pump(0.2);
    let state = s.app.with_window(doc, |w| (w.geometry_sheet.is_some(), w.geometry_field.stringValue().to_string()));
    expect(|| format!("sheet and text after a refusal: {state:?}"), state == Some((true, "abc".into())))?;

    // 0,5 mm closes the 0,3 mm gap: the sheet goes and the two lines are one.
    s.app.with_window(doc, |w| w.geometry_field.setStringValue(&NSString::from_str("0,5 mm")));
    // SAFETY: as above.
    unsafe { ok.performClick(None) };
    s.pump(0.3);
    let after = s.doc(doc).ok_or("no document")?;
    expect(
        || format!("{} objects, dialog {:?}", after.objects.len(), after.geometry_dialog),
        after.objects.len() == 2 && after.geometry_dialog.is_none(),
    )?;
    let gone = s.app.with_window(doc, |w| w.geometry_sheet.is_none()).unwrap_or(false);
    expect(|| "the sheet is gone".into(), gone)?;
    Ok(())
}
