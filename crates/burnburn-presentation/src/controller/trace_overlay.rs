//! What the open tracer shows on the canvas (G7-trace, T03, T04): the centre lines as strokes, and
//! while corners or a brush are asked for, the photo lying on the sheet with what was pointed at.

use std::sync::Arc;

use burnburn_editor::{BedPicture, BedQuad, CompareSide, NodeTarget, Overlay, SnapKind, path_nodes};
use burnburn_geometry::{Path, Segment, Subpath};
use burnburn_types::{ContentHash, HashDomain};

use crate::image_panel::TraceMode;

use super::Session;
use super::trace::Tracer;
use super::trace_g7::{PickMode, pick_scene};

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
        out.extend(stroke_nodes(&c.preview));
    }
    // Both variants of the A/B comparison, once they are traced: contour traces only.
    if t.form.mode == TraceMode::Contour
        && let Some(result) = t.study.compare_result()
    {
        out.push(Overlay::CompareOutline { path: Arc::new(result.a_result.path()), side: CompareSide::A });
        out.push(Overlay::CompareOutline { path: Arc::new(result.b_result.path()), side: CompareSide::B });
    }
    out.extend(protect_overlays(t));
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

/// While contours or corners are being protected (G8-trace, T05, T06): every outline of the result in
/// force, the protected ones heavier, and the nodes of the fit with the protected corners marked.
fn protect_overlays(t: &Tracer) -> Vec<Overlay> {
    let corners_mode = match t.pick.as_ref().map(|p| &p.mode) {
        Some(PickMode::ProtectContours) => false,
        Some(PickMode::ProtectCorners) => true,
        _ => return Vec::new(),
    };
    let Some(result) = t.result.as_ref() else { return Vec::new() };
    let mut out = Vec::new();
    for c in &result.contours {
        let path = Arc::new(Path::from_subpaths(vec![c.path.clone()]));
        if t.protect.contains(&c.id) {
            out.push(Overlay::ProtectedOutline { path });
        } else {
            out.push(Overlay::ResultPreview { path });
        }
        if corners_mode {
            // Where the fit has a node: the candidates for a protected corner.
            out.extend(path_nodes(&Path::from_subpaths(vec![c.path.clone()])).into_iter().filter_map(
                |(target, at)| {
                    matches!(target, NodeTarget::Node(_)).then_some(Overlay::Node { at, target, selected: false })
                },
            ));
        }
    }
    let corners = t.recipe.as_ref().map(|r| r.contour.corners.clone()).unwrap_or_default();
    let (sx, sy) = (result.scale.mm_per_pixel_x(), result.scale.mm_per_pixel_y());
    for (i, corner) in corners.iter().enumerate() {
        let at = burnburn_geometry::Point::new(corner[0] * sx, (f64::from(result.height_px) - corner[1]) * sy);
        let target = NodeTarget::Node(burnburn_geometry::NodeRef { subpath: 0, node: i });
        out.push(Overlay::Node { at, target, selected: true });
    }
    out
}

/// A marker on every node of the strokes (T04): the open ends of a stroke as selected (larger, hollow)
/// markers, the nodes between them as plain ones.
fn stroke_nodes(strokes: &Path) -> Vec<Overlay> {
    let mut out = Vec::new();
    for (si, sub) in strokes.subpaths().iter().enumerate() {
        let last = sub.segments().len();
        let points = std::iter::once(sub.start()).chain(sub.segments().iter().map(|seg| seg.end()));
        for (i, at) in points.enumerate() {
            let target = NodeTarget::Node(burnburn_geometry::NodeRef { subpath: si, node: i });
            let open_end = !sub.is_closed() && (i == 0 || i == last);
            out.push(Overlay::Node { at, target, selected: open_end });
        }
    }
    out
}
