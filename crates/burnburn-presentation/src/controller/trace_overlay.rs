//! What the open tracer shows on the canvas (G7-trace, T03, T04): the centre lines as strokes, and
//! while corners or a brush are asked for, the photo lying on the sheet with what was pointed at.

use std::sync::Arc;

use burnburn_editor::{BedPicture, BedQuad, Overlay, SnapKind};
use burnburn_geometry::{Path, Segment, Subpath};
use burnburn_types::{ContentHash, HashDomain};

use crate::image_panel::TraceMode;

use super::Session;
use super::trace_g7::pick_scene;

/// The centre lines of the open tracer as strokes, where applying them would put them (identity
/// placement); the photo being pointed at and the marks on it. Window aids: never document data.
/// Nothing for a contour trace that is not being pointed at, or without a result.
pub(super) fn overlays(s: &Session) -> Vec<Overlay> {
    let Some(t) = s.tracer.as_ref() else { return Vec::new() };
    let mut out = Vec::new();
    if t.form.mode == TraceMode::Centerline
        && let Some(c) = t.centerline.as_ref()
    {
        out.push(Overlay::ResultPreview { path: Arc::clone(&c.preview) });
    }
    if let Some(scene) = pick_scene(t) {
        let rgba: Vec<u8> = scene.picture.gray8.iter().flat_map(|g| [*g, *g, *g, 255]).collect();
        let hash = ContentHash::of(HashDomain::SourcePixels, &scene.picture.gray8);
        let mut id = [0u8; 8];
        id.copy_from_slice(&hash.as_bytes()[..8]);
        if let Some(picture) =
            BedPicture::new(u64::from_be_bytes(id), scene.picture.width, scene.picture.height, Arc::from(rgba))
        {
            let quad = BedQuad { bed_mm: scene.quad, uv: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] };
            out.push(Overlay::BedImage { picture: Arc::new(picture), quads: Arc::from(vec![quad]), opacity: 1.0 });
        }
        out.extend(scene.corners.iter().map(|at| Overlay::SnapCandidate { at: *at, kind: SnapKind::Endpoint }));
        if let [first, rest @ ..] = scene.stroke.as_slice() {
            let segments = rest.iter().map(|p| Segment::Line { to: *p }).collect();
            if let Ok(stroke) = Subpath::new(*first, segments, false) {
                out.push(Overlay::ResultPreview { path: Arc::new(Path::from_subpaths(vec![stroke])) });
            }
        }
    }
    out
}
