//! LF17, native (quiet windows): the empty states and the progress row of the AppKit host. The empty
//! object list shows the symbol, the sentence and a button that sends the model's intent; the layer
//! table, the history, the libraries, the bundle sheet, the hub, the remnants and the recognition
//! panel say so when they have nothing; the progress row shows a bar or a spinner and its cancel
//! button sends the cancel intent. What the models decide is tested headlessly
//! (`burnburn-host-models/tests/lf17_*`, `burnburn-presentation/tests/lf17_task_progress.rs`).

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("lf17_empty_progress: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    for (name, scenario) in [
        (
            "the_empty_object_list_shows_the_symbol_and_its_button_draws_a_rectangle",
            objects as fn() -> Result<(), String>,
        ),
        ("the_empty_layer_table_says_so_and_leaves_when_a_layer_exists", operations),
        ("the_empty_history_replaces_the_lone_start_line", history),
        ("the_libraries_and_the_bundle_sheet_say_they_are_empty", libraries),
        ("the_hub_the_remnants_and_the_recognition_panel_say_they_are_empty", panels),
        ("the_progress_row_shows_a_bar_or_a_spinner_and_cancel_sends_the_intent", progress),
    ] {
        let code = common::report(name, scenario());
        if code != ExitCode::SUCCESS {
            return code;
        }
    }
    ExitCode::SUCCESS
}

/// The visible text of every label in `view` and below.
#[cfg(target_os = "macos")]
fn visible_texts(view: &objc2_app_kit::NSView, out: &mut Vec<String>) {
    use objc2_app_kit::NSTextField;
    if let Some(field) = view.downcast_ref::<NSTextField>()
        && !view.isHiddenOrHasHiddenAncestor()
    {
        out.push(field.stringValue().to_string());
    }
    for sub in view.subviews().iter() {
        visible_texts(&sub, out);
    }
}

#[cfg(target_os = "macos")]
fn window_texts(window: &objc2_app_kit::NSWindow) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(content) = window.contentView() {
        visible_texts(&content, &mut out);
    }
    out
}

#[cfg(target_os = "macos")]
fn objects() -> Result<(), String> {
    use burnburn_presentation::{CommandKey, UiIntent};
    use common::expect;

    let s = common::start()?;
    let doc = s.new_document()?;
    let probe = s.app.objects_empty_state(doc).ok_or("no window")?;
    expect(|| "the empty object list shows its state".into(), probe.shown)?;
    expect(|| format!("symbol {:?}", probe.symbol), probe.symbol == "square.dashed")?;
    expect(|| "the symbol is drawn".into(), probe.has_image)?;
    expect(|| format!("sentence {:?}", probe.text), probe.text.starts_with("Nog geen objecten."))?;
    expect(
        || format!("button {:?}", probe.button_title),
        probe.button_title.as_deref() == Some("Teken een rechthoek"),
    )?;
    s.app.take_dispatched();
    expect(|| "the button is there to press".into(), probe.click())?;
    s.pump(0.1);
    let sent = s.app.take_dispatched();
    expect(
        || format!("the button sends the draw-a-rectangle intent, got {sent:?}"),
        sent.contains(&UiIntent::Invoke { document: Some(doc), command: CommandKey::ToolRectangle }),
    )?;
    // With an object the list is not empty any more.
    s.drag_mm(doc, (0.0, 0.0), (40.0, 20.0))?;
    let probe = s.app.objects_empty_state(doc).ok_or("no window")?;
    expect(|| "an object takes the state away".into(), !probe.shown)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn operations() -> Result<(), String> {
    use burnburn_presentation::{SelectMode, UiIntent};
    use common::expect;

    let s = common::start()?;
    let doc = s.new_document()?;
    let probe = s.app.operations_empty_state(doc).ok_or("no window")?;
    expect(|| "the empty layer table shows its state".into(), probe.shown)?;
    expect(|| format!("sentence {:?}", probe.text), probe.text.starts_with("Nog geen bewerkingen."))?;
    // The block sits below the column headers, never over them (seen on the LF-E snapshot).
    let header = s
        .app
        .with_window(doc, |w| {
            w.operations_table.headerView().map(|h| {
                let r = h.convertRect_toView(h.bounds(), None);
                [r.origin.x, r.origin.y, r.size.width, r.size.height]
            })
        })
        .flatten()
        .ok_or("no header")?;
    let b = probe.block_in_window;
    let overlaps = b[0] < header[0] + header[2]
        && header[0] < b[0] + b[2]
        && b[1] < header[1] + header[3]
        && header[1] < b[1] + b[3];
    expect(|| format!("empty block {b:?} overlaps the column headers {header:?}"), !overlaps)?;
    s.draw_rectangle(doc, (0.0, 0.0), (40.0, 20.0))?;
    let id = s.doc(doc).ok_or("no document")?.objects[0].id;
    s.app.dispatch(UiIntent::Select { document: doc, ids: vec![id], mode: SelectMode::Replace });
    s.app.dispatch(UiIntent::AssignLayer { document: doc, slot: 1, add: false });
    s.pump(0.1);
    let probe = s.app.operations_empty_state(doc).ok_or("no window")?;
    expect(|| "a layer takes the state away".into(), !probe.shown)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn history() -> Result<(), String> {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_macos::app_menu::AppAction;
    use common::expect;

    let s = common::start()?;
    let doc = s.new_document()?;
    s.menu(MenuItem::App(AppAction::History))?;
    let probe = s.app.history_empty_state().ok_or("no history window")?;
    expect(|| "no step yet: the state shows".into(), probe.shown)?;
    expect(|| format!("sentence {:?}", probe.text), probe.text.starts_with("Nog geen stappen."))?;
    let rows = s.app.history_widgets().map_or(-1, |(_, t)| t.numberOfRows());
    expect(|| format!("no lone Start line under the state, got {rows} rows"), rows == 0)?;
    s.draw_rectangle(doc, (0.0, 0.0), (40.0, 20.0))?;
    let probe = s.app.history_empty_state().ok_or("no history window")?;
    expect(|| "the first step takes the state away".into(), !probe.shown)?;
    let rows = s.app.history_widgets().map_or(-1, |(_, t)| t.numberOfRows());
    expect(|| format!("the list is back, got {rows} rows"), rows == 2)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn libraries() -> Result<(), String> {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_presentation::CommandKey;
    use common::expect;

    let s = common::start()?;
    let doc = s.new_document()?;
    s.menu(MenuItem::Command(CommandKey::ShowMaterialLibrary))?;
    s.wait(5.0, "the empty material library", || s.app.library_empty_texts(doc)[0].is_some())?;
    let text = s.app.library_empty_texts(doc)[0].clone().unwrap_or_default();
    expect(|| format!("material sentence {text:?}"), text.starts_with("De materiaalbibliotheek is leeg."))?;
    s.menu(MenuItem::Command(CommandKey::ShowArtLibrary))?;
    s.wait(5.0, "the empty art library", || s.app.library_empty_texts(doc)[1].is_some())?;
    let text = s.app.library_empty_texts(doc)[1].clone().unwrap_or_default();
    expect(|| format!("art sentence {text:?}"), text.starts_with("De kunstbibliotheek is leeg."))?;
    s.menu(MenuItem::Command(CommandKey::ExportBundle))?;
    s.wait(5.0, "the empty bundle sheet", || s.app.library_empty_texts(doc)[2].is_some())?;
    let text = s.app.library_empty_texts(doc)[2].clone().unwrap_or_default();
    expect(|| format!("bundle sentence {text:?}"), text.starts_with("Er is niets om te bundelen."))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn panels() -> Result<(), String> {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_macos::app_menu::AppAction;
    use common::expect;

    let s = common::start()?;
    let doc = s.new_document()?;
    s.menu(MenuItem::App(AppAction::MachineHub))?;
    let hub = s.app.machine_hub_window().ok_or("no hub window")?;
    let texts = window_texts(&hub);
    for sentence in ["Nog geen machine ingesteld.", "Er staat niets in de wachtrij.", "Nog niets"] {
        // The tabs hold one list each; the open tab shows its own, the rest are checked by the
        // headless form test. Here: the device tab (the first) says so.
        if sentence == "Nog geen machine ingesteld." {
            expect(
                || format!("the device tab says there is no machine, got {texts:?}"),
                texts.iter().any(|t| t.starts_with(sentence)),
            )?;
        }
    }
    s.menu(MenuItem::RemnantsPanel)?;
    s.pump(0.2);
    let all = all_window_texts();
    expect(
        || format!("the remnants panel says none are kept, got {all:?}"),
        all.iter().any(|t| t.starts_with("Nog geen restplaten bewaard.")),
    )?;
    s.menu(MenuItem::RecognitionPanel)?;
    s.pump(0.2);
    let all = all_window_texts();
    expect(
        || format!("the recognition panel says there is no template, got {all:?}"),
        all.iter().any(|t| t.starts_with("Nog geen sjabloon om te herkennen.")),
    )?;
    let _ = doc;
    Ok(())
}

#[cfg(target_os = "macos")]
fn all_window_texts() -> Vec<String> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    let Some(mtm) = MainThreadMarker::new() else { return Vec::new() };
    let mut out = Vec::new();
    for window in NSApplication::sharedApplication(mtm).windows().iter() {
        out.extend(window_texts(&window));
    }
    out
}

#[cfg(target_os = "macos")]
fn progress() -> Result<(), String> {
    use burnburn_host_models::empty_state::{EmptyAction, ProgressRow};
    use burnburn_presentation::UiIntent;
    use burnburn_types::{StableId, TaskId};
    use common::expect;

    let s = common::start()?;
    let doc = s.new_document()?;
    let idle = s.app.progress_row(doc).ok_or("no window")?;
    expect(|| "no task: no progress row".into(), !idle.shown)?;

    let task = TaskId::from_raw(5);
    let row = |fraction| ProgressRow {
        task,
        title: "Plan berekenen…".into(),
        fraction,
        cancel: Some(EmptyAction { label: "Annuleer".into(), intent: UiIntent::CancelTask { document: doc, task } }),
    };
    s.app.show_progress_rows(doc, &[row(None)]);
    let p = s.app.progress_row(doc).ok_or("no window")?;
    expect(|| "a running task shows the row".into(), p.shown)?;
    expect(|| format!("title {:?}", p.title), p.title == "Plan berekenen…")?;
    expect(|| "no progress reported: indeterminate".into(), p.indeterminate)?;
    expect(|| "a compile can be cancelled: the button shows".into(), p.cancel_shown)?;
    s.app.show_progress_rows(doc, &[row(Some(0.4))]);
    let p = s.app.progress_row(doc).ok_or("no window")?;
    expect(
        || format!("determinate at 40 %, got {} indeterminate {}", p.value, p.indeterminate),
        !p.indeterminate && (p.value - 0.4).abs() < 1e-9,
    )?;
    s.app.take_dispatched();
    p.click_cancel();
    s.pump(0.1);
    let sent = s.app.take_dispatched();
    expect(
        || format!("cancel sends the cancel intent for that task, got {sent:?}"),
        sent.contains(&UiIntent::CancelTask { document: doc, task }),
    )?;
    // A task that cannot be cancelled has no button.
    s.app.show_progress_rows(doc, &[ProgressRow { cancel: None, ..row(None) }]);
    let p = s.app.progress_row(doc).ok_or("no window")?;
    expect(|| "no cancel button for a task that finishes what it started".into(), p.shown && !p.cancel_shown)?;
    s.app.show_progress_rows(doc, &[]);
    let p = s.app.progress_row(doc).ok_or("no window")?;
    expect(|| "no task left: the row is gone".into(), !p.shown)?;
    Ok(())
}
