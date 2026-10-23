//! The file, library, export, import, image and tracer panels as [`FormSpec`]s (LF6b). Each
//! function describes the *skeleton* of a panel: its sections with headings, its rows with the unit
//! as a suffix, the ranges as help, and the dialog buttons by role. What the panel shows is still
//! copied in by the host from the panel's view model (`panels_files`, `panels_image`, `import`);
//! the ids below say which widget is which, so a host finds the row and the model never has to
//! know the widget. Rows that only exist for some files or states are in the skeleton and hidden by
//! the host.

use burnburn_presentation::TracePreset;

use crate::form::{Control, DialogButtons, FormButton, FormId, FormRow, FormSection, FormSpec};
use crate::format::{Quantity, unit_label};
use crate::l10n::{HostText, Locale, host_text};
use crate::l10n_files::{FilesText as T, files_text};
use crate::panels::{IMPORT_UNITS, import_unit_name};
use crate::panels_files::{art_unit_titles, import_layers_check_text};
use crate::panels_image::{GRAY_METHODS, gray_method_name, trace_preset_name};
use crate::style::StatusTone;

/// The ids of the export dialog.
pub mod export {
    pub const TITLE: u32 = 1;
    pub const OBJECTS: u32 = 2;
    pub const SELECTION_ONLY: u32 = 3;
    pub const UNIT: u32 = 4;
    pub const LAYERS: u32 = 5;
    pub const CURVES: u32 = 6;
    pub const STROKE: u32 = 7;
    pub const DECIMALS: u32 = 8;
    pub const TOLERANCE: u32 = 9;
    pub const USE_COLORS: u32 = 10;
    pub const LOSSES: u32 = 11;
    pub const ACKNOWLEDGE: u32 = 12;
    pub const STATUS: u32 = 13;
    pub const CLOSE: u32 = 20;
    pub const EXPORT: u32 = 21;
}

/// The ids of the bundle sheet.
pub mod bundle {
    pub const TITLE: u32 = 1;
    pub const STATUS: u32 = 2;
    pub const REPORT: u32 = 3;
    /// The four kinds of an export: `KIND + 0..4`.
    pub const KIND: u32 = 10;
    pub const PREVIEW: u32 = 20;
    pub const CONFLICT: u32 = 21;
    /// The profiles of an import: `PROFILE + 0..PROFILE_SLOTS`.
    pub const PROFILE: u32 = 30;
    pub const CLOSE: u32 = 50;
    pub const PRIMARY: u32 = 51;
    /// Check boxes for the profiles of an incoming bundle (a bundle holds few).
    pub const PROFILE_SLOTS: usize = 4;
    /// The section shown for an export, and the one shown for an import.
    pub const EXPORT_SECTION: usize = 1;
    pub const IMPORT_SECTION: usize = 2;
}

/// The ids of the Material Library panel.
pub mod material {
    pub const STATUS: u32 = 1;
    pub const MATERIAL: u32 = 2;
    pub const THICKNESS: u32 = 3;
    pub const MACHINE: u32 = 4;
    pub const ENTRIES: u32 = 5;
    pub const DETAIL: u32 = 6;
    pub const LAYER: u32 = 7;
    /// Apply, update, update and apply: `LAYER_ACTIONS + 0..3`.
    pub const LAYER_ACTIONS: u32 = 8;
    pub const NAME: u32 = 11;
    pub const STORE_MATERIAL: u32 = 12;
    pub const STORE_THICKNESS: u32 = 13;
    /// Save, overwrite, remove: `STORE_ACTIONS + 0..3`.
    pub const STORE_ACTIONS: u32 = 14;
    pub const CLOSE: u32 = 20;
}

/// The ids of the Art Library panel.
pub mod art {
    pub const STATUS: u32 = 1;
    pub const GROUPS: u32 = 2;
    pub const SEARCH: u32 = 3;
    pub const ITEMS: u32 = 4;
    pub const THUMBNAIL: u32 = 5;
    pub const DETAIL: u32 = 6;
    pub const PLACE: u32 = 7;
    pub const SCALE_PROMPT: u32 = 8;
    pub const SCALE_UNIT: u32 = 9;
    pub const SCALE_RETRY: u32 = 10;
    pub const NAME: u32 = 11;
    /// Add file, add selection: `ADD + 0..2`.
    pub const ADD: u32 = 12;
    /// New group, rename group, remove group: `GROUP_ACTIONS + 0..3`.
    pub const GROUP_ACTIONS: u32 = 14;
    /// Rename item, move item, remove item: `ITEM_ACTIONS + 0..3`.
    pub const ITEM_ACTIONS: u32 = 17;
    pub const CLOSE: u32 = 30;
}

/// The ids of the import sheet.
pub mod import {
    pub const FILE: u32 = 1;
    pub const SIZE: u32 = 2;
    pub const PROMPT: u32 = 3;
    pub const UNITS: u32 = 4;
    pub const PAGES: u32 = 5;
    pub const LAYERS: u32 = 6;
    pub const SUMMARY: u32 = 7;
    pub const REPORT: u32 = 8;
    pub const CANCEL: u32 = 20;
    pub const COMMIT: u32 = 21;
}

/// The ids of the bitmap import sheet.
pub mod image_import {
    pub const FILE: u32 = 1;
    pub const PIXELS: u32 = 2;
    pub const DPI: u32 = 3;
    pub const SUGGESTION: u32 = 4;
    pub const SIZE: u32 = 5;
    pub const ISSUE: u32 = 6;
    pub const CANCEL: u32 = 20;
    pub const COMMIT: u32 = 21;
}

/// The ids of the Adjust Image sheet (two forms: the values left, the preview and buttons right).
pub mod image_adjust {
    pub const TITLE: u32 = 1;
    pub const SOURCE: u32 = 2;
    /// Brightness, contrast, gamma, sharpen amount, sharpen radius: `VALUE + 0..5`.
    pub const VALUE: u32 = 3;
    pub const GRAY: u32 = 8;
    pub const CROP_NOTE: u32 = 9;
    /// Crop x, y, width, height: `CROP + 0..4`.
    pub const CROP: u32 = 10;
    pub const ISSUE: u32 = 14;
    pub const PREVIEW: u32 = 15;
    pub const COMPUTING: u32 = 16;
    pub const CANCEL: u32 = 20;
    pub const APPLY: u32 = 21;
}

/// The ids of the tracer sheet (two forms: the settings left, the preview and buttons right).
pub mod tracer {
    pub const SOURCE: u32 = 1;
    pub const RECIPE: u32 = 2;
    pub const MODE: u32 = 3;
    pub const DPI: u32 = 4;
    pub const SUGGESTION: u32 = 5;
    pub const THRESHOLD: u32 = 6;
    pub const THRESHOLD_VALUE: u32 = 7;
    pub const INVERT: u32 = 8;
    pub const IGNORE: u32 = 9;
    pub const SMOOTHNESS: u32 = 10;
    pub const CURVES: u32 = 11;
    pub const ISSUE: u32 = 12;
    pub const PREVIEW: u32 = 13;
    pub const SUMMARY: u32 = 14;
    pub const COMPUTING: u32 = 15;
    pub const SCALE_HINT: u32 = 16;
    pub const APPLY_LINKED: u32 = 17;
    pub const CLOSE: u32 = 20;
    pub const APPLY: u32 = 21;
}

// ---------------------------------------------------------------------------------------------
// Row helpers.
// ---------------------------------------------------------------------------------------------

/// `text` without a trailing bracket: the caption, and the bracket's content when it is not the
/// unit the row already shows as its suffix (a range such as `−1…1` stays as the row's help).
/// `Lijndikte (mm):` is `Lijndikte:` with the unit as the suffix; `Contrast (0…10)` is `Contrast`
/// with the help `0…10`.
fn split_caption(text: &str, unit: Option<Quantity>) -> (String, Option<String>) {
    let trimmed = text.trim_end();
    let (body, colon) = match trimmed.strip_suffix(':') {
        Some(rest) => (rest.trim_end(), ":"),
        None => (trimmed, ""),
    };
    if let (Some(open), true) = (body.rfind(" ("), body.ends_with(')')) {
        let inside = &body[open + 2..body.len() - 1];
        let caption = format!("{}{colon}", &body[..open]);
        let is_unit = unit.is_some_and(|u| unit_label(u) == inside);
        return (caption, (!is_unit).then(|| inside.to_owned()));
    }
    (trimmed.to_owned(), None)
}

fn text_row(id: u32, label: &str, unit: Option<Quantity>) -> FormRow {
    let (caption, hint) = split_caption(label, unit);
    let mut row = FormRow::new(id, &caption, Control::Text { value: String::new(), placeholder: String::new() });
    row.unit = unit;
    row.help = hint;
    row
}

fn number_row(id: u32, label: &str, unit: Option<Quantity>) -> FormRow {
    let (caption, hint) = split_caption(label, unit);
    let mut row = FormRow::new(id, &caption, Control::Number { value: String::new() });
    row.unit = unit;
    row.help = hint;
    row
}

fn choice_row(id: u32, label: &str, options: Vec<String>) -> FormRow {
    FormRow::new(id, label, Control::Choice { options, selected: 0 })
}

fn check_row(id: u32, title: &str) -> FormRow {
    let (caption, hint) = split_caption(title, None);
    let mut row = FormRow::new(id, &caption, Control::Check { on: false });
    row.help = hint;
    row
}

fn note_row(id: u32, lines: u8, tone: StatusTone, strong: bool) -> FormRow {
    FormRow::new(id, "", Control::Note { value: String::new(), lines, tone, strong })
}

fn report_row(id: u32, lines: u8) -> FormRow {
    FormRow::new(id, "", Control::Report { text: String::new(), lines })
}

fn actions_row(id: u32, titles: &[&str]) -> FormRow {
    FormRow::new(id, "", Control::Actions { titles: titles.iter().map(|t| (*t).to_owned()).collect() })
}

fn preview_row(id: u32, width: u16, height: u16) -> FormRow {
    FormRow::new(id, "", Control::Preview { width, height })
}

fn buttons(cancel: (u32, &str), primary: Option<(u32, &str)>) -> DialogButtons {
    DialogButtons {
        cancel: Some(FormButton::new(cancel.0, cancel.1)),
        primary: primary.map(|(id, title)| FormButton::new(id, title)),
        ..DialogButtons::default()
    }
}

fn section(heading: Option<&str>, rows: Vec<FormRow>) -> FormSection {
    FormSection::new(heading, rows)
}

// ---------------------------------------------------------------------------------------------
// The forms.
// ---------------------------------------------------------------------------------------------

/// The export dialog (SVG and DXF).
pub fn export_form(locale: Locale) -> FormSpec {
    let t = |text: T| files_text(text, locale);
    let mut decimals = number_row(export::DECIMALS, t(T::LabelDecimals), None);
    decimals.help = Some(t(T::DecimalsHint).to_owned());
    FormSpec {
        title: t(T::ExportSvgTitle).to_owned(),
        sections: vec![
            section(
                None,
                vec![
                    note_row(export::TITLE, 1, StatusTone::Neutral, true),
                    note_row(export::OBJECTS, 1, StatusTone::Neutral, false),
                    check_row(export::SELECTION_ONLY, t(T::CheckSelectionOnly)),
                ],
            ),
            section(
                Some(t(T::SectionOptions)),
                vec![
                    choice_row(export::UNIT, t(T::LabelUnit), Vec::new()),
                    choice_row(export::LAYERS, t(T::LabelLayers), Vec::new()),
                    choice_row(export::CURVES, t(T::LabelCurves), Vec::new()),
                    number_row(export::STROKE, t(T::LabelStroke), Some(Quantity::Millimeters)),
                    decimals,
                    number_row(export::TOLERANCE, t(T::LabelTolerance), Some(Quantity::Millimeters)),
                    check_row(export::USE_COLORS, t(T::CheckUseColors)),
                ],
            ),
            section(
                Some(t(T::SectionExportLoss)),
                vec![
                    report_row(export::LOSSES, 7),
                    check_row(export::ACKNOWLEDGE, t(T::ExportAcknowledge)),
                    note_row(export::STATUS, 2, StatusTone::Neutral, false),
                ],
            ),
        ],
        buttons: buttons((export::CLOSE, t(T::ButtonClose)), Some((export::EXPORT, t(T::ButtonExport)))),
    }
}

/// The bundle sheet: section 1 is for an export, section 2 for an import; the host shows one.
pub fn bundle_form(locale: Locale) -> FormSpec {
    let t = |text: T| files_text(text, locale);
    let kinds = (0..4).map(|i| check_row(bundle::KIND + i, "")).collect();
    let mut import_rows =
        vec![report_row(bundle::PREVIEW, 8), choice_row(bundle::CONFLICT, t(T::LabelConflict), Vec::new())];
    import_rows.extend((0..bundle::PROFILE_SLOTS as u32).map(|i| check_row(bundle::PROFILE + i, "")));
    FormSpec {
        title: t(T::BundleExportTitle).to_owned(),
        sections: vec![
            section(
                None,
                vec![
                    note_row(bundle::TITLE, 1, StatusTone::Neutral, true),
                    note_row(bundle::STATUS, 2, StatusTone::Neutral, false),
                    note_row(bundle::REPORT, 3, StatusTone::Neutral, false),
                ],
            ),
            section(Some(t(T::BundleChoose).trim_end_matches(':')), kinds),
            section(Some(t(T::SectionBundlePreview)), import_rows),
        ],
        buttons: buttons((bundle::CLOSE, t(T::ButtonClose)), Some((bundle::PRIMARY, t(T::ButtonExport)))),
    }
}

/// The Material Library panel.
pub fn material_form(locale: Locale) -> FormSpec {
    let t = |text: T| files_text(text, locale);
    FormSpec {
        title: t(T::MaterialTitle).to_owned(),
        sections: vec![
            section(None, vec![note_row(material::STATUS, 2, StatusTone::Neutral, false)]),
            section(
                Some(t(T::SectionSearch)),
                vec![
                    choice_row(material::MATERIAL, t(T::LabelMaterial), Vec::new()),
                    number_row(material::THICKNESS, t(T::LabelThickness), Some(Quantity::Millimeters)),
                    choice_row(material::MACHINE, t(T::LabelMachine), Vec::new()),
                    choice_row(material::ENTRIES, t(T::LabelEntries), Vec::new()),
                    note_row(material::DETAIL, 2, StatusTone::Neutral, false),
                ],
            ),
            section(
                Some(t(T::SectionMaterialLayer)),
                vec![
                    choice_row(material::LAYER, t(T::LabelLayer), Vec::new()),
                    actions_row(
                        material::LAYER_ACTIONS,
                        &[t(T::ButtonApply), t(T::ButtonUpdate), t(T::ButtonUpdateApply)],
                    ),
                ],
            ),
            section(
                Some(t(T::SectionMaterialStore)),
                vec![
                    text_row(material::NAME, t(T::LabelName), None),
                    text_row(material::STORE_MATERIAL, t(T::LabelNewMaterial), None),
                    number_row(material::STORE_THICKNESS, t(T::LabelThickness), Some(Quantity::Millimeters)),
                    actions_row(
                        material::STORE_ACTIONS,
                        &[t(T::ButtonSave), t(T::ButtonOverwrite), t(T::ButtonRemove)],
                    ),
                ],
            ),
        ],
        buttons: buttons((material::CLOSE, t(T::ButtonClose)), None),
    }
}

/// The Art Library panel.
pub fn art_form(locale: Locale) -> FormSpec {
    let t = |text: T| files_text(text, locale);
    let mut name = text_row(art::NAME, t(T::LabelName), None);
    name.help = Some(t(T::ArtNameHint).to_owned());
    FormSpec {
        title: t(T::ArtTitle).to_owned(),
        sections: vec![
            section(None, vec![note_row(art::STATUS, 2, StatusTone::Neutral, false)]),
            section(
                Some(t(T::SectionSearch)),
                vec![
                    choice_row(art::GROUPS, t(T::LabelGroup), Vec::new()),
                    text_row(art::SEARCH, t(T::LabelSearch), None),
                    choice_row(art::ITEMS, t(T::LabelItems), Vec::new()),
                ],
            ),
            section(
                Some(t(T::SectionArtItem)),
                vec![
                    preview_row(art::THUMBNAIL, 128, 128),
                    note_row(art::DETAIL, 2, StatusTone::Neutral, false),
                    actions_row(art::PLACE, &[t(T::ButtonPlace)]),
                    note_row(art::SCALE_PROMPT, 2, StatusTone::Warning, false),
                    choice_row(art::SCALE_UNIT, t(T::LabelUnit), art_unit_titles()),
                    actions_row(art::SCALE_RETRY, &[t(T::ButtonAddFile)]),
                ],
            ),
            section(
                Some(t(T::SectionArtAdd)),
                vec![name, actions_row(art::ADD, &[t(T::ButtonAddFile), t(T::ButtonAddSelection)])],
            ),
            section(
                Some(t(T::SectionArtManage)),
                vec![
                    actions_row(
                        art::GROUP_ACTIONS,
                        &[t(T::ButtonNewGroup), t(T::ButtonRenameGroup), t(T::ButtonRemoveGroup)],
                    ),
                    actions_row(
                        art::ITEM_ACTIONS,
                        &[t(T::ButtonRenameItem), t(T::ButtonMoveItem), t(T::ButtonRemoveItem)],
                    ),
                ],
            ),
        ],
        buttons: buttons((art::CLOSE, t(T::ButtonClose)), None),
    }
}

/// The import sheet of vector files (SVG, DXF, PDF, AI, plotter files, LightBurn projects).
pub fn import_form(locale: Locale) -> FormSpec {
    let h = |text: HostText| host_text(text, locale);
    let f = |text: T| files_text(text, locale);
    let units: Vec<String> = IMPORT_UNITS.iter().map(|u| import_unit_name(*u, locale).to_owned()).collect();
    FormSpec {
        title: h(HostText::ImportTitle).to_owned(),
        sections: vec![
            section(
                None,
                vec![
                    note_row(import::FILE, 1, StatusTone::Neutral, true),
                    note_row(import::SIZE, 1, StatusTone::Neutral, false),
                ],
            ),
            section(
                Some(f(T::SectionOptions)),
                vec![
                    note_row(import::PROMPT, 2, StatusTone::Neutral, false),
                    choice_row(import::UNITS, h(HostText::ImportUnitLabel), units),
                    choice_row(import::PAGES, f(T::ImportPageLabel), Vec::new()),
                    check_row(import::LAYERS, import_layers_check_text(locale)),
                ],
            ),
            section(
                Some(f(T::SectionReport)),
                vec![note_row(import::SUMMARY, 1, StatusTone::Neutral, false), report_row(import::REPORT, 9)],
            ),
        ],
        buttons: buttons(
            (import::CANCEL, h(HostText::ImportCancel)),
            Some((import::COMMIT, h(HostText::ImportCommit))),
        ),
    }
}

/// The bitmap import sheet.
pub fn image_import_form(locale: Locale) -> FormSpec {
    let h = |text: HostText| host_text(text, locale);
    let f = |text: T| files_text(text, locale);
    FormSpec {
        title: h(HostText::ImageImportTitle).to_owned(),
        sections: vec![
            section(
                None,
                vec![
                    note_row(image_import::FILE, 1, StatusTone::Neutral, true),
                    note_row(image_import::PIXELS, 1, StatusTone::Neutral, false),
                ],
            ),
            section(
                Some(f(T::SectionImageSize)),
                vec![
                    number_row(image_import::DPI, h(HostText::ImageImportDpi), Some(Quantity::DotsPerInch)),
                    note_row(image_import::SUGGESTION, 2, StatusTone::Neutral, false),
                    note_row(image_import::SIZE, 1, StatusTone::Neutral, true),
                    note_row(image_import::ISSUE, 2, StatusTone::Error, false),
                ],
            ),
        ],
        buttons: buttons(
            (image_import::CANCEL, h(HostText::ImageImportCancel)),
            Some((image_import::COMMIT, h(HostText::ImageImportCommit))),
        ),
    }
}

/// The Adjust Image sheet: the values on the left, the preview with the buttons on the right.
pub fn image_adjust_forms(locale: Locale) -> (FormSpec, FormSpec) {
    let h = |text: HostText| host_text(text, locale);
    let f = |text: T| files_text(text, locale);
    let gray: Vec<String> = GRAY_METHODS.iter().map(|g| gray_method_name(*g, locale).to_owned()).collect();
    let values = [
        (HostText::ImageAdjustBrightness, None),
        (HostText::ImageAdjustContrast, None),
        (HostText::ImageAdjustGamma, None),
        (HostText::ImageAdjustSharpenAmount, None),
        (HostText::ImageAdjustSharpenRadius, Some(Quantity::Millimeters)),
    ];
    let mut value_rows: Vec<FormRow> = values
        .iter()
        .enumerate()
        .map(|(i, (text, unit))| number_row(image_adjust::VALUE + i as u32, h(*text), *unit))
        .collect();
    value_rows.push(choice_row(image_adjust::GRAY, h(HostText::ImageAdjustGray), gray));
    let crop = [
        HostText::ImageAdjustCropX,
        HostText::ImageAdjustCropY,
        HostText::ImageAdjustCropWidth,
        HostText::ImageAdjustCropHeight,
    ];
    let mut crop_rows = vec![note_row(image_adjust::CROP_NOTE, 2, StatusTone::Neutral, false)];
    crop_rows.extend(
        crop.iter()
            .enumerate()
            .map(|(i, text)| number_row(image_adjust::CROP + i as u32, h(*text), Some(Quantity::Pixels))),
    );
    crop_rows.push(note_row(image_adjust::ISSUE, 2, StatusTone::Error, false));
    let left = FormSpec {
        title: h(HostText::ImageAdjustTitle).to_owned(),
        sections: vec![
            section(
                None,
                vec![
                    note_row(image_adjust::TITLE, 1, StatusTone::Neutral, true),
                    note_row(image_adjust::SOURCE, 1, StatusTone::Neutral, false),
                ],
            ),
            section(Some(f(T::SectionImageValues)), value_rows),
            section(Some(f(T::SectionImageCrop)), crop_rows),
        ],
        buttons: DialogButtons::default(),
    };
    let right = FormSpec {
        title: h(HostText::ImageAdjustTitle).to_owned(),
        sections: vec![section(
            Some(h(HostText::ImageAdjustPreview)),
            vec![
                preview_row(image_adjust::PREVIEW, 320, 322),
                note_row(image_adjust::COMPUTING, 1, StatusTone::Neutral, false),
            ],
        )],
        buttons: buttons(
            (image_adjust::CANCEL, h(HostText::ImageAdjustCancel)),
            Some((image_adjust::APPLY, h(HostText::ImageAdjustApply))),
        ),
    };
    (left, right)
}

/// The tracer sheet: the settings on the left, the preview with the buttons on the right.
pub fn tracer_forms(locale: Locale) -> (FormSpec, FormSpec) {
    let h = |text: HostText| host_text(text, locale);
    let f = |text: T| files_text(text, locale);
    let presets: Vec<String> = TracePreset::ALL.iter().map(|p| trace_preset_name(*p, locale)).collect();
    let modes = vec![h(HostText::TracerModeContour).to_owned(), h(HostText::TracerModeCenterline).to_owned()];
    let thresholds = vec![
        h(HostText::TracerThresholdOtsu).to_owned(),
        h(HostText::TracerThresholdFixed).to_owned(),
        h(HostText::TracerThresholdAdaptive).to_owned(),
    ];
    let left = FormSpec {
        title: h(HostText::TracerTitle).to_owned(),
        sections: vec![
            section(None, vec![note_row(tracer::SOURCE, 1, StatusTone::Neutral, true)]),
            section(
                Some(f(T::SectionTracerKind)),
                vec![
                    choice_row(tracer::RECIPE, h(HostText::TracerRecipe), presets),
                    choice_row(tracer::MODE, h(HostText::TracerMode), modes),
                    number_row(tracer::DPI, h(HostText::TracerDpi), Some(Quantity::DotsPerInch)),
                    actions_row(tracer::SUGGESTION, &[""]),
                ],
            ),
            section(
                Some(f(T::SectionTracerThreshold)),
                vec![
                    choice_row(tracer::THRESHOLD, h(HostText::TracerThreshold), thresholds),
                    number_row(tracer::THRESHOLD_VALUE, h(HostText::TracerThresholdValue), None),
                    check_row(tracer::INVERT, h(HostText::TracerInvert)),
                ],
            ),
            section(
                Some(f(T::SectionTracerOutput)),
                vec![
                    number_row(tracer::IGNORE, h(HostText::TracerIgnore), Some(Quantity::SquareMillimeters)),
                    number_row(tracer::SMOOTHNESS, h(HostText::TracerSmoothness), Some(Quantity::Millimeters)),
                    check_row(tracer::CURVES, h(HostText::TracerCurves)),
                    note_row(tracer::ISSUE, 2, StatusTone::Error, false),
                ],
            ),
        ],
        buttons: DialogButtons::default(),
    };
    let right = FormSpec {
        title: h(HostText::TracerTitle).to_owned(),
        sections: vec![section(
            Some(f(T::SectionPreview)),
            vec![
                preview_row(tracer::PREVIEW, 320, 322),
                note_row(tracer::SUMMARY, 2, StatusTone::Neutral, true),
                note_row(tracer::COMPUTING, 1, StatusTone::Neutral, false),
                note_row(tracer::SCALE_HINT, 2, StatusTone::Warning, false),
                actions_row(tracer::APPLY_LINKED, &[h(HostText::TracerApplyLinked)]),
            ],
        )],
        buttons: buttons((tracer::CLOSE, h(HostText::TracerClose)), Some((tracer::APPLY, h(HostText::TracerApply)))),
    };
    (left, right)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::form::{ButtonRole, Platform};

    const LOCALES: [Locale; 2] = [Locale::Nl, Locale::En];

    fn every_form(locale: Locale) -> Vec<(&'static str, FormSpec)> {
        let (adjust_left, adjust_right) = image_adjust_forms(locale);
        let (tracer_left, tracer_right) = tracer_forms(locale);
        vec![
            ("export", export_form(locale)),
            ("bundle", bundle_form(locale)),
            ("material", material_form(locale)),
            ("art", art_form(locale)),
            ("import", import_form(locale)),
            ("image-import", image_import_form(locale)),
            ("image-adjust left", adjust_left),
            ("image-adjust right", adjust_right),
            ("tracer left", tracer_left),
            ("tracer right", tracer_right),
        ]
    }

    fn suffix(spec: &FormSpec, id: u32) -> Option<&'static str> {
        spec.row(id).and_then(|r| r.suffix())
    }

    fn roles(spec: &FormSpec) -> Vec<ButtonRole> {
        spec.buttons.order(Platform::Mac).into_iter().map(|(r, _)| r).collect()
    }

    #[test]
    fn every_file_form_follows_the_form_rules() {
        for locale in LOCALES {
            for (name, spec) in every_form(locale) {
                assert!(!spec.sections.is_empty(), "{name}: empty form");
                assert_eq!(spec.problems(), Vec::new(), "{name} {locale:?}");
                for row in spec.sections.iter().flat_map(|s| &s.rows) {
                    assert!(!row.label.contains('('), "{name}: '{}' carries a unit or a range", row.label);
                    assert!(!row.label.contains('['), "{name}: '{}' carries a unit or a range", row.label);
                }
                let unheaded = spec.sections.iter().filter(|s| s.heading.is_none()).count();
                assert!(unheaded <= 1, "{name}: only the first section may go without a heading");
                assert!(
                    unheaded == 0 || spec.sections[0].heading.is_none(),
                    "{name}: the section without a heading is the first"
                );
            }
        }
    }

    #[test]
    fn the_export_dialog_has_its_unit_as_a_suffix_and_the_close_and_export_buttons() {
        let spec = export_form(Locale::Nl);
        assert_eq!(suffix(&spec, export::STROKE), Some("mm"));
        assert_eq!(suffix(&spec, export::TOLERANCE), Some("mm"));
        assert_eq!(spec.row(export::STROKE).map(|r| r.label.as_str()), Some("Lijndikte:"));
        assert_eq!(spec.buttons.cancel.as_ref().map(|b| b.id), Some(FormId(export::CLOSE)));
        assert_eq!(spec.buttons.primary.as_ref().map(|b| b.id), Some(FormId(export::EXPORT)));
        assert!(spec.row(export::LOSSES).is_some_and(|r| matches!(r.control, Control::Report { .. })));
        for id in [export::TITLE, export::UNIT, export::LAYERS, export::CURVES, export::DECIMALS, export::ACKNOWLEDGE] {
            assert!(spec.row(id).is_some(), "export row {id}");
        }
    }

    #[test]
    fn the_bundle_sheet_has_four_kinds_and_room_for_the_profiles_and_two_sections_the_host_swaps() {
        let spec = bundle_form(Locale::Nl);
        for i in 0..4 {
            assert!(
                spec.row(bundle::KIND + i).is_some_and(|r| matches!(r.control, Control::Check { .. })),
                "kind {i}"
            );
        }
        for i in 0..bundle::PROFILE_SLOTS as u32 {
            assert!(spec.row(bundle::PROFILE + i).is_some(), "profile {i}");
        }
        assert!(spec.row(bundle::KIND).is_some_and(|_| spec.sections[bundle::EXPORT_SECTION].rows.iter().any(|r| r.id == FormId(bundle::KIND))));
        assert!(spec.sections[bundle::IMPORT_SECTION].rows.iter().any(|r| r.id == FormId(bundle::PREVIEW)));
        assert_eq!(roles(&spec), vec![ButtonRole::Cancel, ButtonRole::Primary]);
    }

    #[test]
    fn the_material_panel_has_the_thickness_in_millimetres_as_a_suffix_in_both_places() {
        let spec = material_form(Locale::Nl);
        assert_eq!(suffix(&spec, material::THICKNESS), Some("mm"));
        assert_eq!(suffix(&spec, material::STORE_THICKNESS), Some("mm"));
        assert_eq!(spec.row(material::THICKNESS).map(|r| r.label.as_str()), Some("Dikte:"));
        for (base, n) in [(material::LAYER_ACTIONS, 3), (material::STORE_ACTIONS, 3)] {
            let row = spec.row(base).expect("actions row");
            assert!(matches!(&row.control, Control::Actions { titles } if titles.len() == n), "{base}");
        }
        // A floating panel is closed, not confirmed.
        assert_eq!(roles(&spec), vec![ButtonRole::Cancel]);
    }

    #[test]
    fn the_art_panel_has_a_128_point_thumbnail_and_the_manage_buttons_in_rows() {
        let spec = art_form(Locale::Nl);
        assert!(matches!(
            spec.row(art::THUMBNAIL).map(|r| &r.control),
            Some(Control::Preview { width: 128, height: 128 })
        ));
        for (base, n) in [(art::ADD, 2), (art::GROUP_ACTIONS, 3), (art::ITEM_ACTIONS, 3), (art::PLACE, 1)] {
            let row = spec.row(base).expect("actions row");
            assert!(matches!(&row.control, Control::Actions { titles } if titles.len() == n), "{base}");
        }
        assert!(spec.row(art::SCALE_UNIT).is_some_and(|r| matches!(&r.control, Control::Choice { options, .. } if options.len() == 3)));
        assert_eq!(roles(&spec), vec![ButtonRole::Cancel]);
    }

    #[test]
    fn the_import_sheet_asks_cancel_then_import() {
        let spec = import_form(Locale::Nl);
        for id in [import::FILE, import::UNITS, import::PAGES, import::LAYERS, import::REPORT] {
            assert!(spec.row(id).is_some(), "import row {id}");
        }
        assert_eq!(roles(&spec), vec![ButtonRole::Cancel, ButtonRole::Primary]);
        assert_eq!(spec.buttons.primary.as_ref().map(|b| b.id), Some(FormId(import::COMMIT)));
    }

    #[test]
    fn the_bitmap_import_resolution_is_in_dpi_as_a_suffix() {
        let spec = image_import_form(Locale::En);
        assert_eq!(suffix(&spec, image_import::DPI), Some("dpi"));
        assert_eq!(spec.row(image_import::DPI).map(|r| r.label.as_str()), Some("Resolution"));
        assert_eq!(roles(&spec), vec![ButtonRole::Cancel, ButtonRole::Primary]);
    }

    #[test]
    fn the_adjust_sheet_keeps_ranges_as_help_units_as_suffixes_and_the_buttons_on_the_right() {
        let (left, right) = image_adjust_forms(Locale::Nl);
        assert_eq!(left.row(image_adjust::VALUE).and_then(|r| r.help.as_deref()), Some("−1…1"));
        assert_eq!(left.row(image_adjust::VALUE).map(|r| r.label.as_str()), Some("Helderheid"));
        assert_eq!(suffix(&left, image_adjust::VALUE + 4), Some("mm"));
        for i in 0..4 {
            assert_eq!(suffix(&left, image_adjust::CROP + i), Some("px"), "crop {i}");
        }
        assert!(roles(&left).is_empty(), "the buttons are in the right form");
        assert_eq!(roles(&right), vec![ButtonRole::Cancel, ButtonRole::Primary]);
        assert!(matches!(
            right.row(image_adjust::PREVIEW).map(|r| &r.control),
            Some(Control::Preview { width: 320, height: 322 })
        ));
    }

    #[test]
    fn the_tracer_has_dpi_and_square_millimetres_as_suffixes_and_the_buttons_beside_the_preview() {
        let (left, right) = tracer_forms(Locale::Nl);
        assert_eq!(suffix(&left, tracer::DPI), Some("dpi"));
        assert_eq!(suffix(&left, tracer::IGNORE), Some("mm²"));
        assert_eq!(suffix(&left, tracer::SMOOTHNESS), Some("mm"));
        assert_eq!(left.row(tracer::THRESHOLD_VALUE).and_then(|r| r.help.as_deref()), Some("0…255"));
        assert!(roles(&left).is_empty());
        assert_eq!(roles(&right), vec![ButtonRole::Cancel, ButtonRole::Primary]);
        assert!(right.row(tracer::APPLY_LINKED).is_some_and(|r| matches!(&r.control, Control::Actions { titles } if titles.len() == 1)));
        assert_eq!(right.buttons.primary.as_ref().map(|b| b.id), Some(FormId(tracer::APPLY)));
    }
}
