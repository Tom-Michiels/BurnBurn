//! The golf-7 editor panels in the real window (G7-editor): the structure panel makes an offset
//! chain on the selection and shows its stack (switch a step off, apply it), makes a plate from
//! typed fields, and the path panel of a text bound to a circle opens by itself and changes the
//! binding.
//!
//! Manual (not covered here): how the longer structure panel looks and scrolls in light and dark,
//! and the bent text on the canvas.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("g7_editor_panels: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    let scenarios: [(&str, fn() -> Result<(), String>); 2] = [
        ("structure_panel_makes_a_chain_switches_a_step_applies_it_and_makes_a_plate", structure),
        ("path_panel_of_a_bound_text_opens_by_itself_and_changes_the_binding", text_path),
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

#[cfg(target_os = "macos")]
fn structure() -> Result<(), String> {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_macos::panels::{EditorControl as E, G5Control as C, G5Panel as P};
    use burnburn_presentation::PlateField;
    use common::expect;

    let s = common::start()?;
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (100.0, 50.0), (130.0, 70.0))?;
    s.menu(MenuItem::StructurePanel)?;
    s.wait(3.0, "the structure panel", || s.app.g5_panel_shown(doc, P::Structure))?;

    // A chain from the typed distance and the button.
    type_into(&view(&s, doc, P::Structure, C::Editor(E::ModOffsetDistance))?, "2 mm")?;
    click(&view(&s, doc, P::Structure, C::Editor(E::ModOffsetMake))?)?;
    s.pump(0.3);
    let chains = s.doc(doc).ok_or("no document")?.structure.modifiers;
    expect(|| format!("one chain, got {}", chains.len()), chains.len() == 1)?;
    let m = chains[0].id;

    // The step goes off with its check box; the stack says so.
    click(&view(&s, doc, P::Structure, C::Editor(E::ModStepOn { modifier: m, step: 0 }))?)?;
    s.pump(0.3);
    let enabled = s.doc(doc).ok_or("no document")?.structure.modifiers[0].steps[0].enabled;
    expect(|| "the step is off".into(), !enabled)?;

    // Apply: the chain goes, its outline stays as an ordinary object.
    let objects = s.doc(doc).ok_or("no document")?.objects.len();
    click(&view(&s, doc, P::Structure, C::Editor(E::ModBake { modifier: m }))?)?;
    s.pump(0.3);
    let after = s.doc(doc).ok_or("no document")?;
    expect(|| "no chain left".into(), after.structure.modifiers.is_empty())?;
    expect(|| format!("{} objects (was {objects})", after.objects.len()), after.objects.len() == objects)?;

    // A plate from typed fields.
    for (field, text) in [
        (PlateField::Name, "Plaat B"),
        (PlateField::Width, "600"),
        (PlateField::Height, "400"),
        (PlateField::Thickness, "3"),
    ] {
        type_into(&view(&s, doc, P::Structure, C::Editor(E::NewPlateText(field)))?, text)?;
    }
    click(&view(&s, doc, P::Structure, C::Editor(E::NewPlateMake))?)?;
    s.pump(0.3);
    let plates = s.doc(doc).ok_or("no document")?.structure.plates;
    expect(
        || format!("a plate of 600 x 400, got {:?}", plates.iter().map(|p| (&p.name, &p.width)).collect::<Vec<_>>()),
        plates.len() == 1 && plates[0].name == "Plaat B" && plates[0].width == "600" && plates[0].height == "400",
    )?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn text_path() -> Result<(), String> {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_macos::panels::{EditorControl as E, G5Control as C, G5Panel as P};
    use burnburn_presentation::{CommandKey, SelectMode, TextPathEdit, UiIntent};
    use common::expect;

    let s = common::start()?;
    let doc = s.new_document()?;
    s.app.dispatch(UiIntent::InsertText { document: doc, text: "RONDOM".into() });
    s.pump(0.2);
    let text_ids = s.doc(doc).ok_or("no document")?.selection.clone();
    s.app.dispatch(UiIntent::Invoke { document: Some(doc), command: CommandKey::ToolCircle });
    s.drag_mm(doc, (100.0, 100.0), (140.0, 100.0))?;
    s.menu(MenuItem::Command(CommandKey::ToolSelect))?;
    let circle = s.doc(doc).ok_or("no document")?.selection.clone();
    let mut both = text_ids.clone();
    both.extend(circle);
    s.app.dispatch(UiIntent::Select { document: doc, ids: both, mode: SelectMode::Replace });
    s.menu(MenuItem::Command(CommandKey::TextOnPath))?;
    s.pump(0.3);
    s.app.dispatch(UiIntent::TextPath { document: doc, edit: TextPathEdit::Apply });
    s.pump(0.3);

    // The path panel is not there for a selection of text and circle; select the text.
    s.app.dispatch(UiIntent::Select { document: doc, ids: text_ids, mode: SelectMode::Replace });
    s.wait(3.0, "the path panel", || s.app.g5_panel_shown(doc, P::TextPath))?;
    type_into(&view(&s, doc, P::TextPath, C::Editor(E::TextPathStart))?, "15")?;
    s.pump(0.3);
    let start = s.doc(doc).and_then(|d| d.text_toolbar).and_then(|b| b.path).map(|p| p.start);
    expect(|| format!("the start was taken, got {start:?}"), start.as_deref() == Some("15"))?;

    // Releasing the text closes the panel.
    click(&view(&s, doc, P::TextPath, C::Editor(E::TextPathUnbind))?)?;
    s.pump(0.3);
    expect(|| "the panel goes with the binding".into(), !s.app.g5_panel_shown(doc, P::TextPath))?;
    Ok(())
}
