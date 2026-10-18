//! Node-edit commands for the selected nodes of one path (W1-L2a): the toolbar / context-menu
//! buttons of node-edit mode. Each maps the node selection to one domain [`PathOp`]; the
//! document applies it as one history entry.

use std::fmt;

use burnburn_geometry::{EndpointRef, NodeEdit, NodeRef, NodeSetOp, Path, PathOp, SegmentKind, SegmentRef};

/// A button of node-edit mode that acts on the selected nodes.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum NodeCommand {
    DeleteNodes,
    /// Cuts the path at the selected nodes (an open path splits, a closed one opens).
    BreakAtNodes,
    MakeSmooth,
    MakeSymmetric,
    MakeCorner,
    /// Selected segments (both ends selected, or the one leaving a single selected node) become lines.
    SegmentsToLines,
    SegmentsToCurves,
    /// Closes the open subpaths that have a selected node (all open subpaths when none is selected).
    ClosePaths,
    /// Joins the two selected ends of open subpaths (a gap is bridged by a line).
    JoinEnds,
    /// The selected node of a closed subpath becomes its start; the selected far end of an open one becomes its start.
    SetStartPoint,
    /// Reverses the subpaths with a selected node (all when none is selected).
    ReverseDirection,
    /// Rounds the selected corners (every straight corner when none is selected).
    RoundCorners {
        radius: f64,
    },
}

/// Why a command is disabled for the current node selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NodeCommandError {
    NoNodesSelected,
    /// The command needs exactly one selected node.
    NeedOneNode,
    /// The command needs exactly two selected ends of open subpaths.
    NeedTwoEnds,
    /// No segment lies between the selected nodes.
    NoSegments,
    /// There is no open subpath to close.
    NothingToClose,
}

impl fmt::Display for NodeCommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            NodeCommandError::NoNodesSelected => "select at least one node",
            NodeCommandError::NeedOneNode => "select exactly one node",
            NodeCommandError::NeedTwoEnds => "select two ends of open paths",
            NodeCommandError::NoSegments => "select nodes that bound a segment",
            NodeCommandError::NothingToClose => "there is no open path to close",
        })
    }
}

impl std::error::Error for NodeCommandError {}

/// The domain operation for `cmd` on `nodes` of `path`. The result may still be rejected by the
/// domain (for example smoothing a corner of a line pair that is already smooth); apply it with
/// `Path::apply_op` to find out before offering the button.
pub fn node_command_op(path: &Path, nodes: &[NodeRef], cmd: NodeCommand) -> Result<PathOp, NodeCommandError> {
    let mut sel: Vec<NodeRef> = Vec::with_capacity(nodes.len());
    for n in nodes {
        if !sel.contains(n) {
            sel.push(*n);
        }
    }
    let need_nodes = || if sel.is_empty() { Err(NodeCommandError::NoNodesSelected) } else { Ok(()) };
    let set = |op| PathOp::NodeSet { op, nodes: sel.clone() };
    match cmd {
        NodeCommand::DeleteNodes => need_nodes().map(|()| set(NodeSetOp::Delete)),
        NodeCommand::BreakAtNodes => need_nodes().map(|()| set(NodeSetOp::Break)),
        NodeCommand::MakeSmooth => need_nodes().map(|()| set(NodeSetOp::Smooth)),
        NodeCommand::MakeSymmetric => need_nodes().map(|()| set(NodeSetOp::Symmetric)),
        NodeCommand::MakeCorner => need_nodes().map(|()| set(NodeSetOp::Corner)),
        NodeCommand::SegmentsToLines | NodeCommand::SegmentsToCurves => {
            need_nodes()?;
            let kind = if cmd == NodeCommand::SegmentsToLines { SegmentKind::Line } else { SegmentKind::Curve };
            let segments = selected_segments(path, &sel);
            if segments.is_empty() {
                Err(NodeCommandError::NoSegments)
            } else {
                Ok(PathOp::SegmentSet { kind, segments })
            }
        }
        NodeCommand::ClosePaths => {
            let wanted: Vec<usize> = if sel.is_empty() {
                (0..path.subpaths().len()).collect()
            } else {
                let mut v: Vec<usize> = sel.iter().map(|n| n.subpath).collect();
                v.sort_unstable();
                v.dedup();
                v
            };
            let edits: Vec<NodeEdit> = wanted
                .into_iter()
                .filter(|&i| path.subpaths().get(i).is_some_and(|s| !s.is_closed() && !s.segments().is_empty()))
                .map(|subpath| NodeEdit::CloseSubpath { subpath })
                .collect();
            if edits.is_empty() { Err(NodeCommandError::NothingToClose) } else { Ok(PathOp::Edits(edits)) }
        }
        NodeCommand::JoinEnds => {
            let [a, b] = sel[..] else { return Err(NodeCommandError::NeedTwoEnds) };
            match (end_of(path, a), end_of(path, b)) {
                (Some(a), Some(b)) => Ok(PathOp::Edits(vec![NodeEdit::JoinEndpoints { a, b }])),
                _ => Err(NodeCommandError::NeedTwoEnds),
            }
        }
        NodeCommand::SetStartPoint => {
            need_nodes()?;
            let [node] = sel[..] else { return Err(NodeCommandError::NeedOneNode) };
            let open_far_end = path
                .subpaths()
                .get(node.subpath)
                .is_some_and(|s| !s.is_closed() && !s.segments().is_empty() && node.node == s.segments().len());
            if open_far_end {
                Ok(PathOp::Reverse { subpaths: Some(vec![node.subpath]) })
            } else {
                Ok(PathOp::Edits(vec![NodeEdit::SetStartPoint { node }]))
            }
        }
        NodeCommand::ReverseDirection => {
            let subpaths = (!sel.is_empty()).then(|| {
                let mut v: Vec<usize> = sel.iter().map(|n| n.subpath).collect();
                v.sort_unstable();
                v.dedup();
                v
            });
            Ok(PathOp::Reverse { subpaths })
        }
        NodeCommand::RoundCorners { radius } => {
            Ok(PathOp::RoundCorners { radius, nodes: (!sel.is_empty()).then(|| sel.clone()) })
        }
    }
}

/// The end of an open subpath that `node` is, if it is one.
fn end_of(path: &Path, node: NodeRef) -> Option<EndpointRef> {
    let sp = path.subpaths().get(node.subpath)?;
    if sp.is_closed() || sp.segments().is_empty() {
        return None;
    }
    if node.node == 0 {
        Some(EndpointRef::Start(node.subpath))
    } else if node.node == sp.segments().len() {
        Some(EndpointRef::End(node.subpath))
    } else {
        None
    }
}

/// Segments with both end nodes selected; a subpath with a single selected node contributes the
/// segment leaving it (the arriving one for the last node of an open subpath). The side that
/// closes a closed subpath joins its last node to node 0.
fn selected_segments(path: &Path, nodes: &[NodeRef]) -> Vec<SegmentRef> {
    let mut out = Vec::new();
    for (si, sp) in path.subpaths().iter().enumerate() {
        let n = sp.segments().len();
        let implicit_closing = sp.is_closed() && sp.end() != sp.start();
        let total = n + usize::from(implicit_closing);
        if total == 0 {
            continue;
        }
        // Node `n` of a closed subpath that returns exactly to its start is node 0.
        let alias_closed = sp.is_closed() && !implicit_closing;
        let mut chosen: Vec<usize> = nodes
            .iter()
            .filter(|r| r.subpath == si)
            .map(|r| if alias_closed && r.node == n { 0 } else { r.node })
            .collect();
        chosen.sort_unstable();
        chosen.dedup();
        let end_node = |i: usize| if sp.is_closed() && i + 1 == total { 0 } else { i + 1 };
        let both: Vec<usize> = (0..total).filter(|&i| chosen.contains(&i) && chosen.contains(&end_node(i))).collect();
        let picked = match (both.is_empty(), chosen.as_slice()) {
            (false, _) => both,
            (true, [only]) if *only < total => vec![*only],
            (true, [only]) if *only >= total && total > 0 => vec![total - 1],
            _ => Vec::new(),
        };
        out.extend(picked.into_iter().map(|segment| SegmentRef { subpath: si, segment }));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use burnburn_geometry::{PathBuilder, Point, Segment};

    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    fn n(subpath: usize, node: usize) -> NodeRef {
        NodeRef { subpath, node }
    }

    fn zigzag() -> Path {
        Path::polyline(&[p(0.0, 0.0), p(1.0, 1.0), p(2.0, 0.0), p(3.0, 1.0), p(4.0, 0.0)], false).unwrap()
    }

    fn square() -> Path {
        Path::polyline(&[p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)], true).unwrap()
    }

    fn result(path: &Path, nodes: &[NodeRef], cmd: NodeCommand) -> Path {
        let op = node_command_op(path, nodes, cmd).unwrap();
        path.apply_op(&op, &burnburn_types::CancellationToken::never()).unwrap()
    }

    fn vertex_counts(path: &Path) -> Vec<usize> {
        path.subpaths().iter().map(|s| s.segments().len() + 1).collect()
    }

    #[test]
    fn delete_break_and_kind_commands_name_the_selected_nodes() {
        let nodes = [n(0, 1), n(0, 3)];
        assert_eq!(vertex_counts(&result(&zigzag(), &nodes, NodeCommand::DeleteNodes)), vec![3]);
        assert_eq!(vertex_counts(&result(&zigzag(), &nodes, NodeCommand::BreakAtNodes)), vec![2, 3, 2]);
        let smooth = result(&zigzag(), &nodes, NodeCommand::MakeSmooth);
        let cubics = smooth.subpaths()[0].segments().iter().filter(|s| matches!(s, Segment::Cubic { .. })).count();
        assert_eq!(cubics, 4, "both selected nodes smoothed the spans around them");
        let sym = result(&zigzag(), &nodes[..1], NodeCommand::MakeSymmetric);
        assert!(matches!(sym.subpaths()[0].segments()[0], Segment::Cubic { .. }));
    }

    #[test]
    fn corner_command_retracts_handles_of_curved_nodes() {
        let wave = PathBuilder::new()
            .move_to(p(0.0, 0.0))
            .cubic_to(p(2.0, 5.0), p(8.0, 5.0), p(10.0, 0.0))
            .cubic_to(p(12.0, -5.0), p(18.0, -5.0), p(20.0, 0.0))
            .build()
            .unwrap();
        let out = result(&wave, &[n(0, 1)], NodeCommand::MakeCorner);
        assert_eq!(
            out.subpaths()[0].segments()[0],
            Segment::Cubic { c1: p(2.0, 5.0), c2: p(10.0, 0.0), to: p(10.0, 0.0) }
        );
    }

    #[test]
    fn commands_without_nodes_are_disabled_where_a_selection_is_needed() {
        for cmd in [
            NodeCommand::DeleteNodes,
            NodeCommand::BreakAtNodes,
            NodeCommand::MakeSmooth,
            NodeCommand::MakeSymmetric,
            NodeCommand::MakeCorner,
            NodeCommand::SegmentsToLines,
            NodeCommand::SegmentsToCurves,
        ] {
            assert_eq!(node_command_op(&zigzag(), &[], cmd), Err(NodeCommandError::NoNodesSelected), "{cmd:?}");
        }
    }

    #[test]
    fn segment_commands_pick_segments_between_selected_nodes_or_after_a_single_one() {
        // Nodes 1 and 2 selected: the segment between them only.
        let curved = result(&zigzag(), &[n(0, 1), n(0, 2)], NodeCommand::SegmentsToCurves);
        let kinds: Vec<bool> =
            curved.subpaths()[0].segments().iter().map(|s| matches!(s, Segment::Cubic { .. })).collect();
        assert_eq!(kinds, vec![false, true, false, false]);
        // One node: the segment that leaves it.
        let one = result(&zigzag(), &[n(0, 2)], NodeCommand::SegmentsToCurves);
        let kinds: Vec<bool> =
            one.subpaths()[0].segments().iter().map(|s| matches!(s, Segment::Cubic { .. })).collect();
        assert_eq!(kinds, vec![false, false, true, false]);
        // The last node of an open path has none leaving: the arriving one.
        let last = result(&zigzag(), &[n(0, 4)], NodeCommand::SegmentsToCurves);
        assert!(matches!(last.subpaths()[0].segments()[3], Segment::Cubic { .. }));
        // Back to lines.
        let line = result(&curved, &[n(0, 1), n(0, 2)], NodeCommand::SegmentsToLines);
        assert!(line.subpaths()[0].segments().iter().all(|s| matches!(s, Segment::Line { .. })));
        // Non-adjacent nodes of an open path bound no segment together; with two nodes selected nothing is left over.
        let gap = node_command_op(&zigzag(), &[n(0, 0), n(0, 3)], NodeCommand::SegmentsToLines);
        assert_eq!(gap, Err(NodeCommandError::NoSegments));
    }

    #[test]
    fn the_closing_side_of_a_closed_path_is_a_segment_between_its_last_and_first_node() {
        let out = result(&square(), &[n(0, 3), n(0, 0)], NodeCommand::SegmentsToCurves);
        let segs = out.subpaths()[0].segments();
        assert!(matches!(segs.last().unwrap(), Segment::Cubic { .. }), "the closing line was converted");
        assert!(matches!(segs[0], Segment::Line { .. }));
    }

    #[test]
    fn close_paths_closes_open_subpaths_with_a_selected_node_or_all_of_them() {
        let two = Path::from_subpaths(vec![zigzag().subpaths()[0].clone(), zigzag().subpaths()[0].clone()]);
        let one = result(&two, &[n(1, 2)], NodeCommand::ClosePaths);
        assert_eq!(one.subpaths().iter().map(|s| s.is_closed()).collect::<Vec<_>>(), vec![false, true]);
        let all = result(&two, &[], NodeCommand::ClosePaths);
        assert!(all.subpaths().iter().all(|s| s.is_closed()));
        assert_eq!(node_command_op(&square(), &[], NodeCommand::ClosePaths), Err(NodeCommandError::NothingToClose));
    }

    #[test]
    fn join_ends_needs_exactly_two_ends_of_open_subpaths() {
        let a = Path::line(p(0.0, 0.0), p(10.0, 0.0)).unwrap();
        let b = Path::line(p(12.0, 0.0), p(20.0, 0.0)).unwrap();
        let two = Path::from_subpaths(vec![a.subpaths()[0].clone(), b.subpaths()[0].clone()]);
        let joined = result(&two, &[n(0, 1), n(1, 0)], NodeCommand::JoinEnds);
        assert_eq!(vertex_counts(&joined), vec![4], "the 2 mm gap is bridged by a line");
        assert_eq!(node_command_op(&two, &[n(0, 1)], NodeCommand::JoinEnds), Err(NodeCommandError::NeedTwoEnds));
        assert_eq!(
            node_command_op(&zigzag(), &[n(0, 1), n(0, 2)], NodeCommand::JoinEnds),
            Err(NodeCommandError::NeedTwoEnds),
            "interior nodes are not ends"
        );
        assert_eq!(
            node_command_op(&square(), &[n(0, 0), n(0, 1)], NodeCommand::JoinEnds),
            Err(NodeCommandError::NeedTwoEnds),
            "closed subpaths have no ends"
        );
        // The two ends of one open subpath close it.
        let closed = result(&zigzag(), &[n(0, 0), n(0, 4)], NodeCommand::JoinEnds);
        assert!(closed.subpaths()[0].is_closed());
    }

    #[test]
    fn set_start_point_moves_the_start_of_a_closed_path_and_flips_an_open_one() {
        let out = result(&square(), &[n(0, 2)], NodeCommand::SetStartPoint);
        assert_eq!(out.subpaths()[0].start(), p(10.0, 10.0));
        let flipped = result(&zigzag(), &[n(0, 4)], NodeCommand::SetStartPoint);
        assert_eq!(flipped.subpaths()[0].start(), p(4.0, 0.0), "the far end became the start");
        assert_eq!(
            node_command_op(&square(), &[n(0, 1), n(0, 2)], NodeCommand::SetStartPoint),
            Err(NodeCommandError::NeedOneNode)
        );
        assert_eq!(node_command_op(&square(), &[], NodeCommand::SetStartPoint), Err(NodeCommandError::NoNodesSelected));
        // An interior node of an open path cannot be a start.
        let op = node_command_op(&zigzag(), &[n(0, 2)], NodeCommand::SetStartPoint).unwrap();
        assert!(zigzag().apply_op(&op, &burnburn_types::CancellationToken::never()).is_err());
    }

    #[test]
    fn reverse_and_round_use_the_selection_or_everything() {
        let two = Path::from_subpaths(vec![zigzag().subpaths()[0].clone(), zigzag().subpaths()[0].clone()]);
        let one = result(&two, &[n(1, 1)], NodeCommand::ReverseDirection);
        assert_eq!(one.subpaths()[0].start(), p(0.0, 0.0));
        assert_eq!(one.subpaths()[1].start(), p(4.0, 0.0));
        let all = result(&two, &[], NodeCommand::ReverseDirection);
        assert_eq!((all.subpaths()[0].start(), all.subpaths()[1].start()), (p(4.0, 0.0), p(4.0, 0.0)));
        let some = result(&square(), &[n(0, 1)], NodeCommand::RoundCorners { radius: 2.0 });
        assert_eq!(some.subpaths()[0].segments().iter().filter(|s| matches!(s, Segment::Arc(_))).count(), 1);
        let every = result(&square(), &[], NodeCommand::RoundCorners { radius: 2.0 });
        assert_eq!(every.subpaths()[0].segments().iter().filter(|s| matches!(s, Segment::Arc(_))).count(), 4);
    }
}
