//! The laser window (W2-L1-laser): a floating utility panel per document window with the position,
//! jog (hold), step, numeric move, position laser, home/origins, pause/resume/stop, start from and
//! the 3×3 job origin, cut selected, frame, save RD, and the files on the controller.
//!
//! The panel only copies the headless model (`crate::panels::laser_panel`) into widgets and turns
//! user actions into `UiIntent`s via the headless builders. Jog is press-and-hold: the button (and
//! the arrow keys while the panel has the keyboard) press on key-down and release on key-up, when
//! the panel loses the keyboard, hides, or closes. Key repeat is not a new press.
//! Nothing here talks to a machine; it all goes through the presentation layer.

#![allow(unsafe_code)] // defining Objective-C subclasses and sending to `super` has no safe API (SAFETY below)

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use burnburn_presentation::{JogKey, LaserControlKind, LaserSetup, LaserViewModel};
use burnburn_types::DocumentId;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAccessibility, NSAutoresizingMaskOptions, NSBackingStoreType, NSButton, NSColor, NSControlStateValueOff,
    NSControlStateValueOn, NSEvent, NSEventType, NSFont, NSPanel, NSScrollView, NSSegmentSwitchTracking,
    NSSegmentedControl, NSTableColumn, NSTableView, NSTableViewDataSource, NSText, NSTextField, NSView,
    NSWindowStyleMask,
};
use objc2_foundation::{NSInteger, NSPoint, NSRect, NSSize, NSString};

use crate::appkit::app::HostApp;
use crate::appkit::classes::ffi_guard;
use crate::appkit::panels::widgets::{field, label, push_button, rect, selected_rows, set_text};
use crate::appkit::symbols::{self, Symbol};
use crate::appkit::window::DocWindow;
use crate::l10n::Locale;
use crate::panels::{
    LaserPanel, LaserText, anchor_for_tag, anchor_text, file_intent, jog_key_for_key_code, jog_press_intent,
    jog_release_intent, laser_panel, laser_text, move_intent, save_rd_intent, segment_for_start_from, setup_intent,
    simple_controls, simple_intent, start_from_for_segment,
};

pub(crate) const WIDTH: f64 = 590.0;
/// Height of the laser window's content. The panel shows at most [`VISIBLE_HEIGHT`] of it and scrolls.
pub(crate) const HEIGHT: f64 = 928.0;
/// How much of the content the panel shows at most (a laptop screen is about 800 pt high).
pub(crate) const VISIBLE_HEIGHT: f64 = 700.0;
const MARGIN: f64 = 12.0;

/// Tags of the three check boxes.
const CHECK_CUT_SELECTED: isize = 0;
const CHECK_SELECTION_ORIGIN: isize = 1;
const CHECK_POSITION_LASER: isize = 2;
/// The fourth, kept apart from the three (W09): aim the job with the red dot.
const CHECK_POINTER_OFFSET: isize = 3;

// ---------------------------------------------------------------------------------------------
// Jog button: key-down on mouse-down, key-up on mouse-up (AppKit tracks the click in `super`).
// ---------------------------------------------------------------------------------------------

pub(crate) struct JogIvars {
    app: Weak<HostApp>,
    document: DocumentId,
    key: JogKey,
}

define_class!(
    #[unsafe(super(NSButton))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnJogButton"]
    #[ivars = JogIvars]
    pub(crate) struct JogButton;

    impl JogButton {
        /// Press: the head moves until the release. The tracking loop of `super` returns at
        /// mouse-up (also when the pointer left the button), and the release follows in every case.
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            ffi_guard("jog mouseDown:", (), || {
                let ivars = self.ivars();
                let Some(app) = ivars.app.upgrade() else { return };
                if !self.isEnabled() {
                    return;
                }
                let intent = app
                    .laser_view_for(ivars.document)
                    .and_then(|vm| jog_press_intent(ivars.document, &vm, ivars.key));
                let Some(intent) = intent else { return };
                app.dispatch(intent);
                // SAFETY: NSButton's own implementation with the same event, initialised instance.
                let () = unsafe { msg_send![super(self), mouseDown: event] };
                app.dispatch(jog_release_intent());
            });
        }
    }

    unsafe impl NSObjectProtocol for JogButton {}
);

impl JogButton {
    fn new(
        mtm: MainThreadMarker,
        app: Weak<HostApp>,
        document: DocumentId,
        key: JogKey,
        title: &str,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(JogIvars { app, document, key });
        // SAFETY: NSButton's designated initialiser on a freshly allocated instance with ivars set.
        let b: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: rect(0.0, 0.0, 100.0, 28.0)] };
        b.setTitle(&NSString::from_str(title));
        b.setBezelStyle(objc2_app_kit::NSBezelStyle::Push);
        b
    }
}

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
                initWithContentRect: rect(0.0, 0.0, WIDTH, VISIBLE_HEIGHT),
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
// Target of every other control, and data source of the controller-file table.
// ---------------------------------------------------------------------------------------------

pub(crate) struct TargetIvars {
    app: Weak<HostApp>,
    document: DocumentId,
    move_fields: [Retained<NSTextField>; 2],
    step_field: Retained<NSTextField>,
    frame_speed: Retained<NSTextField>,
    rd_name: Retained<NSTextField>,
    files_table: Retained<NSTableView>,
    files: RefCell<Vec<(String, String)>>,
    locale: Locale,
}

define_class!(
    #[unsafe(super(objc2::runtime::NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnLaserTarget"]
    #[ivars = TargetIvars]
    pub(crate) struct LaserTarget;

    impl LaserTarget {
        /// A button with a fixed command; its tag is its index in `simple_controls()`.
        #[unsafe(method(laserSimple:))]
        fn laser_simple(&self, sender: Option<&AnyObject>) {
            ffi_guard("laserSimple:", (), || {
                let Some(button) = sender.and_then(|s| s.downcast_ref::<NSButton>()) else { return };
                self.with_view(|app, vm| simple_intent(self.ivars().document, vm, button.tag()).map(|i| app.dispatch(i)));
            });
        }

        /// "Ga naar": the typed X and Y go to presentation unparsed.
        #[unsafe(method(laserMove:))]
        fn laser_move(&self, _sender: Option<&AnyObject>) {
            ffi_guard("laserMove:", (), || {
                let [x, y] = &self.ivars().move_fields;
                let (x, y) = (x.stringValue().to_string(), y.stringValue().to_string());
                self.with_view(|app, vm| move_intent(self.ivars().document, vm, &x, &y).map(|i| app.dispatch(i)));
            });
        }

        #[unsafe(method(laserStartFrom:))]
        fn laser_start_from(&self, sender: Option<&AnyObject>) {
            ffi_guard("laserStartFrom:", (), || {
                let Some(control) = sender.and_then(|s| s.downcast_ref::<NSSegmentedControl>()) else { return };
                if let (Some(start), Some(app)) = (start_from_for_segment(control.selectedSegment()), self.app()) {
                    app.dispatch(setup_intent(LaserSetup::StartFrom(start)));
                }
            });
        }

        #[unsafe(method(laserAnchor:))]
        fn laser_anchor(&self, sender: Option<&AnyObject>) {
            ffi_guard("laserAnchor:", (), || {
                let Some(button) = sender.and_then(|s| s.downcast_ref::<NSButton>()) else { return };
                if let (Some(anchor), Some(app)) = (anchor_for_tag(button.tag()), self.app()) {
                    app.dispatch(setup_intent(LaserSetup::Anchor(anchor)));
                }
            });
        }

        /// The check boxes: cut selected, use selection origin, position-laser mode.
        #[unsafe(method(laserCheck:))]
        fn laser_check(&self, sender: Option<&AnyObject>) {
            ffi_guard("laserCheck:", (), || {
                let Some(button) = sender.and_then(|s| s.downcast_ref::<NSButton>()) else { return };
                let on = button.state() == NSControlStateValueOn;
                let setup = match button.tag() {
                    CHECK_CUT_SELECTED => LaserSetup::CutSelected(on),
                    CHECK_SELECTION_ORIGIN => LaserSetup::UseSelectionOrigin(on),
                    CHECK_POSITION_LASER => LaserSetup::PositionLaserMode(on),
                    CHECK_POINTER_OFFSET => LaserSetup::UsePointerOffset(on),
                    _ => return,
                };
                if let Some(app) = self.app() {
                    app.dispatch(setup_intent(setup));
                }
            });
        }

        /// Step size and frame speed commit (Return, Tab): the text goes to presentation as typed.
        #[unsafe(method(laserNumber:))]
        fn laser_number(&self, sender: Option<&AnyObject>) {
            ffi_guard("laserNumber:", (), || {
                let Some(field) = sender.and_then(|s| s.downcast_ref::<NSTextField>()) else { return };
                let text = field.stringValue().to_string();
                // Return then Tab (or a field that only lost focus) is one commit: text that is
                // empty or equals what the panel shows sends nothing.
                let shown = self.view().map(|vm| laser_panel(Some(&vm), self.ivars().locale));
                let (is_step, is_speed) = (
                    std::ptr::eq(field, &*self.ivars().step_field),
                    std::ptr::eq(field, &*self.ivars().frame_speed),
                );
                let unchanged = shown.is_none_or(|p| {
                    text.trim().is_empty() || (is_step && text == p.step_mm) || (is_speed && text == p.frame_speed)
                });
                if unchanged {
                    return;
                }
                let setup = if is_step {
                    LaserSetup::StepMm(text)
                } else if is_speed {
                    LaserSetup::FrameSpeed(text)
                } else {
                    return;
                };
                if let Some(app) = self.app() {
                    app.dispatch(setup_intent(setup));
                }
            });
        }

        #[unsafe(method(laserSetOrigin:))]
        fn laser_set_origin(&self, _sender: Option<&AnyObject>) {
            ffi_guard("laserSetOrigin:", (), || {
                if let Some(app) = self.app() {
                    app.dispatch(setup_intent(LaserSetup::SetOriginHere));
                }
            });
        }

        #[unsafe(method(laserClearOrigin:))]
        fn laser_clear_origin(&self, _sender: Option<&AnyObject>) {
            ffi_guard("laserClearOrigin:", (), || {
                if let Some(app) = self.app() {
                    app.dispatch(setup_intent(LaserSetup::ClearOrigin));
                }
            });
        }

        /// "Software-stop": the machine panel's cancel control, with the same key and seen state.
        #[unsafe(method(laserSoftStop:))]
        fn laser_soft_stop(&self, _sender: Option<&AnyObject>) {
            ffi_guard("laserSoftStop:", (), || {
                if let Some(app) = self.app() {
                    app.press_machine_control(self.ivars().document, burnburn_presentation::MachineAction::Cancel);
                }
            });
        }

        #[unsafe(method(laserSaveRd:))]
        fn laser_save_rd(&self, _sender: Option<&AnyObject>) {
            ffi_guard("laserSaveRd:", (), || {
                let name = self.ivars().rd_name.stringValue().to_string();
                self.with_view(|app, vm| save_rd_intent(self.ivars().document, vm, &name).map(|i| app.dispatch(i)));
            });
        }

        /// Info / run / delete of the selected controller file; tag 0, 1, 2. Delete asks first.
        #[unsafe(method(laserFile:))]
        fn laser_file(&self, sender: Option<&AnyObject>) {
            ffi_guard("laserFile:", (), || {
                let Some(button) = sender.and_then(|s| s.downcast_ref::<NSButton>()) else { return };
                let Some(row) = selected_rows(&self.ivars().files_table).first().copied() else { return };
                let which = usize::try_from(button.tag()).unwrap_or(usize::MAX);
                let document = self.ivars().document;
                let intent = self.view().and_then(|vm| file_intent(document, &vm, row, which));
                let Some(intent) = intent else { return };
                if which == 2 {
                    if let Some(app) = self.app() {
                        app.confirm_controller_file_delete(document, intent);
                    }
                } else if let Some(app) = self.app() {
                    app.dispatch(intent);
                }
            });
        }
    }

    unsafe impl NSObjectProtocol for LaserTarget {}

    unsafe impl NSTableViewDataSource for LaserTarget {
        #[unsafe(method(numberOfRowsInTableView:))]
        fn number_of_rows(&self, _table_view: &NSTableView) -> NSInteger {
            ffi_guard("laser numberOfRowsInTableView:", 0, || {
                self.ivars().files.try_borrow().map_or(0, |f| f.len() as NSInteger)
            })
        }

        #[unsafe(method_id(tableView:objectValueForTableColumn:row:))]
        fn object_value(
            &self,
            _table_view: &NSTableView,
            table_column: Option<&NSTableColumn>,
            row: NSInteger,
        ) -> Option<Retained<AnyObject>> {
            ffi_guard("laser tableView:objectValueForTableColumn:row:", None, || {
                let row = usize::try_from(row).ok()?;
                let files = self.ivars().files.try_borrow().ok()?;
                let (name, size) = files.get(row)?;
                let text = if table_column?.identifier().to_string() == "name" { name } else { size };
                Some(Retained::into_super(Retained::into_super(NSString::from_str(text))))
            })
        }
    }
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

    /// The files shown by the table; true when they changed.
    fn set_files(&self, files: Vec<(String, String)>) -> bool {
        match self.ivars().files.try_borrow_mut() {
            Ok(mut current) if *current != files => {
                *current = files;
                true
            }
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The section: widgets in one panel.
// ---------------------------------------------------------------------------------------------

pub(crate) struct LaserSection {
    pub(crate) panel: Retained<LaserPanelWindow>,
    pub(crate) target: Retained<LaserTarget>,
    position: Retained<NSTextField>,
    status: Retained<NSTextField>,
    placement: Retained<NSTextField>,
    origin: Retained<NSTextField>,
    jog: Vec<(JogKey, Retained<JogButton>)>,
    /// Buttons with a fixed command, in `simple_controls()` order.
    simple: Vec<Retained<NSButton>>,
    move_button: Retained<NSButton>,
    move_fields: [Retained<NSTextField>; 2],
    set_origin: Retained<NSButton>,
    clear_origin: Retained<NSButton>,
    soft_stop: Retained<NSButton>,
    checks: [Retained<NSButton>; 3],
    /// "Aim with the red dot" and the offset it uses, in words (W09).
    pointer_check: Retained<NSButton>,
    pointer_note: Retained<NSTextField>,
    start_from: Retained<NSSegmentedControl>,
    anchors: Vec<Retained<NSButton>>,
    step_field: Retained<NSTextField>,
    frame_speed: Retained<NSTextField>,
    save_rd: Retained<NSButton>,
    rd_name: Retained<NSTextField>,
    files_table: Retained<NSTableView>,
    file_buttons: [Retained<NSButton>; 3],
    /// The user asked for the panel (menu); it is on screen while this is true.
    pub(crate) shown: bool,
}

#[allow(unsafe_code)]
fn wire_button(b: &NSButton, target: &AnyObject, action: objc2::runtime::Sel, tag: isize) {
    b.setTag(tag);
    // SAFETY: `action` is a `LaserTarget` method taking the sender (`v@:@`); the target is weak and
    // the DocWindow keeps it alive while the button exists (cleared in `LaserSection::clear`).
    unsafe { b.setTarget(Some(target)) };
    // SAFETY: as above.
    unsafe { b.setAction(Some(action)) };
}

#[allow(unsafe_code)]
pub(crate) fn laser_section(
    mtm: MainThreadMarker,
    app: Weak<HostApp>,
    document: DocumentId,
    locale: Locale,
) -> LaserSection {
    let t = |text: LaserText| laser_text(text, locale);
    let panel = LaserPanelWindow::new(mtm, app.clone(), document);
    // SAFETY: ownership stays with this Retained.
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setFloatingPanel(true);
    panel.setHidesOnDeactivate(true);
    panel.setTitle(&NSString::from_str(t(LaserText::PanelTitle)));
    let content = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, WIDTH, HEIGHT));
    let inner = WIDTH - 2.0 * MARGIN;
    let y = std::cell::Cell::new(HEIGHT - 6.0);
    let row = |height: f64, gap: f64| {
        y.set(y.get() - height);
        let at = y.get();
        y.set(at - gap);
        at
    };

    // Position and status.
    let at = row(20.0, 4.0);
    let position = label(mtm, "", rect(MARGIN, at, inner - 170.0, 20.0));
    position.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
    content.addSubview(&position);
    let simple_tags = simple_controls();
    let tag_of = |kind: &LaserControlKind| simple_tags.iter().position(|(k, _)| k == kind).unwrap_or(0) as isize;
    let mut simple: Vec<Option<Retained<NSButton>>> = vec![None; simple_tags.len()];
    let button = |text: LaserText| push_button(mtm, t(text));
    let simple_button =
        |simple: &mut Vec<Option<Retained<NSButton>>>, kind: LaserControlKind, text: LaserText, frame: NSRect| {
            let b = button(text);
            let tag = tag_of(&kind);
            b.setFrame(frame);
            content.addSubview(&b);
            if let Some(slot) = simple.get_mut(usize::try_from(tag).unwrap_or(0)) {
                *slot = Some(b.clone());
            }
            b
        };
    simple_button(
        &mut simple,
        LaserControlKind::ShowPosition,
        LaserText::ShowPosition,
        rect(WIDTH - MARGIN - 160.0, at - 4.0, 160.0, 28.0),
    );
    let at = row(34.0, 2.0);
    let status = label(mtm, "", rect(MARGIN, at, inner, 34.0));
    status.setTextColor(Some(&NSColor::systemRedColor()));
    status.setLineBreakMode(objc2_app_kit::NSLineBreakMode::ByWordWrapping);
    content.addSubview(&status);
    let at = row(18.0, 6.0);
    let placement = label(mtm, "", rect(MARGIN, at, inner, 18.0));
    placement.setFont(Some(&NSFont::systemFontOfSize(11.0)));
    content.addSubview(&placement);

    // Jog (hold) and step.
    let at = row(18.0, 2.0);
    let heading = |text: &str, at: f64| {
        let l = label(mtm, text, rect(MARGIN, at, inner, 18.0));
        l.setFont(Some(&NSFont::boldSystemFontOfSize(12.0)));
        content.addSubview(&l);
    };
    heading(t(LaserText::Jog), at);
    let at = row(28.0, 6.0);
    let quarter = (inner - 3.0 * 6.0) / 4.0;
    let mut jog = Vec::new();
    for (i, (key, text)) in [
        (JogKey::XLeft, LaserText::JogLeft),
        (JogKey::XRight, LaserText::JogRight),
        (JogKey::YTop, LaserText::JogUp),
        (JogKey::YBottom, LaserText::JogDown),
    ]
    .into_iter()
    .enumerate()
    {
        let b = JogButton::new(mtm, app.clone(), document, key, t(text));
        let symbol = match key {
            JogKey::XLeft => Some((Symbol::ArrowLeft, false)),
            JogKey::XRight => Some((Symbol::ArrowRight, true)),
            JogKey::YTop => Some((Symbol::ArrowUp, false)),
            JogKey::YBottom => Some((Symbol::ArrowDown, false)),
            _ => None,
        };
        if let Some((symbol, trailing)) = symbol {
            symbols::apply_with_title(&b, symbol, locale, t(text), trailing);
        }
        b.setFrame(rect(MARGIN + i as f64 * (quarter + 6.0), at, quarter, 28.0));
        content.addSubview(&b);
        jog.push((key, b));
    }
    let at = row(18.0, 2.0);
    heading(t(LaserText::Step), at);
    let at = row(28.0, 6.0);
    let small = 50.0;
    let arrows = [
        (StepDirectionTag::Left, Symbol::ArrowLeft),
        (StepDirectionTag::Right, Symbol::ArrowRight),
        (StepDirectionTag::Up, Symbol::ArrowUp),
        (StepDirectionTag::Down, Symbol::ArrowDown),
    ];
    for (i, (dir, symbol)) in arrows.into_iter().enumerate() {
        let b = push_button(mtm, "");
        symbols::apply(&b, symbol, locale);
        b.setFrame(rect(MARGIN + i as f64 * (small + 6.0), at, small, 28.0));
        content.addSubview(&b);
        if let Some(slot) = simple.get_mut(usize::try_from(tag_of(&LaserControlKind::Step(dir.into()))).unwrap_or(0)) {
            *slot = Some(b);
        }
    }
    // The caption names the field and its unit (it says "(mm)").
    let step_caption =
        label(mtm, t(LaserText::StepSize), rect(WIDTH - MARGIN - 66.0 - 6.0 - 140.0, at + 4.0, 140.0, 20.0));
    step_caption.setAlignment(objc2_app_kit::NSTextAlignment::Right);
    content.addSubview(&step_caption);
    let step_field = field(mtm, rect(WIDTH - MARGIN - 66.0, at + 2.0, 66.0, 24.0));
    step_field.setAccessibilityLabel(Some(&NSString::from_str(t(LaserText::StepSize))));
    content.addSubview(&step_field);

    // Numeric move and position laser.
    let at = row(18.0, 2.0);
    heading(t(LaserText::MoveTo), at);
    let at = row(28.0, 4.0);
    let move_x = field(mtm, rect(MARGIN, at + 2.0, 90.0, 24.0));
    move_x.setPlaceholderString(Some(&NSString::from_str("X")));
    let move_y = field(mtm, rect(MARGIN + 96.0, at + 2.0, 90.0, 24.0));
    move_y.setPlaceholderString(Some(&NSString::from_str("Y")));
    content.addSubview(&move_x);
    content.addSubview(&move_y);
    let move_button = push_button(mtm, t(LaserText::MoveToButton));
    move_button.setFrame(rect(MARGIN + 196.0, at, 110.0, 28.0));
    content.addSubview(&move_button);
    let at = row(22.0, 4.0);
    let checkbox = |text: LaserText, frame: NSRect, tag: isize| {
        // SAFETY: no target and no action yet; wired below once the target exists.
        let b = unsafe { NSButton::checkboxWithTitle_target_action(&NSString::from_str(t(text)), None, None, mtm) };
        b.setFrame(frame);
        b.setTag(tag);
        content.addSubview(&b);
        b
    };
    let position_check = checkbox(LaserText::PositionLaser, rect(MARGIN, at, inner, 22.0), CHECK_POSITION_LASER);
    // The red dot (W09): the switch, and in words which measured offset it uses.
    let at = row(22.0, 0.0);
    let pointer_check = checkbox(LaserText::PointerOffsetUse, rect(MARGIN, at, inner, 22.0), CHECK_POINTER_OFFSET);
    let at = row(14.0, 4.0);
    let pointer_note = label(mtm, "", rect(MARGIN, at, inner, 14.0));
    pointer_note.setFont(Some(&NSFont::systemFontOfSize(10.0)));
    content.addSubview(&pointer_note);

    // Home, origins.
    let at = row(28.0, 4.0);
    let fifth = (inner - 2.0 * 6.0) / 3.0;
    simple_button(&mut simple, LaserControlKind::Home, LaserText::Home, rect(MARGIN, at, fifth, 28.0));
    simple_button(
        &mut simple,
        LaserControlKind::GoToMachineOrigin,
        LaserText::GoMachineOrigin,
        rect(MARGIN + fifth + 6.0, at, fifth, 28.0),
    );
    simple_button(
        &mut simple,
        LaserControlKind::GoToUserOrigin,
        LaserText::GoUserOrigin,
        rect(MARGIN + 2.0 * (fifth + 6.0), at, fifth, 28.0),
    );
    let at = row(28.0, 4.0);
    let set_origin = button(LaserText::SetOrigin);
    set_origin.setFrame(rect(MARGIN, at, fifth, 28.0));
    content.addSubview(&set_origin);
    let clear_origin = button(LaserText::ClearOrigin);
    clear_origin.setFrame(rect(MARGIN + fifth + 6.0, at, fifth, 28.0));
    content.addSubview(&clear_origin);
    let origin = label(mtm, "", rect(MARGIN + 2.0 * (fifth + 6.0), at + 4.0, fifth, 20.0));
    origin.setFont(Some(&NSFont::systemFontOfSize(11.0)));
    content.addSubview(&origin);

    // Pause, resume, stop.
    let at = row(28.0, 2.0);
    simple_button(&mut simple, LaserControlKind::Pause, LaserText::Pause, rect(MARGIN, at, fifth, 28.0));
    simple_button(
        &mut simple,
        LaserControlKind::Resume,
        LaserText::Resume,
        rect(MARGIN + fifth + 6.0, at, fifth, 28.0),
    );
    let soft_stop = button(LaserText::SoftStop);
    soft_stop.setFrame(rect(MARGIN + 2.0 * (fifth + 6.0), at, fifth, 28.0));
    content.addSubview(&soft_stop);
    let at = row(30.0, 6.0);
    let note = label(mtm, t(LaserText::StopNote), rect(MARGIN, at, inner, 30.0));
    note.setFont(Some(&NSFont::systemFontOfSize(10.0)));
    note.setLineBreakMode(objc2_app_kit::NSLineBreakMode::ByWordWrapping);
    content.addSubview(&note);

    // Start from, 3x3 job origin, cut selected.
    let at = row(18.0, 2.0);
    heading(t(LaserText::StartFrom), at);
    let at = row(26.0, 6.0);
    let start_from = NSSegmentedControl::new(mtm);
    start_from.setSegmentCount(3);
    start_from.setTrackingMode(NSSegmentSwitchTracking::SelectOne);
    for (i, text) in [LaserText::StartAbsolute, LaserText::StartCurrent, LaserText::StartUser].into_iter().enumerate() {
        start_from.setLabel_forSegment(&NSString::from_str(t(text)), i as isize);
    }
    start_from.setFrame(rect(MARGIN, at, inner, 26.0));
    start_from.setAccessibilityLabel(Some(&NSString::from_str(t(LaserText::StartFrom))));
    content.addSubview(&start_from);
    let at = row(18.0, 2.0);
    heading(t(LaserText::JobOrigin), at);
    let grid_top = row(0.0, 0.0);
    let cell = 30.0;
    let mut anchors = Vec::new();
    for i in 0..9 {
        let (r, c) = (i / 3, i % 3);
        let b = push_button(mtm, "");
        symbols::apply(&b, Symbol::AnchorOff, locale);
        b.setFrame(rect(MARGIN + c as f64 * (cell + 4.0), grid_top - (r as f64 + 1.0) * (cell + 4.0), cell, cell));
        if let Some(anchor) = anchor_for_tag(i as isize) {
            b.setAccessibilityLabel(Some(&NSString::from_str(t(anchor_text(anchor)))));
            b.setToolTip(Some(&NSString::from_str(t(anchor_text(anchor)))));
        }
        content.addSubview(&b);
        anchors.push(b);
    }
    let right = MARGIN + 3.0 * (cell + 4.0) + 16.0;
    let cut_check = checkbox(
        LaserText::CutSelected,
        rect(right, grid_top - 34.0, WIDTH - right - MARGIN, 22.0),
        CHECK_CUT_SELECTED,
    );
    let selection_check = checkbox(
        LaserText::UseSelectionOrigin,
        rect(right, grid_top - 62.0, WIDTH - right - MARGIN, 22.0),
        CHECK_SELECTION_ORIGIN,
    );
    y.set(grid_top - 3.0 * (cell + 4.0) - 8.0);

    // Frame.
    let at = row(28.0, 2.0);
    simple_button(
        &mut simple,
        LaserControlKind::Frame(burnburn_presentation::FrameShape::BoundingRectangle),
        LaserText::FrameRectangle,
        rect(MARGIN, at, 150.0, 28.0),
    );
    simple_button(
        &mut simple,
        LaserControlKind::Frame(burnburn_presentation::FrameShape::RubberBand),
        LaserText::FrameRubberBand,
        rect(MARGIN + 156.0, at, 150.0, 28.0),
    );
    let at = row(28.0, 2.0);
    let speed_caption = label(mtm, t(LaserText::FrameSpeed), rect(MARGIN, at + 4.0, 170.0, 20.0));
    content.addSubview(&speed_caption);
    let frame_speed = field(mtm, rect(MARGIN + 176.0, at + 2.0, 66.0, 24.0));
    frame_speed.setAccessibilityLabel(Some(&NSString::from_str(t(LaserText::FrameSpeed))));
    content.addSubview(&frame_speed);
    let at = row(30.0, 6.0);
    let frame_note = label(mtm, t(LaserText::FrameNote), rect(MARGIN, at, inner, 30.0));
    frame_note.setFont(Some(&NSFont::systemFontOfSize(10.0)));
    frame_note.setLineBreakMode(objc2_app_kit::NSLineBreakMode::ByWordWrapping);
    content.addSubview(&frame_note);

    // Save RD, files.
    let at = row(28.0, 6.0);
    let rd_name = field(mtm, rect(MARGIN, at + 2.0, 200.0, 24.0));
    rd_name.setPlaceholderString(Some(&NSString::from_str(t(LaserText::SaveRdName))));
    rd_name.setAccessibilityLabel(Some(&NSString::from_str(t(LaserText::SaveRdName))));
    content.addSubview(&rd_name);
    let save_rd = button(LaserText::SaveRd);
    save_rd.setFrame(rect(MARGIN + 208.0, at, 200.0, 28.0));
    content.addSubview(&save_rd);
    let at = row(18.0, 2.0);
    heading(t(LaserText::Files), at);
    let table_height = 100.0;
    let at = row(table_height, 6.0);
    let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(MARGIN, at, inner, table_height));
    scroll.setHasVerticalScroller(true);
    let files_table = NSTableView::initWithFrame(NSTableView::alloc(mtm), rect(0.0, 0.0, inner, table_height));
    for (id, text, width) in [("name", LaserText::FileName, inner - 140.0), ("size", LaserText::FileSize, 110.0)] {
        let column = NSTableColumn::initWithIdentifier(NSTableColumn::alloc(mtm), &NSString::from_str(id));
        column.setTitle(&NSString::from_str(t(text)));
        column.setWidth(width);
        files_table.addTableColumn(&column);
    }
    files_table.setAllowsMultipleSelection(false);
    files_table.setAllowsEmptySelection(true);
    files_table.setAccessibilityLabel(Some(&NSString::from_str(t(LaserText::Files))));
    scroll.setDocumentView(Some(&files_table));
    content.addSubview(&scroll);
    let at = row(28.0, 0.0);
    let q = (inner - 3.0 * 6.0) / 4.0;
    simple_button(&mut simple, LaserControlKind::ListFiles, LaserText::ListFiles, rect(MARGIN, at, q, 28.0));
    let file_buttons = [LaserText::FileInfo, LaserText::FileRun, LaserText::FileDelete].map(button);
    for (i, b) in file_buttons.iter().enumerate() {
        b.setFrame(rect(MARGIN + (i as f64 + 1.0) * (q + 6.0), at, q, 28.0));
        content.addSubview(b);
    }
    // The content is taller than a laptop screen: it scrolls in a window of at most VISIBLE_HEIGHT.
    content.setAutoresizesSubviews(false);
    let page = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(0.0, 0.0, WIDTH, VISIBLE_HEIGHT));
    page.setHasVerticalScroller(true);
    page.setAutohidesScrollers(true);
    page.setDrawsBackground(false);
    page.setAutoresizingMask(NSAutoresizingMaskOptions(
        NSAutoresizingMaskOptions::ViewWidthSizable.0 | NSAutoresizingMaskOptions::ViewHeightSizable.0,
    ));
    page.setDocumentView(Some(&content));
    panel.setContentView(Some(&page));
    panel.setContentMaxSize(NSSize::new(WIDTH, HEIGHT));
    panel.setContentMinSize(NSSize::new(WIDTH, 240.0));
    content.scrollPoint(NSPoint::new(0.0, 1.0e7));

    let target = LaserTarget::alloc(mtm).set_ivars(TargetIvars {
        app,
        document,
        move_fields: [move_x.clone(), move_y.clone()],
        step_field: step_field.clone(),
        frame_speed: frame_speed.clone(),
        rd_name: rd_name.clone(),
        files_table: files_table.clone(),
        files: RefCell::new(Vec::new()),
        locale,
    });
    // SAFETY: NSObject's designated initialiser on a freshly allocated instance with ivars set.
    let target: Retained<LaserTarget> = unsafe { msg_send![super(target), init] };
    let t_ref: &AnyObject = &target;
    let simple: Vec<Retained<NSButton>> =
        simple.into_iter().map(|b| b.unwrap_or_else(|| push_button(mtm, ""))).collect();
    for (i, b) in simple.iter().enumerate() {
        wire_button(b, t_ref, sel!(laserSimple:), i as isize);
    }
    wire_button(&move_button, t_ref, sel!(laserMove:), 0);
    wire_button(&set_origin, t_ref, sel!(laserSetOrigin:), 0);
    wire_button(&clear_origin, t_ref, sel!(laserClearOrigin:), 0);
    wire_button(&soft_stop, t_ref, sel!(laserSoftStop:), 0);
    wire_button(&save_rd, t_ref, sel!(laserSaveRd:), 0);
    for (i, b) in anchors.iter().enumerate() {
        wire_button(b, t_ref, sel!(laserAnchor:), i as isize);
    }
    for (i, b) in file_buttons.iter().enumerate() {
        wire_button(b, t_ref, sel!(laserFile:), i as isize);
    }
    let checks = [cut_check, selection_check, position_check];
    for b in &checks {
        let tag = b.tag();
        wire_button(b, t_ref, sel!(laserCheck:), tag);
    }
    wire_button(&pointer_check, t_ref, sel!(laserCheck:), CHECK_POINTER_OFFSET);
    // SAFETY: as in `wire_button`.
    unsafe { start_from.setTarget(Some(t_ref)) };
    // SAFETY: as above.
    unsafe { start_from.setAction(Some(sel!(laserStartFrom:))) };
    for f in [&step_field, &frame_speed] {
        // SAFETY: as above.
        unsafe { f.setTarget(Some(t_ref)) };
        // SAFETY: as above.
        unsafe { f.setAction(Some(sel!(laserNumber:))) };
    }
    // SAFETY: the data source is weak and cleared in `LaserSection::clear` before the target goes.
    unsafe { files_table.setDataSource(Some(ProtocolObject::from_ref(&*target))) };

    LaserSection {
        panel,
        target,
        position,
        status,
        placement,
        origin,
        jog,
        simple,
        move_button,
        move_fields: [move_x, move_y],
        set_origin,
        clear_origin,
        soft_stop,
        checks,
        pointer_check,
        pointer_note,
        start_from,
        anchors,
        step_field,
        frame_speed,
        save_rd,
        rd_name,
        files_table,
        file_buttons,
        shown: false,
    }
}

/// The step buttons' directions (kept apart from presentation's enum for the arrow glyph table).
#[derive(Clone, Copy)]
enum StepDirectionTag {
    Left,
    Right,
    Up,
    Down,
}

impl From<StepDirectionTag> for burnburn_presentation::StepDirection {
    fn from(d: StepDirectionTag) -> Self {
        match d {
            StepDirectionTag::Left => burnburn_presentation::StepDirection::Left,
            StepDirectionTag::Right => burnburn_presentation::StepDirection::Right,
            StepDirectionTag::Up => burnburn_presentation::StepDirection::Up,
            StepDirectionTag::Down => burnburn_presentation::StepDirection::Down,
        }
    }
}

/// The widgets of a laser panel as native tests drive them (like `WindowParts` for the window).
pub struct LaserParts<'a> {
    pub panel: &'a NSPanel,
    pub shown: bool,
    pub position: &'a NSTextField,
    pub status: &'a NSTextField,
    /// The buttons with a fixed command, in `simple_controls()` order (Home first).
    pub simple: &'a [Retained<NSButton>],
    pub move_fields: [&'a NSTextField; 2],
    pub move_button: &'a NSButton,
    pub start_from: &'a NSSegmentedControl,
    pub anchors: &'a [Retained<NSButton>],
    pub checks: &'a [Retained<NSButton>; 3],
    /// The red-dot switch and the line that names the offset it uses (W09).
    pub pointer_check: &'a NSButton,
    pub pointer_note: &'a NSTextField,
    pub set_origin: &'a NSButton,
}

impl LaserSection {
    pub(crate) fn parts(&self) -> LaserParts<'_> {
        LaserParts {
            panel: &self.panel,
            shown: self.shown,
            position: &self.position,
            status: &self.status,
            simple: &self.simple,
            move_fields: [&self.move_fields[0], &self.move_fields[1]],
            move_button: &self.move_button,
            start_from: &self.start_from,
            anchors: &self.anchors,
            checks: &self.checks,
            pointer_check: &self.pointer_check,
            pointer_note: &self.pointer_note,
            set_origin: &self.set_origin,
        }
    }

    /// Clears every target and data source (a nil one cannot dangle) before the target goes.
    #[allow(unsafe_code)]
    pub(crate) fn clear(&self) {
        let buttons = self
            .simple
            .iter()
            .chain([&self.move_button, &self.set_origin, &self.clear_origin, &self.soft_stop, &self.save_rd])
            .chain(self.checks.iter())
            .chain(std::iter::once(&self.pointer_check))
            .chain(self.anchors.iter())
            .chain(self.file_buttons.iter());
        for b in buttons {
            // SAFETY: a nil target cannot dangle.
            unsafe { b.setTarget(None) };
        }
        // SAFETY: as above.
        unsafe { self.start_from.setTarget(None) };
        for f in [&self.step_field, &self.frame_speed] {
            // SAFETY: as above.
            unsafe { f.setTarget(None) };
        }
        // SAFETY: as above.
        unsafe { self.files_table.setDataSource(None) };
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
            if let Err(e) = crate::appkit::objc_try("showing the laser panel", || {
                panel.setFrameTopLeftPoint(NSPoint::new(
                    frame.origin.x + 24.0,
                    frame.origin.y + frame.size.height - 60.0,
                ));
                // Show the top of the content (the window is shorter than the content).
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

    /// Copies the laser model into the widgets (only while the panel is shown).
    pub(crate) fn refresh_laser(&mut self, vm: Option<&LaserViewModel>) {
        if !self.laser.shown {
            return;
        }
        let l = &self.laser;
        let model = laser_panel(vm, self.locale);
        set_text(&l.position, &model.position);
        set_text(&l.status, &model.status);
        set_text(&l.placement, &model.placement);
        set_text(&l.origin, &model.user_origin);
        for (key, b) in &l.jog {
            apply_control(b, &model, &LaserControlKind::Jog(*key));
        }
        for (b, (kind, _)) in l.simple.iter().zip(simple_controls()) {
            apply_control(b, &model, &kind);
        }
        for (b, kind) in [
            (&l.move_button, LaserControlKind::MoveTo),
            (&l.set_origin, LaserControlKind::SetOrigin),
            (&l.save_rd, LaserControlKind::SaveRd),
        ] {
            apply_control(b, &model, &kind);
        }
        l.clear_origin.setEnabled(model.connected);
        l.soft_stop.setEnabled(model.connected);
        for f in &l.move_fields {
            f.setEnabled(model.connected);
        }
        l.rd_name.setEnabled(model.connected);
        let position_laser = model.controls.iter().find(|c| c.kind == LaserControlKind::PositionLaser);
        l.checks[2].setEnabled(position_laser.is_some_and(|c| c.enabled) || model.position_laser_mode);
        l.checks[0].setEnabled(model.connected);
        l.checks[1].setEnabled(model.connected);
        if let Some(c) = position_laser {
            l.checks[2].setToolTip(Some(&NSString::from_str(&c.tooltip)));
        }
        for (b, on) in l.checks.iter().zip([model.cut_selected, model.use_selection_origin, model.position_laser_mode])
        {
            b.setState(if on { NSControlStateValueOn } else { NSControlStateValueOff });
        }
        l.pointer_check.setEnabled(model.connected && model.can_use_pointer_offset);
        l.pointer_check.setState(if model.use_pointer_offset { NSControlStateValueOn } else { NSControlStateValueOff });
        set_text(&l.pointer_note, &model.pointer_offset);
        l.start_from.setSelectedSegment(segment_for_start_from(model.start_from));
        l.start_from.setEnabled(model.connected);
        for (i, b) in l.anchors.iter().enumerate() {
            b.setEnabled(model.connected);
            let on = anchor_for_tag(i as isize) == Some(model.anchor);
            symbols::set_symbol(b, if on { Symbol::AnchorOn } else { Symbol::AnchorOff }, self.locale);
        }
        if !field_is_editing(&l.step_field) {
            set_text(&l.step_field, &model.step_mm);
        }
        if !field_is_editing(&l.frame_speed) {
            set_text(&l.frame_speed, &model.frame_speed);
        }
        // The controller files: rows, and the buttons of the selected row.
        if l.target.set_files(model.files.clone()) {
            let table = l.files_table.clone();
            if let Err(e) = crate::appkit::objc_try("reloading the controller files", || table.reloadData()) {
                eprintln!("burnburn: {e}");
            }
        }
        let selected = selected_rows(&l.files_table).first().copied();
        let file_control = |row: Option<usize>, which: usize| -> Option<(bool, String)> {
            let name = vm?.files.as_ref()?.get(row?)?.name.clone();
            let kind = match which {
                0 => LaserControlKind::FileMetadata(name),
                1 => LaserControlKind::RunFile(name),
                _ => LaserControlKind::DeleteFile(name),
            };
            model.controls.iter().find(|c| c.kind == kind).map(|c| (c.enabled, c.tooltip.clone()))
        };
        for (which, b) in l.file_buttons.iter().enumerate() {
            let (enabled, tip) = file_control(selected, which).unwrap_or((false, String::new()));
            b.setEnabled(enabled);
            b.setToolTip(Some(&NSString::from_str(&tip)));
        }
    }
}

fn field_is_editing(f: &NSTextField) -> bool {
    f.currentEditor().is_some()
}

/// Enablement and the reason in words as the tooltip.
fn apply_control(b: &NSButton, model: &LaserPanel, kind: &LaserControlKind) {
    let control = model.controls.iter().find(|c| &c.kind == kind);
    b.setEnabled(control.is_some_and(|c| c.enabled));
    let tip = control.map_or("", |c| c.tooltip.as_str());
    b.setToolTip(Some(&NSString::from_str(tip)));
}
