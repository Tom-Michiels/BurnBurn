//! The shape panel and the geometry dialog (W2-L1-tekenen): a floating utility panel with the
//! parameters of the selected basic shape and the options of the active tool, and the sheet that
//! asks for the value of Join, Round Corners and Optimize. Like every panel it only copies the
//! view model into widgets (`crate::panels::shapes_panel`) and forwards raw text as
//! `NativeEvent`s; it has its own small target object so the window controller stays untouched.

use std::rc::Weak;

use burnburn_presentation::AppViewModel;
use burnburn_types::DocumentId;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAccessibility, NSBackingStoreType, NSButton, NSColor, NSControlStateValueOff, NSControlStateValueOn, NSFont,
    NSLineBreakMode, NSPanel, NSTextField, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSObject, NSPoint, NSSize, NSString};

use crate::adapter::NativeEvent;
use crate::appkit::app::HostApp;
use crate::appkit::classes::ffi_guard;
use crate::appkit::panels::widgets::{field, label, push_button, rect, set_text};
use crate::appkit::window::DocWindow;
use crate::l10n::{HostText, Locale, host_text};
use crate::panels::{PARAM_SLOTS, ParamLine, ParamSection, geometry_dialog_panel, shapes_panel};

const PANEL_WIDTH: f64 = 300.0;
const ROW_HEIGHT: f64 = 28.0;
const TITLE_HEIGHT: f64 = 22.0;
const MARGIN: f64 = 12.0;
const CAPTION_WIDTH: f64 = 150.0;
const DIALOG_WIDTH: f64 = 380.0;
const DIALOG_HEIGHT: f64 = 170.0;

/// One row of a section: caption, text field and checkbox (one of the two is shown).
struct Row {
    caption: Retained<NSTextField>,
    field: Retained<NSTextField>,
    check: Retained<NSButton>,
}

pub(crate) struct TargetIvars {
    app: Weak<HostApp>,
    document: DocumentId,
    dialog_field: Retained<NSTextField>,
    /// Fields of the shape section then the tool section; the checkboxes in the same order.
    fields: Vec<Retained<NSTextField>>,
    checks: Vec<Retained<NSButton>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnShapesTarget"]
    #[ivars = TargetIvars]
    pub(crate) struct ShapesTarget;

    impl ShapesTarget {
        /// OK in the geometry dialog: the typed value goes to presentation unparsed.
        #[unsafe(method(geometryDialogOk:))]
        fn geometry_dialog_ok(&self, _sender: Option<&AnyObject>) {
            ffi_guard("geometryDialogOk:", (), || {
                let ivars = self.ivars();
                let text = ivars.dialog_field.stringValue().to_string();
                self.send(NativeEvent::GeometryDialogConfirmed { document: ivars.document, text });
            });
        }

        #[unsafe(method(geometryDialogCancel:))]
        fn geometry_dialog_cancel(&self, _sender: Option<&AnyObject>) {
            ffi_guard("geometryDialogCancel:", (), || {
                self.send(NativeEvent::GeometryDialogCancelled { document: self.ivars().document });
            });
        }

        /// A parameter or option field: Return and end of editing; the adapter drops a repeat.
        #[unsafe(method(paramCommitted:))]
        fn param_committed(&self, sender: Option<&AnyObject>) {
            ffi_guard("paramCommitted:", (), || {
                let Some(field) = sender.and_then(|s| s.downcast_ref::<NSTextField>()) else { return };
                let ivars = self.ivars();
                let Some(index) = ivars.fields.iter().position(|f| std::ptr::eq(field, &**f)) else { return };
                let text = field.stringValue().to_string();
                self.send(NativeEvent::ParamCommitted {
                    document: ivars.document,
                    section: section_of(index),
                    slot: index % PARAM_SLOTS,
                    text,
                });
            });
        }

        /// A checkbox row: `1` or `0`.
        #[unsafe(method(paramToggled:))]
        fn param_toggled(&self, sender: Option<&AnyObject>) {
            ffi_guard("paramToggled:", (), || {
                let Some(check) = sender.and_then(|s| s.downcast_ref::<NSButton>()) else { return };
                let ivars = self.ivars();
                let Some(index) = ivars.checks.iter().position(|c| std::ptr::eq(check, &**c)) else { return };
                let on = check.state() == NSControlStateValueOn;
                self.send(NativeEvent::ParamCommitted {
                    document: ivars.document,
                    section: section_of(index),
                    slot: index % PARAM_SLOTS,
                    text: if on { "1" } else { "0" }.to_owned(),
                });
            });
        }
    }
);

fn section_of(index: usize) -> ParamSection {
    if index < PARAM_SLOTS { ParamSection::Shape } else { ParamSection::Tool }
}

impl ShapesTarget {
    fn send(&self, event: NativeEvent) {
        if let Some(app) = self.ivars().app.upgrade() {
            app.native(event);
        }
    }
}

/// The panel, the dialog sheet and their widgets.
pub(crate) struct ShapesSection {
    target: Retained<ShapesTarget>,
    panel: Retained<NSPanel>,
    shape_title: Retained<NSTextField>,
    tool_title: Retained<NSTextField>,
    /// The shape section's rows, then the tool section's.
    rows: Vec<Row>,
    shown: bool,
    sheet: Retained<NSWindow>,
    dialog_title: Retained<NSTextField>,
    dialog_label: Retained<NSTextField>,
    dialog_field: Retained<NSTextField>,
    dialog_error: Retained<NSTextField>,
    dialog_ok: Retained<NSButton>,
    dialog_cancel: Retained<NSButton>,
    /// Whether the sheet is attached to the window now.
    dialog_shown: bool,
}

#[allow(unsafe_code)]
pub(crate) fn shapes_section(
    mtm: MainThreadMarker,
    app: Weak<HostApp>,
    document: DocumentId,
    locale: Locale,
) -> ShapesSection {
    let t = |text: HostText| host_text(text, locale);
    // --- the floating panel
    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(mtm),
        rect(0.0, 0.0, PANEL_WIDTH, 200.0),
        NSWindowStyleMask::Titled | NSWindowStyleMask::UtilityWindow,
        NSBackingStoreType::Buffered,
        false,
    );
    // SAFETY: ownership stays with this Retained.
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setFloatingPanel(true);
    panel.setHidesOnDeactivate(true);
    panel.setTitle(&NSString::from_str(t(HostText::ShapeSectionTitle)));
    let content = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, PANEL_WIDTH, 200.0));
    content.setAutoresizesSubviews(false);
    let title = || {
        let l = label(mtm, "", rect(MARGIN, 0.0, PANEL_WIDTH - 2.0 * MARGIN, TITLE_HEIGHT - 4.0));
        l.setFont(Some(&NSFont::boldSystemFontOfSize(12.0)));
        l
    };
    let (shape_title, tool_title) = (title(), title());
    content.addSubview(&shape_title);
    content.addSubview(&tool_title);
    let mut rows = Vec::new();
    let mut fields = Vec::new();
    let mut checks = Vec::new();
    for _ in 0..2 * PARAM_SLOTS {
        let caption = label(mtm, "", rect(MARGIN, 0.0, CAPTION_WIDTH, 20.0));
        let f =
            field(mtm, rect(MARGIN + CAPTION_WIDTH + 6.0, 0.0, PANEL_WIDTH - 2.0 * MARGIN - CAPTION_WIDTH - 6.0, 24.0));
        // SAFETY: no target and no action yet; both are set below, once the target object exists.
        let check = unsafe { NSButton::checkboxWithTitle_target_action(&NSString::from_str(""), None, None, mtm) };
        check.setFrame(rect(MARGIN, 0.0, PANEL_WIDTH - 2.0 * MARGIN, 22.0));
        for v in [&caption as &NSView, &f, &check] {
            v.setHidden(true);
            content.addSubview(v);
        }
        fields.push(f.clone());
        checks.push(check.clone());
        rows.push(Row { caption, field: f, check });
    }
    panel.setContentView(Some(&content));

    // --- the dialog sheet
    // SAFETY: a freshly allocated window with a valid rect, style and the buffered backing.
    let sheet = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            rect(0.0, 0.0, DIALOG_WIDTH, DIALOG_HEIGHT),
            NSWindowStyleMask::Titled,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: ownership stays with this Retained.
    unsafe { sheet.setReleasedWhenClosed(false) };
    let sheet_content = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, DIALOG_WIDTH, DIALOG_HEIGHT));
    let dialog_title = label(mtm, "", rect(16.0, DIALOG_HEIGHT - 36.0, DIALOG_WIDTH - 32.0, 20.0));
    dialog_title.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
    let dialog_label = label(mtm, "", rect(16.0, DIALOG_HEIGHT - 70.0, 200.0, 22.0));
    let dialog_field = field(mtm, rect(222.0, DIALOG_HEIGHT - 72.0, DIALOG_WIDTH - 238.0, 24.0));
    let dialog_error = label(mtm, "", rect(16.0, 52.0, DIALOG_WIDTH - 32.0, 34.0));
    dialog_error.setTextColor(Some(&NSColor::systemRedColor()));
    dialog_error.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
    let dialog_cancel = push_button(mtm, t(HostText::GeometryCancel));
    dialog_cancel.setFrame(rect(DIALOG_WIDTH - 232.0, 12.0, 104.0, 28.0));
    dialog_cancel.setKeyEquivalent(&NSString::from_str("\u{1b}"));
    let dialog_ok = push_button(mtm, t(HostText::GeometryOk));
    dialog_ok.setFrame(rect(DIALOG_WIDTH - 116.0, 12.0, 104.0, 28.0));
    dialog_ok.setKeyEquivalent(&NSString::from_str("\r"));
    for v in [&dialog_title as &NSView, &dialog_label, &dialog_field, &dialog_error, &dialog_cancel, &dialog_ok] {
        sheet_content.addSubview(v);
    }
    dialog_field.setAccessibilityLabel(Some(&NSString::from_str(t(HostText::GeometryOk))));
    sheet.setContentView(Some(&sheet_content));

    // --- the target
    let ivars = TargetIvars {
        app,
        document,
        dialog_field: dialog_field.clone(),
        fields: fields.clone(),
        checks: checks.clone(),
    };
    let this = ShapesTarget::alloc(mtm).set_ivars(ivars);
    // SAFETY: NSObject's designated initialiser on a freshly allocated instance with ivars set.
    let target: Retained<ShapesTarget> = unsafe { msg_send![super(this), init] };
    let any: &AnyObject = target.as_ref();
    for f in &fields {
        // SAFETY: the target is weak; the section keeps it alive and clears it in `clear_targets`.
        unsafe { f.setTarget(Some(any)) };
        // SAFETY: `paramCommitted:` takes the sender (`v@:@`).
        unsafe { f.setAction(Some(sel!(paramCommitted:))) };
    }
    for c in &checks {
        // SAFETY: as above.
        unsafe { c.setTarget(Some(any)) };
        // SAFETY: `paramToggled:` takes the sender (`v@:@`).
        unsafe { c.setAction(Some(sel!(paramToggled:))) };
    }
    for (b, action) in [(&dialog_ok, sel!(geometryDialogOk:)), (&dialog_cancel, sel!(geometryDialogCancel:))] {
        // SAFETY: as above.
        unsafe { b.setTarget(Some(any)) };
        // SAFETY: both actions take the sender (`v@:@`).
        unsafe { b.setAction(Some(action)) };
    }
    ShapesSection {
        target,
        panel,
        shape_title,
        tool_title,
        rows,
        shown: false,
        sheet,
        dialog_title,
        dialog_label,
        dialog_field,
        dialog_error,
        dialog_ok,
        dialog_cancel,
        dialog_shown: false,
    }
}

impl ShapesSection {
    /// Nil the targets (weak references) before the section goes: nothing can call a dead object.
    #[allow(unsafe_code)]
    pub(crate) fn clear_targets(&self) {
        for r in &self.rows {
            // SAFETY: a nil target cannot dangle.
            unsafe { r.field.setTarget(None) };
            // SAFETY: as above.
            unsafe { r.check.setTarget(None) };
        }
        for b in [&self.dialog_ok, &self.dialog_cancel] {
            // SAFETY: as above.
            unsafe { b.setTarget(None) };
        }
        let _ = &self.target;
    }

    /// Take the panel and the sheet away (the window is going).
    pub(crate) fn hide(&mut self, window: &NSWindow) {
        if self.dialog_shown {
            self.dialog_shown = false;
            window.endSheet(&self.sheet);
        }
        if self.shown {
            self.shown = false;
            self.panel.orderOut(None);
        }
    }

    /// The dialog sheet and the panel, for tests that drive them like a user.
    pub(crate) fn dialog_widgets(&self) -> (&NSTextField, &NSButton, &NSButton) {
        (&self.dialog_field, &self.dialog_ok, &self.dialog_cancel)
    }

    pub(crate) fn param_widgets(&self, index: usize) -> Option<(&NSTextField, &NSTextField, &NSButton)> {
        self.rows.get(index).map(|r| (&*r.caption, &*r.field, &*r.check))
    }

    pub(crate) fn panel_visible(&self) -> bool {
        self.shown
    }

    pub(crate) fn sheet(&self) -> &NSWindow {
        &self.sheet
    }

    pub(crate) fn dialog_visible(&self) -> bool {
        self.dialog_shown
    }
}

fn show_row(row: &Row, line: &ParamLine, y: f64) {
    row.caption.setFrame(rect(MARGIN, y + 2.0, CAPTION_WIDTH, 20.0));
    row.field.setFrame(rect(MARGIN + CAPTION_WIDTH + 6.0, y, PANEL_WIDTH - 2.0 * MARGIN - CAPTION_WIDTH - 6.0, 24.0));
    row.check.setFrame(rect(MARGIN, y, PANEL_WIDTH - 2.0 * MARGIN, 22.0));
    let caption = if line.error.is_empty() { line.label.clone() } else { format!("{} — {}", line.label, line.error) };
    if line.toggle {
        row.check.setTitle(&NSString::from_str(&line.label));
        row.check.setState(if line.text == "1" { NSControlStateValueOn } else { NSControlStateValueOff });
        row.caption.setHidden(true);
        row.field.setHidden(true);
        row.check.setHidden(false);
    } else {
        set_text(&row.caption, &caption);
        let color = if line.error.is_empty() { NSColor::labelColor() } else { NSColor::systemRedColor() };
        row.caption.setTextColor(Some(&color));
        // A field in error keeps what the user typed, so it can be corrected.
        if line.error.is_empty() && row.field.stringValue().to_string() != line.text {
            row.field.setStringValue(&NSString::from_str(&line.text));
        }
        row.caption.setHidden(false);
        row.field.setHidden(false);
        row.check.setHidden(true);
    }
}

impl DocWindow {
    /// Show, fill or hide the shape panel and the geometry dialog as presentation says.
    pub(crate) fn refresh_shapes(&mut self, vm: &AppViewModel, document: DocumentId) {
        self.refresh_shape_panel(vm, document);
        self.refresh_geometry_dialog(vm, document);
    }

    fn refresh_shape_panel(&mut self, vm: &AppViewModel, document: DocumentId) {
        let panel = shapes_panel(vm, document, self.locale);
        let sections = [(&panel.shape_title, &panel.shape), (&panel.tool_title, &panel.tool)];
        let rows_needed: usize = sections.iter().map(|(_, rows)| rows.len()).sum();
        let s = &mut self.shapes;
        if rows_needed == 0 || vm.active != Some(document) {
            if s.shown {
                s.shown = false;
                let p = s.panel.clone();
                if let Err(e) = crate::appkit::objc_try("hiding the shape panel", || p.orderOut(None)) {
                    eprintln!("burnburn: {e}");
                }
            }
            return;
        }
        // Lay the visible sections out from the top; hide every unused row.
        let height: f64 = sections
            .iter()
            .filter(|(_, rows)| !rows.is_empty())
            .map(|(_, rows)| TITLE_HEIGHT + rows.len() as f64 * ROW_HEIGHT + 6.0)
            .sum::<f64>()
            + MARGIN;
        let mut y = height - 6.0;
        for (section, (title, lines)) in sections.iter().enumerate() {
            let (title_label, first) = if section == 0 { (&s.shape_title, 0) } else { (&s.tool_title, PARAM_SLOTS) };
            title_label.setHidden(lines.is_empty());
            if lines.is_empty() {
                for r in &s.rows[first..first + PARAM_SLOTS] {
                    r.caption.setHidden(true);
                    r.field.setHidden(true);
                    r.check.setHidden(true);
                }
                continue;
            }
            y -= TITLE_HEIGHT;
            title_label.setFrame(rect(MARGIN, y, PANEL_WIDTH - 2.0 * MARGIN, TITLE_HEIGHT - 4.0));
            set_text(title_label, title);
            for (i, row) in s.rows[first..first + PARAM_SLOTS].iter().enumerate() {
                match lines.get(i) {
                    Some(line) => {
                        y -= ROW_HEIGHT;
                        show_row(row, line, y);
                    }
                    None => {
                        row.caption.setHidden(true);
                        row.field.setHidden(true);
                        row.check.setHidden(true);
                    }
                }
            }
            y -= 6.0;
        }
        let p = s.panel.clone();
        let was_shown = s.shown;
        s.shown = true;
        let window_frame = self.window.frame();
        if let Err(e) = crate::appkit::objc_try("showing the shape panel", || {
            p.setContentSize(NSSize::new(PANEL_WIDTH, height));
            if !was_shown {
                p.setFrameTopLeftPoint(NSPoint::new(
                    window_frame.origin.x + 12.0,
                    window_frame.origin.y + window_frame.size.height - 150.0,
                ));
                p.orderFront(None);
            }
        }) {
            eprintln!("burnburn: {e}");
        }
    }

    fn refresh_geometry_dialog(&mut self, vm: &AppViewModel, document: DocumentId) {
        let s = &mut self.shapes;
        let Some(d) = geometry_dialog_panel(vm, document, self.locale) else {
            if s.dialog_shown {
                s.dialog_shown = false;
                let (window, sheet) = (self.window.clone(), s.sheet.clone());
                if let Err(e) = crate::appkit::objc_try("ending the geometry dialog", || window.endSheet(&sheet)) {
                    eprintln!("burnburn: {e}");
                }
            }
            return;
        };
        set_text(&s.dialog_error, &d.error);
        if s.dialog_shown {
            return;
        }
        // Filled once per opening, so a refused OK keeps what the user typed.
        set_text(&s.dialog_title, &d.title);
        set_text(&s.dialog_label, &d.label);
        s.dialog_field.setStringValue(&NSString::from_str(&d.text));
        s.dialog_field.setAccessibilityLabel(Some(&NSString::from_str(&d.label)));
        s.sheet.setTitle(&NSString::from_str(&d.title));
        s.dialog_shown = true;
        let (window, sheet) = (self.window.clone(), s.sheet.clone());
        if let Err(e) =
            crate::appkit::objc_try("showing the geometry dialog", || window.beginSheet_completionHandler(&sheet, None))
        {
            eprintln!("burnburn: {e}");
            s.dialog_shown = false;
        }
    }
}
