//! Node editing (E02): drag an anchor or a Bézier handle of the selected path. The edit is a
//! domain [`NodeEdit`] (`Path::edit_nodes`, Kern IM09); segments it does not touch stay
//! bit-identical. The editor only picks the node, follows the pointer and snaps.

use std::sync::Arc;

use burnburn_geometry::{HandleMode, HandleRef, HandleSide, NodeEdit, NodeRef, Path, Point, Segment};
use burnburn_types::GeometryId;

use super::{
    CLICK_TOLERANCE_PT, DRAG_THRESHOLD_PT, EditCommit, HANDLE_RADIUS_PT, Tool, ToolContext, ToolPreview, ToolStep,
    is_escape, tool_pointer,
};
use crate::input::{CanvasInput, Modifiers, PointerEvent, PointerPhase};
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
}

impl NodeTool {
    fn press(&mut self, ev: &PointerEvent, ctx: &ToolContext<'_>) -> ToolStep {
        if let Some(target) = ctx.selection.single()
            && let Some(obj) = ctx.document.object(target)
        {
            let radius = ctx.tolerance_mm(HANDLE_RADIUS_PT);
            let picked = path_nodes(obj.path())
                .into_iter()
                .map(|(n, p)| (p.distance(ev.position), matches!(n, NodeTarget::Handle(_)), n, p))
                .filter(|(d, ..)| *d <= radius)
                .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            if let Some((_, _, picked, picked_at)) = picked {
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
    use crate::input::{CanvasKey, PointerPhase};
    use crate::selection::SelectionChange;
    use crate::tool::EditCommit;
    use crate::tool::fixture::*;
    use burnburn_geometry::PathBuilder;
    use burnburn_types::StableId;

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
}
