//! Widgets shared by the text panels (W3-UI-tekst): one target object that turns every control's
//! action into a `NativeEvent::TextPanel` (slot + value), builders for the controls, and a small
//! top-down form layout. The panels only copy the headless models of `crate::panels::text` into
//! these widgets; no decision is taken here.

use std::cell::Cell;
use std::rc::Weak;

use burnburn_types::DocumentId;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAccessibility, NSButton, NSColor, NSControl, NSControlStateValueOn, NSFont, NSLineBreakMode, NSPopUpButton,
    NSSegmentSwitchTracking, NSSegmentedControl, NSTextField, NSView,
};
use objc2_foundation::{NSObject, NSString};

use super::widgets::{align_baseline, field, label, push_button, rect, set_text};
use crate::adapter::NativeEvent;
use crate::appkit::app::HostApp;
use crate::appkit::classes::ffi_guard;
use crate::panels::{Slot, SlotKind, SlotValue};
use burnburn_host_models::form::split_caption;
use burnburn_host_models::format::unit_label;

pub(crate) const MARGIN: f64 = 12.0;
pub(crate) const LABEL_WIDTH: f64 = 180.0;
const GAP: f64 = 4.0;

pub(crate) struct TargetIvars {
    app: Weak<HostApp>,
    document: DocumentId,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnTextPanelTarget"]
    #[ivars = TargetIvars]
    pub(crate) struct TextTarget;

    impl TextTarget {
        /// The one action of every control of the text panels: the slot is the control's tag, the
        /// value is read from the control; presentation decides what it means.
        #[unsafe(method(slotChanged:))]
        fn slot_changed(&self, sender: Option<&AnyObject>) {
            ffi_guard("slotChanged:", (), || {
                let Some(control) = sender.and_then(|s| s.downcast_ref::<NSControl>()) else { return };
                let Some(slot) = Slot::from_tag(control.tag()) else { return };
                let ivars = self.ivars();
                let Some(app) = ivars.app.upgrade() else { return };
                let value = match slot.kind() {
                    SlotKind::Press => SlotValue::Press,
                    SlotKind::Csv => {
                        app.show_csv_panel(ivars.document);
                        return;
                    }
                    SlotKind::Toggle => match control.downcast_ref::<NSButton>() {
                        Some(b) => SlotValue::Toggle(b.state() == NSControlStateValueOn),
                        None => return,
                    },
                    SlotKind::Text => match control.downcast_ref::<NSTextField>() {
                        Some(f) => SlotValue::Text(f.stringValue().to_string()),
                        None => return,
                    },
                    SlotKind::Choice => {
                        let index = if let Some(p) = control.downcast_ref::<NSPopUpButton>() {
                            p.indexOfSelectedItem()
                        } else if let Some(s) = control.downcast_ref::<NSSegmentedControl>() {
                            s.selectedSegment()
                        } else {
                            return;
                        };
                        match usize::try_from(index) {
                            Ok(i) => SlotValue::Choice(i),
                            Err(_) => return,
                        }
                    }
                };
                app.native(NativeEvent::TextPanel { document: ivars.document, slot, value });
            });
        }
    }
);

/// The target of one window's text panels.
#[allow(unsafe_code)]
pub(crate) fn new_target(mtm: MainThreadMarker, app: Weak<HostApp>, document: DocumentId) -> Retained<TextTarget> {
    let this = TextTarget::alloc(mtm).set_ivars(TargetIvars { app, document });
    // SAFETY: NSObject's designated initialiser on a freshly allocated instance with ivars set.
    unsafe { msg_send![super(this), init] }
}

/// Gives `control` its slot as tag and the target's action. The target is weak in AppKit: the panel
/// owning the control keeps the target alive and nils it in `clear_targets` before it goes.
#[allow(unsafe_code)]
pub(crate) fn wire(control: &NSControl, target: &TextTarget, slot: Slot) -> Retained<NSControl> {
    control.setTag(slot.tag());
    let any: &AnyObject = target.as_ref();
    // SAFETY: the target is weak and kept alive by the owning section.
    unsafe { control.setTarget(Some(any)) };
    // SAFETY: `slotChanged:` takes the sender (`v@:@`).
    unsafe { control.setAction(Some(sel!(slotChanged:))) };
    control.retain()
}

#[allow(unsafe_code)]
pub(crate) fn unwire(controls: &[Retained<NSControl>]) {
    for c in controls {
        // SAFETY: a nil target cannot dangle.
        unsafe { c.setTarget(None) };
    }
}

/// A caption: small, secondary colour.
pub(crate) fn caption(mtm: MainThreadMarker, text: &str) -> Retained<NSTextField> {
    let l = label(mtm, text, rect(0.0, 0.0, 100.0, 20.0));
    l.setFont(Some(&NSFont::systemFontOfSize(11.0)));
    l.setTextColor(Some(&NSColor::secondaryLabelColor()));
    l
}

/// A wrapping message line; `error` makes it red.
pub(crate) fn message_line(mtm: MainThreadMarker, error: bool) -> Retained<NSTextField> {
    let l = label(mtm, "", rect(0.0, 0.0, 100.0, 30.0));
    l.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
    l.setMaximumNumberOfLines(0);
    l.setFont(Some(&NSFont::systemFontOfSize(11.0)));
    let color = if error { NSColor::systemRedColor() } else { NSColor::secondaryLabelColor() };
    l.setTextColor(Some(&color));
    l
}

pub(crate) fn text_field(mtm: MainThreadMarker, target: &TextTarget, slot: Slot, name: &str) -> Retained<NSTextField> {
    let f = field(mtm, rect(0.0, 0.0, 100.0, 24.0));
    f.setAccessibilityLabel(Some(&NSString::from_str(name)));
    wire(&f, target, slot);
    f
}

#[allow(unsafe_code)]
pub(crate) fn check(mtm: MainThreadMarker, target: &TextTarget, slot: Slot, title: &str) -> Retained<NSButton> {
    // SAFETY: no target and no action yet; `wire` sets both.
    let b = unsafe { NSButton::checkboxWithTitle_target_action(&NSString::from_str(title), None, None, mtm) };
    wire(&b, target, slot);
    b
}

pub(crate) fn popup(mtm: MainThreadMarker, target: &TextTarget, slot: Slot, name: &str) -> Retained<NSPopUpButton> {
    let p = NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), rect(0.0, 0.0, 100.0, 26.0), false);
    p.setAccessibilityLabel(Some(&NSString::from_str(name)));
    wire(&p, target, slot);
    p
}

pub(crate) fn push(mtm: MainThreadMarker, target: &TextTarget, slot: Slot, title: &str) -> Retained<NSButton> {
    let b = push_button(mtm, title);
    wire(&b, target, slot);
    b
}

pub(crate) fn segments(
    mtm: MainThreadMarker,
    target: &TextTarget,
    slot: Slot,
    titles: &[&str],
    name: &str,
) -> Retained<NSSegmentedControl> {
    let s = NSSegmentedControl::new(mtm);
    s.setSegmentCount(titles.len() as isize);
    s.setTrackingMode(NSSegmentSwitchTracking::SelectOne);
    for (i, title) in titles.iter().enumerate() {
        s.setLabel_forSegment(&NSString::from_str(title), i as isize);
    }
    s.setAccessibilityLabel(Some(&NSString::from_str(name)));
    wire(&s, target, slot);
    s
}

/// Fills a pop-up with `titles` (with enablement and tooltip per entry) and selects `selected`.
/// Rewritten only when the entries changed, so an open menu is not disturbed.
pub(crate) fn set_popup_items(p: &NSPopUpButton, items: &[(String, bool, String)], selected: usize) {
    let current: Vec<String> = p.itemTitles().iter().map(|t| t.to_string()).collect();
    let wanted: Vec<&str> = items.iter().map(|i| i.0.as_str()).collect();
    if current.iter().map(String::as_str).collect::<Vec<_>>() != wanted {
        p.removeAllItems();
        for (title, _, _) in items {
            p.addItemWithTitle(&NSString::from_str(title));
        }
    }
    for (i, (_, enabled, tip)) in items.iter().enumerate() {
        if let Some(item) = p.itemAtIndex(i as isize) {
            item.setEnabled(*enabled);
            let tooltip = (!tip.is_empty()).then(|| NSString::from_str(tip));
            item.setToolTip(tooltip.as_deref());
        }
    }
    if p.indexOfSelectedItem() != selected as isize {
        p.selectItemAtIndex(selected as isize);
    }
}

pub(crate) fn set_field(f: &NSTextField, text: &str, keep_typed: bool) {
    if !keep_typed {
        set_text(f, text);
    }
}

pub(crate) fn set_check(b: &NSButton, on: bool, enabled: bool, tip: &str) {
    let state = if on { NSControlStateValueOn } else { objc2_app_kit::NSControlStateValueOff };
    if b.state() != state {
        b.setState(state);
    }
    b.setEnabled(enabled);
    let tooltip = (!tip.is_empty()).then(|| NSString::from_str(tip));
    b.setToolTip(tooltip.as_deref());
}

/// One row of a form.
pub(crate) enum Cells {
    /// A caption on the left, the control on the right.
    Labeled(Retained<NSTextField>, Retained<NSView>),
    /// One control across the width.
    Full(Retained<NSView>),
    /// Controls side by side, equally wide.
    Side(Vec<Retained<NSView>>),
}

pub(crate) struct FormRow {
    pub(crate) cells: Cells,
    pub(crate) height: f64,
    pub(crate) visible: Cell<bool>,
    /// The unit after the control: it left the caption (`Breedte (mm)` is `Breedte` and `mm`).
    suffix: Option<Retained<NSTextField>>,
}

/// Room between a control and its unit.
const SUFFIX_GAP: f64 = 4.0;

impl FormRow {
    pub(crate) fn new(cells: Cells, height: f64) -> FormRow {
        FormRow { cells, height, visible: Cell::new(true), suffix: None }
    }

    fn views(&self) -> Vec<&NSView> {
        match &self.cells {
            Cells::Labeled(l, c) => {
                let mut v: Vec<&NSView> = vec![&**l, &**c];
                v.extend(self.suffix.iter().map(|s| &****s as &NSView));
                v
            }
            Cells::Full(c) => vec![&**c],
            Cells::Side(cs) => cs.iter().map(|c| &**c).collect(),
        }
    }
}

/// A top-down form on a content view.
pub(crate) struct Form {
    pub(crate) content: Retained<NSView>,
    pub(crate) rows: Vec<FormRow>,
    pub(crate) width: f64,
}

impl Form {
    pub(crate) fn new(mtm: MainThreadMarker, width: f64, mut rows: Vec<FormRow>) -> Form {
        let content = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, width, 100.0));
        content.setAutoresizesSubviews(false);
        // A unit in the caption becomes the suffix after the control; what else was in the
        // brackets is the control's tooltip.
        for row in &mut rows {
            if let Cells::Labeled(l, c) = &row.cells {
                let split = split_caption(&l.stringValue().to_string());
                if let Some(unit) = split.unit {
                    l.setStringValue(&NSString::from_str(&split.label));
                    if let Some(hint) = &split.hint {
                        c.setToolTip(Some(&NSString::from_str(hint)));
                    }
                    let suffix = label(mtm, unit_label(unit), rect(0.0, 0.0, 40.0, 20.0));
                    suffix.setTextColor(Some(&NSColor::secondaryLabelColor()));
                    content.addSubview(&suffix);
                    row.suffix = Some(suffix);
                }
            }
        }
        for row in &rows {
            for v in row.views() {
                content.addSubview(v);
            }
        }
        Form { content, rows, width }
    }

    /// Places the visible rows from the top and returns the content height they need.
    pub(crate) fn lay_out(&self) -> f64 {
        let total: f64 =
            self.rows.iter().filter(|r| r.visible.get()).map(|r| r.height + GAP).sum::<f64>() + 2.0 * MARGIN - GAP;
        let mut y = total - MARGIN;
        let inner = self.width - 2.0 * MARGIN;
        for row in &self.rows {
            let show = row.visible.get();
            for v in row.views() {
                v.setHidden(!show);
            }
            if !show {
                continue;
            }
            y -= row.height;
            match &row.cells {
                Cells::Labeled(l, c) => {
                    let mut control_w = inner - LABEL_WIDTH - 6.0;
                    if let Some(suffix) = &row.suffix {
                        let w = suffix.cell().map_or(24.0, |cell| cell.cellSize().width.ceil()) + 4.0;
                        control_w -= w + SUFFIX_GAP;
                        suffix.setFrame(rect(MARGIN + LABEL_WIDTH + 6.0 + control_w + SUFFIX_GAP, y, w, 20.0));
                    }
                    l.setFrame(rect(MARGIN, y + (row.height - 20.0) / 2.0, LABEL_WIDTH, 20.0));
                    c.setFrame(rect(MARGIN + LABEL_WIDTH + 6.0, y, control_w, row.height));
                    align_baseline(l, c);
                    if let Some(suffix) = &row.suffix {
                        align_baseline(suffix, c);
                    }
                }
                Cells::Full(c) => c.setFrame(rect(MARGIN, y, inner, row.height)),
                Cells::Side(cs) => {
                    let n = cs.len() as f64;
                    let w = (inner - 6.0 * (n - 1.0)) / n;
                    for (i, c) in cs.iter().enumerate() {
                        c.setFrame(rect(MARGIN + i as f64 * (w + 6.0), y, w, row.height));
                    }
                }
            }
            y -= GAP;
        }
        total
    }
}
