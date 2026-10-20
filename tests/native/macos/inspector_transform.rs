//! W2-L1 (bewerken), native: the inspector offers the nine fields (also for a multi-selection), the
//! 3×3 reference point, the aspect lock and the flip buttons; the toolbar has the Uitlijnen control.
//! Everything is driven like a user would (typed text, button clicks) and read back from presentation.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("inspector_transform: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    let a = common::report("inspector_shows_transform_fields_and_controls_for_a_multi_selection", inspector());
    if a != ExitCode::SUCCESS {
        return a;
    }
    common::report("uitlijnen_toolbar_control_acts_on_the_selection", toolbar())
}

#[cfg(target_os = "macos")]
fn two_rectangles(s: &common::Session) -> Result<burnburn_types::DocumentId, String> {
    use burnburn_presentation::{CommandKey, UiIntent};
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (0.0, 0.0), (40.0, 30.0))?;
    s.draw_rectangle(doc, (60.0, 10.0), (100.0, 50.0))?;
    s.app.dispatch(UiIntent::Invoke { document: Some(doc), command: CommandKey::SelectAll });
    s.pump(0.1);
    Ok(doc)
}

#[cfg(target_os = "macos")]
fn inspector() -> Result<(), String> {
    use burnburn_presentation::{CommandKey, FieldKey, RefPoint};
    use common::expect;
    use objc2_app_kit::{NSAccessibility, NSControlStateValueOn};

    let s = common::start()?;
    let doc = two_rectangles(&s)?;
    let text = |key: FieldKey| s.app.with_window(doc, |w| w.field(key).map(|f| f.stringValue().to_string())).flatten();
    let enabled = |key: FieldKey| s.app.with_window(doc, |w| w.field(key).map(|f| f.isEnabled())).flatten();

    // Two objects selected: the inspector shows the selection's bounds and the neutral actions.
    for (key, expected) in
        [(FieldKey::Width, "100"), (FieldKey::ScaleX, "100"), (FieldKey::ShearX, "0"), (FieldKey::Rotation, "0")]
    {
        expect(
            || format!("{key:?} shows {:?}, expected {expected:?}", text(key)),
            text(key).as_deref() == Some(expected),
        )?;
        expect(|| format!("{key:?} is editable for a multi-selection"), enabled(key) == Some(true))?;
    }
    let captions: Vec<String> = s
        .app
        .with_window(doc, |w| {
            w.fields.iter().map(|(_, f)| f.accessibilityLabel().map(|l| l.to_string()).unwrap_or_default()).collect()
        })
        .unwrap_or_default();
    expect(
        || format!("nine transform fields and the cut priority, all labelled, got {captions:?}"),
        captions.len() == 10 && captions.iter().all(|c| !c.is_empty()),
    )?;
    expect(
        || format!("units in the captions: {captions:?}"),
        captions.iter().any(|c| c == "Schaal X (%)") && captions.iter().any(|c| c == "Scheef X (°)"),
    )?;

    // Typing a scale amount is a commit of raw text; the width follows, the field shows 100 again.
    s.type_into_field(doc, FieldKey::ScaleX, "50")?;
    s.key(doc, "\r", 36)?;
    s.pump(0.1);
    expect(
        || format!("width halved, got {:?}", text(FieldKey::Width)),
        text(FieldKey::Width).as_deref() == Some("50"),
    )?;
    expect(
        || format!("scale field back to 100, got {:?}", text(FieldKey::ScaleX)),
        text(FieldKey::ScaleX).as_deref() == Some("100"),
    )?;

    // Reference point: nine buttons, the view model's one is on; a click sends SetReferencePoint.
    let n = s.app.with_window(doc, |w| w.reference_buttons.len()).unwrap_or(0);
    expect(|| format!("nine reference buttons, got {n}"), n == 9)?;
    let states = |s: &common::Session| -> Vec<bool> {
        s.app
            .with_window(doc, |w| w.reference_buttons.iter().map(|b| b.state() == NSControlStateValueOn).collect())
            .unwrap_or_default()
    };
    let on = states(&s);
    expect(
        || format!("bottom-left is on first, got {on:?}"),
        on == [false, false, false, false, false, false, true, false, false],
    )?;
    let x_before = text(FieldKey::X);
    {
        let control = s.app.with_window(doc, |w| w.reference_buttons[4].clone()).ok_or("no window")?;
        click(&control);
    }
    s.pump(0.1);
    let point = s.doc(doc).map(|d| d.reference_point);
    expect(|| format!("centre chosen, got {point:?}"), point == Some(RefPoint::Center))?;
    let on = states(&s);
    expect(
        || format!("only the centre button is on, got {on:?}"),
        on == [false, false, false, false, true, false, false, false, false],
    )?;
    expect(
        || format!("X now shows the centre: {x_before:?} -> {:?}", text(FieldKey::X)),
        text(FieldKey::X) != x_before,
    )?;
    // Clicking the chosen point again keeps it on (the view model decides, not the toggle).
    {
        let control = s.app.with_window(doc, |w| w.reference_buttons[4].clone()).ok_or("no window")?;
        click(&control);
    }
    s.pump(0.1);
    expect(|| "the centre button stays on".into(), states(&s)[4])?;

    // Aspect lock.
    let lock = || s.app.with_window(doc, |w| w.aspect_lock.state() == NSControlStateValueOn).unwrap_or(false);
    expect(|| "lock starts off".into(), !lock() && !s.doc(doc).is_some_and(|d| d.aspect_locked))?;
    {
        let control = s.app.with_window(doc, |w| objc2::Message::retain(w.aspect_lock)).ok_or("no window")?;
        click(&control);
    }
    s.pump(0.1);
    expect(
        || "lock is on in the view model and the checkbox".into(),
        lock() && s.doc(doc).is_some_and(|d| d.aspect_locked),
    )?;
    {
        let control = s.app.with_window(doc, |w| objc2::Message::retain(w.aspect_lock)).ok_or("no window")?;
        click(&control);
    }
    s.pump(0.1);
    expect(|| "lock is off again".into(), !lock() && !s.doc(doc).is_some_and(|d| d.aspect_locked))?;

    // Flip: the button runs the command (one document revision).
    let before = s.doc(doc).ok_or("no document")?.revision;
    let flip_enabled = s
        .app
        .with_window(doc, |w| w.flip_buttons.iter().map(|b| b.isEnabled()).collect::<Vec<_>>())
        .unwrap_or_default();
    expect(|| format!("both flip buttons enabled, got {flip_enabled:?}"), flip_enabled == [true, true])?;
    {
        let control = s.app.with_window(doc, |w| w.flip_buttons[0].clone()).ok_or("no window")?;
        click(&control);
    }
    s.pump(0.1);
    let after = s.doc(doc).ok_or("no document")?.revision;
    expect(|| format!("flip is one revision: {before:?} -> {after:?}"), after.raw() == before.raw() + 1)?;
    let (_, flip_title) = s
        .menu_item_state(burnburn_macos::adapter::MenuItem::Command(CommandKey::FlipHorizontal))
        .ok_or("no flip item")?;
    expect(|| format!("menu title {flip_title:?}"), flip_title == "Spiegel horizontaal")?;

    // Nothing selected: the controls switch off.
    s.app.dispatch(burnburn_presentation::UiIntent::Select {
        document: doc,
        ids: vec![],
        mode: burnburn_presentation::SelectMode::Replace,
    });
    s.pump(0.1);
    let any_on = s
        .app
        .with_window(doc, |w| {
            w.reference_buttons.iter().any(|b| b.isEnabled())
                || w.aspect_lock.isEnabled()
                || w.flip_buttons.iter().any(|b| b.isEnabled())
        })
        .unwrap_or(true);
    expect(|| "controls are disabled without a selection".into(), !any_on)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn toolbar() -> Result<(), String> {
    use burnburn_presentation::FieldKey;
    use common::expect;
    use objc2_app_kit::NSAccessibility;

    let s = common::start()?;
    let doc = two_rectangles(&s)?;
    let segments = s.app.with_window(doc, |w| w.arrange_toolbar.segmentCount()).unwrap_or(0);
    expect(|| format!("eight segments, got {segments}"), segments == 8)?;
    let enabled: Vec<bool> = s
        .app
        .with_window(doc, |w| (0..8).map(|i| w.arrange_toolbar.isEnabledForSegment(i)).collect())
        .unwrap_or_default();
    expect(|| format!("align enabled for two objects, got {enabled:?}"), enabled[..6].iter().all(|e| *e))?;
    expect(|| format!("distribute needs three objects, got {enabled:?}"), enabled[6..].iter().all(|e| !*e))?;
    let shown: Vec<bool> = s
        .app
        .with_window(doc, |w| {
            (0..8)
                .map(|i| {
                    w.arrange_toolbar.imageForSegment(i).is_some()
                        || w.arrange_toolbar.labelForSegment(i).is_some_and(|l| !l.to_string().is_empty())
                })
                .collect()
        })
        .unwrap_or_default();
    expect(|| format!("every segment shows an image or text, got {shown:?}"), shown.iter().all(|s| *s))?;

    // Segment 0 is Lijn links uit: both rectangles end up with x = 0.
    // (Outside `with_window`: the click re-enters the host, which borrows the windows.)
    let control = s.app.with_window(doc, |w| objc2::Message::retain(w.arrange_toolbar)).ok_or("no window")?;
    control.setSelectedSegment(0);
    click(&control);
    s.pump(0.1);
    let x = s.app.with_window(doc, |w| w.field(FieldKey::X).map(|f| f.stringValue().to_string())).flatten();
    let width = s.app.with_window(doc, |w| w.field(FieldKey::Width).map(|f| f.stringValue().to_string())).flatten();
    expect(
        || format!("aligned left: selection bounds x {x:?} width {width:?}"),
        x.as_deref() == Some("0") && width.as_deref() == Some("40"),
    )?;
    let name = s.app.with_window(doc, |w| w.arrange_toolbar.accessibilityLabel().map(|l| l.to_string())).flatten();
    expect(|| format!("the control is labelled, got {name:?}"), name.as_deref() == Some("Uitlijnen"))?;
    Ok(())
}

/// A user's click on a control (AppKit's own `performClick:`).
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn click(control: &objc2_app_kit::NSControl) {
    // SAFETY: `performClick:` on a live control on the main thread, with no sender.
    unsafe { control.performClick(None) };
}
