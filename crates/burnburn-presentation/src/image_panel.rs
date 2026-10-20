//! Input and view-model types of the image panels (W2-L1-beeld): bitmap import with DPI proposal
//! (P03), Adjust Image with live preview (T09), crop, and the tracer form (T01, T02, T04, T10).
//!
//! Field input is raw text as typed (DR-05); the controller parses and validates it. Nothing
//! here names AppKit or Windows types.

use std::sync::Arc;

use burnburn_document::process::GrayMethod;
use burnburn_types::OperationId;

/// Why an image panel input was not used. Hosts translate it; every issue names the field it
/// belongs to, so the field can be marked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ImageIssue {
    /// Not a number (or a number with a unit that does not belong).
    InvalidNumber(ImageField),
    /// A number outside the documented range of the field.
    OutOfRange(ImageField),
    /// The crop window is not inside the source image or has no area.
    CropOutsideSource,
    /// A crop needs all four values (or none).
    CropIncomplete,
    /// The file is not a PNG or JPEG the decoder can read (BMP/TIFF are not supported yet).
    UnsupportedFormat,
    /// The file could not be read or decoded.
    Unreadable,
    /// The source pixels of this image are not in the project.
    SourceMissing,
}

/// A field of the image panels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ImageField {
    Dpi,
    Brightness,
    Contrast,
    Gamma,
    SharpenAmount,
    SharpenRadius,
    CropX,
    CropY,
    CropWidth,
    CropHeight,
    TraceThreshold,
    TraceIgnoreBelow,
    TraceSmoothness,
}

/// Raw field text of the Adjust Image panel.
///
/// Ranges: brightness −1…1, contrast 0…10, gamma 0.1…10, sharpen amount 0…10 (0 = off), sharpen
/// radius ≥ 0.01 mm, crop in *source pixels* (all four empty = no crop).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageAdjustInput {
    pub brightness: String,
    pub contrast: String,
    pub gamma: String,
    pub sharpen_amount: String,
    pub sharpen_radius: String,
    pub gray: GrayMethod,
    pub crop_x: String,
    pub crop_y: String,
    pub crop_width: String,
    pub crop_height: String,
}

/// A preview bitmap: the gray values the laser would scan (burn = 0, white = 255), top row first,
/// at the physical scan grid of the operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImagePreview {
    pub width: u32,
    pub height: u32,
    pub gray8: Arc<[u8]>,
}

/// The bitmap import sheet (P03): file, pixel size, resolution proposal and the resulting
/// physical size.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageImportViewModel {
    pub file: String,
    pub width_px: u32,
    pub height_px: u32,
    /// Resolution found in the file (PNG `pHYs`, JPEG JFIF), only a proposal.
    pub dpi_suggestion: Option<f64>,
    /// The DPI as it stands in the field: the proposal, what the user typed, or empty while the
    /// file has no resolution (nothing is assumed; Importeer stays disabled until a value is given).
    pub dpi_text: String,
    /// Physical size in mm for the current DPI; `None` while the DPI is invalid.
    pub size_mm: Option<(f64, f64)>,
    pub issue: Option<ImageIssue>,
}

/// The Adjust Image sheet of one image operation.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageAdjustViewModel {
    pub operation: OperationId,
    /// Name of the operation.
    pub title: String,
    /// Size of the source image in pixels (`None` while the source is still being decoded).
    pub source_px: Option<(u32, u32)>,
    /// The values as typed; always what the preview shows or is computing.
    pub input: ImageAdjustInput,
    pub issue: Option<(ImageField, ImageIssue)>,
    /// A preview task is running; the last valid preview stays visible.
    pub computing: bool,
    pub preview: Option<ImagePreview>,
    /// Apply is possible: valid input that differs from what the operation has now.
    pub can_apply: bool,
}

/// Image panels of one document window.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImageViewModel {
    pub import: Option<ImageImportViewModel>,
    pub adjust: Option<ImageAdjustViewModel>,
}

/// What the tracer produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TraceMode {
    /// Filled outlines with holes (silhouettes, logos).
    #[default]
    Contour,
    /// Open centre lines of strokes (handwriting, line art; T04).
    Centerline,
}

/// How the black/white boundary is found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ThresholdChoice {
    #[default]
    Otsu,
    /// Gray level 0…255 from `TraceFormInput::threshold`.
    Fixed,
    /// Local threshold for uneven lighting (window and offset of the preset).
    Adaptive,
}

/// Built-in tracer recipes (T10): a named set of recipe values. Choosing one keeps the declared
/// resolution and replaces everything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TracePreset {
    /// Clean logo or silhouette: global threshold, curves.
    Logo,
    /// Photograph with uneven light: local threshold, median filter, small specks dropped.
    Photo,
    /// Technical drawing: straight segments only.
    Technical,
    /// Handwriting and line art: centre lines, open paths.
    Handwriting,
}

impl TracePreset {
    pub const ALL: &'static [TracePreset] =
        &[TracePreset::Logo, TracePreset::Photo, TracePreset::Technical, TracePreset::Handwriting];
}

/// Raw field text of the tracer panel.
///
/// `dpi` declares the physical scale of the source pixels (T01). `ignore_below_mm2` is
/// "Ignore less than" (parts and holes smaller than this area are dropped), `smoothness_mm` the
/// distance budget of the fitted outline, `curves` false = straight segments only (optimise).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceFormInput {
    pub mode: TraceMode,
    pub dpi: String,
    pub threshold_choice: ThresholdChoice,
    pub threshold: String,
    pub invert: bool,
    pub ignore_below_mm2: String,
    pub smoothness_mm: String,
    pub curves: bool,
}

/// The tracer form as the panel shows it: the values in force plus what was refused.
#[derive(Debug, Clone, PartialEq)]
pub struct TraceFormViewModel {
    pub input: TraceFormInput,
    /// The preset the values still equal, if any.
    pub preset: Option<TracePreset>,
    pub issue: Option<(ImageField, ImageIssue)>,
    /// Resolution found in the file; a proposal the user can apply.
    pub dpi_suggestion: Option<f64>,
}
