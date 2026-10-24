//! Widget helpers shared by the panels: frames, labels, fields, buttons and the inspector column.

use burnburn_presentation::CommandKey;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, sel};
use objc2_app_kit::{NSAutoresizingMaskOptions, NSButton, NSLineBreakMode, NSTableView, NSTextField, NSView};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

use crate::adapter::{MenuItem, menu_tag};

pub(crate) fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}

pub(crate) const TOP_PINNED: NSAutoresizingMaskOptions = NSAutoresizingMaskOptions(
    NSAutoresizingMaskOptions::ViewMinYMargin.0 | NSAutoresizingMaskOptions::ViewWidthSizable.0,
);

pub(crate) fn label(mtm: MainThreadMarker, text: &str, frame: NSRect) -> Retained<NSTextField> {
    let label = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    label.setFrame(frame);
    label.setAutoresizingMask(TOP_PINNED);
    label
}

pub(crate) fn field(mtm: MainThreadMarker, frame: NSRect) -> Retained<NSTextField> {
    let field = NSTextField::textFieldWithString(&NSString::from_str(""), mtm);
    field.setFrame(frame);
    field.setAutoresizingMask(TOP_PINNED);
    // Commit on Return *and* on end of editing (Tab, click elsewhere): orchestrator decision; the
    // adapter drops a commit whose text equals the view model's, so Return + Tab is one command.
    if let Some(cell) = field.cell() {
        cell.setSendsActionOnEndEditing(true);
    }
    field
}

/// Rows selected in `table`, ascending.
pub(crate) fn selected_rows(table: &NSTableView) -> Vec<usize> {
    let set = table.selectedRowIndexes();
    let mut rows = Vec::new();
    let mut i = set.firstIndex();
    while i != usize::MAX && i != isize::MAX as usize && rows.len() < set.count() {
        rows.push(i);
        i = set.indexGreaterThanIndex(i);
    }
    rows
}

/// The inspector column, laid out from the top.
pub(crate) struct Column<'a> {
    pub(crate) mtm: MainThreadMarker,
    pub(crate) view: &'a NSView,
    pub(crate) y: f64,
    pub(crate) width: f64,
}

impl Column<'_> {
    pub(crate) fn add_label(&mut self, text: &str, height: f64) -> Retained<NSTextField> {
        self.y -= height;
        let l = label(self.mtm, text, rect(12.0, self.y, self.width, height));
        self.view.addSubview(&l);
        self.y -= 4.0;
        l
    }

    /// A full-width field below the cursor (the inspector now lays its fields out in rows).
    #[allow(dead_code)]
    pub(crate) fn add_field(&mut self) -> Retained<NSTextField> {
        self.y -= 24.0;
        let f = field(self.mtm, rect(12.0, self.y, self.width, 24.0));
        self.view.addSubview(&f);
        self.y -= 8.0;
        f
    }

    #[allow(unsafe_code)]
    pub(crate) fn add_button(&mut self, title: &str, target: &AnyObject, command: CommandKey) -> Retained<NSButton> {
        self.y -= 28.0;
        // SAFETY: `panelButton:` is a controller method taking the sender (`v@:@`); the target is
        // weak and DocWindow keeps the controller alive while the button exists (cleared in Drop).
        let b = unsafe {
            NSButton::buttonWithTitle_target_action(
                &NSString::from_str(title),
                Some(target),
                Some(sel!(panelButton:)),
                self.mtm,
            )
        };
        b.setFrame(rect(12.0, self.y, self.width, 28.0));
        b.setAutoresizingMask(TOP_PINNED);
        b.setTag(menu_tag(MenuItem::Command(command)));
        self.view.addSubview(&b);
        self.y -= 4.0;
        b
    }
}

#[allow(unsafe_code)]
pub(crate) fn button(
    mtm: MainThreadMarker,
    title: &str,
    target: &AnyObject,
    action: objc2::runtime::Sel,
    frame: NSRect,
) -> Retained<NSButton> {
    // SAFETY: `action` is a controller method taking the sender (`v@:@`); the target is weak and
    // DocWindow keeps the controller alive while the button exists (cleared in Drop).
    let b =
        unsafe { NSButton::buttonWithTitle_target_action(&NSString::from_str(title), Some(target), Some(action), mtm) };
    b.setFrame(frame);
    // Top-pinned with the width it was given: a row of buttons must not stretch into each other.
    b.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
    b
}

pub(crate) fn strip_label(
    mtm: MainThreadMarker,
    view: &NSView,
    text: &str,
    frame: NSRect,
    wrap: bool,
) -> Retained<NSTextField> {
    let l = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    l.setFrame(frame);
    l.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
    if wrap {
        l.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
    }
    view.addSubview(&l);
    l
}

/// A push button whose target/action are set later (once the window controller exists).
#[allow(unsafe_code)]
pub(crate) fn push_button(mtm: MainThreadMarker, title: &str) -> Retained<NSButton> {
    // SAFETY: no target and no action yet: clicking does nothing until both are set.
    unsafe { NSButton::buttonWithTitle_target_action(&NSString::from_str(title), None, None, mtm) }
}

pub(crate) fn set_button_title(button: &NSButton, title: &str) {
    if button.title().to_string() != title {
        button.setTitle(&NSString::from_str(title));
    }
}

/// Text in the error colour: it starts with the error glyph, so the colour is not the only
/// carrier (LF15). Empty text stays empty.
pub(crate) fn set_error_text(label: &NSTextField, text: &str) {
    set_text(label, &burnburn_host_models::style::toned_text(burnburn_host_models::style::StatusTone::Error, text));
}

pub(crate) fn set_text(label: &NSTextField, text: &str) {
    if label.stringValue().to_string() != text {
        label.setStringValue(&NSString::from_str(text));
    }
}

/// Moves `caption` so that its first text baseline is the one of `field` (both lie in one
/// non-flipped parent). A caption and the field beside it read as one line (LF6c).
#[allow(unsafe_code)]
pub(crate) fn align_baseline(caption: &objc2_app_kit::NSView, field: &objc2_app_kit::NSView) {
    let offset = |v: &objc2_app_kit::NSView| -> f64 {
        // SAFETY: `firstBaselineOffsetFromTop` is an NSView property returning a CGFloat.
        unsafe { objc2::msg_send![v, firstBaselineOffsetFromTop] }
    };
    let f = field.frame();
    let target = f.origin.y + f.size.height - offset(field);
    let c = caption.frame();
    caption.setFrameOrigin(objc2_foundation::NSPoint::new(c.origin.x, target - c.size.height + offset(caption)));
}
