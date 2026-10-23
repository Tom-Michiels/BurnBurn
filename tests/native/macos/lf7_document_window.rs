//! LF7 (look-and-feel): the document window in the real AppKit, quietly and offscreen (alpha 0,
//! click-through, never active):
//!
//! (a) growing the window from its default size makes only the canvas grow; the object list and
//!     the inspector keep their width;
//! (b) the window has a minimum size, and its frame and pane widths are saved and restored (in a
//!     temporary defaults domain: the user's own defaults are never read or written);
//! (c) the layer table shows the colour swatch and the name, and no column is cut off at the
//!     default width;
//! (d) the status bar shows coordinate, zoom and unit, the coordinate after a mouse move;
//! (e) one customisable `NSToolbar` carries the ribbon groups as toolbar items (no second strip);
//! (f) the status line shows the symbol of its tone next to the colour, the plan has a "Kopieer
//!     id" button (private pasteboard), and glyph-titled buttons of this window are SF Symbols.

#![allow(unsafe_code, deprecated)]

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("lf7_document_window: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("lf7_document_window", scenario::run())
}

#[cfg(target_os = "macos")]
mod scenario {
    use burnburn_macos::adapter::MenuItem;
    use burnburn_presentation::{CommandKey, FieldKey, UiIntent};
    use burnburn_types::DocumentId;
    use objc2::Message;
    use objc2::rc::Retained;
    use objc2_app_kit::{NSAccessibility, NSControl, NSEventType, NSView};
    use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

    use crate::common::windows;
    use crate::common::{Session, expect, start_with};

    type Check = Result<(), String>;

    /// The widths of the three panes of the document window.
    fn panes(s: &Session, doc: DocumentId) -> Result<[f64; 3], String> {
        s.app
            .with_window(doc, |w| {
                [w.sidebar_pane.frame().size.width, w.canvas_pane.frame().size.width, w.inspector_pane.frame().size.width]
            })
            .ok_or_else(|| "no window".to_owned())
    }

    fn set_frame(s: &Session, doc: DocumentId, frame: NSRect) {
        s.app.with_window(doc, |w| w.window.setFrame_display(frame, true));
        s.pump(0.3);
    }

    pub fn run() -> Check {
        let suite = format!("app.burnburn.lf7-test-{}", std::process::id());
        let s = start_with(|o| {
            o.quiet_windows = true;
            o.float_windows = false;
        })?;
        let standard = objc2_foundation::NSUserDefaults::standardUserDefaults();
        let frame_key = NSString::from_str("burnburn.window.frame");
        let before = standard.stringForKey(&frame_key).map(|v| v.to_string());
        expect(|| "no test defaults domain".into(), s.app.use_window_state_suite(&suite))?;
        let result = checks(&s);
        // The user's own defaults are exactly as before; the temporary domain goes away again.
        let after = standard.stringForKey(&frame_key).map(|v| v.to_string());
        objc2_foundation::NSUserDefaults::standardUserDefaults().removePersistentDomainForName(&NSString::from_str(&suite));
        result?;
        expect(|| format!("the user's defaults changed: {before:?} -> {after:?}"), before == after)
    }

    fn checks(s: &Session) -> Check {
        let doc = s.new_document()?;
        resize_grows_only_the_canvas(s, doc)?;
        min_size_and_saved_frame(s, doc)?;
        layer_table(s, doc)?;
        status_bar(s, doc)?;
        toolbar(s, doc)?;
        status_symbol_copy_id_and_symbols(s, doc)?;
        Ok(())
    }

    fn resize_grows_only_the_canvas(s: &Session, doc: DocumentId) -> Check {
        let before = panes(s, doc)?;
        let frame = s.app.with_window(doc, |w| w.window.frame()).ok_or("no window")?;
        set_frame(
            s,
            doc,
            NSRect::new(frame.origin, NSSize::new(frame.size.width + 500.0, frame.size.height)),
        );
        let after = panes(s, doc)?;
        expect(|| format!("object list changed width: {before:?} -> {after:?}"), (after[0] - before[0]).abs() < 1.0)?;
        expect(|| format!("inspector changed width: {before:?} -> {after:?}"), (after[2] - before[2]).abs() < 1.0)?;
        expect(
            || format!("the canvas did not take the 500 pt: {before:?} -> {after:?}"),
            (after[1] - before[1] - 500.0).abs() < 2.0,
        )
    }

    fn min_size_and_saved_frame(s: &Session, doc: DocumentId) -> Check {
        let min = s.app.with_window(doc, |w| w.window.contentMinSize()).ok_or("no window")?;
        expect(|| format!("content min size {min:?} is too small"), min.width >= 900.0 && min.height >= 560.0)?;
        // At the minimum size every pane keeps its range and the canvas column its minimum.
        let frame = s.app.with_window(doc, |w| w.window.frame()).ok_or("no window")?;
        let wanted_content = s
            .app
            .with_window(doc, |w| w.window.frameRectForContentRect(NSRect::new(NSPoint::new(0.0, 0.0), min)).size)
            .ok_or("no window")?;
        set_frame(s, doc, NSRect::new(frame.origin, wanted_content));
        let [sidebar, canvas, inspector] = panes(s, doc)?;
        expect(
            || format!("panes at the minimum size: {sidebar} | {canvas} | {inspector}"),
            sidebar >= 149.0 && inspector >= 239.0 && canvas >= 419.0,
        )?;

        // Frame and sidebar width are saved when they change and restored by the next window.
        let wanted = NSRect::new(NSPoint::new(70.0, 90.0), NSSize::new(1210.0, 760.0));
        set_frame(s, doc, wanted);
        s.app.with_window(doc, |w| w.split.setPosition_ofDividerAtIndex(236.0, 0));
        s.pump(0.3);
        let saved_frame = s.app.with_window(doc, |w| w.window.frame()).ok_or("no window")?;
        let saved_sidebar = panes(s, doc)?[0];
        let second = s.new_document()?;
        let restored = s.app.with_window(second, |w| w.window.frame()).ok_or("no window")?;
        expect(
            || format!("frame not restored: saved {saved_frame:?}, new window {restored:?}"),
            (restored.size.width - saved_frame.size.width).abs() < 1.0
                && (restored.size.height - saved_frame.size.height).abs() < 1.0
                && (restored.origin.x - saved_frame.origin.x).abs() < 1.0
                && (restored.origin.y - saved_frame.origin.y).abs() < 1.0,
        )?;
        let sidebar = panes(s, second)?[0];
        expect(|| format!("sidebar width {sidebar} instead of the saved {saved_sidebar}"), (sidebar - saved_sidebar).abs() < 1.0)?;
        s.app.with_window(second, |w| w.window.close());
        s.pump(0.2);
        Ok(())
    }

    fn layer_table(s: &Session, doc: DocumentId) -> Check {
        s.app.dispatch(UiIntent::SetMachineProfile {
            profile: Box::new(windows::machine_profile()?),
            ruida: burnburn_ruida::RuidaProfile {
                magic: burnburn_ruida::Magic::FIXTURE_0X88,
                overscan: burnburn_ruida::OverscanPolicy::ControllerManaged,
            },
        });
        s.draw_rectangle(doc, (10.0, 10.0), (90.0, 60.0))?;
        s.menu(MenuItem::Command(CommandKey::ToolSelect))?;
        s.app.dispatch(UiIntent::AddOperation { document: doc, draft: windows::cut_draft()? });
        s.pump(0.3);
        // Back to the default size, with the default pane widths: the table must fit there.
        s.app.with_window(doc, |w| w.split.setPosition_ofDividerAtIndex(200.0, 0));
        let default_width = 1400.0;
        let frame = s.app.with_window(doc, |w| w.window.frame()).ok_or("no window")?;
        let content = s
            .app
            .with_window(doc, |w| w.window.contentRectForFrameRect(w.window.frame()).size)
            .ok_or("no window")?;
        set_frame(
            s,
            doc,
            NSRect::new(frame.origin, NSSize::new(frame.size.width + default_width - content.width, frame.size.height)),
        );
        let report = s
            .app
            .with_window(doc, |w| {
                let table = w.operations_table;
                let visible = table.enclosingScrollView().map_or(0.0, |sv| sv.contentSize().width);
                let spacing = table.intercellSpacing().width;
                let columns = table.tableColumns();
                let mut total = 0.0;
                let mut problems = Vec::new();
                for (i, column) in columns.iter().enumerate() {
                    total += column.width() + spacing;
                    let header = column.headerCell();
                    let need = header.cellSize().width;
                    if need > column.width() + 0.5 {
                        problems.push(format!("header {:?} needs {need:.0} pt in a {:.0} pt column", header.stringValue(), column.width()));
                    }
                    if table.numberOfRows() > 0 {
                        let cell = table.preparedCellAtColumn_row(i as isize, 0);
                        if let Some(cell) = cell {
                            let is_toggle = column.identifier().to_string() == "output" || column.identifier().to_string() == "visible";
                            let text = cell.stringValue().to_string();
                            if !is_toggle && !text.is_empty() && cell.cellSize().width > column.width() + 0.5 {
                                problems.push(format!("cell {text:?} needs {:.0} pt in a {:.0} pt column", cell.cellSize().width, column.width()));
                            }
                        }
                    }
                }
                if total > visible + 0.5 {
                    problems.push(format!("the columns need {total:.0} pt, the table shows {visible:.0} pt ({:?})", columns.iter().map(|c| (c.identifier().to_string(), c.width().round())).collect::<Vec<_>>()));
                }
                // Swatch and name: the layer cell is filled with the layer colour, the name follows.
                let layer = table.preparedCellAtColumn_row(0, 0);
                let name = table.preparedCellAtColumn_row(1, 0);
                let swatch = layer.as_ref().and_then(|c| c.downcast_ref::<objc2_app_kit::NSTextFieldCell>().map(|c| c.drawsBackground()));
                if swatch != Some(true) {
                    problems.push("the layer column has no colour swatch".into());
                }
                if name.map(|c| c.stringValue().to_string()).as_deref() != Some("Snijden") {
                    problems.push("the second column is not the operation name".into());
                }
                problems
            })
            .ok_or("no window")?;
        expect(|| format!("layer table: {}", report.join("; ")), report.is_empty())
    }

    fn status_bar(s: &Session, _first: DocumentId) -> Check {
        // A fresh window: the pointer has not been over its canvas yet.
        let doc = s.new_document()?;
        let texts = |s: &Session| {
            s.app.with_window(doc, |w| {
                (
                    w.status_coordinate.stringValue().to_string(),
                    w.status_zoom.stringValue().to_string(),
                    w.status_unit.stringValue().to_string(),
                )
            })
        };
        let (coordinate, zoom, unit) = texts(s).ok_or("no window")?;
        expect(|| format!("coordinate {coordinate:?} before any mouse move"), coordinate.is_empty())?;
        expect(|| format!("zoom {zoom:?}"), zoom.ends_with(" %") && zoom.len() > 2)?;
        expect(|| format!("unit {unit:?}"), unit == "mm")?;
        let d = s.doc(doc).ok_or("no document")?;
        let at = d.camera.world_to_points(&d.viewport, burnburn_geometry::Point::new(10.0, 35.0));
        // As with a real mouse the canvas receives the move (tracking area / first responder).
        let canvas = s.app.with_window(doc, |w| w.canvas_view.retain()).ok_or("no canvas")?;
        s.app.window(doc).ok_or("no window")?.makeFirstResponder(Some(&canvas));
        s.mouse(doc, NSEventType::MouseMoved, at)?;
        s.pump(0.2);
        let (coordinate, _, _) = texts(s).ok_or("no window")?;
        expect(|| format!("coordinate after the move: {coordinate:?}"), coordinate == "X 10  Y 35")?;
        s.app.with_window(doc, |w| w.window.close());
        s.pump(0.2);
        Ok(())
    }

    fn toolbar(s: &Session, doc: DocumentId) -> Check {
        let report = s
            .app
            .with_window(doc, |w| {
                let toolbar = w.window.toolbar();
                let Some(toolbar) = toolbar else { return vec!["the window has no toolbar".to_owned()] };
                let mut problems = Vec::new();
                if !toolbar.allowsUserCustomization() {
                    problems.push("the toolbar cannot be customised".to_owned());
                }
                let items = toolbar.items();
                let titles = w.strip.panel_titles();
                for (index, title) in titles.iter().enumerate() {
                    let Some(panel) = w.strip.panel_view(index) else {
                        problems.push(format!("no view for panel {title}"));
                        continue;
                    };
                    let item = items.iter().find(|i| i.view().is_some_and(|v| std::ptr::eq(&*v as *const NSView, &*panel as *const NSView)));
                    let Some(item) = item else {
                        problems.push(format!("panel {title:?} is not a toolbar item"));
                        continue;
                    };
                    if item.label().to_string() != *title {
                        problems.push(format!("item label {:?} instead of {title:?}", item.label()));
                    }
                    let menu_items = item.menuFormRepresentation().and_then(|m| m.submenu()).map_or(0, |m| m.numberOfItems());
                    if menu_items == 0 {
                        problems.push(format!("panel {title:?} has no overflow menu"));
                    }
                }
                if titles.is_empty() {
                    problems.push("no ribbon groups".to_owned());
                }
                // No second strip: the content is the split view and the status bar only.
                let count = w.window.contentView().map_or(0, |v| v.subviews().len());
                if count != 2 {
                    problems.push(format!("the content view has {count} subviews, expected split view and status bar"));
                }
                problems
            })
            .ok_or("no window")?;
        expect(|| format!("toolbar: {}", report.join("; ")), report.is_empty())
    }

    fn status_symbol_copy_id_and_symbols(s: &Session, doc: DocumentId) -> Check {
        use burnburn_host_models::style::StatusTone;
        let window = s.app.window(doc).ok_or("no window")?;
        s.become_key(&window);
        // An invalid typed value is an error: the line starts with the symbol of that tone.
        s.type_into_field(doc, FieldKey::Width, "abc")?;
        s.key(doc, "\r", 36)?;
        s.pump(0.2);
        let line = s.app.with_window(doc, |w| w.status_label.stringValue().to_string()).ok_or("no window")?;
        expect(
            || format!("status line {line:?} lacks the error symbol {:?}", StatusTone::Error.glyph()),
            line.starts_with(StatusTone::Error.glyph()),
        )?;

        // Plan: compile, then Kopieer id puts the plan id on the (private) pasteboard.
        s.menu(MenuItem::Command(CommandKey::Compile))?;
        s.wait(5.0, "the plan", || s.doc(doc).is_some_and(|d| d.plan.is_some()))?;
        s.pump(0.2);
        let id = s.doc(doc).and_then(|d| d.plan).map(|p| p.id.to_string()).ok_or("no plan")?;
        let button: Retained<objc2_app_kit::NSButton> =
            s.app.with_window(doc, |w| w.copy_id_button.retain()).ok_or("no window")?;
        expect(|| "Kopieer id is disabled with a plan".into(), button.isEnabled())?;
        expect(|| format!("button title {:?}", button.title()), button.title().to_string() == "Kopieer id")?;
        // SAFETY: a live button on the main thread; nil sender.
        unsafe { button.performClick(None) };
        s.pump(0.1);
        let copied = s
            .app
            .pasteboard()
            .stringForType(unsafe { objc2_app_kit::NSPasteboardTypeString })
            .map(|t| t.to_string());
        expect(|| format!("pasteboard holds {copied:?}, not {id}"), copied.as_deref() == Some(id.as_str()))?;
        let title = s.app.with_window(doc, |w| w.plan_labels[0].stringValue().to_string()).ok_or("no window")?;
        expect(|| format!("plan title {title:?}"), title.starts_with("Plan van ") && title.contains(" \u{b7} "))?;

        // Glyph-titled buttons of this window are SF Symbols with an accessible description.
        let report = s
            .app
            .with_window(doc, |w| {
                let mut problems = Vec::new();
                let mut check = |what: &str, b: &objc2_app_kit::NSButton| {
                    if b.image().is_none() {
                        problems.push(format!("{what}: no symbol image"));
                    }
                    if !b.title().to_string().is_empty() {
                        problems.push(format!("{what}: still has the title {:?}", b.title()));
                    }
                    if b.accessibilityLabel().is_none_or(|l| l.to_string().is_empty()) {
                        problems.push(format!("{what}: no accessibility label"));
                    }
                };
                for (i, b) in w.flip_buttons.iter().enumerate() {
                    check(&format!("flip {i}"), b);
                }
                for (i, b) in w.palette.iter().enumerate().take(3) {
                    check(&format!("palette {i}"), b);
                }
                if let Some(arrow) = w.strip.arrow("shape") {
                    check("variant arrow", &arrow);
                } else {
                    problems.push("no variant arrow found".into());
                }
                problems
            })
            .ok_or("no window")?;
        let _: Option<&NSControl> = None;
        expect(|| format!("symbols: {}", report.join("; ")), report.is_empty())
    }
}
