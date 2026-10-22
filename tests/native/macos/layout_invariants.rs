//! Layout invariants of every host window, panel and sheet (look-and-feel plan, LF1). The host
//! lays everything out by hand, so a frame can end up outside its container, on top of its
//! neighbour or too small for its text without any test noticing. For the document window (empty,
//! with a selection and with a compiled plan) and every window of `common::windows::steps()` this
//! walks the real view tree, quietly and offscreen (alpha 0, click-through, never active), and
//! requires, before and after the window is made larger:
//!
//! (a) every visible control lies inside the bounds of the view that holds it;
//! (b) no two visible controls of one container overlap;
//! (c) no label or button is cut off: wrapped text fits in the height, other text in the width
//!     (a label that truncates on purpose has a tooltip with the full text);
//! (d) a scroll view that holds controls starts at the top of its content;
//! (e) the default (Return) button is the rightmost button of its row, and every dialog sheet
//!     answers Escape, and Return when it has more than one button;
//! (f) a long text is not shown twice in one window (a reason is said once, not per field);
//! (g) an editable field says what it is: a caption beside or above it, or a placeholder.
//!
//! Dialog sheets that are built but not open (import, image, tracer, export, bundle) are walked
//! too. Baselines of labels and fields are not checked yet.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("layout_invariants: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("layout_invariants", invariants::run())
}

#[cfg(target_os = "macos")]
mod invariants {
    use std::collections::HashSet;
    use std::ptr;

    use burnburn_macos::adapter::MenuItem;
    use burnburn_macos::appkit::HostOptions;
    use burnburn_presentation::{CommandKey, UiIntent};
    use burnburn_types::DocumentId;
    use objc2::Message;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2_app_kit::{
        NSApplication, NSButton, NSControl, NSLineBreakMode, NSPopUpButton, NSScrollView, NSSegmentedControl,
        NSTableView, NSTextField, NSTextView, NSView, NSWindow, NSWindowStyleMask,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize};

    use crate::common::windows;
    use crate::common::{Session, start_with};

    /// Float noise and one-point borders are not a defect.
    const TOLERANCE: f64 = 1.0;

    pub fn run() -> Result<(), String> {
        let s = start_with(|o: &mut HostOptions| {
            o.quiet_windows = true;
            o.float_windows = false;
            o.locale = Some(burnburn_macos::l10n::Locale::Nl);
        })?;
        let mut found: Vec<String> = Vec::new();
        let doc = s.new_document()?;
        check_document(&s, doc, "document (empty)", &mut found)?;
        s.app.dispatch(UiIntent::SetMachineProfile {
            profile: Box::new(windows::machine_profile()?),
            ruida: burnburn_ruida::RuidaProfile {
                magic: burnburn_ruida::Magic::FIXTURE_0X88,
                overscan: burnburn_ruida::OverscanPolicy::ControllerManaged,
            },
        });
        s.draw_rectangle(doc, (10.0, 10.0), (90.0, 60.0))?;
        s.menu(MenuItem::Command(CommandKey::ToolEllipse))?;
        s.drag_mm(doc, (110.0, 20.0), (170.0, 70.0))?;
        s.menu(MenuItem::Command(CommandKey::ToolSelect))?;
        s.pump(0.3);
        s.app.dispatch(UiIntent::AddOperation { document: doc, draft: windows::cut_draft()? });
        s.pump(0.3);
        if let Some(d) = s.doc(doc) {
            let at = d.camera.world_to_points(&d.viewport, burnburn_geometry::Point::new(10.0, 35.0));
            s.mouse(doc, objc2_app_kit::NSEventType::LeftMouseDown, at)?;
            s.mouse(doc, objc2_app_kit::NSEventType::LeftMouseUp, at)?;
        }
        s.pump(0.3);
        check_document(&s, doc, "document (selection)", &mut found)?;
        check_transform_fields_in_view(&s, doc, &mut found);
        if s.menu(MenuItem::Command(CommandKey::Compile)).is_ok() {
            let _ = s.wait(5.0, "the plan", || s.doc(doc).is_some_and(|d| d.plan.is_some()));
            check_document(&s, doc, "document (compiled)", &mut found)?;
        }
        if let Some(op) = s.doc(doc).and_then(|d| d.operations.first().map(|o| o.id)) {
            s.app.dispatch(UiIntent::OpenCutSettings { document: doc, operation: op });
            check_new_windows(&s, doc, "cut-settings", &mut found);
        }
        for (step, item) in windows::steps() {
            if s.menu(item).is_err() {
                continue;
            }
            check_new_windows(&s, doc, step, &mut found);
        }
        // The dialog sheets are built with the window, shown or not.
        let sheets: Vec<(&str, Retained<NSWindow>)> = s
            .app
            .with_window(doc, |w| w.built_sheets.iter().map(|(n, sh)| (*n, (*sh).retain())).collect())
            .unwrap_or_default();
        for (name, sheet) in &sheets {
            walk_window(sheet, &format!("sheet {name}, built"), &mut found);
            check_sheet_keys(sheet, name, &mut found);
        }
        if found.is_empty() {
            return Ok(());
        }
        let limit = if std::env::var_os("BURNBURN_LAYOUT_ALL").is_some() { usize::MAX } else { 80 };
        let shown = found.len().min(limit);
        Err(format!(
            "{} layout violations:\n    {}{}",
            found.len(),
            found[..shown].join("\n    "),
            if found.len() > shown { format!("\n    ... and {} more", found.len() - shown) } else { String::new() }
        ))
    }

    /// Regression (look-and-feel Q1): X, Y, Width and Height of the inspector lay above the visible
    /// part of the column, because the column was shifted twice. They are in view when the
    /// window opens.
    fn check_transform_fields_in_view(s: &Session, doc: DocumentId, found: &mut Vec<String>) {
        use burnburn_presentation::FieldKey;
        s.app.with_window(doc, |w| {
            for key in [FieldKey::X, FieldKey::Y, FieldKey::Width, FieldKey::Height] {
                let Some(field) = w.field(key) else {
                    found.push(format!("inspector: no field for {key:?}"));
                    continue;
                };
                let Some(scroll) = field.enclosingScrollView() else {
                    found.push(format!("inspector: {key:?} is not in a scroll view"));
                    continue;
                };
                let visible = scroll.documentVisibleRect();
                let f = field.frame();
                let in_view = f.origin.y >= visible.origin.y - TOLERANCE
                    && f.origin.y + f.size.height <= visible.origin.y + visible.size.height + TOLERANCE;
                if !in_view {
                    found.push(format!(
                        "inspector: the {key:?} field is out of view: it lies at {:.0}..{:.0}, the column shows {:.0}..{:.0}",
                        f.origin.y,
                        f.origin.y + f.size.height,
                        visible.origin.y,
                        visible.origin.y + visible.size.height
                    ));
                }
            }
        });
    }

    fn check_document(s: &Session, doc: DocumentId, name: &str, found: &mut Vec<String>) -> Result<(), String> {
        let window = s.app.window(doc).ok_or("no window")?;
        check_window(s, &window, name, found);
        Ok(())
    }

    /// Every visible window that appeared for `step`, then hides it again.
    fn check_new_windows(s: &Session, doc: DocumentId, step: &str, found: &mut Vec<String>) {
        s.pump(0.6);
        let main = s.app.window(doc);
        let app = NSApplication::sharedApplication(s.mtm);
        let mut seen = HashSet::new();
        for w in app.windows().iter() {
            if !w.isVisible() || main.as_deref().is_some_and(|m| ptr::eq(m, &*w)) || !seen.insert(w.windowNumber()) {
                continue;
            }
            check_window(s, &w, &format!("{step} ({})", w.title()), found);
            if let Some(parent) = w.sheetParent() {
                parent.endSheet(&w);
            } else {
                w.orderOut(None);
            }
        }
        s.pump(0.2);
    }

    /// The invariants for `window` as it is, and again after it became larger.
    fn check_window(s: &Session, window: &NSWindow, name: &str, found: &mut Vec<String>) {
        window.displayIfNeeded();
        walk_window(window, &format!("{name}, as opened"), found);
        if window.styleMask().contains(NSWindowStyleMask::Resizable)
            && let Some(content) = window.contentView()
        {
            let old = content.frame().size;
            window.setContentSize(NSSize::new(old.width + 120.0, old.height + 80.0));
            s.pump(0.15);
            window.displayIfNeeded();
            walk_window(window, &format!("{name}, larger"), found);
            window.setContentSize(old);
            // A non-flipped column keeps its lower-left anchor when the window shrinks again; the
            // user scrolls back, so the next check starts from the top like a fresh window.
            scroll_to_top(&content);
            s.pump(0.1);
        }
    }

    fn scroll_to_top(view: &NSView) {
        for child in view.subviews().iter() {
            if let Some(scroll) = as_any(&child).downcast_ref::<NSScrollView>() {
                if let Some(doc) = scroll.documentView().filter(|d| !is_leaf(d)) {
                    doc.scrollPoint(NSPoint::new(0.0, 1.0e7));
                    scroll_to_top(&doc);
                }
            } else if !is_leaf(&child) {
                scroll_to_top(&child);
            }
        }
    }

    fn walk_window(window: &NSWindow, name: &str, found: &mut Vec<String>) {
        let Some(root) = window.contentView() else { return };
        let mut texts = Vec::new();
        walk(&root, name, found, &mut texts, default_cell(window));
        // (f) a long text twice in one window.
        texts.sort();
        for pair in texts.windows(2) {
            if pair[0] == pair[1] && pair[0].chars().count() > 20 {
                let times = texts.iter().filter(|t| **t == pair[0]).count();
                let shown: String = pair[0].chars().take(60).collect();
                let message = format!("{name}: the same text {times} times: '{shown}'");
                if !found.contains(&message) {
                    found.push(message);
                }
            }
        }
    }

    /// The Escape key equivalent of a button.
    fn answers_escape(view: &NSView) -> bool {
        as_any(view).downcast_ref::<NSButton>().is_some_and(|b| b.keyEquivalent().to_string() == "\u{1b}")
    }

    /// The window's default button cell (Return): AppKit makes a "\r" key equivalent the window's
    /// default and reports the key equivalent itself as empty.
    fn default_cell(window: &NSWindow) -> *const AnyObject {
        window.defaultButtonCell().map_or(ptr::null(), |c| Retained::as_ptr(&c).cast())
    }

    fn answers_return(view: &NSView, default: *const AnyObject) -> bool {
        !default.is_null()
            && as_any(view)
                .downcast_ref::<NSButton>()
                .and_then(|b| b.cell())
                .is_some_and(|c| ptr::eq(Retained::as_ptr(&c).cast::<AnyObject>(), default))
    }

    /// Push buttons of the tree (no checkboxes, pop-ups or arrows), with the window-relative frame.
    fn push_buttons(view: &NSView, out: &mut Vec<Retained<NSView>>) {
        for child in view.subviews().iter() {
            if child.isHidden() {
                continue;
            }
            let any = as_any(&child);
            if let Some(b) = any.downcast_ref::<NSButton>() {
                if any.downcast_ref::<NSPopUpButton>().is_none()
                    && child.frame().size.width >= 60.0
                    && !b.title().to_string().is_empty()
                    && b.bezelStyle() != objc2_app_kit::NSBezelStyle::SmallSquare
                    && b.isBordered()
                {
                    out.push(child.retain());
                }
            } else if !is_leaf(&child) {
                push_buttons(&child, out);
            } else if let Some(scroll) = any.downcast_ref::<NSScrollView>() {
                if let Some(doc) = scroll.documentView() {
                    push_buttons(&doc, out);
                }
            }
        }
    }

    /// (e) for a dialog sheet: it answers Escape, and Return when it has a button row.
    fn check_sheet_keys(sheet: &NSWindow, name: &str, found: &mut Vec<String>) {
        let Some(root) = sheet.contentView() else { return };
        let mut buttons = Vec::new();
        push_buttons(&root, &mut buttons);
        let default = default_cell(sheet);
        let escapes = buttons.iter().filter(|b| answers_escape(b)).count();
        let returns = buttons.iter().filter(|b| answers_return(b, default)).count();
        let bar = buttons.iter().filter(|b| b.frame().origin.y < 60.0).count();
        if escapes != 1 {
            found.push(format!("sheet {name}: {escapes} buttons answer Escape, expected one"));
        }
        if bar >= 2 && returns != 1 {
            found.push(format!("sheet {name}: {returns} buttons answer Return, expected one"));
        }
    }

    fn as_any(view: &NSView) -> &AnyObject {
        view.as_ref()
    }

    /// Controls with their own insides (nothing to descend into).
    fn is_leaf(view: &NSView) -> bool {
        let any = as_any(view);
        any.downcast_ref::<NSControl>().is_some()
            || any.downcast_ref::<NSTableView>().is_some()
            || any.downcast_ref::<NSTextView>().is_some()
    }

    /// A control that takes part in the checks. Empty labels draw nothing.
    fn counts(view: &NSView) -> bool {
        if view.isHidden() {
            return false;
        }
        let any = as_any(view);
        if let Some(t) = any.downcast_ref::<NSTextField>()
            && !t.isEditable()
            && !t.isBezeled()
            && t.stringValue().to_string().trim().is_empty()
        {
            return false;
        }
        any.downcast_ref::<NSControl>().is_some() || any.downcast_ref::<NSScrollView>().is_some()
    }

    fn describe(view: &NSView) -> String {
        let any = as_any(view);
        let f = view.frame();
        let text = if let Some(t) = any.downcast_ref::<NSTextField>() {
            t.stringValue().to_string()
        } else if let Some(b) = any.downcast_ref::<NSButton>() {
            b.title().to_string()
        } else {
            String::new()
        };
        let kind = if any.downcast_ref::<NSPopUpButton>().is_some() {
            "popup"
        } else if any.downcast_ref::<NSSegmentedControl>().is_some() {
            "segments"
        } else if any.downcast_ref::<NSButton>().is_some() {
            "button"
        } else if any.downcast_ref::<NSTextField>().is_some() {
            "text"
        } else if any.downcast_ref::<NSScrollView>().is_some() {
            "scroll"
        } else {
            "view"
        };
        let text: String = text.chars().take(28).collect();
        format!("{kind} '{text}' @({:.0},{:.0} {:.0}x{:.0})", f.origin.x, f.origin.y, f.size.width, f.size.height)
    }

    fn overlap(a: NSRect, b: NSRect) -> bool {
        let w = (a.origin.x + a.size.width).min(b.origin.x + b.size.width) - a.origin.x.max(b.origin.x);
        let h = (a.origin.y + a.size.height).min(b.origin.y + b.size.height) - a.origin.y.max(b.origin.y);
        w > TOLERANCE && h > TOLERANCE
    }

    fn inside(inner: NSRect, outer: NSRect) -> bool {
        inner.origin.x >= outer.origin.x - TOLERANCE
            && inner.origin.y >= outer.origin.y - TOLERANCE
            && inner.origin.x + inner.size.width <= outer.origin.x + outer.size.width + TOLERANCE
            && inner.origin.y + inner.size.height <= outer.origin.y + outer.size.height + TOLERANCE
    }

    fn cut_off(view: &NSView) -> Option<String> {
        let any = as_any(view);
        let f = view.frame();
        if let Some(t) = any.downcast_ref::<NSTextField>() {
            if t.isEditable() || t.isBezeled() || t.stringValue().to_string().trim().is_empty() {
                return None;
            }
            let cell = t.cell()?;
            let mode = t.lineBreakMode();
            if mode == NSLineBreakMode::ByWordWrapping || mode == NSLineBreakMode::ByCharWrapping {
                let need =
                    cell.cellSizeForBounds(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(f.size.width, 10_000.0)));
                (need.height > f.size.height + TOLERANCE)
                    .then(|| format!("wrapped text needs {:.0} pt of height, has {:.0}", need.height, f.size.height))
            } else {
                let need = cell.cellSize();
                let truncates = mode == NSLineBreakMode::ByTruncatingTail
                    || mode == NSLineBreakMode::ByTruncatingMiddle
                    || mode == NSLineBreakMode::ByTruncatingHead;
                let tip = t.toolTip().is_some_and(|tip| !tip.to_string().is_empty());
                (need.width > f.size.width + TOLERANCE && !(truncates && tip))
                    .then(|| format!("text needs {:.0} pt of width, has {:.0}", need.width, f.size.width))
            }
        } else if let Some(b) = any.downcast_ref::<NSButton>() {
            if any.downcast_ref::<NSPopUpButton>().is_some() {
                return None;
            }
            let need = b.cell()?.cellSize();
            let tip = b.toolTip().is_some_and(|tip| !tip.to_string().is_empty());
            (need.width > f.size.width + TOLERANCE && !tip)
                .then(|| format!("title needs {:.0} pt of width, has {:.0}", need.width, f.size.width))
        } else {
            None
        }
    }

    /// (e) among the buttons of one row the Return button is the rightmost.
    fn check_default_rightmost(
        items: &[&Retained<NSView>],
        path: &str,
        found: &mut Vec<String>,
        default_cell: *const AnyObject,
    ) {
        let buttons: Vec<&&Retained<NSView>> = items
            .iter()
            .filter(|v| {
                as_any(v).downcast_ref::<NSButton>().is_some_and(|b| {
                    as_any(v).downcast_ref::<NSPopUpButton>().is_none()
                        && v.frame().size.width >= 60.0
                        && b.isBordered()
                        && b.bezelStyle() != objc2_app_kit::NSBezelStyle::SmallSquare
                })
            })
            .collect();
        for default in buttons.iter().filter(|b| answers_return(b, default_cell)) {
            let f = default.frame();
            for other in &buttons {
                let o = other.frame();
                let same_row = (o.origin.y - f.origin.y).abs() < 6.0;
                if same_row && !ptr::eq(&***other, &***default) && o.origin.x > f.origin.x + TOLERANCE {
                    found.push(format!(
                        "{path}: the default button is not rightmost of its row: {}  is left of  {}",
                        describe(default),
                        describe(other)
                    ));
                }
            }
        }
    }

    /// (g) an editable field has a caption beside or above it, or a placeholder.
    fn check_captioned(items: &[&Retained<NSView>], path: &str, found: &mut Vec<String>) {
        for item in items {
            let Some(field) = as_any(item).downcast_ref::<NSTextField>() else { continue };
            if !field.isEditable() || !field.isBezeled() {
                continue;
            }
            if field.placeholderString().is_some_and(|p| !p.to_string().is_empty()) {
                continue;
            }
            let f = item.frame();
            let has_caption = items.iter().any(|other| {
                let Some(label) = as_any(other).downcast_ref::<NSTextField>() else { return false };
                if label.isEditable() || label.stringValue().to_string().trim().is_empty() {
                    return false;
                }
                let o = other.frame();
                let beside = (o.origin.y + o.size.height / 2.0 - (f.origin.y + f.size.height / 2.0)).abs() < 14.0
                    && o.origin.x + o.size.width <= f.origin.x + TOLERANCE + 2.0
                    && f.origin.x - (o.origin.x + o.size.width) < 60.0;
                let above = (o.origin.x - f.origin.x).abs() < 30.0
                    && o.origin.y >= f.origin.y + f.size.height - 4.0
                    && o.origin.y - (f.origin.y + f.size.height) < 12.0;
                beside || above
            });
            if !has_caption {
                found.push(format!("{path}: editable field without caption or placeholder: {}", describe(item)));
            }
        }
    }

    fn walk(
        view: &NSView,
        path: &str,
        found: &mut Vec<String>,
        texts: &mut Vec<String>,
        default_cell: *const AnyObject,
    ) {
        if view.isHidden() {
            return;
        }
        let bounds = view.bounds();
        let children: Vec<Retained<NSView>> = view.subviews().iter().collect();
        let items: Vec<&Retained<NSView>> = children.iter().filter(|c| counts(c)).collect();
        for item in &items {
            if !inside(item.frame(), bounds) {
                found.push(format!(
                    "{path}: outside its container ({:.0}x{:.0}): {}",
                    bounds.size.width,
                    bounds.size.height,
                    describe(item)
                ));
            }
            if let Some(why) = cut_off(item) {
                found.push(format!("{path}: cut off ({why}): {}", describe(item)));
            }
        }
        for item in &items {
            if let Some(t) = as_any(item).downcast_ref::<NSTextField>()
                && !t.isEditable()
                && !t.isBezeled()
            {
                texts.push(t.stringValue().to_string());
            }
        }
        check_default_rightmost(&items, path, found, default_cell);
        check_captioned(&items, path, found);
        for (i, a) in items.iter().enumerate() {
            for b in &items[i + 1..] {
                if overlap(a.frame(), b.frame()) {
                    found.push(format!("{path}: overlap: {}  x  {}", describe(a), describe(b)));
                }
            }
        }
        for child in &children {
            if child.isHidden() {
                continue;
            }
            let any = as_any(child);
            if let Some(scroll) = any.downcast_ref::<NSScrollView>() {
                let Some(doc) = scroll.documentView() else { continue };
                if is_leaf(&doc) {
                    continue;
                }
                walk(&doc, path, found, texts, default_cell);
                // (d) a scroll view starts at the top of its content.
                let visible = scroll.documentVisibleRect();
                let top = doc
                    .subviews()
                    .iter()
                    .filter(|c| counts(c))
                    .map(|c| c.frame().origin.y + c.frame().size.height)
                    .fold(f64::NEG_INFINITY, f64::max);
                if top.is_finite() && top > visible.origin.y + visible.size.height + TOLERANCE {
                    found.push(format!(
                        "{path}: scroll content starts below its top: highest control reaches {top:.0}, view shows {:.0}..{:.0} of a {:.0} high document",
                        visible.origin.y,
                        visible.origin.y + visible.size.height,
                        doc.frame().size.height
                    ));
                }
            } else if !is_leaf(child) {
                walk(child, path, found, texts, default_cell);
            }
        }
    }
}
