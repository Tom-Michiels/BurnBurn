//! The golf-5 panels in the real window (G5-native): Archief ▸ Notities tonen shows the note panel
//! and saving it is one undo step; Venster ▸ Offset en reparatie previews and applies an offset with
//! the chosen options; Venster ▸ Structuur chooses a variant, types a local override and picks the
//! plate Compile uses; an open centre-line trace shows its junction decisions.
//!
//! Manual (not covered here): how the panels look next to the window in light and dark, scrolling
//! of a tall structure panel, and the repair before/after with a real open contour (the headless
//! test `tests/g5_panels.rs` covers its model; drawing a gap with the pen needs a pixel-exact hand).

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(target_os = "macos")]
#[path = "../../../crates/burnburn-macos/tests/g5_fixtures/mod.rs"]
mod g5_fixtures;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("g5_panels: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    let scenarios: [(&str, fn() -> Result<(), String>); 5] = [
        ("notes_panel_saves_one_undo_step", notes),
        ("offset_panel_previews_and_applies_with_the_options", offset),
        ("structure_panel_chooses_variant_override_and_plate", structure),
        ("centerline_panel_decides_a_junction", centerline),
        ("trace_prep_panel_adds_a_mask_and_a_local_threshold", trace_prep),
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
    panel: burnburn_macos::panels::G5Panel,
    c: burnburn_macos::panels::G5Control,
) -> Result<objc2::rc::Retained<objc2_app_kit::NSView>, String> {
    s.app.g5_control(doc, panel, c).ok_or(format!("no control {c:?} in {panel:?}"))
}

#[cfg(target_os = "macos")]
fn press(
    s: &common::Session,
    doc: burnburn_types::DocumentId,
    panel: burnburn_macos::panels::G5Panel,
    c: burnburn_macos::panels::G5Control,
) -> Result<(), String> {
    let view = control(s, doc, panel, c)?;
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
    panel: burnburn_macos::panels::G5Panel,
    c: burnburn_macos::panels::G5Control,
    text: &str,
) -> Result<(), String> {
    use objc2::Message;
    use objc2_foundation::NSString;
    let view = control(s, doc, panel, c)?;
    let any: &objc2::runtime::AnyObject = view.as_ref();
    let field = any.downcast_ref::<objc2_app_kit::NSTextField>().ok_or("not a field")?.retain();
    field.setStringValue(&NSString::from_str(text));
    // SAFETY: sending the field's own action as AppKit does after Return.
    unsafe { field.sendAction_to(field.action(), field.target().as_deref()) };
    s.pump(0.1);
    Ok(())
}

/// A user chooses item `index` of a pop-up of the panel.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn choose(
    s: &common::Session,
    doc: burnburn_types::DocumentId,
    panel: burnburn_macos::panels::G5Panel,
    c: burnburn_macos::panels::G5Control,
    index: isize,
) -> Result<(), String> {
    use objc2::Message;
    let view = control(s, doc, panel, c)?;
    let any: &objc2::runtime::AnyObject = view.as_ref();
    let popup = any.downcast_ref::<objc2_app_kit::NSPopUpButton>().ok_or("not a pop-up")?.retain();
    popup.selectItemAtIndex(index);
    // SAFETY: sending the pop-up's own action as AppKit does after a choice.
    unsafe { popup.sendAction_to(popup.action(), popup.target().as_deref()) };
    s.pump(0.1);
    Ok(())
}

#[cfg(target_os = "macos")]
fn labels(s: &common::Session, doc: burnburn_types::DocumentId, panel: burnburn_macos::panels::G5Panel) -> Vec<String> {
    s.app.g5_labels(doc, panel)
}

#[cfg(target_os = "macos")]
fn notes() -> Result<(), String> {
    use burnburn_document::UndoLabel;
    use burnburn_document::process::ProcessLabel;
    use burnburn_macos::adapter::MenuItem;
    use burnburn_macos::panels::{G5Control as C, G5Panel as P};
    use burnburn_presentation::CommandKey;
    use common::expect;
    use objc2::Message;
    use objc2_foundation::NSString;

    let s = common::start()?;
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (10.0, 10.0), (30.0, 30.0))?;
    expect(|| "no note panel before Notities tonen".into(), !s.app.g5_panel_shown(doc, P::Notes))?;
    s.menu(MenuItem::Command(CommandKey::ShowNotes))?;
    s.pump(0.2);
    expect(|| "the note panel is shown".into(), s.app.g5_panel_shown(doc, P::Notes))?;
    expect(
        || "the menu item shows a check mark".into(),
        s.menu_item_checked(MenuItem::Command(CommandKey::ShowNotes)) == Some(true),
    )?;

    let view = control(&s, doc, P::Notes, C::NotesText)?;
    let any: &objc2::runtime::AnyObject = view.as_ref();
    let text = any.downcast_ref::<objc2_app_kit::NSTextView>().ok_or("the note is not a text view")?.retain();
    text.setString(&NSString::from_str("Klant Jansen"));
    press(&s, doc, P::Notes, C::NotesSave)?;
    let vm = s.doc(doc).ok_or("no document")?;
    expect(
        || format!("the note is stored, got {:?}", vm.notes),
        vm.notes.as_ref().is_some_and(|n| n.text == "Klant Jansen"),
    )?;
    expect(
        || format!("one undo step, got {:?}", vm.undo),
        vm.undo == Some(UndoLabel::Process(ProcessLabel::SetNotes)),
    )?;
    expect(|| "the window is marked changed".into(), vm.dirty)?;

    press(&s, doc, P::Notes, C::NotesClose)?;
    s.pump(0.1);
    expect(|| "the panel is closed".into(), !s.app.g5_panel_shown(doc, P::Notes))?;
    expect(|| "the document keeps the note".into(), s.doc(doc).is_some_and(|d| d.notes.is_none() && d.dirty))?;
    Ok(())
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn offset() -> Result<(), String> {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_macos::panels::{G5Control as C, G5Panel as P};
    use burnburn_presentation::{CommandKey, OffsetJoin, UiIntent};
    use common::expect;
    use objc2::Message;

    let s = common::start()?;
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (10.0, 10.0), (30.0, 30.0))?;
    s.app.dispatch(UiIntent::Invoke { document: Some(doc), command: CommandKey::SelectAll });
    s.pump(0.1);
    s.menu(MenuItem::OffsetPanel)?;
    s.pump(0.2);
    expect(|| "the offset panel is shown".into(), s.app.g5_panel_shown(doc, P::Offset))?;

    type_into(&s, doc, P::Offset, C::OffsetDistance, "2 mm")?;
    press(&s, doc, P::Offset, C::OffsetPreview)?;
    s.wait(3.0, "the offset preview", || labels(&s, doc, P::Offset).iter().any(|l| l.contains("24 × 24 mm")))?;
    expect(
        || "the preview did not change the document".into(),
        s.doc(doc).is_some_and(|d| d.objects.len() == 1 && d.offset.preview.is_some()),
    )?;

    // Mitred corners: the choice is sent as the option set and the preview is recomputed.
    choose(&s, doc, P::Offset, C::OffsetJoin, 1)?;
    let vm = s.doc(doc).ok_or("no document")?;
    expect(|| format!("mitred, got {:?}", vm.offset.options), vm.offset.options.join == OffsetJoin::Miter)?;

    // "Keep the source" off: the source is replaced by its offset.
    let view = control(&s, doc, P::Offset, C::OffsetKeepSources)?;
    let any: &objc2::runtime::AnyObject = view.as_ref();
    let check = any.downcast_ref::<objc2_app_kit::NSButton>().ok_or("not a check box")?.retain();
    click(&check)?;
    s.pump(0.1);
    expect(|| "keep sources is off".into(), s.doc(doc).is_some_and(|d| !d.offset.options.keep_sources))?;
    press(&s, doc, P::Offset, C::OffsetApply)?;
    let vm = s.doc(doc).ok_or("no document")?;
    expect(|| format!("the source was replaced: {} objects", vm.objects.len()), vm.objects.len() == 1)?;
    expect(|| "the preview is gone".into(), vm.offset.preview.is_none())?;

    // The menu item hides the panel again.
    s.menu(MenuItem::OffsetPanel)?;
    s.pump(0.1);
    expect(|| "the panel is hidden by its menu item".into(), !s.app.g5_panel_shown(doc, P::Offset))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn structure() -> Result<(), String> {
    use burnburn_io::{Project, ProjectUuid, save_project};
    use burnburn_macos::adapter::MenuItem;
    use burnburn_macos::panels::{G5Control as C, G5Panel as P};
    use burnburn_presentation::UiIntent;
    use common::expect;

    let s = common::start()?;
    let (snapshot, [_, _, plain]) = g5_fixtures::structure_snapshot();
    let path = s.dir.join("onderdelen.burnburn");
    save_project(&burnburn_io::std_fs::StdFileSystem, &path, &Project::new(ProjectUuid::from_bytes([3; 16]), snapshot))
        .map_err(|e| format!("{e:?}"))?;
    let before = s.app.open_documents();
    s.app.dispatch(UiIntent::Open { path });
    let mut found = None;
    s.wait(5.0, "the opened project", || {
        found = s.app.open_documents().into_iter().find(|d| !before.contains(d));
        found.is_some_and(|d| s.app.with_window(d, |w| w.window.isVisible()).unwrap_or(false))
    })?;
    let doc = found.ok_or("no window")?;
    s.pump(0.2);

    s.menu(MenuItem::StructurePanel)?;
    s.pump(0.2);
    expect(|| "the structure panel is shown".into(), s.app.g5_panel_shown(doc, P::Structure))?;
    let all = labels(&s, doc, P::Structure);
    expect(|| format!("the plate is listed, got {all:?}"), all.iter().any(|l| l.starts_with("Plaat A:")))?;
    expect(|| format!("the broken reference shows, got {all:?}"), all.iter().any(|l| l.contains("Rand: gebroken")))?;
    expect(
        || format!("the invalid variant fails alone, got {all:?}"),
        all.iter().filter(|l| l.contains("mislukt")).count() == 1,
    )?;

    // The instance without a variant gets "klein" (items: none, kapot, klein).
    choose(&s, doc, P::Structure, C::Variant { instance: plain }, 2)?;
    let variant = |d: &burnburn_presentation::DocumentViewModel| {
        d.structure.parts[0].instances.iter().find(|i| i.id == plain).and_then(|i| i.variant.clone())
    };
    expect(|| "the variant is klein".into(), s.doc(doc).is_some_and(|d| variant(&d).as_deref() == Some("klein")))?;

    // A local override per instance; the part keeps its own value.
    type_into(&s, doc, P::Structure, C::Override { instance: plain, parameter: 0 }, "80 mm")?;
    let vm = s.doc(doc).ok_or("no document")?;
    let row = vm.structure.parts[0].instances.iter().find(|i| i.id == plain).ok_or("no instance")?;
    expect(
        || format!("override stored, got {:?}", row.overrides),
        row.overrides.iter().any(|o| o.name == "breedte" && o.value == "80"),
    )?;
    expect(|| "the part itself is unchanged".into(), vm.structure.parts[0].parameters[0].value == "40")?;

    // The plate for Compile.
    let plate = vm.structure.plates[0].id;
    choose(&s, doc, P::Structure, C::Plate, 1)?;
    expect(|| "the plate is chosen".into(), s.doc(doc).is_some_and(|d| d.structure.compile_plate == Some(plate)))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn centerline() -> Result<(), String> {
    use burnburn_macos::panels::{G5Control as C, G5Panel as P};
    use burnburn_presentation::{JunctionState, TraceMode, UiIntent};
    use common::expect;

    let s = common::start()?;
    let doc = s.new_document()?;
    let path = s.dir.join("t.png");
    std::fs::write(&path, g5_fixtures::t_png()).map_err(|e| e.to_string())?;
    s.app.dispatch(UiIntent::OpenTraceSource { document: doc, path });
    s.wait(5.0, "the tracer", || s.doc(doc).is_some_and(|d| d.tracer.is_some()))?;
    let mut input = s.doc(doc).and_then(|d| d.tracer).ok_or("no tracer")?.form.input;
    input.mode = TraceMode::Centerline;
    s.app.dispatch(UiIntent::SetTraceForm { document: doc, input: Box::new(input) });
    s.wait(5.0, "the centre-line result", || s.doc(doc).is_some_and(|d| d.centerline.is_some()))?;
    s.pump(0.3);
    expect(|| "the decisions panel opens by itself".into(), s.app.g5_panel_shown(doc, P::Centerline))?;
    let node = s.doc(doc).and_then(|d| d.centerline).ok_or("no centre-line")?.junctions[0].node;
    press(&s, doc, P::Centerline, C::Junction { node, candidate: 0 })?;
    expect(
        || "the junction is decided".into(),
        s.doc(doc).and_then(|d| d.centerline).is_some_and(|c| matches!(c.junctions[0].state, JunctionState::Resolved)),
    )?;
    expect(
        || "the pairing buttons are gone".into(),
        s.app.g5_control(doc, P::Centerline, C::Junction { node, candidate: 0 }).is_none(),
    )?;
    Ok(())
}

/// Beeld ▸ Beeld voorbereiden (G6-werk): the item is greyed without a tracer; with one the panel shows,
/// a mask typed into the area fields is added with the chosen kind, and the local threshold button
/// puts the local threshold in force.
#[cfg(target_os = "macos")]
fn trace_prep() -> Result<(), String> {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_macos::panels::{G5Control as C, G5Panel as P};
    use burnburn_presentation::UiIntent;
    use common::expect;

    let s = common::start()?;
    let doc = s.new_document()?;
    let path = s.dir.join("t.png");
    std::fs::write(&path, g5_fixtures::t_png()).map_err(|e| e.to_string())?;
    s.app.dispatch(UiIntent::OpenTraceSource { document: doc, path });
    s.wait(5.0, "the tracer", || s.doc(doc).is_some_and(|d| d.tracer.is_some()))?;
    s.pump(0.3);
    s.menu(MenuItem::TracePrepPanel)?;
    s.pump(0.2);
    expect(|| "the preparation panel is shown".into(), s.app.g5_panel_shown(doc, P::TracePrep))?;

    type_into(&s, doc, P::TracePrep, C::PrepAreaY, "40")?;
    type_into(&s, doc, P::TracePrep, C::PrepAreaHeight, "24")?;
    choose(&s, doc, P::TracePrep, C::PrepMaskKind, 1)?;
    press(&s, doc, P::TracePrep, C::PrepAddMask)?;
    let masks = s.doc(doc).and_then(|d| d.trace_prep).ok_or("no preparation view")?.masks;
    expect(
        || format!("one remove mask 0,40 64 x 24, got {masks:?}"),
        masks.len() == 1
            && masks[0].mode == burnburn_presentation::MaskMode::Remove
            && (masks[0].x, masks[0].y, masks[0].width, masks[0].height) == (0, 40, 64, 24),
    )?;

    type_into(&s, doc, P::TracePrep, C::PrepRadius, "9")?;
    press(&s, doc, P::TracePrep, C::PrepUseLocal)?;
    expect(
        || "the local threshold is in force".into(),
        s.doc(doc).and_then(|d| d.trace_prep).is_some_and(|p| p.threshold.local && p.threshold.radius == "9"),
    )?;
    Ok(())
}
