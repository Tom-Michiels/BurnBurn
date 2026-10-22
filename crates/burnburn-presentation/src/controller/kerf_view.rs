//! The kerf on the canvas (C03, G5-cam): a selected shape on a Line layer with kerf shows the path
//! the beam follows, the same offset the compiler cuts (half the kerf into the waste, miter
//! joins). Only the selection is drawn, and only closed contours: an open contour has no side and
//! the compiler refuses it.

use std::sync::Arc;

use burnburn_editor::Overlay;
use burnburn_geometry::{Affine, OffsetJoin, ProductionTolerance, offset};
use burnburn_types::{CancellationToken, Length};

use super::Session;

/// More selected shapes than this are not drawn (an offset per shape is not free).
const MAX_SHAPES: usize = 50;
/// The display flattening of the shown path, mm.
const TOLERANCE_MM: f64 = 0.01;

pub(super) fn scene_overlays(s: &Session) -> Vec<Overlay> {
    let snapshot = s.doc.snapshot();
    let Some(tolerance) = Length::from_mm(TOLERANCE_MM).ok().and_then(|l| ProductionTolerance::new(l).ok()) else {
        return Vec::new();
    };
    let never = CancellationToken::never();
    let mut shown = Vec::new();
    for &g in s.selection.ids().iter().take(MAX_SHAPES) {
        let Some(object) = snapshot.object(g) else { continue };
        let mut widths: Vec<f64> = Vec::new();
        for op in snapshot.process().operations().filter(|o| o.output_enabled() && o.targets().contains(&g)) {
            if let burnburn_document::process::OperationMode::Line { kerf } = op.mode()
                && kerf.width_mm() > 0.0
                && !widths.contains(&kerf.width_mm())
            {
                widths.push(kerf.width_mm());
            }
        }
        for width in widths {
            if let Ok(path) = offset(object.path(), width / 2.0, OffsetJoin::Miter, &tolerance, &never)
                && !path.is_empty()
            {
                shown.push(Overlay::DragPreview { path: Arc::new(path), transform: Affine::IDENTITY });
            }
        }
    }
    shown
}
