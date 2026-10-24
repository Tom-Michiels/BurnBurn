//! LF15: Tab walks every editable control in reading order, quietly and offscreen (alpha 0,
//! click-through, never active). The oracle is the position of the controls on screen (top to
//! bottom, left to right within a line), never the order they were built in:
//!
//! (a) a form the Mac builder makes (plain, with a memo and small buttons, with tabs) has a
//!     `nextKeyView` loop over its visible controls in reading order, dialog buttons last, and the
//!     window it lands in gets an `initialFirstResponder`;
//! (b) Shift-Tab (`previousKeyView`) walks the same loop backwards;
//! (c) every form window of `common::windows::steps()` passes (a) and (b) for each form it holds;
//! (d) the document window's loop runs object list, canvas, inspector fields, then the operations
//!     of the production strip, covers every visible focusable control, and comes round.

#![allow(unsafe_code, deprecated)]

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("lf15_tab_order: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("lf15_tab_order", scenario::run())
}

#[cfg(target_os = "macos")]
mod scenario {
    use std::ptr;

    use burnburn_host_models::form::{Control, DialogButtons, FormButton, FormRow, FormSection, FormSpec, FormTool};
    use burnburn_macos::appkit::HostOptions;
    use burnburn_macos::appkit::form::build_form;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{MainThreadOnly, Message};
    use objc2_app_kit::{
        NSApplication, NSBackingStoreType, NSButton, NSPopUpButton, NSScrollView, NSTableView, NSTextField, NSTextView,
        NSView, NSWindow, NSWindowStyleMask,
    };
    use objc2_foundation::{NSPoint, NSRect, NSSize};

    use crate::common::windows;
    use crate::common::{Session, expect, start_with};

    pub fn run() -> Result<(), String> {
        let s = start_with(|o: &mut HostOptions| {
            o.quiet_windows = true;
            o.float_windows = false;
            o.locale = Some(burnburn_macos::l10n::Locale::Nl);
        })?;
        let mut found: Vec<String> = Vec::new();
        built_forms(&s, &mut found);
        let doc = s.new_document()?;
        s.pump(0.4);
        document_window(&s, doc, &mut found);
        for (step, item) in windows::steps() {
            if s.menu(item).is_err() {
                continue;
            }
            s.pump(0.6);
            let app = NSApplication::sharedApplication(s.mtm);
            let main = s.app.window(doc);
            for w in app.windows().iter() {
                if !w.isVisible() || main.as_deref().is_some_and(|m| ptr::eq(m, &*w)) {
                    continue;
                }
                let name = format!("{step} ({})", w.title());
                check_window_forms(&w, &name, &mut found);
                if let Some(parent) = w.sheetParent() {
                    parent.endSheet(&w);
                } else {
                    w.orderOut(None);
                }
            }
            s.pump(0.2);
        }
        found.sort();
        found.dedup();
        expect(|| format!("{} problems:\n    {}", found.len(), found.join("\n    ")), found.is_empty())
    }

    fn next_key(view: &NSView) -> Option<Retained<NSView>> {
        // SAFETY: a read of the key-view link on the main thread.
        unsafe { view.nextKeyView() }
    }

    fn prev_key(view: &NSView) -> Option<Retained<NSView>> {
        // SAFETY: a read of the key-view link on the main thread.
        unsafe { view.previousKeyView() }
    }

    fn superview(view: &NSView) -> Option<Retained<NSView>> {
        // SAFETY: a read of the view hierarchy on the main thread.
        unsafe { view.superview() }
    }

    fn class_name(view: &NSView) -> String {
        view.class().name().to_string_lossy().into_owned()
    }

    fn as_any(view: &NSView) -> &AnyObject {
        // SAFETY: an NSView is an Objective-C object.
        unsafe { &*(view as *const NSView as *const AnyObject) }
    }

    fn descendants(view: &NSView, out: &mut Vec<Retained<NSView>>) {
        for child in view.subviews().iter() {
            out.push(child.clone());
            descendants(&child, out);
        }
    }

    /// A control the user can Tab to (hidden, disabled and read-only ones are skipped).
    fn focusable(view: &NSView) -> bool {
        if view.isHiddenOrHasHiddenAncestor() || view.window().is_none() {
            return false;
        }
        let any = as_any(view);
        if let Some(t) = any.downcast_ref::<NSTextField>() {
            return t.isEditable() && t.isEnabled();
        }
        if let Some(t) = any.downcast_ref::<NSTextView>() {
            // The field editor of a text field being edited is not a control of its own.
            let mut up = superview(view);
            while let Some(p) = up {
                if as_any(&p).downcast_ref::<NSTextField>().is_some() {
                    return false;
                }
                up = superview(&p);
            }
            return t.isEditable();
        }
        if let Some(b) = any.downcast_ref::<NSButton>() {
            // The buttons inside a tab strip or a pop-up are AppKit's own.
            return b.isEnabled() && superview(b).is_some_and(|p| !class_name(&p).contains("NSTab"));
        }
        any.downcast_ref::<NSPopUpButton>().is_some_and(|p| p.isEnabled())
    }

    /// Window-base rectangle, y up.
    fn base_rect(view: &NSView) -> NSRect {
        view.convertRect_toView(view.bounds(), None)
    }

    /// Where a reader finds `view`: (is it a dialog button, distance from the top of its page, x).
    /// A form that scrolls keeps its rows in the document view of a scroll view; rows are placed
    /// in that document, not by how far it happens to be scrolled. Dialog buttons stand directly
    /// in the form's root.
    fn place(view: &NSView) -> (u8, f64, f64) {
        let mut up = superview(view);
        let mut document: Option<Retained<NSView>> = None;
        while let Some(p) = up {
            if let Some(scroll) = as_any(&p).downcast_ref::<NSScrollView>()
                && let Some(doc) = scroll.documentView()
                && as_any(&doc).downcast_ref::<NSTextView>().is_none()
            {
                document = Some(doc);
                break;
            }
            up = superview(&p);
        }
        let dialog = u8::from(
            as_any(view).downcast_ref::<NSButton>().is_some()
                && superview(view).is_some_and(|p| class_name(&p) == "BurnBurnFormRoot"),
        );
        let (rect, flipped) = match &document {
            Some(doc) => (view.convertRect_toView(view.bounds(), Some(doc)), doc.isFlipped()),
            None => (base_rect(view), false),
        };
        let mid = rect.origin.y + rect.size.height / 2.0;
        (dialog, if flipped { mid } else { -mid }, rect.origin.x)
    }

    /// The controls sorted the way a reader meets them: lines from the top, left to right, the
    /// dialog buttons last.
    fn reading_order(views: Vec<Retained<NSView>>) -> Vec<Retained<NSView>> {
        let mut placed: Vec<((u8, f64, f64), Retained<NSView>)> = views.into_iter().map(|v| (place(&v), v)).collect();
        placed.sort_by(|a, b| a.0.0.cmp(&b.0.0).then(a.0.1.total_cmp(&b.0.1)));
        let mut lines: Vec<Vec<((u8, f64, f64), Retained<NSView>)>> = Vec::new();
        for p in placed {
            match lines.last_mut() {
                Some(line) if line.first().is_some_and(|f| f.0.0 == p.0.0 && (f.0.1 - p.0.1).abs() < 9.0) => {
                    line.push(p);
                }
                _ => lines.push(vec![p]),
            }
        }
        for line in &mut lines {
            line.sort_by(|a, b| a.0.2.total_cmp(&b.0.2));
        }
        lines.into_iter().flatten().map(|(_, v)| v).collect()
    }

    fn describe(view: &NSView) -> String {
        let any = as_any(view);
        let text = if let Some(t) = any.downcast_ref::<NSTextField>() {
            t.stringValue().to_string()
        } else if let Some(b) = any.downcast_ref::<NSButton>() {
            b.title().to_string()
        } else {
            String::new()
        };
        let r = base_rect(view);
        let shown: String = text.chars().take(24).collect();
        format!("{} '{shown}' at ({:.0},{:.0})", class_name(view), r.origin.x, r.origin.y)
    }

    /// The loop from `start` through `nextKeyView` (or `previousKeyView`) back to `start`.
    fn walk(start: &NSView, forward: bool) -> Result<Vec<Retained<NSView>>, String> {
        let mut out = vec![start.retain()];
        let mut at = start.retain();
        for _ in 0..800 {
            let next = if forward { next_key(&at) } else { prev_key(&at) };
            let Some(next) = next else { return Err(format!("the loop breaks after {}", describe(&at))) };
            if ptr::eq(&*next, start) {
                return Ok(out);
            }
            out.push(next.clone());
            at = next;
        }
        Err("the loop does not come round".into())
    }

    /// Checks the loop of the form under `root` (the view the builder made).
    fn check_form(root: &NSView, single: bool, name: &str, found: &mut Vec<String>) {
        let Some(window) = root.window() else {
            found.push(format!("{name}: form is not in a window"));
            return;
        };
        let mut all = Vec::new();
        descendants(root, &mut all);
        let expected = reading_order(all.into_iter().filter(|v| focusable(v)).collect());
        if expected.is_empty() {
            return;
        }
        let first = expected[0].clone();
        match window.initialFirstResponder() {
            None => found.push(format!("{name}: the window has no initialFirstResponder")),
            Some(r) if single && !ptr::eq(&*r, &*first) => {
                found.push(format!(
                    "{name}: initialFirstResponder is {}, not the first control {}",
                    describe(&r),
                    describe(&first)
                ));
            }
            Some(_) => {}
        }
        let want: Vec<String> = expected.iter().map(|v| describe(v)).collect();
        for forward in [true, false] {
            let dir = if forward { "Tab" } else { "Shift-Tab" };
            let chain = match walk(&first, forward) {
                Ok(c) => c,
                Err(e) => {
                    found.push(format!("{name}: {dir}: {e}"));
                    continue;
                }
            };
            // The controls of the form that a user can reach: the loop filtered like AppKit does.
            let mut seen: Vec<Retained<NSView>> =
                chain.into_iter().filter(|v| v.isDescendantOf(root) && focusable(v)).collect();
            if !forward {
                // Backwards from the first control: it, then the rest in reverse.
                if !seen.is_empty() {
                    let head = seen.remove(0);
                    seen.reverse();
                    seen.insert(0, head);
                }
            }
            let got: Vec<String> = seen.iter().map(|v| describe(v)).collect();
            if want != got {
                found.push(format!("{name}: {dir} order differs.\n      want {want:?}\n      got  {got:?}"));
            }
        }
    }

    fn form_roots(window: &NSWindow) -> Vec<Retained<NSView>> {
        let mut all = Vec::new();
        if let Some(content) = window.contentView() {
            all.push(content.clone());
            descendants(&content, &mut all);
        }
        all.into_iter().filter(|v| class_name(v) == "BurnBurnFormRoot").collect()
    }

    fn check_window_forms(window: &NSWindow, name: &str, found: &mut Vec<String>) {
        window.displayIfNeeded();
        let roots = form_roots(window);
        for (i, root) in roots.iter().enumerate() {
            check_form(root, roots.len() == 1, &format!("{name}, form {i}"), found);
        }
    }

    fn scratch_window(s: &Session, size: NSSize) -> Retained<NSWindow> {
        // SAFETY: a freshly allocated window on the main thread; released with the Retained.
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(s.mtm),
                NSRect::new(NSPoint::new(0.0, 0.0), size),
                NSWindowStyleMask::Titled,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        window.setAlphaValue(0.0);
        window
    }

    fn text(id: u32, label: &str) -> FormRow {
        FormRow::new(id, label, Control::Text { value: String::new(), placeholder: String::new() })
    }

    fn buttons() -> DialogButtons {
        DialogButtons {
            cancel: Some(FormButton::new(90, "Annuleer")),
            primary: Some(FormButton::new(91, "Pas toe")),
            help: Some(FormButton::new(92, "Hulp")),
            destructive: None,
        }
    }

    fn built_forms(s: &Session, found: &mut Vec<String>) {
        let plain = FormSpec {
            title: "Snij-instellingen".into(),
            sections: vec![
                FormSection::new(
                    Some("Snijden"),
                    vec![
                        text(1, "Snelheid"),
                        FormRow::new(2, "Vermogen", Control::Number { value: "20".into() }),
                        FormRow::new(
                            3,
                            "Modus",
                            Control::Choice { options: vec!["Lijn".into(), "Vullen".into()], selected: 0 },
                        ),
                        FormRow::new(4, "Lucht aan", Control::Check { on: true }).tool(FormTool::new(8, "+", "Meer")),
                        FormRow::new(5, "Opmerking", Control::Memo { text: "Eik".into() }).help("Met het project"),
                    ],
                ),
                FormSection::new(Some("Machine"), vec![text(6, "Naam"), text(7, "Adres")]),
            ],
            buttons: buttons(),
            ..FormSpec::default()
        };
        let tabbed = FormSpec {
            title: "Instellingen".into(),
            sections: vec![
                FormSection::new(None, vec![text(1, "Naam"), text(2, "Plaats")]).on_tab("Algemeen"),
                FormSection::new(None, vec![text(3, "Raster")]).on_tab("Canvas"),
            ],
            buttons: buttons(),
            ..FormSpec::default()
        };
        for (name, spec) in [("plain form", plain), ("tabbed form", tabbed)] {
            let form = build_form(s.mtm, &spec, 460.0, None);
            let window = scratch_window(s, form.size());
            let Some(content) = window.contentView() else { continue };
            content.addSubview(form.root());
            form.root().setFrameOrigin(NSPoint::new(0.0, 0.0));
            window.displayIfNeeded();
            check_form(form.root(), true, name, found);
            if name == "plain form" {
                let mut all = Vec::new();
                descendants(form.root(), &mut all);
                let stops = reading_order(all.into_iter().filter(|v| focusable(v)).collect());
                // 2 fields, number, choice, check, tool, memo, 2 fields, 3 dialog buttons.
                if stops.len() != 11 {
                    let titles: Vec<String> = stops.iter().map(|v| describe(v)).collect();
                    found.push(format!("{name}: expected 11 tab stops, found {}: {titles:?}", stops.len()));
                }
                let last: Vec<String> = stops.iter().rev().take(3).map(|v| describe(v)).collect();
                if !last.iter().all(|d| d.contains("NSButton") || d.contains("Button")) {
                    found.push(format!("{name}: the dialog buttons are not last: {last:?}"));
                }
            }
        }
    }

    /// (d) The document window's loop.
    fn document_window(s: &Session, doc: burnburn_types::DocumentId, found: &mut Vec<String>) {
        let Some(window) = s.app.window(doc) else {
            found.push("document window: missing".into());
            return;
        };
        window.displayIfNeeded();
        let Some(content) = window.contentView() else { return };
        let mut all = Vec::new();
        descendants(&content, &mut all);
        let Some(first) = window.initialFirstResponder() else {
            found.push("document window: no initialFirstResponder".into());
            return;
        };
        let chain = match walk(&first, true) {
            Ok(c) => c,
            Err(e) => {
                found.push(format!("document window: Tab: {e}"));
                return;
            }
        };
        // The loop is a circle: read it from the object list.
        let mut chain = chain;
        if let Some(at) =
            chain.iter().position(|v| as_any(v).downcast_ref::<NSTableView>().is_some() && base_rect(v).origin.x < 5.0)
        {
            chain.rotate_left(at);
        }
        match walk(&first, false) {
            Ok(back) if back.len() == chain.len() => {}
            Ok(back) => {
                found.push(format!("document window: Shift-Tab visits {} views, Tab {}", back.len(), chain.len()));
            }
            Err(e) => found.push(format!("document window: Shift-Tab: {e}")),
        }
        let canvas_right = all
            .iter()
            .find(|v| class_name(v) == "BurnBurnCanvasView")
            .map(|v| base_rect(v).origin.x + base_rect(v).size.width)
            .unwrap_or(0.0);
        let at = |pred: &dyn Fn(&Retained<NSView>) -> bool| chain.iter().position(|v| pred(v));
        let table = at(&|v| as_any(v).downcast_ref::<NSTableView>().is_some() && base_rect(v).origin.x < 5.0);
        let canvas = at(&|v| class_name(v) == "BurnBurnCanvasView");
        let operations = at(&|v| {
            as_any(v).downcast_ref::<NSTableView>().is_some()
                && base_rect(v).origin.x > 5.0
                && base_rect(v).origin.x < canvas_right
        });
        let fields: Vec<usize> = chain
            .iter()
            .enumerate()
            .filter(|(_, v)| {
                as_any(v).downcast_ref::<NSTextField>().is_some_and(|t| t.isEditable())
                    && base_rect(v).origin.x >= canvas_right - 1.0
            })
            .map(|(i, _)| i)
            .collect();
        match (table, canvas, operations) {
            (Some(t), Some(c), Some(o)) => {
                if t >= c {
                    found.push("document window: the object list does not come before the canvas".into());
                }
                if fields.is_empty() {
                    found.push("document window: no inspector field in the loop".into());
                }
                if fields.iter().any(|f| *f < c || *f > o) {
                    found.push("document window: inspector fields are not between canvas and operations".into());
                }
                let tops: Vec<f64> = fields.iter().map(|i| -base_rect(&chain[*i]).origin.y).collect();
                if !tops.windows(2).all(|w| w[0] <= w[1] + 9.0) {
                    found.push("document window: inspector fields are not top to bottom".into());
                }
            }
            other => found.push(format!("document window: loop lacks object list/canvas/operations: {other:?}")),
        }
        // Everything a user can reach with the keyboard is in the loop.
        for v in all.iter().filter(|v| focusable(v)) {
            if !chain.iter().any(|c| ptr::eq(&**c, &**v)) {
                found.push(format!("document window: not in the loop: {}", describe(v)));
            }
        }
    }
}
