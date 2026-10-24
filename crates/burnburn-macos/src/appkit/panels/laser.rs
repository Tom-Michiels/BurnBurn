//! The laser window (W2-L1-laser, LFE): a floating utility panel per document window with the
//! position, jog (hold), step, numeric move, position laser, home/origins, pause/resume/stop, start
//! from and the 3×3 job origin, cut selected, frame, save RD, the files on the controller and the
//! diagnostics export. Since LFE it is a form: `crate::panels::laser_form` describes it (sections
//! with a heading, the unit as a suffix, the reason a section is greyed said once, the jog cross and
//! the job origin as pads) and the form builder lays it out; this file wires the controls.
//!
//! The panel only copies the headless model (`crate::panels::laser_panel`) into the form and turns
//! user actions into `UiIntent`s via the headless builders. Jog is press-and-hold: a jog button (and
//! the arrow keys while the panel has the keyboard) press on key-down and release on key-up, when
//! the panel loses the keyboard, hides, or closes. Key repeat is not a new press.
//! Nothing here talks to a machine; it all goes through the presentation layer.

#![allow(unsafe_code)] // defining Objective-C subclasses and sending to `super` has no safe API (SAFETY below)

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use burnburn_host_models::form::{Control, FormId, FormSpec, PadGlyph};
use burnburn_presentation::{LaserSetup, LaserViewModel};
use burnburn_types::DocumentId;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAutoresizingMaskOptions, NSBackingStoreType, NSButton, NSCellImagePosition, NSControl, NSControlStateValueOn,
    NSEvent, NSEventType, NSPanel, NSPopUpButton, NSScrollView, NSText, NSTextField, NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSSize, NSString};

use crate::appkit::app::HostApp;
use crate::appkit::classes::ffi_guard;
use crate::appkit::panels::form::{FormView, Wiring, build_form};
use crate::appkit::panels::pad::{HoldHandler, fire_hold, set_hold_handler};
use crate::appkit::panels::widgets::rect;
use crate::appkit::symbols::{self, Symbol};
use crate::appkit::window::DocWindow;
use crate::l10n::Locale;
use crate::panels::{
    LaserCheck, LaserFormAction, LaserText, anchor_for_tag, file_intent, jog_key_for_key_code, jog_press_intent,
    jog_release_intent, laser_form, laser_form_action, laser_id, laser_panel, laser_text, laser_window_height,
    laser_window_top_left, move_intent, save_rd_intent, setup_intent, simple_intent, start_from_for_segment,
};

pub(crate) const WIDTH: f64 = 600.0;
/// How much of the content the panel shows before the screen is known (a laptop screen is about
/// 800 pt high); the real bound is [`laser_window_height`] on the screen of the document window.
const FIRST_VISIBLE_HEIGHT: f64 = 700.0;

// ---------------------------------------------------------------------------------------------
// The panel: arrow keys jog while the panel has the keyboard and no text is being edited.
// ---------------------------------------------------------------------------------------------

pub(crate) struct PanelIvars {
    app: Weak<HostApp>,
    document: DocumentId,
}

define_class!(
    #[unsafe(super(NSPanel))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnLaserPanel"]
    #[ivars = PanelIvars]
    pub(crate) struct LaserPanelWindow;

    impl LaserPanelWindow {
        /// Arrow keys become jog keys (down: press, up: release), but never while a field is being
        /// edited, with a modifier, or as key repeat. Everything else goes to AppKit.
        #[unsafe(method(sendEvent:))]
        fn send_event(&self, event: &NSEvent) {
            ffi_guard("laser panel sendEvent:", (), || {
                if !self.handle_jog_key(event) {
                    // SAFETY: NSWindow's own implementation with the same event, initialised instance.
                    let () = unsafe { msg_send![super(self), sendEvent: event] };
                }
            });
        }

        /// The panel lost the keyboard: a held key must not keep the head moving.
        #[unsafe(method(resignKeyWindow))]
        fn resign_key_window(&self) {
            ffi_guard("laser panel resignKeyWindow", (), || {
                if let Some(app) = self.ivars().app.upgrade() {
                    app.dispatch(jog_release_intent());
                }
                // SAFETY: NSWindow's own implementation, no arguments, initialised instance.
                let () = unsafe { msg_send![super(self), resignKeyWindow] };
            });
        }
    }

    unsafe impl NSObjectProtocol for LaserPanelWindow {}
);

impl LaserPanelWindow {
    fn new(mtm: MainThreadMarker, app: Weak<HostApp>, document: DocumentId) -> Retained<Self> {
        let style = NSWindowStyleMask::Titled
            | NSWindowStyleMask::UtilityWindow
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Resizable;
        let this = Self::alloc(mtm).set_ivars(PanelIvars { app, document });
        // SAFETY: NSPanel's designated initialiser on a freshly allocated instance with ivars set.
        let panel: Retained<Self> = unsafe {
            msg_send![
                super(this),
                initWithContentRect: rect(0.0, 0.0, WIDTH, FIRST_VISIBLE_HEIGHT),
                styleMask: style,
                backing: NSBackingStoreType::Buffered,
                defer: false
            ]
        };
        panel
    }

    /// True when the event was a jog key and has been handled here.
    fn handle_jog_key(&self, event: &NSEvent) -> bool {
        let kind = event.r#type();
        if kind != NSEventType::KeyDown && kind != NSEventType::KeyUp {
            return false;
        }
        let Some(key) = jog_key_for_key_code(event.keyCode()) else { return false };
        // A field being edited owns the arrow keys (cursor movement).
        let editing = self.firstResponder().is_some_and(|r| {
            let object: &AnyObject = &r;
            object.downcast_ref::<NSText>().is_some()
        });
        let plain = event.modifierFlags().0
            & (crate::adapter::flags::COMMAND
                | crate::adapter::flags::CONTROL
                | crate::adapter::flags::OPTION
                | crate::adapter::flags::SHIFT)
            == 0;
        if editing || !plain {
            return false;
        }
        let Some(app) = self.ivars().app.upgrade() else { return true };
        if kind == NSEventType::KeyUp {
            app.dispatch(jog_release_intent());
        } else if !event.isARepeat() {
            let document = self.ivars().document;
            let intent = app.laser_view_for(document).and_then(|vm| jog_press_intent(document, &vm, key));
            if let Some(intent) = intent {
                app.dispatch(intent);
            }
        }
        true
    }
}

// ---------------------------------------------------------------------------------------------
// Target of every control of the form: the tag is the form id.
// ---------------------------------------------------------------------------------------------

/// The fields of the current form that an action reads (the form is built again when its shape
/// changes, and these follow).
#[derive(Default)]
struct Handles {
    move_x: Option<Retained<NSTextField>>,
    move_y: Option<Retained<NSTextField>>,
    rd_name: Option<Retained<NSTextField>>,
    file_choice: Option<Retained<NSPopUpButton>>,
}

pub(crate) struct TargetIvars {
    app: Weak<HostApp>,
    document: DocumentId,
    handles: RefCell<Handles>,
    locale: Locale,
}

define_class!(
    #[unsafe(super(objc2::runtime::NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnLaserTarget"]
    #[ivars = TargetIvars]
    pub(crate) struct LaserTarget;

    impl LaserTarget {
        /// Any control of the laser form; its tag is its form id (a pad or a row of buttons: the id
        /// of the row plus the place of the button).
        #[unsafe(method(laserForm:))]
        fn laser_form_used(&self, sender: Option<&AnyObject>) {
            ffi_guard("laserForm:", (), || {
                let Some(control) = sender.and_then(|s| s.downcast_ref::<NSControl>()) else { return };
                let Ok(id) = u32::try_from(control.tag()) else { return };
                if let Some(action) = laser_form_action(FormId(id)) {
                    self.run(action, control);
                }
            });
        }
    }

    unsafe impl NSObjectProtocol for LaserTarget {}
);

impl LaserTarget {
    fn app(&self) -> Option<Rc<HostApp>> {
        self.ivars().app.upgrade()
    }

    fn view(&self) -> Option<LaserViewModel> {
        self.app()?.laser_view_for(self.ivars().document)
    }

    fn with_view(&self, body: impl FnOnce(&HostApp, &LaserViewModel) -> Option<burnburn_presentation::AppUpdate>) {
        if let (Some(app), Some(vm)) = (self.app(), self.view()) {
            let _ = body(&app, &vm);
        }
    }

    fn setup(&self, setup: LaserSetup) {
        if let Some(app) = self.app() {
            app.dispatch(setup_intent(setup));
        }
    }

    fn text_of(field: &Option<Retained<NSTextField>>) -> String {
        field.as_ref().map(|f| f.stringValue().to_string()).unwrap_or_default()
    }

    /// The row of the file chosen in the list of controller files.
    fn chosen_file(&self) -> Option<usize> {
        let handles = self.ivars().handles.try_borrow().ok()?;
        usize::try_from(handles.file_choice.as_ref()?.indexOfSelectedItem()).ok()
    }

    fn run(&self, action: LaserFormAction, sender: &NSControl) {
        let document = self.ivars().document;
        match action {
            // A button with a fixed command; its tag in `simple_controls()` is in the action.
            LaserFormAction::Simple(tag) => {
                self.with_view(|app, vm| simple_intent(document, vm, tag).map(|i| app.dispatch(i)));
            }
            // The jog buttons act on press and release (their hold handler), not on a click.
            LaserFormAction::Jog(_) => {}
            // Step size and frame speed commit (Return, Tab): the text goes to presentation as
            // typed. Return then Tab (or a field that only lost focus) is one commit: text that is
            // empty or equals what the panel shows sends nothing.
            LaserFormAction::StepSize | LaserFormAction::FrameSpeed => {
                let Some(field) = (sender as &AnyObject).downcast_ref::<NSTextField>() else { return };
                let text = field.stringValue().to_string();
                let shown = self.view().map(|vm| laser_panel(Some(&vm), self.ivars().locale));
                let is_step = action == LaserFormAction::StepSize;
                let unchanged = shown
                    .is_none_or(|p| text.trim().is_empty() || text == if is_step { p.step_mm } else { p.frame_speed });
                if !unchanged {
                    self.setup(if is_step { LaserSetup::StepMm(text) } else { LaserSetup::FrameSpeed(text) });
                }
            }
            // "Ga naar": the typed X and Y go to presentation unparsed.
            LaserFormAction::MoveTo => {
                let (x, y) = match self.ivars().handles.try_borrow() {
                    Ok(h) => (Self::text_of(&h.move_x), Self::text_of(&h.move_y)),
                    Err(_) => return,
                };
                self.with_view(|app, vm| move_intent(document, vm, &x, &y).map(|i| app.dispatch(i)));
            }
            // The typed values are read when "Ga naar" or "RD bewaren" is pressed.
            LaserFormAction::MoveX | LaserFormAction::MoveY | LaserFormAction::RdName => {}
            LaserFormAction::SetOrigin => self.setup(LaserSetup::SetOriginHere),
            LaserFormAction::ClearOrigin => self.setup(LaserSetup::ClearOrigin),
            // "Software-stop": the machine panel's cancel control, with the same key and seen state.
            LaserFormAction::SoftStop => {
                if let Some(app) = self.app() {
                    app.press_machine_control(document, burnburn_presentation::MachineAction::Cancel);
                }
            }
            LaserFormAction::SaveRd => {
                let name = match self.ivars().handles.try_borrow() {
                    Ok(h) => Self::text_of(&h.rd_name),
                    Err(_) => return,
                };
                self.with_view(|app, vm| save_rd_intent(document, vm, &name).map(|i| app.dispatch(i)));
            }
            LaserFormAction::Check(check) => {
                let Some(button) = (sender as &AnyObject).downcast_ref::<NSButton>() else { return };
                let on = button.state() == NSControlStateValueOn;
                self.setup(match check {
                    LaserCheck::CutSelected => LaserSetup::CutSelected(on),
                    LaserCheck::UseSelectionOrigin => LaserSetup::UseSelectionOrigin(on),
                    LaserCheck::PositionLaser => LaserSetup::PositionLaserMode(on),
                    LaserCheck::PointerOffset => LaserSetup::UsePointerOffset(on),
                });
            }
            LaserFormAction::StartFrom => {
                let Some(popup) = (sender as &AnyObject).downcast_ref::<NSPopUpButton>() else { return };
                if let Some(start) = start_from_for_segment(popup.indexOfSelectedItem()) {
                    self.setup(LaserSetup::StartFrom(start));
                }
            }
            LaserFormAction::Anchor(cell) => {
                if let Some(anchor) = anchor_for_tag(cell as isize) {
                    self.setup(LaserSetup::Anchor(anchor));
                }
            }
            // Another file: the buttons of the list follow the chosen file.
            LaserFormAction::FileChoice => {
                if let Some(app) = self.app() {
                    app.refresh_laser_panel();
                }
            }
            // Info / run / delete of the chosen controller file. Delete asks first.
            LaserFormAction::File(which) => {
                let Some(row) = self.chosen_file() else { return };
                let intent = self.view().and_then(|vm| file_intent(document, &vm, row, which));
                let Some(intent) = intent else { return };
                if let Some(app) = self.app() {
                    if which == 2 {
                        app.confirm_controller_file_delete(document, intent);
                    } else {
                        app.dispatch(intent);
                    }
                }
            }
            LaserFormAction::Diagnostics => {
                if let Some(app) = self.app() {
                    app.show_diagnostics_panel();
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The section: the form in a scrolling panel.
// ---------------------------------------------------------------------------------------------

pub(crate) struct LaserSection {
    pub(crate) panel: Retained<LaserPanelWindow>,
    pub(crate) target: Retained<LaserTarget>,
    page: Retained<NSScrollView>,
    form: FormView,
    mtm: MainThreadMarker,
    app: Weak<HostApp>,
    document: DocumentId,
    locale: Locale,
    /// A jog button is held: the form is not built again under the pointer.
    holding: Rc<Cell<bool>>,
    /// The user asked for the panel (menu); it is on screen while this is true.
    pub(crate) shown: bool,
}

/// Builds the form and puts the form id on every control as its tag (what `laserForm:` reports).
fn build(mtm: MainThreadMarker, target: &LaserTarget, spec: &FormSpec) -> FormView {
    let t: &AnyObject = target;
    let view = build_form(mtm, spec, WIDTH, Some(Wiring { target: t, action: sel!(laserForm:) }));
    for (id, control) in view.controls() {
        let any: &AnyObject = control.as_ref();
        if let Some(c) = any.downcast_ref::<NSControl>() {
            c.setTag(id.0 as isize);
        }
    }
    view
}

/// The symbol a pad glyph is drawn as.
fn symbol_of(glyph: PadGlyph) -> Symbol {
    match glyph {
        PadGlyph::Left => Symbol::ArrowLeft,
        PadGlyph::Right => Symbol::ArrowRight,
        PadGlyph::Up => Symbol::ArrowUp,
        PadGlyph::Down => Symbol::ArrowDown,
        PadGlyph::Off => Symbol::AnchorOff,
        PadGlyph::On => Symbol::AnchorOn,
    }
}

pub(crate) fn laser_section(
    mtm: MainThreadMarker,
    app: Weak<HostApp>,
    document: DocumentId,
    locale: Locale,
) -> LaserSection {
    let panel = LaserPanelWindow::new(mtm, app.clone(), document);
    // SAFETY: ownership stays with this Retained.
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setFloatingPanel(true);
    panel.setHidesOnDeactivate(true);
    panel.setTitle(&NSString::from_str(laser_text(LaserText::PanelTitle, locale)));

    let target = LaserTarget::alloc(mtm).set_ivars(TargetIvars {
        app: app.clone(),
        document,
        handles: RefCell::new(Handles::default()),
        locale,
    });
    // SAFETY: NSObject's designated initialiser on a freshly allocated instance with ivars set.
    let target: Retained<LaserTarget> = unsafe { msg_send![super(target), init] };
    let spec = laser_form(&laser_panel(None, locale), 0, locale);
    let form = build(mtm, &target, &spec);

    // The content is taller than a laptop screen: it scrolls in a window of at most the screen.
    let size = form.size();
    let page = NSScrollView::initWithFrame(
        NSScrollView::alloc(mtm),
        rect(0.0, 0.0, WIDTH, size.height.min(FIRST_VISIBLE_HEIGHT)),
    );
    page.setHasVerticalScroller(true);
    page.setAutohidesScrollers(true);
    page.setDrawsBackground(false);
    page.setAutoresizingMask(NSAutoresizingMaskOptions(
        NSAutoresizingMaskOptions::ViewWidthSizable.0 | NSAutoresizingMaskOptions::ViewHeightSizable.0,
    ));
    page.setDocumentView(Some(form.root()));
    panel.setContentView(Some(&page));
    panel.setContentSize(NSSize::new(WIDTH, size.height.min(FIRST_VISIBLE_HEIGHT)));
    panel.setContentMaxSize(NSSize::new(WIDTH, size.height));
    panel.setContentMinSize(NSSize::new(WIDTH, 240.0_f64.min(size.height)));
    form.root().scrollPoint(NSPoint::new(0.0, 1.0e7));

    let mut section = LaserSection {
        panel,
        target,
        page,
        form,
        mtm,
        app,
        document,
        locale,
        holding: Rc::new(Cell::new(false)),
        shown: false,
    };
    section.attach(&spec);
    section
}

/// The widgets of a laser panel as native tests drive them (like `WindowParts` for the window).
pub struct LaserParts<'a> {
    pub panel: &'a NSPanel,
    pub shown: bool,
    /// The form: find a control by its id (`burnburn_macos::panels::laser_id`).
    pub form: &'a FormView,
}

impl LaserParts<'_> {
    /// The button (or check box) with this form id.
    pub fn button(&self, id: u32) -> Option<Retained<NSButton>> {
        self.form.control(FormId(id)).and_then(|v| v.downcast::<NSButton>().ok())
    }

    /// The text of a read-only row, a note or a field.
    pub fn text(&self, id: u32) -> String {
        self.form
            .control(FormId(id))
            .and_then(|v| v.downcast::<NSTextField>().ok())
            .map(|f| f.stringValue().to_string())
            .unwrap_or_default()
    }

    /// Presses (`true`) or releases (`false`) a jog button the way AppKit's tracking loop reports it
    /// to the button; `false` when the id is no jog button or the press is refused.
    pub fn hold(&self, id: u32, press: bool) -> bool {
        self.button(id).is_some_and(|b| fire_hold(&b, press))
    }

    /// The help line under a row.
    pub fn help(&self, id: u32) -> String {
        self.form.help_label(FormId(id)).map(|l| l.stringValue().to_string()).unwrap_or_default()
    }
}

impl LaserSection {
    pub(crate) fn parts(&self) -> LaserParts<'_> {
        LaserParts { panel: &self.panel, shown: self.shown, form: &self.form }
    }

    /// The row of the controller file chosen in the list (0 without one).
    fn selected_file(&self) -> usize {
        self.form
            .values()
            .into_iter()
            .find(|(id, _)| id.0 == laser_id::FILE_CHOICE)
            .and_then(|(_, v)| v.parse().ok())
            .unwrap_or(0)
    }

    /// The fields an action reads, the hold handlers of the jog cross and the symbols on the pad
    /// buttons, for the form that is there now.
    fn attach(&mut self, spec: &FormSpec) {
        let field = |id: u32| self.form.control(FormId(id)).and_then(|v| v.downcast::<NSTextField>().ok());
        if let Ok(mut h) = self.target.ivars().handles.try_borrow_mut() {
            *h = Handles {
                move_x: field(laser_id::MOVE_X),
                move_y: field(laser_id::MOVE_Y),
                rd_name: field(laser_id::RD_NAME),
                file_choice: self
                    .form
                    .control(FormId(laser_id::FILE_CHOICE))
                    .and_then(|v| v.downcast::<NSPopUpButton>().ok()),
            };
        }
        for cell in 0..9 {
            let id = FormId(laser_id::JOG_PAD + cell);
            let Some(LaserFormAction::Jog(key)) = laser_form_action(id) else { continue };
            let Some(button) = self.form.control(id).and_then(|v| v.downcast::<NSButton>().ok()) else { continue };
            let (app, document, holding) = (self.app.clone(), self.document, self.holding.clone());
            let handler: HoldHandler = Rc::new(move |press: bool| {
                let Some(app) = app.upgrade() else { return false };
                if press {
                    let intent = app.laser_view_for(document).and_then(|vm| jog_press_intent(document, &vm, key));
                    let Some(intent) = intent else { return false };
                    holding.set(true);
                    app.dispatch(intent);
                    true
                } else {
                    holding.set(false);
                    app.dispatch(jog_release_intent());
                    true
                }
            });
            set_hold_handler(&button, Some(handler));
        }
        self.decorate(spec);
    }

    /// The symbols on the pad buttons (the arrows, the dot that is on or off).
    fn decorate(&self, spec: &FormSpec) {
        for row in spec.sections.iter().flat_map(|s| &s.rows) {
            let Control::Pad { cells, .. } = &row.control else { continue };
            for (i, cell) in cells.iter().enumerate() {
                let Some(glyph) = cell.glyph.filter(|_| cell.visible) else { continue };
                let id = FormId(row.id.0 + i as u32);
                let Some(button) = self.form.control(id).and_then(|v| v.downcast::<NSButton>().ok()) else { continue };
                symbols::set_symbol(&button, symbol_of(glyph), self.locale);
                button.setImagePosition(if cell.title.is_empty() {
                    NSCellImagePosition::ImageOnly
                } else if glyph == PadGlyph::Right {
                    NSCellImagePosition::ImageTrailing
                } else {
                    NSCellImagePosition::ImageLeading
                });
            }
        }
    }

    /// How far the visible part lies below the top of the content.
    fn top_gap(&self) -> f64 {
        let Some(doc) = self.page.documentView() else { return 0.0 };
        let visible = doc.visibleRect();
        doc.frame().size.height - (visible.origin.y + visible.size.height)
    }

    fn restore_top_gap(&self, gap: f64) {
        let Some(doc) = self.page.documentView() else { return };
        let clip_h = self.page.contentSize().height;
        doc.scrollPoint(NSPoint::new(0.0, (doc.frame().size.height - clip_h - gap).max(0.0)));
    }

    /// Copies `spec` into the form. A form of another shape (the position became known, the list of
    /// files has another length) is built again: what the user typed is kept, the place in the
    /// scrolled content too. Never while a jog button is held: the button under the pointer stays.
    fn apply(&mut self, spec: &FormSpec) {
        let gap = self.top_gap();
        if !self.form.update(spec) {
            if self.holding.get() {
                return;
            }
            let edits = self.form.edits();
            self.form.clear_targets();
            let mut form = build(self.mtm, &self.target, spec);
            for (id, typed, pushed) in edits {
                form.restore(id, &typed, &pushed);
            }
            self.page.setDocumentView(Some(form.root()));
            self.form = form;
            self.attach(spec);
        } else {
            self.decorate(spec);
        }
        let height = self.form.size().height;
        self.panel.setContentMaxSize(NSSize::new(WIDTH, height));
        self.restore_top_gap(gap);
    }

    /// Clears every target and hold handler (a nil one cannot dangle) before the target goes.
    pub(crate) fn clear(&self) {
        self.form.clear_targets();
        if self.shown {
            self.panel.orderOut(None);
        }
    }
}

impl DocWindow {
    /// Shows or hides the laser panel (Laser ▸ Laservenster). Hiding releases a held jog key.
    pub(crate) fn toggle_laser(&mut self) -> bool {
        self.laser.shown = !self.laser.shown;
        let panel = self.laser.panel.clone();
        if self.laser.shown {
            let frame = self.window.frame();
            let content = self.laser.form.size().height;
            // The visible part of the screen the document window is on (menu bar and Dock excluded).
            let window = self.window.clone();
            if let Err(e) = crate::appkit::objc_try("showing the laser panel", || {
                let visible = window.screen().or_else(|| panel.screen()).map(|s| s.visibleFrame());
                if let Some(v) = visible {
                    panel.setContentSize(NSSize::new(WIDTH, laser_window_height(v.size.height, content)));
                }
                let size = panel.frame().size;
                let wanted = (frame.origin.x + 24.0, frame.origin.y + frame.size.height - 60.0);
                let (x, y) = match visible {
                    Some(v) => laser_window_top_left(
                        wanted,
                        (size.width, size.height),
                        (v.origin.x, v.origin.y, v.size.width, v.size.height),
                    ),
                    None => wanted,
                };
                panel.setFrameTopLeftPoint(NSPoint::new(x, y));
                // Show the top of the content (the window may be shorter than the content).
                if let Some(doc) =
                    panel.contentView().and_then(|v| v.downcast::<NSScrollView>().ok()).and_then(|s| s.documentView())
                {
                    doc.scrollPoint(NSPoint::new(0.0, 1.0e7));
                }
                panel.orderFront(None);
                // The window itself takes the keyboard, not the first text field: the arrow keys
                // jog until the user clicks into a field.
                panel.makeFirstResponder(None);
            }) {
                eprintln!("burnburn: {e}");
            }
        } else if let Err(e) = crate::appkit::objc_try("hiding the laser panel", || panel.orderOut(None)) {
            eprintln!("burnburn: {e}");
        }
        self.laser.shown
    }

    /// Copies the laser model into the form (only while the panel is shown).
    pub(crate) fn refresh_laser(&mut self, vm: Option<&LaserViewModel>) {
        if !self.laser.shown {
            return;
        }
        let model = laser_panel(vm, self.locale);
        let spec = laser_form(&model, self.laser.selected_file(), self.locale);
        self.laser.apply(&spec);
    }
}
