//! The modifier stack in the inspector column of the real window (G9-editor, E06): a selection that
//! is the source or an output of a chain shows "Stapel van de selectie" inside the document window's
//! inspector (not in a floating panel), its controls work there, it goes with the selection, and the
//! linear series and the series along a path can be added from the add-a-step block.
//!
//! Manual (not covered here): how the longer inspector column looks and scrolls in light and dark.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("g9_editor_inspector: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    let scenarios: [(&str, fn() -> Result<(), String>); 2] = [
        ("the_stack_is_part_of_the_inspector_column_and_works_there", stack_in_the_inspector),
        ("the_series_kinds_are_added_from_the_inspector_stack", series_from_the_stack),
    ];
    for (name, scenario) in scenarios {
        let code = common::report(name, scenario());
        if code != ExitCode::SUCCESS {
            return code;
        }
    }
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn view(
    s: &common::Session,
    doc: burnburn_types::DocumentId,
    panel: burnburn_macos::panels::G5Panel,
    c: burnburn_macos::panels::G5Control,
) -> Result<objc2::rc::Retained<objc2_app_kit::NSView>, String> {
    s.app.g5_control(doc, panel, c).ok_or(format!("no control {c:?} in {panel:?}"))
}

/// A user's click on a control (AppKit's own `performClick:`).
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn click(view: &objc2::rc::Retained<objc2_app_kit::NSView>) -> Result<(), String> {
    use objc2::Message;
    let any: &objc2::runtime::AnyObject = view.as_ref();
    let control = any.downcast_ref::<objc2_app_kit::NSControl>().ok_or("not a control")?;
    // SAFETY: `performClick:` on a live control on the main thread, with no sender.
    unsafe { control.retain().performClick(None) };
    Ok(())
}

/// A user types `text` into a field and leaves it.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn type_into(view: &objc2::rc::Retained<objc2_app_kit::NSView>, text: &str) -> Result<(), String> {
    use objc2::Message;
    use objc2_foundation::NSString;
    let any: &objc2::runtime::AnyObject = view.as_ref();
    let field = any.downcast_ref::<objc2_app_kit::NSTextField>().ok_or("not a field")?.retain();
    field.setStringValue(&NSString::from_str(text));
    // SAFETY: sending the field's own action as AppKit does after Return.
    unsafe { field.sendAction_to(field.action(), field.target().as_deref()) };
    Ok(())
}

/// A document with a rectangle and an offset chain on it; the rectangle is selected (so the stack
/// shows). Returns the chain's id.
#[cfg(target_os = "macos")]
fn chain(s: &common::Session, doc: burnburn_types::DocumentId) -> Result<u64, String> {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_macos::panels::{EditorControl as E, G5Control as C, G5Panel as P};
    use common::expect;

    s.draw_rectangle(doc, (100.0, 50.0), (130.0, 70.0))?;
    s.menu(MenuItem::StructurePanel)?;
    s.wait(3.0, "the structure panel", || s.app.g5_panel_shown(doc, P::Structure))?;
    type_into(&view(s, doc, P::Structure, C::Editor(E::ModOffsetDistance))?, "2 mm")?;
    click(&view(s, doc, P::Structure, C::Editor(E::ModOffsetMake))?)?;
    s.pump(0.3);
    let chains = s.doc(doc).ok_or("no document")?.structure.modifiers;
    expect(|| format!("one chain, got {}", chains.len()), chains.len() == 1)?;
    Ok(chains[0].id)
}

#[cfg(target_os = "macos")]
fn stack_in_the_inspector() -> Result<(), String> {
    use burnburn_macos::panels::{EditorControl as E, G5Control as C, G5Panel as P};
    use burnburn_presentation::{SelectMode, UiIntent};
    use common::expect;
    use objc2_app_kit::NSPanel;

    let s = common::start()?;
    let doc = s.new_document()?;
    let m = chain(&s, doc)?;
    // The chain was made on the selection: its output is selected, so the stack is there.
    s.wait(3.0, "the stack", || s.app.g5_panel_shown(doc, P::Stack))?;
    let step = view(&s, doc, P::Stack, C::Editor(E::ModStepOn { modifier: m, step: 0 }))?;

    // It lives in the document window's inspector column, not in a floating panel.
    let window = step.window().ok_or("the control is in no window")?;
    let in_panel = (&*window as &objc2::runtime::AnyObject).downcast_ref::<NSPanel>().is_some();
    expect(|| "the stack control is in a floating panel".into(), !in_panel)?;
    let content = window.contentView().ok_or("no content view")?;
    let at = step.convertRect_toView(step.bounds(), None);
    let whole = content.frame();
    expect(
        || format!("the control {at:?} lies in the window {whole:?}"),
        at.origin.x >= whole.size.width - 300.0
            && at.origin.x + at.size.width <= whole.size.width + 1.0
            && at.origin.y >= -1.0
            && at.origin.y + at.size.height <= whole.size.height + whole.origin.y + 80.0,
    )?;

    // It works there: switching the step off is the document's.
    click(&step)?;
    s.pump(0.3);
    let enabled = s.doc(doc).ok_or("no document")?.structure.modifiers[0].steps[0].enabled;
    expect(|| "the step is off".into(), !enabled)?;

    // It goes with the selection.
    s.app.dispatch(UiIntent::Select { document: doc, ids: vec![], mode: SelectMode::Replace });
    s.wait(3.0, "the stack to go", || !s.app.g5_panel_shown(doc, P::Stack))?;
    expect(|| "the stack control is still visible".into(), step.isHiddenOrHasHiddenAncestor())?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn series_from_the_stack() -> Result<(), String> {
    use burnburn_macos::panels::{EditorControl as E, G5Control as C, G5Panel as P};
    use common::expect;
    use objc2_app_kit::NSPopUpButton;

    let s = common::start()?;
    let doc = s.new_document()?;
    let m = chain(&s, doc)?;
    s.wait(3.0, "the stack", || s.app.g5_panel_shown(doc, P::Stack))?;

    // Choose "Linear series" (the eighth kind), type its numbers and add it to the chain.
    let kind = view(&s, doc, P::Stack, C::Editor(E::ModNewKind))?;
    let popup = (&*kind as &objc2::runtime::AnyObject).downcast_ref::<NSPopUpButton>().ok_or("not a pop-up")?;
    expect(|| format!("nine kinds, got {}", popup.numberOfItems()), popup.numberOfItems() == 9)?;
    popup.selectItemAtIndex(7);
    type_into(&view(&s, doc, P::Stack, C::Editor(E::ModNewLinearCount))?, "3")?;
    type_into(&view(&s, doc, P::Stack, C::Editor(E::ModNewStepX))?, "40")?;
    type_into(&view(&s, doc, P::Stack, C::Editor(E::ModNewStepY))?, "0")?;
    click(&view(&s, doc, P::Stack, C::Editor(E::ModAddStep { modifier: m }))?)?;
    s.pump(0.3);
    let rows = s.doc(doc).ok_or("no document")?.structure.modifiers;
    expect(|| format!("two steps, got {}", rows[0].steps.len()), rows[0].steps.len() == 2)?;
    let value = rows[0].steps[1].value.as_ref().map(|v| v.1.clone());
    expect(|| format!("a count of 3, got {value:?}"), value.as_deref() == Some("3"))?;
    Ok(())
}
