//! One or two forms side by side in a panel or sheet (LF6b), and the window that holds them.
//!
//! The file, library, import, image and tracer panels keep their widgets in the struct of the
//! panel and copy the view model into them as before; what changed is who made the widgets and laid
//! them out: [`build_form`] from a `FormSpec` of `burnburn_host_models::panels::files_form`. This
//! module finds the widgets of a built form by their row id, hides the rows a file type does not
//! have, and fits the window to the form after a text grew or a row came or went.

use burnburn_host_models::form::{ButtonRole, FormId, FormSpec};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSButton, NSImageView, NSPanel, NSPopUpButton, NSTextField, NSTextView, NSView, NSWindow,
    NSWindowStyleMask,
};
use objc2_foundation::{NSRect, NSSize, NSString};

use super::form::{FormView, build_form};
use super::widgets::rect;

/// Forms side by side with the widgets of their rows.
pub(crate) struct FormSurface {
    mtm: MainThreadMarker,
    forms: Vec<FormView>,
    root: Retained<NSView>,
}

impl FormSurface {
    /// Builds each `(spec, width)` as a form and puts them side by side, left to right.
    pub(crate) fn new(mtm: MainThreadMarker, specs: &[(&FormSpec, f64)]) -> Self {
        let forms: Vec<FormView> = specs.iter().map(|(spec, width)| build_form(mtm, spec, *width, None)).collect();
        let root = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 0.0, 0.0));
        for form in &forms {
            root.addSubview(form.root());
        }
        let mut surface = FormSurface { mtm, forms, root };
        surface.relayout();
        surface
    }

    /// The view to make the content view of the window.
    pub(crate) fn root(&self) -> &NSView {
        &self.root
    }

    pub(crate) fn size(&self) -> NSSize {
        self.root.frame().size
    }

    /// Fits every form to its content, gives them all the height of the tallest (the buttons stay
    /// at the bottom) and sizes the root to the forms side by side.
    pub(crate) fn relayout(&mut self) -> NSSize {
        let natural = self.forms.iter_mut().map(|f| f.relayout(0.0).height).fold(0.0_f64, f64::max);
        let mut x = 0.0;
        for form in &mut self.forms {
            let size = form.relayout(natural);
            form.root().setFrameOrigin(objc2_foundation::NSPoint::new(x, 0.0));
            x += size.width;
        }
        let size = NSSize::new(x, natural);
        self.root.setFrame(rect(0.0, 0.0, size.width, size.height));
        size
    }

    /// Copies `spec` into form `form` without touching what the user is typing; `false` when the
    /// shape differs.
    pub(crate) fn update(&mut self, form: usize, spec: &FormSpec) -> bool {
        self.forms.get_mut(form).is_some_and(|f| f.update(spec))
    }

    /// Copies `spec` into form `form`, also over what was typed (a dialog that opens again).
    pub(crate) fn load(&mut self, form: usize, spec: &FormSpec) -> bool {
        self.forms.get_mut(form).is_some_and(|f| f.load(spec))
    }

    pub(crate) fn set_hidden(&mut self, id: u32, hidden: bool) {
        for form in &mut self.forms {
            if form.control(FormId(id)).is_some() {
                form.set_hidden(FormId(id), hidden);
            }
        }
    }

    pub(crate) fn set_section_hidden(&mut self, form: usize, section: usize, hidden: bool) {
        if let Some(f) = self.forms.get_mut(form) {
            f.set_section_hidden(section, hidden);
        }
    }

    fn control(&self, id: u32) -> Option<Retained<NSView>> {
        self.forms.iter().find_map(|f| f.control(FormId(id)))
    }

    /// The caption of a row (a detached label when the row has none, so a caller always has one).
    pub(crate) fn caption(&self, id: u32) -> Retained<NSTextField> {
        self.forms
            .iter()
            .find_map(|f| f.caption(FormId(id)))
            .unwrap_or_else(|| NSTextField::labelWithString(&NSString::from_str(""), self.mtm))
    }

    /// A text field, or the text of a note (a detached one when the id is not a text row).
    pub(crate) fn text(&self, id: u32) -> Retained<NSTextField> {
        self.control(id)
            .and_then(|v| v.downcast::<NSTextField>().ok())
            .unwrap_or_else(|| NSTextField::labelWithString(&NSString::from_str(""), self.mtm))
    }

    pub(crate) fn popup(&self, id: u32) -> Retained<NSPopUpButton> {
        self.control(id).and_then(|v| v.downcast::<NSPopUpButton>().ok()).unwrap_or_else(|| {
            NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(self.mtm), rect(0.0, 0.0, 0.0, 0.0), false)
        })
    }

    /// A check box or a button of a row of buttons.
    pub(crate) fn button(&self, id: u32) -> Retained<NSButton> {
        self.control(id)
            .and_then(|v| v.downcast::<NSButton>().ok())
            .unwrap_or_else(|| super::widgets::push_button(self.mtm, ""))
    }

    /// The dialog button of this role, in whichever of the forms has it.
    pub(crate) fn dialog_button(&self, role: ButtonRole) -> Retained<NSButton> {
        self.forms
            .iter()
            .flat_map(|f| f.buttons())
            .find(|(r, _, _)| *r == role)
            .map(|(_, _, b)| b)
            .unwrap_or_else(|| super::widgets::push_button(self.mtm, ""))
    }

    /// A report: its text view.
    pub(crate) fn report(&self, id: u32) -> Retained<NSTextView> {
        self.control(id)
            .and_then(|v| v.downcast::<NSTextView>().ok())
            .unwrap_or_else(|| NSTextView::initWithFrame(NSTextView::alloc(self.mtm), rect(0.0, 0.0, 0.0, 0.0)))
    }

    /// A picture row: its image view.
    pub(crate) fn image(&self, id: u32) -> Retained<NSImageView> {
        self.control(id)
            .and_then(|v| v.downcast::<NSImageView>().ok())
            .unwrap_or_else(|| NSImageView::initWithFrame(NSImageView::alloc(self.mtm), rect(0.0, 0.0, 0.0, 0.0)))
    }

    /// Fits `window` to the form: the content size changes and the top left corner stays where it
    /// is. Does nothing while the size is already right.
    pub(crate) fn fit_window(&mut self, window: &NSWindow) {
        let size = self.relayout();
        resize_content(window, size);
    }
}

/// Sets the content size of `window` keeping its top left corner.
pub(crate) fn resize_content(window: &NSWindow, size: NSSize) {
    let frame = window.frame();
    let content = window.contentRectForFrameRect(frame);
    if (content.size.width - size.width).abs() < 0.5 && (content.size.height - size.height).abs() < 0.5 {
        return;
    }
    let (dw, dh) = (size.width - content.size.width, size.height - content.size.height);
    let new = NSRect::new(
        objc2_foundation::NSPoint::new(frame.origin.x, frame.origin.y - dh),
        NSSize::new(frame.size.width + dw, frame.size.height + dh),
    );
    window.setFrame_display(new, true);
}

/// A titled sheet window for `surface`.
#[allow(unsafe_code)]
pub(crate) fn form_sheet(mtm: MainThreadMarker, surface: &FormSurface, title: &str) -> Retained<NSWindow> {
    let size = surface.size();
    // SAFETY: a freshly allocated window with a valid rect, style and the buffered backing.
    let sheet = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            rect(0.0, 0.0, size.width, size.height),
            NSWindowStyleMask::Titled,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: ownership stays with the caller's Retained.
    unsafe { sheet.setReleasedWhenClosed(false) };
    sheet.setTitle(&NSString::from_str(title));
    sheet.setContentView(Some(surface.root()));
    sheet
}

/// A floating utility panel for `surface`.
#[allow(unsafe_code)]
pub(crate) fn form_panel(mtm: MainThreadMarker, surface: &FormSurface, title: &str) -> Retained<NSPanel> {
    let size = surface.size();
    let style = NSWindowStyleMask::Titled | NSWindowStyleMask::UtilityWindow;
    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(mtm),
        rect(0.0, 0.0, size.width, size.height),
        style,
        NSBackingStoreType::Buffered,
        false,
    );
    // SAFETY: ownership stays with the caller's Retained; AppKit must not also release on close.
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setFloatingPanel(true);
    panel.setHidesOnDeactivate(true);
    panel.setTitle(&NSString::from_str(title));
    panel.setContentView(Some(surface.root()));
    panel
}

/// Points a control at `target` with `action`, as the window controller's buttons are wired.
#[allow(unsafe_code)]
pub(crate) fn point_at(control: &objc2_app_kit::NSControl, target: &AnyObject, action: objc2::runtime::Sel) {
    // SAFETY: the target is weak; the owner of the form keeps it and clears the targets before it
    // goes. `action` is one of the target's methods taking the sender (`v@:@`).
    unsafe { control.setTarget(Some(target)) };
    // SAFETY: as above.
    unsafe { control.setAction(Some(action)) };
}
