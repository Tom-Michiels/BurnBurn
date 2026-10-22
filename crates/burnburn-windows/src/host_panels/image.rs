//! The Beeld panel on Windows (W4-L8b): bitmap import with the DPI proposal, Adjust Image with
//! its preview and crop fields, the tracer, and the Beeld commands (mask, replace, refresh, save
//! processed bitmap). The texts, the enablement and the intents come from the plain-data models
//! shared with the Mac host (`crate::panels_image`); this module gives them control ids.
//!
//! Numbers go on as typed (DR-05). Only one sheet is shown at a time, in this order: bitmap import,
//! Adjust Image, tracer.

use burnburn_presentation::{AppViewModel, CommandKey, MessageKey, UiIntent};
use burnburn_types::DocumentId;

use super::filters::{bitmap_filter, import_filters, png_filter, trace_filter};
use super::ids::IMAGE_CONTROLS;
use super::spec::notify;
use super::{Chosen, ControlSpec, DialogRequest, FilePurpose, PanelEvent, PanelReaction, PanelSpec, Row, command_on};
use crate::l10n::{HostText, Locale, host_text, message_or_key};
use crate::panels_image::{
    GRAY_METHODS, gray_method_name, image_adjust_intent, image_adjust_panel, image_dpi_intent, image_import_panel,
    suggested_bitmap_name, trace_form_intent, trace_preset_intent, trace_suggestion_intent, tracer_panel,
};

/// Control ids of the Beeld panel (`7200..7400`).
pub mod id {
    use super::IMAGE_CONTROLS as B;

    // Beeld commands (always shown).
    pub const IMPORT: u16 = B;
    pub const TRACE: u16 = B + 1;
    pub const ADJUST: u16 = B + 2;
    pub const MASK: u16 = B + 3;
    pub const REPLACE: u16 = B + 4;
    pub const REFRESH: u16 = B + 5;
    pub const SAVE_BITMAP: u16 = B + 6;
    pub const NO_SHEET: u16 = B + 7;

    // Bitmap import sheet.
    pub const IMP_HEADING: u16 = B + 10;
    pub const IMP_FILE: u16 = B + 11;
    pub const IMP_PIXELS: u16 = B + 12;
    pub const IMP_DPI_CAPTION: u16 = B + 13;
    pub const IMP_DPI: u16 = B + 14;
    pub const IMP_SUGGESTION: u16 = B + 15;
    pub const IMP_SIZE: u16 = B + 16;
    pub const IMP_ISSUE: u16 = B + 17;
    pub const IMP_COMMIT: u16 = B + 18;
    pub const IMP_CANCEL: u16 = B + 19;

    // Adjust Image sheet.
    pub const ADJ_HEADING: u16 = B + 20;
    pub const ADJ_SOURCE: u16 = B + 21;
    /// Captions of the nine fields: `+0..+8`.
    pub const ADJ_CAPTION: u16 = B + 22;
    /// The nine fields in `ADJUST_FIELDS` order: `+0..+8`.
    pub const ADJ_FIELD: u16 = B + 31;
    pub const ADJ_GRAY_CAPTION: u16 = B + 40;
    pub const ADJ_GRAY: u16 = B + 41;
    pub const ADJ_ISSUE: u16 = B + 42;
    pub const ADJ_COMPUTING: u16 = B + 43;
    pub const ADJ_PREVIEW: u16 = B + 44;
    pub const ADJ_APPLY: u16 = B + 45;
    pub const ADJ_CANCEL: u16 = B + 46;
    pub const ADJ_CROP_CAPTION: u16 = B + 47;

    // Tracer.
    pub const TR_HEADING: u16 = B + 50;
    pub const TR_SOURCE: u16 = B + 51;
    pub const TR_PRESET: u16 = B + 52;
    pub const TR_MODE: u16 = B + 53;
    pub const TR_DPI: u16 = B + 54;
    pub const TR_SUGGESTION: u16 = B + 55;
    pub const TR_THRESHOLD_CHOICE: u16 = B + 56;
    pub const TR_THRESHOLD: u16 = B + 57;
    pub const TR_IGNORE: u16 = B + 58;
    pub const TR_SMOOTHNESS: u16 = B + 59;
    pub const TR_INVERT: u16 = B + 60;
    pub const TR_CURVES: u16 = B + 61;
    pub const TR_SUMMARY: u16 = B + 62;
    pub const TR_COMPUTING: u16 = B + 63;
    pub const TR_HINT: u16 = B + 64;
    pub const TR_ISSUE: u16 = B + 65;
    pub const TR_PREVIEW: u16 = B + 66;
    pub const TR_APPLY: u16 = B + 67;
    pub const TR_CLOSE: u16 = B + 68;
    /// Captions of the tracer rows: `+0..+9`.
    pub const TR_CAPTION: u16 = B + 70;
}

/// What the Beeld panel is built from.
pub struct ImageInput<'a> {
    pub document: DocumentId,
    pub vm: &'a AppViewModel,
    pub locale: Locale,
}

/// Height of the preview pictures, in pixels at 96 dpi.
const ADJUST_PREVIEW_PX: i32 = 160;
const TRACE_PREVIEW_PX: i32 = 200;

fn ht(text: HostText, locale: Locale) -> &'static str {
    host_text(text, locale)
}

fn command_name(command: CommandKey, locale: Locale) -> String {
    message_or_key(MessageKey::Command(command), locale)
}

fn caption(id: u16, text: HostText, locale: Locale) -> ControlSpec {
    ControlSpec::label(id, ht(text, locale)).weighted(2)
}

/// The Beeld commands, always shown above the sheet.
fn command_rows(input: &ImageInput<'_>) -> Vec<Row> {
    let (vm, doc, locale) = (input.vm, input.document, input.locale);
    let open = vm.document(doc).is_some();
    let cmd = |id: u16, command: CommandKey| {
        ControlSpec::button(id, command_name(command, locale), command_on(vm, doc, command))
    };
    vec![
        Row::of(vec![
            ControlSpec::button(id::IMPORT, ht(HostText::ImageMenuImport, locale), open),
            ControlSpec::button(id::TRACE, ht(HostText::ImageMenuTrace, locale), open),
        ]),
        Row::of(vec![
            cmd(id::ADJUST, CommandKey::AdjustImage),
            cmd(id::MASK, CommandKey::MaskImage),
            cmd(id::REPLACE, CommandKey::ReplaceImage),
        ]),
        Row::of(vec![
            cmd(id::REFRESH, CommandKey::RefreshImage),
            cmd(id::SAVE_BITMAP, CommandKey::SaveProcessedBitmap),
        ]),
    ]
}

fn labeled_edit(caption_id: u16, caption: &str, edit_id: u16, text: &str, enabled: bool) -> Row {
    Row::of(vec![ControlSpec::label(caption_id, caption).weighted(2), ControlSpec::edit(edit_id, text, enabled)])
}

pub fn image_spec(input: &ImageInput<'_>) -> PanelSpec {
    let locale = input.locale;
    let mut rows = command_rows(input);
    let on = |command: CommandKey| command_on(input.vm, input.document, command);
    let title = ht(HostText::ImageMenu, locale).to_owned();
    let Some(doc) = input.vm.document(input.document) else {
        return PanelSpec { title, rows };
    };
    if let Some(import) = &doc.image.import {
        let p = image_import_panel(import, locale);
        rows.push(Row::one(ControlSpec::heading(id::IMP_HEADING, ht(HostText::ImageImportTitle, locale))));
        rows.push(Row::one(ControlSpec::label(id::IMP_FILE, p.file.clone())));
        rows.push(Row::one(ControlSpec::label(id::IMP_PIXELS, p.pixels.clone())));
        rows.push(labeled_edit(id::IMP_DPI_CAPTION, &p.dpi_caption, id::IMP_DPI, &p.dpi_text, true));
        rows.push(Row::one(ControlSpec::label(id::IMP_SUGGESTION, p.suggestion.clone())));
        rows.push(Row::one(ControlSpec::label(id::IMP_SIZE, p.size.clone())));
        rows.push(Row::one(ControlSpec::label(id::IMP_ISSUE, p.issue.clone())));
        rows.push(Row::of(vec![
            ControlSpec::button(
                id::IMP_COMMIT,
                ht(HostText::ImageImportCommit, locale),
                p.commit_enabled && on(CommandKey::CommitImport),
            ),
            ControlSpec::button(id::IMP_CANCEL, ht(HostText::ImageImportCancel, locale), on(CommandKey::CancelImport)),
        ]));
    } else if let Some(adjust) = &doc.image.adjust {
        let p = image_adjust_panel(adjust, locale);
        rows.push(Row::one(ControlSpec::heading(
            id::ADJ_HEADING,
            format!("{} — {}", ht(HostText::ImageAdjustTitle, locale), p.title),
        )));
        rows.push(Row::one(ControlSpec::label(id::ADJ_SOURCE, p.source.clone())));
        let captions = [
            HostText::ImageAdjustBrightness,
            HostText::ImageAdjustContrast,
            HostText::ImageAdjustGamma,
            HostText::ImageAdjustSharpenAmount,
            HostText::ImageAdjustSharpenRadius,
            HostText::ImageAdjustCropX,
            HostText::ImageAdjustCropY,
            HostText::ImageAdjustCropWidth,
            HostText::ImageAdjustCropHeight,
        ];
        for (i, text) in captions.iter().enumerate() {
            let n = i as u16;
            if i == 5 {
                rows.push(Row::one(ControlSpec::heading(id::ADJ_CROP_CAPTION, ht(HostText::ImageAdjustCrop, locale))));
            }
            rows.push(labeled_edit(id::ADJ_CAPTION + n, ht(*text, locale), id::ADJ_FIELD + n, &p.fields[i], true));
        }
        rows.push(Row::of(vec![
            caption(id::ADJ_GRAY_CAPTION, HostText::ImageAdjustGray, locale),
            ControlSpec::combo(
                id::ADJ_GRAY,
                GRAY_METHODS.iter().map(|g| gray_method_name(*g, locale).to_owned()).collect(),
                Some(p.gray_index),
                true,
            ),
        ]));
        rows.push(Row::one(ControlSpec::label(id::ADJ_ISSUE, p.issue.clone())));
        rows.push(Row::one(ControlSpec::label(id::ADJ_COMPUTING, p.computing.clone())));
        rows.push(Row::one(ControlSpec::bitmap(id::ADJ_PREVIEW, ADJUST_PREVIEW_PX, p.preview.clone())));
        rows.push(Row::of(vec![
            ControlSpec::button(
                id::ADJ_APPLY,
                ht(HostText::ImageAdjustApply, locale),
                p.apply_enabled && on(CommandKey::ApplyImageAdjust),
            ),
            ControlSpec::button(
                id::ADJ_CANCEL,
                ht(HostText::ImageAdjustCancel, locale),
                on(CommandKey::CancelImageAdjust),
            ),
        ]));
    } else if let Some(tracer) = &doc.tracer {
        let p = tracer_panel(tracer, locale);
        rows.push(Row::one(ControlSpec::heading(id::TR_HEADING, ht(HostText::TracerTitle, locale))));
        rows.push(Row::one(ControlSpec::label(id::TR_SOURCE, p.source.clone())));
        rows.push(Row::of(vec![
            caption(id::TR_CAPTION, HostText::TracerRecipe, locale),
            ControlSpec::combo(id::TR_PRESET, p.preset_titles.clone(), p.preset_index, true),
        ]));
        rows.push(Row::of(vec![
            caption(id::TR_CAPTION + 1, HostText::TracerMode, locale),
            ControlSpec::combo(
                id::TR_MODE,
                vec![
                    ht(HostText::TracerModeContour, locale).to_owned(),
                    ht(HostText::TracerModeCenterline, locale).to_owned(),
                ],
                Some(p.mode_index),
                true,
            ),
        ]));
        rows.push(labeled_edit(id::TR_CAPTION + 2, ht(HostText::TracerDpi, locale), id::TR_DPI, &p.fields[0], true));
        rows.push(Row::one(
            ControlSpec::button(id::TR_SUGGESTION, p.suggestion_button.clone().unwrap_or_default(), true)
                .hidden_if(p.suggestion_button.is_none()),
        ));
        rows.push(Row::of(vec![
            caption(id::TR_CAPTION + 3, HostText::TracerThreshold, locale),
            ControlSpec::combo(
                id::TR_THRESHOLD_CHOICE,
                vec![
                    ht(HostText::TracerThresholdOtsu, locale).to_owned(),
                    ht(HostText::TracerThresholdFixed, locale).to_owned(),
                    ht(HostText::TracerThresholdAdaptive, locale).to_owned(),
                ],
                Some(p.threshold_index),
                true,
            ),
        ]));
        rows.push(labeled_edit(
            id::TR_CAPTION + 4,
            ht(HostText::TracerThresholdValue, locale),
            id::TR_THRESHOLD,
            &p.fields[1],
            p.threshold_enabled,
        ));
        rows.push(labeled_edit(
            id::TR_CAPTION + 5,
            ht(HostText::TracerIgnore, locale),
            id::TR_IGNORE,
            &p.fields[2],
            p.ignore_enabled,
        ));
        rows.push(labeled_edit(
            id::TR_CAPTION + 6,
            ht(HostText::TracerSmoothness, locale),
            id::TR_SMOOTHNESS,
            &p.fields[3],
            true,
        ));
        rows.push(Row::one(ControlSpec::check(id::TR_INVERT, ht(HostText::TracerInvert, locale), p.invert, true)));
        rows.push(Row::one(ControlSpec::check(id::TR_CURVES, ht(HostText::TracerCurves, locale), p.curves, true)));
        rows.push(Row::one(ControlSpec::label(id::TR_SUMMARY, p.summary.clone())));
        rows.push(Row::one(ControlSpec::label(id::TR_COMPUTING, p.computing.clone())));
        rows.push(Row::one(ControlSpec::label(id::TR_HINT, p.scale_hint.clone())));
        rows.push(Row::one(ControlSpec::label(id::TR_ISSUE, p.issue.clone())));
        rows.push(Row::one(ControlSpec::bitmap(id::TR_PREVIEW, TRACE_PREVIEW_PX, p.preview.clone())));
        rows.push(Row::of(vec![
            ControlSpec::button(id::TR_APPLY, ht(HostText::TracerApply, locale), on(CommandKey::ApplyTrace)),
            ControlSpec::button(id::TR_CLOSE, ht(HostText::TracerClose, locale), on(CommandKey::CloseTracer)),
        ]));
    } else {
        rows.push(Row::one(ControlSpec::label(id::NO_SHEET, ht(HostText::ImageNoPreview, locale))));
    }
    PanelSpec { title, rows }
}

fn invoke(input: &ImageInput<'_>, command: CommandKey) -> PanelReaction {
    if command_on(input.vm, input.document, command) {
        PanelReaction::Intent(UiIntent::Invoke { document: Some(input.document), command })
    } else {
        PanelReaction::Nothing
    }
}

fn intent(i: Option<UiIntent>) -> PanelReaction {
    i.map_or(PanelReaction::Nothing, PanelReaction::Intent)
}

pub fn on_image_event(input: &ImageInput<'_>, event: &PanelEvent) -> PanelReaction {
    let (vm, document, locale) = (input.vm, input.document, input.locale);
    let Some(doc) = vm.document(document) else { return PanelReaction::Nothing };
    let (cid, code) = (event.id, event.code);
    let commit = matches!(code, notify::ENTER | notify::EDIT_DONE);
    let clicked = code == notify::CLICKED;
    let open = |purpose: FilePurpose, title: HostText, filters| {
        PanelReaction::Dialog(DialogRequest::Open { purpose, title: ht(title, locale).to_owned(), filters })
    };
    match cid {
        id::IMPORT if clicked => {
            open(FilePurpose::ImportDrawing { document }, HostText::ImageMenuImport, import_filters(true))
        }
        id::TRACE if clicked => {
            open(FilePurpose::TraceImage { document }, HostText::ImageMenuTrace, vec![trace_filter()])
        }
        id::REPLACE if clicked => {
            if command_on(vm, document, CommandKey::ReplaceImage) {
                open(FilePurpose::ReplaceImage { document }, HostText::ImageMenuImport, vec![bitmap_filter()])
            } else {
                PanelReaction::Nothing
            }
        }
        id::SAVE_BITMAP if clicked => {
            if command_on(vm, document, CommandKey::SaveProcessedBitmap) {
                PanelReaction::Dialog(DialogRequest::Save {
                    purpose: FilePurpose::SaveProcessedBitmap { document },
                    title: command_name(CommandKey::SaveProcessedBitmap, locale),
                    suggested_name: suggested_bitmap_name(&doc.title),
                    filters: vec![png_filter()],
                })
            } else {
                PanelReaction::Nothing
            }
        }
        id::ADJUST if clicked => invoke(input, CommandKey::AdjustImage),
        id::MASK if clicked => invoke(input, CommandKey::MaskImage),
        id::REFRESH if clicked => invoke(input, CommandKey::RefreshImage),
        id::IMP_COMMIT if clicked => {
            let ready = doc.image.import.as_ref().is_some_and(|i| image_import_panel(i, locale).commit_enabled);
            if ready { invoke(input, CommandKey::CommitImport) } else { PanelReaction::Nothing }
        }
        id::IMP_CANCEL if clicked => invoke(input, CommandKey::CancelImport),
        id::IMP_DPI if commit => {
            let Some(shown) = doc.image.import.as_ref() else { return PanelReaction::Nothing };
            intent(image_dpi_intent(document, shown, event.values.text(id::IMP_DPI).to_owned()))
        }
        id::ADJ_APPLY if clicked => invoke(input, CommandKey::ApplyImageAdjust),
        id::ADJ_CANCEL if clicked => invoke(input, CommandKey::CancelImageAdjust),
        c if (commit && (id::ADJ_FIELD..id::ADJ_FIELD + 9).contains(&c))
            || (c == id::ADJ_GRAY && code == notify::CHANGED) =>
        {
            let Some(shown) = doc.image.adjust.as_ref() else { return PanelReaction::Nothing };
            let fields: [String; 9] = std::array::from_fn(|i| event.values.text(id::ADJ_FIELD + i as u16).to_owned());
            intent(image_adjust_intent(document, shown, fields, event.values.selected(id::ADJ_GRAY)))
        }
        id::TR_APPLY if clicked => invoke(input, CommandKey::ApplyTrace),
        id::TR_CLOSE if clicked => invoke(input, CommandKey::CloseTracer),
        id::TR_PRESET if code == notify::CHANGED => {
            if doc.tracer.is_some() {
                intent(trace_preset_intent(document, event.values.selected(id::TR_PRESET)))
            } else {
                PanelReaction::Nothing
            }
        }
        id::TR_SUGGESTION if clicked => {
            let Some(shown) = doc.tracer.as_ref() else { return PanelReaction::Nothing };
            intent(trace_suggestion_intent(document, &shown.form))
        }
        c if (commit && [id::TR_DPI, id::TR_THRESHOLD, id::TR_IGNORE, id::TR_SMOOTHNESS].contains(&c))
            || (code == notify::CHANGED && (c == id::TR_MODE || c == id::TR_THRESHOLD_CHOICE))
            || (clicked && (c == id::TR_INVERT || c == id::TR_CURVES)) =>
        {
            let Some(shown) = doc.tracer.as_ref() else { return PanelReaction::Nothing };
            let v = &event.values;
            let fields: [String; 4] = [
                v.text(id::TR_DPI).to_owned(),
                v.text(id::TR_THRESHOLD).to_owned(),
                v.text(id::TR_IGNORE).to_owned(),
                v.text(id::TR_SMOOTHNESS).to_owned(),
            ];
            intent(trace_form_intent(
                document,
                &shown.form,
                fields,
                v.selected(id::TR_MODE),
                v.selected(id::TR_THRESHOLD_CHOICE),
                v.checked(id::TR_INVERT),
                v.checked(id::TR_CURVES),
            ))
        }
        _ => PanelReaction::Nothing,
    }
}

/// A file dialog of this family closed (cancel, or a window that is gone, sends nothing).
pub fn on_image_file_chosen(vm: &AppViewModel, purpose: &FilePurpose, path: Chosen) -> PanelReaction {
    let Some(path) = path else { return PanelReaction::Nothing };
    let (document, build): (DocumentId, fn(DocumentId, std::path::PathBuf) -> UiIntent) = match purpose {
        FilePurpose::ImportDrawing { document } => (*document, |document, path| UiIntent::Import { document, path }),
        FilePurpose::ReplaceImage { document } => {
            (*document, |document, path| UiIntent::ReplaceImage { document, path })
        }
        FilePurpose::TraceImage { document } => {
            (*document, |document, path| UiIntent::OpenTraceSource { document, path })
        }
        FilePurpose::SaveProcessedBitmap { document } => {
            (*document, |document, path| UiIntent::SaveProcessedBitmap { document, path })
        }
        _ => return PanelReaction::Nothing,
    };
    if vm.document(document).is_none() {
        return PanelReaction::Nothing;
    }
    PanelReaction::Intent(build(document, path))
}
