//! W4-L8b family 2, headless: the Windows Beeld panel shows the bitmap import sheet (with the DPI
//! proposal), Adjust Image (with preview and crop), the tracer and the Beeld commands from the
//! presentation's view models, and every edit becomes one intent with the typed text unparsed.
//! The view models are hand-built (no controller behaviour is assumed); no dialog is shown.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use burnburn_document::process::GrayMethod;
use burnburn_geometry::{Path, Point, Rect};
use burnburn_presentation::{
    AppViewModel, CommandKey, CommandState, HeadlessEditorHarness, ImageAdjustInput, ImageAdjustViewModel,
    ImageImportViewModel, ImageIssue, ImageField, ImagePreview, MessageKey, ThresholdChoice, TraceFormInput, TraceFormViewModel,
    TraceMode, TracePreset, TraceSummary, TracerViewModel, UiIntent,
};
use burnburn_types::{DocumentId, Length, OperationId, StableId};
use burnburn_windows::host_panels::image::{
    ImageInput, id, image_spec, on_image_event, on_image_file_chosen,
};
use burnburn_windows::host_panels::{
    ControlKind, DialogRequest, FilePurpose, PanelEvent, PanelReaction, PanelSpec, PanelValues, notify,
};
use burnburn_windows::l10n::Locale;
use common::{click, event, texts};

fn operation() -> OperationId {
    <OperationId as StableId>::from_raw(7)
}

fn adjust_input() -> ImageAdjustInput {
    ImageAdjustInput {
        brightness: "0".into(),
        contrast: "1".into(),
        gamma: "1".into(),
        sharpen_amount: "0".into(),
        sharpen_radius: "0,3".into(),
        gray: GrayMethod::Rec709,
        crop_x: String::new(),
        crop_y: String::new(),
        crop_width: String::new(),
        crop_height: String::new(),
    }
}

fn adjust_model() -> ImageAdjustViewModel {
    ImageAdjustViewModel {
        operation: operation(),
        title: "Foto".into(),
        source_px: Some((1200, 800)),
        input: adjust_input(),
        issue: None,
        computing: false,
        preview: None,
        can_apply: true,
    }
}

fn import_model() -> ImageImportViewModel {
    ImageImportViewModel {
        file: "foto.png".into(),
        width_px: 1200,
        height_px: 800,
        dpi_suggestion: Some(300.0),
        dpi_text: "300".into(),
        size_mm: Some((101.6, 67.733_333)),
        issue: None,
    }
}

fn trace_input() -> TraceFormInput {
    TraceFormInput {
        mode: TraceMode::Contour,
        dpi: "300".into(),
        threshold_choice: ThresholdChoice::Fixed,
        threshold: "128".into(),
        invert: false,
        ignore_below_mm2: "0,5".into(),
        smoothness_mm: "0,05".into(),
        curves: true,
    }
}

fn tracer_model() -> TracerViewModel {
    let rect =
        Rect::new(Point::new(0.0, 0.0), Length::from_mm(40.0).unwrap(), Length::from_mm(25.0).unwrap()).unwrap();
    TracerViewModel {
        width_px: 800,
        height_px: 600,
        needs_scale: false,
        computing: false,
        result: Some(TraceSummary {
            components: 3,
            holes: 1,
            width_mm: 40.0,
            height_mm: 25.0,
            mm_per_pixel: (0.0847, 0.0847),
            fit_tolerance_mm: 0.05,
            preview: Arc::new(Path::rectangle(rect)),
        }),
        form: TraceFormViewModel {
            input: trace_input(),
            preset: Some(TracePreset::Logo),
            issue: None,
            dpi_suggestion: Some(300.0),
        },
    }
}

/// The real headless view model of a new document with the given image state injected.
fn vm_with(f: impl FnOnce(&mut burnburn_presentation::DocumentViewModel)) -> (AppViewModel, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let mut vm = (*h.view()).clone();
    let d = vm.documents.iter_mut().find(|d| d.id == doc).unwrap();
    f(d);
    (vm, doc)
}

fn enable(vm: &mut AppViewModel, commands: &[CommandKey]) {
    for c in commands {
        vm.commands.insert(*c, CommandState { enabled: true, checked: false, label: MessageKey::Command(*c) });
    }
}

fn spec(vm: &AppViewModel, document: DocumentId) -> PanelSpec {
    image_spec(&ImageInput { document, vm, locale: Locale::Nl })
}

fn react(vm: &AppViewModel, document: DocumentId, ev: PanelEvent) -> PanelReaction {
    on_image_event(&ImageInput { document, vm, locale: Locale::Nl }, &ev)
}

fn text(spec: &PanelSpec, id: u16) -> String {
    spec.control(id).unwrap_or_else(|| panic!("control {id} missing")).text.clone()
}

fn only_intent(reaction: PanelReaction) -> UiIntent {
    match reaction {
        PanelReaction::Intent(i) => i,
        other => panic!("expected one intent, got {other:?}"),
    }
}

// ---- layout ------------------------------------------------------------------------------------

#[test]
fn every_sheet_has_unique_ids_inside_the_image_block() {
    let sheets: [Box<dyn FnOnce(&mut burnburn_presentation::DocumentViewModel)>; 4] = [
        Box::new(|_| {}),
        Box::new(|d| d.image.import = Some(import_model())),
        Box::new(|d| d.image.adjust = Some(adjust_model())),
        Box::new(|d| d.tracer = Some(tracer_model())),
    ];
    for f in sheets {
        let (vm, doc) = vm_with(f);
        let mut seen = HashSet::new();
        for c in spec(&vm, doc).controls() {
            assert!((7200..7400).contains(&c.id), "{} outside 7200..7400", c.id);
            assert!(seen.insert(c.id), "duplicate id {}", c.id);
        }
    }
}

// ---- the Beeld commands ------------------------------------------------------------------------

#[test]
fn without_an_open_sheet_the_panel_offers_the_beeld_commands_by_their_enablement() {
    let (mut vm, doc) = vm_with(|_| {});
    enable(&mut vm, &[CommandKey::AdjustImage, CommandKey::RefreshImage]);
    let s = spec(&vm, doc);
    assert!(s.control(id::IMPORT).unwrap().enabled, "import needs only a window");
    assert!(s.control(id::TRACE).unwrap().enabled);
    assert!(s.control(id::ADJUST).unwrap().enabled);
    assert!(s.control(id::REFRESH).unwrap().enabled);
    assert!(!s.control(id::MASK).unwrap().enabled, "no shape/image selected: command is off");
    assert!(!s.control(id::REPLACE).unwrap().enabled);
    assert!(!s.control(id::SAVE_BITMAP).unwrap().enabled);
    assert!(!text(&s, id::NO_SHEET).is_empty(), "the empty state says what to do");
    // A command that is off sends nothing.
    assert_eq!(react(&vm, doc, click(id::MASK, PanelValues::default())), PanelReaction::Nothing);
    assert_eq!(
        only_intent(react(&vm, doc, click(id::ADJUST, PanelValues::default()))),
        UiIntent::Invoke { document: Some(doc), command: CommandKey::AdjustImage }
    );
    assert_eq!(
        only_intent(react(&vm, doc, click(id::REFRESH, PanelValues::default()))),
        UiIntent::Invoke { document: Some(doc), command: CommandKey::RefreshImage }
    );
}

#[test]
fn import_trace_replace_and_save_ask_for_a_file_first_with_the_right_types() {
    let (mut vm, doc) = vm_with(|_| {});
    enable(&mut vm, &[CommandKey::ReplaceImage, CommandKey::SaveProcessedBitmap]);
    match react(&vm, doc, click(id::IMPORT, PanelValues::default())) {
        PanelReaction::Dialog(DialogRequest::Open { purpose, filters, .. }) => {
            assert_eq!(purpose, FilePurpose::ImportDrawing { document: doc });
            assert_eq!(filters[0].extensions, vec!["png", "jpg", "jpeg", "bmp", "tif", "tiff"], "bitmap types first");
        }
        other => panic!("{other:?}"),
    }
    match react(&vm, doc, click(id::TRACE, PanelValues::default())) {
        PanelReaction::Dialog(DialogRequest::Open { purpose, filters, .. }) => {
            assert_eq!(purpose, FilePurpose::TraceImage { document: doc });
            assert_eq!(filters[0].extensions, vec!["png", "jpg", "jpeg"]);
        }
        other => panic!("{other:?}"),
    }
    match react(&vm, doc, click(id::REPLACE, PanelValues::default())) {
        PanelReaction::Dialog(DialogRequest::Open { purpose, .. }) => {
            assert_eq!(purpose, FilePurpose::ReplaceImage { document: doc });
        }
        other => panic!("{other:?}"),
    }
    match react(&vm, doc, click(id::SAVE_BITMAP, PanelValues::default())) {
        PanelReaction::Dialog(DialogRequest::Save { purpose, suggested_name, filters, .. }) => {
            assert_eq!(purpose, FilePurpose::SaveProcessedBitmap { document: doc });
            // A never-saved document is named by its "untitled" title; the bitmap gets `.png`.
            assert_eq!(suggested_name, format!("{}.png", vm.document(doc).unwrap().title));
            assert_eq!(filters[0].extensions, vec!["png"]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_chosen_file_becomes_the_matching_intent_and_cancel_or_a_closed_window_sends_nothing() {
    let (vm, doc) = vm_with(|_| {});
    let path = PathBuf::from("C:/foto.png");
    let chosen = |purpose: FilePurpose, p: Option<PathBuf>| on_image_file_chosen(&vm, &purpose, p);
    assert_eq!(
        chosen(FilePurpose::ImportDrawing { document: doc }, Some(path.clone())),
        PanelReaction::Intent(UiIntent::Import { document: doc, path: path.clone() })
    );
    assert_eq!(
        chosen(FilePurpose::ReplaceImage { document: doc }, Some(path.clone())),
        PanelReaction::Intent(UiIntent::ReplaceImage { document: doc, path: path.clone() })
    );
    assert_eq!(
        chosen(FilePurpose::TraceImage { document: doc }, Some(path.clone())),
        PanelReaction::Intent(UiIntent::OpenTraceSource { document: doc, path: path.clone() })
    );
    assert_eq!(
        chosen(FilePurpose::SaveProcessedBitmap { document: doc }, Some(path.clone())),
        PanelReaction::Intent(UiIntent::SaveProcessedBitmap { document: doc, path: path.clone() })
    );
    assert_eq!(chosen(FilePurpose::TraceImage { document: doc }, None), PanelReaction::Nothing);
    let gone = DocumentId::from_raw(4242);
    assert_eq!(chosen(FilePurpose::ReplaceImage { document: gone }, Some(path)), PanelReaction::Nothing);
}

// ---- bitmap import sheet -----------------------------------------------------------------------

#[test]
fn the_import_sheet_shows_the_dpi_proposal_and_commits_only_with_a_known_size() {
    let (mut vm, doc) = vm_with(|d| d.image.import = Some(import_model()));
    enable(&mut vm, &[CommandKey::CommitImport, CommandKey::CancelImport]);
    let s = spec(&vm, doc);
    assert_eq!(text(&s, id::IMP_FILE), "foto.png");
    assert_eq!(text(&s, id::IMP_PIXELS), "1200 × 800 pixels");
    assert_eq!(text(&s, id::IMP_DPI), "300");
    assert_eq!(text(&s, id::IMP_SUGGESTION), "Resolutie uit het bestand: 300 dpi");
    assert_eq!(text(&s, id::IMP_SIZE), "Afmeting: 101,6 × 67,7 mm");
    assert!(s.control(id::IMP_COMMIT).unwrap().enabled);

    // No resolution in the file: nothing is assumed, the proposal is the 254 dpi default and
    // Importeer stays off until a value is given.
    let (mut vm, doc) = vm_with(|d| {
        d.image.import = Some(ImageImportViewModel {
            dpi_suggestion: None,
            dpi_text: String::new(),
            size_mm: None,
            issue: Some(ImageIssue::InvalidNumber(ImageField::Dpi)),
            ..import_model()
        })
    });
    enable(&mut vm, &[CommandKey::CommitImport, CommandKey::CancelImport]);
    let s = spec(&vm, doc);
    assert!(!s.control(id::IMP_COMMIT).unwrap().enabled);
    assert!(s.control(id::IMP_CANCEL).unwrap().enabled, "cancel always works");
    assert!(!text(&s, id::IMP_ISSUE).is_empty(), "the refusal is in words");
    assert_eq!(react(&vm, doc, click(id::IMP_COMMIT, PanelValues::default())), PanelReaction::Nothing);
}

#[test]
fn a_typed_dpi_is_sent_unparsed_once_and_commit_and_cancel_invoke_their_commands() {
    let (mut vm, doc) = vm_with(|d| d.image.import = Some(import_model()));
    enable(&mut vm, &[CommandKey::CommitImport, CommandKey::CancelImport]);
    // Return, then the focus loss: the same text must not be sent twice.
    let typed = texts(&[(id::IMP_DPI, "254,5")]);
    assert_eq!(
        only_intent(react(&vm, doc, event(id::IMP_DPI, notify::ENTER, typed.clone()))),
        UiIntent::SetImageDpi { document: doc, dpi: "254,5".into() }
    );
    let same = texts(&[(id::IMP_DPI, "300")]);
    assert_eq!(react(&vm, doc, event(id::IMP_DPI, notify::EDIT_DONE, same)), PanelReaction::Nothing);
    assert_eq!(
        only_intent(react(&vm, doc, click(id::IMP_COMMIT, PanelValues::default()))),
        UiIntent::Invoke { document: Some(doc), command: CommandKey::CommitImport }
    );
    assert_eq!(
        only_intent(react(&vm, doc, click(id::IMP_CANCEL, PanelValues::default()))),
        UiIntent::Invoke { document: Some(doc), command: CommandKey::CancelImport }
    );
}

// ---- Adjust Image ------------------------------------------------------------------------------

#[test]
fn adjust_shows_typed_values_gray_method_issue_and_the_preview_bitmap() {
    let mut m = adjust_model();
    m.input.gamma = "2,2".into();
    m.input.gray = GrayMethod::Lightness;
    m.input.crop_x = "10".into();
    m.computing = true;
    m.issue = Some((ImageField::Gamma, ImageIssue::OutOfRange(ImageField::Gamma)));
    m.preview = Some(ImagePreview { width: 2, height: 1, gray8: Arc::from(vec![0u8, 255u8]) });
    let (mut vm, doc) = vm_with(|d| d.image.adjust = Some(m));
    enable(&mut vm, &[CommandKey::ApplyImageAdjust, CommandKey::CancelImageAdjust]);
    let s = spec(&vm, doc);
    assert_eq!(text(&s, id::ADJ_SOURCE), "Bron: 1200 × 800 pixels");
    assert_eq!(text(&s, id::ADJ_FIELD + 2), "2,2", "the third field is the gamma");
    assert_eq!(text(&s, id::ADJ_FIELD + 5), "10", "the sixth is the crop x");
    match &s.control(id::ADJ_GRAY).unwrap().kind {
        ControlKind::Combo { items, selected } => {
            assert_eq!(items.len(), 7);
            assert_eq!(*selected, Some(3));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(text(&s, id::ADJ_ISSUE), "Waarde buiten bereik (Gamma)");
    assert_eq!(text(&s, id::ADJ_COMPUTING), "berekenen…");
    match &s.control(id::ADJ_PREVIEW).unwrap().kind {
        ControlKind::Bitmap { image: Some(img), .. } => {
            assert_eq!((img.width, img.height), (2, 1));
            assert_eq!(img.rgba, vec![0, 0, 0, 255, 255, 255, 255, 255]);
        }
        other => panic!("{other:?}"),
    }
    assert!(s.control(id::ADJ_APPLY).unwrap().enabled);
}

#[test]
fn an_edited_adjust_field_sends_every_field_as_typed_and_an_unchanged_one_nothing() {
    let (mut vm, doc) = vm_with(|d| d.image.adjust = Some(adjust_model()));
    enable(&mut vm, &[CommandKey::ApplyImageAdjust, CommandKey::CancelImageAdjust]);
    let shown = adjust_input();
    let mut values = PanelValues::default();
    let fields = [&shown.brightness, &shown.contrast, "2,5", &shown.sharpen_amount, &shown.sharpen_radius, "10", "20", "300", "200"];
    for (i, text) in fields.iter().enumerate() {
        values.texts.insert(id::ADJ_FIELD + i as u16, (*text).to_owned());
    }
    values.selection.insert(id::ADJ_GRAY, Some(3));
    let intent = only_intent(react(&vm, doc, event(id::ADJ_FIELD + 2, notify::EDIT_DONE, values.clone())));
    match intent {
        UiIntent::SetImageAdjust { document, input } => {
            assert_eq!(document, doc);
            assert_eq!(input.gamma, "2,5");
            assert_eq!(input.gray, GrayMethod::Lightness);
            assert_eq!((input.crop_x.as_str(), input.crop_y.as_str()), ("10", "20"));
            assert_eq!((input.crop_width.as_str(), input.crop_height.as_str()), ("300", "200"));
        }
        other => panic!("{other:?}"),
    }
    // The gray pop-up alone also sends the whole form.
    assert!(matches!(
        react(&vm, doc, event(id::ADJ_GRAY, notify::CHANGED, values)),
        PanelReaction::Intent(UiIntent::SetImageAdjust { .. })
    ));
    // Everything as shown (Return then focus loss): nothing.
    let mut same = PanelValues::default();
    let shown_fields = [
        &shown.brightness, &shown.contrast, &shown.gamma, &shown.sharpen_amount, &shown.sharpen_radius,
        &shown.crop_x, &shown.crop_y, &shown.crop_width, &shown.crop_height,
    ];
    for (i, text) in shown_fields.iter().enumerate() {
        same.texts.insert(id::ADJ_FIELD + i as u16, (*text).to_owned());
    }
    same.selection.insert(id::ADJ_GRAY, Some(0));
    assert_eq!(react(&vm, doc, event(id::ADJ_FIELD, notify::EDIT_DONE, same)), PanelReaction::Nothing);
    assert_eq!(
        only_intent(react(&vm, doc, click(id::ADJ_APPLY, PanelValues::default()))),
        UiIntent::Invoke { document: Some(doc), command: CommandKey::ApplyImageAdjust }
    );
    assert_eq!(
        only_intent(react(&vm, doc, click(id::ADJ_CANCEL, PanelValues::default()))),
        UiIntent::Invoke { document: Some(doc), command: CommandKey::CancelImageAdjust }
    );
}

// ---- tracer ------------------------------------------------------------------------------------

#[test]
fn the_tracer_shows_its_form_summary_and_preview_and_greys_out_what_does_not_apply() {
    let mut t = tracer_model();
    t.form.input.mode = TraceMode::Centerline;
    t.form.input.threshold_choice = ThresholdChoice::Otsu;
    let (mut vm, doc) = vm_with(|d| d.tracer = Some(t));
    enable(&mut vm, &[CommandKey::ApplyTrace, CommandKey::CloseTracer]);
    let s = spec(&vm, doc);
    assert_eq!(text(&s, id::TR_DPI), "300");
    assert_eq!(text(&s, id::TR_SUGGESTION), "Gebruik voorstel (300 dpi)");
    assert!(!s.control(id::TR_IGNORE).unwrap().enabled, "no 'ignore less than' on centre lines");
    assert!(!s.control(id::TR_THRESHOLD).unwrap().enabled, "the value applies to the fixed threshold only");
    assert!(!text(&s, id::TR_SUMMARY).is_empty());
    match &s.control(id::TR_PRESET).unwrap().kind {
        ControlKind::Combo { items, selected } => {
            assert_eq!(items.len(), TracePreset::ALL.len());
            assert_eq!(*selected, Some(0));
        }
        other => panic!("{other:?}"),
    }
    match &s.control(id::TR_PREVIEW).unwrap().kind {
        ControlKind::Bitmap { image, .. } => assert!(image.is_some(), "the outline is drawn"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn tracer_edits_presets_and_the_suggestion_become_their_intents() {
    let (mut vm, doc) = vm_with(|d| d.tracer = Some(tracer_model()));
    enable(&mut vm, &[CommandKey::ApplyTrace, CommandKey::CloseTracer]);
    let mut values = PanelValues::default();
    for (cid, text) in [(id::TR_DPI, "600"), (id::TR_THRESHOLD, "100"), (id::TR_IGNORE, "0,5"), (id::TR_SMOOTHNESS, "0,05")] {
        values.texts.insert(cid, text.to_owned());
    }
    values.selection.insert(id::TR_MODE, Some(1));
    values.selection.insert(id::TR_THRESHOLD_CHOICE, Some(2));
    values.checks.insert(id::TR_INVERT, true);
    values.checks.insert(id::TR_CURVES, false);
    match only_intent(react(&vm, doc, click(id::TR_INVERT, values.clone()))) {
        UiIntent::SetTraceForm { document, input } => {
            assert_eq!(document, doc);
            assert_eq!(input.dpi, "600", "typed text, unparsed");
            assert_eq!(input.mode, TraceMode::Centerline);
            assert_eq!(input.threshold_choice, ThresholdChoice::Adaptive);
            assert!(input.invert && !input.curves);
        }
        other => panic!("{other:?}"),
    }
    let mut preset = PanelValues::default();
    preset.selection.insert(id::TR_PRESET, Some(3));
    assert_eq!(
        only_intent(react(&vm, doc, event(id::TR_PRESET, notify::CHANGED, preset))),
        UiIntent::ApplyTracePreset { document: doc, preset: TracePreset::Handwriting }
    );
    // The form already holds the file's resolution: using the proposal changes nothing.
    assert_eq!(react(&vm, doc, click(id::TR_SUGGESTION, PanelValues::default())), PanelReaction::Nothing);
    let (vm2, doc2) = vm_with(|d| {
        let mut t = tracer_model();
        t.form.input.dpi = "150".into();
        d.tracer = Some(t)
    });
    match only_intent(react(&vm2, doc2, click(id::TR_SUGGESTION, PanelValues::default()))) {
        UiIntent::SetTraceForm { input, .. } => assert_eq!(input.dpi, "300"),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        only_intent(react(&vm, doc, click(id::TR_APPLY, PanelValues::default()))),
        UiIntent::Invoke { document: Some(doc), command: CommandKey::ApplyTrace }
    );
}

#[test]
fn a_sheet_of_a_window_that_is_gone_reaches_nothing() {
    let (vm, _) = vm_with(|d| d.image.import = Some(import_model()));
    let gone = DocumentId::from_raw(777);
    assert_eq!(react(&vm, gone, click(id::IMP_COMMIT, PanelValues::default())), PanelReaction::Nothing);
}
