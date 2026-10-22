//! The golf-6 editor panels in the real window (G6-editor): Tekenen ▸ Doos of scharnier… shows the
//! generator panel, typing a field previews the box, a refused value is named in the panel, Insert
//! makes the six panels as one undo step and the panel closes; Bewerk generator… opens it again on
//! the object with an Apply button.
//!
//! Manual (not covered here): how the panel looks next to the window in light and dark, and the
//! preview outline on the canvas (the headless test checks the overlay is in the scene).

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("g6_panels: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    let scenarios: [(&str, fn() -> Result<(), String>); 1] =
        [("generator_panel_previews_refuses_inserts_and_opens_again", generator)];
    for (name, scenario) in scenarios {
        let code = common::report(name, scenario());
        if code != ExitCode::SUCCESS {
            return code;
        }
    }
    ExitCode::SUCCESS
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

#[cfg(target_os = "macos")]
fn control(
    s: &common::Session,
    doc: burnburn_types::DocumentId,
    c: burnburn_macos::panels::G5Control,
) -> Result<objc2::rc::Retained<objc2_app_kit::NSView>, String> {
    s.app.g5_control(doc, burnburn_macos::panels::G5Panel::Generator, c).ok_or(format!("no control {c:?}"))
}

#[cfg(target_os = "macos")]
fn press(
    s: &common::Session,
    doc: burnburn_types::DocumentId,
    c: burnburn_macos::panels::G5Control,
) -> Result<(), String> {
    let view = control(s, doc, c)?;
    click(&view)?;
    s.pump(0.1);
    Ok(())
}

/// A user types `text` into a field of the panel and leaves it (Return / end of editing).
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn type_into(
    s: &common::Session,
    doc: burnburn_types::DocumentId,
    c: burnburn_macos::panels::G5Control,
    text: &str,
) -> Result<(), String> {
    use objc2::Message;
    use objc2_foundation::NSString;
    let view = control(s, doc, c)?;
    let any: &objc2::runtime::AnyObject = view.as_ref();
    let field = any.downcast_ref::<objc2_app_kit::NSTextField>().ok_or("not a field")?.retain();
    field.setStringValue(&NSString::from_str(text));
    // SAFETY: sending the field's own action as AppKit does after Return.
    unsafe { field.sendAction_to(field.action(), field.target().as_deref()) };
    s.pump(0.1);
    Ok(())
}

#[cfg(target_os = "macos")]
fn generator() -> Result<(), String> {
    use burnburn_document::UndoLabel;
    use burnburn_document::generator::GeneratorLabel;
    use burnburn_macos::adapter::MenuItem;
    use burnburn_macos::panels::{G5Control as C, G5Panel as P};
    use burnburn_presentation::{CommandKey, GeneratorField};
    use common::expect;

    let s = common::start()?;
    let doc = s.new_document()?;
    expect(|| "no generator panel before the menu item".into(), !s.app.g5_panel_shown(doc, P::Generator))?;
    s.menu(MenuItem::Command(CommandKey::GeneratorNew))?;
    s.wait(3.0, "the generator panel", || s.app.g5_panel_shown(doc, P::Generator))?;
    let vm = s.doc(doc).ok_or("no document")?;
    let dialog = vm.generator_dialog.ok_or("no dialog in the view model")?;
    expect(|| format!("six panels in the preview, got {:?}", dialog.preview), dialog.preview.as_ref().is_some_and(|p| p.parts == 6))?;

    // Typing a field previews the new size; a refused value is named in the panel and greys Insert.
    type_into(&s, doc, C::GeneratorField(GeneratorField::Depth), "45")?;
    let depth = s.doc(doc).and_then(|d| d.generator_dialog).and_then(|d| {
        d.fields.iter().find(|f| f.field == GeneratorField::Depth).map(|f| f.text.clone())
    });
    expect(|| format!("the depth was taken, got {depth:?}"), depth.as_deref() == Some("45"))?;
    type_into(&s, doc, C::GeneratorField(GeneratorField::FingerWidth), "2")?;
    let labels = s.app.g5_labels(doc, P::Generator).join(" | ");
    expect(|| format!("the refusal is in the panel, got {labels}"), labels.contains("at least") || labels.contains("minstens"))?;
    let insert = control(&s, doc, C::GeneratorInsert)?;
    #[allow(unsafe_code)]
    let enabled = {
        use objc2::Message;
        let any: &objc2::runtime::AnyObject = insert.as_ref();
        any.downcast_ref::<objc2_app_kit::NSControl>().map(|c| c.retain().isEnabled()).unwrap_or(true)
    };
    expect(|| "Insert is greyed while a field is refused".into(), !enabled)?;
    type_into(&s, doc, C::GeneratorField(GeneratorField::FingerWidth), "10")?;

    // Insert: six objects, one undo step, the panel is gone.
    press(&s, doc, C::GeneratorInsert)?;
    let vm = s.doc(doc).ok_or("no document")?;
    expect(|| format!("six panels were made, got {}", vm.objects.len()), vm.objects.len() == 6)?;
    expect(
        || format!("one generator history entry, got {:?}", vm.undo),
        vm.undo == Some(UndoLabel::Generator(GeneratorLabel::Create)),
    )?;
    expect(|| "the panel closes".into(), !s.app.g5_panel_shown(doc, P::Generator))?;

    // The new panels are selected: Bewerk generator… opens it again with an Apply button.
    s.menu(MenuItem::Command(CommandKey::GeneratorEdit))?;
    s.wait(3.0, "the panel again", || s.app.g5_panel_shown(doc, P::Generator))?;
    let editing = s.doc(doc).and_then(|d| d.generator_dialog).is_some_and(|d| d.editing);
    expect(|| "the dialog edits the existing object".into(), editing)?;
    press(&s, doc, C::GeneratorCancel)?;
    expect(|| "Cancel closes it".into(), !s.app.g5_panel_shown(doc, P::Generator))?;
    expect(|| "and changes nothing".into(), s.doc(doc).is_some_and(|d| d.objects.len() == 6))?;
    Ok(())
}
