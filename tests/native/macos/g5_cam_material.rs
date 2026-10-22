//! C08 (G5-cam): the Material Library panel's "Bijwerken" and "Bijwerken en toepassen" buttons in
//! the real panel. A library update is offered, never applied; refreshing the recipe leaves the
//! layer alone; only the explicit second button changes the layer. No file is written.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("g5_cam_material: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("material_update_buttons_refresh_the_recipe_and_only_apply_changes_the_layer", scenario())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn scenario() -> Result<(), String> {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_presentation::{CommandKey, OperationField, SelectMode, UiIntent};
    use common::expect;
    use objc2::Message;
    use objc2_foundation::NSString;

    let s = common::start()?;
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (0.0, 0.0), (40.0, 20.0))?;
    s.draw_rectangle(doc, (60.0, 0.0), (100.0, 20.0))?;
    s.menu(MenuItem::Command(CommandKey::ToolSelect))?;
    let ids: Vec<_> = s.doc(doc).ok_or("no document")?.objects.iter().map(|o| o.id).collect();
    for (i, slot) in [(0usize, 1u8), (1, 2)] {
        s.app.dispatch(UiIntent::Select { document: doc, ids: vec![ids[i]], mode: SelectMode::Replace });
        s.app.dispatch(UiIntent::AssignLayer { document: doc, slot, add: false });
    }
    let ops: Vec<_> = s.doc(doc).ok_or("no document")?.operations.iter().map(|o| o.id).collect();
    let speed = |i: usize| s.doc(doc).map(|d| d.operations[i].speed.clone());
    s.app.dispatch(UiIntent::SetOperationField {
        document: doc,
        operation: ops[0],
        field: OperationField::Speed,
        text: "12".into(),
    });
    s.menu(MenuItem::Command(CommandKey::ShowMaterialLibrary))?;
    s.wait(5.0, "the material panel", || {
        s.app.with_window(doc, |w| w.files.material_panel.is_some()).unwrap_or(false)
    })?;
    let (inputs, save, layer, apply, overwrite, update, update_apply) = s
        .app
        .with_window(doc, |w| {
            (
                w.files.material_inputs.map(|f| f.retain()),
                w.files.material_save.retain(),
                w.files.material_layer.retain(),
                w.files.material_apply.retain(),
                w.files.material_overwrite.retain(),
                w.files.material_update.retain(),
                w.files.material_update_apply.retain(),
            )
        })
        .ok_or("no window")?;
    let type_into = |f: &objc2_app_kit::NSTextField, text: &str| {
        f.setStringValue(&NSString::from_str(text));
        // SAFETY: the field's own action to its own target.
        unsafe { f.sendAction_to(f.action(), f.target().as_deref()) };
    };
    let click = |b: &objc2_app_kit::NSButton| {
        // SAFETY: clicking our own button with no sender, as the user does.
        unsafe { b.performClick(None) };
        s.pump(0.3);
    };
    let choose_layer = |i: isize| {
        layer.selectItemAtIndex(i);
        // SAFETY: the pop-up's own action to its own target.
        unsafe { layer.sendAction_to(layer.action(), layer.target().as_deref()) };
        s.pump(0.1);
    };
    let titles = |p: &objc2_app_kit::NSPopUpButton| p.itemTitles().iter().map(|t| t.to_string()).collect::<Vec<_>>();

    // Save layer one as an item and copy it onto layer two.
    choose_layer(0);
    type_into(&inputs[0], "Snijden");
    type_into(&inputs[1], "Berken");
    type_into(&inputs[2], "3");
    click(&save);
    choose_layer(1);
    click(&apply);
    let t = titles(&layer);
    expect(|| format!("layer two is linked and current: {t:?}"), t[1].contains("bibliotheekitem 1: actueel"))?;
    expect(|| "layer two copied speed 12".into(), speed(1).as_deref() == Some("12"))?;

    // A better speed measured on layer one overwrites the item: revision 2.
    s.app.dispatch(UiIntent::SetOperationField {
        document: doc,
        operation: ops[0],
        field: OperationField::Speed,
        text: "20".into(),
    });
    choose_layer(0);
    click(&overwrite);
    choose_layer(1);
    let t = titles(&layer);
    expect(|| format!("an update is offered, not applied: {t:?}"), !t[1].contains("actueel"))?;
    expect(|| "the layer keeps 12".into(), speed(1).as_deref() == Some("12"))?;
    expect(|| "both update buttons are enabled".into(), update.isEnabled() && update_apply.isEnabled())?;

    click(&update);
    let t = titles(&layer);
    expect(|| format!("the recipe is refreshed: {t:?}"), t[1].contains("actueel"))?;
    expect(|| "refreshing did not touch the layer".into(), speed(1).as_deref() == Some("12"))?;
    // Up to date, there is nothing to "update": the layer is brought in line with Toepassen.
    let flags = (update.isEnabled(), update_apply.isEnabled());
    expect(
        || format!("update buttons {flags:?}; layer {} of {:?}", layer.indexOfSelectedItem(), titles(&layer)),
        flags == (true, true),
    )?;
    expect(
        || format!("apply enabled {} / speed {:?} / layers {:?}", apply.isEnabled(), speed(1), titles(&layer)),
        apply.isEnabled(),
    )?;
    click(&apply);
    expect(|| format!("Toepassen re-applied the entry, speed {:?}", speed(1)), speed(1).as_deref() == Some("20"))?;

    // A third revision, and now both steps in one: Bijwerken en toepassen.
    s.app.dispatch(UiIntent::SetOperationField {
        document: doc,
        operation: ops[0],
        field: OperationField::Speed,
        text: "30".into(),
    });
    choose_layer(0);
    click(&overwrite);
    choose_layer(1);
    expect(|| "the layer keeps 20 until asked".into(), speed(1).as_deref() == Some("20"))?;
    click(&update_apply);
    expect(|| "Bijwerken en toepassen changed the layer".into(), speed(1).as_deref() == Some("30"))?;
    let t = titles(&layer);
    expect(|| format!("and it is current: {t:?}"), t[1].contains("actueel"))?;
    Ok(())
}
