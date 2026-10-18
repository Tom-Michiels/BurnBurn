//! Arc variants: three-point, centre-start-end and tangent.

use std::sync::Arc;

use burnburn_document::DocumentSnapshot;
use burnburn_geometry::{
    EllipticalArc, Path, PathBuilder, Point, Vector, arc_center_start_end, arc_tangent, arc_through_three,
};

use super::click::{Out, Variant};
use super::{
    CLICK_TOLERANCE_PT, EditCommit, ShapeKind, Tool, ToolContext, ToolPreview, ToolStep, is_escape, tool_pointer,
};
use crate::input::{CanvasInput, PointerPhase};
use crate::snap::SnapCandidate;

/// An open path holding exactly `arc`.
pub(super) fn arc_path(arc: &EllipticalArc) -> Option<Path> {
    PathBuilder::new().move_to(arc.start_point()).arc(*arc).build().ok()
}

pub(super) fn build(variant: Variant, pts: &[Point]) -> Option<Out> {
    let arc = match variant {
        Variant::ArcThreePoint => arc_through_three(*pts.first()?, *pts.get(1)?, *pts.get(2)?),
        // Counter-clockwise from the start click to the direction of the end click.
        Variant::ArcCenterPoint => arc_center_start_end(*pts.first()?, *pts.get(1)?, *pts.get(2)?, true),
        _ => return None,
    }
    .ok()?;
    arc_path(&arc).map(Out::Path)
}

/// A free end of an open subpath near `p` and the direction a continuation leaves it in.
fn pick_end(doc: &DocumentSnapshot, p: Point, tolerance: f64) -> Option<(Point, Vector)> {
    let mut best: Option<(f64, Point, Vector)> = None;
    let mut consider = |at: Point, dir: Vector| {
        let d = p.distance(at);
        if d <= tolerance && dir.length() > 0.0 && best.is_none_or(|(bd, _, _)| d < bd) {
            best = Some((d, at, dir));
        }
    };
    for (_, object) in doc.objects() {
        for sub in object.path().subpaths().iter().filter(|s| !s.is_closed()) {
            let segs = sub.segments();
            let (Some(first), Some(last)) = (segs.first(), segs.last()) else { continue };
            consider(sub.start(), first.derivative(sub.start(), 0.0) * -1.0);
            let before_last = if segs.len() >= 2 { segs[segs.len() - 2].end() } else { sub.start() };
            consider(last.end(), last.derivative(before_last, 1.0));
        }
    }
    best.map(|(_, at, dir)| (at, dir))
}

/// Arc that continues the free end of a picked line or arc: the first click picks the end (and so
/// the tangent), the second click is where the arc ends.
#[derive(Debug, Clone, Default)]
pub struct TangentArcTool {
    start: Option<(Point, Vector)>,
    hover: Option<Point>,
    /// Length of the tangent guide shown before the pointer moves, in millimetres.
    guide_mm: f64,
    snap: Option<SnapCandidate>,
}

impl TangentArcTool {
    fn reset(&mut self) {
        self.start = None;
        self.hover = None;
    }
}

impl Tool for TangentArcTool {
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
        match (ev.phase, self.start) {
            (PointerPhase::Down, None) => {
                match pick_end(ctx.document, ev.position, ctx.tolerance_mm(CLICK_TOLERANCE_PT)) {
                    Some(found) => {
                        self.start = Some(found);
                        self.guide_mm = ctx.tolerance_mm(40.0);
                        ToolStep::PreviewChanged
                    }
                    None => ToolStep::Nothing,
                }
            }
            (PointerPhase::Down, Some((at, dir))) => {
                self.reset();
                match arc_tangent(at, dir, world).ok().as_ref().and_then(arc_path) {
                    Some(path) => ToolStep::Commit(EditCommit::CreatePath { path }),
                    None => ToolStep::Cancelled,
                }
            }
            (PointerPhase::Move, start) => {
                self.hover = Some(world);
                if start.is_some() || snap_changed { ToolStep::PreviewChanged } else { ToolStep::Nothing }
            }
            _ => ToolStep::Nothing,
        }
    }

    fn cancel(&mut self) -> ToolStep {
        self.snap = None;
        let active = self.start.is_some();
        self.reset();
        if active { ToolStep::Cancelled } else { ToolStep::Nothing }
    }

    fn preview(&self) -> Option<ToolPreview> {
        let (at, dir) = self.start?;
        let arc_to = |to: Point| arc_tangent(at, dir, to).ok().as_ref().and_then(arc_path);
        let path = match self.hover {
            Some(h) if h != at => arc_to(h).or_else(|| Path::line(at, h).ok())?,
            // Before the pointer moves: the tangent the arc leaves in.
            _ => Path::line(at, at + dir * (self.guide_mm / dir.length())).ok()?,
        };
        Some(ToolPreview::Shape { path: Arc::new(path), shape: ShapeKind::Curve })
    }

    fn snap(&self) -> Option<SnapCandidate> {
        self.snap
    }

    fn is_active(&self) -> bool {
        self.start.is_some()
    }
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_2, PI};

    use burnburn_document::GeometryObject;
    use burnburn_geometry::{EllipticalArc, Path, PathBuilder, Point, Segment};

    use super::super::click::kit::*;
    use super::super::fixture::*;
    use super::super::{EditCommit, ToolKind, ToolStep};
    use crate::input::{CanvasKey, PointerPhase};

    fn arc_of(c: &EditCommit) -> EllipticalArc {
        let EditCommit::CreatePath { path } = c else { panic!("not a path: {c:?}") };
        assert_eq!(path.subpaths().len(), 1);
        assert!(!path.subpaths()[0].is_closed(), "an arc is an open curve");
        match path.subpaths()[0].segments() {
            [Segment::Arc(a)] => *a,
            other => panic!("one arc expected: {other:?}"),
        }
    }

    #[test]
    fn three_point_arc_runs_through_the_middle_click() {
        let (_, steps) = clicks(ToolKind::ArcThreePoint, &[(10.0, 0.0), (0.0, 10.0), (-10.0, 0.0)]);
        let a = arc_of(&only(&steps));
        assert!(close(a.start_point(), Point::new(10.0, 0.0)) && close(a.end_point(), Point::new(-10.0, 0.0)));
        assert!(close(a.point_at_fraction(0.5), Point::new(0.0, 10.0)));
        assert!(near(a.sweep_parameter(), PI));
    }

    #[test]
    fn three_point_arc_with_collinear_clicks_cancels() {
        let (_, steps) = clicks(ToolKind::ArcThreePoint, &[(0.0, 0.0), (5.0, 0.0), (10.0, 0.0)]);
        assert!(commits(&steps).is_empty(), "{steps:?}");
    }

    #[test]
    fn centre_point_arc_runs_counter_clockwise_to_the_end_direction() {
        let (_, steps) = clicks(ToolKind::ArcCenterPoint, &[(0.0, 0.0), (10.0, 0.0), (0.0, 25.0)]);
        let a = arc_of(&only(&steps));
        assert!(close(a.center(), Point::new(0.0, 0.0)) && close(a.end_point(), Point::new(0.0, 10.0)));
        assert!(near(a.sweep_parameter(), FRAC_PI_2));
    }

    fn line_obj(raw: u64, a: (f64, f64), b: (f64, f64)) -> GeometryObject {
        path_obj(raw, Path::line(Point::new(a.0, a.1), Point::new(b.0, b.1)).unwrap())
    }

    #[test]
    fn tangent_arc_continues_the_end_of_a_picked_line() {
        let f = Fixture::new(vec![line_obj(1, (0.0, 0.0), (10.0, 0.0))]);
        let mut ed = editor(ToolKind::ArcTangent);
        assert!(commits(&click(&mut ed, &f, 10.2, 0.1)).is_empty());
        ed.input(&at(PointerPhase::Move, 20.0, 10.0, none()), &f.ctx());
        assert!(ed.preview().is_some());
        let a = arc_of(&only(&click(&mut ed, &f, 20.0, 10.0)));
        assert!(close(a.center(), Point::new(10.0, 10.0)), "{:?}", a.center());
        assert!(close(a.start_point(), Point::new(10.0, 0.0)) && close(a.end_point(), Point::new(20.0, 10.0)));
        assert!(near(a.sweep_parameter(), FRAC_PI_2));
    }

    #[test]
    fn tangent_arc_leaves_the_start_of_a_line_outwards() {
        let f = Fixture::new(vec![line_obj(1, (0.0, 0.0), (10.0, 0.0))]);
        let mut ed = editor(ToolKind::ArcTangent);
        click(&mut ed, &f, -0.2, 0.1);
        let a = arc_of(&only(&click(&mut ed, &f, -10.0, -10.0)));
        // Heading -x from the origin, curving to -y: centre (0, -10), counter-clockwise.
        assert!(close(a.center(), Point::new(0.0, -10.0)), "{:?}", a.center());
        assert!(near(a.sweep_parameter(), FRAC_PI_2));
    }

    #[test]
    fn tangent_arc_follows_the_end_tangent_of_an_arc() {
        // Quarter circle (10, 0) -> (0, 10) about the origin; it leaves (0, 10) heading -x.
        let a = EllipticalArc::circle(
            Point::new(0.0, 0.0),
            10.0,
            burnburn_types::Angle::ZERO,
            burnburn_types::Angle::from_radians(FRAC_PI_2).unwrap(),
        )
        .unwrap();
        let path = PathBuilder::new().move_to(a.start_point()).arc(a).build().unwrap();
        let f = Fixture::new(vec![path_obj(1, path)]);
        let mut ed = editor(ToolKind::ArcTangent);
        click(&mut ed, &f, 0.1, 10.1);
        let out = arc_of(&only(&click(&mut ed, &f, -10.0, 20.0)));
        assert!(close(out.start_point(), Point::new(0.0, 10.0)));
        assert!(close(out.center(), Point::new(0.0, 20.0)), "{:?}", out.center());
    }

    #[test]
    fn tangent_arc_ignores_clicks_that_are_not_on_a_free_end() {
        let f = Fixture::new(vec![line_obj(1, (0.0, 0.0), (10.0, 0.0)), rect(2, 50.0, 50.0, 10.0, 10.0)]);
        let mut ed = editor(ToolKind::ArcTangent);
        assert_eq!(click(&mut ed, &f, 5.0, 0.0), vec![ToolStep::Nothing, ToolStep::Nothing], "mid-line");
        assert_eq!(click(&mut ed, &f, 50.0, 50.0), vec![ToolStep::Nothing, ToolStep::Nothing], "closed contour");
        assert!(!ed.is_gesture_active());
    }

    #[test]
    fn tangent_arc_to_a_point_on_the_tangent_line_cancels_and_esc_resets() {
        let f = Fixture::new(vec![line_obj(1, (0.0, 0.0), (10.0, 0.0))]);
        let mut ed = editor(ToolKind::ArcTangent);
        click(&mut ed, &f, 10.0, 0.0);
        assert!(commits(&click(&mut ed, &f, 30.0, 0.0)).is_empty());
        assert!(!ed.is_gesture_active(), "cancelled, not stuck");
        click(&mut ed, &f, 10.0, 0.0);
        assert_eq!(ed.input(&key(CanvasKey::Escape), &f.ctx()), ToolStep::Cancelled);
        assert_eq!(ed.preview(), None);
    }
}
