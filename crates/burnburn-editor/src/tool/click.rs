//! The shared engine of the point-sequence drawing tools (S-draw). A [`Variant`] says how many
//! points a tool needs and how they become a shape; [`DrawTool`] collects them from drag
//! (press, move, release) and click-click gestures, previews the shape under the pointer and
//! returns exactly one [`EditCommit`] when the last point is placed. Esc, a pointer cancel or a
//! tool switch leaves no half object.

use std::sync::Arc;

use burnburn_geometry::{Path, Point, Rect, Shape};

use super::{
    DRAG_THRESHOLD_PT, EditCommit, ShapeKind, Tool, ToolContext, ToolKind, ToolPreview, ToolStep, arc, circle, ellipse,
    is_escape, line, point, rect, slot, tool_pointer,
};
use crate::input::{CanvasInput, PointerPhase};
use crate::snap::SnapCandidate;

/// What a finished (or previewed) gesture produces.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Out {
    Shape(Shape, ShapeKind),
    Path(Path),
    Rect(Rect),
    /// Construction geometry: never part of production output.
    Construction(Shape),
    Point(Point),
}

impl Out {
    fn commit(self) -> EditCommit {
        match self {
            Out::Shape(shape, _) => EditCommit::CreateShape { shape },
            Out::Path(path) => EditCommit::CreatePath { path },
            Out::Rect(rect) => EditCommit::CreateRectangle { rect },
            Out::Construction(shape) => EditCommit::CreateConstruction { shape },
            Out::Point(at) => EditCommit::CreatePoint { at },
        }
    }

    fn preview(&self) -> Option<ToolPreview> {
        let (path, shape) = match self {
            Out::Shape(shape, kind) => (shape.to_path().ok()?, *kind),
            Out::Path(path) => (path.clone(), ShapeKind::Curve),
            Out::Rect(rect) => (Path::rectangle(*rect), ShapeKind::Polyline),
            Out::Construction(shape) => (shape.to_path().ok()?, ShapeKind::Line),
            Out::Point(_) => return None,
        };
        Some(ToolPreview::Shape { path: Arc::new(path), shape })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Variant {
    LineMidpoint,
    Centerline,
    RectCenter,
    RectThreePoint,
    CircleTwoPoint,
    CircleThreePoint,
    ArcThreePoint,
    ArcCenterPoint,
    EllipseAxes,
    SlotCenterToCenter,
    SlotOverall,
    SlotCenterPoint,
    SlotArcThreePoint,
    SlotArcCenter,
    Point,
}

impl Variant {
    pub(super) fn of(kind: ToolKind) -> Option<Variant> {
        Some(match kind {
            ToolKind::LineMidpoint => Variant::LineMidpoint,
            ToolKind::Centerline => Variant::Centerline,
            ToolKind::RectangleCenter => Variant::RectCenter,
            ToolKind::RectangleThreePoint => Variant::RectThreePoint,
            ToolKind::CircleTwoPoint => Variant::CircleTwoPoint,
            ToolKind::CircleThreePoint => Variant::CircleThreePoint,
            ToolKind::ArcThreePoint => Variant::ArcThreePoint,
            ToolKind::ArcCenterPoint => Variant::ArcCenterPoint,
            ToolKind::EllipseAxes => Variant::EllipseAxes,
            ToolKind::SlotCenterToCenter => Variant::SlotCenterToCenter,
            ToolKind::SlotOverall => Variant::SlotOverall,
            ToolKind::SlotCenterPoint => Variant::SlotCenterPoint,
            ToolKind::SlotArcThreePoint => Variant::SlotArcThreePoint,
            ToolKind::SlotArcCenter => Variant::SlotArcCenter,
            ToolKind::Point => Variant::Point,
            _ => return None,
        })
    }

    /// How many points complete the shape.
    pub(super) fn needed(self) -> usize {
        match self {
            Variant::Point => 1,
            Variant::LineMidpoint | Variant::Centerline | Variant::RectCenter | Variant::CircleTwoPoint => 2,
            Variant::RectThreePoint
            | Variant::CircleThreePoint
            | Variant::ArcThreePoint
            | Variant::ArcCenterPoint
            | Variant::EllipseAxes
            | Variant::SlotCenterToCenter
            | Variant::SlotOverall
            | Variant::SlotCenterPoint => 3,
            Variant::SlotArcThreePoint | Variant::SlotArcCenter => 4,
        }
    }

    /// Whether press-drag-release may place the first two points (click-click always works).
    fn drag(self) -> bool {
        !matches!(
            self,
            Variant::CircleThreePoint
                | Variant::ArcThreePoint
                | Variant::ArcCenterPoint
                | Variant::EllipseAxes
                | Variant::SlotArcThreePoint
                | Variant::SlotArcCenter
        )
    }

    /// The shape for exactly `needed()` points; `None` for a degenerate set.
    pub(super) fn build(self, pts: &[Point]) -> Option<Out> {
        line::build(self, pts)
            .or_else(|| rect::build(self, pts))
            .or_else(|| circle::build(self, pts))
            .or_else(|| arc::build(self, pts))
            .or_else(|| ellipse::build(self, pts))
            .or_else(|| slot::build(self, pts))
            .or_else(|| point::build(self, pts))
    }

    /// What to draw while fewer than `needed()` points are known (including the pointer).
    fn guide(self, pts: &[Point]) -> Option<Out> {
        slot::guide(self, pts).or_else(|| {
            let (first, rest) = pts.split_first()?;
            let last = rest.last()?;
            let mut points = vec![*first];
            points.extend(rest.iter().copied());
            (first != last).then_some(Out::Shape(Shape::Polyline { points, closed: false }, ShapeKind::Polyline))
        })
    }

    /// All points of the shape for a size typed while one point is placed and the pointer is at
    /// `cur`; `None` when the tool has no typed size.
    fn typed(self, p0: Point, cur: Point, w: f64, h: f64) -> Option<Vec<Point>> {
        let d = cur - p0;
        let len = d.length();
        let dir = if len > 0.0 { d * (1.0 / len) } else { burnburn_geometry::Vector::new(1.0, 0.0) };
        let left = burnburn_geometry::Vector::new(-dir.y, dir.x);
        let sign = |v: f64| if v < 0.0 { -1.0 } else { 1.0 };
        let (sx, sy) = (sign(d.x), sign(d.y));
        Some(match self {
            Variant::RectCenter => vec![p0, Point::new(p0.x + sx * w / 2.0, p0.y + sy * h / 2.0)],
            // The width is the diameter / the full length / the circumscribed diameter.
            Variant::CircleTwoPoint | Variant::Centerline => vec![p0, p0 + dir * w],
            Variant::LineMidpoint => vec![p0, p0 + dir * (w / 2.0)],
            // Length along the pointer direction, then the slot width.
            Variant::SlotCenterToCenter | Variant::SlotOverall => {
                let end = p0 + dir * w;
                vec![p0, end, p0.lerp(end, 0.5) + left * (h / 2.0)]
            }
            _ => return None,
        })
    }
}

/// Drag and click-click tool for every [`Variant`].
#[derive(Debug, Clone)]
pub struct DrawTool {
    variant: Variant,
    pts: Vec<Point>,
    hover: Option<Point>,
    /// The first press is still down: a release far away places the second point.
    pressed: bool,
    snap: Option<SnapCandidate>,
}

impl DrawTool {
    pub(super) fn new(variant: Variant) -> Self {
        DrawTool { variant, pts: Vec::new(), hover: None, pressed: false, snap: None }
    }

    /// The tool for a draw [`ToolKind`] (`Point` for any other kind, which `Active::new` never passes).
    pub(super) fn for_kind(kind: ToolKind) -> Self {
        DrawTool::new(Variant::of(kind).unwrap_or(Variant::Point))
    }

    fn reset(&mut self) {
        self.pts.clear();
        self.hover = None;
        self.pressed = false;
    }

    fn finish(&mut self, pts: &[Point]) -> ToolStep {
        self.reset();
        match self.variant.build(pts) {
            Some(out) => ToolStep::Commit(out.commit()),
            None => ToolStep::Cancelled,
        }
    }

    /// Adds a point; completes the shape when it was the last one.
    fn place(&mut self, p: Point) -> ToolStep {
        self.pts.push(p);
        if self.pts.len() >= self.variant.needed() {
            let pts = std::mem::take(&mut self.pts);
            return self.finish(&pts);
        }
        ToolStep::PreviewChanged
    }
}

impl Tool for DrawTool {
    fn input(&mut self, input: &CanvasInput, ctx: &ToolContext<'_>) -> ToolStep {
        if is_escape(input) {
            return self.cancel();
        }
        let Some(ev) = tool_pointer(input) else { return ToolStep::Nothing };
        if ev.phase == PointerPhase::Cancel {
            return self.cancel();
        }
        let (world, snap) = ctx.snapped(ev, &[]);
        let snap_changed = snap != self.snap;
        self.snap = snap;
        match ev.phase {
            PointerPhase::Down => {
                if self.pts.last() == Some(&world) {
                    return ToolStep::Nothing;
                }
                self.pressed = self.pts.is_empty();
                self.hover = Some(world);
                self.place(world)
            }
            PointerPhase::Move => {
                self.hover = Some(world);
                if self.pts.is_empty() && snap_changed {
                    ToolStep::PreviewChanged
                } else if self.pts.is_empty() {
                    ToolStep::Nothing
                } else {
                    ToolStep::PreviewChanged
                }
            }
            PointerPhase::Up if self.pressed => {
                self.pressed = false;
                let far = self.pts.first().is_some_and(|p0| p0.distance(world) >= ctx.tolerance_mm(DRAG_THRESHOLD_PT));
                if far && self.variant.drag() { self.place(world) } else { ToolStep::PreviewChanged }
            }
            _ => ToolStep::Nothing,
        }
    }

    fn cancel(&mut self) -> ToolStep {
        self.snap = None;
        let active = !self.pts.is_empty();
        self.reset();
        if active { ToolStep::Cancelled } else { ToolStep::Nothing }
    }

    fn preview(&self) -> Option<ToolPreview> {
        if self.pts.is_empty() {
            return None;
        }
        let mut pts = self.pts.clone();
        pts.extend(self.hover);
        if pts.len() >= self.variant.needed() {
            pts.truncate(self.variant.needed());
            self.variant.build(&pts)?.preview()
        } else {
            self.variant.guide(&pts)?.preview()
        }
    }

    fn snap(&self) -> Option<SnapCandidate> {
        self.snap
    }

    fn is_active(&self) -> bool {
        !self.pts.is_empty()
    }

    fn exact_size(&mut self, width: burnburn_types::Length, height: burnburn_types::Length) -> ToolStep {
        let (Some(&p0), cur) = (self.pts.first(), self.hover) else { return ToolStep::Nothing };
        let cur = cur.unwrap_or(p0);
        match self.variant.typed(p0, cur, width.mm(), height.mm()) {
            Some(pts) => self.finish(&pts),
            None => ToolStep::Nothing,
        }
    }
}

/// Helpers shared by the tests of all draw families.
#[cfg(test)]
pub(super) mod kit {
    use burnburn_geometry::Path;

    use super::super::fixture::*;
    use super::super::{EditCommit, Editor, ToolKind, ToolStep};
    use crate::input::PointerPhase;

    pub fn editor(kind: ToolKind) -> Editor {
        let mut ed = Editor::new();
        ed.select_tool(kind);
        ed
    }

    pub fn empty() -> Fixture {
        Fixture::new(Vec::new())
    }

    /// A press and release in place.
    pub fn click(ed: &mut Editor, f: &Fixture, x: f64, y: f64) -> Vec<ToolStep> {
        vec![
            ed.input(&at(PointerPhase::Down, x, y, none()), &f.ctx()),
            ed.input(&at(PointerPhase::Up, x, y, none()), &f.ctx()),
        ]
    }

    /// Press at `a`, move halfway and to `b`, release at `b`.
    pub fn drag(ed: &mut Editor, f: &Fixture, a: (f64, f64), b: (f64, f64)) -> Vec<ToolStep> {
        vec![
            ed.input(&at(PointerPhase::Down, a.0, a.1, none()), &f.ctx()),
            ed.input(&at(PointerPhase::Move, (a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0, none()), &f.ctx()),
            ed.input(&at(PointerPhase::Move, b.0, b.1, none()), &f.ctx()),
            ed.input(&at(PointerPhase::Up, b.0, b.1, none()), &f.ctx()),
        ]
    }

    /// Clicks every point in turn; returns the editor and every step.
    pub fn clicks(kind: ToolKind, pts: &[(f64, f64)]) -> (Editor, Vec<ToolStep>) {
        let (f, mut ed, mut steps) = (empty(), editor(kind), Vec::new());
        for &(x, y) in pts {
            steps.extend(click(&mut ed, &f, x, y));
        }
        (ed, steps)
    }

    pub fn commits(steps: &[ToolStep]) -> Vec<EditCommit> {
        steps
            .iter()
            .filter_map(|s| match s {
                ToolStep::Commit(c) => Some(c.clone()),
                _ => None,
            })
            .collect()
    }

    /// The single commit of a gesture.
    pub fn only(steps: &[ToolStep]) -> EditCommit {
        let c = commits(steps);
        assert_eq!(c.len(), 1, "exactly one commit: {steps:?}");
        c[0].clone()
    }

    /// The outline a commit creates.
    pub fn outline(c: &EditCommit) -> Path {
        match c {
            EditCommit::CreateShape { shape } | EditCommit::CreateConstruction { shape } => {
                shape.to_path().expect("valid shape")
            }
            EditCommit::CreatePath { path } => path.clone(),
            EditCommit::CreateRectangle { rect } => Path::rectangle(*rect),
            other => panic!("not an outline: {other:?}"),
        }
    }

    pub fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }
}

#[cfg(test)]
mod tests {
    use burnburn_geometry::Point;
    use burnburn_types::Length;

    use super::kit::*;
    use super::*;
    use crate::input::CanvasKey;
    use crate::tool::ToolKind;
    use crate::tool::fixture::*;

    /// One complete click script per tool that is built from points.
    fn scripts() -> Vec<(ToolKind, Vec<(f64, f64)>)> {
        vec![
            (ToolKind::LineMidpoint, vec![(10.0, 10.0), (14.0, 13.0)]),
            (ToolKind::Centerline, vec![(0.0, 0.0), (20.0, 0.0)]),
            (ToolKind::RectangleCenter, vec![(0.0, 0.0), (5.0, 3.0)]),
            (ToolKind::RectangleThreePoint, vec![(0.0, 0.0), (4.0, 0.0), (1.0, 3.0)]),
            (ToolKind::CircleTwoPoint, vec![(0.0, 0.0), (10.0, 0.0)]),
            (ToolKind::CircleThreePoint, vec![(0.0, 0.0), (10.0, 0.0), (0.0, 10.0)]),
            (ToolKind::ArcThreePoint, vec![(10.0, 0.0), (0.0, 10.0), (-10.0, 0.0)]),
            (ToolKind::ArcCenterPoint, vec![(0.0, 0.0), (10.0, 0.0), (0.0, 25.0)]),
            (ToolKind::EllipseAxes, vec![(1.0, 1.0), (5.0, 1.0), (2.0, 3.0)]),
            (ToolKind::SlotCenterToCenter, vec![(0.0, 0.0), (10.0, 0.0), (5.0, 2.0)]),
            (ToolKind::SlotOverall, vec![(0.0, 0.0), (14.0, 0.0), (7.0, 2.0)]),
            (ToolKind::SlotCenterPoint, vec![(0.0, 0.0), (5.0, 0.0), (0.0, 1.0)]),
            (ToolKind::SlotArcThreePoint, vec![(10.0, 0.0), (0.0, 10.0), (-10.0, 0.0), (0.0, 11.0)]),
            (ToolKind::SlotArcCenter, vec![(0.0, 0.0), (10.0, 0.0), (0.0, 10.0), (12.0, 0.0)]),
            (ToolKind::Point, vec![(3.0, 4.0)]),
        ]
    }

    #[test]
    fn every_tool_commits_exactly_once_on_its_last_point_and_returns_to_idle() {
        for (kind, pts) in scripts() {
            let (f, mut ed) = (empty(), editor(kind));
            let mut all = Vec::new();
            for (i, &(x, y)) in pts.iter().enumerate() {
                let steps = click(&mut ed, &f, x, y);
                let n = commits(&steps).len();
                assert_eq!(n, usize::from(i + 1 == pts.len()), "{kind:?}: commit only with the last point (point {i})");
                all.extend(steps);
            }
            assert_eq!(commits(&all).len(), 1, "{kind:?}");
            assert_eq!(ed.preview(), None, "{kind:?}: idle after the commit");
            assert!(!ed.is_gesture_active(), "{kind:?}");
            // A stray release after the commit changes nothing.
            assert_eq!(ed.input(&at(PointerPhase::Up, 1.0, 1.0, none()), &f.ctx()), ToolStep::Nothing, "{kind:?}");
        }
    }

    #[test]
    fn cancelling_mid_gesture_commits_nothing_and_leaves_no_preview() {
        for (kind, pts) in scripts().into_iter().filter(|(_, p)| p.len() >= 2) {
            for how in ["esc", "pointer-cancel", "tool-switch"] {
                let (f, mut ed) = (empty(), editor(kind));
                for &(x, y) in &pts[..pts.len() - 1] {
                    click(&mut ed, &f, x, y);
                }
                let (x, y) = pts[pts.len() - 1];
                ed.input(&at(PointerPhase::Move, x, y, none()), &f.ctx());
                assert!(ed.is_gesture_active(), "{kind:?}/{how}: a gesture is running");
                let step = match how {
                    "esc" => ed.input(&key(CanvasKey::Escape), &f.ctx()),
                    "pointer-cancel" => ed.input(&at(PointerPhase::Cancel, x, y, none()), &f.ctx()),
                    _ => ed.select_tool(ToolKind::Select),
                };
                assert_eq!(step, ToolStep::Cancelled, "{kind:?}/{how}");
                assert_eq!(ed.preview(), None, "{kind:?}/{how}");
                ed.select_tool(kind);
                assert_eq!(
                    ed.input(&at(PointerPhase::Up, x, y, none()), &f.ctx()),
                    ToolStep::Nothing,
                    "{kind:?}/{how}"
                );
            }
        }
    }

    #[test]
    fn a_drag_from_press_to_release_places_two_points_and_a_stray_click_places_none() {
        let f = empty();
        let mut ed = editor(ToolKind::CircleTwoPoint);
        let steps = drag(&mut ed, &f, (0.0, 0.0), (10.0, 0.0));
        assert_eq!(commits(&steps).len(), 1, "{steps:?}");
        // A press that does not move is a click, not a zero-size shape.
        let steps = click(&mut ed, &f, 5.0, 5.0);
        assert!(commits(&steps).is_empty(), "{steps:?}");
        let steps = click(&mut ed, &f, 5.0, 5.0);
        assert!(commits(&steps).is_empty(), "the same point twice is ignored: {steps:?}");
        assert!(ed.is_gesture_active());
    }

    #[test]
    fn first_points_snap_to_object_targets_unless_suppressed() {
        let mut f = Fixture::new(vec![rect(1, 0.0, 0.0, 40.0, 20.0)]);
        f.snap.objects = true;
        let mut ed = editor(ToolKind::LineMidpoint);
        click(&mut ed, &f, 40.6, 20.4);
        assert_eq!(ed.snap().map(|s| s.at), Some(Point::new(40.0, 20.0)));
        let steps = click(&mut ed, &f, 60.0, 30.0);
        let Some(EditCommit::CreateShape { shape: Shape::Line { from, to } }) = commits(&steps).pop() else {
            panic!("{steps:?}")
        };
        assert!(close(from, Point::new(20.0, 10.0)) && close(to, Point::new(60.0, 30.0)), "{from:?} {to:?}");

        let mut ed = editor(ToolKind::LineMidpoint);
        ed.input(&at(PointerPhase::Down, 40.6, 20.4, command()), &f.ctx());
        assert_eq!(ed.snap(), None);
    }

    #[test]
    fn hover_without_a_press_previews_nothing_but_reports_snap_changes() {
        let mut f = Fixture::new(vec![rect(1, 0.0, 0.0, 40.0, 20.0)]);
        f.snap.objects = true;
        let mut ed = editor(ToolKind::CircleThreePoint);
        assert_eq!(ed.input(&hover(39.0, 0.5), &f.ctx()), ToolStep::PreviewChanged);
        assert_eq!(ed.preview(), None);
        assert_eq!(ed.input(&hover(39.1, 0.5), &f.ctx()), ToolStep::Nothing);
    }

    #[test]
    fn the_preview_is_the_shape_that_a_click_at_the_pointer_would_commit() {
        let f = empty();
        let mut ed = editor(ToolKind::CircleThreePoint);
        click(&mut ed, &f, 0.0, 0.0);
        click(&mut ed, &f, 10.0, 0.0);
        ed.input(&at(PointerPhase::Move, 0.0, 10.0, none()), &f.ctx());
        let Some(ToolPreview::Shape { path, .. }) = ed.preview() else { panic!("no preview") };
        let b = path.bounds().unwrap();
        assert!(near(b.min().x, 5.0 - 50.0_f64.sqrt()) && near(b.max().y, 5.0 + 50.0_f64.sqrt()), "{b:?}");
        // With only one point the preview is a guide line to the pointer.
        let mut ed = editor(ToolKind::CircleThreePoint);
        click(&mut ed, &f, 0.0, 0.0);
        ed.input(&at(PointerPhase::Move, 4.0, 3.0, none()), &f.ctx());
        let Some(ToolPreview::Shape { path, .. }) = ed.preview() else { panic!("no guide") };
        assert!(near(path.bounds().unwrap().max().x, 4.0));
    }

    #[test]
    fn a_degenerate_last_point_cancels_instead_of_leaving_a_half_object() {
        // Three collinear points have no circle.
        let (ed, steps) = clicks(ToolKind::CircleThreePoint, &[(0.0, 0.0), (5.0, 5.0), (10.0, 10.0)]);
        assert!(commits(&steps).is_empty(), "{steps:?}");
        assert_eq!(steps.iter().filter(|s| **s == ToolStep::Cancelled).count(), 1, "{steps:?}");
        assert_eq!(ed.preview(), None);
    }

    #[test]
    fn typed_size_before_a_gesture_does_nothing() {
        let mut ed = editor(ToolKind::RectangleCenter);
        let mm = |v| Length::from_mm(v).unwrap();
        assert_eq!(ed.exact_size(mm(10.0), mm(5.0)), ToolStep::Nothing);
    }
}
