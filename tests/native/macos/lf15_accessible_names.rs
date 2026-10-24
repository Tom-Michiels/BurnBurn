//! LF15: every control says what it is to VoiceOver, quietly and offscreen. For the document
//! window (empty and with a selection) and every window of `common::windows::steps()`:
//!
//! (a) a control with no words of its own (an icon, a glyph, a symbol) has an
//!     `accessibilityLabel` with letters in it;
//! (b) no accessibility label or title holds a glyph or a bare symbol name;
//! (c) every editable field, pop-up and check box is tied to its name: a label, a title element
//!     (its caption) or a title of its own;
//! (d) every column of a table has a header title.
//!
//! Colour is never the only carrier of a status either: a red or orange text in a window starts
//! with the glyph of its tone or says the tone's word (the lint on the status models is in
//! `burnburn-host-models`).

#![allow(unsafe_code, deprecated)]

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("lf15_accessible_names: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("lf15_accessible_names", scenario::run())
}

#[cfg(target_os = "macos")]
mod scenario {
    use std::ptr;

    use burnburn_host_models::style::StatusTone;
    use burnburn_macos::adapter::MenuItem;
    use burnburn_macos::appkit::HostOptions;
    use burnburn_presentation::{CommandKey, UiIntent};
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2_app_kit::{
        NSAccessibility, NSApplication, NSButton, NSColor, NSPopUpButton, NSSegmentedControl, NSTableView, NSTextField,
        NSTextView, NSView, NSWindow,
    };

    use objc2_foundation::NSObjectProtocol;

    use crate::common::windows;
    use crate::common::{Session, expect, start_with};

    pub fn run() -> Result<(), String> {
        let s = start_with(|o: &mut HostOptions| {
            o.quiet_windows = true;
            o.float_windows = false;
            o.locale = Some(burnburn_macos::l10n::Locale::Nl);
        })?;
        let mut found: Vec<String> = Vec::new();
        built_form(&s, &mut found);
        let doc = s.new_document()?;
        s.pump(0.4);
        if let Some(w) = s.app.window(doc) {
            check_window(&w, "document (empty)", &mut found);
        }
        s.draw_rectangle(doc, (10.0, 10.0), (90.0, 60.0))?;
        s.menu(MenuItem::Command(CommandKey::ToolSelect))?;
        s.app.dispatch(UiIntent::AddOperation { document: doc, draft: windows::cut_draft()? });
        s.pump(0.4);
        if let Some(w) = s.app.window(doc) {
            check_window(&w, "document (with a layer)", &mut found);
        }
        for (step, item) in windows::steps() {
            if s.menu(item).is_err() {
                continue;
            }
            s.pump(0.6);
            visit_new_windows(&s, doc, step, &mut found);
        }
        found.sort();
        found.dedup();
        expect(|| format!("{} problems:\n    {}", found.len(), found.join("\n    ")), found.is_empty())
    }

    /// The builder: a note in a warning or error tone starts with the glyph of its tone (also after
    /// an update), a bold title note does not, and a memo without a caption is named by its section.
    fn built_form(s: &Session, found: &mut Vec<String>) {
        use burnburn_host_models::form::{Control, FormRow, FormSection, FormSpec};
        use burnburn_macos::appkit::form::build_form;
        let note = |id: u32, tone: StatusTone, strong: bool, text: &str| {
            FormRow::new(id, "", Control::Note { value: text.into(), lines: 1, tone, strong })
        };
        let spec = FormSpec {
            title: "Voorbeeld".into(),
            sections: vec![FormSection::new(
                Some("Opmerkingen"),
                vec![
                    note(1, StatusTone::Error, false, "Kon niet"),
                    note(2, StatusTone::Warning, false, "Let op"),
                    note(3, StatusTone::Neutral, false, "Rustig"),
                    note(4, StatusTone::Error, true, "Titel"),
                    FormRow::new(5, "", Control::Memo { text: String::new() }),
                    FormRow::new(6, "Naam", Control::Text { value: String::new(), placeholder: String::new() }),
                ],
            )],
            ..FormSpec::default()
        };
        let mut form = build_form(s.mtm, &spec, 400.0, None);
        let text_of = |form: &burnburn_macos::appkit::form::FormView, id: u32| {
            form.control(burnburn_host_models::form::FormId(id))
                .and_then(|v| as_any(&v).downcast_ref::<NSTextField>().map(|t| t.stringValue().to_string()))
                .unwrap_or_default()
        };
        let expect_text = |form: &burnburn_macos::appkit::form::FormView, found: &mut Vec<String>, when: &str| {
            for (id, want) in [
                (1, format!("{} Kon niet", StatusTone::Error.glyph())),
                (2, format!("{} Let op", StatusTone::Warning.glyph())),
                (3, "Rustig".to_owned()),
                (4, "Titel".to_owned()),
            ] {
                let got = text_of(form, id);
                if got != want {
                    found.push(format!("form note {id} {when}: '{got}', want '{want}'"));
                }
            }
        };
        expect_text(&form, found, "as built");
        form.update(&spec);
        expect_text(&form, found, "after an update");
        let memo = form.control(burnburn_host_models::form::FormId(5));
        if memo.is_none_or(|m| label_of(&m) != "Opmerkingen") {
            found.push("form: a memo without caption is not named by its section heading".into());
        }
        let name = form.control(burnburn_host_models::form::FormId(6));
        let caption = form.caption(burnburn_host_models::form::FormId(6));
        let tied = name.is_some_and(|n| {
            n.accessibilityTitleUIElement().is_some_and(|t| {
                caption.is_some_and(|c| {
                    let c: &AnyObject = c.as_ref();
                    ptr::eq(&*t as *const AnyObject, c as *const AnyObject)
                })
            })
        });
        if !tied {
            found.push("form: a captioned field is not tied to its caption (accessibilityTitleUIElement)".into());
        }
    }

    fn visit_new_windows(s: &Session, doc: burnburn_types::DocumentId, step: &str, found: &mut Vec<String>) {
        let app = NSApplication::sharedApplication(s.mtm);
        let main = s.app.window(doc);
        for w in app.windows().iter() {
            if !w.isVisible() || main.as_deref().is_some_and(|m| ptr::eq(m, &*w)) {
                continue;
            }
            check_window(&w, &format!("{step} ({})", w.title()), found);
            if let Some(parent) = w.sheetParent() {
                parent.endSheet(&w);
            } else {
                w.orderOut(None);
            }
        }
        s.pump(0.2);
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

    /// A symbol-font glyph or pictograph.
    fn has_glyph(text: &str) -> bool {
        text.chars().any(|c| matches!(c as u32, 0x2190..=0x2BFF | 0x1F000..=0x1FAFF | 0xE000..=0xF8FF))
    }

    /// An SF Symbol name such as `xmark.octagon` instead of words.
    fn is_symbol_name(text: &str) -> bool {
        text.contains('.')
            && !text.contains(' ')
            && text.chars().all(|c| c.is_ascii_lowercase() || c == '.' || c.is_ascii_digit())
    }

    fn has_letters(text: &str) -> bool {
        text.chars().filter(|c| c.is_alphabetic()).count() >= 2
    }

    fn label_of(view: &NSView) -> String {
        view.accessibilityLabel().map(|l| l.to_string()).unwrap_or_default()
    }

    fn describe(view: &NSView) -> String {
        let any = as_any(view);
        let class = view.class().name().to_string_lossy().into_owned();
        let text = if let Some(t) = any.downcast_ref::<NSTextField>() {
            t.stringValue().to_string()
        } else if let Some(b) = any.downcast_ref::<NSButton>() {
            b.title().to_string()
        } else {
            String::new()
        };
        let f = view.frame();
        let shown: String = text.chars().take(24).collect();
        format!("{class} '{shown}' at ({:.0},{:.0}) {:.0}x{:.0}", f.origin.x, f.origin.y, f.size.width, f.size.height)
    }

    fn check_window(window: &NSWindow, name: &str, found: &mut Vec<String>) {
        window.displayIfNeeded();
        let Some(root) = window.contentView() else { return };
        let mut all = vec![root.clone()];
        descendants(&root, &mut all);
        for v in all.iter().filter(|v| !v.isHiddenOrHasHiddenAncestor()) {
            check_view(v, name, found);
        }
    }

    fn check_view(view: &NSView, name: &str, found: &mut Vec<String>) {
        let any = as_any(view);
        let class = view.class().name().to_string_lossy().into_owned();
        let label = label_of(view);
        // (b) no glyph or symbol name in what is read out.
        if has_glyph(&label) || is_symbol_name(&label) {
            found.push(format!(
                "{name}: accessibility label holds a glyph or symbol name '{label}': {}",
                describe(view)
            ));
        }
        if let Some(b) = any.downcast_ref::<NSButton>() {
            let title = b.title().to_string();
            // (a) an icon or a glyph on a button needs words.
            if !has_letters(&title) && !has_letters(&label) {
                found.push(format!(
                    "{name}: a button without words and without accessibility label: {}",
                    describe(view)
                ));
            }
            // (c) a check box names itself with its title (checked above); nothing more to tie.
        }
        let needs_name = if let Some(t) = any.downcast_ref::<NSTextField>() {
            t.isEditable() && t.isBezeled()
        } else {
            // AppKit's own tab strip names itself; the field editor of a field being edited is the
            // field.
            !class.contains("NSTab")
                && (any.downcast_ref::<NSPopUpButton>().is_some()
                    || any.downcast_ref::<NSSegmentedControl>().is_some()
                    || any.downcast_ref::<NSTextView>().is_some_and(|t| t.isEditable() && !in_text_field(view)))
        };
        if needs_name {
            let tied = has_letters(&label)
                || view.accessibilityTitleUIElement().is_some()
                || any
                    .downcast_ref::<NSTextField>()
                    .is_some_and(|t| t.placeholderString().is_some_and(|p| has_letters(&p.to_string())));
            if !tied {
                found.push(format!("{name}: a field/pop-up without an accessible name: {}", describe(view)));
            }
        }
        if let Some(table) = any.downcast_ref::<NSTableView>() {
            for column in table.tableColumns().iter() {
                let title = column.title().to_string();
                if !has_letters(&title) && !has_letters(&label) {
                    found.push(format!("{name}: a table column without a header title in {}", describe(view)));
                }
            }
        }
        // Colour is never the only carrier: a text in a status colour starts with the glyph of
        // its tone or is a sentence that says so.
        if let Some(t) = any.downcast_ref::<NSTextField>()
            && !t.isEditable()
        {
            let text = t.stringValue().to_string();
            let coloured = t.textColor().is_some_and(|c| is_status_colour(&c));
            let marked = [StatusTone::Warning, StatusTone::Error].iter().any(|tone| text.starts_with(tone.glyph()));
            if coloured && !text.is_empty() && !marked {
                found.push(format!("{name}: status colour without a symbol in front: {}", describe(view)));
            }
        }
    }

    fn in_text_field(view: &NSView) -> bool {
        // SAFETY: a read of the view hierarchy on the main thread.
        let mut up = unsafe { view.superview() };
        while let Some(p) = up {
            if as_any(&p).downcast_ref::<NSTextField>().is_some() {
                return true;
            }
            // SAFETY: as above.
            up = unsafe { p.superview() };
        }
        false
    }

    fn is_status_colour(color: &NSColor) -> bool {
        [NSColor::systemRedColor(), NSColor::systemOrangeColor()].iter().any(|c| c.isEqual(Some(color)))
    }
}
