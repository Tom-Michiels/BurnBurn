//! The Schikken panel (W3-UI-schikken, LF6c): a floating utility panel that draws the lines of
//! `crate::panels::schikken_panel` as a form (`crate::panels::schikken_form`, built by
//! `appkit::panels::form`): sections with a heading, captions on one baseline with their field, the
//! unit as a suffix, and the buttons that end a flow as the dialog buttons. It reports each use of
//! a control, with the panel's current values, as a `NativeEvent`. It decides nothing: what is
//! shown and what a control means is the headless model's. The form is updated in place while its
//! shape stays (never a field the user is typing in) and built again when the shape changes.

use std::cell::RefCell;
use std::rc::Weak;

use burnburn_host_models::form::FormId;
use burnburn_presentation::AppViewModel;
use burnburn_types::DocumentId;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSBackingStoreType, NSButton, NSControlStateValueOn, NSPanel, NSPopUpButton, NSTextField, NSView, NSWindow,
    NSWindowStyleMask,
};
use objc2_foundation::{NSObject, NSPoint, NSString};

use crate::adapter::NativeEvent;
use crate::appkit::app::HostApp;
use crate::appkit::classes::ffi_guard;
use crate::appkit::panels::form::{FormView, Wiring, build_form, install_form};
use crate::appkit::panels::widgets::rect;
use crate::appkit::window::DocWindow;
use crate::l10n::Locale;
use crate::panels::{SchikkenControl, schikken_form, schikken_panel};

const PANEL_WIDTH: f64 = 380.0;
/// The panel never grows beyond this; the form scrolls instead.
const MAX_HEIGHT: f64 = 640.0;

pub(crate) struct TargetIvars {
    app: Weak<HostApp>,
    document: DocumentId,
    /// The controls with their widgets, in panel order (the target reads their values).
    controls: RefCell<Vec<(SchikkenControl, Retained<NSView>)>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnSchikkenTarget"]
    #[ivars = TargetIvars]
    pub(crate) struct SchikkenTarget;

    impl SchikkenTarget {
        /// A field committed (Return or end of editing), a pop-up chosen, a checkbox toggled or a
        /// button clicked: the sender identifies the control.
        #[unsafe(method(schikkenControl:))]
        fn schikken_control(&self, sender: Option<&AnyObject>) {
            ffi_guard("schikkenControl:", (), || {
                let Some(sender) = sender else { return };
                let ivars = self.ivars();
                let (control, values) = {
                    let Ok(controls) = ivars.controls.try_borrow() else { return };
                    let Some((control, _)) = controls.iter().find(|(_, v)| {
                        let ptr: *const AnyObject = (&**v as *const NSView).cast();
                        std::ptr::eq(ptr, sender)
                    }) else {
                        return;
                    };
                    (*control, read_values(&controls))
                };
                if let Some(app) = ivars.app.upgrade() {
                    app.native(NativeEvent::SchikkenControl { document: ivars.document, control, values });
                }
            });
        }
    }
);

fn is_check(control: SchikkenControl) -> bool {
    matches!(
        control,
        SchikkenControl::PlaceByGroup
            | SchikkenControl::WarpBilinear
            | SchikkenControl::NestMirror
            | SchikkenControl::NestHoles
            | SchikkenControl::NestTight
            | SchikkenControl::NestCommonLine
    )
}

/// Every value-carrying control's value now.
fn read_values(controls: &[(SchikkenControl, Retained<NSView>)]) -> Vec<(SchikkenControl, String)> {
    controls
        .iter()
        .filter_map(|(control, view)| {
            let any: &AnyObject = view.as_ref();
            if let Some(f) = any.downcast_ref::<NSTextField>() {
                Some((*control, f.stringValue().to_string()))
            } else if let Some(p) = any.downcast_ref::<NSPopUpButton>() {
                Some((*control, p.indexOfSelectedItem().to_string()))
            } else if let Some(b) = any.downcast_ref::<NSButton>() {
                // A checkbox reports its state; a push button has no value.
                is_check(*control)
                    .then(|| (*control, if b.state() == NSControlStateValueOn { "1" } else { "0" }.to_owned()))
            } else {
                None
            }
        })
        .collect()
}

/// A panel drawn by the form builder, and which panel control each form id is.
struct FormState {
    view: FormView,
    size: objc2_foundation::NSSize,
    ids: Vec<(FormId, SchikkenControl)>,
}

/// The panel and its form.
pub(crate) struct SchikkenSection {
    target: Retained<SchikkenTarget>,
    panel: Retained<NSPanel>,
    form: Option<FormState>,
    /// Schikken ▸ Schikkenpaneel was chosen: the options panel shows even without a flow.
    pub(crate) requested: bool,
    pub(crate) shown: bool,
}

pub(crate) fn schikken_section(
    mtm: MainThreadMarker,
    app: Weak<HostApp>,
    document: DocumentId,
    locale: Locale,
) -> SchikkenSection {
    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(mtm),
        rect(0.0, 0.0, PANEL_WIDTH, 120.0),
        NSWindowStyleMask::Titled | NSWindowStyleMask::UtilityWindow | NSWindowStyleMask::Closable,
        NSBackingStoreType::Buffered,
        false,
    );
    // SAFETY: ownership stays with this Retained.
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setFloatingPanel(true);
    panel.setHidesOnDeactivate(true);
    panel.setTitle(&NSString::from_str(&crate::panels::schikken_text(crate::panels::SchikkenText::PanelTitle, locale)));
    let ivars = TargetIvars { app, document, controls: RefCell::new(Vec::new()) };
    let this = SchikkenTarget::alloc(mtm).set_ivars(ivars);
    // SAFETY: NSObject's designated initialiser on a freshly allocated instance with ivars set.
    let target: Retained<SchikkenTarget> = unsafe { msg_send![super(this), init] };
    SchikkenSection { target, panel, form: None, requested: false, shown: false }
}

impl SchikkenSection {
    /// Nil the targets (weak references) before the section goes: nothing can call a dead object.
    pub(crate) fn clear_targets(&self) {
        if let Some(form) = &self.form {
            form.view.clear_targets();
        }
        let _ = &self.target;
    }

    pub(crate) fn hide(&mut self) {
        if self.shown {
            self.shown = false;
            self.panel.orderOut(None);
        }
    }

    /// The visible widgets of the panel, in line order, for tests that drive them like a user.
    pub(crate) fn controls(&self) -> Vec<(SchikkenControl, Retained<NSView>)> {
        self.target.ivars().controls.try_borrow().map(|c| c.clone()).unwrap_or_default()
    }

    /// Every text the form shows (headings, captions, help, errors).
    pub(crate) fn labels(&self) -> Vec<String> {
        self.form.as_ref().map(|f| f.view.texts()).unwrap_or_default()
    }

    /// The control's widget as a plain view, if the panel has it now.
    pub(crate) fn control_view(&self, control: SchikkenControl) -> Option<Retained<NSView>> {
        self.controls().into_iter().find(|(c, _)| *c == control).map(|(_, v)| v)
    }

    /// Shows `lines` as a form: updates the widgets in place while its shape stays, builds a new
    /// form (keeping what the user typed) when it changes. Returns the size the panel needs.
    fn show_form(
        &mut self,
        mtm: MainThreadMarker,
        title: &str,
        lines: &[crate::panels::SchikkenLine],
    ) -> objc2_foundation::NSSize {
        let form = schikken_form(title, lines);
        if let Some(state) = &mut self.form
            && state.view.update(&form.spec)
        {
            state.ids = form.controls;
            // A text that wraps differently changes the height of the form.
            if (state.view.size().height - state.size.height).abs() > 0.5 && state.size.height <= MAX_HEIGHT {
                state.size = install_form(mtm, &self.panel, &state.view, MAX_HEIGHT);
            }
            return state.size;
        }
        let typed = self.form.as_ref().map(|s| (s.view.edits(), s.ids.clone())).unwrap_or_default();
        self.clear_targets();
        let any: &AnyObject = self.target.as_ref();
        let wiring = Wiring { target: any, action: sel!(schikkenControl:) };
        let mut view = build_form(mtm, &form.spec, PANEL_WIDTH, Some(wiring));
        // What the user typed survives a rebuild (a problem line appearing changes the shape of the
        // panel, not the field), together with what the model last put there.
        let (edits, old_ids) = typed;
        for (old_id, text, pushed) in edits {
            if let Some(control) = old_ids.iter().find(|(id, _)| *id == old_id).map(|(_, c)| *c)
                && let Some(new_id) = form.controls.iter().find(|(_, c)| *c == control).map(|(id, _)| *id)
            {
                view.restore(new_id, &text, &pushed);
            }
        }
        let controls: Vec<(SchikkenControl, Retained<NSView>)> = view
            .controls()
            .into_iter()
            .filter_map(|(id, v)| form.controls.iter().find(|(i, _)| *i == id).map(|(_, c)| (*c, v)))
            .collect();
        if let Ok(mut slot) = self.target.ivars().controls.try_borrow_mut() {
            *slot = controls;
        }
        let size = install_form(mtm, &self.panel, &view, MAX_HEIGHT);
        self.form = Some(FormState { view, size, ids: form.controls });
        size
    }
}

impl DocWindow {
    /// Shows, fills or hides the Schikken panel as the model says: a flow opens it by itself, the
    /// Schikkenpaneel menu item opens the options panel, and it only belongs to the active window.
    pub(crate) fn refresh_schikken(&mut self, vm: &AppViewModel, document: DocumentId) {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let model = schikken_panel(vm, document, self.locale);
        let wanted = vm.active == Some(document) && model.as_ref().is_some_and(|m| m.flow || self.schikken.requested);
        let Some(model) = model.filter(|_| wanted) else {
            self.schikken.hide();
            return;
        };
        let size = self.schikken.show_form(mtm, &model.title, &model.lines);
        let was_shown = self.schikken.shown;
        self.schikken.shown = true;
        let panel = self.schikken.panel.clone();
        let window_frame = self.window.frame();
        if let Err(e) = crate::appkit::objc_try("showing the Schikken panel", || {
            panel.setContentSize(size);
            if !was_shown {
                panel.setFrameTopLeftPoint(NSPoint::new(
                    window_frame.origin.x + window_frame.size.width - size.width - 24.0,
                    window_frame.origin.y + window_frame.size.height - 60.0,
                ));
                panel.orderFront(None);
            }
        }) {
            eprintln!("burnburn: {e}");
        }
    }

    /// Schikken ▸ Schikkenpaneel: asks for (or dismisses) the options panel.
    pub(crate) fn toggle_schikken(&mut self) -> bool {
        self.schikken.requested = !self.schikken.requested;
        self.schikken.requested
    }
}

#[allow(dead_code)]
fn _window_type(_: &NSWindow) {}
