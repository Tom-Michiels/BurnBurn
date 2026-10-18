//! The object list is an NSTableView fed from the view model: rows follow the document, the
//! selection follows presentation, and a user selection in the table becomes one Select intent.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("native_object_list: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("native_object_list_follows_view_model_and_selects", scenario())
}

#[cfg(target_os = "macos")]
fn scenario() -> Result<(), String> {
    use burnburn_presentation::{FieldKey, SelectMode, UiIntent};
    use common::expect;
    use objc2::Message;
    use objc2_foundation::NSIndexSet;

    let s = common::start()?;
    let doc = s.new_document()?;
    s.draw_rectangle(doc, (0.0, 0.0), (120.0, 80.0))?;
    s.draw_rectangle(doc, (150.0, 0.0), (200.0, 30.0))?;
    let rows = s.app.with_window(doc, |w| w.table.numberOfRows()).unwrap_or(0);
    expect(|| format!("two rows, got {rows}"), rows == 2)?;
    let selected = s.app.with_window(doc, |w| w.table.selectedRow()).unwrap_or(-1);
    expect(|| format!("the new rectangle (row 1) is selected, got {selected}"), selected == 1)?;
    s.app.take_dispatched();

    // The user clicks row 0.
    let table = s.app.with_window(doc, |w| w.table.retain()).ok_or("no table")?;
    table.selectRowIndexes_byExtendingSelection(&NSIndexSet::indexSetWithIndex(0), false);
    s.pump(0.1);
    let sent = s.app.take_dispatched();
    let first = s.doc(doc).ok_or("no document")?.objects[0].id;
    expect(
        || format!("expected one Select of row 0, got {sent:?}"),
        sent == [UiIntent::Select { document: doc, ids: vec![first], mode: SelectMode::Replace }],
    )?;
    let width = s.doc(doc).and_then(|d| d.inspector.and_then(|i| i.field(FieldKey::Width).map(|f| f.text.clone())));
    expect(|| format!("inspector shows row 0's width, got {width:?}"), width.as_deref() == Some("120"))?;
    let shown = s
        .app
        .with_window(doc, |w| {
            w.field(burnburn_presentation::FieldKey::Width).map(|f| f.stringValue().to_string()).unwrap_or_default()
        })
        .unwrap_or_default();
    expect(|| format!("field shows 120, got {shown:?}"), shown == "120")?;
    Ok(())
}
