//! The image panels without AppKit (W2-L1-beeld): the bitmap import sheet (P03), Adjust Image with
//! its preview, and the tracer panel (T01, T02, T04, T10).
//!
//! View-model data becomes plain panel structs with localized text, field indices, enablement and
//! popup selections; widget events become intents with the typed text unparsed (DR-05). The
//! functions take the sub-view-models (`ImageAdjustViewModel`, `ImageImportViewModel`,
//! `TracerViewModel`), not the whole `AppViewModel`, so they are tested with hand-built models.

use burnburn_document::process::GrayMethod;
use burnburn_geometry::{DisplayTolerance, Path, Point};
use burnburn_presentation::{
    ImageAdjustInput, ImageAdjustViewModel, ImageField, ImageImportViewModel, ImageIssue, ImagePreview,
    ThresholdChoice, TraceFormInput, TraceFormViewModel, TraceMode, TracePreset, TracerViewModel, UiIntent,
};
use burnburn_types::{DocumentId, Length, OperationId};

use crate::l10n::{HostText, Locale, host_text, image_field_name, image_issue_text};

/// The resolution proposed for a file that gives none: 0.1 mm per pixel.
pub const DEFAULT_DPI: f64 = 254.0;

/// Long side (pixels) of the picture the tracer panel draws its outline into.
pub const TRACE_PREVIEW_PX: u32 = 280;

/// Empty border (pixels) around the outline drawn by [`trace_preview_bitmap`].
const PREVIEW_MARGIN: u32 = 4;

/// `template` with `{0}`, `{1}`, … replaced by `args`.
pub(crate) fn fill(template: &str, args: &[&str]) -> String {
    let mut out = template.to_owned();
    for (i, arg) in args.iter().enumerate() {
        out = out.replace(&format!("{{{i}}}"), arg);
    }
    out
}

/// `value` with at most `decimals` decimals, trailing zeros dropped, in the locale's decimal sign.
pub(crate) fn number(value: f64, decimals: usize, locale: Locale) -> String {
    let mut text = format!("{value:.decimals$}");
    if text.contains('.') {
        text = text.trim_end_matches('0').trim_end_matches('.').to_owned();
    }
    if text == "-0" {
        text = "0".to_owned();
    }
    if locale == Locale::Nl { text.replace('.', ",") } else { text }
}

/// The issue as a sentence; an input error names its field behind the sentence.
fn issue_line(issue: ImageIssue, locale: Locale) -> String {
    let text = image_issue_text(issue, locale);
    match issue {
        ImageIssue::InvalidNumber(field) | ImageIssue::OutOfRange(field) => {
            format!("{text} ({})", image_field_name(field, locale))
        }
        _ => text,
    }
}

/// A bitmap ready for a native image view: opaque RGBA, top row first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitmapImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Gray methods of the Adjust Image pop-up, top to bottom.
pub const GRAY_METHODS: [GrayMethod; 7] = [
    GrayMethod::Rec709,
    GrayMethod::Rec601,
    GrayMethod::Average,
    GrayMethod::Lightness,
    GrayMethod::Red,
    GrayMethod::Green,
    GrayMethod::Blue,
];

/// The gray method of pop-up item `index` (−1 = nothing chosen).
pub fn gray_method(index: isize) -> Option<GrayMethod> {
    GRAY_METHODS.get(usize::try_from(index).ok()?).copied()
}

/// The pop-up title of a gray method.
pub fn gray_method_name(method: GrayMethod, locale: Locale) -> &'static str {
    let text = match method {
        GrayMethod::Rec709 => HostText::GrayRec709,
        GrayMethod::Rec601 => HostText::GrayRec601,
        GrayMethod::Average => HostText::GrayAverage,
        GrayMethod::Lightness => HostText::GrayLightness,
        GrayMethod::Red => HostText::GrayRed,
        GrayMethod::Green => HostText::GrayGreen,
        GrayMethod::Blue => HostText::GrayBlue,
    };
    host_text(text, locale)
}

/// Text fields of the Adjust Image sheet, top to bottom.
pub const ADJUST_FIELDS: [ImageField; 9] = [
    ImageField::Brightness,
    ImageField::Contrast,
    ImageField::Gamma,
    ImageField::SharpenAmount,
    ImageField::SharpenRadius,
    ImageField::CropX,
    ImageField::CropY,
    ImageField::CropWidth,
    ImageField::CropHeight,
];

/// Text fields of the tracer panel, top to bottom.
pub const TRACE_FIELDS: [ImageField; 4] =
    [ImageField::Dpi, ImageField::TraceThreshold, ImageField::TraceIgnoreBelow, ImageField::TraceSmoothness];

/// Tracer mode pop-up items, top to bottom.
pub const TRACE_MODES: [TraceMode; 2] = [TraceMode::Contour, TraceMode::Centerline];

/// Tracer threshold pop-up items, top to bottom.
pub const THRESHOLD_CHOICES: [ThresholdChoice; 3] =
    [ThresholdChoice::Otsu, ThresholdChoice::Fixed, ThresholdChoice::Adaptive];

/// The preset of recipe pop-up item `index` (−1 = nothing chosen).
pub fn trace_preset(index: isize) -> Option<TracePreset> {
    TracePreset::ALL.get(usize::try_from(index).ok()?).copied()
}

/// The pop-up title of a tracer recipe.
pub fn trace_preset_name(preset: TracePreset, locale: Locale) -> String {
    let text = match preset {
        TracePreset::Logo => HostText::TracerPresetLogo,
        TracePreset::Photo => HostText::TracerPresetPhoto,
        TracePreset::Technical => HostText::TracerPresetTechnical,
        TracePreset::Handwriting => HostText::TracerPresetHandwriting,
        other => return format!("{other:?}"),
    };
    host_text(text, locale).to_owned()
}

/// The bitmap import sheet's texts and states.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImageImportPanel {
    pub file: String,
    pub pixels: String,
    pub dpi_caption: String,
    pub dpi_text: String,
    pub suggestion: String,
    pub size: String,
    pub issue: String,
    /// The DPI field is the one in error (no physical size can be given).
    pub dpi_invalid: bool,
    pub commit_enabled: bool,
}

pub fn image_import_panel(m: &ImageImportViewModel, locale: Locale) -> ImageImportPanel {
    let t = |text: HostText| host_text(text, locale);
    let suggestion = match m.dpi_suggestion {
        Some(dpi) => fill(t(HostText::ImageImportSuggestionFile), &[&number(dpi, 2, locale)]),
        None => fill(t(HostText::ImageImportSuggestionNone), &[&number(DEFAULT_DPI, 2, locale)]),
    };
    let size = match m.size_mm {
        Some((w, h)) => fill(t(HostText::ImageImportSize), &[&number(w, 1, locale), &number(h, 1, locale)]),
        None => t(HostText::ImageImportSizeUnknown).to_owned(),
    };
    ImageImportPanel {
        file: m.file.clone(),
        pixels: fill(t(HostText::ImageImportPixels), &[&m.width_px.to_string(), &m.height_px.to_string()]),
        dpi_caption: t(HostText::ImageImportDpi).to_owned(),
        dpi_text: m.dpi_text.clone(),
        suggestion,
        size,
        issue: m.issue.map(|i| issue_line(i, locale)).unwrap_or_default(),
        dpi_invalid: m.size_mm.is_none()
            || matches!(
                m.issue,
                Some(ImageIssue::InvalidNumber(ImageField::Dpi) | ImageIssue::OutOfRange(ImageField::Dpi))
            ),
        commit_enabled: m.size_mm.is_some() && m.issue.is_none(),
    }
}

/// The Adjust Image sheet's texts and states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageAdjustPanel {
    pub operation: OperationId,
    pub title: String,
    pub source: String,
    /// Text of the fields in [`ADJUST_FIELDS`] order.
    pub fields: [String; 9],
    /// Pop-up item of the gray method.
    pub gray_index: usize,
    /// Issue sentence under the fields, empty without issue.
    pub issue: String,
    /// Index into [`ADJUST_FIELDS`] of the field in error.
    pub issue_field: Option<usize>,
    /// "berekenen…" while a preview task runs, otherwise empty.
    pub computing: String,
    pub preview: Option<BitmapImage>,
    pub apply_enabled: bool,
}

pub fn image_adjust_panel(m: &ImageAdjustViewModel, locale: Locale) -> ImageAdjustPanel {
    let t = |text: HostText| host_text(text, locale);
    let i = &m.input;
    ImageAdjustPanel {
        operation: m.operation,
        title: m.title.clone(),
        source: match m.source_px {
            Some((w, h)) => fill(t(HostText::ImageAdjustSource), &[&w.to_string(), &h.to_string()]),
            None => t(HostText::ImageAdjustSourceLoading).to_owned(),
        },
        fields: adjust_fields(i),
        gray_index: GRAY_METHODS.iter().position(|g| *g == i.gray).unwrap_or(0),
        issue: m.issue.map(|(_, issue)| issue_line(issue, locale)).unwrap_or_default(),
        issue_field: m.issue.and_then(|(field, _)| ADJUST_FIELDS.iter().position(|f| *f == field)),
        computing: if m.computing { t(HostText::ImageComputing).to_owned() } else { String::new() },
        preview: m.preview.as_ref().and_then(gray_preview_bitmap),
        apply_enabled: m.can_apply,
    }
}

/// The Adjust Image field texts in [`ADJUST_FIELDS`] order.
fn adjust_fields(i: &ImageAdjustInput) -> [String; 9] {
    [
        i.brightness.clone(),
        i.contrast.clone(),
        i.gamma.clone(),
        i.sharpen_amount.clone(),
        i.sharpen_radius.clone(),
        i.crop_x.clone(),
        i.crop_y.clone(),
        i.crop_width.clone(),
        i.crop_height.clone(),
    ]
}

/// The tracer panel's texts and states.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TracerPanel {
    pub source: String,
    pub mode_index: usize,
    /// Text of the fields in [`TRACE_FIELDS`] order.
    pub fields: [String; 4],
    pub threshold_index: usize,
    /// The threshold value field applies to the fixed threshold only.
    pub threshold_enabled: bool,
    /// "Negeer kleiner dan" has no effect on centre lines, so the field is greyed out there.
    pub ignore_enabled: bool,
    pub invert: bool,
    pub curves: bool,
    /// Titles of the recipe pop-up, in `TracePreset::ALL` order.
    pub preset_titles: Vec<String>,
    /// The recipe the values still equal.
    pub preset_index: Option<usize>,
    /// "Gebruik voorstel (300 dpi)" when the file has a resolution.
    pub suggestion_button: Option<String>,
    pub summary: String,
    pub computing: String,
    pub scale_hint: String,
    pub issue: String,
    pub issue_field: Option<usize>,
    pub preview: Option<BitmapImage>,
}

pub fn tracer_panel(m: &TracerViewModel, locale: Locale) -> TracerPanel {
    let t = |text: HostText| host_text(text, locale);
    let input = &m.form.input;
    let summary = m.result.as_ref().map_or_else(String::new, |r| {
        fill(
            t(HostText::TracerSummary),
            &[
                &r.components.to_string(),
                &r.holes.to_string(),
                &number(r.width_mm, 1, locale),
                &number(r.height_mm, 1, locale),
            ],
        )
    });
    TracerPanel {
        source: fill(t(HostText::TracerSource), &[&m.width_px.to_string(), &m.height_px.to_string()]),
        mode_index: TRACE_MODES.iter().position(|x| *x == input.mode).unwrap_or(0),
        fields: [
            input.dpi.clone(),
            input.threshold.clone(),
            input.ignore_below_mm2.clone(),
            input.smoothness_mm.clone(),
        ],
        threshold_index: THRESHOLD_CHOICES.iter().position(|x| *x == input.threshold_choice).unwrap_or(0),
        threshold_enabled: input.threshold_choice == ThresholdChoice::Fixed,
        ignore_enabled: input.mode == TraceMode::Contour,
        invert: input.invert,
        curves: input.curves,
        preset_titles: TracePreset::ALL.iter().map(|p| trace_preset_name(*p, locale)).collect(),
        preset_index: m.form.preset.and_then(|p| TracePreset::ALL.iter().position(|x| *x == p)),
        suggestion_button: m
            .form
            .dpi_suggestion
            .map(|dpi| fill(t(HostText::TracerUseSuggestion), &[&number(dpi, 2, locale)])),
        summary,
        computing: if m.computing { t(HostText::ImageComputing).to_owned() } else { String::new() },
        scale_hint: if m.needs_scale { t(HostText::TracerNeedsScale).to_owned() } else { String::new() },
        issue: m.form.issue.map(|(_, issue)| issue_line(issue, locale)).unwrap_or_default(),
        issue_field: m.form.issue.and_then(|(field, _)| TRACE_FIELDS.iter().position(|f| *f == field)),
        preview: m.result.as_ref().and_then(|r| trace_preview_bitmap(&r.preview, TRACE_PREVIEW_PX)),
    }
}

/// Opaque RGBA of the gray values the laser would scan.
pub fn gray_preview_bitmap(preview: &ImagePreview) -> Option<BitmapImage> {
    let pixels = usize::try_from(preview.width).ok()?.checked_mul(usize::try_from(preview.height).ok()?)?;
    if pixels == 0 || preview.gray8.len() != pixels {
        return None;
    }
    let rgba = preview.gray8.iter().flat_map(|g| [*g, *g, *g, 255]).collect();
    Some(BitmapImage { width: preview.width, height: preview.height, rgba })
}

/// The traced outline drawn into a bitmap of at most `max_px` on its long side (black on white,
/// y up in the path becomes top row first).
pub fn trace_preview_bitmap(path: &Path, max_px: u32) -> Option<BitmapImage> {
    // The outline's end pixels lie `scale * extent` pixels apart, inside a margin on every side.
    let inner = max_px.checked_sub(2 * PREVIEW_MARGIN + 1).filter(|n| *n > 0)?;
    let bounds = path.bounds()?;
    let (min, max) = (bounds.min(), bounds.max());
    let extent = bounds.width().max(bounds.height());
    if !(extent.is_finite() && extent > 0.0) {
        return None;
    }
    let scale = f64::from(inner) / extent;
    let tolerance = DisplayTolerance::new(Length::from_mm(extent / f64::from(max_px) / 4.0).ok()?).ok()?;
    let span = |mm: f64| (mm * scale).round() as u32;
    let width = span(bounds.width()) + 2 * PREVIEW_MARGIN + 1;
    let height = span(bounds.height()) + 2 * PREVIEW_MARGIN + 1;
    let mut rgba = vec![255u8; width as usize * height as usize * 4];
    let to_px = |p: Point| {
        let x = i64::from(PREVIEW_MARGIN) + ((p.x - min.x) * scale).round() as i64;
        let y = i64::from(PREVIEW_MARGIN) + ((max.y - p.y) * scale).round() as i64;
        (x, y)
    };
    let mut plot = |x: i64, y: i64| {
        if let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y))
            && x < width
            && y < height
        {
            let i = (y as usize * width as usize + x as usize) * 4;
            rgba[i..i + 3].fill(0);
        }
    };
    for line in path.flatten(&tolerance) {
        let mut points: Vec<_> = line.points.iter().map(|p| to_px(*p)).collect();
        if line.closed
            && let Some(first) = points.first().copied()
        {
            points.push(first);
        }
        if let [only] = points[..] {
            plot(only.0, only.1);
        }
        for pair in points.windows(2) {
            line_pixels(pair[0], pair[1], &mut plot);
        }
    }
    Some(BitmapImage { width, height, rgba })
}

/// Bresenham: every pixel on the segment from `a` to `b`, both ends included.
fn line_pixels(a: (i64, i64), b: (i64, i64), plot: &mut impl FnMut(i64, i64)) {
    let (mut x, mut y) = a;
    let (dx, dy) = ((b.0 - a.0).abs(), -(b.1 - a.1).abs());
    let (sx, sy) = (if a.0 < b.0 { 1 } else { -1 }, if a.1 < b.1 { 1 } else { -1 });
    let mut err = dx + dy;
    loop {
        plot(x, y);
        if (x, y) == b {
            break;
        }
        let doubled = 2 * err;
        if doubled >= dy {
            err += dy;
            x += sx;
        }
        if doubled <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// `SetImageDpi` for the text typed in the DPI field; `None` when it is what the sheet shows.
pub fn image_dpi_intent(document: DocumentId, shown: &ImageImportViewModel, text: String) -> Option<UiIntent> {
    // Return and the following end of editing both send the action: only changed text is a
    // commit. A refused text stays in the field, so a retry of the same text still goes.
    if shown.issue.is_none() && shown.dpi_text == text {
        return None;
    }
    Some(UiIntent::SetImageDpi { document, dpi: text })
}

/// `SetImageAdjust` with every field as typed and the gray pop-up item.
pub fn image_adjust_intent(
    document: DocumentId,
    shown: &ImageAdjustViewModel,
    fields: [String; 9],
    gray_index: isize,
) -> Option<UiIntent> {
    let [brightness, contrast, gamma, sharpen_amount, sharpen_radius, crop_x, crop_y, crop_width, crop_height] = fields;
    let input = ImageAdjustInput {
        brightness,
        contrast,
        gamma,
        sharpen_amount,
        sharpen_radius,
        gray: gray_method(gray_index).unwrap_or(shown.input.gray),
        crop_x,
        crop_y,
        crop_width,
        crop_height,
    };
    if shown.issue.is_none() && shown.input == input {
        return None;
    }
    Some(UiIntent::SetImageAdjust { document, input: Box::new(input) })
}

/// `SetTraceForm` with every field, pop-up and checkbox as they stand.
pub fn trace_form_intent(
    document: DocumentId,
    shown: &TraceFormViewModel,
    fields: [String; 4],
    mode_index: isize,
    threshold_index: isize,
    invert: bool,
    curves: bool,
) -> Option<UiIntent> {
    let [dpi, threshold, ignore_below_mm2, smoothness_mm] = fields;
    let input = TraceFormInput {
        mode: usize::try_from(mode_index).ok().and_then(|i| TRACE_MODES.get(i).copied()).unwrap_or(shown.input.mode),
        dpi,
        threshold_choice: usize::try_from(threshold_index)
            .ok()
            .and_then(|i| THRESHOLD_CHOICES.get(i).copied())
            .unwrap_or(shown.input.threshold_choice),
        threshold,
        invert,
        ignore_below_mm2,
        smoothness_mm,
        curves,
    };
    if shown.issue.is_none() && shown.input == input {
        return None;
    }
    Some(UiIntent::SetTraceForm { document, input: Box::new(input) })
}

/// `ApplyTracePreset` for recipe pop-up item `index`.
pub fn trace_preset_intent(document: DocumentId, index: isize) -> Option<UiIntent> {
    Some(UiIntent::ApplyTracePreset { document, preset: trace_preset(index)? })
}

/// `SetTraceForm` with the resolution found in the file in the DPI field.
pub fn trace_suggestion_intent(document: DocumentId, shown: &TraceFormViewModel) -> Option<UiIntent> {
    let dpi = number(shown.dpi_suggestion?, 2, Locale::En);
    if shown.issue.is_none() && shown.input.dpi == dpi {
        return None;
    }
    Some(UiIntent::SetTraceForm { document, input: Box::new(TraceFormInput { dpi, ..shown.input.clone() }) })
}

/// The save panel's suggested file name: the document title as `.png`.
pub fn suggested_bitmap_name(title: &str) -> String {
    let title = title.trim();
    let stem = match title.len().checked_sub(".burnburn".len()) {
        Some(at) if title.is_char_boundary(at) && title[at..].eq_ignore_ascii_case(".burnburn") => &title[..at],
        _ => title,
    };
    let stem = if stem.is_empty() { "Naamloos" } else { stem };
    format!("{stem}.png")
}
