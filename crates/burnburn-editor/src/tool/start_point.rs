//! Set Start Point (W1-L2a): click a node of the selected path to make it where cutting starts.
//! A closed contour is rotated to start there; clicking the far end of an open path reverses it
//! so that end becomes the start. The path's shape never changes.

use burnburn_document::OpsCommand;
use burnburn_types::CancellationToken;

use super::pick::{anchor_under, select_under};
use super::{EditCommit, Tool, ToolContext, ToolPreview, ToolStep, tool_pointer};
use crate::input::{CanvasInput, PointerPhase};
use crate::node_ops::{NodeCommand, node_command_op};
use crate::snap::SnapCandidate;

#[derive(Debug, Clone, Default)]
pub struct StartPointTool;

impl Tool for StartPointTool {
    fn input(&mut self, input: &CanvasInput, ctx: &ToolContext<'_>) -> ToolStep {
        let Some(ev) = tool_pointer(input) else { return ToolStep::Nothing };
        if ev.phase != PointerPhase::Down {
            return ToolStep::Nothing;
        }
        let Some((target, node)) = anchor_under(ctx, ev) else { return select_under(ctx, ev) };
        let Some(obj) = ctx.document.object(target) else { return ToolStep::Nothing };
        let accepted = node_command_op(obj.path(), &[node], NodeCommand::SetStartPoint)
            .ok()
            .filter(|op| obj.path().apply_op(op, &CancellationToken::never()).is_ok());
        match accepted {
            Some(op) => ToolStep::Commit(EditCommit::Ops(OpsCommand::PathOp { targets: vec![target], op })),
            None => ToolStep::Nothing,
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
    use burnburn_geometry::{Path, Point};
    use burnburn_types::{GeometryId, StableId};

    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    fn fixture() -> Fixture {
        let square = Path::polyline(&[p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)], true).unwrap();
        let open = Path::polyline(&[p(100.0, 0.0), p(110.0, 5.0), p(120.0, 0.0)], false).unwrap();
        let mut f = Fixture::new(vec![path_obj(1, square), path_obj(2, open)]);
        f.select(&[1]);
        f
    }

    fn click(tool: &mut StartPointTool, f: &Fixture, x: f64, y: f64) -> ToolStep {
        tool.input(&at(PointerPhase::Down, x, y, none()), &f.ctx())
    }

    fn applied(step: &ToolStep, path: &Path) -> Path {
        let ToolStep::Commit(EditCommit::Ops(OpsCommand::PathOp { op, .. })) = step else {
            panic!("expected an Ops commit, got {step:?}")
        };
        path.apply_op(op, &CancellationToken::never()).unwrap()
    }

    #[test]
    fn clicking_a_corner_of_a_closed_path_makes_it_the_start() {
        let f = fixture();
        let mut tool = StartPointTool;
        let step = click(&mut tool, &f, 10.0, 10.0);
        let obj = f.snapshot.object(GeometryId::from_raw(1)).unwrap();
        let out = applied(&step, obj.path());
        assert_eq!(out.subpaths()[0].start(), p(10.0, 10.0));
        assert_eq!(out.signed_area(), obj.path().signed_area(), "same outline");
        if let ToolStep::Commit(EditCommit::Ops(OpsCommand::PathOp { targets, .. })) = &step {
            assert_eq!(targets, &vec![GeometryId::from_raw(1)]);
        }
    }

    #[test]
    fn the_current_start_and_empty_space_commit_nothing() {
        let f = fixture();
        let mut tool = StartPointTool;
        assert_eq!(click(&mut tool, &f, 0.0, 0.0), ToolStep::Nothing, "already the start");
        let away = click(&mut tool, &f, 300.0, 300.0);
        assert_eq!(away, ToolStep::Select(SelectionChange::Clear));
        // Hover and release are ignored.
        assert_eq!(tool.input(&hover(10.0, 10.0), &f.ctx()), ToolStep::Nothing);
        assert_eq!(tool.input(&at(PointerPhase::Up, 10.0, 10.0, none()), &f.ctx()), ToolStep::Nothing);
    }

    #[test]
    fn on_an_open_path_only_the_far_end_can_become_the_start() {
        let mut f = fixture();
        f.select(&[2]);
        let mut tool = StartPointTool;
        assert_eq!(click(&mut tool, &f, 100.0, 0.0), ToolStep::Nothing, "already the start");
        assert_eq!(click(&mut tool, &f, 110.0, 5.0), ToolStep::Nothing, "an interior node is not an end");
        let step = click(&mut tool, &f, 120.0, 0.0);
        let obj = f.snapshot.object(GeometryId::from_raw(2)).unwrap();
        assert_eq!(applied(&step, obj.path()).subpaths()[0].start(), p(120.0, 0.0));
    }

    #[test]
    fn pressing_an_unselected_object_selects_it() {
        let f = fixture();
        let mut tool = StartPointTool;
        let step = click(&mut tool, &f, 105.0, 2.5);
        assert_eq!(step, ToolStep::Select(SelectionChange::Replace(vec![GeometryId::from_raw(2)])));
    }
}
