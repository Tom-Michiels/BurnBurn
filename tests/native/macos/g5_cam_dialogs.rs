//! The G5-cam dialogs in the real window (AR02, C06, C09, LY01): Array makes a preview and one
//! modifier, Cutting order fills its pop-ups from the layers and adds a constraint, Preflight lists
//! the findings of the plan and selects the cause, and the palette ends with T1/T2. Driven the way
//! a user does it (menu items, typed text, clicks) and read back from presentation and the widgets.
//! No file is written and no machine is contacted.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("g5_cam_dialogs: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    let steps: [(&str, fn() -> Result<(), String>); 3] = [
        ("array_dialog_previews_and_makes_one_modifier", array),
        ("order_dialog_lists_the_layers_and_adds_a_constraint", order),
        ("preflight_dialog_lists_findings_and_selects_the_cause", preflight),
    ];
    for (name, scenario) in steps {
        let code = common::report(name, scenario());
        if code != ExitCode::SUCCESS {
            return code;
        }
    }
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn type_text(c: &objc2_app_kit::NSControl, text: &str) {
    use objc2::msg_send;
    use objc2_foundation::NSString;
    c.setStringValue(&NSString::from_str(text));
    // SAFETY: the control's own action to its own target, as Return sends it.
    let _: bool = unsafe { msg_send![c, sendAction: c.action(), to: c.target().as_deref()] };
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn choose(c: &objc2_app_kit::NSControl, index: isize) -> Result<(), String> {
    use objc2::msg_send;
    let popup = c.downcast_ref::<objc2_app_kit::NSPopUpButton>().ok_or("not a pop-up")?;
    popup.selectItemAtIndex(index);
    // SAFETY: the control's own action to its own target, as a menu choice sends it.
    let _: bool = unsafe { msg_send![c, sendAction: c.action(), to: c.target().as_deref()] };
    Ok(())
}

#[cfg(target_os = "macos")]
fn popup_titles(
    s: &common::Session,
    doc: burnburn_types::DocumentId,
    dialog: burnburn_presentation::CamDialog,
    field: burnburn_presentation::DialogField,
) -> Option<Vec<String>> {
    s.app
        .with_cam_panel(doc, dialog, |p| {
            p.rows.iter().find(|r| r.0 == field).and_then(|r| {
                r.1.downcast_ref::<objc2_app_kit::NSPopUpButton>()
                    .map(|b| b.itemTitles().iter().map(|t| t.to_string()).collect())
            })
        })
        .flatten()
}

#[cfg(target_os = "macos")]
fn act(
    s: &common::Session,
    doc: burnburn_types::DocumentId,
    dialog: burnburn_presentation::CamDialog,
    field: burnburn_presentation::DialogField,
    f: impl FnOnce(&objc2_app_kit::NSControl) -> Result<(), String>,
) -> Result<(), String> {
    use objc2::Message;
    let control = s
        .app
        .with_cam_panel(doc, dialog, |p| p.rows.iter().find(|r| r.0 == field).map(|r| r.1.retain()))
        .flatten()
        .ok_or(format!("no row {field:?}"))?;
    f(&control)?;
    s.pump(0.1);
    Ok(())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn press(
    s: &common::Session,
    doc: burnburn_types::DocumentId,
    dialog: burnburn_presentation::CamDialog,
    kind: burnburn_presentation::DialogActionKind,
) -> Result<(), String> {
    use objc2::Message;
    use objc2::rc::Retained;
    let button: Retained<objc2_app_kit::NSButton> = s
        .app
        .with_cam_panel(doc, dialog, |p| p.buttons.iter().find(|b| b.0 == kind).map(|b| b.1.retain()))
        .flatten()
        .ok_or(format!("no {kind:?} button"))?;
    if !button.isEnabled() {
        return Err(format!("the {kind:?} button is disabled"));
    }
    // SAFETY: clicking our own button with no sender, as the user does.
    unsafe { button.performClick(None) };
    s.pump(0.2);
    Ok(())
}

#[cfg(target_os = "macos")]
fn profile() -> Result<burnburn_presentation::UiIntent, String> {
    use burnburn_job::{MachineCapabilities, MachineFrame, MachineProfile, OriginCorner, PowerRatio};
    use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};
    let profile = MachineProfile::new(
        "native-1500x900",
        1,
        MachineFrame::new(1500.0, 900.0, OriginCorner::BottomLeft).map_err(|e| e.to_string())?,
        1000.0,
        400.0,
        PowerRatio::new(0.0).map_err(|e| e.to_string())?,
        PowerRatio::new(1.0).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?
    .with_capabilities(MachineCapabilities::conservative())
    .map_err(|e| e.to_string())?;
    Ok(burnburn_presentation::UiIntent::SetMachineProfile {
        profile: Box::new(profile),
        ruida: RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged },
    })
}

#[cfg(target_os = "macos")]
fn select(s: &common::Session, doc: burnburn_types::DocumentId, ids: Vec<burnburn_types::GeometryId>) {
    s.app.dispatch(burnburn_presentation::UiIntent::Select {
        document: doc,
        ids,
        mode: burnburn_presentation::SelectMode::Replace,
    });
}

#[cfg(target_os = "macos")]
fn array() -> Result<(), String> {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_presentation::{CamDialog, DialogActionKind, DialogField};
    use common::expect;
    const ARR: CamDialog = CamDialog::Array;
    let s = common::start()?;
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (10.0, 10.0), (30.0, 30.0))?;
    let id = s.doc(doc).ok_or("no document")?.objects[0].id;
    select(&s, doc, vec![id]);
    expect(|| "Array is a menu item".into(), s.menu_item_state(MenuItem::CamDialog(ARR)).is_some())?;
    s.menu(MenuItem::CamDialog(ARR))?;
    s.pump(0.3);
    let title = s.app.with_cam_panel(doc, ARR, |p| p.title.clone()).ok_or("no panel")?;
    expect(|| format!("title {title:?}"), title == "Reeks")?;
    let kinds = popup_titles(&s, doc, ARR, DialogField::ArrayKind).ok_or("no kind pop-up")?;
    expect(|| format!("grid, circular, path: {kinds:?}"), kinds.len() == 3)?;

    act(&s, doc, ARR, DialogField::GridColumns, |c| {
        type_text(c, "4");
        Ok(())
    })?;
    act(&s, doc, ARR, DialogField::GridRows, |c| {
        type_text(c, "2");
        Ok(())
    })?;
    let notes = s.app.with_cam_panel(doc, ARR, |p| p.notes.stringValue().to_string()).ok_or("no panel")?;
    expect(|| format!("the preview says what it makes: {notes:?}"), notes.contains("8 kopie"))?;
    let overlays = s.app.scene(doc).map_or(0, |sc| {
        sc.overlays.iter().filter(|o| matches!(o, burnburn_editor::Overlay::DragPreview { .. })).count()
    });
    expect(|| format!("7 ghosts on the canvas, got {overlays}"), overlays == 7)?;
    let before = s.doc(doc).ok_or("no document")?;
    press(&s, doc, ARR, DialogActionKind::Apply)?;
    let after = s.doc(doc).ok_or("no document")?;
    expect(|| "one object more".into(), after.objects.len() == before.objects.len() + 1)?;
    expect(|| "the panel closed".into(), s.app.with_cam_panel(doc, ARR, |p| p.visible) != Some(true))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn order() -> Result<(), String> {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_presentation::{CamDialog, DialogActionKind, DialogField, UiIntent};
    use common::expect;
    const ORD: CamDialog = CamDialog::Order;
    let s = common::start()?;
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (10.0, 10.0), (30.0, 30.0))?;
    s.draw_rectangle(doc, (60.0, 10.0), (80.0, 30.0))?;
    let ids: Vec<_> = s.doc(doc).ok_or("no document")?.objects.iter().map(|o| o.id).collect();
    select(&s, doc, vec![ids[0]]);
    s.app.dispatch(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    select(&s, doc, vec![ids[1]]);
    s.app.dispatch(UiIntent::AssignLayer { document: doc, slot: 2, add: false });

    s.menu(MenuItem::CamDialog(ORD))?;
    s.pump(0.3);
    let before = popup_titles(&s, doc, ORD, DialogField::OrderBefore).ok_or("no pop-up")?;
    expect(|| format!("both layers are listed: {before:?}"), before == vec!["C01".to_owned(), "C02".to_owned()])?;
    let existing = popup_titles(&s, doc, ORD, DialogField::OrderExisting).ok_or("no pop-up")?;
    expect(|| "no constraint yet".into(), existing.is_empty())?;
    // C02 before C01.
    act(&s, doc, ORD, DialogField::OrderBefore, |c| choose(c, 1))?;
    act(&s, doc, ORD, DialogField::OrderAfter, |c| choose(c, 0))?;
    press(&s, doc, ORD, DialogActionKind::AddRow)?;
    let existing = popup_titles(&s, doc, ORD, DialogField::OrderExisting).ok_or("no pop-up")?;
    expect(|| format!("the constraint is listed: {existing:?}"), existing == vec!["C02 \u{2192} C01".to_owned()])?;
    // The reverse would be a cycle: refused with the layers named.
    act(&s, doc, ORD, DialogField::OrderBefore, |c| choose(c, 0))?;
    act(&s, doc, ORD, DialogField::OrderAfter, |c| choose(c, 1))?;
    press(&s, doc, ORD, DialogActionKind::AddRow)?;
    let notes = s.app.with_cam_panel(doc, ORD, |p| p.notes.stringValue().to_string()).ok_or("no panel")?;
    expect(
        || format!("the cycle is said with both layers: {notes:?}"),
        notes.contains("kring") && notes.contains("C01") && notes.contains("C02"),
    )?;
    let _ = press(&s, doc, ORD, DialogActionKind::Close);
    Ok(())
}

#[cfg(target_os = "macos")]
fn preflight() -> Result<(), String> {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_presentation::{CamDialog, CommandKey, DialogActionKind, DialogField, UiIntent};
    use common::expect;
    const PRE: CamDialog = CamDialog::Preflight;
    let s = common::start()?;
    let doc = s.new_document()?;
    s.app.dispatch(profile()?);
    s.draw_rectangle(doc, (10.0, 10.0), (30.0, 30.0))?;
    s.draw_rectangle(doc, (100.0, 10.0), (120.0, 30.0))?;
    let ids: Vec<_> = s.doc(doc).ok_or("no document")?.objects.iter().map(|o| o.id).collect();
    select(&s, doc, vec![ids[0]]);
    s.app.dispatch(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    s.app.dispatch(UiIntent::Invoke { document: Some(doc), command: CommandKey::Compile });
    s.wait(10.0, "the plan", || s.doc(doc).is_some_and(|d| d.plan.is_some()))?;

    s.menu(MenuItem::CamDialog(PRE))?;
    s.pump(0.3);
    let items = popup_titles(&s, doc, PRE, DialogField::PreflightPick).ok_or("no pop-up")?;
    expect(
        || format!("the unassigned rectangle is listed with its name: {items:?}"),
        items.len() == 1 && items[0].contains("Rechthoek 2"),
    )?;
    let notes = s.app.with_cam_panel(doc, PRE, |p| p.notes.stringValue().to_string()).ok_or("no panel")?;
    expect(|| format!("counts: {notes:?}"), notes.contains("0 fouten") && notes.contains("1 waarschuwing"))?;
    press(&s, doc, PRE, DialogActionKind::ShowSource)?;
    let selection = s.doc(doc).ok_or("no document")?.selection;
    expect(|| format!("Show source selects the cause: {selection:?}"), selection == vec![ids[1]])?;
    let _ = press(&s, doc, PRE, DialogActionKind::Close);
    Ok(())
}
