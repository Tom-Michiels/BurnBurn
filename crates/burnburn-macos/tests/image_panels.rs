//! The image panels, headless (W2-L1-beeld): the bitmap import sheet, Adjust Image with its
//! preview and the tracer panel. What the widgets show comes from hand-built presentation view
//! models (no controller behaviour is assumed), and every edit becomes one intent with the typed
//! text unparsed (DR-05). No AppKit here; the native sheets need a human (docs/dev-notes/IM20.md).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use burnburn_document::process::GrayMethod;
use burnburn_geometry::{Path, Point, Rect};
use burnburn_macos::adapter::{
    MenuItem, NativeEvent, Reaction, menu_enabled, menu_item, menu_tag, menu_title, translate,
};
use burnburn_macos::l10n::{Locale, image_field_name, image_issue_text};
use burnburn_macos::panels_image::{
    ADJUST_FIELDS, BitmapImage, GRAY_METHODS, THRESHOLD_CHOICES, TRACE_FIELDS, TRACE_MODES, gray_method,
    gray_preview_bitmap, image_adjust_intent, image_adjust_panel, image_dpi_intent, image_import_panel,
    suggested_bitmap_name, trace_form_intent, trace_preset, trace_preset_intent, trace_preview_bitmap,
    trace_suggestion_intent, tracer_panel,
};
use burnburn_presentation::{
    AppViewModel, CommandKey, HeadlessEditorHarness, ImageAdjustInput, ImageAdjustViewModel, ImageField,
    ImageImportViewModel, ImageIssue, ImagePreview, ThresholdChoice, TraceFormInput, TraceFormViewModel, TraceMode,
    TracePreset, TraceSummary, TracerViewModel, UiIntent,
};
use burnburn_types::{DocumentId, Length, OperationId, StableId};

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
        can_apply: false,
    }
}

fn adjust_fields(input: &ImageAdjustInput) -> [String; 9] {
    [
        input.brightness.clone(),
        input.contrast.clone(),
        input.gamma.clone(),
        input.sharpen_amount.clone(),
        input.sharpen_radius.clone(),
        input.crop_x.clone(),
        input.crop_y.clone(),
        input.crop_width.clone(),
        input.crop_height.clone(),
    ]
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
        mode: TraceMode::Centerline,
        dpi: "300".into(),
        threshold_choice: ThresholdChoice::Fixed,
        threshold: "128".into(),
        invert: true,
        ignore_below_mm2: "0,5".into(),
        smoothness_mm: "0,05".into(),
        curves: false,
    }
}

fn trace_form() -> TraceFormViewModel {
    TraceFormViewModel {
        input: trace_input(),
        preset: Some(TracePreset::Handwriting),
        issue: None,
        dpi_suggestion: Some(300.0),
    }
}

fn outline(width_mm: f64, height_mm: f64) -> Arc<Path> {
    let rect = Rect::new(Point::new(0.0, 0.0), Length::from_mm(width_mm).unwrap(), Length::from_mm(height_mm).unwrap())
        .unwrap();
    Arc::new(Path::rectangle(rect))
}

fn tracer_model() -> TracerViewModel {
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
            preview: outline(40.0, 25.0),
        }),
        form: trace_form(),
    }
}

/// The real headless view model of a new document with the given image state injected.
fn with_image_state(f: impl FnOnce(&mut burnburn_presentation::DocumentViewModel)) -> (AppViewModel, DocumentId) {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    let mut vm = (*h.view()).clone();
    let d = vm.documents.iter_mut().find(|d| d.id == doc).unwrap();
    f(d);
    (vm, doc)
}

fn intent_of(event: NativeEvent, vm: &AppViewModel) -> Option<UiIntent> {
    match translate(event, vm) {
        Reaction::Intent(i) => Some(i),
        _ => None,
    }
}

// ---- Adjust Image ------------------------------------------------------------------------------

#[test]
fn adjust_panel_puts_the_gamma_issue_on_the_gamma_row() {
    let mut m = adjust_model();
    m.issue = Some((ImageField::Gamma, ImageIssue::OutOfRange(ImageField::Gamma)));
    let panel = image_adjust_panel(&m, Locale::Nl);
    assert_eq!(panel.issue_field, Some(2), "gamma is the third field");
    assert_eq!(ADJUST_FIELDS[2], ImageField::Gamma);
    assert_eq!(panel.issue, "Waarde buiten bereik (Gamma)");
    let en = image_adjust_panel(&m, Locale::En);
    assert_eq!(en.issue, "Value out of range (Gamma)");
    // The same sheet without a refused input has no issue at all.
    let clean = image_adjust_panel(&adjust_model(), Locale::Nl);
    assert_eq!((clean.issue.as_str(), clean.issue_field), ("", None));
}

#[test]
fn adjust_crop_issues_name_the_crop_row_but_no_field_suffix() {
    let mut m = adjust_model();
    m.issue = Some((ImageField::CropWidth, ImageIssue::CropOutsideSource));
    let panel = image_adjust_panel(&m, Locale::Nl);
    assert_eq!(panel.issue, "Uitsnede valt buiten de afbeelding");
    assert_eq!(panel.issue_field, Some(7));
    m.issue = Some((ImageField::CropX, ImageIssue::CropIncomplete));
    let panel = image_adjust_panel(&m, Locale::Nl);
    assert_eq!(panel.issue, "Uitsnede is niet volledig ingevuld");
    assert_eq!(panel.issue_field, Some(5));
}

#[test]
fn adjust_panel_shows_the_typed_values_source_size_gray_method_and_state() {
    let mut m = adjust_model();
    m.input.gamma = "2,2".into();
    m.input.gray = GrayMethod::Lightness;
    m.input.crop_x = "10".into();
    m.input.crop_y = "20".into();
    m.input.crop_width = "300".into();
    m.input.crop_height = "200".into();
    m.computing = true;
    m.can_apply = true;
    let panel = image_adjust_panel(&m, Locale::Nl);
    assert_eq!(panel.operation, operation());
    assert_eq!(panel.title, "Foto");
    assert_eq!(panel.fields, adjust_fields(&m.input));
    assert_eq!(panel.gray_index, 3);
    assert_eq!(GRAY_METHODS[panel.gray_index], GrayMethod::Lightness);
    assert_eq!(panel.source, "Bron: 1200 × 800 pixels");
    assert_eq!(panel.computing, "berekenen…");
    assert!(panel.apply_enabled);
    m.source_px = None;
    m.computing = false;
    m.can_apply = false;
    let panel = image_adjust_panel(&m, Locale::En);
    assert_eq!(panel.source, "Reading the source…");
    assert_eq!(panel.computing, "");
    assert!(!panel.apply_enabled);
}

#[test]
fn gray_preview_becomes_opaque_rgba_of_the_same_size_top_row_first() {
    let preview = ImagePreview { width: 2, height: 2, gray8: Arc::from(vec![0u8, 128, 255, 10]) };
    let bitmap = gray_preview_bitmap(&preview).unwrap();
    assert_eq!((bitmap.width, bitmap.height), (2, 2));
    assert_eq!(bitmap.rgba, vec![0, 0, 0, 255, 128, 128, 128, 255, 255, 255, 255, 255, 10, 10, 10, 255]);
    // A buffer that does not fit its size, or an empty one, is no picture.
    let short = ImagePreview { width: 3, height: 2, gray8: Arc::from(vec![0u8; 5]) };
    assert_eq!(gray_preview_bitmap(&short), None);
    let empty = ImagePreview { width: 0, height: 0, gray8: Arc::from(Vec::<u8>::new()) };
    assert_eq!(gray_preview_bitmap(&empty), None);
    // The panel carries the converted preview.
    let mut m = adjust_model();
    m.preview = Some(preview);
    let panel = image_adjust_panel(&m, Locale::Nl);
    assert_eq!(panel.preview.map(|b| (b.width, b.height)), Some((2, 2)));
}

#[test]
fn typing_in_the_four_crop_fields_is_one_set_image_adjust_with_every_field() {
    let m = adjust_model();
    let doc = <DocumentId as StableId>::from_raw(1);
    let mut typed = adjust_fields(&m.input);
    typed[5] = "10".into();
    typed[6] = "20".into();
    typed[7] = "300".into();
    typed[8] = "200".into();
    let intent = image_adjust_intent(doc, &m, typed, 0).unwrap();
    let mut expected = adjust_input();
    expected.crop_x = "10".into();
    expected.crop_y = "20".into();
    expected.crop_width = "300".into();
    expected.crop_height = "200".into();
    assert_eq!(intent, UiIntent::SetImageAdjust { document: doc, input: Box::new(expected) });
}

#[test]
fn the_gray_pop_up_sets_the_method_and_an_unknown_item_keeps_the_current_one() {
    let mut m = adjust_model();
    m.input.gray = GrayMethod::Green;
    let doc = <DocumentId as StableId>::from_raw(1);
    let fields = adjust_fields(&m.input);
    let UiIntent::SetImageAdjust { input, .. } = image_adjust_intent(doc, &m, fields.clone(), 6).unwrap() else {
        panic!("not an adjust intent");
    };
    assert_eq!(input.gray, GrayMethod::Blue);
    // Item -1 (nothing chosen) and a stale item beyond the list keep Green: no change, no intent.
    assert_eq!(image_adjust_intent(doc, &m, fields.clone(), -1), None);
    assert_eq!(image_adjust_intent(doc, &m, fields, 99), None);
    assert_eq!(
        (gray_method(0), gray_method(6), gray_method(7), gray_method(-1)),
        (Some(GrayMethod::Rec709), Some(GrayMethod::Blue), None, None)
    );
}

#[test]
fn return_plus_end_of_editing_is_one_commit_and_a_refused_input_can_be_retried() {
    let mut m = adjust_model();
    let doc = <DocumentId as StableId>::from_raw(1);
    let same = adjust_fields(&m.input);
    assert_eq!(image_adjust_intent(doc, &m, same.clone(), 0), None, "text equal to the shown values");
    m.issue = Some((ImageField::Gamma, ImageIssue::InvalidNumber(ImageField::Gamma)));
    m.input.gamma = "abc".into();
    let mut again = adjust_fields(&m.input);
    assert!(image_adjust_intent(doc, &m, again.clone(), 0).is_some(), "same refused text goes again");
    again[2] = "2".into();
    assert!(image_adjust_intent(doc, &m, again, 0).is_some());
}

#[test]
fn adjust_edits_travel_through_translate_and_stale_callbacks_reach_nothing() {
    let (vm, doc) = with_image_state(|d| d.image.adjust = Some(adjust_model()));
    let mut typed = adjust_fields(&adjust_input());
    typed[2] = "2".into();
    let event = NativeEvent::ImageAdjustEdited { document: doc, fields: typed, gray_index: 0 };
    let mut expected = adjust_input();
    expected.gamma = "2".into();
    assert_eq!(intent_of(event, &vm), Some(UiIntent::SetImageAdjust { document: doc, input: Box::new(expected) }));
    // The sheet closed meanwhile: a late commit changes nothing.
    let (closed, doc) = with_image_state(|_| {});
    let event = NativeEvent::ImageAdjustEdited { document: doc, fields: adjust_fields(&adjust_input()), gray_index: 0 };
    assert_eq!(intent_of(event, &closed), None);
}

// ---- Bitmap import sheet -----------------------------------------------------------------------

#[test]
fn import_sheet_shows_the_dpi_proposal_and_the_physical_size() {
    let panel = image_import_panel(&import_model(), Locale::Nl);
    assert_eq!(panel.file, "foto.png");
    assert_eq!(panel.pixels, "1200 × 800 pixels");
    assert_eq!(panel.dpi_text, "300");
    assert_eq!(panel.suggestion, "Resolutie uit het bestand: 300 dpi");
    assert_eq!(panel.size, "Afmeting: 101,6 × 67,7 mm");
    assert!(panel.commit_enabled && !panel.dpi_invalid);
    assert_eq!(image_import_panel(&import_model(), Locale::En).size, "Size: 101.6 × 67.7 mm");
    // A file without a resolution gets the 254 dpi proposal (0.1 mm per pixel), said as such.
    let mut m = import_model();
    m.dpi_suggestion = None;
    m.dpi_text = "254".into();
    m.size_mm = Some((304.8, 203.2));
    let panel = image_import_panel(&m, Locale::Nl);
    assert_eq!(
        panel.suggestion,
        "Het bestand geeft geen resolutie: vul die zelf in (bijvoorbeeld 254 dpi = 0,1 mm per pixel)."
    );
    assert_eq!(panel.size, "Afmeting: 304,8 × 203,2 mm");
}

#[test]
fn import_is_disabled_without_a_physical_size_and_the_dpi_field_carries_the_issue() {
    let mut m = import_model();
    m.dpi_text = "abc".into();
    m.size_mm = None;
    m.issue = Some(ImageIssue::InvalidNumber(ImageField::Dpi));
    let panel = image_import_panel(&m, Locale::Nl);
    assert!(!panel.commit_enabled);
    assert!(panel.dpi_invalid);
    assert_eq!(panel.dpi_text, "abc", "what the user typed stays");
    assert_eq!(panel.issue, "Ongeldig getal (Resolutie)");
    assert_eq!(panel.size, "Afmeting: onbekend zolang de resolutie ongeldig is");
    // Even with a size, a pending issue (a stale refused read) does not allow importing.
    m.size_mm = Some((1.0, 1.0));
    assert!(!image_import_panel(&m, Locale::Nl).commit_enabled);
}

#[test]
fn the_dpi_field_commits_its_raw_text_once() {
    let m = import_model();
    let doc = <DocumentId as StableId>::from_raw(1);
    assert_eq!(
        image_dpi_intent(doc, &m, "300 dpi".into()),
        Some(UiIntent::SetImageDpi { document: doc, dpi: "300 dpi".into() })
    );
    assert_eq!(image_dpi_intent(doc, &m, "300".into()), None, "equal to the shown text");
    let mut refused = import_model();
    refused.dpi_text = "abc".into();
    refused.issue = Some(ImageIssue::InvalidNumber(ImageField::Dpi));
    assert!(image_dpi_intent(doc, &refused, "abc".into()).is_some(), "a retry of a refused text still goes");
    let (vm, doc) = with_image_state(|d| d.image.import = Some(import_model()));
    assert_eq!(
        intent_of(NativeEvent::ImageDpiCommitted { document: doc, text: "600".into() }, &vm),
        Some(UiIntent::SetImageDpi { document: doc, dpi: "600".into() })
    );
    let (closed, doc) = with_image_state(|_| {});
    assert_eq!(intent_of(NativeEvent::ImageDpiCommitted { document: doc, text: "600".into() }, &closed), None);
}

// ---- Tracer ------------------------------------------------------------------------------------

#[test]
fn tracer_panel_mirrors_the_form_and_the_summary() {
    let panel = tracer_panel(&tracer_model(), Locale::Nl);
    assert_eq!(panel.source, "Bron: 800 × 600 pixels");
    assert_eq!(TRACE_MODES[panel.mode_index], TraceMode::Centerline);
    assert_eq!(panel.fields, ["300", "128", "0,5", "0,05"].map(String::from));
    assert_eq!(TRACE_FIELDS[1], ImageField::TraceThreshold);
    assert_eq!(THRESHOLD_CHOICES[panel.threshold_index], ThresholdChoice::Fixed);
    assert!(panel.threshold_enabled && panel.invert && !panel.curves);
    assert_eq!(panel.suggestion_button.as_deref(), Some("Gebruik voorstel (300 dpi)"));
    assert_eq!(panel.summary, "Delen: 3, gaten: 1, formaat 40 × 25 mm");
    assert_eq!((panel.computing.as_str(), panel.scale_hint.as_str(), panel.issue.as_str()), ("", "", ""));
    assert_eq!(panel.preview.as_ref().map(|b| b.width.max(b.height)), Some(280));
}

#[test]
fn tracer_panel_says_computing_missing_scale_and_the_refused_field() {
    let mut m = tracer_model();
    m.computing = true;
    m.needs_scale = true;
    m.result = None;
    m.form.dpi_suggestion = None;
    m.form.input.threshold_choice = ThresholdChoice::Otsu;
    m.form.issue = Some((ImageField::TraceIgnoreBelow, ImageIssue::OutOfRange(ImageField::TraceIgnoreBelow)));
    let panel = tracer_panel(&m, Locale::Nl);
    assert_eq!(panel.computing, "berekenen…");
    assert_eq!(panel.scale_hint, "Schaal ontbreekt: geef de resolutie (dpi) van het beeld op.");
    assert_eq!(panel.suggestion_button, None);
    assert_eq!(panel.summary, "");
    assert_eq!(panel.preview, None);
    assert!(!panel.threshold_enabled, "the value field applies to the fixed threshold only");
    assert_eq!(panel.issue, "Waarde buiten bereik (Negeer kleiner dan)");
    assert_eq!(panel.issue_field, Some(2));
}

#[test]
fn recipe_pop_up_lists_the_presets_and_selects_the_one_the_values_equal() {
    let panel = tracer_panel(&tracer_model(), Locale::Nl);
    assert_eq!(
        panel.preset_titles,
        ["Logo of silhouet", "Foto", "Technische tekening", "Handschrift en lijntekening"].map(String::from)
    );
    assert_eq!(panel.preset_titles.len(), TracePreset::ALL.len());
    assert_eq!(panel.preset_index, Some(3));
    let mut custom = tracer_model();
    custom.form.preset = None;
    assert_eq!(tracer_panel(&custom, Locale::Nl).preset_index, None);
}

#[test]
fn choosing_a_recipe_applies_exactly_that_preset() {
    let doc = <DocumentId as StableId>::from_raw(1);
    for (i, preset) in TracePreset::ALL.iter().enumerate() {
        assert_eq!(trace_preset(i as isize), Some(*preset));
        assert_eq!(
            trace_preset_intent(doc, i as isize),
            Some(UiIntent::ApplyTracePreset { document: doc, preset: *preset })
        );
    }
    assert_eq!(trace_preset_intent(doc, -1), None);
    assert_eq!(trace_preset_intent(doc, TracePreset::ALL.len() as isize), None);
    let (vm, doc) = with_image_state(|d| d.tracer = Some(tracer_model()));
    assert_eq!(
        intent_of(NativeEvent::TracePresetChosen { document: doc, index: 1 }, &vm),
        Some(UiIntent::ApplyTracePreset { document: doc, preset: TracePreset::Photo })
    );
    let (closed, doc) = with_image_state(|_| {});
    assert_eq!(intent_of(NativeEvent::TracePresetChosen { document: doc, index: 1 }, &closed), None);
}

#[test]
fn the_form_survives_panel_and_back_and_one_edit_is_one_set_trace_form() {
    let m = tracer_model();
    let doc = <DocumentId as StableId>::from_raw(1);
    let panel = tracer_panel(&m, Locale::Nl);
    let widgets = |panel: &burnburn_macos::panels_image::TracerPanel, fields: [String; 4]| {
        trace_form_intent(
            doc,
            &m.form,
            fields,
            panel.mode_index as isize,
            panel.threshold_index as isize,
            panel.invert,
            panel.curves,
        )
    };
    // Widgets exactly as the panel filled them: nothing changed, nothing sent.
    assert_eq!(widgets(&panel, panel.fields.clone()), None);
    // One field typed: all fields go, as typed.
    let mut typed = panel.fields.clone();
    typed[3] = "0,1".into();
    let mut expected = trace_input();
    expected.smoothness_mm = "0,1".into();
    assert_eq!(widgets(&panel, typed), Some(UiIntent::SetTraceForm { document: doc, input: Box::new(expected) }));
    // Pop-ups and checkboxes likewise: mode, threshold kind, invert and curves.
    let UiIntent::SetTraceForm { input, .. } =
        trace_form_intent(doc, &m.form, panel.fields.clone(), 0, 2, false, true).unwrap()
    else {
        panic!("not a form intent");
    };
    assert_eq!(
        *input,
        TraceFormInput {
            mode: TraceMode::Contour,
            threshold_choice: ThresholdChoice::Adaptive,
            invert: false,
            curves: true,
            ..trace_input()
        }
    );
}

#[test]
fn the_form_event_translates_with_the_shown_form() {
    let (vm, doc) = with_image_state(|d| d.tracer = Some(tracer_model()));
    let event = NativeEvent::TraceFormEdited {
        document: doc,
        fields: ["300", "200", "0,5", "0,05"].map(String::from),
        mode_index: 1,
        threshold_index: 1,
        invert: true,
        curves: false,
    };
    let mut expected = trace_input();
    expected.threshold = "200".into();
    assert_eq!(intent_of(event, &vm), Some(UiIntent::SetTraceForm { document: doc, input: Box::new(expected) }));
}

#[test]
fn the_proposal_button_puts_the_file_resolution_in_the_dpi_field() {
    let doc = <DocumentId as StableId>::from_raw(1);
    let mut form = trace_form();
    form.input.dpi = String::new();
    form.dpi_suggestion = Some(299.9994);
    let mut expected = form.input.clone();
    expected.dpi = "300".into();
    assert_eq!(
        trace_suggestion_intent(doc, &form),
        Some(UiIntent::SetTraceForm { document: doc, input: Box::new(expected) })
    );
    form.dpi_suggestion = Some(72.5);
    let UiIntent::SetTraceForm { input, .. } = trace_suggestion_intent(doc, &form).unwrap() else { panic!() };
    assert_eq!(input.dpi, "72.5");
    form.dpi_suggestion = None;
    assert_eq!(trace_suggestion_intent(doc, &form), None);
}

fn pixel(b: &BitmapImage, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * b.width + x) * 4) as usize;
    [b.rgba[i], b.rgba[i + 1], b.rgba[i + 2]]
}

#[test]
fn the_trace_preview_draws_the_outline_to_scale_on_white() {
    // 10 × 4 mm in at most 100 px: 9.1 px/mm inside a 4 px margin, so 91 × 36 px of outline.
    let bitmap = trace_preview_bitmap(&outline(10.0, 4.0), 100).unwrap();
    assert_eq!((bitmap.width, bitmap.height), (100, 45));
    assert_eq!(bitmap.rgba.len(), 100 * 45 * 4);
    assert!(bitmap.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255), "opaque");
    let black = [0, 0, 0];
    let white = [255, 255, 255];
    // The border of the rectangle is black on all four sides, the inside and the margin are white.
    assert_eq!(pixel(&bitmap, 50, 4), black);
    assert_eq!(pixel(&bitmap, 50, 40), black);
    assert_eq!(pixel(&bitmap, 4, 22), black);
    assert_eq!(pixel(&bitmap, 95, 22), black);
    assert_eq!(pixel(&bitmap, 50, 22), white);
    assert_eq!(pixel(&bitmap, 0, 0), white);
    assert_eq!(pixel(&bitmap, 99, 44), white);
    let black_pixels = bitmap.rgba.as_chunks::<4>().0.iter().filter(|p| p[0] == 0).count();
    assert_eq!(black_pixels, 2 * 92 + 2 * 35, "exactly the four one-pixel edges");
}

#[test]
fn the_trace_preview_flips_y_up_into_top_row_first() {
    // A diagonal from the lower left (0, 0) to the upper right (10, 10) of the drawing.
    let path = Path::line(Point::new(0.0, 0.0), Point::new(10.0, 10.0)).unwrap();
    let bitmap = trace_preview_bitmap(&path, 64).unwrap();
    assert_eq!((bitmap.width, bitmap.height), (64, 64));
    assert_eq!(pixel(&bitmap, 4, 59), [0, 0, 0], "lower left of the drawing is bottom left of the picture");
    assert_eq!(pixel(&bitmap, 59, 4), [0, 0, 0], "upper right of the drawing is top right of the picture");
    assert_eq!(pixel(&bitmap, 4, 4), [255, 255, 255]);
    assert_eq!(pixel(&bitmap, 59, 59), [255, 255, 255]);
    // Nothing to draw: no picture.
    assert_eq!(trace_preview_bitmap(&Path::from_subpaths(Vec::new()), 64), None);
    assert_eq!(trace_preview_bitmap(&path, 0), None);
}

// ---- Menu, file panels and texts ---------------------------------------------------------------

#[test]
fn the_image_menu_items_ask_for_the_right_panels() {
    let (vm, _doc) = with_image_state(|_| {});
    assert_eq!(translate(NativeEvent::Menu(MenuItem::ImportImage), &vm), Reaction::ShowImportPanel);
    assert_eq!(translate(NativeEvent::Menu(MenuItem::TraceImage), &vm), Reaction::ShowTracePanel);
    assert_eq!(
        translate(NativeEvent::Menu(MenuItem::Command(CommandKey::ReplaceImage)), &vm),
        Reaction::ShowReplaceImagePanel
    );
    assert_eq!(
        translate(NativeEvent::Menu(MenuItem::Command(CommandKey::SaveProcessedBitmap)), &vm),
        Reaction::ShowBitmapSavePanel
    );
    // The other image commands are plain presentation commands.
    for key in [CommandKey::AdjustImage, CommandKey::MaskImage, CommandKey::RefreshImage] {
        assert_eq!(
            translate(NativeEvent::Menu(MenuItem::Command(key)), &vm),
            Reaction::Intent(UiIntent::Invoke { document: None, command: key })
        );
    }
    for item in [MenuItem::ImportImage, MenuItem::TraceImage] {
        assert_eq!(menu_item(menu_tag(item)), Some(item));
        assert!(menu_enabled(item, &vm), "{item:?} needs an open document window");
    }
    assert_eq!(menu_title(MenuItem::ImportImage, &vm, Locale::Nl), "Beeld importeren…");
    assert_eq!(menu_title(MenuItem::TraceImage, &vm, Locale::Nl), "Beeld tracen…");
    assert_eq!(menu_title(MenuItem::TraceImage, &vm, Locale::En), "Trace Image…");
}

#[test]
fn the_image_menu_commands_get_enablement_from_the_view_model_and_their_names_from_the_catalogue() {
    let (vm, _) = with_image_state(|_| {});
    for (key, nl) in [
        (CommandKey::AdjustImage, "Beeld aanpassen…"),
        (CommandKey::MaskImage, "Masker op beeld"),
        (CommandKey::ReplaceImage, "Beeld vervangen…"),
        (CommandKey::RefreshImage, "Beeld vernieuwen"),
        (CommandKey::SaveProcessedBitmap, "Verwerkte bitmap bewaren…"),
    ] {
        assert_eq!(menu_title(MenuItem::Command(key), &vm, Locale::Nl), nl);
        assert_eq!(
            menu_enabled(MenuItem::Command(key), &vm),
            vm.commands.get(&key).is_some_and(|c| c.enabled),
            "{key:?} follows the view model"
        );
    }
}

#[test]
fn chosen_files_become_the_image_intents_and_cancel_changes_nothing() {
    let (vm, doc) = with_image_state(|_| {});
    let path = PathBuf::from("/tmp/foto.png");
    assert_eq!(
        intent_of(NativeEvent::ReplaceImageFileChosen { document: doc, path: Some(path.clone()) }, &vm),
        Some(UiIntent::ReplaceImage { document: doc, path: path.clone() })
    );
    assert_eq!(
        intent_of(NativeEvent::BitmapSavePanelFinished { document: doc, path: Some(path.clone()) }, &vm),
        Some(UiIntent::SaveProcessedBitmap { document: doc, path: path.clone() })
    );
    assert_eq!(
        intent_of(NativeEvent::TraceFileChosen { document: doc, path: Some(path.clone()) }, &vm),
        Some(UiIntent::OpenTraceSource { document: doc, path })
    );
    for event in [
        NativeEvent::ReplaceImageFileChosen { document: doc, path: None },
        NativeEvent::BitmapSavePanelFinished { document: doc, path: None },
        NativeEvent::TraceFileChosen { document: doc, path: None },
    ] {
        assert_eq!(translate(event, &vm), Reaction::Nothing);
    }
    let gone = <DocumentId as StableId>::from_raw(999);
    assert_eq!(
        translate(NativeEvent::TraceFileChosen { document: gone, path: Some(PathBuf::from("/tmp/x.png")) }, &vm),
        Reaction::Nothing
    );
}

#[test]
fn the_save_panel_suggests_the_document_title_as_png() {
    assert_eq!(suggested_bitmap_name("Naamloos"), "Naamloos.png");
    assert_eq!(suggested_bitmap_name("Plaat.burnburn"), "Plaat.png");
    assert_eq!(suggested_bitmap_name("kast.v2.burnburn"), "kast.v2.png");
    assert_eq!(suggested_bitmap_name(""), "Naamloos.png");
}

#[test]
fn every_image_issue_has_dutch_and_english_text() {
    let issues = [
        (ImageIssue::InvalidNumber(ImageField::Gamma), "Ongeldig getal", "Not a valid number"),
        (ImageIssue::OutOfRange(ImageField::Gamma), "Waarde buiten bereik", "Value out of range"),
        (ImageIssue::CropOutsideSource, "Uitsnede valt buiten de afbeelding", "The crop lies outside the image"),
        (ImageIssue::CropIncomplete, "Uitsnede is niet volledig ingevuld", "The crop is not completely filled in"),
        (
            ImageIssue::UnsupportedFormat,
            "Bestandstype niet ondersteund (alleen PNG en JPEG)",
            "File type not supported (PNG and JPEG only)",
        ),
        (ImageIssue::Unreadable, "Bestand kon niet worden gelezen", "The file could not be read"),
        (
            ImageIssue::SourceMissing,
            "Bronpixels ontbreken in het project",
            "The source pixels are missing from the project",
        ),
    ];
    for (issue, nl, en) in issues {
        assert_eq!(image_issue_text(issue, Locale::Nl), nl);
        assert_eq!(image_issue_text(issue, Locale::En), en);
    }
    for field in ADJUST_FIELDS.iter().chain(TRACE_FIELDS.iter()).chain([ImageField::Dpi].iter()) {
        for locale in [Locale::Nl, Locale::En] {
            assert!(!image_field_name(*field, locale).trim().is_empty(), "{field:?} {locale:?}");
        }
    }
}

#[test]
fn ignore_smaller_than_applies_to_contours_only_while_smoothness_applies_to_both() {
    let mut m = tracer_model();
    m.form.input.mode = TraceMode::Centerline;
    let centre = tracer_panel(&m, Locale::Nl);
    assert!(!centre.ignore_enabled, "no effect on centre lines");
    assert_eq!(centre.fields[3], "0,05", "smoothness stays editable text in both modes");
    m.form.input.mode = TraceMode::Contour;
    assert!(tracer_panel(&m, Locale::Nl).ignore_enabled);
}
