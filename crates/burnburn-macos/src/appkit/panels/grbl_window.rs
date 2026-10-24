//! The GRBL laser window (G5-grbl-ui): one floating utility panel for the whole app with the
//! controller status, the console (transcript, input with history on the arrow keys), the macro
//! list with its editor, jog/home/unlock/hold/resume/reset and the job start that stays off.
//!
//! The window only copies the headless model (`crate::panels::grbl_window_panel`) into widgets and
//! turns user actions into `UiIntent`s via the headless builders. Nothing here opens a serial
//! device: the host app hands in a port (`HostApp::attach_grbl_port`); until then every control
//! says "geen seriële poort verbonden".

use std::cell::RefCell;
use std::rc::Weak;

use burnburn_presentation::{GrblControlKind, StepDirection};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAccessibility, NSBackingStoreType, NSButton, NSColor, NSControl, NSControlTextEditingDelegate, NSFont, NSPanel,
    NSScrollView, NSTableColumn, NSTableView, NSTableViewDataSource, NSTableViewDelegate, NSTextField,
    NSTextFieldDelegate, NSTextView, NSView, NSWindowStyleMask,
};
use objc2_foundation::{NSInteger, NSNotification, NSString};

use crate::appkit::app::HostApp;
use crate::appkit::classes::ffi_guard;
use crate::appkit::panels::widgets::{
    field, label, push_button, rect, selected_rows, set_button_title, set_error_text, set_text,
};
use crate::l10n::Locale;
use crate::panels::{GrblNumber, GrblWindowPanel};

pub(crate) const WIDTH: f64 = 560.0;
pub(crate) const HEIGHT: f64 = 700.0;
const MARGIN: f64 = 12.0;

/// The buttons in the order of the view model's controls (their tag is the index).
const KINDS: [GrblControlKind; 11] = [
    GrblControlKind::Jog(StepDirection::Left),
    GrblControlKind::Jog(StepDirection::Right),
    GrblControlKind::Jog(StepDirection::Up),
    GrblControlKind::Jog(StepDirection::Down),
    GrblControlKind::JogCancel,
    GrblControlKind::Home,
    GrblControlKind::Unlock,
    GrblControlKind::Hold,
    GrblControlKind::Resume,
    GrblControlKind::Reset,
    GrblControlKind::StartJob,
];

const FIELD_STEP: isize = 1;
const FIELD_FEED: isize = 2;

pub(crate) struct TargetIvars {
    app: Weak<HostApp>,
    input: Retained<NSTextField>,
    macro_name: Retained<NSTextField>,
    macro_program: Retained<NSTextView>,
    table: Retained<NSTableView>,
    /// The macro names the table shows.
    names: RefCell<Vec<String>>,
    /// The programs of the macros, same order.
    programs: RefCell<Vec<String>>,
}

define_class!(
    #[unsafe(super(objc2::runtime::NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnGrblTarget"]
    #[ivars = TargetIvars]
    pub(crate) struct GrblTarget;

    impl GrblTarget {
        /// Return in the input field, or the Verstuur button.
        #[unsafe(method(grblSend:))]
        fn grbl_send(&self, _sender: Option<&AnyObject>) {
            ffi_guard("grblSend:", (), || {
                let text = self.ivars().input.stringValue().to_string();
                if let Some(app) = self.ivars().app.upgrade() {
                    app.grbl_send_typed(&text);
                }
            });
        }

        /// A control button; its tag is its index in `KINDS`.
        #[unsafe(method(grblButton:))]
        fn grbl_button(&self, sender: Option<&AnyObject>) {
            ffi_guard("grblButton:", (), || {
                let Some(button) = sender.and_then(|s| s.downcast_ref::<NSButton>()) else { return };
                let Some(kind) = usize::try_from(button.tag()).ok().and_then(|i| KINDS.get(i)).copied() else { return };
                if let Some(app) = self.ivars().app.upgrade() {
                    app.grbl_control_pressed(kind);
                }
            });
        }

        /// Step size and feed commit (Return, Tab): the text goes to presentation as typed.
        #[unsafe(method(grblNumber:))]
        fn grbl_number(&self, sender: Option<&AnyObject>) {
            ffi_guard("grblNumber:", (), || {
                let Some(field) = sender.and_then(|s| s.downcast_ref::<NSTextField>()) else { return };
                let which = match field.tag() {
                    FIELD_STEP => GrblNumber::Step,
                    FIELD_FEED => GrblNumber::Feed,
                    _ => return,
                };
                let text = field.stringValue().to_string();
                if let Some(app) = self.ivars().app.upgrade() {
                    app.grbl_number_committed(which, &text);
                }
            });
        }

        /// Voer uit (0), Bewaar (1), Verwijder (2) of the macro list.
        #[unsafe(method(grblMacro:))]
        fn grbl_macro(&self, sender: Option<&AnyObject>) {
            ffi_guard("grblMacro:", (), || {
                let Some(button) = sender.and_then(|s| s.downcast_ref::<NSButton>()) else { return };
                let Some(app) = self.ivars().app.upgrade() else { return };
                let row = selected_rows(&self.ivars().table).first().copied();
                match button.tag() {
                    0 => app.grbl_macro_run(row),
                    1 => {
                        let name = self.ivars().macro_name.stringValue().to_string();
                        let program = self.ivars().macro_program.string().to_string();
                        app.grbl_macro_save(&name, &program);
                    }
                    2 => app.grbl_macro_remove(row),
                    _ => {}
                }
            });
        }
    }

    unsafe impl NSObjectProtocol for GrblTarget {}

    unsafe impl NSControlTextEditingDelegate for GrblTarget {
        /// Up and down in the console input walk the history; the other keys stay AppKit's.
        #[unsafe(method(control:textView:doCommandBySelector:))]
        fn control_do_command(&self, _control: &NSControl, _text_view: &NSTextView, command: Sel) -> bool {
            ffi_guard("grbl control:textView:doCommandBySelector:", false, || {
                let name = command.name().to_string_lossy().into_owned();
                self.ivars().app.upgrade().is_some_and(|app| app.grbl_input_command(&name))
            })
        }
    }

    unsafe impl NSTextFieldDelegate for GrblTarget {}

    unsafe impl NSTableViewDataSource for GrblTarget {
        #[unsafe(method(numberOfRowsInTableView:))]
        fn number_of_rows(&self, _table_view: &NSTableView) -> NSInteger {
            ffi_guard("grbl numberOfRowsInTableView:", 0, || {
                self.ivars().names.try_borrow().map_or(0, |n| n.len() as NSInteger)
            })
        }

        #[unsafe(method_id(tableView:objectValueForTableColumn:row:))]
        fn object_value(
            &self,
            _table_view: &NSTableView,
            _table_column: Option<&NSTableColumn>,
            row: NSInteger,
        ) -> Option<Retained<AnyObject>> {
            ffi_guard("grbl tableView:objectValueForTableColumn:row:", None, || {
                let row = usize::try_from(row).ok()?;
                let names = self.ivars().names.try_borrow().ok()?;
                Some(Retained::into_super(Retained::into_super(NSString::from_str(names.get(row)?))))
            })
        }
    }

    unsafe impl NSTableViewDelegate for GrblTarget {
        /// Choosing a macro shows its name and lines in the editor.
        #[unsafe(method(tableViewSelectionDidChange:))]
        fn selection_changed(&self, _notification: &NSNotification) {
            ffi_guard("grbl tableViewSelectionDidChange:", (), || {
                let Some(row) = selected_rows(&self.ivars().table).first().copied() else { return };
                let name = self.ivars().names.try_borrow().ok().and_then(|n| n.get(row).cloned());
                let program = self.ivars().programs.try_borrow().ok().and_then(|p| p.get(row).cloned());
                if let (Some(name), Some(program)) = (name, program) {
                    set_text(&self.ivars().macro_name, &name);
                    self.ivars().macro_program.setString(&NSString::from_str(&program));
                }
                // The Voer uit and Verwijder buttons follow the selected row.
                if let Some(app) = self.ivars().app.upgrade() {
                    app.refresh_grbl_window();
                }
            });
        }
    }
);

impl GrblTarget {
    fn new(mtm: MainThreadMarker, ivars: TargetIvars) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ivars);
        // SAFETY: NSObject's designated initialiser on a freshly allocated instance with ivars set.
        unsafe { msg_send![super(this), init] }
    }

    /// The macros the table shows; true when they changed (the table must reload).
    fn set_macros(&self, macros: &[(String, String)]) -> bool {
        let names: Vec<String> = macros.iter().map(|m| m.0.clone()).collect();
        let programs: Vec<String> = macros.iter().map(|m| m.1.clone()).collect();
        let (Ok(mut n), Ok(mut p)) = (self.ivars().names.try_borrow_mut(), self.ivars().programs.try_borrow_mut())
        else {
            return false;
        };
        let changed = *n != names || *p != programs;
        *n = names;
        *p = programs;
        changed
    }
}

/// The widgets of the window.
pub(crate) struct GrblWindow {
    pub(crate) panel: Retained<NSPanel>,
    target: Retained<GrblTarget>,
    status: Retained<NSTextField>,
    refusal: Retained<NSTextField>,
    notes: Retained<NSTextField>,
    transcript: Retained<NSTextView>,
    input: Retained<NSTextField>,
    send: Retained<NSButton>,
    buttons: Vec<(GrblControlKind, Retained<NSButton>)>,
    step: Retained<NSTextField>,
    feed: Retained<NSTextField>,
    table: Retained<NSTableView>,
    macro_name: Retained<NSTextField>,
    macro_program: Retained<NSTextView>,
    macro_buttons: [Retained<NSButton>; 3],
    /// What the input field last got from the model (a refusal must not wipe typed text).
    shown_input: RefCell<String>,
    /// The user asked for the window (menu); it is on screen while this is true.
    pub(crate) shown: bool,
}

/// Borrowed native parts, for tests.
pub struct GrblParts<'a> {
    pub panel: &'a NSPanel,
    pub status: &'a NSTextField,
    pub refusal: &'a NSTextField,
    pub notes: &'a NSTextField,
    pub transcript: &'a NSTextView,
    pub input: &'a NSTextField,
    pub send: &'a NSButton,
    pub buttons: &'a [(GrblControlKind, Retained<NSButton>)],
    pub step: &'a NSTextField,
    pub feed: &'a NSTextField,
    pub macro_table: &'a NSTableView,
    pub macro_name: &'a NSTextField,
    pub macro_program: &'a NSTextView,
    /// Voer uit, Bewaar, Verwijder.
    pub macro_buttons: &'a [Retained<NSButton>; 3],
}

#[allow(unsafe_code)]
fn wire(control: &NSControl, target: &AnyObject, action: objc2::runtime::Sel, tag: isize) {
    control.setTag(tag);
    // SAFETY: `action` is a `GrblTarget` method taking the sender (`v@:@`); the target is kept by
    // the `GrblWindow`, which outlives every control (the panel is never released while it lives).
    unsafe { control.setTarget(Some(target)) };
    // SAFETY: as above.
    unsafe { control.setAction(Some(action)) };
}

fn text_view(
    mtm: MainThreadMarker,
    frame: objc2_foundation::NSRect,
    editable: bool,
) -> (Retained<NSScrollView>, Retained<NSTextView>) {
    let scroll = NSTextView::scrollableTextView(mtm);
    scroll.setFrame(frame);
    scroll.setHasVerticalScroller(true);
    let view = scroll.documentView().and_then(|v| v.downcast::<NSTextView>().ok()).unwrap_or_else(|| {
        NSTextView::initWithFrame(NSTextView::alloc(mtm), rect(0.0, 0.0, frame.size.width, frame.size.height))
    });
    view.setEditable(editable);
    view.setSelectable(true);
    view.setFont(Some(&NSFont::userFixedPitchFontOfSize(11.0).unwrap_or_else(|| NSFont::systemFontOfSize(11.0))));
    (scroll, view)
}

pub(crate) fn grbl_window(mtm: MainThreadMarker, app: Weak<HostApp>, locale: Locale) -> GrblWindow {
    let nl = locale == Locale::Nl;
    let t = |nl_text: &'static str, en_text: &'static str| if nl { nl_text } else { en_text };
    let style = NSWindowStyleMask::Titled | NSWindowStyleMask::UtilityWindow | NSWindowStyleMask::Closable;
    // SAFETY: NSPanel's designated initialiser on a freshly allocated instance.
    let panel: Retained<NSPanel> = unsafe {
        msg_send![
            NSPanel::alloc(mtm),
            initWithContentRect: rect(0.0, 0.0, WIDTH, HEIGHT),
            styleMask: style,
            backing: NSBackingStoreType::Buffered,
            defer: false
        ]
    };
    // SAFETY: ownership stays with this Retained.
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setFloatingPanel(true);
    panel.setHidesOnDeactivate(true);
    panel.setTitle(&NSString::from_str(t("GRBL-laservenster", "GRBL Laser Window")));
    let content = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, WIDTH, HEIGHT));
    let inner = WIDTH - 2.0 * MARGIN;
    let y = std::cell::Cell::new(HEIGHT);
    let row = |height: f64, gap: f64| {
        y.set(y.get() - height);
        let at = y.get();
        y.set(at - gap);
        at
    };
    let heading = |text: &str, at: f64| {
        let l = label(mtm, text, rect(MARGIN, at, inner, 18.0));
        l.setFont(Some(&NSFont::boldSystemFontOfSize(12.0)));
        content.addSubview(&l);
    };

    // Status, refusal and the standing note.
    let at = row(20.0, 2.0);
    let status = label(mtm, "", rect(MARGIN, at, inner, 20.0));
    status.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
    content.addSubview(&status);
    let at = row(34.0, 2.0);
    let refusal = label(mtm, "", rect(MARGIN, at, inner, 34.0));
    refusal.setTextColor(Some(&NSColor::systemRedColor()));
    refusal.setLineBreakMode(objc2_app_kit::NSLineBreakMode::ByWordWrapping);
    content.addSubview(&refusal);
    let at = row(30.0, 6.0);
    let notes = label(mtm, "", rect(MARGIN, at, inner, 30.0));
    notes.setFont(Some(&NSFont::systemFontOfSize(10.0)));
    notes.setLineBreakMode(objc2_app_kit::NSLineBreakMode::ByWordWrapping);
    content.addSubview(&notes);

    // Console.
    let at = row(18.0, 2.0);
    heading(t("Console", "Console"), at);
    let at = row(190.0, 6.0);
    let (scroll, transcript) = text_view(mtm, rect(MARGIN, at, inner, 190.0), false);
    transcript.setAccessibilityLabel(Some(&NSString::from_str(t("Console, transcript", "Console transcript"))));
    content.addSubview(&scroll);
    let at = row(26.0, 8.0);
    let input = field(mtm, rect(MARGIN, at + 1.0, inner - 96.0, 24.0));
    input.setPlaceholderString(Some(&NSString::from_str(t("G-code of $-commando", "G-code or $ command"))));
    input.setAccessibilityLabel(Some(&NSString::from_str(t("Consoleregel", "Console line"))));
    if let Some(cell) = input.cell() {
        // Return sends; leaving the field does not.
        cell.setSendsActionOnEndEditing(false);
    }
    content.addSubview(&input);
    let send = push_button(mtm, t("Verstuur", "Send"));
    send.setFrame(rect(WIDTH - MARGIN - 88.0, at, 88.0, 28.0));
    content.addSubview(&send);

    // Jog.
    let at = row(18.0, 2.0);
    heading(t("Beweging", "Move"), at);
    let at = row(28.0, 6.0);
    let mut buttons: Vec<(GrblControlKind, Retained<NSButton>)> = Vec::new();
    let quarter = (inner - 4.0 * 6.0) / 5.0;
    for (i, kind) in KINDS.iter().take(5).enumerate() {
        let b = push_button(mtm, "");
        b.setFrame(rect(MARGIN + i as f64 * (quarter + 6.0), at, quarter, 28.0));
        content.addSubview(&b);
        buttons.push((*kind, b));
    }
    let at = row(28.0, 6.0);
    heading_inline(mtm, &content, t("Stap (mm)", "Step (mm)"), rect(MARGIN, at + 4.0, 80.0, 18.0));
    let step = field(mtm, rect(MARGIN + 84.0, at + 2.0, 70.0, 24.0));
    step.setAccessibilityLabel(Some(&NSString::from_str(t("Jogstap in millimeter", "Jog step in millimetres"))));
    content.addSubview(&step);
    heading_inline(mtm, &content, t("Snelheid (mm/min)", "Feed (mm/min)"), rect(MARGIN + 170.0, at + 4.0, 130.0, 18.0));
    let feed = field(mtm, rect(MARGIN + 304.0, at + 2.0, 80.0, 24.0));
    feed.setAccessibilityLabel(Some(&NSString::from_str(t(
        "Jogsnelheid in mm per minuut",
        "Jog feed in mm per minute",
    ))));
    content.addSubview(&feed);

    // Home, unlock, hold, resume, reset, start.
    let at = row(28.0, 4.0);
    let third = (inner - 2.0 * 6.0) / 3.0;
    for (i, kind) in KINDS.iter().skip(5).take(3).enumerate() {
        let b = push_button(mtm, "");
        b.setFrame(rect(MARGIN + i as f64 * (third + 6.0), at, third, 28.0));
        content.addSubview(&b);
        buttons.push((*kind, b));
    }
    let at = row(28.0, 8.0);
    for (i, kind) in KINDS.iter().skip(8).enumerate() {
        let b = push_button(mtm, "");
        b.setFrame(rect(MARGIN + i as f64 * (third + 6.0), at, third, 28.0));
        content.addSubview(&b);
        buttons.push((*kind, b));
    }

    // Macros.
    let at = row(18.0, 2.0);
    heading(t("Macro’s", "Macros"), at);
    let list_height = 130.0;
    let top = row(list_height, 6.0);
    let list_width = 180.0;
    let table = NSTableView::initWithFrame(NSTableView::alloc(mtm), rect(0.0, 0.0, list_width, list_height));
    let column = NSTableColumn::initWithIdentifier(NSTableColumn::alloc(mtm), &NSString::from_str("name"));
    column.setWidth(list_width - 20.0);
    table.addTableColumn(&column);
    column.setTitle(&NSString::from_str(t("Macro’s", "Macros")));
    table.setAllowsMultipleSelection(false);
    table.setAllowsEmptySelection(true);
    table.setAccessibilityLabel(Some(&NSString::from_str(t("Macrolijst", "Macro list"))));
    let table_scroll =
        NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(MARGIN, top, list_width, list_height));
    table_scroll.setHasVerticalScroller(true);
    table_scroll.setDocumentView(Some(&table));
    content.addSubview(&table_scroll);
    let right = MARGIN + list_width + 10.0;
    let right_width = WIDTH - MARGIN - right;
    let macro_name = field(mtm, rect(right, top + list_height - 24.0, right_width, 24.0));
    macro_name.setPlaceholderString(Some(&NSString::from_str(t("Naam van de macro", "Macro name"))));
    macro_name.setAccessibilityLabel(Some(&NSString::from_str(t("Macronaam", "Macro name"))));
    content.addSubview(&macro_name);
    let (program_scroll, macro_program) =
        text_view(mtm, rect(right, top + 32.0, right_width, list_height - 62.0), true);
    macro_program.setAccessibilityLabel(Some(&NSString::from_str(t("Macroregels", "Macro lines"))));
    content.addSubview(&program_scroll);
    let third_right = (right_width - 2.0 * 6.0) / 3.0;
    let mut macro_buttons = Vec::new();
    for (i, text) in [t("Voer uit", "Run"), t("Bewaar", "Save"), t("Verwijder", "Remove")].into_iter().enumerate() {
        let b = push_button(mtm, text);
        b.setFrame(rect(right + i as f64 * (third_right + 6.0), top, third_right, 28.0));
        content.addSubview(&b);
        macro_buttons.push(b);
    }
    let macro_buttons: [Retained<NSButton>; 3] = match macro_buttons.try_into() {
        Ok(b) => b,
        Err(_) => unreachable_buttons(mtm),
    };

    let target = GrblTarget::new(
        mtm,
        TargetIvars {
            app,
            input: input.clone(),
            macro_name: macro_name.clone(),
            macro_program: macro_program.clone(),
            table: table.clone(),
            names: RefCell::new(Vec::new()),
            programs: RefCell::new(Vec::new()),
        },
    );
    let target_object: &AnyObject = target.as_ref();
    wire(&send, target_object, sel!(grblSend:), 0);
    wire(&input, target_object, sel!(grblSend:), 0);
    for (i, (_, b)) in buttons.iter().enumerate() {
        wire(b, target_object, sel!(grblButton:), i as isize);
    }
    wire(&step, target_object, sel!(grblNumber:), FIELD_STEP);
    wire(&feed, target_object, sel!(grblNumber:), FIELD_FEED);
    for (i, b) in macro_buttons.iter().enumerate() {
        wire(b, target_object, sel!(grblMacro:), i as isize);
    }
    // SAFETY: the delegate and data source are unretained references; the `GrblWindow` keeps the
    // target alive and clears them in Drop.
    unsafe { input.setDelegate(Some(objc2::runtime::ProtocolObject::from_ref(&*target))) };
    // SAFETY: as above.
    unsafe { table.setDataSource(Some(objc2::runtime::ProtocolObject::from_ref(&*target))) };
    // SAFETY: as above.
    unsafe { table.setDelegate(Some(objc2::runtime::ProtocolObject::from_ref(&*target))) };
    panel.setContentView(Some(&content));
    GrblWindow {
        panel,
        target,
        status,
        refusal,
        notes,
        transcript,
        input,
        send,
        buttons,
        step,
        feed,
        table,
        macro_name,
        macro_program,
        macro_buttons,
        shown_input: RefCell::new(String::new()),
        shown: false,
    }
}

fn heading_inline(mtm: MainThreadMarker, content: &NSView, text: &str, frame: objc2_foundation::NSRect) {
    let l = label(mtm, text, frame);
    l.setFont(Some(&NSFont::systemFontOfSize(11.0)));
    content.addSubview(&l);
}

/// Three buttons were pushed above; a different count cannot happen. Kept total instead of unwrap.
fn unreachable_buttons(mtm: MainThreadMarker) -> [Retained<NSButton>; 3] {
    [push_button(mtm, ""), push_button(mtm, ""), push_button(mtm, "")]
}

impl GrblWindow {
    pub(crate) fn parts(&self) -> GrblParts<'_> {
        GrblParts {
            panel: &self.panel,
            status: &self.status,
            refusal: &self.refusal,
            notes: &self.notes,
            transcript: &self.transcript,
            input: &self.input,
            send: &self.send,
            buttons: &self.buttons,
            step: &self.step,
            feed: &self.feed,
            macro_table: &self.table,
            macro_name: &self.macro_name,
            macro_program: &self.macro_program,
            macro_buttons: &self.macro_buttons,
        }
    }

    /// Copies the model into the widgets.
    pub(crate) fn refresh(&self, model: &GrblWindowPanel) {
        set_text(&self.status, &model.status);
        set_error_text(&self.refusal, &model.refusal);
        set_text(&self.notes, &model.notes);
        if self.transcript.string().to_string() != model.transcript {
            self.transcript.setString(&NSString::from_str(&model.transcript));
            let end = model.transcript.encode_utf16().count();
            self.transcript.scrollRangeToVisible(objc2_foundation::NSRange::new(end, 0));
        }
        // The input follows the model only when the model changed it (history recall, a sent line).
        if let Ok(mut shown) = self.shown_input.try_borrow_mut()
            && *shown != model.input
        {
            *shown = model.input.clone();
            set_text(&self.input, &model.input);
        }
        self.input.setEnabled(model.console_enabled);
        self.send.setEnabled(model.console_enabled);
        let tip = (!model.console_tooltip.is_empty()).then(|| NSString::from_str(&model.console_tooltip));
        self.input.setToolTip(tip.as_deref());
        self.send.setToolTip(tip.as_deref());
        for (kind, b) in &self.buttons {
            if let Some(m) = model.buttons.iter().find(|m| m.kind == *kind) {
                set_button_title(b, &m.title);
                b.setEnabled(m.enabled);
                let tip = (!m.tooltip.is_empty()).then(|| NSString::from_str(&m.tooltip));
                b.setToolTip(tip.as_deref());
            }
        }
        for (name, field) in [(GrblNumber::Step, &self.step), (GrblNumber::Feed, &self.feed)] {
            let text = match name {
                GrblNumber::Step => &model.jog_step,
                GrblNumber::Feed => &model.jog_feed,
            };
            // A field being edited keeps what the user types.
            let editing = field.currentEditor().is_some();
            if !editing {
                set_text(field, text);
            }
        }
        let rows: Vec<(String, String)> = model.macros.iter().map(|m| (m.name.clone(), m.program.clone())).collect();
        if self.target.set_macros(&rows) {
            self.table.reloadData();
        }
        let selected = selected_rows(&self.table).first().and_then(|r| model.macros.get(*r));
        let run_enabled = selected.is_some_and(|m| m.enabled);
        self.macro_buttons[0].setEnabled(run_enabled);
        self.macro_buttons[1].setEnabled(true);
        self.macro_buttons[2].setEnabled(selected.is_some());
        let run_tip = (!run_enabled && selected.is_some() && !model.console_tooltip.is_empty())
            .then(|| NSString::from_str(&model.console_tooltip));
        self.macro_buttons[0].setToolTip(run_tip.as_deref());
    }
}

impl Drop for GrblWindow {
    fn drop(&mut self) {
        // The delegate and data source are unretained references to `target`: clear them first.
        // SAFETY: setting no delegate / data source is always valid.
        unsafe { self.input.setDelegate(None) };
        // SAFETY: as above.
        unsafe { self.table.setDataSource(None) };
        // SAFETY: as above.
        unsafe { self.table.setDelegate(None) };
    }
}

// ---------------------------------------------------------------------------------------------
// The GRBL refusals at the plan: a pull-down under the findings line; choosing a row selects its
// object (the click-through of a preflight finding).
// ---------------------------------------------------------------------------------------------

pub(crate) struct RefusalIvars {
    app: Weak<HostApp>,
    document: burnburn_types::DocumentId,
}

define_class!(
    #[unsafe(super(objc2::runtime::NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnGrblRefusalTarget"]
    #[ivars = RefusalIvars]
    pub(crate) struct RefusalTarget;

    impl RefusalTarget {
        /// A row of the pull-down; item 0 is its title, row `n` is item `n + 1`.
        #[unsafe(method(grblRefusal:))]
        fn grbl_refusal(&self, sender: Option<&AnyObject>) {
            ffi_guard("grblRefusal:", (), || {
                let Some(popup) = sender.and_then(|s| s.downcast_ref::<objc2_app_kit::NSPopUpButton>()) else {
                    return;
                };
                let Some(row) = usize::try_from(popup.indexOfSelectedItem()).ok().and_then(|i| i.checked_sub(1))
                else {
                    return;
                };
                if let Some(app) = self.ivars().app.upgrade() {
                    app.grbl_refusal_chosen(self.ivars().document, row);
                }
            });
        }
    }

    unsafe impl NSObjectProtocol for RefusalTarget {}
);

/// The pull-down and its target (the window keeps both alive).
pub(crate) fn refusal_popup(
    mtm: MainThreadMarker,
    app: Weak<HostApp>,
    document: burnburn_types::DocumentId,
    frame: objc2_foundation::NSRect,
) -> (Retained<objc2_app_kit::NSPopUpButton>, Retained<RefusalTarget>) {
    let this = RefusalTarget::alloc(mtm).set_ivars(RefusalIvars { app, document });
    // SAFETY: NSObject's designated initialiser on a freshly allocated instance with ivars set.
    let target: Retained<RefusalTarget> = unsafe { msg_send![super(this), init] };
    let popup =
        objc2_app_kit::NSPopUpButton::initWithFrame_pullsDown(objc2_app_kit::NSPopUpButton::alloc(mtm), frame, true);
    popup.setAutoresizingMask(crate::appkit::panels::widgets::TOP_PINNED);
    popup.setControlSize(objc2_app_kit::NSControlSize::Small);
    popup.setFont(Some(&NSFont::systemFontOfSize(10.0)));
    popup.setHidden(true);
    let target_object: &AnyObject = target.as_ref();
    wire(&popup, target_object, sel!(grblRefusal:), 0);
    (popup, target)
}

/// Copies the GRBL refusals into the pull-down: hidden when the plan has none.
pub(crate) fn refresh_refusals(popup: &objc2_app_kit::NSPopUpButton, panel: &crate::panels::PlanPanel, locale: Locale) {
    let titles: Vec<String> = panel.grbl.iter().map(|r| r.text.clone()).collect();
    popup.setHidden(titles.is_empty());
    let current: Vec<String> = popup.itemTitles().iter().skip(1).map(|t| t.to_string()).collect();
    if current == titles {
        return;
    }
    popup.removeAllItems();
    let heading = match locale {
        Locale::Nl => format!("GRBL-weigeringen ({})", titles.len()),
        Locale::En => format!("GRBL refusals ({})", titles.len()),
    };
    popup.addItemWithTitle(&NSString::from_str(&heading));
    for t in &titles {
        popup.addItemWithTitle(&NSString::from_str(t));
    }
}
