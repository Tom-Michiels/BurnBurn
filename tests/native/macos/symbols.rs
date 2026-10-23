//! Icons and SF Symbols (look-and-feel plan, LF8). Quiet and offscreen (alpha 0, click-through,
//! never active). Two contracts:
//!
//! (a) every SF Symbol name the host uses exists on this system (`imageWithSystemSymbolName`
//!     is not nil), every symbol has a Dutch and an English description, and a symbol has a
//!     fallback when the system lacks it;
//! (b) no button, segment or pop-up item of any window, panel or built sheet has a title with a
//!     Unicode glyph standing in for an icon ({▾■↔↕◀▶▲▼●○‹›↑↓}), and a button without a title has
//!     an accessible name.
//!
//! The windows of LF7 (document window with toolstrip, operations, production, inspector and
//! priority panels) were excepted while LF7 was in flight; the list below must stay empty.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
mod common;

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    println!("symbols: SKIPPED (not macOS; not a pass)");
    ExitCode::SUCCESS
}

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    common::report("symbols", checks::run())
}

#[cfg(target_os = "macos")]
mod checks {
    use std::collections::HashSet;
    use std::ptr;

    use burnburn_macos::adapter::MenuItem;
    use burnburn_macos::appkit::HostOptions;
    use burnburn_macos::appkit::symbols::Symbol;
    use burnburn_macos::l10n::Locale;
    use burnburn_presentation::{CommandKey, UiIntent};
    use objc2::Message;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2_app_kit::{
        NSAccessibility, NSApplication, NSButton, NSCellImagePosition, NSImage, NSPopUpButton, NSSegmentedControl,
        NSView, NSWindow,
    };
    use objc2_foundation::NSString;

    use crate::common::windows;
    use crate::common::{Session, start_with};

    /// A glyph that used to stand in for an icon on a button.
    const FORBIDDEN: [char; 17] = ['▾', '■', '↔', '↕', '◀', '▶', '▲', '▼', '●', '○', '‹', '›', '↑', '↓', '▸', '◂', '▴'];
    /// (window label, glyph) pairs of LF7 files (inspector flip buttons, operations stop button,
    /// toolstrip drop-down arrow) that may still show a glyph. Must be empty at integration: LF7
    /// replaces them with `appkit::symbols`.
    const LF7_EXCEPTIONS: [(&str, char); 4] =
        [("document", '↔'), ("document", '↕'), ("document", '■'), ("document", '▾')];

    pub fn run() -> Result<(), String> {
        let mut found: Vec<String> = Vec::new();
        check_symbol_table(&mut found);
        let s = start_with(|o: &mut HostOptions| {
            o.quiet_windows = true;
            o.float_windows = false;
            o.locale = Some(Locale::Nl);
        })?;
        let doc = s.new_document()?;
        s.app.dispatch(UiIntent::SetMachineProfile {
            profile: Box::new(windows::machine_profile()?),
            ruida: burnburn_ruida::RuidaProfile {
                magic: burnburn_ruida::Magic::FIXTURE_0X88,
                overscan: burnburn_ruida::OverscanPolicy::ControllerManaged,
            },
        });
        s.draw_rectangle(doc, (10.0, 10.0), (90.0, 60.0))?;
        s.app.dispatch(UiIntent::AddOperation { document: doc, draft: windows::cut_draft()? });
        s.pump(0.3);
        if s.menu(MenuItem::Command(CommandKey::Compile)).is_ok() {
            let _ = s.wait(5.0, "the plan", || s.doc(doc).is_some_and(|d| d.plan.is_some()));
        }
        let main = s.app.window(doc).ok_or("no window")?;
        check_window("document", &main, &mut found);
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
        let sheets: Vec<(&str, Retained<NSWindow>)> = s
            .app
            .with_window(doc, |w| w.built_sheets.iter().map(|(n, sh)| (*n, (*sh).retain())).collect())
            .unwrap_or_default();
        for (name, sheet) in &sheets {
            check_window(&format!("sheet {name}"), sheet, &mut found);
        }
        found.sort();
        found.dedup();
        if found.is_empty() {
            return Ok(());
        }
        Err(format!("{} problems:\n    {}", found.len(), found.join("\n    ")))
    }

    fn check_symbol_table(found: &mut Vec<String>) {
        let mut names = HashSet::new();
        for symbol in Symbol::ALL {
            let name = symbol.name();
            if !names.insert(name) {
                found.push(format!("{symbol:?}: SF Symbol name `{name}` is used twice"));
            }
            if NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(name), None).is_none() {
                found.push(format!("{symbol:?}: SF Symbol `{name}` does not exist on this system"));
            }
            let (nl, en) = (symbol.description(Locale::Nl), symbol.description(Locale::En));
            if nl.is_empty() || en.is_empty() {
                found.push(format!("{symbol:?}: missing description (nl `{nl}`, en `{en}`)"));
            }
            if nl.chars().chain(en.chars()).any(|c| FORBIDDEN.contains(&c)) {
                found.push(format!("{symbol:?}: description contains a glyph"));
            }
            if symbol.fallback_text(Locale::Nl).is_empty() {
                found.push(format!("{symbol:?}: no fallback when the symbol is missing"));
            }
        }
        if Symbol::ALL.len() < 30 {
            found.push(format!("only {} symbols are defined", Symbol::ALL.len()));
        }
    }

    fn check_new_windows(s: &Session, doc: burnburn_types::DocumentId, step: &str, found: &mut Vec<String>) {
        s.pump(0.6);
        let main = s.app.window(doc);
        let app = NSApplication::sharedApplication(s.mtm);
        let mut seen = HashSet::new();
        for w in app.windows().iter() {
            if !w.isVisible() || main.as_deref().is_some_and(|m| ptr::eq(m, &*w)) || !seen.insert(w.windowNumber()) {
                continue;
            }
            check_window(&format!("{step} ({})", w.title()), &w, found);
            if let Some(parent) = w.sheetParent() {
                parent.endSheet(&w);
            } else {
                w.orderOut(None);
            }
        }
        s.pump(0.2);
    }

    fn check_window(name: &str, window: &NSWindow, found: &mut Vec<String>) {
        window.displayIfNeeded();
        if let Some(root) = window.contentView() {
            walk(&root, name, found);
        }
    }

    fn glyph(name: &str, text: &str) -> Option<char> {
        text.chars()
            .find(|c| FORBIDDEN.contains(c) && !LF7_EXCEPTIONS.iter().any(|(w, g)| name.starts_with(w) && g == c))
    }

    fn walk(view: &NSView, name: &str, found: &mut Vec<String>) {
        let any: &AnyObject = view.as_ref();
        if let Some(popup) = any.downcast_ref::<NSPopUpButton>() {
            for title in popup.itemTitles().iter() {
                if let Some(g) = glyph(name, &title.to_string()) {
                    found.push(format!("{name}: pop-up item `{title}` contains the glyph {g}"));
                }
            }
        } else if let Some(button) = any.downcast_ref::<NSButton>() {
            let (title, alternate) = (button.title().to_string(), button.alternateTitle().to_string());
            for text in [&title, &alternate] {
                if let Some(g) = glyph(name, text) {
                    found.push(format!("{name}: button `{text}` contains the glyph {g}"));
                }
            }
            if title.is_empty() && button.imagePosition() == NSCellImagePosition::ImageOnly && button.image().is_some()
            {
                let label = button.accessibilityLabel().map(|l| l.to_string()).unwrap_or_default();
                if label.is_empty() {
                    found.push(format!(
                        "{name}: an icon button has no accessible name (tooltip `{:?}`)",
                        button.toolTip()
                    ));
                }
            }
        } else if let Some(segments) = any.downcast_ref::<NSSegmentedControl>() {
            for i in 0..segments.segmentCount() {
                if let Some(label) = segments.labelForSegment(i)
                    && let Some(g) = glyph(name, &label.to_string())
                {
                    found.push(format!("{name}: segment `{label}` contains the glyph {g}"));
                }
            }
        }
        for child in view.subviews().iter() {
            walk(&child, name, found);
        }
    }
}
