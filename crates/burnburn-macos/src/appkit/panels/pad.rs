//! The pad of the form builder (LFE): a grid of small square buttons for `Control::Pad`, the jog
//! cross and the 3x3 job origin of the laser window. A pad whose buttons *hold* gets
//! [`HoldButton`]s: the host sets a handler that is told about the press and, once AppKit's
//! tracking loop returns, about the release. Symbols (the arrows, the dot that is on or off) are
//! put on the buttons by the host that owns the language, not here.

#![allow(unsafe_code)] // defining an Objective-C subclass and sending to `super` has no safe API (SAFETY below)

use std::cell::RefCell;
use std::rc::Rc;

use burnburn_host_models::form::PadCell;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSAccessibility, NSBezelStyle, NSButton, NSEvent, NSView};
use objc2_foundation::NSString;

use crate::appkit::classes::ffi_guard;
use crate::appkit::panels::widgets::{push_button, rect};

/// Told about a press (`true`; answer whether it was taken) and a release (`false`; the answer is ignored).
pub(crate) type HoldHandler = Rc<dyn Fn(bool) -> bool>;

pub(crate) struct HoldIvars {
    handler: RefCell<Option<HoldHandler>>,
}

define_class!(
    #[unsafe(super(NSButton))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnHoldButton"]
    #[ivars = HoldIvars]
    pub(crate) struct HoldButton;

    impl HoldButton {
        /// Press: the handler may refuse (nothing is held then). The tracking loop of `super`
        /// returns at mouse-up, also when the pointer left the button, and the release follows in
        /// every case.
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            ffi_guard("hold mouseDown:", (), || {
                if !self.isEnabled() {
                    return;
                }
                let handler = self.ivars().handler.try_borrow().ok().and_then(|h| h.clone());
                let Some(handler) = handler else { return };
                if !handler(true) {
                    return;
                }
                // SAFETY: NSButton's own implementation with the same event, initialised instance.
                let () = unsafe { msg_send![super(self), mouseDown: event] };
                handler(false);
            });
        }
    }

    unsafe impl NSObjectProtocol for HoldButton {}
);

impl HoldButton {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(HoldIvars { handler: RefCell::new(None) });
        // SAFETY: NSButton's designated initialiser on a freshly allocated instance with ivars set.
        let b: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: rect(0.0, 0.0, 34.0, 28.0)] };
        b.setBezelStyle(NSBezelStyle::Push);
        b
    }

    pub(crate) fn set_handler(&self, handler: Option<HoldHandler>) {
        if let Ok(mut h) = self.ivars().handler.try_borrow_mut() {
            *h = handler;
        }
    }
}

/// Sets (or clears) the hold handler of `button` when it is a [`HoldButton`].
pub(crate) fn set_hold_handler(button: &NSButton, handler: Option<HoldHandler>) {
    let any: &AnyObject = button.as_ref();
    if let Some(h) = any.downcast_ref::<HoldButton>() {
        h.set_handler(handler);
    }
}

/// Reports a press or a release to the handler of a hold button as the tracking loop would; `false`
/// when `button` holds no handler (native tests drive the jog cross with it: a mouse-down would
/// wait for the mouse-up in AppKit's tracking loop).
pub(crate) fn fire_hold(button: &NSButton, press: bool) -> bool {
    let any: &AnyObject = button.as_ref();
    let handler = any
        .downcast_ref::<HoldButton>()
        .and_then(|h| h.ivars().handler.try_borrow().ok().and_then(|handler| handler.clone()));
    handler.is_some_and(|h| h(press))
}

const GAP: f64 = 4.0;
const HEIGHT: f64 = 28.0;
/// A button with a title needs more room than one with only a symbol.
const WIDE: f64 = 64.0;
const NARROW: f64 = 40.0;

/// The buttons of a pad and the view that holds them.
pub(crate) struct Pad {
    pub(crate) container: Retained<NSView>,
    pub(crate) buttons: Vec<Retained<NSButton>>,
}

/// Builds the pad of `cells`, `columns` to a line. `wire` puts the target and the action on a button
/// (it is not called for a pad that holds).
pub(crate) fn build_pad(
    mtm: MainThreadMarker,
    columns: u8,
    cells: &[PadCell],
    hold: bool,
    wire: &dyn Fn(&NSButton),
) -> Pad {
    let columns = usize::from(columns.max(1));
    let rows = cells.len().div_ceil(columns).max(1);
    let cell_w = cell_width(cells);
    let (width, height) = pad_size(columns as u8, cells);
    let container = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, width, height));
    let mut buttons = Vec::new();
    for (i, cell) in cells.iter().enumerate() {
        let b: Retained<NSButton> =
            if hold { Retained::into_super(HoldButton::new(mtm)) } else { push_button(mtm, &cell.title) };
        let (row, col) = (i / columns, i % columns);
        b.setFrame(rect(col as f64 * (cell_w + GAP), (rows - 1 - row) as f64 * (HEIGHT + GAP), cell_w, HEIGHT));
        if !hold {
            wire(&b);
        }
        container.addSubview(&b);
        buttons.push(b);
    }
    apply_cells(&buttons, cells, true);
    Pad { container, buttons }
}

/// The size a pad of these cells has.
pub(crate) fn pad_size(columns: u8, cells: &[PadCell]) -> (f64, f64) {
    let columns = usize::from(columns.max(1));
    let rows = cells.len().div_ceil(columns).max(1);
    (
        columns as f64 * cell_width(cells) + (columns as f64 - 1.0) * GAP,
        rows as f64 * HEIGHT + (rows as f64 - 1.0) * GAP,
    )
}

fn cell_width(cells: &[PadCell]) -> f64 {
    if cells.iter().any(|c| !c.title.is_empty()) { WIDE } else { NARROW }
}

/// Copies the cells into the buttons: title, name for the screen reader, tooltip, whether it acts
/// (`row_enabled` is the row's own flag) and whether it shows.
pub(crate) fn apply_cells(buttons: &[Retained<NSButton>], cells: &[PadCell], row_enabled: bool) {
    for (b, cell) in buttons.iter().zip(cells) {
        if b.title().to_string() != cell.title {
            b.setTitle(&NSString::from_str(&cell.title));
        }
        b.setAccessibilityLabel(Some(&NSString::from_str(&cell.label)));
        let tip = cell.tooltip.as_deref().map(NSString::from_str);
        b.setToolTip(tip.as_deref());
        b.setEnabled(row_enabled && cell.enabled && cell.visible);
        if b.isHidden() == cell.visible {
            b.setHidden(!cell.visible);
        }
    }
}
