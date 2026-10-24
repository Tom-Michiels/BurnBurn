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
    /// The layer-palette slot of the Fill / Offset Fill operation that fills this object: the canvas
    /// draws a light fill in that colour. `None` for line-only objects.
    pub fill_slot: Option<u16>,
}

/// Which variant of an A/B comparison an outline belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareSide {
    /// The recipe in force.
    A,
    /// The recipe it is compared with.
    B,
}

/// Which physical bed corner is the controller's origin (the editor's own copy; no machine types).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BedOrigin {
    BottomLeft,
    TopLeft,
    TopRight,
    BottomRight,
}

/// Window-local decorations drawn above the items (the grid below them).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Overlay {
    /// Background grid; spacing chosen from the zoom so lines stay readable.
    Grid {
        spacing_mm: f64,
        /// Every n-th line (counted from the world origin) is a major line; 0 means no major lines.
        major_every: u32,
    },
    /// The machine's work area (bed) in bed view (y up, `(0, 0)` bottom-left) and the corner that
    /// is the controller's origin. Drawn beneath the grid; the grid is only drawn inside it.
    Bed {
        width_mm: f64,
        height_mm: f64,
        origin: BedOrigin,
    },
    /// Union of the selected objects' bounds.
    SelectionBounds {
        bounds: Bounds,
    },
    /// A selection handle (eight scale grips, one rotate grip). The rotate grip hangs on a stem
    /// that starts at `stem_from` (the top edge's midpoint); `hovered` grips are drawn larger.
    Handle {
        at: Point,
        handle: Handle,
        stem_from: Option<Point>,
        hovered: bool,
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
    /// An outline of the tracer's result that the user protected (G8-trace, T05); drawn heavier than
    /// the plain outlines of `ResultPreview`. A window aid; never document data.
    ProtectedOutline {
        path: Arc<Path>,
    },
    /// One variant of the tracer's A/B comparison, laid on the sheet where applying it would put
    /// it (G8-trace, T10). A window aid; never document data.
    CompareOutline {
        path: Arc<Path>,
        side: CompareSide,
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
    /// A camera picture on the bed (W3-UI-camera): drawn beneath the grid and the design, with
    /// `opacity` 0..=1. A visual placement aid only; never document data.
    BedImage {
        picture: Arc<crate::bed_image::BedPicture>,
        quads: Arc<[crate::bed_image::BedQuad]>,
        opacity: f32,
    },
    /// A bitmap drawn on top of the design objects it belongs to (W4-UI-machine): the live
    /// preview of Adjust Image on the image's bounds. Selection marks stay above it. Never
    /// document data.
    ImagePreview {
        picture: Arc<crate::bed_image::BedPicture>,
        quads: Arc<[crate::bed_image::BedQuad]>,
        opacity: f32,
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

impl SceneSnapshot {
    /// The same scene with the selection grip under the pointer marked, so that it can be drawn
    /// larger (LF9). `None` clears the mark.
    pub fn with_hovered_handle(mut self, hovered: Option<Handle>) -> Self {
        for overlay in &mut self.overlays {
            if let Overlay::Handle { handle, hovered: flag, .. } = overlay {
                *flag = hovered == Some(*handle);
            }
        }
        self
    }
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

/// How many grid lines make a major line, so that majors fall on powers of ten: spacing 1 → 10,
/// 2 → 5, 5 → 2 (times a decade).
pub fn grid_major_every(spacing_mm: f64) -> u32 {
    if !(spacing_mm.is_finite() && spacing_mm > 0.0) {
        return 0;
    }
    let decade = 10f64.powf(spacing_mm.log10().floor());
    let mantissa = spacing_mm / decade;
    // Guard against log10 rounding just below a power of ten (mantissa ~ 10 is 1 of the next decade).
    let mantissa = if mantissa > 9.999 { 1.0 } else { mantissa };
    (10.0 / mantissa).round().clamp(1.0, 10.0) as u32
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
            crate::work::scene_item();
            Some(SceneItem {
                source: id,
                path: Arc::clone(obj.path()),
                transform: Affine::IDENTITY,
                role: if req.selection.contains(id) { StyleRole::Selected } else { StyleRole::Design },
                fill_slot: None,
            })
        })
        .collect();
    let spacing_mm = grid_spacing_mm(&req.camera);
    let mut overlays = vec![Overlay::Grid { spacing_mm, major_every: grid_major_every(spacing_mm) }];
    let mm_per_point = req.camera.mm_per_point();
    let gesture = req.preview.is_some();
    if let Some(bounds) = req.selection.bounds(req.snapshot) {
        overlays.push(Overlay::SelectionBounds { bounds });
        if req.tool == ToolKind::Select && !gesture {
            let top_middle = Point::new((bounds.min().x + bounds.max().x) / 2.0, bounds.max().y);
            overlays.extend(crate::tool::selection_handles(bounds, mm_per_point).into_iter().map(|(handle, at)| {
                Overlay::Handle {
                    at,
                    handle,
                    stem_from: (handle == Handle::Rotate).then_some(top_middle),
                    hovered: false,
                }
            }));
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
    fn major_grid_lines_fall_on_decades_for_every_1_2_5_spacing() {
        for (spacing, every) in [(1.0, 10), (2.0, 5), (5.0, 2), (0.5, 2), (20.0, 5), (50.0, 2), (0.1, 10)] {
            assert_eq!(grid_major_every(spacing), every, "spacing {spacing}");
        }
        // And the scene carries it, whatever the zoom.
        let snap = doc();
        let sel = Selection::default();
        let scene = build_scene(&request(&snap, &sel, None, ToolKind::Select));
        let Some(Overlay::Grid { spacing_mm, major_every }) = scene.overlays.first() else { panic!("no grid") };
        assert_eq!(*major_every, grid_major_every(*spacing_mm));
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
        assert!(
            scene.overlays.contains(&Overlay::SnapCandidate { at: Point::new(40.0, 0.0), kind: SnapKind::Endpoint })
        );
        assert!(matches!(scene.overlays.first(), Some(Overlay::Grid { .. })), "grid below everything");
    }

    /// LF9: eight scale grips (corners and edge midpoints) plus a rotate grip on a stem.
    #[test]
    fn selection_has_eight_scale_grips_and_a_rotate_grip_on_a_stem() {
        use crate::tool::ScaleGrip::*;
        let snap = doc();
        let mut sel = Selection::default();
        sel.apply(&SelectionChange::Replace(vec![id(1), id(2)]));
        let scene = build_scene(&request(&snap, &sel, None, ToolKind::Select));
        let handles: Vec<_> = scene
            .overlays
            .iter()
            .filter_map(|o| match o {
                Overlay::Handle { at, handle, stem_from, hovered } => Some((*handle, *at, *stem_from, *hovered)),
                _ => None,
            })
            .collect();
        assert_eq!(handles.len(), 9);
        let scale = |g| handles.iter().find(|h| h.0 == Handle::Scale(g)).map(|h| h.1);
        // Selection bounds (0,0)-(60,20).
        assert_eq!(scale(BottomLeft), Some(Point::new(0.0, 0.0)));
        assert_eq!(scale(Top), Some(Point::new(30.0, 20.0)));
        assert_eq!(scale(Right), Some(Point::new(60.0, 10.0)));
        assert_eq!(scale(Left), Some(Point::new(0.0, 10.0)));
        assert_eq!(scale(Bottom), Some(Point::new(30.0, 0.0)));
        assert_eq!(handles.iter().filter(|h| matches!(h.0, Handle::Scale(_))).count(), 8);
        let rotate = handles.iter().find(|h| h.0 == Handle::Rotate).unwrap();
        assert_eq!(rotate.1, Point::new(30.0, 20.0 + ROTATE_HANDLE_OFFSET_PT * 0.5));
        assert_eq!(rotate.2, Some(Point::new(30.0, 20.0)), "the stem starts at the top edge");
        assert!(handles.iter().all(|h| !h.3), "nothing hovered by default");
        assert!(handles.iter().filter(|h| h.0 != Handle::Rotate).all(|h| h.2.is_none()));
    }

    /// LF9: the hovered grip is marked in the scene (a render hint, never document data).
    #[test]
    fn hovered_grip_is_marked_in_the_scene() {
        let snap = doc();
        let mut sel = Selection::default();
        sel.apply(&SelectionChange::Replace(vec![id(1)]));
        let scene = build_scene(&request(&snap, &sel, None, ToolKind::Select))
            .with_hovered_handle(Some(Handle::Scale(crate::tool::ScaleGrip::Right)));
        let hovered: Vec<_> = scene
            .overlays
            .iter()
            .filter_map(|o| match o {
                Overlay::Handle { handle, hovered: true, .. } => Some(*handle),
                _ => None,
            })
            .collect();
        assert_eq!(hovered, vec![Handle::Scale(crate::tool::ScaleGrip::Right)]);
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
