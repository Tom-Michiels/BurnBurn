//! Offset options and preview (E05): window state and the view model; the preview itself is a
//! revision-checked task (`TaskInput::OffsetPreview`).

pub use burnburn_geometry::OffsetJoin;

/// How the next offset is made: corner shape, and whether the sources stay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OffsetOptions {
    pub join: OffsetJoin,
    /// True: the offset is a new object next to its source; false: it replaces the source.
    pub keep_sources: bool,
}

impl Default for OffsetOptions {
    fn default() -> Self {
        OffsetOptions { join: OffsetJoin::Round, keep_sources: true }
    }
}

/// What the preview of the open offset shows: the size of the result and its area.
#[derive(Debug, Clone, PartialEq)]
pub struct OffsetPreviewViewModel {
    /// The distance as normalised text (millimetres).
    pub distance: String,
    pub width: String,
    pub height: String,
    pub area_mm2: f64,
}

/// The offset panel: the options in force and the preview, when one is open and current.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OffsetViewModel {
    pub options: OffsetOptions,
    pub preview: Option<OffsetPreviewViewModel>,
}
