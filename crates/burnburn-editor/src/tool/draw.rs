//! Drawing tools for ellipse, circle, line, polyline and pen (E01). Each finished gesture is one
//! `EditCommit::CreatePath`; Esc, a pointer cancel or a tool switch leaves no half object.

use std::f64::consts::FRAC_PI_4;
use std::sync::Arc;

use burnburn_geometry::{Bounds, Path, PathBuilder, Point, Shape, Vector};
use burnburn_types::{Angle, Length};

use super::rectangle::RectanglePreview;
use super::{
    CLICK_TOLERANCE_PT, EditCommit, ShapeKind, Tool, ToolContext, ToolPreview, ToolStep, is_escape, key_down,
    tool_pointer,
};
use crate::input::{CanvasInput, CanvasKey, PointerEvent, PointerPhase};
use crate::snap::SnapCandidate;

/// An axis-aligned ellipse (or circle) inscribed in `b`; `None` without area.
fn ellipse_in(b: Bounds, circle: bool) -> Option<Shape> {
    let (rx, ry) = (Length::from_mm(b.width() / 2.0).ok()?, Length::from_mm(b.height() / 2.0).ok()?);
    if !(rx.mm() > 0.0 && ry.mm() > 0.0) {
        return None;
    }
    Some(if circle && rx == ry {
        Shape::Circle { center: b.center(), radius: rx }
    } else {
        Shape::Ellipse { center: b.center(), rx, ry, rotation: Angle::ZERO }
    })
}

fn circle(center: Point, radius: f64) -> Option<Shape> {
    let radius = Length::from_mm(radius).ok().filter(|r| r.mm() > 0.0)?;
    Some(Shape::Circle { center, radius })
}

/// One `CreateShape` commit, or a cancelled gesture when the shape is degenerate (no half object).
fn create(shape: Option<Shape>) -> ToolStep {
    match shape.filter(|s| s.to_path().is_ok()) {
        Some(shape) => ToolStep::Commit(EditCommit::CreateShape { shape }),
        None => ToolStep::Cancelled,
    }
}

fn shape_preview(shape: Option<Shape>, kind: ShapeKind) -> Option<ToolPreview> {
    let path = shape?.to_path().ok()?;
    Some(ToolPreview::Shape { path: Arc::new(path), shape: kind })
}

/// Shared pointer bookkeeping: snapped position and whether the snap target changed.
fn track(snap: &mut Option<SnapCandidate>, ev: &PointerEvent, ctx: &ToolContext<'_>) -> (Point, bool) {
    let (p, c) = ctx.snapped(ev, &[]);
    let changed = c != *snap;
    *snap = c;
    (p, changed)
}

fn idle_step(changed: bool) -> ToolStep {
    if changed { ToolStep::PreviewChanged } else { ToolStep::Nothing }
}

/// Bounding-box ellipse: Shift = circle, Alt/Option = from centre (same rules as the rectangle).
#[derive(Debug, Clone, Default)]
pub struct EllipseTool {
    drag: Option<RectanglePreview>,
    snap: Option<SnapCandidate>,
}

impl EllipseTool {
    fn shape(p: &RectanglePreview) -> Option<Shape> {
        ellipse_in(p.bounds(), p.square)
    }

    fn kind(p: &RectanglePreview) -> ShapeKind {
        if p.square { ShapeKind::Circle } else { ShapeKind::Ellipse }
    }
}

impl Tool for EllipseTool {
    fn input(&mut self, input: &CanvasInput, ctx: &ToolContext<'_>) -> ToolStep {
        if is_escape(input) {
            return self.cancel();
        }
        let Some(ev) = tool_pointer(input) else { return ToolStep::Nothing };
        if ev.phase == PointerPhase::Cancel {
            return self.cancel();
        }
        let (world, changed) = track(&mut self.snap, ev, ctx);
        let (square, from_center) = (ev.modifiers.shift, ev.modifiers.alt);
        match (ev.phase, self.drag) {
            (PointerPhase::Down, _) => {
                self.drag = Some(RectanglePreview { anchor: world, current: world, square, from_center });
                ToolStep::PreviewChanged
            }
            (PointerPhase::Move, Some(p)) => {
                self.drag = Some(RectanglePreview { current: world, square, from_center, ..p });
                ToolStep::PreviewChanged
            }
            (PointerPhase::Up, Some(p)) => {
                self.drag = None;
                create(Self::shape(&RectanglePreview { current: world, square, from_center, ..p }))
            }
            (PointerPhase::Move, None) => idle_step(changed),
            _ => ToolStep::Nothing,
        }
    }

    fn cancel(&mut self) -> ToolStep {
        self.snap = None;
        if self.drag.take().is_some() { ToolStep::Cancelled } else { ToolStep::Nothing }
    }

    fn preview(&self) -> Option<ToolPreview> {
        let p = self.drag?;
        shape_preview(Self::shape(&p), Self::kind(&p))
    }

    fn snap(&self) -> Option<SnapCandidate> {
        self.snap
    }

    fn exact_size(&mut self, width: Length, height: Length) -> ToolStep {
        match self.drag.take() {
            Some(p) => create(Self::shape(&p.with_size(width.mm(), height.mm()))),
            None => ToolStep::Nothing,
        }
    }
}

/// Centre-radius circle: press on the centre, release on the rim; or type the diameter.
#[derive(Debug, Clone, Default)]
pub struct CircleTool {
    drag: Option<(Point, Point)>,
    snap: Option<SnapCandidate>,
}

impl Tool for CircleTool {
    fn input(&mut self, input: &CanvasInput, ctx: &ToolContext<'_>) -> ToolStep {
        if is_escape(input) {
            return self.cancel();
        }
        let Some(ev) = tool_pointer(input) else { return ToolStep::Nothing };
        if ev.phase == PointerPhase::Cancel {
            return self.cancel();
        }
        let (world, changed) = track(&mut self.snap, ev, ctx);
        match (ev.phase, self.drag) {
            (PointerPhase::Down, _) => {
                self.drag = Some((world, world));
                ToolStep::PreviewChanged
            }
            (PointerPhase::Move, Some((c, _))) => {
                self.drag = Some((c, world));
                ToolStep::PreviewChanged
            }
            (PointerPhase::Up, Some((c, _))) => {
                self.drag = None;
                create(circle(c, c.distance(world)))
            }
            (PointerPhase::Move, None) => idle_step(changed),
            _ => ToolStep::Nothing,
        }
    }

    fn cancel(&mut self) -> ToolStep {
        self.snap = None;
        if self.drag.take().is_some() { ToolStep::Cancelled } else { ToolStep::Nothing }
    }

    fn preview(&self) -> Option<ToolPreview> {
        let (c, rim) = self.drag?;
        shape_preview(circle(c, c.distance(rim)), ShapeKind::Circle)
    }

    fn snap(&self) -> Option<SnapCandidate> {
        self.snap
    }

    /// The width is the diameter (Ø); the height is ignored.
    fn exact_size(&mut self, width: Length, _height: Length) -> ToolStep {
        match self.drag.take() {
            Some((c, _)) => create(circle(c, width.mm() / 2.0)),
            None => ToolStep::Nothing,
        }
    }
}

/// Shift: direction constrained to multiples of 45°, length projected onto that direction.
fn constrain_45(anchor: Point, p: Point) -> Point {
    let d = p - anchor;
    if d.length() == 0.0 {
        return p;
    }
    let angle = (d.y.atan2(d.x) / FRAC_PI_4).round() * FRAC_PI_4;
    let dir = Vector::new(angle.cos(), angle.sin());
    let len = d.dot(dir);
    // Snap tiny cosine noise at multiples of 90° to exact axes.
    let clean = |v: f64| if v.abs() < 1e-12 { 0.0 } else { v };
    anchor + Vector::new(clean(dir.x * len), clean(dir.y * len))
}

#[derive(Debug, Clone, Default)]
pub struct LineTool {
    drag: Option<(Point, Point)>,
    snap: Option<SnapCandidate>,
}

impl LineTool {
    fn shape(from: Point, to: Point) -> Option<Shape> {
        (from != to).then_some(Shape::Line { from, to })
    }
}

impl Tool for LineTool {
    fn input(&mut self, input: &CanvasInput, ctx: &ToolContext<'_>) -> ToolStep {
        if is_escape(input) {
            return self.cancel();
        }
        let Some(ev) = tool_pointer(input) else { return ToolStep::Nothing };
        if ev.phase == PointerPhase::Cancel {
            return self.cancel();
        }
        let (world, changed) = track(&mut self.snap, ev, ctx);
        let end = |a: Point| if ev.modifiers.shift { constrain_45(a, world) } else { world };
        match (ev.phase, self.drag) {
            (PointerPhase::Down, _) => {
                self.drag = Some((world, world));
                ToolStep::PreviewChanged
            }
            (PointerPhase::Move, Some((a, _))) => {
                self.drag = Some((a, end(a)));
                ToolStep::PreviewChanged
            }
            (PointerPhase::Up, Some((a, _))) => {
                self.drag = None;
                create(Self::shape(a, end(a)))
            }
            (PointerPhase::Move, None) => idle_step(changed),
            _ => ToolStep::Nothing,
        }
    }

    fn cancel(&mut self) -> ToolStep {
        self.snap = None;
        if self.drag.take().is_some() { ToolStep::Cancelled } else { ToolStep::Nothing }
    }

    fn preview(&self) -> Option<ToolPreview> {
        let (a, b) = self.drag?;
        shape_preview(Self::shape(a, b), ShapeKind::Line)
    }

    fn snap(&self) -> Option<SnapCandidate> {
        self.snap
    }
}

/// Click per vertex; Return or double-click finishes open, a click on the first vertex closes.
#[derive(Debug, Clone, Default)]
pub struct PolylineTool {
    points: Vec<Point>,
    hover: Option<Point>,
    snap: Option<SnapCandidate>,
}

impl PolylineTool {
    fn finish(&mut self, closed: bool) -> ToolStep {
        let points = std::mem::take(&mut self.points);
        self.hover = None;
        if points.len() < 2 {
            return ToolStep::Cancelled;
        }
        create(Some(Shape::Polyline { points, closed }))
    }
}

impl Tool for PolylineTool {
    fn input(&mut self, input: &CanvasInput, ctx: &ToolContext<'_>) -> ToolStep {
        if is_escape(input) {
            return self.cancel();
        }
        match key_down(input) {
            Some((CanvasKey::Return, _)) => return self.finish(false),
            Some((CanvasKey::Backspace, _)) if !self.points.is_empty() => {
                self.points.pop();
                return ToolStep::PreviewChanged;
            }
            _ => {}
        }
        let Some(ev) = tool_pointer(input) else { return ToolStep::Nothing };
        if ev.phase == PointerPhase::Cancel {
            return self.cancel();
        }
        let (world, changed) = track(&mut self.snap, ev, ctx);
        match ev.phase {
            PointerPhase::Down => {
                if ev.click_count >= 2 && !self.points.is_empty() {
                    return self.finish(false);
                }
                if self.points.len() >= 3 && self.points[0].distance(world) <= ctx.tolerance_mm(CLICK_TOLERANCE_PT) {
                    return self.finish(true);
                }
                if self.points.last() != Some(&world) {
                    self.points.push(world);
                }
                self.hover = None;
                ToolStep::PreviewChanged
            }
            PointerPhase::Move => {
                self.hover = Some(world);
                idle_step(changed || !self.points.is_empty())
            }
            _ => ToolStep::Nothing,
        }
    }

    fn cancel(&mut self) -> ToolStep {
        self.snap = None;
        self.hover = None;
        if std::mem::take(&mut self.points).is_empty() { ToolStep::Nothing } else { ToolStep::Cancelled }
    }

    fn preview(&self) -> Option<ToolPreview> {
        if self.points.is_empty() {
            return None;
        }
        let mut points = self.points.clone();
        points.extend(self.hover);
        if points.len() < 2 {
            return None;
        }
        shape_preview(Some(Shape::Polyline { points, closed: false }), ShapeKind::Polyline)
    }

    fn snap(&self) -> Option<SnapCandidate> {
        self.snap
    }
}

/// One pen anchor with its handles; a corner has both handles on the anchor.
#[derive(Debug, Clone, Copy, PartialEq)]
struct PenNode {
    at: Point,
    handle_in: Point,
    handle_out: Point,
}

/// Bézier pen: click = corner, press-drag = smooth node with symmetric handles; Return or
/// double-click finishes open, a press on the first anchor closes.
#[derive(Debug, Clone, Default)]
pub struct PenTool {
    nodes: Vec<PenNode>,
    dragging: bool,
    hover: Option<Point>,
    snap: Option<SnapCandidate>,
}

impl PenTool {
    fn path(nodes: &[PenNode], closed: bool) -> Option<Path> {
        let (first, rest) = nodes.split_first()?;
        let span = |b: PathBuilder, a: &PenNode, n: &PenNode| {
            if a.handle_out == a.at && n.handle_in == n.at {
                b.line_to(n.at)
            } else {
                b.cubic_to(a.handle_out, n.handle_in, n.at)
            }
        };
        let mut b = PathBuilder::new().move_to(first.at);
        let mut prev = first;
        for n in rest {
            b = span(b, prev, n);
            prev = n;
        }
        if closed {
            // A curved closing span is explicit; a straight one is the implicit close.
            if !(prev.handle_out == prev.at && first.handle_in == first.at) {
                b = b.cubic_to(prev.handle_out, first.handle_in, first.at);
            }
            b = b.close();
        }
        b.build().ok()
    }

    fn finish(&mut self, closed: bool) -> ToolStep {
        let nodes = std::mem::take(&mut self.nodes);
        self.dragging = false;
        self.hover = None;
        if nodes.len() < 2 {
            return ToolStep::Cancelled;
        }
        match Self::path(&nodes, closed) {
            Some(path) => ToolStep::Commit(EditCommit::CreatePath { path }),
            None => ToolStep::Cancelled,
        }
    }
}

impl Tool for PenTool {
    fn input(&mut self, input: &CanvasInput, ctx: &ToolContext<'_>) -> ToolStep {
        if is_escape(input) {
            return self.cancel();
        }
        match key_down(input) {
            Some((CanvasKey::Return, _)) => return self.finish(false),
            Some((CanvasKey::Backspace, _)) if !self.nodes.is_empty() => {
                self.nodes.pop();
                self.dragging = false;
                return ToolStep::PreviewChanged;
            }
            _ => {}
        }
        let Some(ev) = tool_pointer(input) else { return ToolStep::Nothing };
        if ev.phase == PointerPhase::Cancel {
            return self.cancel();
        }
        let (world, changed) = track(&mut self.snap, ev, ctx);
        match ev.phase {
            PointerPhase::Down => {
                if ev.click_count >= 2 && !self.nodes.is_empty() {
                    return self.finish(false);
                }
                if self.nodes.len() >= 2 && self.nodes[0].at.distance(world) <= ctx.tolerance_mm(CLICK_TOLERANCE_PT) {
                    return self.finish(true);
                }
                self.nodes.push(PenNode { at: world, handle_in: world, handle_out: world });
                self.dragging = true;
                self.hover = None;
                ToolStep::PreviewChanged
            }
            PointerPhase::Move if self.dragging => {
                if let Some(n) = self.nodes.last_mut() {
                    // Handles follow the raw pointer: snapping a handle would bend the curve.
                    let out = ev.position;
                    n.handle_out = out;
                    n.handle_in = n.at + (n.at - out);
                }
                ToolStep::PreviewChanged
            }
            PointerPhase::Move => {
                self.hover = Some(world);
                idle_step(changed || !self.nodes.is_empty())
            }
            PointerPhase::Up => {
                self.dragging = false;
                ToolStep::Nothing
            }
            PointerPhase::Cancel => ToolStep::Nothing,
        }
    }

    fn cancel(&mut self) -> ToolStep {
        self.snap = None;
        self.hover = None;
        self.dragging = false;
        if std::mem::take(&mut self.nodes).is_empty() { ToolStep::Nothing } else { ToolStep::Cancelled }
    }

    fn preview(&self) -> Option<ToolPreview> {
        if self.nodes.is_empty() {
            return None;
        }
        let mut nodes = self.nodes.clone();
        nodes.extend(self.hover.map(|h| PenNode { at: h, handle_in: h, handle_out: h }));
        Some(ToolPreview::Shape { path: Arc::new(Self::path(&nodes, false)?), shape: ShapeKind::Curve })
    }

    fn snap(&self) -> Option<SnapCandidate> {
        self.snap
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{CanvasKey, PointerPhase};
    use crate::snap::SnapKind;
    use crate::tool::fixture::*;
    use crate::tool::{EditCommit, ShapeKind};
    use burnburn_document::GeometryObject;
    use burnburn_geometry::Segment;

    fn empty() -> Fixture {
        Fixture::new(Vec::<GeometryObject>::new())
    }

    fn created(step: &ToolStep) -> Option<(Path, ShapeKind)> {
        match step {
            ToolStep::Commit(EditCommit::CreateShape { shape }) => {
                let kind = match shape {
                    Shape::Ellipse { .. } => ShapeKind::Ellipse,
                    Shape::Circle { .. } => ShapeKind::Circle,
                    Shape::Line { .. } => ShapeKind::Line,
                    Shape::Polyline { .. } => ShapeKind::Polyline,
                    other => panic!("unexpected {other:?}"),
                };
                Some((shape.to_path().unwrap(), kind))
            }
            ToolStep::Commit(EditCommit::CreatePath { path }) => Some((path.clone(), ShapeKind::Curve)),
            _ => None,
        }
    }

    fn bounds(p: &Path) -> (f64, f64, f64, f64) {
        let b = p.bounds().unwrap();
        (b.min().x, b.min().y, b.max().x, b.max().y)
    }

    fn approx4(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9 && (a.2 - b.2).abs() < 1e-9 && (a.3 - b.3).abs() < 1e-9
    }

    fn drag(
        tool: &mut dyn Tool,
        f: &Fixture,
        from: (f64, f64),
        to: (f64, f64),
        mods: crate::input::Modifiers,
    ) -> ToolStep {
        tool.input(&at(PointerPhase::Down, from.0, from.1, mods), &f.ctx());
        tool.input(&at(PointerPhase::Move, to.0, to.1, mods), &f.ctx());
        tool.input(&at(PointerPhase::Up, to.0, to.1, mods), &f.ctx())
    }

    fn click(tool: &mut dyn Tool, f: &Fixture, x: f64, y: f64) -> ToolStep {
        tool.input(&at(PointerPhase::Down, x, y, none()), &f.ctx());
        tool.input(&at(PointerPhase::Up, x, y, none()), &f.ctx())
    }

    #[test]
    fn ellipse_drag_commits_closed_ellipse_inscribed_in_the_drag_box() {
        let f = empty();
        let mut tool = EllipseTool::default();
        tool.input(&at(PointerPhase::Down, 0.0, 0.0, none()), &f.ctx());
        tool.input(&at(PointerPhase::Move, 20.0, 10.0, none()), &f.ctx());
        assert!(matches!(tool.preview(), Some(ToolPreview::Shape { shape: ShapeKind::Ellipse, .. })));
        let (path, shape) = created(&tool.input(&at(PointerPhase::Up, 40.0, 20.0, none()), &f.ctx())).unwrap();
        assert_eq!(shape, ShapeKind::Ellipse);
        assert!(approx4(bounds(&path), (0.0, 0.0, 40.0, 20.0)), "{:?}", bounds(&path));
        let sp = &path.subpaths()[0];
        assert!(sp.is_closed());
        let Segment::Arc(arc) = sp.segments()[0] else { panic!("an exact arc, not a polygon") };
        assert!(close(arc.center(), Point::new(20.0, 10.0)));
        assert_eq!(tool.preview(), None);
    }

    #[test]
    fn ellipse_shift_makes_a_circle_and_alt_draws_from_centre() {
        let f = empty();
        let mut tool = EllipseTool::default();
        let (path, shape) = created(&drag(&mut tool, &f, (0.0, 0.0), (30.0, 10.0), shift())).unwrap();
        assert_eq!(shape, ShapeKind::Circle);
        assert!(approx4(bounds(&path), (0.0, 0.0, 30.0, 30.0)));
        let (path, _) = created(&drag(&mut tool, &f, (10.0, 10.0), (15.0, 13.0), alt())).unwrap();
        assert!(approx4(bounds(&path), (5.0, 7.0, 15.0, 13.0)));
    }

    #[test]
    fn ellipse_without_area_or_cancelled_creates_nothing() {
        let f = empty();
        let mut tool = EllipseTool::default();
        assert_eq!(drag(&mut tool, &f, (0.0, 0.0), (30.0, 0.0), none()), ToolStep::Cancelled);
        tool.input(&at(PointerPhase::Down, 0.0, 0.0, none()), &f.ctx());
        tool.input(&at(PointerPhase::Move, 9.0, 9.0, none()), &f.ctx());
        assert_eq!(tool.input(&key(CanvasKey::Escape), &f.ctx()), ToolStep::Cancelled);
        assert_eq!(tool.input(&at(PointerPhase::Up, 9.0, 9.0, none()), &f.ctx()), ToolStep::Nothing);
    }

    #[test]
    fn circle_tool_presses_centre_and_releases_on_radius() {
        let f = empty();
        let mut tool = CircleTool::default();
        let (path, shape) = created(&drag(&mut tool, &f, (10.0, 10.0), (13.0, 14.0), none())).unwrap();
        assert_eq!(shape, ShapeKind::Circle);
        assert!(approx4(bounds(&path), (5.0, 5.0, 15.0, 15.0)), "{:?}", bounds(&path));
        assert_eq!(drag(&mut tool, &f, (10.0, 10.0), (10.0, 10.0), none()), ToolStep::Cancelled, "zero radius");
    }

    /// B1 step 3: click the centre on a snapped corner, type Ø 4.
    #[test]
    fn circle_centre_snaps_and_typed_diameter_is_exact() {
        let mut f = Fixture::new(vec![rect(1, 0.0, 0.0, 40.0, 20.0)]);
        f.snap.objects = true;
        let mut tool = CircleTool::default();
        tool.input(&at(PointerPhase::Down, 39.4, 19.5, none()), &f.ctx());
        assert_eq!(tool.snap().map(|s| s.kind), Some(SnapKind::Endpoint));
        let four = Length::from_mm(4.0).unwrap();
        let (path, shape) = created(&tool.exact_size(four, four)).unwrap();
        assert_eq!(shape, ShapeKind::Circle);
        assert!(approx4(bounds(&path), (38.0, 18.0, 42.0, 22.0)), "{:?}", bounds(&path));
    }

    #[test]
    fn line_drag_commits_open_line_and_shift_constrains_to_45_degrees() {
        let f = empty();
        let mut tool = LineTool::default();
        let (path, shape) = created(&drag(&mut tool, &f, (0.0, 0.0), (30.0, 40.0), none())).unwrap();
        assert_eq!(shape, ShapeKind::Line);
        let sp = &path.subpaths()[0];
        assert!(!sp.is_closed());
        assert_eq!(
            (sp.start(), sp.segments()),
            (Point::new(0.0, 0.0), &[Segment::Line { to: Point::new(30.0, 40.0) }][..])
        );
        // 10° off horizontal with Shift: horizontal, length kept along the constrained direction.
        let (path, _) = created(&drag(&mut tool, &f, (0.0, 0.0), (50.0, 8.8), shift())).unwrap();
        let end = path.subpaths()[0].end();
        assert!((end.y).abs() < 1e-9 && (end.x - 50.0).abs() < 1e-9, "{end:?}");
        assert_eq!(drag(&mut tool, &f, (5.0, 5.0), (5.0, 5.0), none()), ToolStep::Cancelled, "zero length");
    }

    #[test]
    fn polyline_clicks_then_return_commit_one_open_polyline() {
        let f = empty();
        let mut tool = PolylineTool::default();
        click(&mut tool, &f, 0.0, 0.0);
        click(&mut tool, &f, 10.0, 0.0);
        click(&mut tool, &f, 10.0, 10.0);
        tool.input(&hover(0.0, 20.0), &f.ctx());
        let Some(ToolPreview::Shape { path, .. }) = tool.preview() else { panic!("preview") };
        assert_eq!(path.subpaths()[0].segments().len(), 3, "two placed edges plus the rubber edge");
        let (path, shape) = created(&tool.input(&key(CanvasKey::Return), &f.ctx())).unwrap();
        assert_eq!(shape, ShapeKind::Polyline);
        let sp = &path.subpaths()[0];
        assert!(!sp.is_closed());
        assert_eq!(
            sp.segments(),
            &[Segment::Line { to: Point::new(10.0, 0.0) }, Segment::Line { to: Point::new(10.0, 10.0) }][..]
        );
        assert_eq!(tool.preview(), None);
    }

    #[test]
    fn polyline_click_on_first_point_closes_and_double_click_finishes() {
        let f = empty();
        let mut tool = PolylineTool::default();
        click(&mut tool, &f, 0.0, 0.0);
        click(&mut tool, &f, 10.0, 0.0);
        click(&mut tool, &f, 10.0, 10.0);
        let step = tool.input(&at(PointerPhase::Down, 0.5, 0.5, none()), &f.ctx());
        let (path, _) = created(&step).unwrap();
        let sp = &path.subpaths()[0];
        assert!(sp.is_closed());
        assert_eq!(sp.segments().len(), 2, "closing edge is the implicit close, not a duplicate point");

        click(&mut tool, &f, 0.0, 0.0);
        click(&mut tool, &f, 10.0, 0.0);
        let step = tool.input(&double_click(10.0, 0.0), &f.ctx());
        let (path, _) = created(&step).unwrap();
        assert_eq!(
            path.subpaths()[0].segments(),
            &[Segment::Line { to: Point::new(10.0, 0.0) }][..],
            "no duplicate vertex"
        );
    }

    #[test]
    fn polyline_escape_or_single_point_leaves_no_half_object() {
        let f = empty();
        let mut tool = PolylineTool::default();
        click(&mut tool, &f, 0.0, 0.0);
        click(&mut tool, &f, 10.0, 0.0);
        assert_eq!(tool.input(&key(CanvasKey::Escape), &f.ctx()), ToolStep::Cancelled);
        assert_eq!(tool.preview(), None);
        click(&mut tool, &f, 0.0, 0.0);
        assert_eq!(tool.input(&key(CanvasKey::Return), &f.ctx()), ToolStep::Cancelled);
    }

    #[test]
    fn polyline_backspace_removes_the_last_point() {
        let f = empty();
        let mut tool = PolylineTool::default();
        click(&mut tool, &f, 0.0, 0.0);
        click(&mut tool, &f, 10.0, 0.0);
        click(&mut tool, &f, 10.0, 10.0);
        assert_eq!(tool.input(&key(CanvasKey::Backspace), &f.ctx()), ToolStep::PreviewChanged);
        let (path, _) = created(&tool.input(&key(CanvasKey::Return), &f.ctx())).unwrap();
        assert_eq!(path.subpaths()[0].segments().len(), 1);
    }

    #[test]
    fn pen_drag_makes_a_smooth_node_with_symmetric_handles() {
        let f = empty();
        let mut tool = PenTool::default();
        click(&mut tool, &f, 0.0, 0.0);
        tool.input(&at(PointerPhase::Down, 10.0, 0.0, none()), &f.ctx());
        tool.input(&at(PointerPhase::Move, 15.0, 5.0, none()), &f.ctx());
        tool.input(&at(PointerPhase::Up, 15.0, 5.0, none()), &f.ctx());
        click(&mut tool, &f, 20.0, 0.0);
        let (path, shape) = created(&tool.input(&key(CanvasKey::Return), &f.ctx())).unwrap();
        assert_eq!(shape, ShapeKind::Curve);
        assert_eq!(
            path.subpaths()[0].segments(),
            &[
                Segment::Cubic { c1: Point::new(0.0, 0.0), c2: Point::new(5.0, -5.0), to: Point::new(10.0, 0.0) },
                Segment::Cubic { c1: Point::new(15.0, 5.0), c2: Point::new(20.0, 0.0), to: Point::new(20.0, 0.0) },
            ][..]
        );
    }

    #[test]
    fn pen_corner_clicks_make_lines_and_first_anchor_closes() {
        let f = empty();
        let mut tool = PenTool::default();
        click(&mut tool, &f, 0.0, 0.0);
        click(&mut tool, &f, 10.0, 0.0);
        click(&mut tool, &f, 10.0, 10.0);
        let (path, _) = created(&tool.input(&at(PointerPhase::Down, 0.3, 0.2, none()), &f.ctx())).unwrap();
        let sp = &path.subpaths()[0];
        assert!(sp.is_closed());
        assert_eq!(
            sp.segments(),
            &[Segment::Line { to: Point::new(10.0, 0.0) }, Segment::Line { to: Point::new(10.0, 10.0) }][..]
        );
        click(&mut tool, &f, 0.0, 0.0);
        assert_eq!(tool.input(&key(CanvasKey::Escape), &f.ctx()), ToolStep::Cancelled);
    }
}
