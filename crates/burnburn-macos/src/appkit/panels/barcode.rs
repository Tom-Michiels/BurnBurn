//! The barcode sheet (W3-UI-tekst, E10): type pop-up over the 29 symbologies (unsupported ones
//! greyed out with the reason as tooltip), the parameters that belong to the type, the encoded size
//! and the readability warning. It copies the headless model (`crate::panels::text_panels`) into
//! widgets; Maak creates the symbol through presentation. The sheet is a form built from
//! `dialog_forms::barcode_form` (LFC); the rows a type does not have are hidden rows.

use std::rc::Weak;

use burnburn_host_models::form::ButtonRole;
use burnburn_host_models::panels::dialog_forms::{barcode as id, barcode_form};
use burnburn_presentation::AppViewModel;
use burnburn_types::DocumentId;
use objc2::rc::Retained;
use objc2_app_kit::{NSButton, NSControl, NSPopUpButton, NSTextField};

use super::form_surface::{FormSurface, form_sheet};
use super::text::SheetHolder;
use super::textform::{TextTarget, new_target, set_field, set_popup_items, unwire, wire};
use super::widgets::{set_error_text, set_text};
use crate::appkit::app::HostApp;
use crate::appkit::window::DocWindow;
use crate::l10n::Locale;
use crate::panels::{Slot, TextText, text_panels, text_text};

const SHEET_WIDTH: f64 = 480.0;

pub(crate) struct BarcodeSection {
    target: Retained<TextTarget>,
    controls: Vec<Retained<NSControl>>,
    sheet: SheetHolder,
    form: FormSurface,
    types: Retained<NSPopUpButton>,
    payload: Retained<NSTextField>,
    payload_hint: Retained<NSTextField>,
    module: Retained<NSTextField>,
    min_module: Retained<NSTextField>,
    quiet: Retained<NSTextField>,
    height: Retained<NSTextField>,
    ecc: Retained<NSPopUpButton>,
    input_mode: Retained<NSPopUpButton>,
    columns: Retained<NSTextField>,
    width: Retained<NSTextField>,
    size_line: Retained<NSTextField>,
    canonical_line: Retained<NSTextField>,
    problems: Retained<NSTextField>,
    readability: Retained<NSTextField>,
    create: Retained<NSButton>,
    cancel: Retained<NSButton>,
}

pub(crate) fn barcode_section(
    mtm: objc2::MainThreadMarker,
    app: Weak<HostApp>,
    document: DocumentId,
    locale: Locale,
) -> BarcodeSection {
    let target = new_target(mtm, app, document);
    let spec = barcode_form(locale);
    let form = FormSurface::new(mtm, &[(&spec, SHEET_WIDTH)]);
    let types = form.popup(id::TYPE);
    let ecc = form.popup(id::ECC);
    let input_mode = form.popup(id::INPUT_MODE);
    let (payload, module, min_module, quiet) =
        (form.text(id::PAYLOAD), form.text(id::MODULE), form.text(id::MIN_MODULE), form.text(id::QUIET));
    let (height, columns, width) = (form.text(id::HEIGHT), form.text(id::COLUMNS), form.text(id::WIDTH));
    let (create, cancel) = (form.dialog_button(ButtonRole::Primary), form.dialog_button(ButtonRole::Cancel));
    // The slot is the tag of a control; the one action of the text panels reads it.
    let wired: [(&NSControl, Slot); 12] = [
        (&types, Slot::BcType),
        (&payload, Slot::BcPayload),
        (&module, Slot::BcModule),
        (&min_module, Slot::BcMinModule),
        (&quiet, Slot::BcQuiet),
        (&height, Slot::BcHeight),
        (&ecc, Slot::BcEcc),
        (&input_mode, Slot::BcInputMode),
        (&columns, Slot::BcColumns),
        (&width, Slot::BcWidth),
        (&create, Slot::BcCreate),
        (&cancel, Slot::BcCancel),
    ];
    let controls = wired.iter().map(|(c, slot)| wire(c, &target, *slot)).collect();
    let sheet = form_sheet(mtm, &form, text_text(TextText::BarcodeTitle, locale));
    BarcodeSection {
        target,
        controls,
        sheet: SheetHolder { sheet, shown: false },
        types,
        payload,
        payload_hint: form.text(id::PAYLOAD_HINT),
        module,
        min_module,
        quiet,
        height,
        ecc,
        input_mode,
        columns,
        width,
        size_line: form.text(id::SIZE_LINE),
        canonical_line: form.text(id::CANONICAL),
        problems: form.text(id::PROBLEMS),
        readability: form.text(id::READABILITY),
        create,
        cancel,
        form,
    }
}

impl BarcodeSection {
    pub(crate) fn clear_targets(&self) {
        unwire(&self.controls);
        let _ = &self.target;
    }

    pub(crate) fn hide(&mut self, window: &objc2_app_kit::NSWindow) {
        self.sheet.hide(window);
    }

    pub(crate) fn visible(&self) -> bool {
        self.sheet.shown
    }

    /// The widgets, for tests that drive them like a user.
    pub(crate) fn widgets(&self) -> BarcodeWidgets<'_> {
        BarcodeWidgets {
            sheet: &self.sheet.sheet,
            types: &self.types,
            payload: &self.payload,
            module: &self.module,
            min_module: &self.min_module,
            quiet: &self.quiet,
            height: &self.height,
            ecc: &self.ecc,
            input_mode: &self.input_mode,
            columns: &self.columns,
            width: &self.width,
            size_line: &self.size_line,
            canonical_line: &self.canonical_line,
            problems: &self.problems,
            readability: &self.readability,
            create: &self.create,
            cancel: &self.cancel,
        }
    }
}

/// The barcode sheet's widgets as a test sees them.
pub struct BarcodeWidgets<'a> {
    pub sheet: &'a objc2_app_kit::NSWindow,
    pub types: &'a NSPopUpButton,
    pub payload: &'a NSTextField,
    pub module: &'a NSTextField,
    pub min_module: &'a NSTextField,
    pub quiet: &'a NSTextField,
    pub height: &'a NSTextField,
    pub ecc: &'a NSPopUpButton,
    pub input_mode: &'a NSPopUpButton,
    pub columns: &'a NSTextField,
    pub width: &'a NSTextField,
    pub size_line: &'a NSTextField,
    pub canonical_line: &'a NSTextField,
    pub problems: &'a NSTextField,
    pub readability: &'a NSTextField,
    pub create: &'a NSButton,
    pub cancel: &'a NSButton,
}

impl DocWindow {
    /// Show, fill or hide the barcode sheet as presentation says.
    pub(crate) fn refresh_barcode(&mut self, vm: &AppViewModel, document: DocumentId) {
        let panels = text_panels(vm, document, self.locale);
        let window = self.window.clone();
        let s = &mut self.barcode;
        let Some(p) = panels.barcode.as_ref().filter(|_| vm.active == Some(document)) else {
            s.hide(&window);
            return;
        };
        let items: Vec<(String, bool, String)> =
            p.types.iter().map(|ty| (ty.title.clone(), ty.enabled, ty.tip.clone())).collect();
        set_popup_items(&s.types, &items, p.symbology);
        set_field(&s.payload, &p.payload, false);
        set_text(&s.payload_hint, &p.payload_hint);
        set_field(&s.module, &p.module, false);
        set_field(&s.min_module, &p.min_module, false);
        set_field(&s.quiet, &p.quiet, false);
        s.quiet.setPlaceholderString(Some(&objc2_foundation::NSString::from_str(&p.quiet_hint)));
        set_field(&s.width, &p.width, false);
        // Only the rows the chosen type has.
        s.form.set_hidden(id::HEIGHT, p.height.is_none());
        s.form.set_hidden(id::ECC, p.ecc.is_none());
        s.form.set_hidden(id::INPUT_MODE, p.input_modes.is_none());
        s.form.set_hidden(id::COLUMNS, p.columns.is_none());
        if let Some(h) = &p.height {
            set_field(&s.height, h, false);
        }
        if let Some((labels, selected)) = &p.ecc {
            let items: Vec<(String, bool, String)> = labels.iter().map(|l| (l.clone(), true, String::new())).collect();
            set_popup_items(&s.ecc, &items, *selected);
        }
        if let Some((titles, selected)) = &p.input_modes {
            let items: Vec<(String, bool, String)> = titles.iter().map(|l| (l.clone(), true, String::new())).collect();
            set_popup_items(&s.input_mode, &items, *selected);
        }
        if let Some(c) = &p.columns {
            set_field(&s.columns, c, false);
        }
        set_text(&s.size_line, &p.size_line);
        set_text(&s.canonical_line, &p.canonical_line);
        set_error_text(&s.problems, &p.problems);
        set_error_text(&s.readability, &p.readability);
        s.create.setEnabled(p.create_enabled);
        s.create.setTitle(&objc2_foundation::NSString::from_str(&p.create_title));
        let size = s.form.relayout();
        s.sheet.show(&window, (size.width, size.height));
    }
}
