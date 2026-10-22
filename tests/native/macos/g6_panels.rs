//! The golf-6 editor panels in the real window (G6-editor): Tekenen ▸ Doos of scharnier… shows the
//! generator panel, typing a field previews the box, a refused value is named in the panel, Insert
//! makes the six panels as one undo step and the panel closes; Bewerk generator… opens it again on
//! the object with an Apply button. Archief ▸ Zoek projecten… reads the folder of the open project,
//! a query narrows the list, and a frozen revision opens next to the design as an unsaved window.
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
    let scenarios: [(&str, fn() -> Result<(), String>); 2] = [
        ("generator_panel_previews_refuses_inserts_and_opens_again", generator),
        ("search_panel_finds_a_project_and_opens_an_old_revision_next_to_the_design", search),
    ];
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
    panel_control(s, doc, burnburn_macos::panels::G5Panel::Generator, c)
}

#[cfg(target_os = "macos")]
fn panel_control(
    s: &common::Session,
    doc: burnburn_types::DocumentId,
    panel: burnburn_macos::panels::G5Panel,
    c: burnburn_macos::panels::G5Control,
) -> Result<objc2::rc::Retained<objc2_app_kit::NSView>, String> {
    s.app.g5_control(doc, panel, c).ok_or(format!("no control {c:?} in {panel:?}"))
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
    expect(
        || format!("six panels in the preview, got {:?}", dialog.preview),
        dialog.preview.as_ref().is_some_and(|p| p.parts == 6),
    )?;

    // Typing a field previews the new size; a refused value is named in the panel and greys Insert.
    type_into(&s, doc, C::GeneratorField(GeneratorField::Depth), "45")?;
    let depth = s
        .doc(doc)
        .and_then(|d| d.generator_dialog)
        .and_then(|d| d.fields.iter().find(|f| f.field == GeneratorField::Depth).map(|f| f.text.clone()));
    expect(|| format!("the depth was taken, got {depth:?}"), depth.as_deref() == Some("45"))?;
    type_into(&s, doc, C::GeneratorField(GeneratorField::FingerWidth), "2")?;
    let labels = s.app.g5_labels(doc, P::Generator).join(" | ");
    expect(
        || format!("the refusal is in the panel, got {labels}"),
        labels.contains("at least") || labels.contains("minstens"),
    )?;
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

#[cfg(target_os = "macos")]
fn search() -> Result<(), String> {
    use burnburn_document::{CommandPayload, DocumentCommand, DocumentSession};
    use burnburn_geometry::{Point, Rect};
    use burnburn_io::{Project, ProjectUuid, save_project};
    use burnburn_macos::adapter::MenuItem;
    use burnburn_macos::panels::{G5Control as C, G5Panel as P};
    use burnburn_presentation::{CommandKey, UiIntent};
    use burnburn_types::{DocumentId, Length, RequestId, StableId};
    use common::expect;

    let s = common::start()?;
    // Two projects on the real disk (a fresh temp folder): deurbord with a frozen revision of an
    // earlier design, and onderzetters.
    let werk = s.dir.join("werk");
    std::fs::create_dir_all(&werk).map_err(|e| e.to_string())?;
    let rect = |session: &mut DocumentSession, name: &str, x: f64| -> Result<(), String> {
        let r = Rect::new(
            Point::new(x, 0.0),
            Length::from_mm(10.0).map_err(|e| e.to_string())?,
            Length::from_mm(10.0).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let cmd = DocumentCommand {
            document: session.id(),
            expected: session.revision(),
            request: RequestId::from_raw(1),
            payload: CommandPayload::CreateRectangle { rect: r, name: name.into() },
        };
        session.apply(cmd).map(|_| ()).map_err(|e| format!("{e:?}"))
    };
    let mut design = DocumentSession::new(DocumentId::from_raw(1));
    rect(&mut design, "Deurbord", 0.0)?;
    let mut deurbord = Project::new(ProjectUuid::from_bytes(*b"burnburn-native1"), (*design.snapshot()).clone());
    deurbord.freeze_revision("Order 4711").map_err(|e| format!("{e:?}"))?;
    rect(&mut design, "Extra paneel", 20.0)?;
    deurbord.document = (*design.snapshot()).clone();
    let deurbord_path = werk.join("deurbord.burnburn");
    save_project(&burnburn_io::std_fs::StdFileSystem, &deurbord_path, &deurbord).map_err(|e| format!("{e:?}"))?;
    let mut other = DocumentSession::new(DocumentId::from_raw(2));
    rect(&mut other, "Onderzetter", 0.0)?;
    save_project(
        &burnburn_io::std_fs::StdFileSystem,
        &werk.join("onderzetters.burnburn"),
        &Project::new(ProjectUuid::from_bytes(*b"burnburn-native2"), (*other.snapshot()).clone()),
    )
    .map_err(|e| format!("{e:?}"))?;

    s.app.dispatch(UiIntent::Open { path: deurbord_path });
    s.wait(5.0, "the project window", || s.app.open_documents().len() == 1)?;
    let doc = s.app.open_documents()[0];
    expect(|| "no search panel before the menu item".into(), !s.app.g5_panel_shown(doc, P::Search))?;
    s.menu(MenuItem::Command(CommandKey::SearchProjects))?;
    s.wait(5.0, "the search panel", || s.app.g5_panel_shown(doc, P::Search))?;
    s.wait(5.0, "the folder to be read", || {
        s.doc(doc).and_then(|d| d.files.search).is_some_and(|f| f.scanned == Some(2))
    })?;
    s.pump(0.2);
    let search = s.doc(doc).and_then(|d| d.files.search).ok_or("no search view")?;
    expect(
        || format!("the folder of the open project, got {}", search.folder),
        search.folder == werk.display().to_string(),
    )?;
    // A query narrows the list to the project whose frozen revision carries the label.
    panel_control(&s, doc, P::Search, C::SearchQuery)?;
    type_into_search(&s, doc, C::SearchQuery, "4711")?;
    let hits = s.doc(doc).and_then(|d| d.files.search).map_or(0, |f| f.results.len());
    expect(|| format!("one hit for the revision label, got {hits}"), hits == 1)?;
    let labels = s.app.g5_labels(doc, P::Search).join(" | ");
    expect(|| format!("the panel says what matched, got {labels}"), labels.contains("revisie"))?;

    // Open the old revision: a window of its own next to the design, which is untouched.
    let before = s.app.open_documents().len();
    press_search(&s, doc, C::SearchOpenRevision)?;
    s.wait(5.0, "the revision window", || s.app.open_documents().len() == before + 1)?;
    let opened = s.app.open_documents().into_iter().find(|d| *d != doc).ok_or("no second window")?;
    let revision = s.doc(opened).ok_or("no revision document")?;
    expect(
        || {
            format!(
                "the design as it was frozen, got {:?}",
                revision.objects.iter().map(|o| &o.label).collect::<Vec<_>>()
            )
        },
        revision.objects.len() == 1,
    )?;
    expect(|| format!("title names the revision, got {}", revision.title), revision.title.contains("Order 4711"))?;
    expect(
        || "the current design keeps both objects".into(),
        s.doc(doc).is_some_and(|d| d.objects.len() == 2 && !d.dirty),
    )?;
    press_search(&s, doc, C::SearchClose)?;
    expect(|| "Sluit closes the panel".into(), !s.app.g5_panel_shown(doc, P::Search))?;
    Ok(())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn type_into_search(
    s: &common::Session,
    doc: burnburn_types::DocumentId,
    c: burnburn_macos::panels::G5Control,
    text: &str,
) -> Result<(), String> {
    use objc2::Message;
    use objc2_foundation::NSString;
    let view = panel_control(s, doc, burnburn_macos::panels::G5Panel::Search, c)?;
    let any: &objc2::runtime::AnyObject = view.as_ref();
    let field = any.downcast_ref::<objc2_app_kit::NSTextField>().ok_or("not a field")?.retain();
    field.setStringValue(&NSString::from_str(text));
    // SAFETY: sending the field's own action as AppKit does after Return.
    unsafe { field.sendAction_to(field.action(), field.target().as_deref()) };
    s.pump(0.2);
    Ok(())
}

#[cfg(target_os = "macos")]
fn press_search(
    s: &common::Session,
    doc: burnburn_types::DocumentId,
    c: burnburn_macos::panels::G5Control,
) -> Result<(), String> {
    let view = panel_control(s, doc, burnburn_macos::panels::G5Panel::Search, c)?;
    click(&view)?;
    s.pump(0.3);
    Ok(())
}
