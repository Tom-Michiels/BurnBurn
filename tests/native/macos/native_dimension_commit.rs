//! IM06 end test: typing 150 into the native width field and pressing Return dispatches exactly
//! one `CommitField` to the shared AppController, which makes exactly one document revision.
//! Tab afterwards (end of editing, same text) sends nothing more. Menu Undo goes through the same
//! controller (one undo stack) and menu validation follows the view model.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("native_dimension_commit: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("native_dimension_commit_dispatches_one_document_command", scenario())
}

#[cfg(target_os = "macos")]
fn scenario() -> Result<(), String> {
    use burnburn_document::UndoLabel;
    use burnburn_macos::adapter::MenuItem;
    use burnburn_presentation::{CommandKey, FieldKey, UiIntent};
    use common::expect;

    let s = common::start()?;
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (0.0, 0.0), (120.0, 80.0))?;
    let width = |s: &common::Session| {
        s.doc(doc).and_then(|d| d.inspector.and_then(|i| i.field(FieldKey::Width).map(|f| f.text.clone())))
    };
    expect(|| format!("drawn natively: expected width 120, got {:?}", width(&s)), width(&s).as_deref() == Some("120"))?;
    let shown = s
        .app
        .with_window(doc, |w| {
            w.field(burnburn_presentation::FieldKey::Width).map(|f| f.stringValue().to_string()).unwrap_or_default()
        })
        .unwrap_or_default();
    expect(|| format!("field shows the view model, got {shown:?}"), shown == "120")?;
    let (undo_enabled, undo_title) = s.menu_item_state(MenuItem::Command(CommandKey::Undo)).ok_or("no undo item")?;
    expect(
        || format!("Undo validated from the view model: {undo_enabled} {undo_title:?}"),
        undo_enabled && undo_title == "Herstel Rechthoek tekenen",
    )?;
    let (redo_enabled, _) = s.menu_item_state(MenuItem::Command(CommandKey::Redo)).ok_or("no redo item")?;
    expect(|| "Redo must be disabled (nothing undone)".into(), !redo_enabled)?;

    let r0 = s.doc(doc).ok_or("no document")?.revision;
    s.app.take_dispatched();
    s.type_into_field(doc, FieldKey::Width, "150")?;
    s.key(doc, "\r", 36)?;
    s.pump(0.1);
    let commits = s.commit_field_intents();
    expect(
        || format!("expected exactly one CommitField(Width, \"150\"), got {commits:?}"),
        commits == [UiIntent::CommitField { document: doc, field: FieldKey::Width, text: "150".into() }],
    )?;
    let d = s.doc(doc).ok_or("no document")?;
    expect(
        || format!("one document command = one revision: {r0:?} -> {:?}", d.revision),
        d.revision.raw() == r0.raw() + 1,
    )?;
    expect(|| format!("width now 150, got {:?}", width(&s)), width(&s).as_deref() == Some("150"))?;
    expect(|| format!("undo label {:?}", d.undo), d.undo == Some(UndoLabel::SetDimension))?;

    // Tab ends editing: the field sends its action again with the same text; nothing happens.
    s.key(doc, "\t", 48)?;
    s.pump(0.1);
    let again = s.commit_field_intents();
    expect(|| format!("Tab after Return must not commit again, got {again:?}"), again.is_empty())?;
    expect(|| "no extra revision after Tab".into(), s.doc(doc).is_some_and(|d| d.revision.raw() == r0.raw() + 1))?;

    // Invalid text: presentation's error shows next to the field, no command.
    s.type_into_field(doc, FieldKey::Width, "abc")?;
    s.key(doc, "\r", 36)?;
    s.pump(0.1);
    let status = s.app.with_window(doc, |w| w.status_label.stringValue().to_string()).unwrap_or_default();
    expect(|| format!("error shown, got {status:?}"), status == "Geen geldig getal.")?;
    expect(|| "an invalid value changes nothing".into(), width(&s).as_deref() == Some("150"))?;

    // Menu Undo: the same controller's undo stack.
    s.menu(MenuItem::Command(CommandKey::Undo))?;
    s.pump(0.1);
    expect(|| format!("menu undo restores 120, got {:?}", width(&s)), width(&s).as_deref() == Some("120"))?;
    let (redo_enabled, redo_title) = s.menu_item_state(MenuItem::Command(CommandKey::Redo)).ok_or("no redo item")?;
    expect(
        || format!("Redo now enabled: {redo_enabled} {redo_title:?}"),
        redo_enabled && redo_title == "Opnieuw Maat wijzigen",
    )?;
    s.menu(MenuItem::Command(CommandKey::Redo))?;
    s.pump(0.1);
    expect(|| "menu redo restores 150".into(), width(&s).as_deref() == Some("150"))?;
    Ok(())
}
