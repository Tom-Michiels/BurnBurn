//! Canvas accessibility without AppKit: every visible document object becomes one element with
//! its name, its physical size and its frame in canvas points (top-left origin, y down), taken
//! from the same `SceneSnapshot` the GPU draws. The AppKit layer wraps them in
//! `NSAccessibilityElement`s; the Windows host can do the same for UI Automation.
//!
//! Interim: contracts-phase-c §IM20 lets presentation provide this `AccessibilityTree`; until it
//! does, the host derives it here from the scene and the object rows (`docs/coordination/gui.md`).

use burnburn_editor::SceneSnapshot;
use burnburn_geometry::Point;
use burnburn_presentation::{AppViewModel, format_mm};
use burnburn_types::{DocumentId, GeometryId};

use crate::l10n::Locale;

#[derive(Debug, Clone, PartialEq)]
pub struct AxElement {
    pub object: GeometryId,
    /// The object's name as in the object list ("Rechthoek 1").
    pub label: String,
    /// Physical size of its bounds ("120 × 80 mm").
    pub value: String,
    /// `[x, y, width, height]` in canvas points, top-left origin.
    pub frame_pt: [f64; 4],
    pub selected: bool,
}

/// Elements in object-list order. Objects without drawable geometry are left out.
pub fn canvas_elements(
    scene: &SceneSnapshot,
    vm: &AppViewModel,
    document: DocumentId,
    _locale: Locale,
) -> Vec<AxElement> {
    let Some(doc) = vm.document(document) else { return Vec::new() };
    let mut out = Vec::new();
    for row in &doc.objects {
        let Some((min, max)) = world_bounds(scene, row.id) else { continue };
        let corners =
            [Point::new(min.x, min.y), Point::new(max.x, min.y), Point::new(min.x, max.y), Point::new(max.x, max.y)]
                .map(|p| scene.camera.world_to_points(&scene.viewport, p));
        let xs = corners.map(|c| c[0]);
        let ys = corners.map(|c| c[1]);
        let (x0, x1) =
            (xs.iter().copied().fold(f64::INFINITY, f64::min), xs.iter().copied().fold(f64::NEG_INFINITY, f64::max));
        let (y0, y1) =
            (ys.iter().copied().fold(f64::INFINITY, f64::min), ys.iter().copied().fold(f64::NEG_INFINITY, f64::max));
        if ![x0, x1, y0, y1].iter().all(|v| v.is_finite()) {
            continue;
        }
        out.push(AxElement {
            object: row.id,
            label: row.label.clone(),
            value: format!("{} × {} mm", format_mm(max.x - min.x), format_mm(max.y - min.y)),
            frame_pt: [x0, y0, x1 - x0, y1 - y0],
            selected: row.selected,
        });
    }
    out
}

/// World-space bounds of every scene item of `object` (its transform applied to the path bounds).
fn world_bounds(scene: &SceneSnapshot, object: GeometryId) -> Option<(Point, Point)> {
    let mut acc: Option<(Point, Point)> = None;
    for item in scene.items.iter().filter(|i| i.source == object) {
        let Some(b) = item.path.bounds() else { continue };
        let (lo, hi) = (b.min(), b.max());
        for p in [Point::new(lo.x, lo.y), Point::new(hi.x, lo.y), Point::new(lo.x, hi.y), Point::new(hi.x, hi.y)] {
            let q = item.transform.apply(p);
            acc = Some(match acc {
                None => (q, q),
                Some((a, b)) => (Point::new(a.x.min(q.x), a.y.min(q.y)), Point::new(b.x.max(q.x), b.y.max(q.y))),
            });
        }
    }
    acc
}
