//! Warp 4 points and Deform 16 points (W3-UI-schikken): handles on the selection box, a live
//! preview of the reshaped outlines, Enter applies and Escape drops. The tool only holds the
//! handles; the result is the document command `Warp4`/`Deform16` and nothing is committed before
//! Enter. A handle position the domain cannot map (a non-convex quadrilateral, a folded net) stays
//! visible, is marked invalid and refuses to be applied.

use std::sync::Arc;

use burnburn_document::{DocumentSnapshot, ReshapeCommand};
use burnburn_geometry::{
    Bounds, DeformGrid, Path, PathBuilder, Point, PointMap, ProductionTolerance, QuadWarp, WarpError, WarpKind,
};
use burnburn_types::{CancellationToken, GeometryId, Length, RequestId, StableId};

use super::{
    EditCommit, HANDLE_RADIUS_PT, Rejection, Tool, ToolContext, ToolParam, ToolPreview, ToolStep, is_escape, key_down,
    tool_pointer,
};
use crate::arrange::ArrangeError;
use crate::input::{CanvasInput, CanvasKey, PointerPhase};
use crate::place::{PreviewError, reshape_operands, warp_preview, warp_start_corners};
use crate::snap::SnapCandidate;

/// The two reshape tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReshapeMode {
    /// Four corner handles.
    Warp,
    /// A 4 × 4 control net.
    Deform,
}

/// Selections with more segments than this are previewed by their frame only: the preview runs on
/// every pointer move and must stay interactive.
pub const PREVIEW_SEGMENT_LIMIT: usize = 4_000;

/// Error budget of the committed result (mm).
const COMMIT_TOLERANCE_MM: f64 = 0.01;

/// What a scene draws while the tool is active.
#[derive(Debug, Clone, PartialEq)]
pub struct ReshapePreview {
    pub mode: ReshapeMode,
    pub kind: WarpKind,
    /// Warp: `[bottom-left, bottom-right, top-right, top-left]`. Deform: the net, `row * 4 + column`,
    /// row 0 at the bottom.
    pub handles: Vec<Point>,
    /// The handle being dragged.
    pub active: Option<usize>,
    /// The quadrilateral (warp) or the net lines (deform).
    pub frame: Arc<Path>,
    /// The reshaped outlines; empty while nothing moved, or when the selection is too large to
    /// preview (`simplified`).
    pub items: Vec<(GeometryId, Arc<Path>)>,
    /// Why the handles cannot be applied as they are.
    pub error: Option<WarpError>,
    /// Some handle is off its start position.
    pub modified: bool,
    /// Only the frame is drawn: the selection has more than [`PREVIEW_SEGMENT_LIMIT`] segments.
    pub simplified: bool,
}

#[derive(Debug, Clone)]
struct Session {
    ids: Vec<GeometryId>,
    bounds: Bounds,
    start: Vec<Point>,
    handles: Vec<Point>,
    drag: Option<usize>,
    items: Vec<(GeometryId, Arc<Path>)>,
    error: Option<WarpError>,
    simplified: bool,
}

#[derive(Debug, Clone)]
pub struct ReshapeTool {
    mode: ReshapeMode,
    kind: WarpKind,
    session: Option<Session>,
    requests: u64,
}

impl ReshapeTool {
    pub(super) fn new(mode: ReshapeMode) -> Self {
        ReshapeTool { mode, kind: WarpKind::Projective, session: None, requests: 0 }
    }

    pub fn mode(&self) -> ReshapeMode {
        self.mode
    }

    /// Starts a session on the selection: the handles at their start positions. With a session
    /// already running on the same selection the handles stay and the preview is recomputed (after
    /// a change of the warp kind, say).
    pub fn begin(&mut self, ctx: &ToolContext<'_>) -> ToolStep {
        match reshape_operands(ctx.document, ctx.selection) {
            Err(ArrangeError::TooFewObjects { .. }) => {
                self.session = None;
                ToolStep::Rejected(Rejection::NeedsSelection)
            }
            Err(_) => {
                self.session = None;
                ToolStep::Rejected(Rejection::InvalidSelection)
            }
            Ok((ids, bounds)) => {
                let same = self.session.as_ref().is_some_and(|s| s.ids == ids && s.bounds == bounds);
                if !same {
                    self.session = Some(self.fresh(ctx.document, ids, bounds));
                }
                self.refresh(ctx);
                ToolStep::PreviewChanged
            }
        }
    }

    /// Puts every handle back on its start position.
    pub fn reset(&mut self, ctx: &ToolContext<'_>) -> ToolStep {
        let Some(session) = self.session.as_mut() else { return ToolStep::Nothing };
        session.handles = session.start.clone();
        session.drag = None;
        self.refresh(ctx);
        ToolStep::PreviewChanged
    }

    /// Applies the current handles as one command.
    pub fn apply(&mut self) -> ToolStep {
        let Some(session) = self.session.as_ref() else { return ToolStep::Nothing };
        if session.handles == session.start {
            return ToolStep::Nothing;
        }
        if session.error.is_some() {
            return ToolStep::Rejected(Rejection::InvalidValue);
        }
        let Some(tolerance) = Length::from_mm(COMMIT_TOLERANCE_MM).ok().and_then(|l| ProductionTolerance::new(l).ok())
        else {
            return ToolStep::Rejected(Rejection::InvalidValue);
        };
        let targets = session.ids.clone();
        let command = match self.mode {
            ReshapeMode::Warp => {
                let h = &session.handles;
                ReshapeCommand::Warp4 { targets, corners: [h[0], h[1], h[2], h[3]], kind: self.kind, tolerance }
            }
            ReshapeMode::Deform => {
                let mut net = [[Point::ORIGIN; 4]; 4];
                for (i, p) in session.handles.iter().enumerate() {
                    net[i / 4][i % 4] = *p;
                }
                ReshapeCommand::Deform16 { targets, net: Box::new(net), tolerance }
            }
        };
        self.session = None;
        ToolStep::Commit(EditCommit::Reshape(command))
    }

    fn fresh(&self, document: &DocumentSnapshot, ids: Vec<GeometryId>, bounds: Bounds) -> Session {
        let start: Vec<Point> = match self.mode {
            ReshapeMode::Warp => warp_start_corners(&bounds).to_vec(),
            ReshapeMode::Deform => DeformGrid::identity(bounds).net().iter().flatten().copied().collect(),
        };
        let segments: usize = ids
            .iter()
            .filter_map(|id| document.object(*id))
            .map(|o| o.path().subpaths().iter().map(|sp| sp.segments().len()).sum::<usize>())
            .sum();
        Session {
            ids,
            bounds,
            handles: start.clone(),
            start,
            drag: None,
            items: Vec::new(),
            error: None,
            simplified: segments > PREVIEW_SEGMENT_LIMIT,
        }
    }

    /// The selection of the context still is the one the session started on.
    fn keep_session(&mut self, ctx: &ToolContext<'_>) -> bool {
        match reshape_operands(ctx.document, ctx.selection) {
            Ok((ids, bounds)) => {
                if !self.session.as_ref().is_some_and(|s| s.ids == ids && s.bounds == bounds) {
                    self.session = Some(self.fresh(ctx.document, ids, bounds));
                }
                true
            }
            Err(_) => {
                self.session = None;
                false
            }
        }
    }

    /// Recomputes the error and the preview outlines from the handles.
    fn refresh(&mut self, ctx: &ToolContext<'_>) {
        let (mode, kind) = (self.mode, self.kind);
        let Some(session) = self.session.as_mut() else { return };
        session.items.clear();
        session.error = None;
        if session.handles == session.start {
            return;
        }
        let map: Result<Box<dyn PointMap>, WarpError> = match mode {
            ReshapeMode::Warp => {
                let h = &session.handles;
                QuadWarp::new(session.bounds, [h[0], h[1], h[2], h[3]], kind).map(|m| Box::new(m) as Box<dyn PointMap>)
            }
            ReshapeMode::Deform => {
                let mut net = [[Point::ORIGIN; 4]; 4];
                for (i, p) in session.handles.iter().enumerate() {
                    net[i / 4][i % 4] = *p;
                }
                DeformGrid::new(session.bounds, net).map(|m| Box::new(m) as Box<dyn PointMap>)
            }
        };
        let map = match map {
            Ok(map) => map,
            Err(e) => {
                session.error = Some(e);
                return;
            }
        };
        if session.simplified {
            return;
        }
        // Display accuracy: a quarter of a screen point.
        let Some(tolerance) =
            Length::from_mm((ctx.mm_per_point * 0.25).max(0.001)).ok().and_then(|l| ProductionTolerance::new(l).ok())
        else {
            return;
        };
        self.requests += 1;
        let request = RequestId::from_raw(self.requests);
        match warp_preview(
            ctx.document,
            request,
            &session.ids,
            &MapRef(map.as_ref()),
            &tolerance,
            &CancellationToken::never(),
        ) {
            Ok(preview) if preview.request == request => {
                session.items = preview.items.into_iter().map(|(id, path)| (id, Arc::new(path))).collect();
            }
            Ok(_) => {}
            Err(PreviewError::Warp(e)) => session.error = Some(e),
            Err(PreviewError::UnknownObject(_)) => self.session = None,
        }
    }

    fn frame(&self, session: &Session) -> Arc<Path> {
        let h = &session.handles;
        let path = match self.mode {
            ReshapeMode::Warp => Path::polyline(h, true).unwrap_or_default(),
            ReshapeMode::Deform => {
                let mut b = PathBuilder::new();
                for r in 0..4 {
                    b = b.move_to(h[r * 4]);
                    for c in 1..4 {
                        b = b.line_to(h[r * 4 + c]);
                    }
                }
                for c in 0..4 {
                    b = b.move_to(h[c]);
                    for r in 1..4 {
                        b = b.line_to(h[r * 4 + c]);
                    }
                }
                b.build().unwrap_or_default()
            }
        };
        Arc::new(path)
    }

    fn hit(&self, at: Point, radius_mm: f64) -> Option<usize> {
        let session = self.session.as_ref()?;
        session
            .handles
            .iter()
            .enumerate()
            .map(|(i, p)| (i, p.distance(at)))
            .filter(|(_, d)| *d <= radius_mm)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }
}

/// `warp_preview` wants a sized map; this lends a boxed one.
struct MapRef<'a>(&'a dyn PointMap);

impl PointMap for MapRef<'_> {
    fn domain(&self) -> Bounds {
        self.0.domain()
    }

    fn eval(&self, p: Point) -> Option<Point> {
        self.0.eval(p)
    }
}

impl Tool for ReshapeTool {
    fn input(&mut self, input: &CanvasInput, ctx: &ToolContext<'_>) -> ToolStep {
        if is_escape(input) {
            return self.cancel();
        }
        if let Some((CanvasKey::Return, _)) = key_down(input) {
            return self.apply();
        }
        let Some(ev) = tool_pointer(input) else { return ToolStep::Nothing };
        // The handles belong to the selection they were made for.
        let before = self.session.is_some();
        if !self.keep_session(ctx) {
            return if before { ToolStep::PreviewChanged } else { ToolStep::Nothing };
        }
        let restarted = !before;
        match ev.phase {
            PointerPhase::Down => {
                let radius = HANDLE_RADIUS_PT * ctx.mm_per_point;
                match self.hit(ev.position, radius) {
                    Some(i) => {
                        if let Some(s) = self.session.as_mut() {
                            s.drag = Some(i);
                        }
                        ToolStep::PreviewChanged
                    }
                    None => ToolStep::Nothing,
                }
            }
            PointerPhase::Move => {
                let Some(i) = self.session.as_ref().and_then(|s| s.drag) else {
                    return if restarted { ToolStep::PreviewChanged } else { ToolStep::Nothing };
                };
                if let Some(s) = self.session.as_mut() {
                    s.handles[i] = ev.position;
                }
                self.refresh(ctx);
                ToolStep::PreviewChanged
            }
            PointerPhase::Up | PointerPhase::Cancel => {
                let dragging = self.session.as_mut().is_some_and(|s| s.drag.take().is_some());
                if dragging || restarted { ToolStep::PreviewChanged } else { ToolStep::Nothing }
            }
        }
    }

    fn cancel(&mut self) -> ToolStep {
        let modified = self.session.as_ref().is_some_and(|s| s.handles != s.start);
        self.session = None;
        if modified { ToolStep::Cancelled } else { ToolStep::Nothing }
    }

    fn preview(&self) -> Option<ToolPreview> {
        let session = self.session.as_ref()?;
        Some(ToolPreview::Reshape(ReshapePreview {
            mode: self.mode,
            kind: self.kind,
            handles: session.handles.clone(),
            active: session.drag,
            frame: self.frame(session),
            items: session.items.clone(),
            error: session.error,
            modified: session.handles != session.start,
            simplified: session.simplified,
        }))
    }

    fn snap(&self) -> Option<SnapCandidate> {
        None
    }

    fn is_active(&self) -> bool {
        self.session.as_ref().is_some_and(|s| s.handles != s.start)
    }

    fn set_param(&mut self, param: ToolParam) -> bool {
        match param {
            ToolParam::Bilinear(b) if self.mode == ReshapeMode::Warp => {
                self.kind = if b { WarpKind::Bilinear } else { WarpKind::Projective };
                if let Some(s) = self.session.as_mut() {
                    // The outlines belong to the old kind; `begin` recomputes them.
                    s.items.clear();
                }
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{CanvasKey, PointerPhase};
    use crate::tool::fixture::{Fixture, at, key, none, rect};
    use crate::tool::{Editor, ToolKind};

    fn fixture() -> Fixture {
        let mut f = Fixture::new(vec![rect(1, 0.0, 0.0, 40.0, 20.0), rect(2, 60.0, 0.0, 20.0, 20.0)]);
        f.select(&[1, 2]);
        f
    }

    fn editor(kind: ToolKind, f: &Fixture) -> Editor {
        let mut e = Editor::new();
        e.select_tool(kind);
        assert_eq!(e.begin_tool(&f.ctx()), ToolStep::PreviewChanged);
        e
    }

    fn preview(e: &Editor) -> ReshapePreview {
        match e.preview() {
            Some(ToolPreview::Reshape(p)) => p,
            other => panic!("expected a reshape preview, got {other:?}"),
        }
    }

    fn drag(e: &mut Editor, f: &Fixture, from: (f64, f64), to: (f64, f64)) {
        e.input(&at(PointerPhase::Down, from.0, from.1, none()), &f.ctx());
        e.input(&at(PointerPhase::Move, to.0, to.1, none()), &f.ctx());
        e.input(&at(PointerPhase::Up, to.0, to.1, none()), &f.ctx());
    }

    #[test]
    fn warp_starts_with_the_four_corners_of_the_common_box_and_draws_nothing_moved() {
        let f = fixture();
        let e = editor(ToolKind::Warp, &f);
        let p = preview(&e);
        assert_eq!(
            p.handles,
            vec![Point::new(0.0, 0.0), Point::new(80.0, 0.0), Point::new(80.0, 20.0), Point::new(0.0, 20.0)]
        );
        assert!(!p.modified && p.items.is_empty() && p.error.is_none());
        assert!(!e.is_gesture_active(), "nothing to apply yet");
    }

    #[test]
    fn dragging_a_corner_previews_the_warped_outlines_without_touching_the_document() {
        let f = fixture();
        let mut e = editor(ToolKind::Warp, &f);
        drag(&mut e, &f, (80.0, 20.0), (70.0, 35.0));
        let p = preview(&e);
        assert_eq!(p.handles[2], Point::new(70.0, 35.0));
        assert!(p.modified && p.error.is_none());
        assert_eq!(p.items.len(), 2);
        // The second rectangle's top-right corner is the corner of the common box.
        let second = &p.items[1].1.subpaths()[0];
        let vertices: Vec<Point> = std::iter::once(second.start())
            .chain(second.segments().iter().map(burnburn_geometry::Segment::end))
            .collect();
        assert!(vertices.iter().any(|v| v.distance(Point::new(70.0, 35.0)) < 1e-6), "{vertices:?}");
        assert_eq!(f.snapshot.object(p.items[0].0).unwrap().bounds().unwrap().max(), Point::new(40.0, 20.0));
        assert!(e.is_gesture_active());
    }

    #[test]
    fn enter_commits_one_warp_with_the_handles_and_the_chosen_kind() {
        let f = fixture();
        let mut e = editor(ToolKind::Warp, &f);
        assert!(e.set_param(ToolParam::Bilinear(true)));
        drag(&mut e, &f, (80.0, 20.0), (70.0, 35.0));
        let step = e.input(&key(CanvasKey::Return), &f.ctx());
        let ToolStep::Commit(EditCommit::Reshape(ReshapeCommand::Warp4 { targets, corners, kind, .. })) = step else {
            panic!("expected a Warp4 commit, got {step:?}");
        };
        assert_eq!(targets, vec![GeometryId::from_raw(1), GeometryId::from_raw(2)]);
        assert_eq!(corners[2], Point::new(70.0, 35.0));
        assert_eq!(corners[0], Point::new(0.0, 0.0));
        assert_eq!(kind, WarpKind::Bilinear);
        assert!(e.preview().is_none(), "the session ended with the commit");
    }

    #[test]
    fn a_non_convex_quadrilateral_stays_visible_marked_invalid_and_cannot_be_applied() {
        let f = fixture();
        let mut e = editor(ToolKind::Warp, &f);
        // Pull the top-right corner far below the bottom edge: the quadrilateral folds.
        drag(&mut e, &f, (80.0, 20.0), (40.0, -30.0));
        let p = preview(&e);
        assert!(p.error.is_some(), "{p:?}");
        let step = e.input(&key(CanvasKey::Return), &f.ctx());
        assert!(matches!(step, ToolStep::Rejected(Rejection::InvalidValue)), "{step:?}");
        assert!(e.preview().is_some(), "the handles stay so the user can fix them");
        // Moving it back makes the warp valid again.
        drag(&mut e, &f, (40.0, -30.0), (90.0, 25.0));
        assert!(preview(&e).error.is_none());
        assert!(matches!(e.input(&key(CanvasKey::Return), &f.ctx()), ToolStep::Commit(_)));
    }

    #[test]
    fn escape_drops_the_session_and_commits_nothing() {
        let f = fixture();
        let mut e = editor(ToolKind::Warp, &f);
        drag(&mut e, &f, (80.0, 20.0), (70.0, 35.0));
        let step = e.input(&key(CanvasKey::Escape), &f.ctx());
        assert_eq!(step, ToolStep::Cancelled);
        assert!(e.preview().is_none());
    }

    #[test]
    fn reset_puts_the_handles_back_and_enter_without_a_change_commits_nothing() {
        let f = fixture();
        let mut e = editor(ToolKind::Warp, &f);
        drag(&mut e, &f, (80.0, 20.0), (70.0, 35.0));
        assert_eq!(e.reshape_reset(&f.ctx()), ToolStep::PreviewChanged);
        let p = preview(&e);
        assert!(!p.modified && p.items.is_empty());
        assert_eq!(p.handles[2], Point::new(80.0, 20.0));
        assert_eq!(e.input(&key(CanvasKey::Return), &f.ctx()), ToolStep::Nothing);
    }

    #[test]
    fn deform_has_sixteen_handles_and_commits_the_net() {
        let f = fixture();
        let mut e = editor(ToolKind::Deform, &f);
        let p = preview(&e);
        assert_eq!(p.handles.len(), 16);
        assert_eq!(p.handles[0], Point::new(0.0, 0.0));
        assert_eq!(p.handles[15], Point::new(80.0, 20.0));
        // Handle (row 1, column 1) of the regular net sits at one third of both sides.
        let inner = Point::new(80.0 / 3.0, 20.0 / 3.0);
        assert!(p.handles[5].distance(inner) < 1e-9);
        drag(&mut e, &f, (inner.x, inner.y), (30.0, 14.0));
        let p = preview(&e);
        assert_eq!(p.handles[5], Point::new(30.0, 14.0));
        assert!(p.modified && p.error.is_none() && p.items.len() == 2);
        let step = e.input(&key(CanvasKey::Return), &f.ctx());
        let ToolStep::Commit(EditCommit::Reshape(ReshapeCommand::Deform16 { targets, net, .. })) = step else {
            panic!("expected a Deform16 commit, got {step:?}");
        };
        assert_eq!(targets.len(), 2);
        assert_eq!(net[1][1], Point::new(30.0, 14.0));
        assert_eq!(net[3][3], Point::new(80.0, 20.0));
    }

    #[test]
    fn a_net_that_folds_the_plane_is_marked_invalid() {
        let f = fixture();
        let mut e = editor(ToolKind::Deform, &f);
        // Drag the corner handle across the opposite one: the patch turns inside out.
        drag(&mut e, &f, (0.0, 0.0), (120.0, 40.0));
        assert!(preview(&e).error.is_some());
        assert!(matches!(e.input(&key(CanvasKey::Return), &f.ctx()), ToolStep::Rejected(Rejection::InvalidValue)));
    }

    #[test]
    fn a_flat_or_empty_selection_has_nothing_to_reshape() {
        let mut f = Fixture::new(vec![rect(1, 0.0, 0.0, 40.0, 20.0)]);
        let mut e = Editor::new();
        e.select_tool(ToolKind::Warp);
        assert_eq!(e.begin_tool(&f.ctx()), ToolStep::Rejected(Rejection::NeedsSelection));
        f.select(&[1]);
        let line = crate::tool::fixture::path_obj(2, Path::line(Point::new(0.0, 0.0), Point::new(0.0, 9.0)).unwrap());
        let mut flat = Fixture::new(vec![line]);
        flat.select(&[2]);
        assert_eq!(e.begin_tool(&flat.ctx()), ToolStep::Rejected(Rejection::InvalidSelection));
        assert!(e.preview().is_none());
    }

    #[test]
    fn a_document_change_during_the_session_starts_it_over_from_the_new_geometry() {
        let f = fixture();
        let mut e = editor(ToolKind::Warp, &f);
        drag(&mut e, &f, (80.0, 20.0), (70.0, 35.0));
        let other = Fixture::new(vec![rect(1, 0.0, 0.0, 50.0, 50.0)]);
        let mut other = other;
        other.select(&[1]);
        e.input(&at(PointerPhase::Move, 5.0, 5.0, none()), &other.ctx());
        let p = preview(&e);
        assert!(!p.modified, "stale handles are not applied to a different document");
        assert_eq!(p.handles[2], Point::new(50.0, 50.0));
    }
}
