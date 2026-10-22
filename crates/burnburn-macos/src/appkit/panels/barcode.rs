//! The barcode sheet (W3-UI-tekst, E10): type pop-up over the 29 symbologies (unsupported ones
//! greyed out with the reason as tooltip), the parameters that belong to the type, the encoded size
//! and the readability warning. It copies the headless model (`crate::panels::text_panels`) into
//! widgets; Maak creates the symbol through presentation.

use std::rc::Weak;

use burnburn_presentation::AppViewModel;
use burnburn_types::DocumentId;
use objc2::rc::Retained;
use objc2_app_kit::{NSButton, NSControl, NSPopUpButton, NSTextField};

use super::image::sheet_window;
use super::text::{SheetHolder, view};
use super::textform::{
    Cells, Form, FormRow, TextTarget, caption, message_line, new_target, popup, push, set_field, set_popup_items,
    text_field, unwire,
};
use super::widgets::set_text;
use crate::appkit::app::HostApp;
use crate::appkit::window::DocWindow;
use crate::l10n::Locale;
use crate::panels::{Slot, TextText, text_panels, text_text};

const SHEET_WIDTH: f64 = 480.0;

// Row positions in the form, for showing only the rows the chosen type has.
const ROW_HEIGHT: usize = 6;
const ROW_ECC: usize = 7;
const ROW_INPUT: usize = 8;
const ROW_COLUMNS: usize = 9;

pub(crate) struct BarcodeSection {
    target: Retained<TextTarget>,
    controls: Vec<Retained<NSControl>>,
    sheet: SheetHolder,
    form: Form,
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
    let t = |text: TextText| text_text(text, locale);
    let target = new_target(mtm, app, document);
    let labelled = |text: TextText, control: &objc2_app_kit::NSView, height: f64| {
        FormRow::new(Cells::Labeled(caption(mtm, t(text)), objc2::Message::retain(control)), height)
    };
    let types = popup(mtm, &target, Slot::BcType, t(TextText::BarcodeType));
    let payload = text_field(mtm, &target, Slot::BcPayload, t(TextText::Payload));
    let payload_hint = message_line(mtm, false);
    let module = text_field(mtm, &target, Slot::BcModule, t(TextText::Module));
    let min_module = text_field(mtm, &target, Slot::BcMinModule, t(TextText::MinModule));
    let quiet = text_field(mtm, &target, Slot::BcQuiet, t(TextText::Quiet));
    let height = text_field(mtm, &target, Slot::BcHeight, t(TextText::BarHeight));
    let ecc = popup(mtm, &target, Slot::BcEcc, t(TextText::Ecc));
    let input_mode = popup(mtm, &target, Slot::BcInputMode, t(TextText::InputMode));
    let columns = text_field(mtm, &target, Slot::BcColumns, t(TextText::Columns));
    let width = text_field(mtm, &target, Slot::BcWidth, t(TextText::Width));
    let size_line = message_line(mtm, false);
    let canonical_line = message_line(mtm, false);
    let problems = message_line(mtm, true);
    let readability = message_line(mtm, true);
    let cancel = push(mtm, &target, Slot::BcCancel, t(TextText::Cancel));
    let create = push(mtm, &target, Slot::BcCreate, t(TextText::Create));
    create.setKeyEquivalent(&objc2_foundation::NSString::from_str("\r"));
    cancel.setKeyEquivalent(&objc2_foundation::NSString::from_str("\u{1b}"));
    let rows = vec![
        labelled(TextText::BarcodeType, &types, 26.0),
        labelled(TextText::Payload, &payload, 24.0),
        FormRow::new(Cells::Full(view!(payload_hint)), 30.0),
        labelled(TextText::Module, &module, 24.0),
        labelled(TextText::MinModule, &min_module, 24.0),
        labelled(TextText::Quiet, &quiet, 24.0),
        labelled(TextText::BarHeight, &height, 24.0),
        labelled(TextText::Ecc, &ecc, 26.0),
        labelled(TextText::InputMode, &input_mode, 26.0),
        labelled(TextText::Columns, &columns, 24.0),
        labelled(TextText::Width, &width, 24.0),
        FormRow::new(Cells::Full(view!(size_line)), 18.0),
        FormRow::new(Cells::Full(view!(canonical_line)), 18.0),
        FormRow::new(Cells::Full(view!(problems)), 50.0),
        FormRow::new(Cells::Full(view!(readability)), 34.0),
        FormRow::new(Cells::Side(vec![view!(cancel), view!(create)]), 28.0),
    ];
    let form = Form::new(mtm, SHEET_WIDTH, rows);
    let (sheet, _) = sheet_window(mtm, (SHEET_WIDTH, 460.0), t(TextText::BarcodeTitle));
    sheet.setContentView(Some(&form.content));
    let all: Vec<&NSControl> = vec![
        &types,
        &payload,
        &module,
        &min_module,
        &quiet,
        &height,
        &ecc,
        &input_mode,
        &columns,
        &width,
        &cancel,
        &create,
    ];
    let controls = all.iter().map(|c| objc2::Message::retain(*c)).collect();
    BarcodeSection {
        target,
        controls,
        sheet: SheetHolder { sheet, shown: false },
        form,
        types,
        payload,
        payload_hint,
        module,
        min_module,
        quiet,
        height,
        ecc,
        input_mode,
        columns,
        width,
        size_line,
        canonical_line,
        problems,
        readability,
        create,
        cancel,
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
        s.form.rows[ROW_HEIGHT].visible.set(p.height.is_some());
        s.form.rows[ROW_ECC].visible.set(p.ecc.is_some());
        s.form.rows[ROW_INPUT].visible.set(p.input_modes.is_some());
        s.form.rows[ROW_COLUMNS].visible.set(p.columns.is_some());
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
        set_text(&s.problems, &p.problems);
        set_text(&s.readability, &p.readability);
        s.create.setEnabled(p.create_enabled);
        s.create.setTitle(&objc2_foundation::NSString::from_str(&p.create_title));
        let height = s.form.lay_out();
        s.sheet.show(&window, (SHEET_WIDTH, height));
    }
}
