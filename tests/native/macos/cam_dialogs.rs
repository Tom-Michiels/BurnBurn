//! The CAM dialogs in the real window (W3-UI-cam; C06, M01, M02, M08): Device Settings creates and
//! changes the machine profile (rotary included), Optimization Settings reorders its levels and
//! applies one command, the test dialogs make a private job and say what they are, the focus test
//! is greyed with its reason, and the inspector's priority field commits. Everything is driven the
//! way a user does it (menu items, typed text, ticks, button clicks) and read back from
//! presentation and the widgets. No file is written and no machine is contacted.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("cam_dialogs: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    let steps: [(&str, fn() -> Result<(), String>); 4] = [
        ("device_settings_create_and_change_the_profile_with_rotary", device),
        ("optimization_dialog_reorders_and_applies_one_command", optimization),
        ("test_dialogs_make_a_private_job_and_focus_test_is_greyed_with_its_reason", tests),
        ("inspector_priority_field_commits_and_fits_the_window", priority),
    ];
    for (name, scenario) in steps {
        let code = common::report(name, scenario());
        if code != ExitCode::SUCCESS {
            return code;
        }
    }
    ExitCode::SUCCESS
}

/// Type into a text control and send its action, as Return does.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn type_text(c: &objc2_app_kit::NSControl, text: &str) {
    use objc2::msg_send;
    use objc2_foundation::NSString;
    c.setStringValue(&NSString::from_str(text));
    // SAFETY: the control's own action to its own target, as a click or Return sends it.
    let _: bool = unsafe { msg_send![c, sendAction: c.action(), to: c.target().as_deref()] };
}

/// Tick or untick a checkbox and send its action.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn tick(c: &objc2_app_kit::NSControl, on: bool) {
    use objc2::msg_send;
    c.setIntegerValue(isize::from(on));
    // SAFETY: as in `type_text`.
    let _: bool = unsafe { msg_send![c, sendAction: c.action(), to: c.target().as_deref()] };
}

#[cfg(target_os = "macos")]
fn panel_exists(
    s: &common::Session,
    doc: burnburn_types::DocumentId,
    dialog: burnburn_presentation::CamDialog,
) -> bool {
    s.app.with_cam_panel(doc, dialog, |p| p.visible).unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn with_row<R>(
    s: &common::Session,
    doc: burnburn_types::DocumentId,
    dialog: burnburn_presentation::CamDialog,
    field: burnburn_presentation::DialogField,
    f: impl FnOnce(&objc2_app_kit::NSControl, &objc2_app_kit::NSTextField) -> R,
) -> Option<R> {
    s.app.with_cam_panel(doc, dialog, |p| p.rows.iter().find(|r| r.0 == field).map(|r| f(r.1, r.2))).flatten()
}

/// Runs `f` on the retained control of a field after the panel borrow ended (an action re-enters
/// the host and refreshes the panels).
#[cfg(target_os = "macos")]
fn act(
    s: &common::Session,
    doc: burnburn_types::DocumentId,
    dialog: burnburn_presentation::CamDialog,
    field: burnburn_presentation::DialogField,
    f: impl FnOnce(&objc2_app_kit::NSControl),
) -> Option<()> {
    use objc2::Message;
    let control = with_row(s, doc, dialog, field, |c, _| c.retain())?;
    f(&control);
    Some(())
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
fn device() -> Result<(), String> {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_presentation::{CamDialog, DialogAction, DialogActionKind, DialogField, DialogValue, UiIntent};
    use common::expect;
    let s = common::start()?;
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (10.0, 10.0), (50.0, 30.0))?;
    let id = s.doc(doc).ok_or("no document")?.objects[0].id;
    s.app.dispatch(UiIntent::Select { document: doc, ids: vec![id], mode: burnburn_presentation::SelectMode::Replace });
    s.app.dispatch(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    const DEV: CamDialog = CamDialog::Device;
    expect(|| "Device Settings is a menu item".into(), s.menu_item_state(MenuItem::CamDialog(DEV)).is_some())?;
    s.menu(MenuItem::CamDialog(DEV))?;
    s.pump(0.3);
    expect(|| "the panel is on screen".into(), panel_exists(&s, doc, DEV))?;
    let title = s.app.with_cam_panel(doc, DEV, |p| p.title.clone()).ok_or("no panel")?;
    expect(|| format!("title {title:?}"), title == "Apparaatinstellingen")?;
    let notes = s.app.with_cam_panel(doc, DEV, |p| p.notes.stringValue().to_string()).ok_or("no panel")?;
    expect(|| format!("a first profile says so: {notes:?}"), notes.contains("Er is nog geen profiel"))?;
    let caps = s.app.with_cam_panel(doc, DEV, |p| p.capabilities.stringValue().to_string()).ok_or("no panel")?;
    expect(
        || format!("the capability list says the Z axis is missing: {caps:?}"),
        caps.contains("Z-stap") && caps.contains("niet ondersteund") && caps.contains("schuine"),
    )?;
    let width = with_row(&s, doc, DEV, DialogField::BedWidth, |c, _| c.stringValue().to_string());
    expect(|| format!("start value of the bed: {width:?}"), width.as_deref() == Some("600"))?;

    // A typed value is one SetCamField; the same text again sends nothing.
    s.app.take_dispatched();
    act(&s, doc, DEV, DialogField::BedWidth, |c| type_text(c, "800")).ok_or("no bed width field")?;
    s.pump(0.1);
    let sent = s.app.take_dispatched();
    expect(
        || format!("typed text goes to presentation raw: {sent:?}"),
        sent.iter().any(|i| {
            matches!(i, UiIntent::SetCamField { field: DialogField::BedWidth, value: DialogValue::Text(t), .. } if t == "800")
        }),
    )?;
    act(&s, doc, DEV, DialogField::BedWidth, |c| type_text(c, "800")).ok_or("no field")?;
    expect(|| "a repeat sends nothing".into(), s.app.take_dispatched().is_empty())?;

    // An invalid value is said next to the field and keeps the panel open.
    act(&s, doc, DEV, DialogField::MaxSpeed, |c| type_text(c, "abc")).ok_or("no field")?;
    press(&s, doc, DEV, DialogActionKind::Apply)?;
    let note = with_row(&s, doc, DEV, DialogField::MaxSpeed, |_, n| n.stringValue().to_string()).ok_or("no row")?;
    expect(|| format!("the error stands beside the field: {note:?}"), note.contains("Geen geldige waarde"))?;
    expect(|| "the panel stays".into(), panel_exists(&s, doc, DEV))?;
    act(&s, doc, DEV, DialogField::MaxSpeed, |c| type_text(c, "500")).ok_or("no field")?;

    // Apply creates the profile: the document compiles for it, the panel goes.
    s.app.take_dispatched();
    press(&s, doc, DEV, DialogActionKind::Apply)?;
    let sent = s.app.take_dispatched();
    expect(
        || format!("Apply is one intent: {sent:?}"),
        sent.iter().any(|i| matches!(i, UiIntent::CamDialogAction { action: DialogAction::Apply, .. })),
    )?;
    expect(|| "the panel closed".into(), !panel_exists(&s, doc, DEV))?;
    s.app.dispatch(UiIntent::Invoke { document: Some(doc), command: burnburn_presentation::CommandKey::Compile });
    s.wait(5.0, "the plan for the new profile", || s.doc(doc).is_some_and(|d| d.plan.is_some()))?;
    expect(|| "the plan is current".into(), s.doc(doc).and_then(|d| d.plan).is_some_and(|p| !p.stale))?;

    // Reopen: the profile is there; a rotary is a tick and three numbers; the profile change makes the plan stale.
    s.menu(MenuItem::CamDialog(DEV))?;
    s.pump(0.3);
    let width = with_row(&s, doc, DEV, DialogField::BedWidth, |c, _| c.stringValue().to_string());
    expect(|| format!("the profile shows 800: {width:?}"), width.as_deref() == Some("800"))?;
    act(&s, doc, DEV, DialogField::RotaryEnabled, |c| tick(c, true)).ok_or("no rotary tick")?;
    for (field, text) in [
        (DialogField::RotaryDiameter, "80"),
        (DialogField::StepsPerRotation, "16000"),
        (DialogField::YStepsPerMm, "40"),
    ] {
        act(&s, doc, DEV, field, |c| type_text(c, text)).ok_or("no rotary field")?;
    }
    let notes = s.app.with_cam_panel(doc, DEV, |p| p.notes.stringValue().to_string()).ok_or("no panel")?;
    expect(|| format!("the rotary says it is unconfirmed: {notes:?}"), notes.contains("bevestigd"))?;
    press(&s, doc, DEV, DialogActionKind::Apply)?;
    expect(
        || "the profile change made the compiled plan stale".into(),
        s.doc(doc).and_then(|d| d.plan).is_some_and(|p| p.stale),
    )?;

    // Back to the flat bed is a button of the dialog.
    s.menu(MenuItem::CamDialog(DEV))?;
    s.pump(0.3);
    let on = with_row(&s, doc, DEV, DialogField::RotaryEnabled, |c, _| c.integerValue() == 1);
    expect(|| "the rotary shows on".into(), on == Some(true))?;
    press(&s, doc, DEV, DialogActionKind::ReturnToFlatbed)?;
    expect(|| "the dialog closed".into(), !panel_exists(&s, doc, DEV))?;
    s.menu(MenuItem::CamDialog(DEV))?;
    s.pump(0.3);
    let on = with_row(&s, doc, DEV, DialogField::RotaryEnabled, |c, _| c.integerValue() == 1);
    expect(|| "the flat bed again: the rotary is off".into(), on == Some(false))?;
    let _ = press(&s, doc, DEV, DialogActionKind::Close);
    Ok(())
}

#[cfg(target_os = "macos")]
fn optimization() -> Result<(), String> {
    use burnburn_document::UndoLabel;
    use burnburn_document::process::{OrderKey, ProcessLabel};
    use burnburn_macos::adapter::MenuItem;
    use burnburn_presentation::{CamDialog, DialogAction, DialogActionKind, DialogField, UiIntent};
    use common::expect;

    const OPT: CamDialog = CamDialog::Optimization;
    let s = common::start()?;
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (10.0, 10.0), (50.0, 30.0))?;
    s.menu(MenuItem::CamDialog(OPT))?;
    s.pump(0.3);
    expect(|| "the panel is on screen".into(), panel_exists(&s, doc, OPT))?;
    let order = s
        .app
        .with_cam_panel(doc, OPT, |p| {
            p.rows
                .iter()
                .filter_map(|r| match r.0 {
                    DialogField::OrderEnabled(k) => Some((k, r.1.integerValue() == 1)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        })
        .ok_or("no panel")?;
    expect(
        || format!("Layer is the stored level: {order:?}"),
        order == vec![(OrderKey::Layer, true), (OrderKey::Group, false), (OrderKey::Priority, false)],
    )?;
    let revision = s.doc(doc).map(|d| d.revision);

    // Tick Priority and move it up twice with its arrow.
    act(&s, doc, OPT, DialogField::OrderEnabled(OrderKey::Priority), |c| tick(c, true)).ok_or("no tick")?;
    for _ in 0..2 {
        #[allow(unsafe_code)]
        {
            use objc2::rc::Retained;
            let up: Retained<objc2_app_kit::NSButton> = s
                .app
                .with_cam_panel(doc, OPT, |p| {
                    p.arrows.iter().find(|a| a.0 == OrderKey::Priority).map(|a| objc2::Message::retain(a.1))
                })
                .flatten()
                .ok_or("no arrow")?;
            // SAFETY: clicking our own button with no sender.
            unsafe { up.performClick(None) };
            s.pump(0.1);
        }
    }
    expect(|| "the draft is not a document change".into(), s.doc(doc).map(|d| d.revision) == revision)?;
    s.app.take_dispatched();
    press(&s, doc, OPT, DialogActionKind::Apply)?;
    let sent = s.app.take_dispatched();
    expect(
        || format!("Apply is one intent: {sent:?}"),
        sent.iter().any(|i| matches!(i, UiIntent::CamDialogAction { action: DialogAction::Apply, .. })),
    )?;
    expect(|| "the panel closed".into(), !panel_exists(&s, doc, OPT))?;
    let after = s.doc(doc).ok_or("no document")?;
    expect(
        || format!("one SetOptimization undo step, got {:?}", after.undo),
        after.undo == Some(UndoLabel::Process(ProcessLabel::SetOptimization)),
    )?;
    expect(|| "one command".into(), Some(after.revision) != revision)?;

    // Reopened: Priority leads.
    s.menu(MenuItem::CamDialog(OPT))?;
    s.pump(0.3);
    let first = s
        .app
        .with_cam_panel(doc, OPT, |p| {
            p.rows.iter().find_map(|r| match r.0 {
                DialogField::OrderEnabled(k) => Some(k),
                _ => None,
            })
        })
        .flatten();
    expect(|| format!("the order is Priority first, got {first:?}"), first == Some(OrderKey::Priority))?;
    let _ = press(&s, doc, OPT, DialogActionKind::Close);
    Ok(())
}

#[cfg(target_os = "macos")]
fn tests() -> Result<(), String> {
    use burnburn_job::{Feature, MachineCapabilities, MachineFrame, MachineProfile, OriginCorner, PowerRatio};
    use burnburn_macos::adapter::MenuItem;
    use burnburn_presentation::{CamDialog, DialogActionKind, DialogField, UiIntent};
    use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};
    use common::expect;

    let s = common::start()?;
    let doc = s.new_document()?;
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
    s.app.dispatch(UiIntent::SetMachineProfile {
        profile: Box::new(profile),
        ruida: RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged },
    });
    let before = s.doc(doc).ok_or("no document")?;

    // Material test: generate, read the result, nothing in the project changes.
    const MAT: CamDialog = CamDialog::MaterialTest;
    s.menu(MenuItem::CamDialog(MAT))?;
    s.pump(0.3);
    expect(|| "the material test panel is up".into(), panel_exists(&s, doc, MAT))?;
    let save_enabled = s
        .app
        .with_cam_panel(doc, MAT, |p| p.buttons.iter().any(|b| b.0 == DialogActionKind::ExportRd && b.1.isEnabled()));
    expect(|| "nothing to save before a test exists".into(), save_enabled == Some(false))?;
    press(&s, doc, MAT, DialogActionKind::Generate)?;
    s.wait(5.0, "the test job", || {
        s.app.with_cam_panel(doc, MAT, |p| !p.result.stringValue().to_string().is_empty()).unwrap_or(false)
    })?;
    let result = s.app.with_cam_panel(doc, MAT, |p| p.result.stringValue().to_string()).ok_or("no panel")?;
    expect(
        || format!("the result names its cells, plan and estimate: {result:?}"),
        result.contains("25 vakken") && result.contains("plan-") && result.contains("schatting"),
    )?;
    let after = s.doc(doc).ok_or("no document")?;
    expect(
        || "the document is untouched".into(),
        after.revision == before.revision && !after.dirty && after.plan.is_none() && after.operations.is_empty(),
    )?;
    let save_enabled = s
        .app
        .with_cam_panel(doc, MAT, |p| p.buttons.iter().any(|b| b.0 == DialogActionKind::ExportRd && b.1.isEnabled()));
    expect(|| "Save as RD is open once a test exists".into(), save_enabled == Some(true))?;
    // A changed field drops the result.
    act(&s, doc, MAT, DialogField::BaseSpeed, |c| type_text(c, "30")).ok_or("no field")?;
    s.pump(0.1);
    let result = s.app.with_cam_panel(doc, MAT, |p| p.result.stringValue().to_string()).ok_or("no panel")?;
    expect(|| format!("the old result went with the edit: {result:?}"), result.is_empty())?;
    let _ = press(&s, doc, MAT, DialogActionKind::Close);

    // Focus test without a Z axis: greyed, with the reason in words.
    const FOC: CamDialog = CamDialog::FocusTest;
    s.menu(MenuItem::CamDialog(FOC))?;
    s.pump(0.3);
    let generate = s.app.with_cam_panel(doc, FOC, |p| {
        p.buttons.iter().find(|b| b.0 == DialogActionKind::Generate).map(|b| b.1.isEnabled())
    });
    expect(|| format!("Generate is greyed: {generate:?}"), generate == Some(Some(false)))?;
    let (enabled, note) =
        with_row(&s, doc, FOC, DialogField::FocusStart, |c, n| (c.isEnabled(), n.stringValue().to_string()))
            .ok_or("no row")?;
    expect(
        || format!("the field is greyed with its reason: {note:?}"),
        !enabled && note.contains("Ontbrekende capability") && note.contains("Z-stap"),
    )?;
    let notes = s.app.with_cam_panel(doc, FOC, |p| p.notes.stringValue().to_string()).ok_or("no panel")?;
    expect(|| format!("the dialog says why: {notes:?}"), notes.contains("geen Z-as"))?;
    let _ = Feature::ZStep;
    let _ = press(&s, doc, FOC, DialogActionKind::Close);
    Ok(())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn priority() -> Result<(), String> {
    use burnburn_document::UndoLabel;
    use burnburn_document::process::ProcessLabel;
    use burnburn_presentation::{FieldKey, SelectMode, UiIntent};
    use common::expect;

    let s = common::start()?;
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (10.0, 10.0), (50.0, 30.0))?;
    let id = s.doc(doc).ok_or("no document")?.objects[0].id;
    s.app.dispatch(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    s.pump(0.2);
    let text = s.app.with_window(doc, |w| w.field(FieldKey::Priority).map(|f| f.stringValue().to_string())).flatten();
    expect(|| format!("the priority field shows the default: {text:?}"), text.as_deref() == Some("0"))?;
    s.app.take_dispatched();
    s.type_into_field(doc, FieldKey::Priority, "5")?;
    s.key(doc, "\r", 36)?;
    s.pump(0.3);
    let sent = s.commit_field_intents();
    expect(
        || format!("the priority commit goes to presentation raw: {sent:?}"),
        sent.iter().any(|i| matches!(i, UiIntent::CommitField { field: FieldKey::Priority, text, .. } if text == "5")),
    )?;
    let after = s.doc(doc).ok_or("no document")?;
    expect(
        || format!("one SetPriority undo step, got {:?}", after.undo),
        after.undo == Some(UndoLabel::Process(ProcessLabel::SetPriority)),
    )?;
    // The inspector column still fits the window: the lowest button is not pushed off the bottom.
    let lowest = s
        .app
        .with_window(doc, |w| {
            // SAFETY: reading the parent view of our own button.
            let column = unsafe { w.compile_button.superview() }?;
            let ys = column.subviews().iter().map(|v| v.frame().origin.y).fold(f64::MAX, f64::min);
            Some(ys)
        })
        .flatten()
        .ok_or("no inspector column")?;
    expect(|| format!("every inspector widget is inside the window (lowest y = {lowest})"), lowest >= 0.0)?;
    Ok(())
}
