//! Node editing (E02): drag an anchor or a Bézier handle of the selected path. The edit is a
//! domain [`NodeEdit`] (`Path::edit_nodes`, Kern IM09); segments it does not touch stay
//! bit-identical. The editor only picks the node, follows the pointer and snaps.

use std::sync::Arc;

use burnburn_document::OpsCommand;
use burnburn_geometry::{HandleMode, HandleRef, HandleSide, NodeEdit, NodeRef, Path, Point, Segment};
use burnburn_types::{CancellationToken, GeometryId};

use super::{
    CLICK_TOLERANCE_PT, DRAG_THRESHOLD_PT, EditCommit, HANDLE_RADIUS_PT, Tool, ToolContext, ToolPreview, ToolStep,
    is_escape, key_down, tool_pointer,
};
use crate::input::{CanvasInput, CanvasKey, Modifiers, PointerEvent, PointerPhase};
use crate::node_ops::{NodeCommand, node_command_op};
use crate::selection::{SelectionChange, hit_test};
use crate::snap::SnapCandidate;

/// A pickable point of a path in node-edit mode (domain numbering, see `burnburn_geometry::NodeRef`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeTarget {
    Node(NodeRef),
    Handle(HandleRef),
}

/// All nodes and cubic handles of a path with their positions (for overlays and picking).
pub fn path_nodes(path: &Path) -> Vec<(NodeTarget, Point)> {
    let mut out = Vec::new();
    for (si, sp) in path.subpaths().iter().enumerate() {
        let node = |i: usize| NodeRef { subpath: si, node: i };
        out.push((NodeTarget::Node(node(0)), sp.start()));
        for (i, seg) in sp.segments().iter().enumerate() {
            if let Segment::Cubic { c1, c2, .. } = *seg {
                out.push((NodeTarget::Handle(HandleRef { node: node(i), side: HandleSide::Out }), c1));
                out.push((NodeTarget::Handle(HandleRef { node: node(i + 1), side: HandleSide::In }), c2));
            }
            out.push((NodeTarget::Node(node(i + 1)), seg.end()));
        }
    }
    out
}

/// Handle coupling while dragging: plain = corner (only this span), Alt/Option = smooth,
/// Shift = symmetric (E02: the node type is chosen explicitly).
fn handle_mode(m: Modifiers) -> HandleMode {
    if m.shift {
        HandleMode::Symmetric
    } else if m.alt {
        HandleMode::Smooth
    } else {
        HandleMode::Corner
    }
}

#[derive(Debug, Clone)]
struct NodeDrag {
    target: GeometryId,
    original: Arc<Path>,
    picked: NodeTarget,
    picked_at: Point,
    press: Point,
    /// Last valid edit and its result (an edit the domain rejects keeps the previous one).
    edited: Option<(NodeEdit, Arc<Path>)>,
}

/// Node tool: press on a node of the single selected path to drag it; press elsewhere selects
/// the object under the pointer.
#[derive(Debug, Clone, Default)]
pub struct NodeTool {
    drag: Option<NodeDrag>,
    snap: Option<SnapCandidate>,
    /// Selected nodes of `selected_target` (window-local, never document data).
    selected: Vec<NodeRef>,
    selected_target: Option<GeometryId>,
}

impl NodeTool {
    /// The selected nodes of the single selected object, in selection order.
    pub fn selected_nodes(&self) -> &[NodeRef] {
        &self.selected
    }

    fn clear_nodes(&mut self) {
        self.selected.clear();
        self.selected_target = None;
    }

    /// The domain operation for `cmd` on the selected nodes and whether the domain accepts it.
    fn op_for(&self, cmd: NodeCommand, ctx: &ToolContext<'_>) -> Option<(GeometryId, burnburn_geometry::PathOp)> {
        let target = ctx.selection.single()?;
        let obj = ctx.document.object(target)?;
        let nodes = if self.selected_target == Some(target) { self.selected.as_slice() } else { &[] };
        let op = node_command_op(obj.path(), nodes, cmd).ok()?;
        obj.path().apply_op(&op, &CancellationToken::never()).ok()?;
        Some((target, op))
    }

    /// Whether the button for `cmd` can act on the current node selection.
    pub fn command_enabled(&self, cmd: NodeCommand, ctx: &ToolContext<'_>) -> bool {
        self.op_for(cmd, ctx).is_some()
    }

    /// Applies `cmd` to the selected nodes as exactly one commit (`Nothing` when disabled).
    /// Commands that renumber nodes clear the node selection.
    pub fn command(&mut self, cmd: NodeCommand, ctx: &ToolContext<'_>) -> ToolStep {
        let Some((target, op)) = self.op_for(cmd, ctx) else { return ToolStep::Nothing };
        if !matches!(
            cmd,
            NodeCommand::MakeSmooth
                | NodeCommand::MakeSymmetric
                | NodeCommand::MakeCorner
                | NodeCommand::SegmentsToLines
                | NodeCommand::SegmentsToCurves
                | NodeCommand::ClosePaths
        ) {
            self.clear_nodes();
        }
        ToolStep::Commit(EditCommit::Ops(OpsCommand::PathOp { targets: vec![target], op }))
    }

    /// A double click on the outline inserts a node there (the curve is not changed).
    fn insert_at(
        &self,
        target: GeometryId,
        obj: &burnburn_document::GeometryObject,
        ev: &PointerEvent,
        ctx: &ToolContext<'_>,
    ) -> Option<ToolStep> {
        let near = obj.path().nearest(ev.position)?;
        if near.distance > ctx.tolerance_mm(CLICK_TOLERANCE_PT) {
            return None;
        }
        let sp = obj.path().subpaths().get(near.at.subpath)?;
        if near.at.segment >= sp.segments().len() || !(near.t > 1e-6 && near.t < 1.0 - 1e-6) {
            return None;
        }
        let edit = NodeEdit::InsertNode { subpath: near.at.subpath, segment: near.at.segment, t: near.t };
        let path = obj.path().edit_nodes(&edit).ok()?;
        Some(ToolStep::Commit(EditCommit::EditNodes { target, edit, path }))
    }

    fn press(&mut self, ev: &PointerEvent, ctx: &ToolContext<'_>) -> ToolStep {
        if let Some(target) = ctx.selection.single()
            && let Some(obj) = ctx.document.object(target)
        {
            if self.selected_target != Some(target) {
                self.selected.clear();
                self.selected_target = Some(target);
            }
            let radius = ctx.tolerance_mm(HANDLE_RADIUS_PT);
            let picked = path_nodes(obj.path())
                .into_iter()
                .map(|(n, p)| (p.distance(ev.position), matches!(n, NodeTarget::Handle(_)), n, p))
                .filter(|(d, ..)| *d <= radius)
                .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            if let Some((_, _, picked, picked_at)) = picked {
                if let NodeTarget::Node(n) = picked {
                    match (ev.modifiers.shift, self.selected.iter().position(|s| *s == n)) {
                        (true, Some(i)) => {
                            self.selected.remove(i);
                        }
                        (true, None) => self.selected.push(n),
                        (false, None) => self.selected = vec![n],
                        (false, Some(_)) => {}
                    }
                }
                self.drag = Some(NodeDrag {
                    target,
                    original: Arc::clone(obj.path()),
                    picked,
                    picked_at,
                    press: ev.position,
                    edited: None,
                });
                return ToolStep::PreviewChanged;
            }
            if ev.click_count >= 2
                && let Some(step) = self.insert_at(target, obj, ev, ctx)
            {
                return step;
            }
            self.selected.clear();
        }
        let hits = hit_test(ctx.document, ev.position, ctx.tolerance_mm(CLICK_TOLERANCE_PT));
        match hits.first() {
            Some(top) if ctx.selection.single() == Some(*top) => ToolStep::Nothing,
            Some(top) => ToolStep::Select(SelectionChange::Replace(vec![*top])),
            None if ctx.selection.is_empty() => ToolStep::Nothing,
            None => ToolStep::Select(SelectionChange::Clear),
        }
    }
}

impl Tool for NodeTool {
    fn input(&mut self, input: &CanvasInput, ctx: &ToolContext<'_>) -> ToolStep {
        if is_escape(input) {
            return self.cancel();
        }
        if let Some((CanvasKey::Delete, _)) = key_down(input) {
            return self.command(NodeCommand::DeleteNodes, ctx);
        }
        let Some(ev) = tool_pointer(input) else { return ToolStep::Nothing };
        match ev.phase {
            PointerPhase::Cancel => self.cancel(),
            PointerPhase::Down => self.press(ev, ctx),
            PointerPhase::Move => {
                let Some(drag) = self.drag.as_mut() else { return ToolStep::Nothing };
                let raw = ev.position - drag.press;
                if drag.edited.is_none() && raw.length() < ctx.tolerance_mm(DRAG_THRESHOLD_PT) {
                    return ToolStep::Nothing;
                }
                // The node itself snaps (not the pointer); its own object offers no targets.
                let mut probe = *ev;
                probe.position = drag.picked_at + raw;
                let (to, snap) = ctx.snapped(&probe, &[drag.target]);
                self.snap = snap;
                let edit = match drag.picked {
                    NodeTarget::Node(node) => NodeEdit::MoveNode { node, to },
                    NodeTarget::Handle(handle) => NodeEdit::MoveHandle { handle, to, mode: handle_mode(ev.modifiers) },
                };
                if let Ok(path) = drag.original.edit_nodes(&edit) {
                    drag.edited = Some((edit, Arc::new(path)));
                }
                ToolStep::PreviewChanged
            }
            PointerPhase::Up => {
                self.snap = None;
                match self.drag.take() {
                    Some(NodeDrag { target, edited: Some((edit, path)), .. }) => {
                        ToolStep::Commit(EditCommit::EditNodes { target, edit, path: (*path).clone() })
                    }
                    _ => ToolStep::Nothing,
                }
            }
        }
    }

    fn cancel(&mut self) -> ToolStep {
        self.snap = None;
        match self.drag.take() {
            Some(_) => ToolStep::Cancelled,
            None => ToolStep::Nothing,
        }
    }

    fn preview(&self) -> Option<ToolPreview> {
        let d = self.drag.as_ref()?;
        let (_, path) = d.edited.clone()?;
        Some(ToolPreview::Node { target: d.target, path, dragged: d.picked })
    }

    fn snap(&self) -> Option<SnapCandidate> {
        self.snap
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::CanvasKey;
    use crate::input::PointerPhase;
    use crate::selection::SelectionChange;
    use crate::tool::EditCommit;
    use crate::tool::fixture::*;
    use burnburn_geometry::PathBuilder;
    use burnburn_types::{CancellationToken, StableId};

    /// Three cubic spans: (0,0) → (30,0) → (60,0) → (90,0), handles 10 mm above/below.
    fn wave() -> Path {
        PathBuilder::new()
            .move_to(Point::new(0.0, 0.0))
            .cubic_to(Point::new(10.0, 10.0), Point::new(20.0, 10.0), Point::new(30.0, 0.0))
            .cubic_to(Point::new(40.0, -10.0), Point::new(50.0, -10.0), Point::new(60.0, 0.0))
            .cubic_to(Point::new(70.0, 10.0), Point::new(80.0, 10.0), Point::new(90.0, 0.0))
            .build()
            .unwrap()
    }

    fn fixture() -> Fixture {
        let mut f = Fixture::new(vec![path_obj(1, wave()), rect(2, 200.0, 0.0, 10.0, 10.0)]);
        f.select(&[1]);
        f
    }

    fn drag(tool: &mut NodeTool, f: &Fixture, from: (f64, f64), to: (f64, f64)) -> ToolStep {
        tool.input(&at(PointerPhase::Down, from.0, from.1, none()), &f.ctx());
        tool.input(&at(PointerPhase::Move, to.0, to.1, none()), &f.ctx());
        tool.input(&at(PointerPhase::Up, to.0, to.1, none()), &f.ctx())
    }

    fn replaced(step: &ToolStep) -> Option<(GeometryId, Path)> {
        match step {
            ToolStep::Commit(EditCommit::EditNodes { target, edit, path }) => {
                let original = wave();
                assert_eq!(
                    original.edit_nodes(edit).as_ref(),
                    Ok(path),
                    "the commit carries the domain edit and its result"
                );
                Some((*target, path.clone()))
            }
            _ => None,
        }
    }

    #[test]
    fn path_nodes_lists_anchors_and_cubic_handles() {
        let nodes = path_nodes(&wave());
        let anchors = nodes.iter().filter(|(n, _)| matches!(n, NodeTarget::Node(_))).count();
        assert_eq!((anchors, nodes.len()), (4, 10));
        let out1 = HandleRef { node: NodeRef { subpath: 0, node: 1 }, side: HandleSide::Out };
        assert!(nodes.contains(&(NodeTarget::Handle(out1), Point::new(40.0, -10.0))));
        assert!(nodes.contains(&(NodeTarget::Node(NodeRef { subpath: 0, node: 3 }), Point::new(90.0, 0.0))));
    }

    /// E02 interaction side of `editing_one_bezier_span_preserves_other_spans`.
    #[test]
    fn dragging_one_handle_changes_only_that_span() {
        let f = fixture();
        let mut tool = NodeTool::default();
        let (target, path) = replaced(&drag(&mut tool, &f, (40.5, -10.5), (44.0, -20.0))).unwrap();
        assert_eq!(target, GeometryId::from_raw(1));
        let w = wave();
        let (old, new) = (&w.subpaths()[0], &path.subpaths()[0]);
        assert_eq!(new.start(), old.start());
        assert_eq!(new.segments()[0], old.segments()[0], "span 0 bit-identical");
        assert_eq!(new.segments()[2], old.segments()[2], "span 2 bit-identical");
        let Segment::Cubic { c1, c2, to } = new.segments()[1] else { panic!("still a cubic") };
        assert!(close(c1, Point::new(43.5, -19.5)), "{c1:?} moved by the drag delta");
        assert_eq!((c2, to), (Point::new(50.0, -10.0), Point::new(60.0, 0.0)));
    }

    #[test]
    fn dragging_an_anchor_moves_its_handles_and_only_the_adjacent_spans() {
        let f = fixture();
        let mut tool = NodeTool::default();
        let (_, path) = replaced(&drag(&mut tool, &f, (60.0, 0.0), (60.0, 5.0))).unwrap();
        let w = wave();
        let (old, new) = (&w.subpaths()[0], &path.subpaths()[0]);
        assert_eq!(new.segments()[0], old.segments()[0]);
        assert_eq!(
            new.segments()[1],
            Segment::Cubic { c1: Point::new(40.0, -10.0), c2: Point::new(50.0, -5.0), to: Point::new(60.0, 5.0) }
        );
        assert_eq!(
            new.segments()[2],
            Segment::Cubic { c1: Point::new(70.0, 15.0), c2: Point::new(80.0, 10.0), to: Point::new(90.0, 0.0) }
        );
    }

    #[test]
    fn escape_during_node_drag_restores_the_original_outline() {
        let f = fixture();
        let mut tool = NodeTool::default();
        tool.input(&at(PointerPhase::Down, 40.0, -10.0, none()), &f.ctx());
        tool.input(&at(PointerPhase::Move, 44.0, -20.0, none()), &f.ctx());
        let Some(ToolPreview::Node { path, .. }) = tool.preview() else { panic!("node preview") };
        assert_ne!(*path, wave());
        assert_eq!(tool.input(&key(CanvasKey::Escape), &f.ctx()), ToolStep::Cancelled);
        assert_eq!(tool.preview(), None);
        assert_eq!(tool.input(&at(PointerPhase::Up, 44.0, -20.0, none()), &f.ctx()), ToolStep::Nothing);
    }

    #[test]
    fn release_without_movement_commits_nothing() {
        let f = fixture();
        let mut tool = NodeTool::default();
        assert_eq!(drag(&mut tool, &f, (30.0, 0.0), (30.0, 0.0)), ToolStep::Nothing);
    }

    #[test]
    fn press_away_from_nodes_selects_the_object_under_the_pointer() {
        let f = fixture();
        let mut tool = NodeTool::default();
        let step = tool.input(&at(PointerPhase::Down, 205.0, 5.0, none()), &f.ctx());
        assert_eq!(step, ToolStep::Select(SelectionChange::Replace(vec![GeometryId::from_raw(2)])));
    }

    fn click(tool: &mut NodeTool, f: &Fixture, x: f64, y: f64, mods: Modifiers) {
        tool.input(&at(PointerPhase::Down, x, y, mods), &f.ctx());
        tool.input(&at(PointerPhase::Up, x, y, mods), &f.ctx());
    }

    fn node(i: usize) -> NodeRef {
        NodeRef { subpath: 0, node: i }
    }

    #[test]
    fn clicking_nodes_selects_them_and_shift_adds_or_removes() {
        let f = fixture();
        let mut tool = NodeTool::default();
        click(&mut tool, &f, 30.0, 0.0, none());
        assert_eq!(tool.selected_nodes(), &[node(1)]);
        click(&mut tool, &f, 60.0, 0.0, shift());
        assert_eq!(tool.selected_nodes(), &[node(1), node(2)]);
        click(&mut tool, &f, 30.0, 0.0, shift());
        assert_eq!(tool.selected_nodes(), &[node(2)], "shift on a selected node removes it");
        click(&mut tool, &f, 90.0, 0.0, none());
        assert_eq!(tool.selected_nodes(), &[node(3)], "a plain click replaces the selection");
        // Clicking handles does not select them.
        click(&mut tool, &f, 40.0, -10.0, none());
        assert_eq!(tool.selected_nodes(), &[node(3)]);
    }

    #[test]
    fn clicking_the_body_or_another_object_clears_the_node_selection() {
        let mut f = fixture();
        let mut tool = NodeTool::default();
        click(&mut tool, &f, 30.0, 0.0, none());
        click(&mut tool, &f, 45.0, 0.0, none());
        assert!(tool.selected_nodes().is_empty(), "a click on the path between nodes drops the nodes");
        click(&mut tool, &f, 30.0, 0.0, none());
        let step = tool.input(&at(PointerPhase::Down, 205.0, 5.0, none()), &f.ctx());
        f.follow(&step);
        assert!(tool.selected_nodes().is_empty(), "pressing another object drops the nodes of the old one");
        assert_eq!(f.selection.single(), Some(GeometryId::from_raw(2)));
    }

    #[test]
    fn delete_key_removes_the_selected_nodes_as_one_commit() {
        let f = fixture();
        let mut tool = NodeTool::default();
        click(&mut tool, &f, 30.0, 0.0, none());
        click(&mut tool, &f, 60.0, 0.0, shift());
        let step = tool.input(&key(CanvasKey::Delete), &f.ctx());
        let ToolStep::Commit(EditCommit::Ops(OpsCommand::PathOp { targets, op })) = step else {
            panic!("expected an Ops commit, got {step:?}")
        };
        assert_eq!(targets, vec![GeometryId::from_raw(1)]);
        let out = wave().apply_op(&op, &CancellationToken::never()).unwrap();
        assert_eq!(out.subpaths()[0].segments().len(), 1, "only the end nodes 0 and 3 remain");
        assert!(tool.selected_nodes().is_empty(), "the deleted nodes are no longer selected");
        assert_eq!(tool.input(&key(CanvasKey::Delete), &f.ctx()), ToolStep::Nothing);
    }

    #[test]
    fn commands_commit_the_domain_operation_for_the_selected_nodes() {
        let f = fixture();
        let mut tool = NodeTool::default();
        assert!(!tool.command_enabled(NodeCommand::BreakAtNodes, &f.ctx()), "no node selected");
        assert_eq!(tool.command(NodeCommand::BreakAtNodes, &f.ctx()), ToolStep::Nothing);
        click(&mut tool, &f, 60.0, 0.0, none());
        assert!(tool.command_enabled(NodeCommand::BreakAtNodes, &f.ctx()));
        let ToolStep::Commit(EditCommit::Ops(OpsCommand::PathOp { op, .. })) =
            tool.command(NodeCommand::BreakAtNodes, &f.ctx())
        else {
            panic!("expected a commit")
        };
        let out = wave().apply_op(&op, &CancellationToken::never()).unwrap();
        assert_eq!(out.subpaths().len(), 2);
        assert!(tool.selected_nodes().is_empty(), "breaking renumbers nodes");
        // A kind change keeps node numbers, so the selection stays.
        click(&mut tool, &f, 30.0, 0.0, none());
        let smooth = tool.command(NodeCommand::MakeCorner, &f.ctx());
        assert!(matches!(smooth, ToolStep::Commit(EditCommit::Ops(_))));
        assert_eq!(tool.selected_nodes(), &[node(1)]);
        // The domain refuses what would change nothing: disabled, not a silent empty commit.
        assert!(!tool.command_enabled(NodeCommand::SegmentsToCurves, &f.ctx()), "wave spans are already curves");
    }

    #[test]
    fn double_clicking_the_outline_inserts_a_node_without_changing_the_curve() {
        let line = Path::line(Point::new(0.0, 0.0), Point::new(40.0, 0.0)).unwrap();
        let mut f = Fixture::new(vec![path_obj(1, line.clone())]);
        f.select(&[1]);
        let mut tool = NodeTool::default();
        let step = tool.input(&double_click(10.0, 0.2), &f.ctx());
        let ToolStep::Commit(EditCommit::EditNodes { target, edit, path }) = step else {
            panic!("expected an insert commit, got {step:?}")
        };
        assert_eq!(target, GeometryId::from_raw(1));
        let NodeEdit::InsertNode { subpath: 0, segment: 0, t } = edit else { panic!("{edit:?}") };
        assert!((t - 0.25).abs() < 1e-9, "t = {t}");
        assert_eq!(path.subpaths()[0].segments().len(), 2);
        assert_eq!(path.bounds(), line.bounds(), "the outline is the same line");
        // Far from the outline, or on an existing node, a double click inserts nothing.
        assert!(!matches!(tool.input(&double_click(10.0, 30.0), &f.ctx()), ToolStep::Commit(_)));
        assert!(!matches!(tool.input(&double_click(0.0, 0.0), &f.ctx()), ToolStep::Commit(_)));
    }
}
