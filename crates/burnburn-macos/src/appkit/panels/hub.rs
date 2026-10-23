//! The machine hub window (G5-machine-rest, LF6c): a floating utility window with a tab per
//! subject (device, production, history, calibration), built by the form builder from
//! `crate::hub_form::hub_form`. It copies `crate::hub_panel::HubPanel` into the form and reports
//! each use of a control as a tag (the form id); the host turns the tag into a `HubEvent`. It
//! decides nothing.

use std::rc::Weak;

use burnburn_host_models::form::FormId;
use burnburn_host_models::hub_form::{ID_DEVICE, ID_FIELD_FIRST, ID_HISTORY, ID_KIND, ID_PROOFS, hub_form};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSBackingStoreType, NSControl, NSFloatingWindowLevel, NSWindow, NSWindowStyleMask};
use objc2_foundation::{NSObject, NSString};

use crate::appkit::app::HostApp;
use crate::appkit::classes::ffi_guard;
use crate::appkit::panels::form::{FormView, Wiring, build_form};
use crate::appkit::panels::widgets::rect;
use crate::hub_panel::{HubPanel, ProofForm};
use crate::l10n::Locale;

pub(crate) const TAG_DEVICE: isize = 2;
pub(crate) const TAG_STOP: isize = 3;
pub(crate) const TAG_REPEAT: isize = 4;
pub(crate) const TAG_CONFIRM: isize = 5;
pub(crate) const TAG_CANCEL: isize = 6;
pub(crate) const TAG_RECORD: isize = 7;
pub(crate) const TAG_REMOVE: isize = 8;

pub(crate) struct HubIvars {
    app: Weak<HostApp>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnHubTarget"]
    #[ivars = HubIvars]
    pub(crate) struct HubTarget;

    impl HubTarget {
        /// Any control of the hub window; its tag says which.
        #[unsafe(method(hubAction:))]
        fn hub_action(&self, sender: Option<&AnyObject>) {
            ffi_guard("hubAction:", (), || {
                let Some(control) = sender.and_then(|s| s.downcast_ref::<NSControl>()) else { return };
                if let Some(app) = self.ivars().app.upgrade() {
                    app.hub_action(control.tag());
                }
            });
        }
    }
);

impl HubTarget {
    fn new(mtm: MainThreadMarker, app: Weak<HostApp>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(HubIvars { app });
        // SAFETY: NSObject's designated initialiser on a freshly allocated instance with ivars set.
        unsafe { msg_send![super(this), init] }
    }
}

pub(crate) struct HubWindow {
    pub(crate) window: Retained<NSWindow>,
    target: Retained<HubTarget>,
    mtm: MainThreadMarker,
    locale: Locale,
    view: FormView,
}

const W: f64 = 680.0;

/// Builds the form and puts the form id on every control as its tag (what `hubAction:` reports).
fn build(mtm: MainThreadMarker, target: &HubTarget, panel: &HubPanel, locale: Locale) -> FormView {
    let t: &AnyObject = target;
    let view = build_form(mtm, &hub_form(panel, locale), W, Some(Wiring { target: t, action: sel!(hubAction:) }));
    for (id, control) in view.controls() {
        let any: &AnyObject = control.as_ref();
        if let Some(c) = any.downcast_ref::<NSControl>() {
            c.setTag(id.0 as isize);
        }
    }
    view
}

pub(crate) fn build_hub(mtm: MainThreadMarker, app: Weak<HostApp>, locale: Locale, float: bool) -> HubWindow {
    let target = HubTarget::new(mtm, app);
    let panel = HubPanel::default();
    let view = build(mtm, &target, &panel, locale);
    let style = NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Miniaturizable;
    let size = view.size();
    // SAFETY: a freshly allocated window with a valid rect, style and the buffered backing.
    #[allow(unsafe_code)]
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            rect(0.0, 0.0, size.width, size.height),
            style,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: ownership stays with the caller's Retained.
    #[allow(unsafe_code)]
    unsafe {
        window.setReleasedWhenClosed(false)
    };
    window.setTitle(&NSString::from_str(&hub_form(&panel, locale).title));
    if float {
        window.setLevel(NSFloatingWindowLevel);
    }
    window.setContentView(Some(view.root()));
    window.center();
    HubWindow { window, target, mtm, locale, view }
}

impl HubWindow {
    fn value(&self, id: u32) -> Option<String> {
        self.view.values().into_iter().find(|(i, _)| i.0 == id).map(|(_, v)| v)
    }

    fn selected(&self, id: u32) -> Option<usize> {
        self.value(id)?.parse().ok()
    }

    pub(crate) fn selected_device(&self) -> Option<usize> {
        self.selected(ID_DEVICE)
    }

    pub(crate) fn selected_history(&self) -> Option<usize> {
        self.selected(ID_HISTORY)
    }

    pub(crate) fn selected_proof(&self) -> Option<usize> {
        self.selected(ID_PROOFS)
    }

    /// The index of the open tab.
    pub(crate) fn section_selected(&self) -> isize {
        self.view
            .tab_view()
            .and_then(|t| t.selectedTabViewItem().and_then(|i| t.indexOfTabViewItem(&i).into()))
            .unwrap_or(0)
    }

    /// The form as typed.
    pub(crate) fn form(&self) -> ProofForm {
        let s = |i: u32| self.value(ID_FIELD_FIRST + i).unwrap_or_default();
        ProofForm {
            kind: self.selected(ID_KIND).unwrap_or(usize::MAX),
            material: s(0),
            speed: s(1),
            value: s(2),
            value_y: s(3),
            method: s(4),
            date: s(5),
        }
    }

    /// Copies the panel into the form in place; a change of shape (a problem appearing) builds the
    /// form again and keeps what the user typed, chose and the tab that was open.
    pub(crate) fn apply(&mut self, p: &HubPanel, locale: Locale) {
        self.locale = locale;
        let spec = hub_form(p, locale);
        if self.view.update(&spec) {
            let size = self.view.size();
            if (self.window.contentView().map_or(0.0, |v| v.frame().size.height) - size.height).abs() > 0.5 {
                self.window.setContentSize(size);
            }
            return;
        }
        let edits = self.view.edits();
        let choices: Vec<(FormId, String)> = self
            .view
            .values()
            .into_iter()
            .filter(|(id, _)| matches!(id.0, ID_DEVICE | ID_HISTORY | ID_PROOFS | ID_KIND))
            .collect();
        let tab = self.section_selected();
        self.view.clear_targets();
        let mut view = build(self.mtm, &self.target, p, locale);
        for (id, typed, pushed) in edits {
            view.restore(id, &typed, &pushed);
        }
        for (id, index) in choices {
            if let Ok(i) = index.parse::<usize>() {
                view.restore_choice(id, i);
            }
        }
        if let Some(tabs) = view.tab_view() {
            tabs.selectTabViewItemAtIndex(tab);
        }
        self.window.setContentView(Some(view.root()));
        self.view = view;
    }
}
