//! Spline (fit-point and control-point) and conic tools.

use burnburn_geometry::{Path, Point};

use super::modify::{Action, ModifyParam, path_preview};
use super::{CLICK_TOLERANCE_PT, EditCommit, ToolContext, ToolPreview, ToolStep};
use crate::input::{CanvasKey, PointerEvent};

/// Collects points by clicking; Return, a double click or a click on the first point finishes.
#[derive(Debug, Clone)]
pub(super) struct SplineAction {
    fit: bool,
    points: Vec<Point>,
    hover: Option<Point>,
}

impl SplineAction {
    pub(super) fn new(fit: bool) -> Self {
        SplineAction { fit, points: Vec::new(), hover: None }
    }

    fn min_points(&self, closed: bool) -> usize {
        match (self.fit, closed) {
            (true, false) => 2,
            (true, true) | (false, true) => 3,
            (false, false) => 4,
        }
    }

    fn build(&self, points: &[Point], closed: bool) -> Option<Path> {
        if self.fit { Path::spline_fit(points, closed).ok() } else { Path::spline_control(points, closed).ok() }
    }

    fn finish(&mut self, closed: bool) -> ToolStep {
        let points = std::mem::take(&mut self.points);
        self.hover = None;
        if points.len() < self.min_points(closed) {
            return ToolStep::Cancelled;
        }
        match self.build(&points, closed) {
            Some(path) => ToolStep::Commit(EditCommit::CreatePath { path }),
            None => ToolStep::Cancelled,
        }
    }
}

impl Action for SplineAction {
    fn press(&mut self, ctx: &ToolContext<'_>, at: Point, ev: &PointerEvent) -> ToolStep {
        if ev.click_count >= 2 && !self.points.is_empty() {
            return self.finish(false);
        }
        if self.points.len() >= 3 && self.points[0].distance(at) <= ctx.tolerance_mm(CLICK_TOLERANCE_PT) {
            return self.finish(true);
        }
        if self.points.last() != Some(&at) {
            self.points.push(at);
        }
        self.hover = None;
        ToolStep::PreviewChanged
    }

    fn hover(&mut self, _ctx: &ToolContext<'_>, at: Point, _ev: &PointerEvent) {
        self.hover = Some(at);
    }

    fn key(&mut self, key: CanvasKey, _ctx: &ToolContext<'_>) -> ToolStep {
        match key {
            CanvasKey::Return => self.finish(false),
            CanvasKey::Backspace if !self.points.is_empty() => {
                self.points.pop();
                ToolStep::PreviewChanged
            }
            _ => ToolStep::Nothing,
        }
    }

    fn reset(&mut self) -> bool {
        self.hover = None;
        !std::mem::take(&mut self.points).is_empty()
    }

    fn preview(&self) -> Option<ToolPreview> {
        let mut points = self.points.clone();
        points.extend(self.hover);
        if self.points.is_empty() || points.len() < 2 {
            return None;
        }
        // Until the spline has enough points, show the control polygon.
        let path = self.build(&points, false).or_else(|| Path::polyline(&points, false).ok())?;
        Some(path_preview(path))
    }
}

/// Start, end, apex: three clicks; `rho` shapes the conic.
#[derive(Debug, Clone)]
pub(super) struct ConicAction {
    points: Vec<Point>,
    hover: Option<Point>,
    rho: f64,
}

impl Default for ConicAction {
    fn default() -> Self {
        ConicAction { points: Vec::new(), hover: None, rho: 0.5 }
    }
}

impl Action for ConicAction {
    fn press(&mut self, _ctx: &ToolContext<'_>, at: Point, _ev: &PointerEvent) -> ToolStep {
        if self.points.last() != Some(&at) {
            self.points.push(at);
        }
        self.hover = None;
        if self.points.len() < 3 {
            return ToolStep::PreviewChanged;
        }
        let points = std::mem::take(&mut self.points);
        match Path::conic(points[0], points[1], points[2], self.rho) {
            Ok(path) => ToolStep::Commit(EditCommit::CreatePath { path }),
            Err(_) => ToolStep::Cancelled,
        }
    }

    fn hover(&mut self, _ctx: &ToolContext<'_>, at: Point, _ev: &PointerEvent) {
        self.hover = Some(at);
    }

    fn reset(&mut self) -> bool {
        self.hover = None;
        !std::mem::take(&mut self.points).is_empty()
    }

    fn preview(&self) -> Option<ToolPreview> {
        let (first, rest) = self.points.split_first()?;
        let hover = self.hover?;
        let path = match rest {
            [end] => Path::conic(*first, *end, hover, self.rho).ok()?,
            _ => Path::line(*first, hover).ok()?,
        };
        Some(path_preview(path))
    }

    fn set_param(&mut self, param: ModifyParam) -> bool {
        match param {
            ModifyParam::Rho(rho) if rho.is_finite() && rho > 0.0 && rho < 1.0 => {
                self.rho = rho;
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use burnburn_geometry::{Point, Segment};

    use super::super::fixture::*;
    use super::super::modify::ModifyParam;
    use super::super::modify::kit::*;
    use super::super::{EditCommit, ToolKind, ToolPreview, ToolStep};
    use crate::input::CanvasKey;

    fn path_points(c: &EditCommit) -> Vec<Point> {
        let EditCommit::CreatePath { path } = c else { panic!("expected CreatePath, got {c:?}") };
        let sp = &path.subpaths()[0];
        std::iter::once(sp.start()).chain(sp.segments().iter().map(Segment::end)).collect()
    }

    #[test]
    fn fit_spline_commits_one_path_through_the_clicked_points() {
        let (f, mut ed) = (empty(), editor(ToolKind::SplineFit));
        let mut steps = clicks(&mut ed, &f, &[(0.0, 0.0), (10.0, 8.0), (20.0, 0.0)]);
        assert!(commits(&steps).is_empty(), "clicking only collects points");
        hover_to(&mut ed, &f, 30.0, 5.0);
        assert!(matches!(ed.preview(), Some(ToolPreview::Shape { .. })), "the curve follows the pointer");
        steps.push(press_key(&mut ed, &f, CanvasKey::Return));
        let pts = path_points(&only(&steps));
        assert_eq!(pts, vec![Point::new(0.0, 0.0), Point::new(10.0, 8.0), Point::new(20.0, 0.0)]);
        assert!(ed.preview().is_none(), "no preview after the commit");
    }

    #[test]
    fn double_click_finishes_without_a_duplicate_point() {
        let (f, mut ed) = (empty(), editor(ToolKind::SplineFit));
        let mut steps = clicks(&mut ed, &f, &[(0.0, 0.0), (10.0, 8.0)]);
        steps.push(ed.input(&double_click(10.0, 8.0), &f.ctx()));
        assert_eq!(path_points(&only(&steps)), vec![Point::new(0.0, 0.0), Point::new(10.0, 8.0)]);
    }

    #[test]
    fn clicking_the_first_point_closes_a_fit_spline() {
        let (f, mut ed) = (empty(), editor(ToolKind::SplineFit));
        let steps = clicks(&mut ed, &f, &[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.3, 0.3)]);
        let EditCommit::CreatePath { path } = only(&steps) else { panic!("path") };
        assert!(path.subpaths()[0].is_closed());
        assert_eq!(path.subpaths()[0].segments().len(), 3, "one cubic per fit point");
    }

    #[test]
    fn escape_and_a_single_point_leave_no_half_object() {
        let (f, mut ed) = (empty(), editor(ToolKind::SplineFit));
        clicks(&mut ed, &f, &[(0.0, 0.0), (10.0, 8.0)]);
        assert_eq!(press_key(&mut ed, &f, CanvasKey::Escape), ToolStep::Cancelled);
        assert!(ed.preview().is_none());
        click(&mut ed, &f, 3.0, 3.0);
        assert_eq!(press_key(&mut ed, &f, CanvasKey::Return), ToolStep::Cancelled, "one point is no spline");
    }

    #[test]
    fn backspace_removes_the_last_point() {
        let (f, mut ed) = (empty(), editor(ToolKind::SplineFit));
        clicks(&mut ed, &f, &[(0.0, 0.0), (10.0, 8.0), (20.0, 0.0)]);
        press_key(&mut ed, &f, CanvasKey::Backspace);
        let steps = vec![press_key(&mut ed, &f, CanvasKey::Return)];
        assert_eq!(path_points(&only(&steps)).len(), 2);
    }

    #[test]
    fn control_spline_needs_four_points_and_touches_the_first_and_last() {
        let (f, mut ed) = (empty(), editor(ToolKind::SplineControl));
        clicks(&mut ed, &f, &[(0.0, 0.0), (5.0, 10.0), (15.0, 10.0)]);
        assert_eq!(press_key(&mut ed, &f, CanvasKey::Return), ToolStep::Cancelled, "three control points: too few");
        let mut steps = clicks(&mut ed, &f, &[(0.0, 0.0), (5.0, 10.0), (15.0, 10.0), (20.0, 0.0)]);
        steps.push(press_key(&mut ed, &f, CanvasKey::Return));
        let pts = path_points(&only(&steps));
        assert_eq!(
            (pts.first().copied(), pts.last().copied()),
            (Some(Point::new(0.0, 0.0)), Some(Point::new(20.0, 0.0)))
        );
        assert_eq!(pts.len(), 2, "four control points are one Bézier");
    }

    #[test]
    fn conic_takes_start_end_and_apex_and_rho_moves_the_shoulder() {
        let (f, mut ed) = (empty(), editor(ToolKind::Conic));
        let mut steps = clicks(&mut ed, &f, &[(0.0, 0.0), (20.0, 0.0)]);
        hover_to(&mut ed, &f, 10.0, 20.0);
        assert!(matches!(ed.preview(), Some(ToolPreview::Shape { .. })));
        steps.extend(click(&mut ed, &f, 10.0, 20.0));
        let pts = path_points(&only(&steps));
        assert_eq!((pts[0], *pts.last().unwrap()), (Point::new(0.0, 0.0), Point::new(20.0, 0.0)));
        assert!(near(pts[pts.len() / 2], Point::new(10.0, 10.0)), "parabola: halfway to the apex {pts:?}");

        param(&mut ed, ModifyParam::Rho(0.7));
        let steps = clicks(&mut ed, &f, &[(0.0, 0.0), (20.0, 0.0), (10.0, 20.0)]);
        let pts = path_points(&only(&steps));
        assert!(near(pts[pts.len() / 2], Point::new(10.0, 14.0)), "rho 0.7: 70% of the way to the apex {pts:?}");
        assert!(!ed.set_param_ok(ModifyParam::Rho(1.0)), "rho must lie strictly between 0 and 1");
    }

    #[test]
    fn conic_escape_cancels() {
        let (f, mut ed) = (empty(), editor(ToolKind::Conic));
        clicks(&mut ed, &f, &[(0.0, 0.0), (20.0, 0.0)]);
        assert_eq!(press_key(&mut ed, &f, CanvasKey::Escape), ToolStep::Cancelled);
        assert!(ed.preview().is_none());
    }
}
