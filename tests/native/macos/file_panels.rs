//! The file panels natively (W3-UI-bestanden): the Material Library panel, the export dialog, the
//! bundle sheet, the PDF page choice of the import sheet and the template menus, on a real window
//! with the real controller. File panels (NSOpenPanel / NSSavePanel) run in another process and
//! cannot be answered from a test: their completions are driven through the same intents they
//! send (manual: pressing OK in them).

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("file_panels: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("file_panels_on_a_real_window", scenario())
}

/// A two-page PDF (100 × 50 pt box on page 1, a 100 pt line on page 2), built by hand.
#[cfg(target_os = "macos")]
fn two_page_pdf() -> Vec<u8> {
    let stream = |data: &str| format!("<< /Length {} >>\nstream\n{data}\nendstream", data.len()).into_bytes();
    let objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 50] /Contents 4 0 R >>".to_vec(),
        stream("0 0 40 20 re S"),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 100] /Contents 6 0 R >>".to_vec(),
        stream("0 0 m 100 0 l S"),
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for o in &offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes(),
    );
    out
}

/// Send the control's own action to its own target, as a click or a committed field does.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn act(control: &objc2_app_kit::NSControl) {
    // SAFETY: the control's own action to its own target.
    unsafe { control.sendAction_to(control.action(), control.target().as_deref()) };
}

/// Click a button as the user does.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn click(button: &objc2_app_kit::NSButton) {
    // SAFETY: clicking our own button with no sender, as the user does.
    unsafe { button.performClick(None) };
}

#[cfg(target_os = "macos")]
fn type_into(field: &objc2_app_kit::NSTextField, text: &str) {
    field.setStringValue(&objc2_foundation::NSString::from_str(text));
    act(field);
}

#[cfg(target_os = "macos")]
fn titles(popup: &objc2_app_kit::NSPopUpButton) -> Vec<String> {
    popup.itemTitles().iter().map(|t| t.to_string()).collect()
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn scenario() -> Result<(), String> {
    use burnburn_macos::adapter::{MenuItem, NativeEvent};
    use burnburn_presentation::{CommandKey, FilesIntent, OperationField, SelectMode, UiIntent};
    use common::expect;
    use objc2::Message;

    let s = common::start()?;
    let doc = s.new_document()?;

    // ---- Menus: the new Archief and Venster items.
    for (item, title) in [
        (MenuItem::Command(CommandKey::SaveAsTemplate), "Bewaar als template…"),
        (MenuItem::Command(CommandKey::NewFromTemplate), "Nieuw van template…"),
        (MenuItem::Command(CommandKey::ExportSvg), "SVG…"),
        (MenuItem::Command(CommandKey::ExportDxf), "DXF…"),
        (MenuItem::Command(CommandKey::ExportBundle), "Bundel exporteren…"),
        (MenuItem::Command(CommandKey::ImportBundle), "Bundel importeren…"),
        (MenuItem::Command(CommandKey::ShowMaterialLibrary), "Materiaalbibliotheek"),
        (MenuItem::Command(CommandKey::ShowArtLibrary), "Kunstbibliotheek"),
    ] {
        let state = s.menu_item_state(item);
        expect(|| format!("{title}: {state:?}"), state == Some((true, title.to_owned())))?;
    }

    // ---- A layer with settings, then the Material Library panel.
    s.draw_rectangle(doc, (0.0, 0.0), (40.0, 20.0))?;
    s.menu(MenuItem::Command(CommandKey::ToolSelect))?;
    let id = s.doc(doc).ok_or("no document")?.objects[0].id;
    s.app.dispatch(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    s.app.dispatch(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    let op = s.doc(doc).ok_or("no document")?.operations[0].id;
    s.app.dispatch(UiIntent::SetOperationField {
        document: doc,
        operation: op,
        field: OperationField::Speed,
        text: "12".into(),
    });

    s.menu(MenuItem::Command(CommandKey::ShowMaterialLibrary))?;
    s.wait(5.0, "the material panel", || {
        s.app
            .with_window(doc, |w| {
                w.files.material_panel.is_some()
                    && !w.files.material_status.stringValue().to_string().starts_with("De bibliotheek")
            })
            .unwrap_or(false)
    })?;
    let checked = s.menu_item_checked(MenuItem::Command(CommandKey::ShowMaterialLibrary));
    expect(|| format!("the menu item is checked while the panel is up: {checked:?}"), checked == Some(true))?;
    let status = s.app.with_window(doc, |w| w.files.material_status.stringValue().to_string()).unwrap_or_default();
    expect(
        || format!("without a folder the panel says changes are not saved: {status:?}"),
        status == "Wijzigingen worden niet in een bestand bewaard.",
    )?;
    let (inputs, save, layer) = s
        .app
        .with_window(doc, |w| {
            (
                w.files.material_inputs.map(|f| f.retain()),
                w.files.material_save.retain(),
                w.files.material_layer.retain(),
            )
        })
        .ok_or("no window")?;
    let layer_titles = titles(&layer);
    expect(
        || format!("one layer, set by hand: {layer_titles:?}"),
        layer_titles.len() == 1 && layer_titles[0].ends_with("handmatig ingesteld"),
    )?;
    type_into(&inputs[0], "Snijden");
    type_into(&inputs[1], "Berken");
    type_into(&inputs[2], "3");
    expect(|| "Bewaar laag als item is enabled".into(), save.isEnabled())?;
    click(&save);
    s.pump(0.3);
    let (entries, detail, apply) = s
        .app
        .with_window(doc, |w| {
            (
                titles(w.files.material_entries),
                w.files.material_detail.stringValue().to_string(),
                w.files.material_apply.retain(),
            )
        })
        .ok_or("no window")?;
    expect(|| format!("entries {entries:?}"), entries == ["Snijden — Berken 3 mm (r1)"])?;
    expect(|| format!("detail {detail:?}"), detail == "Lijn · 12 mm/s · 20 % · 1 pass(es)")?;
    expect(|| "Toepassen is enabled for the chosen layer".into(), apply.isEnabled())?;
    click(&apply);
    s.pump(0.3);
    let layer_titles = titles(&layer);
    expect(
        || format!("the layer now says it was copied from the library: {layer_titles:?}"),
        layer_titles[0].contains("bibliotheekitem 1: actueel"),
    )?;
    s.wait_on_screen(doc)?;
    s.app.with_window(doc, |w| common::screenshot(w.window));
    s.menu(MenuItem::Command(CommandKey::ShowMaterialLibrary))?;
    s.pump(0.2);
    let hidden = s.app.with_window(doc, |w| w.files.material_panel.is_none()).unwrap_or(false);
    expect(|| "the menu item closes the panel".into(), hidden)?;

    // ---- Export dialog: options and the (empty) loss report.
    s.menu(MenuItem::Command(CommandKey::ExportSvg))?;
    s.wait(5.0, "the export sheet", || {
        s.app
            .with_window(doc, |w| {
                w.files.export_sheet.is_some()
                    && w.window.attachedSheet().is_some()
                    && w.files.export_losses.string().to_string() == "Er gaat niets verloren."
            })
            .unwrap_or(false)
    })?;
    let (title, objects, unit, primary, ack_hidden) = s
        .app
        .with_window(doc, |w| {
            (
                w.files.export_title.stringValue().to_string(),
                w.files.export_objects.stringValue().to_string(),
                w.files.export_unit.retain(),
                w.files.export_primary.retain(),
                w.files.export_acknowledge.isHidden(),
            )
        })
        .ok_or("no window")?;
    expect(
        || format!("title {title:?} objects {objects:?}"),
        title == "Exporteer als SVG" && objects == "1 objecten worden geëxporteerd.",
    )?;
    expect(|| "nothing is lost, so no acknowledgement is asked".into(), ack_hidden && primary.isEnabled())?;
    unit.selectItemAtIndex(1);
    act(&unit);
    s.pump(0.3);
    let chosen = s.app.with_window(doc, |w| w.files.export_unit.titleOfSelectedItem().map(|t| t.to_string())).flatten();
    expect(|| format!("the unit choice is kept: {chosen:?}"), chosen.as_deref() == Some("inch"))?;
    s.app.dispatch(UiIntent::Files(FilesIntent::CloseExport { document: doc }));
    s.wait(3.0, "the export sheet to close", || {
        s.app.with_window(doc, |w| w.window.attachedSheet().is_none()).unwrap_or(false)
    })?;

    // ---- Bundle sheet: the material entry can be ticked, then Exporteer… is enabled.
    s.menu(MenuItem::Command(CommandKey::ExportBundle))?;
    s.wait(5.0, "the bundle sheet", || {
        s.app
            .with_window(doc, |w| w.files.bundle_sheet.is_some() && w.window.attachedSheet().is_some())
            .unwrap_or(false)
    })?;
    s.pump(0.3);
    let (kind0, primary) = s
        .app
        .with_window(doc, |w| (w.files.bundle_kinds[0].retain(), w.files.bundle_primary.retain()))
        .ok_or("no sheet")?;
    let title0 = kind0.title().to_string();
    expect(|| format!("materials row {title0:?}"), title0 == "Materiaalinstellingen (0 van 1)")?;
    expect(|| "Exporteer… waits for a choice".into(), !primary.isEnabled())?;
    click(&kind0);
    s.pump(0.3);
    expect(|| "Exporteer… is enabled once something is chosen".into(), primary.isEnabled())?;
    s.app.dispatch(burnburn_macos::panels_files::bundle_close_intent(doc));
    s.wait(3.0, "the bundle sheet to close", || {
        s.app.with_window(doc, |w| w.window.attachedSheet().is_none()).unwrap_or(false)
    })?;

    // ---- Import: a PDF with two pages offers the page choice.
    let pdf = s.dir.join("twee.pdf");
    std::fs::write(&pdf, two_page_pdf()).map_err(|e| e.to_string())?;
    s.app.native(NativeEvent::ImportFileChosen { document: doc, path: Some(pdf) });
    s.wait(5.0, "the import sheet", || {
        s.app.with_window(doc, |w| w.import_sheet.is_some() && w.window.attachedSheet().is_some()).unwrap_or(false)
    })?;
    s.pump(0.3);
    let (pages, commit, units_hidden) = s
        .app
        .with_window(doc, |w| (w.import_pages.retain(), w.import_commit.retain(), w.import_units.isHidden()))
        .ok_or("no sheet")?;
    let page_titles = titles(&pages);
    expect(
        || format!("pages {page_titles:?}"),
        page_titles == ["Pagina 1 — 35,3 × 17,6 mm", "Pagina 2 — 70,6 × 35,3 mm", "Alle pagina's naast elkaar"],
    )?;
    expect(|| "a PDF has no unit question".into(), units_hidden)?;
    pages.selectItemAtIndex(2);
    act(&pages);
    s.wait(5.0, "both pages", || s.doc(doc).and_then(|d| d.import.map(|i| i.items == 2 && !i.busy)).unwrap_or(false))?;
    expect(|| "Importeer is enabled".into(), commit.isEnabled())?;
    let before = s.doc(doc).map(|d| d.objects.len()).unwrap_or(0);
    click(&commit);
    s.wait(3.0, "the sheet to close", || {
        s.app.with_window(doc, |w| w.window.attachedSheet().is_none()).unwrap_or(false)
    })?;
    let after = s.doc(doc).map(|d| d.objects.len()).unwrap_or(0);
    expect(|| format!("two imported objects: {before} → {after}"), after == before + 2)?;

    // ---- Templates: the controller reports a saved template, the menu offers it again.
    let template = s.dir.join("plaat.burnburntemplate");
    s.app.dispatch(UiIntent::Files(FilesIntent::SaveAsTemplate {
        document: doc,
        path: template.clone(),
        scope: burnburn_presentation::TemplateScope::Everything,
    }));
    s.wait(5.0, "the template file", || template.exists())?;
    s.pump(0.3);
    let item = s.menu_item_state(MenuItem::RecentTemplate(0));
    expect(|| format!("recent template {item:?}"), item == Some((true, "plaat.burnburntemplate".to_owned())))?;
    let windows = s.app.open_documents().len();
    s.menu(MenuItem::RecentTemplate(0))?;
    s.wait(5.0, "the window made from the template", || s.app.open_documents().len() == windows + 1)?;
    Ok(())
}
