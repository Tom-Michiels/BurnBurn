//! The Cut Settings sheet of one layer (LF6a): the form the LF5 builder makes of
//! `panels::cut_form` — the layer's mode, numbers and switches under its name, and below them the
//! option rows of the chosen mode in a scrolling area, with Cancel (Escape) and OK (Return) in the
//! button row. The form is built once, with a row for every option field of every mode; a refresh
//! shows the rows of the chosen mode, greys what is off and fills the notes, so the widgets the
//! window controller reads on OK stay the same for the life of the window.

use crate::appkit::panels::form::{FormLayout, FormView, build_form_with};
use crate::appkit::panels::widgets::rect;
use crate::appkit::window::DocWindow;

use burnburn_host_models::form::FormId;
use burnburn_presentation::{AppViewModel, CutControl, CutExtras, CutField, CutSettingsViewModel, CutValue};
use burnburn_types::{DocumentId, OperationId};
use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSButton, NSControlStateValueOn, NSPopUpButton, NSTextField, NSView, NSWindow,
    NSWindowStyleMask,
};
use objc2_foundation::NSString;

use crate::l10n::{HostText, Locale, host_text};
use crate::panels::{CutForm, CutTarget, CutTop, cut_form, cut_settings_panel, sheet_mode_of};

/// The control of one option field.
#[derive(Clone)]
pub(crate) enum OptionControl {
    Text(Retained<NSTextField>),
    Flag(Retained<NSButton>),
    Popup(Retained<NSPopUpButton>),
}

impl OptionControl {
    fn view(&self) -> &NSView {
        match self {
            OptionControl::Text(f) => f,
            OptionControl::Flag(b) => b,
            OptionControl::Popup(p) => p,
        }
    }

    /// Whether the control is on the sheet now (a field of another mode is hidden).
    pub(crate) fn shown(&self) -> bool {
        !self.view().isHidden()
    }

    /// The value as typed or chosen.
    pub(crate) fn read(&self) -> CutValue {
        match self {
            OptionControl::Text(f) => CutValue::Text(f.stringValue().to_string()),
            OptionControl::Flag(b) => CutValue::Flag(b.state() == NSControlStateValueOn),
            OptionControl::Popup(p) => CutValue::Choice(usize::try_from(p.indexOfSelectedItem()).unwrap_or(0)),
        }
    }

    /// Target and action for the controls whose change can grey other rows.
    #[allow(unsafe_code)]
    pub(crate) fn set_target(&self, target: Option<&objc2::runtime::AnyObject>) {
        let action = target.map(|_| objc2::sel!(cutSettingsOptionChanged:));
        match self {
            OptionControl::Text(_) => {}
            OptionControl::Flag(b) => {
                // SAFETY: a nil target cannot dangle; a controller target is weak and DocWindow keeps
                // the controller alive while the control exists (cleared in Drop).
                unsafe { b.setTarget(target) };
                // SAFETY: as above; the action is a controller method taking the sender.
                unsafe { b.setAction(action) };
            }
            OptionControl::Popup(p) => {
                // SAFETY: as above.
                unsafe { p.setTarget(target) };
                // SAFETY: as above.
                unsafe { p.setAction(action) };
            }
        }
    }
}

/// One option control of the Cut Settings sheet, borrowed for tests (`WindowParts::cut_options`).
pub struct CutOptionPart<'a> {
    pub field: CutField,
    pub control: &'a objc2_app_kit::NSControl,
    /// The note under the row (why it is greyed, or the advice).
    pub note: &'a NSTextField,
}

impl OptionWidgets {
    pub(crate) fn part(&self) -> CutOptionPart<'_> {
        let control: &objc2_app_kit::NSControl = match &self.control {
            OptionControl::Text(f) => f,
            OptionControl::Flag(b) => b,
            OptionControl::Popup(p) => p,
        };
        CutOptionPart { field: self.field, control, note: &self.note }
    }
}

/// Everything one option row consists of.
pub(crate) struct OptionWidgets {
    pub(crate) field: CutField,
    pub(crate) control: OptionControl,
    pub(crate) note: Retained<NSTextField>,
}

/// Size of the sheet: the rows scroll above the button row when they do not fit.
const SHEET_W: f64 = 560.0;
const SHEET_H: f64 = 680.0;

/// The Cut Settings sheet of one layer.
pub(crate) struct CutSettingsSheet {
    pub(crate) sheet: Retained<NSWindow>,
    pub(crate) title: Retained<NSTextField>,
    pub(crate) mode: Retained<NSPopUpButton>,
    /// Speed, min power, max power, passes, kerf.
    pub(crate) fields: [Retained<NSTextField>; 5],
    pub(crate) output: Retained<NSButton>,
    pub(crate) visible: Retained<NSButton>,
    pub(crate) error: Retained<NSTextField>,
    pub(crate) ok: Retained<NSButton>,
    pub(crate) cancel: Retained<NSButton>,
    /// The operation whose values are in the fields while the sheet is up.
    pub(crate) shown: Option<OperationId>,
    /// One row per option field, in the scrolling area; rows of other modes are hidden.
    pub(crate) options: Vec<OptionWidgets>,
    view: FormView,
    form: CutForm,
}

fn text_field_of(view: Option<Retained<NSView>>, mtm: MainThreadMarker) -> Retained<NSTextField> {
    view.and_then(|v| v.downcast::<NSTextField>().ok())
        .unwrap_or_else(|| NSTextField::labelWithString(&NSString::from_str(""), mtm))
}

fn button_of(view: Option<Retained<NSView>>, mtm: MainThreadMarker) -> Retained<NSButton> {
    view.and_then(|v| v.downcast::<NSButton>().ok())
        .unwrap_or_else(|| crate::appkit::panels::widgets::push_button(mtm, ""))
}

/// The Cut Settings sheet (targets are set once the controller exists).
#[allow(unsafe_code)]
pub(crate) fn cut_settings_sheet(mtm: MainThreadMarker, locale: Locale) -> CutSettingsSheet {
    let form = cut_form(
        None,
        &CutTop::blank(locale),
        &CutExtras::new(),
        burnburn_presentation::OperationKind::Line,
        locale,
    );
    let view = build_form_with(mtm, &form.spec, SHEET_W, None, FormLayout::Fixed(SHEET_H));
    let size = view.size();
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
    // SAFETY: ownership stays with this Retained.
    unsafe { sheet.setReleasedWhenClosed(false) };
    sheet.setTitle(&NSString::from_str(host_text(HostText::CutSettingsTitle, locale)));
    sheet.setContentView(Some(view.root()));
    let id = |target: CutTarget| form.id(target).unwrap_or(FormId(0));
    let text = |target: CutTarget| text_field_of(view.control(id(target)), mtm);
    let mode = view
        .control(id(CutTarget::Mode))
        .and_then(|v| v.downcast::<NSPopUpButton>().ok())
        .unwrap_or_else(|| NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), rect(0.0, 0.0, 1.0, 1.0), false));
    let buttons = view.buttons();
    let pressed = |target: CutTarget| {
        buttons.iter().find(|(_, bid, _)| *bid == id(target)).map(|(_, _, b)| b.clone())
    };
    let options = CutField::ALL
        .into_iter()
        .filter_map(|field| {
            let fid = form.id(CutTarget::Option(field))?;
            let control = view.control(fid)?;
            let control = match field.control() {
                CutControl::Text => OptionControl::Text(control.downcast::<NSTextField>().ok()?),
                CutControl::Flag => OptionControl::Flag(control.downcast::<NSButton>().ok()?),
                CutControl::Choice => OptionControl::Popup(control.downcast::<NSPopUpButton>().ok()?),
            };
            let note = view.help_label(fid).unwrap_or_else(|| text_field_of(None, mtm));
            Some(OptionWidgets { field, control, note })
        })
        .collect();
    CutSettingsSheet {
        title: view.heading_label(0).unwrap_or_else(|| text_field_of(None, mtm)),
        mode,
        fields: [
            text(CutTarget::Speed),
            text(CutTarget::PowerMin),
            text(CutTarget::PowerMax),
            text(CutTarget::Passes),
            text(CutTarget::Kerf),
        ],
        output: button_of(view.control(id(CutTarget::Output)), mtm),
        visible: button_of(view.control(id(CutTarget::Visible)), mtm),
        error: view.error_label(id(CutTarget::Error)).unwrap_or_else(|| text_field_of(None, mtm)),
        ok: pressed(CutTarget::Ok).unwrap_or_else(|| button_of(None, mtm)),
        cancel: pressed(CutTarget::Cancel).unwrap_or_else(|| button_of(None, mtm)),
        sheet,
        shown: None,
        options,
        view,
        form,
    }
}

/// The values on the option widgets that are on the sheet now.
pub(crate) fn read_options(options: &[(CutField, OptionControl)]) -> Vec<(CutField, CutValue)> {
    options.iter().filter(|(_, c)| c.shown()).map(|(f, c)| (*f, c.read())).collect()
}

/// The values the option widgets hold now, for the fields this sheet has.
fn live_options(c: &CutSettingsSheet, sheet: &CutSettingsViewModel) -> CutExtras {
    let mut live = CutExtras::new();
    for w in &c.options {
        if sheet.extras.get(w.field).is_some() {
            live.set(w.field, w.control.read());
        }
    }
    live
}

impl DocWindow {
    /// Show, fill or end the Cut Settings sheet as presentation opens and closes it. The fields
    /// are filled once per opening, so a refused OK keeps what the user typed; the option rows
    /// are shown again on every refresh from the values on the sheet (a mode, a switch or a
    /// pop-up changes which rows exist and which are greyed).
    pub(crate) fn refresh_cut_settings(&mut self, vm: &AppViewModel, document: DocumentId) {
        let locale = self.locale;
        let c = &mut self.cut_settings;
        let Some(panel) = cut_settings_panel(vm, document, locale) else {
            if c.shown.take().is_some() {
                let (window, sheet) = (self.window.clone(), c.sheet.clone());
                if let Err(e) = crate::appkit::objc_try("ending the Cut Settings sheet", || window.endSheet(&sheet)) {
                    eprintln!("burnburn: {e}");
                }
            }
            return;
        };
        let Some(sheet_vm) = vm.document(document).and_then(|d| d.cut_settings.as_ref()) else { return };
        let opening = c.shown != Some(panel.operation);
        let mut top = CutTop::opening(&panel);
        let (live, mode_index) = if opening {
            (sheet_vm.extras.clone(), panel.mode_index.map_or(-1, |i| i as isize))
        } else {
            // What the user has typed and chosen stays; the layout follows it.
            let typed = |i: usize| c.fields[i].stringValue().to_string();
            top.speed = typed(0);
            top.power_min = typed(1);
            top.power_max = typed(2);
            top.passes = typed(3);
            top.kerf = typed(4);
            top.output = c.output.state() == NSControlStateValueOn;
            top.visible = c.visible.state() == NSControlStateValueOn;
            let index = c.mode.indexOfSelectedItem();
            top.mode_index = usize::try_from(index).ok();
            (live_options(c, sheet_vm), index)
        };
        let kind = sheet_mode_of(sheet_vm, mode_index);
        let form = cut_form(Some(sheet_vm), &top, &live, kind, locale);
        let done = if opening { c.view.load(&form.spec) } else { c.view.update(&form.spec) };
        if !done {
            // The sheet always has the same rows; a different shape is a defect, not a state.
            eprintln!("burnburn: the Cut Settings form changed shape");
        }
        c.form = form;
        if opening {
            c.view.scroll_to_top();
            let attached = c.shown.is_some();
            c.shown = Some(panel.operation);
            if !attached {
                let (window, sheet) = (self.window.clone(), c.sheet.clone());
                if let Err(e) = crate::appkit::objc_try("showing the Cut Settings sheet", || {
                    window.beginSheet_completionHandler(&sheet, None)
                }) {
                    eprintln!("burnburn: {e}");
                    c.shown = None;
                }
            }
        }
    }
}
