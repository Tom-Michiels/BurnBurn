//! Round Corners tool (W1-L2a): click a corner of the selected path to replace it by an arc of
//! the editor's corner radius. The radius is limited by the adjacent straight segments.

use burnburn_document::OpsCommand;
use burnburn_geometry::PathOp;
use burnburn_types::CancellationToken;

use super::pick::{anchor_under, select_under};
use super::{EditCommit, Tool, ToolContext, ToolPreview, ToolStep, tool_pointer};
use crate::input::{CanvasInput, PointerPhase};
use crate::snap::SnapCandidate;

#[derive(Debug, Clone)]
pub struct RoundCornerTool {
    radius_mm: f64,
}

impl RoundCornerTool {
    pub fn new(radius_mm: f64) -> Self {
        RoundCornerTool { radius_mm }
    }

    pub fn radius_mm(&self) -> f64 {
        self.radius_mm
    }

    pub fn set_radius_mm(&mut self, radius_mm: f64) {
        self.radius_mm = radius_mm;
    }
}

impl Tool for RoundCornerTool {
    fn input(&mut self, input: &CanvasInput, ctx: &ToolContext<'_>) -> ToolStep {
        let Some(ev) = tool_pointer(input) else { return ToolStep::Nothing };
        if ev.phase != PointerPhase::Down {
            return ToolStep::Nothing;
        }
        let Some((target, node)) = anchor_under(ctx, ev) else { return select_under(ctx, ev) };
        let Some(obj) = ctx.document.object(target) else { return ToolStep::Nothing };
        let op = PathOp::RoundCorners { radius: self.radius_mm, nodes: Some(vec![node]) };
        match obj.path().apply_op(&op, &CancellationToken::never()) {
            Ok(_) => ToolStep::Commit(EditCommit::Ops(OpsCommand::PathOp { targets: vec![target], op })),
            Err(_) => ToolStep::Nothing,
        }
    }

    fn cancel(&mut self) -> ToolStep {
        ToolStep::Nothing
    }

    fn preview(&self) -> Option<ToolPreview> {
        None
    }

    fn snap(&self) -> Option<SnapCandidate> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selection::SelectionChange;
    use crate::tool::fixture::*;
    use burnburn_geometry::{Path, PathBuilder, Point, Segment};
    use burnburn_types::{GeometryId, StableId};

    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    fn fixture() -> Fixture {
        let square = Path::polyline(&[p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)], true).unwrap();
        let curved = PathBuilder::new()
            .move_to(p(100.0, 0.0))
            .cubic_to(p(103.0, 5.0), p(107.0, 5.0), p(110.0, 0.0))
            .line_to(p(110.0, -10.0))
            .build()
            .unwrap();
        let mut f = Fixture::new(vec![path_obj(1, square), path_obj(2, curved)]);
        f.select(&[1]);
        f
    }

    fn arcs(path: &Path) -> Vec<(Point, f64)> {
        path.subpaths()
            .iter()
            .flat_map(|s| s.segments())
            .filter_map(|s| match s {
                Segment::Arc(a) => Some((a.center(), a.semi_diameters().0.length())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn clicking_a_corner_rounds_exactly_that_corner_with_the_tool_radius() {
        let f = fixture();
        let mut tool = RoundCornerTool::new(2.0);
        let step = tool.input(&at(PointerPhase::Down, 10.0, 10.0, none()), &f.ctx());
        let ToolStep::Commit(EditCommit::Ops(OpsCommand::PathOp { targets, op })) = step else {
            panic!("expected an Ops commit, got {step:?}")
        };
        assert_eq!(targets, vec![GeometryId::from_raw(1)]);
        let obj = f.snapshot.object(GeometryId::from_raw(1)).unwrap();
        let out = obj.path().apply_op(&op, &CancellationToken::never()).unwrap();
        let found = arcs(&out);
        assert_eq!(found.len(), 1);
        assert!(found[0].0.distance(p(8.0, 8.0)) < 1e-9 && (found[0].1 - 2.0).abs() < 1e-9, "{found:?}");
    }

    #[test]
    fn the_radius_is_changeable_and_limited_by_the_adjacent_sides() {
        let f = fixture();
        let mut tool = RoundCornerTool::new(2.0);
        tool.set_radius_mm(50.0);
        assert_eq!(tool.radius_mm(), 50.0);
        let step = tool.input(&at(PointerPhase::Down, 0.0, 0.0, none()), &f.ctx());
        let ToolStep::Commit(EditCommit::Ops(OpsCommand::PathOp { op, .. })) = step else { panic!("{step:?}") };
        let out = f
            .snapshot
            .object(GeometryId::from_raw(1))
            .unwrap()
            .path()
            .apply_op(&op, &CancellationToken::never())
            .unwrap();
        assert!((arcs(&out)[0].1 - 10.0).abs() < 1e-9, "a 10 mm side limits the radius to 10");
    }

    #[test]
    fn corners_that_cannot_be_rounded_and_empty_space_commit_nothing() {
        let mut f = fixture();
        f.select(&[2]);
        let mut tool = RoundCornerTool::new(2.0);
        // Between a curve and a line: not a straight corner.
        assert_eq!(tool.input(&at(PointerPhase::Down, 110.0, 0.0, none()), &f.ctx()), ToolStep::Nothing);
        // An open end is not a corner.
        assert_eq!(tool.input(&at(PointerPhase::Down, 110.0, -10.0, none()), &f.ctx()), ToolStep::Nothing);
        let away = tool.input(&at(PointerPhase::Down, 300.0, 300.0, none()), &f.ctx());
        assert_eq!(away, ToolStep::Select(SelectionChange::Clear));
        let other = tool.input(&at(PointerPhase::Down, 5.0, 0.0, none()), &f.ctx());
        assert_eq!(other, ToolStep::Select(SelectionChange::Replace(vec![GeometryId::from_raw(1)])));
    }
}
