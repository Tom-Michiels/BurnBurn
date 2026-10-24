//! The barcode sheet and the geometry dialog (Round Corners, Join, Optimize) as [`FormSpec`]s (LFC).
//! Like the other panels the host keeps its own widgets and copies the view model into them; the
//! ids below say which widget is which.

use super::shapes::GeometryDialogPanel;
use super::text::{TextText, text_text};
use crate::form::{Control, DialogButtons, FormButton, FormRow, FormSection, FormSpec, split_caption};
use crate::l10n::{HostText, Locale, host_text};
use crate::style::StatusTone;

/// The ids of the barcode sheet.
pub mod barcode {
    pub const TYPE: u32 = 1;
    pub const PAYLOAD: u32 = 2;
    pub const PAYLOAD_HINT: u32 = 3;
    pub const MODULE: u32 = 4;
    pub const MIN_MODULE: u32 = 5;
    pub const QUIET: u32 = 6;
    pub const HEIGHT: u32 = 7;
    pub const ECC: u32 = 8;
    pub const INPUT_MODE: u32 = 9;
    pub const COLUMNS: u32 = 10;
    pub const WIDTH: u32 = 11;
    pub const SIZE_LINE: u32 = 12;
    pub const CANONICAL: u32 = 13;
    pub const PROBLEMS: u32 = 14;
    pub const READABILITY: u32 = 15;
    pub const CANCEL: u32 = 20;
    pub const CREATE: u32 = 21;
}

/// The ids of the geometry dialog.
pub mod geometry {
    pub const VALUE: u32 = 1;
    pub const CANCEL: u32 = 20;
    pub const OK: u32 = 21;
}

/// A row captioned `caption`; a unit in brackets at its end becomes the suffix, any other brackets
/// the tooltip of the control.
fn captioned(id: u32, caption: &str, control: Control) -> FormRow {
    let split = split_caption(caption);
    let mut row = FormRow::new(id, &split.label, control);
    row.unit = split.unit;
    row.tooltip = split.hint;
    row
}

fn note(id: u32, lines: u8, tone: StatusTone) -> FormRow {
    FormRow::new(id, "", Control::Note { value: String::new(), lines, tone, strong: false })
}

fn dialog_buttons(cancel: (u32, &str), primary: (u32, &str)) -> DialogButtons {
    DialogButtons {
        cancel: Some(FormButton::new(cancel.0, cancel.1)),
        primary: Some(FormButton::new(primary.0, primary.1)),
        ..DialogButtons::default()
    }
}

/// The barcode sheet: its rows are all there; the host hides the ones the chosen type lacks.
pub fn barcode_form(locale: Locale) -> FormSpec {
    let t = |x: TextText| text_text(x, locale);
    let text = || Control::Text { value: String::new(), placeholder: String::new() };
    let choice = || Control::Choice { options: Vec::new(), selected: 0 };
    let rows = vec![
        captioned(barcode::TYPE, t(TextText::BarcodeType), choice()),
        captioned(barcode::PAYLOAD, t(TextText::Payload), text()),
        note(barcode::PAYLOAD_HINT, 2, StatusTone::Neutral),
        captioned(barcode::MODULE, t(TextText::Module), text()),
        captioned(barcode::MIN_MODULE, t(TextText::MinModule), text()),
        captioned(barcode::QUIET, t(TextText::Quiet), text()),
        captioned(barcode::HEIGHT, t(TextText::BarHeight), text()),
        captioned(barcode::ECC, t(TextText::Ecc), choice()),
        captioned(barcode::INPUT_MODE, t(TextText::InputMode), choice()),
        captioned(barcode::COLUMNS, t(TextText::Columns), text()),
        captioned(barcode::WIDTH, t(TextText::Width), text()),
        note(barcode::SIZE_LINE, 1, StatusTone::Neutral),
        note(barcode::CANONICAL, 1, StatusTone::Neutral),
        note(barcode::PROBLEMS, 3, StatusTone::Error),
        note(barcode::READABILITY, 2, StatusTone::Error),
    ];
    FormSpec {
        title: t(TextText::BarcodeTitle).to_owned(),
        sections: vec![FormSection::new(None, rows)],
        buttons: dialog_buttons((barcode::CANCEL, t(TextText::Cancel)), (barcode::CREATE, t(TextText::Create))),
        ..FormSpec::default()
    }
}

/// The geometry dialog for `dialog`: the title as heading, the value with its unit, the refusal
/// under the field. The shape is the same for every kind and with or without a refusal.
pub fn geometry_dialog_form(dialog: &GeometryDialogPanel, locale: Locale) -> FormSpec {
    let mut row = captioned(geometry::VALUE, &dialog.label, Control::Number { value: dialog.text.clone() });
    // The refusal row always exists (empty shows nothing), so the form keeps its shape.
    row.error = Some(dialog.error.clone());
    row.invalid = !dialog.error.is_empty();
    FormSpec {
        title: dialog.title.clone(),
        sections: vec![FormSection::new(Some(&dialog.title), vec![row])],
        buttons: dialog_buttons(
            (geometry::CANCEL, host_text(HostText::GeometryCancel, locale)),
            (geometry::OK, host_text(HostText::GeometryOk, locale)),
        ),
        ..FormSpec::default()
    }
}
