//! One generic panel for the CAM dialogs (W3-UI-cam, LF6a): Optimization Settings, Material /
//! Interval / Focus Test, Device Settings, Array, Feeder, Preflight, Cutting order and Sub-layers.
//! A floating utility panel that holds the form the LF5 builder makes of the dialog's
//! [`DialogViewModel`] (`panels::cam_form`): sections with a heading, the unit as a suffix, the
//! reason a field is greyed once per section, Close (Escape) and Apply or Generate (Return) as the
//! dialog buttons. Every refresh copies the view model into the widgets in place; the panel builds
//! the form again only when the dialog gets other rows. The widgets only forward what the user did
//! — a changed field, a pressed button — to the host, which turns it into an intent
//! (`crate::panels::cam_*_intent`).
//!
//! The panel has no close button: Close is a button of the dialog, so presentation always knows
//! whether the draft exists.

use std::rc::Weak;

use burnburn_document::process::OrderKey;
use burnburn_host_models::form::{FormId, FormSpec};
use burnburn_presentation::{CamDialog, DialogActionKind, DialogField, DialogValue, DialogViewModel};
use burnburn_types::DocumentId;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAccessibility, NSBackingStoreType, NSButton, NSControl, NSControlStateValueOn, NSPanel, NSPopUpButton,
    NSTextField, NSView, NSWindowStyleMask,
};
use objc2_foundation::{NSObject, NSString};

use crate::appkit::app::HostApp;
use crate::appkit::classes::ffi_guard;
use crate::appkit::panels::form::{FormLayout, FormView, build_form_with};
use crate::appkit::panels::widgets::rect;
use crate::appkit::symbols::{self, Symbol};
use crate::l10n::Locale;
use crate::panels::{CamForm, CamTarget, arrow_label, cam_form, cam_title};

/// The width of a dialog.
const WIDTH: f64 = 560.0;
/// The tallest a dialog gets; the rows scroll above the button row beyond it.
const MAX_HEIGHT: f64 = 640.0;
const ORDER_KEYS: [OrderKey; 3] = [OrderKey::Layer, OrderKey::Group, OrderKey::Priority];
/// Buttons in the order of the bar, left to right; the tag of a button is its place here.
const ACTION_KINDS: [DialogActionKind; 10] = [
    DialogActionKind::ShowSource,
    DialogActionKind::AddRow,
    DialogActionKind::RemoveRow,
    DialogActionKind::MoveRowUp,
    DialogActionKind::MoveRowDown,
    DialogActionKind::ReturnToFlatbed,
    DialogActionKind::Generate,
    DialogActionKind::ExportRd,
    DialogActionKind::Close,
    DialogActionKind::Apply,
];
/// Tags of the order buttons: `UP_TAG + index in ORDER_KEYS`, `DOWN_TAG + …`.
const UP_TAG: isize = 10;
const DOWN_TAG: isize = 20;

impl RowUi {
    /// The control as a plain `NSControl` (tests set its value and send its action).
    pub(crate) fn control_ref(&self) -> &NSControl {
        &self.control
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
    pub(crate) control: Retained<NSControl>,
    /// The error of the field (empty when there is none).
    pub(crate) note: Retained<NSTextField>,
    up: Option<Retained<NSButton>>,
    down: Option<Retained<NSButton>>,
}

pub(crate) struct TargetIvars {
    app: Weak<HostApp>,
    document: DocumentId,
    dialog: CamDialog,
    /// The field of each control, by its tag.
    fields: std::cell::RefCell<Vec<DialogField>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnCamTarget"]
    #[ivars = TargetIvars]
    pub(crate) struct CamActions;

    impl CamActions {
        /// A field changed: Return or end of editing of a text field, a tick, a pop-up choice.
        #[unsafe(method(camField:))]
        fn cam_field(&self, sender: Option<&AnyObject>) {
            ffi_guard("camField:", (), || {
                let Some(control) = sender.and_then(|s| s.downcast_ref::<NSControl>()) else { return };
                let ivars = self.ivars();
                let field = {
                    let Ok(fields) = ivars.fields.try_borrow() else { return };
                    let Some(field) = usize::try_from(control.tag()).ok().and_then(|i| fields.get(i)) else { return };
                    *field
                };
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
                    app.cam_field_changed(ivars.document, ivars.dialog, field, value);
                }
            });
        }

        /// A button of the dialog.
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
    mtm: MainThreadMarker,
    target: Retained<CamActions>,
    pub(crate) panel: Retained<NSPanel>,
    view: FormView,
    form: CamForm,
    pub(crate) rows: Vec<RowUi>,
    /// The notes, the generated test and the capability list (empty hidden labels where the dialog
    /// has none).
    pub(crate) notes: Retained<NSTextField>,
    pub(crate) result: Retained<NSTextField>,
    pub(crate) capabilities: Retained<NSTextField>,
    /// Why fields are greyed: said once per dialog, in the section it applies to.
    pub(crate) reasons: String,
    pub(crate) buttons: Vec<(DialogActionKind, Retained<NSButton>)>,
}

#[allow(unsafe_code)]
fn wire(control: &NSControl, target: &AnyObject, action: objc2::runtime::Sel, tag: isize) {
    control.setTag(tag);
    // SAFETY: the target is weak; the panel keeps it and clears it in `clear_targets`.
    unsafe { control.setTarget(Some(target)) };
    // SAFETY: every action takes the sender (`v@:@`).
    unsafe { control.setAction(Some(action)) };
}

fn text_of(view: Option<Retained<NSView>>, mtm: MainThreadMarker) -> Retained<NSTextField> {
    view.and_then(|v| v.downcast::<NSTextField>().ok()).unwrap_or_else(|| {
        let l = NSTextField::labelWithString(&NSString::from_str(""), mtm);
        l.setHidden(true);
        l
    })
}

fn as_control(view: Option<Retained<NSView>>) -> Option<Retained<NSControl>> {
    view.and_then(|v| v.downcast::<NSControl>().ok())
}

fn find_id(form: &CamForm, wanted: CamTarget) -> Option<FormId> {
    form.targets.iter().find(|(_, t)| *t == wanted).map(|(id, _)| *id)
}

impl CamPanelUi {
    /// Builds the panel for the rows `vm` has now.
    pub(crate) fn new(
        mtm: MainThreadMarker,
        app: Weak<HostApp>,
        document: DocumentId,
        vm: &DialogViewModel,
        locale: Locale,
    ) -> CamPanelUi {
        let dialog = vm.dialog;
        let this = CamActions::alloc(mtm).set_ivars(TargetIvars {
            app,
            document,
            dialog,
            fields: std::cell::RefCell::new(Vec::new()),
        });
        // SAFETY: NSObject's designated initialiser on a freshly allocated instance with ivars set.
        let target: Retained<CamActions> = unsafe { msg_send![super(this), init] };
        let form = cam_form(vm, locale);
        let view = build_form_with(mtm, &form.spec, WIDTH, None, FormLayout::Capped(MAX_HEIGHT));
        let size = view.size();
        let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
            NSPanel::alloc(mtm),
            rect(0.0, 0.0, size.width, size.height),
            NSWindowStyleMask::Titled | NSWindowStyleMask::UtilityWindow,
            NSBackingStoreType::Buffered,
            false,
        );
        // SAFETY: ownership stays with this Retained.
        #[allow(unsafe_code)]
        unsafe {
            panel.setReleasedWhenClosed(false)
        };
        panel.setFloatingPanel(true);
        panel.setHidesOnDeactivate(false);
        panel.setTitle(&NSString::from_str(cam_title(dialog, locale)));
        panel.setContentView(Some(view.root()));
        panel.center();
        let blank = |mtm| text_of(None, mtm);
        let mut ui = CamPanelUi {
            mtm,
            target,
            panel,
            view,
            form,
            rows: Vec::new(),
            notes: blank(mtm),
            result: blank(mtm),
            capabilities: blank(mtm),
            reasons: String::new(),
            buttons: Vec::new(),
        };
        ui.attach(locale);
        ui
    }

    /// Wires the controls of the form to the target and collects the widgets the host and the tests
    /// reach by field or button.
    fn attach(&mut self, locale: Locale) {
        let any: &AnyObject = self.target.as_ref();
        let mtm = self.mtm;
        let mut fields: Vec<DialogField> = Vec::new();
        let mut rows: Vec<RowUi> = Vec::new();
        let mut buttons: Vec<(DialogActionKind, Retained<NSButton>)> = Vec::new();
        let dialog_buttons = self.view.buttons();
        for (id, target) in self.form.targets.clone() {
            match target {
                CamTarget::Field(field) => {
                    let Some(control) = as_control(self.view.control(id)) else { continue };
                    wire(&control, any, sel!(camField:), fields.len() as isize);
                    fields.push(field);
                    let note = self.view.error_label(id).unwrap_or_else(|| text_of(None, mtm));
                    let (mut up, mut down) = (None, None);
                    if let DialogField::OrderEnabled(key) = field
                        && let Some(k) = ORDER_KEYS.iter().position(|o| *o == key)
                    {
                        for (is_up, slot) in [(true, &mut up), (false, &mut down)] {
                            let tool = find_id(&self.form, CamTarget::Move(key, is_up)).and_then(|t| self.view.tool(t));
                            if let Some(b) = tool {
                                let (base, symbol) =
                                    if is_up { (UP_TAG, Symbol::OrderUp) } else { (DOWN_TAG, Symbol::OrderDown) };
                                symbols::apply(&b, symbol, locale);
                                b.setAccessibilityLabel(Some(&NSString::from_str(&arrow_label(key, is_up, locale))));
                                wire(&b, any, sel!(camMove:), base + k as isize);
                                *slot = Some(b);
                            }
                        }
                    }
                    rows.push(RowUi { field, control, note, up, down });
                }
                CamTarget::Action(kind) => {
                    let slot = ACTION_KINDS.iter().position(|k| *k == kind).unwrap_or(0) as isize;
                    let button = dialog_buttons
                        .iter()
                        .find(|(_, bid, _)| *bid == id)
                        .map(|(_, _, b)| b.clone())
                        .or_else(|| self.view.control(id).and_then(|v| v.downcast::<NSButton>().ok()));
                    if let Some(b) = button {
                        wire(&b, any, sel!(camAction:), slot);
                        buttons.push((kind, b));
                    }
                }
                CamTarget::Move(..) | CamTarget::Notes | CamTarget::Result | CamTarget::Capabilities => {}
            }
        }
        if let Ok(mut slot) = self.target.ivars().fields.try_borrow_mut() {
            *slot = fields;
        }
        self.rows = rows;
        self.buttons = buttons;
        self.notes = text_of(find_id(&self.form, CamTarget::Notes).and_then(|i| self.view.control(i)), mtm);
        self.result = text_of(find_id(&self.form, CamTarget::Result).and_then(|i| self.view.control(i)), mtm);
        self.capabilities =
            text_of(find_id(&self.form, CamTarget::Capabilities).and_then(|i| self.view.control(i)), mtm);
        self.reasons = self.form.reasons.clone();
    }

    /// Copies the view model into the widgets: values, enablement, reasons, texts, buttons.
    pub(crate) fn refresh(&mut self, vm: &DialogViewModel, locale: Locale) {
        let form = cam_form(vm, locale);
        if self.view.update(&form.spec) {
            self.form = form;
            self.reasons = self.form.reasons.clone();
        } else {
            self.rebuild(form, locale);
        }
        let size = self.view.size();
        let now = self.panel.contentView().map(|c| c.frame().size);
        if now.is_none_or(|n| (n.width - size.width).abs() > 0.5 || (n.height - size.height).abs() > 0.5) {
            self.panel.setContentSize(size);
        }
    }

    /// The dialog got other rows: build the form again, keeping what the user typed.
    fn rebuild(&mut self, form: CamForm, locale: Locale) {
        let typed: Vec<(FormId, String, String)> = self.view.edits();
        let old_targets = self.form.targets.clone();
        self.view.clear_targets();
        let spec: &FormSpec = &form.spec;
        let mut view = build_form_with(self.mtm, spec, WIDTH, None, FormLayout::Capped(MAX_HEIGHT));
        for (old_id, text, pushed) in typed {
            let target = old_targets.iter().find(|(id, _)| *id == old_id).map(|(_, t)| *t);
            if let Some(target) = target
                && let Some(new_id) = find_id(&form, target)
            {
                view.restore(new_id, &text, &pushed);
            }
        }
        self.panel.setContentView(Some(view.root()));
        self.view = view;
        self.form = form;
        self.attach(locale);
    }

    /// Nil the targets (weak references) before the panel goes.
    pub(crate) fn clear_targets(&self) {
        self.view.clear_targets();
    }

    pub(crate) fn show(&self) {
        self.view.scroll_to_top();
        self.panel.makeKeyAndOrderFront(None);
    }

    pub(crate) fn hide(&self) {
        self.panel.orderOut(None);
    }
}
