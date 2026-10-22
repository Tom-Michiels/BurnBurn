//! What the open tracer shows on the canvas (G7-trace, T04).

use burnburn_editor::Overlay;

use crate::image_panel::TraceMode;

use super::Session;

/// The centre lines of the open tracer as strokes, where applying them would put them (identity
/// placement). A window aid: never document data. Nothing for a contour trace or without a result.
pub(super) fn overlays(s: &Session) -> Vec<Overlay> {
    let Some(t) = s.tracer.as_ref().filter(|t| t.form.mode == TraceMode::Centerline) else { return Vec::new() };
    t.centerline
        .as_ref()
        .map(|c| Overlay::ResultPreview { path: std::sync::Arc::clone(&c.preview) })
        .into_iter()
        .collect()
}
