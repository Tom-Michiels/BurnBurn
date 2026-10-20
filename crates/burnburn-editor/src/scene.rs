//! Scene snapshot (contract §5): an immutable view of one document revision plus window overlays,
//! for one window. It is not the document and not a job plan; `burnburn-render` turns it into
//! device pixels. Geometry stays `burnburn_geometry::Path` in world millimetres (`f64`).

use std::sync::Arc;

use burnburn_document::DocumentSnapshot;
use burnburn_geometry::{Affine, Bounds, NodeRef, Path, Point};
use burnburn_types::{DocumentId, GeometryId, RevisionId};

use crate::issues::{GeometryIssue, IssueKind};
use crate::selection::Selection;
use crate::sketch::{PointFreedom, SketchGlyph, SketchMark};
use crate::snap::{SnapCandidate, SnapKind};
use crate::tool::{Handle, NodeTarget, ToolKind, ToolPreview};

use crate::view::{Camera, Viewport};

/// Semantic styling. Colours, dashes and widths are a render/theme decision (X03: not colour-only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StyleRole {
    Design,
    ProcessLayer(u16),
    Selected,
    Hovered,
    Disabled,
}

/// One document object as drawn: resolved path, placement and role.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneItem {
    pub source: GeometryId,
    pub path: Arc<Path>,
    pub transform: Affine,
    pub role: StyleRole,
}

/// Window-local decorations drawn above the items (the grid below them).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Overlay {
    /// Background grid; spacing chosen from the zoom so lines stay readable.
    Grid {
        spacing_mm: f64,
    },
    /// Union of the selected objects' bounds.
    SelectionBounds {
        bounds: Bounds,
    },
    /// A selection handle (scale corners, rotate).
    Handle {
        at: Point,
        handle: Handle,
    },
    /// A handle of Warp or Deform; `index` is the handle's place in `ReshapePreview::handles`. An
    /// `invalid` set of handles (non-convex quadrilateral, folded net) is drawn as a problem.
    ReshapeHandle {
        at: Point,
        index: usize,
        active: bool,
        invalid: bool,
    },
    /// One solution of Tangent Circle; the `chosen` one is what a click would create.
    TangentCandidate {
        path: Arc<Path>,
        chosen: bool,
    },
    /// Where a tangent circle touches one of its targets (Tangent Circle, all solutions shown).
    TangentContact {
        at: Point,
    },
    /// A path node or Bézier handle in node-edit mode; `selected` nodes are the ones the node
    /// buttons act on (never set for a handle).
    Node {
        at: Point,
        target: NodeTarget,
        selected: bool,
    },
    /// The named snap target under the pointer (label next to the cursor).
    SnapCandidate {
        at: Point,
        kind: SnapKind,
    },
    RubberBand {
        bounds: Bounds,
    },
    /// Temporary gesture geometry (drawing, move/scale/rotate, node drag); never document data.
    DragPreview {
        path: Arc<Path>,
        transform: Affine,
    },
    /// A located geometry problem (E09).
    Issue {
        at: Point,
        kind: IssueKind,
    },
    /// A computed result not yet applied (boolean preview); drawn distinct from document items.
    ResultPreview {
        path: Arc<Path>,
    },
    /// A sketch point in sketch mode, with how free it still is.
    SketchPoint {
        at: Point,
        freedom: PointFreedom,
        selected: bool,
    },
    /// A sketch line or circle emphasised because it is selected or part of a conflict.
    SketchEntity {
        path: Arc<Path>,
        mark: SketchMark,
    },
    /// The symbol of a sketch relation, drawn beside the geometry it relates.
    SketchGlyph {
        at: Point,
        glyph: SketchGlyph,
        mark: SketchMark,
    },
    /// A sketch dimension: the lines to draw (extension and dimension lines, world mm) and its
    /// label. `text` already has the brackets of a driven dimension and the R/Ø prefix.
    SketchDimension {
        lines: Vec<[Point; 2]>,
        label_at: Point,
        text: String,
        driven: bool,
        mark: SketchMark,
    },
    /// A relation suggested while drawing, shown dashed; applied when the gesture ends.
    SketchSuggestion {
        at: Point,
        glyph: SketchGlyph,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneSnapshot {
    pub document: DocumentId,
    pub revision: RevisionId,
    pub camera: Camera,
    pub viewport: Viewport,
    /// Back to front.
    pub items: Vec<SceneItem>,
    pub overlays: Vec<Overlay>,
}

/// Everything one window contributes to its scene.
pub struct SceneRequest<'a> {
    pub document: DocumentId,
    pub snapshot: &'a DocumentSnapshot,
    pub selection: &'a Selection,
    pub tool: ToolKind,
    pub preview: Option<&'a ToolPreview>,
    pub snap: Option<SnapCandidate>,
    pub issues: &'a [GeometryIssue],
    /// Overlays the window adds on top (sketch mode, see `sketch_overlays`).
    pub extra: &'a [Overlay],
    /// A computed result shown before it is applied (boolean preview, E05).
    pub result_preview: Option<Arc<Path>>,
    /// The nodes selected in node-edit mode (`Editor::node_selection`); drawn as selected.
    pub selected_nodes: &'a [NodeRef],
    pub camera: Camera,
    pub viewport: Viewport,
}

/// Minimum distance between grid lines on screen, in logical points.
pub const MIN_GRID_SPACING_PT: f64 = 8.0;

/// The finest 1-2-5 spacing (mm) whose lines are at least [`MIN_GRID_SPACING_PT`] apart.
pub fn grid_spacing_mm(camera: &Camera) -> f64 {
    let min_mm = MIN_GRID_SPACING_PT / camera.points_per_mm();
    let mut decade = 10f64.powf(min_mm.log10().floor());
    // Guard against log10 rounding just below a power of ten.
    if decade * 10.0 <= min_mm {
        decade *= 10.0;
    }
    for _ in 0..3 {
        for step in [1.0, 2.0, 5.0] {
            let spacing = decade * step;
            if spacing >= min_mm * (1.0 - 1e-12) {
                return spacing;
            }
        }
        decade *= 10.0;
    }
    decade
}

/// Builds the scene for one window.
pub fn build_scene(req: &SceneRequest<'_>) -> SceneSnapshot {
    // Back to front in draw order; hidden objects are not drawn.
    let items = req
        .snapshot
        .draw_order()
        .into_iter()
        .filter(|id| !req.snapshot.is_hidden(*id))
        .filter_map(|id| {
            let obj = req.snapshot.object(id)?;
            Some(SceneItem {
                source: id,
                path: Arc::clone(obj.path()),
                transform: Affine::IDENTITY,
                role: if req.selection.contains(id) { StyleRole::Selected } else { StyleRole::Design },
            })
        })
        .collect();
    let mut overlays = vec![Overlay::Grid { spacing_mm: grid_spacing_mm(&req.camera) }];
    let mm_per_point = req.camera.mm_per_point();
    let gesture = req.preview.is_some();
    if let Some(bounds) = req.selection.bounds(req.snapshot) {
        overlays.push(Overlay::SelectionBounds { bounds });
        if req.tool == ToolKind::Select && !gesture {
            overlays.extend(
                crate::tool::selection_handles(bounds, mm_per_point)
                    .into_iter()
                    .map(|(handle, at)| Overlay::Handle { at, handle }),
            );
        }
    }
    if req.tool == ToolKind::Node
        && let Some(obj) = req.selection.single().and_then(|id| req.snapshot.object(id))
    {
        let path = match req.preview {
            Some(ToolPreview::Node { path, .. }) => Arc::clone(path),
            _ => Arc::clone(obj.path()),
        };
        overlays.extend(crate::tool::path_nodes(&path).into_iter().map(|(target, at)| {
            let selected = matches!(target, NodeTarget::Node(n) if req.selected_nodes.contains(&n));
            Overlay::Node { at, target, selected }
        }));
    }
    match req.preview {
        Some(ToolPreview::Rectangle(p)) => {
            if let Some(rect) = crate::tool::rect_from_bounds(p.bounds()) {
                overlays
                    .push(Overlay::DragPreview { path: Arc::new(Path::rectangle(rect)), transform: Affine::IDENTITY });
            }
        }
        Some(ToolPreview::Shape { path, .. }) | Some(ToolPreview::Node { path, .. }) => {
            overlays.push(Overlay::DragPreview { path: Arc::clone(path), transform: Affine::IDENTITY });
        }
        Some(ToolPreview::RubberBand { bounds }) => overlays.push(Overlay::RubberBand { bounds: *bounds }),
        Some(ToolPreview::Transform { targets, transform, .. }) => {
            overlays.extend(
                targets
                    .iter()
                    .filter_map(|id| req.snapshot.object(*id))
                    .map(|obj| Overlay::DragPreview { path: Arc::clone(obj.path()), transform: *transform }),
            );
        }
        Some(ToolPreview::Reshape(p)) => {
            overlays.push(Overlay::DragPreview { path: Arc::clone(&p.frame), transform: Affine::IDENTITY });
            overlays.extend(
                p.items
                    .iter()
                    .map(|(_, path)| Overlay::DragPreview { path: Arc::clone(path), transform: Affine::IDENTITY }),
            );
            overlays.extend(p.handles.iter().enumerate().map(|(index, at)| Overlay::ReshapeHandle {
                at: *at,
                index,
                active: p.active == Some(index),
                invalid: p.error.is_some(),
            }));
        }
        Some(ToolPreview::Tangent(p)) => {
            overlays.push(Overlay::DragPreview { path: Arc::clone(&p.picks), transform: Affine::IDENTITY });
            overlays.extend(
                p.candidates
                    .iter()
                    .enumerate()
                    .map(|(i, path)| Overlay::TangentCandidate { path: Arc::clone(path), chosen: p.chosen == Some(i) }),
            );
            overlays.extend(p.contacts.iter().map(|at| Overlay::TangentContact { at: *at }));
        }
        None => {}
    }
    if let Some(path) = &req.result_preview {
        overlays.push(Overlay::ResultPreview { path: Arc::clone(path) });
    }
    overlays.extend(req.issues.iter().map(|i| Overlay::Issue { at: i.at, kind: i.kind }));
    overlays.extend(req.extra.iter().cloned());
    if let Some(c) = req.snap {
        overlays.push(Overlay::SnapCandidate { at: c.at, kind: c.kind });
    }
    SceneSnapshot {
        document: req.document,
        revision: req.snapshot.revision(),
        camera: req.camera,
        viewport: req.viewport,
        items,
        overlays,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selection::SelectionChange;
    use crate::tool::fixture::{path_obj, rect};
    use crate::tool::{ROTATE_HANDLE_OFFSET_PT, TransformKind};
    use burnburn_geometry::{HandleRef, HandleSide, NodeRef};
    use burnburn_geometry::{PathBuilder, Vector};
    use burnburn_types::StableId;

    fn id(raw: u64) -> GeometryId {
        GeometryId::from_raw(raw)
    }

    fn camera() -> Camera {
        Camera::new(Point::new(0.0, 0.0), 2.0).unwrap()
    }

    fn viewport() -> Viewport {
        Viewport::new(800.0, 600.0, 2.0).unwrap()
    }

    fn request<'a>(
        snap: &'a DocumentSnapshot,
        sel: &'a Selection,
        preview: Option<&'a ToolPreview>,
        tool: ToolKind,
    ) -> SceneRequest<'a> {
        SceneRequest {
            document: DocumentId::from_raw(7),
            snapshot: snap,
            selection: sel,
            tool,
            preview,
            snap: None,
            issues: &[],
            extra: &[],
            result_preview: None,
            selected_nodes: &[],
            camera: camera(),
            viewport: viewport(),
        }
    }

    fn doc() -> DocumentSnapshot {
        DocumentSnapshot::from_parts(vec![rect(1, 0.0, 0.0, 40.0, 20.0), rect(2, 50.0, 0.0, 10.0, 10.0)], 3).unwrap()
    }

    #[test]
    fn items_follow_document_order_with_selection_role_and_shared_paths() {
        let snap = doc();
        let mut sel = Selection::default();
        sel.apply(&SelectionChange::Replace(vec![id(2)]));
        let scene = build_scene(&request(&snap, &sel, None, ToolKind::Select));
        assert_eq!((scene.document, scene.revision), (DocumentId::from_raw(7), snap.revision()));
        let roles: Vec<_> = scene.items.iter().map(|i| (i.source, i.role)).collect();
        assert_eq!(roles, vec![(id(1), StyleRole::Design), (id(2), StyleRole::Selected)]);
        assert!(Arc::ptr_eq(&scene.items[0].path, snap.object(id(1)).unwrap().path()), "no geometry copy");
        assert!(scene.items.iter().all(|i| i.transform == Affine::IDENTITY));
    }

    #[test]
    fn selection_shows_bounds_and_handles_and_the_snap_target() {
        let snap = doc();
        let mut sel = Selection::default();
        sel.apply(&SelectionChange::Replace(vec![id(1), id(2)]));
        let mut req = request(&snap, &sel, None, ToolKind::Select);
        req.snap = Some(SnapCandidate { at: Point::new(40.0, 0.0), kind: SnapKind::Endpoint, source: Some(id(1)) });
        let scene = build_scene(&req);
        let bounds = Bounds::from_corners(Point::new(0.0, 0.0), Point::new(60.0, 20.0));
        assert!(scene.overlays.contains(&Overlay::SelectionBounds { bounds }));
        assert!(scene.overlays.contains(&Overlay::Handle {
            at: Point::new(30.0, 20.0 + ROTATE_HANDLE_OFFSET_PT * 0.5),
            handle: Handle::Rotate
        }));
        assert_eq!(scene.overlays.iter().filter(|o| matches!(o, Overlay::Handle { .. })).count(), 5);
        assert!(
            scene.overlays.contains(&Overlay::SnapCandidate { at: Point::new(40.0, 0.0), kind: SnapKind::Endpoint })
        );
        assert!(matches!(scene.overlays.first(), Some(Overlay::Grid { .. })), "grid below everything");
    }

    #[test]
    fn transform_preview_draws_targets_moved_without_changing_items() {
        let snap = doc();
        let mut sel = Selection::default();
        sel.apply(&SelectionChange::Replace(vec![id(1)]));
        let m = Affine::translate(Vector::new(5.0, 0.0)).unwrap();
        let preview = ToolPreview::Transform { targets: vec![id(1)], transform: m, kind: TransformKind::Move };
        let scene = build_scene(&request(&snap, &sel, Some(&preview), ToolKind::Select));
        assert!(scene.items.iter().all(|i| i.transform == Affine::IDENTITY), "document geometry unchanged");
        assert!(
            scene
                .overlays
                .contains(&Overlay::DragPreview { path: Arc::clone(snap.object(id(1)).unwrap().path()), transform: m })
        );
        assert!(!scene.overlays.iter().any(|o| matches!(o, Overlay::Handle { .. })), "no handles while dragging");
    }

    #[test]
    fn node_tool_shows_nodes_of_the_single_selected_path_and_issues_are_marked() {
        let curve = PathBuilder::new()
            .move_to(Point::new(0.0, 0.0))
            .cubic_to(Point::new(0.0, 10.0), Point::new(10.0, 10.0), Point::new(10.0, 0.0))
            .build()
            .unwrap();
        let snap = DocumentSnapshot::from_parts(vec![path_obj(1, curve)], 2).unwrap();
        let mut sel = Selection::default();
        sel.apply(&SelectionChange::Replace(vec![id(1)]));
        let issues = [GeometryIssue {
            kind: IssueKind::OpenContour,
            at: Point::new(1.0, 1.0),
            objects: vec![id(1)],
            size_mm: Some(0.1),
        }];
        let mut req = request(&snap, &sel, None, ToolKind::Node);
        req.issues = &issues;
        let scene = build_scene(&req);
        let nodes: Vec<_> = scene
            .overlays
            .iter()
            .filter_map(|o| match o {
                Overlay::Node { target, .. } => Some(*target),
                _ => None,
            })
            .collect();
        assert_eq!(nodes.len(), 4);
        let n = |i| NodeRef { subpath: 0, node: i };
        assert!(nodes.contains(&NodeTarget::Handle(HandleRef { node: n(0), side: HandleSide::Out })));
        assert!(nodes.contains(&NodeTarget::Node(n(1))));
        assert!(scene.overlays.contains(&Overlay::Issue { at: Point::new(1.0, 1.0), kind: IssueKind::OpenContour }));
    }

    #[test]
    fn grid_spacing_is_1_2_5_and_at_least_the_minimum_on_screen() {
        let spacing = |ppm: f64| grid_spacing_mm(&Camera::new(Point::new(0.0, 0.0), ppm).unwrap());
        assert_eq!(spacing(2.0), 5.0, "≥ 4 mm");
        assert_eq!(spacing(8.0), 1.0);
        assert_eq!(spacing(4.0), 2.0);
        assert_eq!(spacing(20.0), 0.5);
        assert_eq!(spacing(0.1), 100.0);
        for ppm in [0.013, 0.7, 3.3, 47.0, 900.0] {
            let s = spacing(ppm);
            assert!(s * ppm >= MIN_GRID_SPACING_PT - 1e-9, "{ppm}: {s}");
            assert!(s * ppm < MIN_GRID_SPACING_PT * 2.5 + 1e-9, "finest that fits: {ppm}: {s}");
        }
    }
}
