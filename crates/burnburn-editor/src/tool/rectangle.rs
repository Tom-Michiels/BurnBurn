use burnburn_geometry::{Bounds, Point, Rect};
use burnburn_types::Length;

use super::{EditCommit, Tool, ToolContext, ToolPreview, ToolStep, is_escape, tool_pointer};
use crate::input::{CanvasInput, PointerPhase};
use crate::snap::SnapCandidate;

/// Rubber-band rectangle in world millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectanglePreview {
    pub anchor: Point,
    pub current: Point,
    pub square: bool,
    pub from_center: bool,
}

impl RectanglePreview {
    /// Shift constrains to a square; Alt/Option draws from the anchor as centre.
    pub fn bounds(&self) -> Bounds {
        let mut dx = self.current.x - self.anchor.x;
        let mut dy = self.current.y - self.anchor.y;
        if self.square {
            let side = dx.abs().max(dy.abs());
            dx = side.copysign(dx);
            dy = side.copysign(dy);
        }
        let far = Point::new(self.anchor.x + dx, self.anchor.y + dy);
        if self.from_center {
            Bounds::from_corners(Point::new(self.anchor.x - dx, self.anchor.y - dy), far)
        } else {
            Bounds::from_corners(self.anchor, far)
        }
    }

    /// The same gesture with an exact width and height (typed while dragging, E01). The drag
    /// direction is kept; a drag that has not moved yet grows towards +x/+y.
    pub fn with_size(&self, width: f64, height: f64) -> RectanglePreview {
        let sx = if self.current.x < self.anchor.x { -1.0 } else { 1.0 };
        let sy = if self.current.y < self.anchor.y { -1.0 } else { 1.0 };
        let (dx, dy) = if self.from_center { (width / 2.0, height / 2.0) } else { (width, height) };
        RectanglePreview {
            current: Point::new(self.anchor.x + sx * dx, self.anchor.y + sy * dy),
            square: false,
            ..*self
        }
    }
}

/// Bounds → exact rectangle; `None` for a gesture without area (no half object, E01).
pub(crate) fn rect_from_bounds(b: Bounds) -> Option<Rect> {
    if !(b.width() > 0.0 && b.height() > 0.0) {
        return None;
    }
    let (w, h) = (Length::from_mm(b.width()).ok()?, Length::from_mm(b.height()).ok()?);
    Rect::new(b.min(), w, h).ok()
}

#[derive(Debug, Clone, Default)]
pub struct RectangleTool {
    preview: Option<RectanglePreview>,
    snap: Option<SnapCandidate>,
}

impl RectangleTool {
    fn commit(&mut self, preview: RectanglePreview) -> ToolStep {
        self.preview = None;
        match rect_from_bounds(preview.bounds()) {
            Some(rect) => ToolStep::Commit(EditCommit::CreateRectangle { rect }),
            None => ToolStep::Cancelled,
        }
    }
}

impl Tool for RectangleTool {
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
        let (square, from_center) = (ev.modifiers.shift, ev.modifiers.alt);
        match (ev.phase, self.preview) {
            (PointerPhase::Down, _) => {
                self.preview = Some(RectanglePreview { anchor: world, current: world, square, from_center });
                ToolStep::PreviewChanged
            }
            (PointerPhase::Move, Some(p)) => {
                self.preview = Some(RectanglePreview { current: world, square, from_center, ..p });
                ToolStep::PreviewChanged
            }
            (PointerPhase::Up, Some(p)) => self.commit(RectanglePreview { current: world, square, from_center, ..p }),
            (PointerPhase::Move, None) if snap_changed => ToolStep::PreviewChanged,
            _ => ToolStep::Nothing,
        }
    }

    fn cancel(&mut self) -> ToolStep {
        self.snap = None;
        match self.preview.take() {
            Some(_) => ToolStep::Cancelled,
            None => ToolStep::Nothing,
        }
    }

    fn preview(&self) -> Option<ToolPreview> {
        self.preview.map(ToolPreview::Rectangle)
    }

    fn snap(&self) -> Option<SnapCandidate> {
        self.snap
    }

    fn exact_size(&mut self, width: Length, height: Length) -> ToolStep {
        match self.preview {
            Some(p) => self.commit(p.with_size(width.mm(), height.mm())),
            None => ToolStep::Nothing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::CanvasKey;
    use crate::snap::SnapKind;
    use crate::tool::fixture::*;
    use burnburn_document::GeometryObject;

    fn created(step: &ToolStep) -> Option<(f64, f64, f64, f64)> {
        match step {
            ToolStep::Commit(EditCommit::CreateRectangle { rect }) => {
                Some((rect.origin().x, rect.origin().y, rect.width().mm(), rect.height().mm()))
            }
            _ => None,
        }
    }

    fn approx(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9 && (a.2 - b.2).abs() < 1e-9 && (a.3 - b.3).abs() < 1e-9
    }

    fn empty() -> Fixture {
        Fixture::new(Vec::<GeometryObject>::new())
    }

    #[test]
    fn rectangle_drag_commits_one_create_with_normalised_bounds() {
        let f = empty();
        let mut tool = RectangleTool::default();
        assert_eq!(tool.input(&at(PointerPhase::Down, 120.0, 80.0, none()), &f.ctx()), ToolStep::PreviewChanged);
        assert_eq!(tool.input(&at(PointerPhase::Move, 60.0, 40.0, none()), &f.ctx()), ToolStep::PreviewChanged);
        assert!(tool.preview().is_some());
        let step = tool.input(&at(PointerPhase::Up, 0.0, 0.0, none()), &f.ctx());
        assert!(approx(created(&step).unwrap(), (0.0, 0.0, 120.0, 80.0)), "{step:?}");
        assert_eq!(tool.preview(), None, "back to idle after the commit");
    }

    #[test]
    fn rectangle_cancel_mid_drag_returns_without_commit() {
        for how in ["esc", "pointer-cancel", "tool-switch"] {
            let f = empty();
            let mut ed = crate::tool::Editor::new();
            ed.select_tool(crate::tool::ToolKind::Rectangle);
            ed.input(&at(PointerPhase::Down, 0.0, 0.0, none()), &f.ctx());
            ed.input(&at(PointerPhase::Move, 50.0, 50.0, none()), &f.ctx());
            let step = match how {
                "esc" => ed.input(&key(CanvasKey::Escape), &f.ctx()),
                "pointer-cancel" => ed.input(&at(PointerPhase::Cancel, 50.0, 50.0, none()), &f.ctx()),
                _ => ed.select_tool(crate::tool::ToolKind::Select),
            };
            assert_eq!(step, ToolStep::Cancelled, "{how}");
            assert_eq!(ed.preview(), None, "{how}");
            ed.select_tool(crate::tool::ToolKind::Rectangle);
            // A release after cancelling must not resurrect the gesture.
            assert_eq!(ed.input(&at(PointerPhase::Up, 50.0, 50.0, none()), &f.ctx()), ToolStep::Nothing, "{how}");
        }
    }

    #[test]
    fn rectangle_release_without_area_creates_nothing() {
        for (x, y) in [(0.0, 0.0), (30.0, 0.0), (0.0, 30.0)] {
            let f = empty();
            let mut tool = RectangleTool::default();
            tool.input(&at(PointerPhase::Down, 0.0, 0.0, none()), &f.ctx());
            assert_eq!(tool.input(&at(PointerPhase::Up, x, y, none()), &f.ctx()), ToolStep::Cancelled, "({x},{y})");
        }
    }

    #[test]
    fn shift_constrains_to_square_and_alt_draws_from_centre() {
        let f = empty();
        let mut tool = RectangleTool::default();
        tool.input(&at(PointerPhase::Down, 0.0, 0.0, shift()), &f.ctx());
        let sq = created(&tool.input(&at(PointerPhase::Up, 30.0, 10.0, shift()), &f.ctx())).unwrap();
        assert!(approx(sq, (0.0, 0.0, 30.0, 30.0)), "{sq:?}");

        let mut tool = RectangleTool::default();
        tool.input(&at(PointerPhase::Down, 10.0, 10.0, alt()), &f.ctx());
        let c = created(&tool.input(&at(PointerPhase::Up, 15.0, 13.0, alt()), &f.ctx())).unwrap();
        assert!(approx(c, (5.0, 7.0, 10.0, 6.0)), "{c:?}");
    }

    /// E04: corners land exactly on a named target; Cmd/Ctrl held suppresses the snap.
    #[test]
    fn rectangle_corners_snap_to_endpoints_unless_suppressed() {
        let mut f = Fixture::new(vec![rect(1, 0.0, 0.0, 40.0, 20.0)]);
        f.snap.objects = true;
        let mut tool = RectangleTool::default();
        tool.input(&at(PointerPhase::Down, 40.6, 20.4, none()), &f.ctx());
        assert_eq!(tool.snap().map(|s| s.kind), Some(SnapKind::Endpoint));
        let r = created(&tool.input(&at(PointerPhase::Up, 80.0, 50.0, none()), &f.ctx())).unwrap();
        assert!(approx(r, (40.0, 20.0, 40.0, 30.0)), "{r:?}");

        let mut tool = RectangleTool::default();
        tool.input(&at(PointerPhase::Down, 40.6, 20.4, command()), &f.ctx());
        assert_eq!(tool.snap(), None);
        let r = created(&tool.input(&at(PointerPhase::Up, 80.0, 50.0, command()), &f.ctx())).unwrap();
        assert!(approx(r, (40.6, 20.4, 39.4, 29.6)), "{r:?}");
    }

    /// Hovering near a target shows it before any press (cursor label), without a preview.
    #[test]
    fn hover_reports_snap_candidate_without_preview() {
        let mut f = Fixture::new(vec![rect(1, 0.0, 0.0, 40.0, 20.0)]);
        f.snap.objects = true;
        let mut tool = RectangleTool::default();
        assert_eq!(tool.input(&hover(39.0, 0.5), &f.ctx()), ToolStep::PreviewChanged);
        assert_eq!(tool.snap().map(|s| (s.at, s.kind)), Some((Point::new(40.0, 0.0), SnapKind::Endpoint)));
        assert_eq!(tool.preview(), None);
        assert_eq!(tool.input(&hover(39.1, 0.5), &f.ctx()), ToolStep::Nothing, "same target: nothing to redraw");
    }

    /// E01: typing 120 × 80 while dragging commits exactly that size from the anchor.
    #[test]
    fn typed_size_while_dragging_commits_exact_rectangle() {
        let f = empty();
        let mut tool = RectangleTool::default();
        assert_eq!(
            tool.exact_size(Length::from_mm(1.0).unwrap(), Length::from_mm(1.0).unwrap()),
            ToolStep::Nothing,
            "idle"
        );
        tool.input(&at(PointerPhase::Down, 10.0, 10.0, none()), &f.ctx());
        tool.input(&at(PointerPhase::Move, 3.0, 50.0, none()), &f.ctx());
        let step = tool.exact_size(Length::from_mm(120.0).unwrap(), Length::from_mm(80.0).unwrap());
        assert!(approx(created(&step).unwrap(), (-110.0, 10.0, 120.0, 80.0)), "drag went left/up: {step:?}");
        assert_eq!(tool.preview(), None);
    }
}
