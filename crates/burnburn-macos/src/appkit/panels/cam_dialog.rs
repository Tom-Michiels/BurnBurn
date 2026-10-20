//! One generic panel for the CAM dialogs (W3-UI-cam): Optimization Settings, Material / Interval /
//! Focus Test and Device Settings. A floating utility panel built once from the dialog's rows (their
//! number and kind are fixed per dialog); every refresh copies presentation's `DialogViewModel`
//! into the widgets. The widgets only forward what the user did — a changed field, a pressed
//! button — to the host, which turns it into an intent (`crate::panels::cam_*_intent`).
//!
//! The panel has no close button: Close is a button of the dialog, so presentation always knows
//! whether the draft exists.

use std::rc::Weak;

use burnburn_document::process::OrderKey;
use burnburn_presentation::{
    CamDialog, DialogActionKind, DialogControl, DialogField, DialogValue, DialogViewModel, FieldState,
};
use burnburn_types::DocumentId;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAccessibility, NSAutoresizingMaskOptions, NSBackingStoreType, NSButton, NSColor, NSControl, NSControlStateValueOff,
    NSControlStateValueOn, NSFont, NSLineBreakMode, NSPanel, NSPopUpButton, NSScrollView, NSTextField, NSView,
    NSWindowStyleMask,
};
use objc2_foundation::{NSObject, NSPoint, NSString};

use crate::appkit::app::HostApp;
use crate::appkit::classes::ffi_guard;
use crate::appkit::panels::widgets::{field, label, push_button, rect, set_text};
use crate::l10n::{Locale, message_or_key};
use crate::panels::{
    cam_action_title, cam_caption, cam_choice_label, cam_error, cam_note, cam_result_text, cam_support_line, cam_title,
    order_key_caption,
};

const WIDTH: f64 = 700.0;
const MARGIN: f64 = 14.0;
const CAPTION_W: f64 = 250.0;
const CONTROL_W: f64 = 190.0;
const ROW_H: f64 = 30.0;
const NOTES_LINE_H: f64 = 14.0;
const BUTTON_BAR_H: f64 = 46.0;
const MAX_SCROLL_H: f64 = 560.0;
const ORDER_KEYS: [OrderKey; 3] = [OrderKey::Layer, OrderKey::Group, OrderKey::Priority];
/// Buttons in the order of the bar, left to right.
const ACTION_KINDS: [DialogActionKind; 5] = [
    DialogActionKind::ReturnToFlatbed,
    DialogActionKind::Generate,
    DialogActionKind::ExportRd,
    DialogActionKind::Close,
    DialogActionKind::Apply,
];
/// Tags of the order buttons: `UP_TAG + index in ORDER_KEYS`, `DOWN_TAG + …`.
const UP_TAG: isize = 10;
const DOWN_TAG: isize = 20;

/// The control of one field.
#[derive(Clone)]
pub(crate) enum Control {
    Text(Retained<NSTextField>),
    Flag(Retained<NSButton>),
    Popup(Retained<NSPopUpButton>),
}

impl Control {
    fn view(&self) -> &NSView {
        match self {
            Control::Text(f) => f,
            Control::Flag(b) => b,
            Control::Popup(p) => p,
        }
    }

    fn control(&self) -> &NSControl {
        match self {
            Control::Text(f) => f,
            Control::Flag(b) => b,
            Control::Popup(p) => p,
        }
    }
}

impl RowUi {
    /// The control as a plain `NSControl` (tests set its value and send its action).
    pub(crate) fn control_ref(&self) -> &NSControl {
        self.control.control()
    }

    /// The arrows beside an `Order by` level.
    pub(crate) fn arrows(&self) -> Option<(OrderKey, &NSButton, &NSButton)> {
        match (self.field, &self.up, &self.down) {
            (DialogField::OrderEnabled(key), Some(up), Some(down)) => Some((key, up, down)),
            _ => None,
        }
    }
}

/// Everything one field consists of.
pub(crate) struct RowUi {
    pub(crate) field: DialogField,
    pub(crate) caption: Retained<NSTextField>,
    pub(crate) control: Control,
    /// The error, or the reason the field is greyed.
    pub(crate) note: Retained<NSTextField>,
    up: Option<Retained<NSButton>>,
    down: Option<Retained<NSButton>>,
}

pub(crate) struct TargetIvars {
    app: Weak<HostApp>,
    document: DocumentId,
    dialog: CamDialog,
    /// The field of each control, by its tag.
    fields: Vec<DialogField>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnCamTarget"]
    #[ivars = TargetIvars]
    pub(crate) struct CamTarget;

    impl CamTarget {
        /// A field changed: Return or end of editing of a text field, a tick, a pop-up choice.
        #[unsafe(method(camField:))]
        fn cam_field(&self, sender: Option<&AnyObject>) {
            ffi_guard("camField:", (), || {
                let Some(control) = sender.and_then(|s| s.downcast_ref::<NSControl>()) else { return };
                let ivars = self.ivars();
                let Some(field) = usize::try_from(control.tag()).ok().and_then(|i| ivars.fields.get(i)) else { return };
                let value = if let Some(p) = control.downcast_ref::<NSPopUpButton>() {
                    DialogValue::Choice(usize::try_from(p.indexOfSelectedItem()).unwrap_or(0))
                } else if let Some(b) = control.downcast_ref::<NSButton>() {
                    DialogValue::Flag(b.state() == NSControlStateValueOn)
                } else if let Some(f) = control.downcast_ref::<NSTextField>() {
                    DialogValue::Text(f.stringValue().to_string())
                } else {
                    return;
                };
                if let Some(app) = ivars.app.upgrade() {
                    app.cam_field_changed(ivars.document, ivars.dialog, *field, value);
                }
            });
        }

        /// A button of the bar.
        #[unsafe(method(camAction:))]
        fn cam_action(&self, sender: Option<&AnyObject>) {
            ffi_guard("camAction:", (), || {
                let Some(button) = sender.and_then(|s| s.downcast_ref::<NSControl>()) else { return };
                let ivars = self.ivars();
                let Some(kind) = usize::try_from(button.tag()).ok().and_then(|i| ACTION_KINDS.get(i)) else { return };
                if let Some(app) = ivars.app.upgrade() {
                    app.cam_button_pressed(ivars.document, ivars.dialog, *kind);
                }
            });
        }

        /// An arrow beside an `Order by` level.
        #[unsafe(method(camMove:))]
        fn cam_move(&self, sender: Option<&AnyObject>) {
            ffi_guard("camMove:", (), || {
                let Some(button) = sender.and_then(|s| s.downcast_ref::<NSControl>()) else { return };
                let ivars = self.ivars();
                let tag = button.tag();
                let (up, index) = if tag >= DOWN_TAG { (false, tag - DOWN_TAG) } else { (true, tag - UP_TAG) };
                let Some(key) = usize::try_from(index).ok().and_then(|i| ORDER_KEYS.get(i)) else { return };
                if let Some(app) = ivars.app.upgrade() {
                    app.cam_order_moved(ivars.document, *key, up);
                }
            });
        }
    }
);

/// The panel of one dialog in one window.
pub(crate) struct CamPanelUi {
    pub(crate) dialog: CamDialog,
    target: Retained<CamTarget>,
    pub(crate) panel: Retained<NSPanel>,
    pub(crate) rows: Vec<RowUi>,
    scroll_doc: Retained<NSView>,
    pub(crate) notes: Retained<NSTextField>,
    pub(crate) result: Retained<NSTextField>,
    pub(crate) capabilities: Retained<NSTextField>,
    pub(crate) buttons: Vec<(DialogActionKind, Retained<NSButton>)>,
    capability_lines: usize,
    rows_count: usize,
}

#[allow(unsafe_code)]
fn checkbox(mtm: MainThreadMarker) -> Retained<NSButton> {
    // SAFETY: no target and no action yet; both are set once the target object exists.
    unsafe { NSButton::checkboxWithTitle_target_action(&NSString::from_str(""), None, None, mtm) }
}

#[allow(unsafe_code)]
fn wire(control: &NSControl, target: &AnyObject, action: objc2::runtime::Sel, tag: isize) {
    control.setTag(tag);
    // SAFETY: the target is weak; the panel keeps it alive and clears it in `clear_targets`.
    unsafe { control.setTarget(Some(target)) };
    // SAFETY: every action takes the sender (`v@:@`).
    unsafe { control.setAction(Some(action)) };
}

impl CamPanelUi {
    /// Builds the panel for the rows `vm` has now (their number and kind do not change).
    #[allow(unsafe_code)]
    pub(crate) fn new(
        mtm: MainThreadMarker,
        app: Weak<HostApp>,
        document: DocumentId,
        vm: &DialogViewModel,
        locale: Locale,
    ) -> CamPanelUi {
        let dialog = vm.dialog;
        let rows_count = vm.rows.len();
        let capability_lines = vm.capabilities.len();
        let notes_h = NOTES_LINE_H * 6.0;
        let result_h = if dialog.is_test() { NOTES_LINE_H * 7.0 } else { 0.0 };
        let caps_h = capability_lines as f64 * 15.0 + if capability_lines > 0 { 22.0 } else { 0.0 };
        let content_h = rows_count as f64 * ROW_H + notes_h + result_h + caps_h + 3.0 * MARGIN;
        let scroll_h = content_h.min(MAX_SCROLL_H);
        let panel_h = scroll_h + BUTTON_BAR_H;

        let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
            NSPanel::alloc(mtm),
            rect(0.0, 0.0, WIDTH, panel_h),
            NSWindowStyleMask::Titled | NSWindowStyleMask::UtilityWindow,
            NSBackingStoreType::Buffered,
            false,
        );
        // SAFETY: ownership stays with this Retained.
        unsafe { panel.setReleasedWhenClosed(false) };
        panel.setFloatingPanel(true);
        panel.setHidesOnDeactivate(false);
        panel.setTitle(&NSString::from_str(cam_title(dialog, locale)));
        let content = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, WIDTH, panel_h));

        let doc_view = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, WIDTH - 16.0, content_h));
        let mut fields = Vec::new();
        let mut rows = Vec::new();
        for (index, row) in vm.rows.iter().enumerate() {
            let caption = label(mtm, "", rect(MARGIN, 0.0, CAPTION_W, 20.0));
            let control = match row.field.control() {
                DialogControl::Text => Control::Text(field(mtm, rect(MARGIN + CAPTION_W + 8.0, 0.0, CONTROL_W, 24.0))),
                DialogControl::Flag => Control::Flag(checkbox(mtm)),
                DialogControl::Choice => {
                    let p = NSPopUpButton::initWithFrame_pullsDown(
                        NSPopUpButton::alloc(mtm),
                        rect(MARGIN + CAPTION_W + 8.0, 0.0, CONTROL_W, 26.0),
                        false,
                    );
                    p.setAutoenablesItems(false);
                    Control::Popup(p)
                }
            };
            let note = label(mtm, "", rect(MARGIN + CAPTION_W + CONTROL_W + 16.0, 0.0, WIDTH - 3.0 * MARGIN - CAPTION_W - CONTROL_W - 8.0, 28.0));
            note.setFont(Some(&NSFont::systemFontOfSize(10.0)));
            note.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
            let (up, down) = if matches!(row.field, DialogField::OrderEnabled(_)) {
                let arrow = |glyph: &str| {
                    let b = push_button(mtm, glyph);
                    b.setFrame(rect(0.0, 0.0, 26.0, 24.0));
                    b
                };
                (Some(arrow("\u{2191}")), Some(arrow("\u{2193}")))
            } else {
                (None, None)
            };
            for v in [&*caption as &NSView, control.view(), &*note] {
                doc_view.addSubview(v);
            }
            for b in up.iter().chain(down.iter()) {
                doc_view.addSubview(b);
            }
            fields.push(row.field);
            rows.push(RowUi { field: row.field, caption, control, note, up, down });
            let _ = index;
        }
        let notes = label(mtm, "", rect(MARGIN, 0.0, WIDTH - 3.0 * MARGIN, notes_h));
        notes.setFont(Some(&NSFont::systemFontOfSize(11.0)));
        notes.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
        let result = label(mtm, "", rect(MARGIN, 0.0, WIDTH - 3.0 * MARGIN, result_h));
        result.setFont(Some(&NSFont::systemFontOfSize(11.0)));
        result.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
        let capabilities = label(mtm, "", rect(MARGIN, 0.0, WIDTH - 3.0 * MARGIN, caps_h));
        capabilities.setFont(Some(&NSFont::systemFontOfSize(11.0)));
        capabilities.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
        for v in [&*notes as &NSView, &*result, &*capabilities] {
            doc_view.addSubview(v);
        }
        result.setHidden(!dialog.is_test());
        capabilities.setHidden(capability_lines == 0);

        let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(0.0, BUTTON_BAR_H, WIDTH, scroll_h));
        scroll.setHasVerticalScroller(true);
        scroll.setDrawsBackground(false);
        scroll.setAutoresizingMask(NSAutoresizingMaskOptions(
            NSAutoresizingMaskOptions::ViewWidthSizable.0 | NSAutoresizingMaskOptions::ViewHeightSizable.0,
        ));
        scroll.setDocumentView(Some(&doc_view));
        content.addSubview(&scroll);

        // The button bar, from the right: Apply / Close, then the dialog's own buttons.
        let mut buttons = Vec::new();
        for (slot, kind) in ACTION_KINDS.iter().enumerate() {
            let b = push_button(mtm, cam_action_title(*kind, dialog, locale));
            b.setFrame(rect(0.0, 10.0, 130.0, 28.0));
            b.setTag(slot as isize);
            content.addSubview(&b);
            buttons.push((*kind, b));
        }
        panel.setContentView(Some(&content));

        let this = CamTarget::alloc(mtm).set_ivars(TargetIvars { app, document, dialog, fields });
        // SAFETY: NSObject's designated initialiser on a freshly allocated instance with ivars set.
        let target: Retained<CamTarget> = unsafe { msg_send![super(this), init] };
        let any: &AnyObject = target.as_ref();
        for (i, r) in rows.iter().enumerate() {
            wire(r.control.control(), any, sel!(camField:), i as isize);
            for (b, base) in [(&r.up, UP_TAG), (&r.down, DOWN_TAG)] {
                if let (Some(b), DialogField::OrderEnabled(key)) = (b, r.field)
                    && let Some(k) = ORDER_KEYS.iter().position(|o| *o == key)
                {
                    wire(b, any, sel!(camMove:), base + k as isize);
                }
            }
        }
        for (kind, b) in &buttons {
            let slot = ACTION_KINDS.iter().position(|k| k == kind).unwrap_or(0);
            wire(b, any, sel!(camAction:), slot as isize);
        }
        // Apply is the default button (Return), Close answers Escape.
        for (kind, b) in &buttons {
            match kind {
                DialogActionKind::Apply | DialogActionKind::Generate => b.setKeyEquivalent(&NSString::from_str("\r")),
                DialogActionKind::Close => b.setKeyEquivalent(&NSString::from_str("\u{1b}")),
                _ => {}
            }
        }
        panel.center();
        CamPanelUi {
            dialog,
            target,
            panel,
            rows,
            scroll_doc: doc_view,
            notes,
            result,
            capabilities,
            buttons,
            capability_lines,
            rows_count,
        }
    }

    /// Copies the view model into the widgets: values, enablement, reasons, texts, buttons.
    pub(crate) fn refresh(&self, vm: &DialogViewModel, locale: Locale) {
        let content_h = self.scroll_doc.frame().size.height;
        let width = self.scroll_doc.frame().size.width;
        let mut y = content_h - MARGIN;
        for (slot, row_vm) in vm.rows.iter().enumerate() {
            let Some(ui) = self.rows.iter().find(|r| r.field == row_vm.field) else { continue };
            let _ = slot;
            y -= ROW_H;
            self.layout_row(ui, y, width);
            let caption = cam_caption(row_vm.field, vm, locale);
            let (enabled, hidden) = match row_vm.state {
                FieldState::Hidden => (false, true),
                FieldState::Editable => (true, false),
                FieldState::Disabled(_) => (false, false),
            };
            match &ui.control {
                Control::Text(f) => {
                    set_text(&ui.caption, &caption);
                    f.setAccessibilityLabel(Some(&NSString::from_str(&caption)));
                    // A field being typed in keeps its text; the draft has it after the commit.
                    if f.currentEditor().is_none()
                        && let DialogValue::Text(t) = &row_vm.value
                        && f.stringValue().to_string() != *t
                    {
                        f.setStringValue(&NSString::from_str(t));
                    }
                    f.setEnabled(enabled);
                    f.setHidden(hidden);
                }
                Control::Flag(b) => {
                    set_text(&ui.caption, "");
                    b.setTitle(&NSString::from_str(&caption));
                    b.setState(if row_vm.value.flag() { NSControlStateValueOn } else { NSControlStateValueOff });
                    b.setEnabled(enabled);
                    b.setHidden(hidden);
                }
                Control::Popup(p) => {
                    set_text(&ui.caption, &caption);
                    p.setAccessibilityLabel(Some(&NSString::from_str(&caption)));
                    self.fill_popup(p, row_vm, locale);
                    p.setEnabled(enabled);
                    p.setHidden(hidden);
                }
            }
            ui.caption.setHidden(hidden || matches!(ui.control, Control::Flag(_)));
            // The error beats the reason a field is greyed; neither: empty.
            let note = match (row_vm.error, row_vm.state) {
                (Some(why), _) => cam_error(why, locale),
                (None, FieldState::Disabled(Some(reason))) => message_or_key(reason, locale),
                _ => String::new(),
            };
            set_text(&ui.note, &note);
            let colour = if row_vm.error.is_some() { NSColor::systemRedColor() } else { NSColor::secondaryLabelColor() };
            ui.note.setTextColor(Some(&colour));
            ui.note.setHidden(hidden);
            if let (Some((can_up, can_down)), DialogField::OrderEnabled(key)) = (row_vm.reorder, row_vm.field) {
                for (b, can, up) in [(&ui.up, can_up, true), (&ui.down, can_down, false)] {
                    if let Some(b) = b {
                        b.setEnabled(can);
                        b.setHidden(false);
                        b.setAccessibilityLabel(Some(&NSString::from_str(&arrow_label(key, up, locale))));
                    }
                }
            }
        }
        let mut below = MARGIN;
        let mut place = |view: &NSTextField, height: f64| {
            view.setFrame(rect(MARGIN, below, width - 2.0 * MARGIN, height));
            below += height + MARGIN;
        };
        let notes_h = NOTES_LINE_H * 6.0;
        let result_h = if self.dialog.is_test() { NOTES_LINE_H * 7.0 } else { 0.0 };
        let caps_h = self.capability_lines as f64 * 15.0 + if self.capability_lines > 0 { 22.0 } else { 0.0 };
        // From the bottom: the capability list, the result, the notes, then the rows above.
        place(&self.capabilities, caps_h);
        place(&self.result, result_h);
        place(&self.notes, notes_h);
        let notes: Vec<String> = vm.notes.iter().map(|n| cam_note(*n, locale)).filter(|t| !t.is_empty()).collect();
        set_text(&self.notes, &notes.join("\n"));
        let capabilities: Vec<String> = vm.capabilities.iter().map(|c| cam_support_line(c, locale)).collect();
        set_text(&self.capabilities, &capabilities.join("\n"));
        let result = match (&vm.result, vm.busy) {
            (Some(r), _) => cam_result_text(r, locale),
            (None, true) => match locale {
                Locale::Nl => "Bezig met compileren…".to_owned(),
                Locale::En => "Compiling…".to_owned(),
            },
            (None, false) => String::new(),
        };
        set_text(&self.result, &result);
        self.layout_buttons(vm, locale);
    }

    fn layout_row(&self, ui: &RowUi, y: f64, width: f64) {
        let control_x = MARGIN + CAPTION_W + 8.0;
        ui.caption.setFrame(rect(MARGIN, y + 4.0, CAPTION_W, 20.0));
        match &ui.control {
            Control::Text(f) => f.setFrame(rect(control_x, y + 2.0, CONTROL_W, 24.0)),
            Control::Popup(p) => p.setFrame(rect(control_x, y + 1.0, CONTROL_W, 26.0)),
            // A checkbox carries its own caption and spans caption and control.
            Control::Flag(b) => b.setFrame(rect(MARGIN, y + 3.0, CAPTION_W + CONTROL_W + 8.0 - 66.0, 22.0)),
        }
        if let (Some(up), Some(down)) = (&ui.up, &ui.down) {
            let x = MARGIN + CAPTION_W + CONTROL_W + 8.0 - 58.0;
            up.setFrame(rect(x, y + 3.0, 26.0, 24.0));
            down.setFrame(rect(x + 30.0, y + 3.0, 26.0, 24.0));
        }
        let note_x = MARGIN + CAPTION_W + CONTROL_W + 16.0;
        ui.note.setFrame(rect(note_x, y, (width - note_x - MARGIN).max(80.0), 28.0));
    }

    fn fill_popup(&self, p: &NSPopUpButton, row: &burnburn_presentation::DialogRow, locale: Locale) {
        let choices = row.field.choices();
        let titles: Vec<String> = p.itemTitles().iter().map(|t| t.to_string()).collect();
        let wanted: Vec<&str> = choices.iter().map(|c| cam_choice_label(*c, locale)).collect();
        if titles.iter().map(String::as_str).collect::<Vec<_>>() != wanted {
            p.removeAllItems();
            for title in &wanted {
                p.addItemWithTitle(&NSString::from_str(title));
            }
        }
        for (i, choice) in choices.iter().enumerate() {
            if let Some(item) = p.itemAtIndex(i as isize) {
                let blocked = row.blocked_choices.iter().find(|(c, _)| c == choice);
                item.setEnabled(blocked.is_none());
                let tip = blocked.map(|(_, reason)| message_or_key(*reason, locale));
                item.setToolTip(tip.as_deref().map(NSString::from_str).as_deref());
            }
        }
        if let DialogValue::Choice(i) = row.value {
            p.selectItemAtIndex(i as isize);
        }
    }

    /// Buttons right to left in the order of [`ACTION_KINDS`] reversed; the ones this dialog does not
    /// have are hidden.
    fn layout_buttons(&self, vm: &DialogViewModel, locale: Locale) {
        let mut x = WIDTH - MARGIN;
        for kind in ACTION_KINDS.iter().rev() {
            let Some((_, b)) = self.buttons.iter().find(|(k, _)| k == kind) else { continue };
            let state = vm.action(*kind);
            b.setHidden(state.is_none());
            let Some(state) = state else { continue };
            x -= 136.0;
            b.setFrame(rect(x, 10.0, 130.0, 28.0));
            b.setEnabled(state.enabled);
            set_button_title(b, cam_action_title(*kind, vm.dialog, locale));
            let tip = state.reason.map(|n| cam_note(n, locale));
            b.setToolTip(tip.as_deref().map(NSString::from_str).as_deref());
        }
    }

    /// Nil the targets (weak references) before the panel goes.
    #[allow(unsafe_code)]
    pub(crate) fn clear_targets(&self) {
        for r in &self.rows {
            // SAFETY: a nil target cannot dangle.
            unsafe { r.control.control().setTarget(None) };
            for b in r.up.iter().chain(r.down.iter()) {
                // SAFETY: as above.
                unsafe { b.setTarget(None) };
            }
        }
        for (_, b) in &self.buttons {
            // SAFETY: as above.
            unsafe { b.setTarget(None) };
        }
        let _ = (&self.target, self.rows_count);
    }

    pub(crate) fn show(&self) {
        // The document view is not flipped: start at its top.
        self.scroll_doc.scrollPoint(NSPoint::new(0.0, self.scroll_doc.frame().size.height));
        self.panel.makeKeyAndOrderFront(None);
    }

    pub(crate) fn hide(&self) {
        self.panel.orderOut(None);
    }
}

fn set_button_title(button: &NSButton, title: &str) {
    if button.title().to_string() != title {
        button.setTitle(&NSString::from_str(title));
    }
}

/// The accessibility label of an arrow beside an `Order by` level.
fn arrow_label(key: OrderKey, up: bool, locale: Locale) -> String {
    let word = match (up, locale) {
        (true, Locale::Nl) => "omhoog",
        (false, Locale::Nl) => "omlaag",
        (true, Locale::En) => "up",
        (false, Locale::En) => "down",
    };
    format!("{} {}", order_key_caption(key, locale), word)
}
