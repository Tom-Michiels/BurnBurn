//! The cells of the layer table (LFC): the table is view-based. The layer column shows a 12 pt circle
//! drawn in the layer's colour followed by the layer's name (no coloured cell any more), the
//! editable columns hold borderless text fields and Output / Show are check boxes. A cell only
//! shows what the view model says; its action is forwarded to the window controller, which turns it
//! into the same `NativeEvent` the cell edits of the old table sent.

use std::cell::RefCell;

use objc2::rc::{Retained, Weak};
use objc2::runtime::{AnyObject, NSObjectProtocol};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAccessibility, NSAutoresizingMaskOptions, NSBezierPath, NSButton, NSColor, NSControlStateValueOff,
    NSControlStateValueOn, NSControlTextEditingDelegate, NSFont, NSTableCellView, NSTableColumn, NSTableView,
    NSTableViewDelegate, NSTextField, NSView,
};
use objc2_foundation::{NSObject, NSRect, NSString};

use crate::appkit::classes::{WindowController, ffi_guard};
use crate::appkit::panels::operations::layer_colour;
use crate::appkit::panels::widgets::rect;
use crate::panels::{LayerColumn, OperationLine};

/// The diameter of the circle in the layer column.
pub(crate) const DOT: f64 = 12.0;
/// The space left of the circle, and between the circle and the name.
const PAD: f64 = 4.0;
/// The width of a check box.
const CHECK: f64 = 18.0;
/// The height of a row of the layer table.
pub(crate) const ROW_HEIGHT: f64 = 22.0;

pub(crate) struct DotIvars {
    colour: Retained<NSColor>,
}

define_class!(
    /// A circle in one colour; the whole view is the circle.
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnLayerDot"]
    #[ivars = DotIvars]
    pub(crate) struct LayerDot;

    impl LayerDot {
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            ffi_guard("drawRect:", (), || {
                self.ivars().colour.setFill();
                NSBezierPath::bezierPathWithOvalInRect(self.bounds()).fill();
            });
        }
    }
);

impl LayerDot {
    #[allow(unsafe_code)]
    pub(crate) fn new(mtm: MainThreadMarker, colour: Retained<NSColor>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(DotIvars { colour });
        // SAFETY: NSView's designated initialiser on a freshly allocated instance with ivars set.
        unsafe { msg_send![super(this), initWithFrame: rect(0.0, 0.0, DOT, DOT)] }
    }
}

pub(crate) struct DelegateIvars {
    controller: RefCell<Option<Weak<WindowController>>>,
}

define_class!(
    /// The delegate of the layer table: it makes the cell of every column.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnLayerTableDelegate"]
    #[ivars = DelegateIvars]
    pub(crate) struct LayerTableDelegate;

    unsafe impl NSObjectProtocol for LayerTableDelegate {}

    unsafe impl NSControlTextEditingDelegate for LayerTableDelegate {}

    unsafe impl NSTableViewDelegate for LayerTableDelegate {
        #[unsafe(method_id(tableView:viewForTableColumn:row:))]
        fn view_for_column(
            &self,
            _table: &NSTableView,
            column: Option<&NSTableColumn>,
            row: isize,
        ) -> Option<Retained<NSView>> {
            ffi_guard("tableView:viewForTableColumn:row:", None, || {
                let mtm = MainThreadMarker::new()?;
                let table_column = column?;
                let column = LayerColumn::from_id(&table_column.identifier().to_string())?;
                let title = table_column.title().to_string();
                let controller = self.ivars().controller.try_borrow().ok()?.as_ref()?.load()?;
                let line = controller.operation_line(usize::try_from(row).ok()?)?;
                Some(layer_cell(mtm, &line, column, &title, table_column.width(), &controller))
            })
        }
    }
);

impl LayerTableDelegate {
    #[allow(unsafe_code)]
    pub(crate) fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(DelegateIvars { controller: RefCell::new(None) });
        // SAFETY: NSObject's designated initialiser on a freshly allocated instance with ivars set.
        unsafe { msg_send![super(this), init] }
    }

    /// The controller the cells report to (weak: the window owns both).
    pub(crate) fn attach(&self, controller: &WindowController) {
        if let Ok(mut slot) = self.ivars().controller.try_borrow_mut() {
            *slot = Some(Weak::from_retained(&controller.retain()));
        }
    }
}

use objc2::Message;

/// The view of one cell of the layer table.
#[allow(unsafe_code)]
pub(crate) fn layer_cell(
    mtm: MainThreadMarker,
    line: &OperationLine,
    column: LayerColumn,
    title: &str,
    width: f64,
    target: &WindowController,
) -> Retained<NSView> {
    let cell = NSTableCellView::initWithFrame(NSTableCellView::alloc(mtm), rect(0.0, 0.0, width, ROW_HEIGHT));
    let any: &AnyObject = target.as_ref();
    if let Some(on) = line.checked(column) {
        // SAFETY: the target is weak and outlives the cell (the window owns the controller and
        // the table); `layerToggled:` takes the sender (`v@:@`).
        let check = unsafe {
            NSButton::checkboxWithTitle_target_action(
                &NSString::from_str(""),
                Some(any),
                Some(sel!(layerToggled:)),
                mtm,
            )
        };
        check.setState(if on { NSControlStateValueOn } else { NSControlStateValueOff });
        check.setFrame(rect(((width - CHECK) / 2.0).max(0.0), (ROW_HEIGHT - CHECK) / 2.0, CHECK, CHECK));
        check.setAccessibilityLabel(Some(&NSString::from_str(&format!("{title} {}", line.layer))));
        cell.addSubview(&check);
        return Retained::into_super(cell);
    }
    let editable = column.field().is_some();
    let text_field = NSTextField::labelWithString(&NSString::from_str(&line.text(column)), mtm);
    text_field.setFont(Some(&NSFont::systemFontOfSize(NSFont::smallSystemFontSize())));
    let mut x = 0.0;
    if column == LayerColumn::Layer {
        let dot = LayerDot::new(mtm, layer_colour(line.slot));
        dot.setFrame(rect(PAD, (ROW_HEIGHT - DOT) / 2.0, DOT, DOT));
        cell.addSubview(&dot);
        x = PAD + DOT + PAD;
    }
    text_field.setFrame(rect(x, (ROW_HEIGHT - 17.0) / 2.0, (width - x).max(0.0), 17.0));
    text_field.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
    if editable {
        text_field.setEditable(true);
        text_field.setSelectable(true);
        if let Some(c) = text_field.cell() {
            c.setSendsActionOnEndEditing(true);
        }
        // SAFETY: as above; `layerFieldEdited:` takes the sender (`v@:@`).
        unsafe { text_field.setTarget(Some(any)) };
        // SAFETY: as above.
        unsafe { text_field.setAction(Some(sel!(layerFieldEdited:))) };
        text_field.setAccessibilityLabel(Some(&NSString::from_str(&format!("{title} {}", line.layer))));
    }
    cell.addSubview(&text_field);
    // SAFETY: the field is a subview of the cell; the cell only uses it for accessibility and the
    // selection colour.
    unsafe { cell.setTextField(Some(&text_field)) };
    Retained::into_super(cell)
}
