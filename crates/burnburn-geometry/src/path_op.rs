//! Whole-path operations as data (W1-L2a): the edits a toolbar button or dialog applies to one
//! or several selected objects at once. A document records one of these as a single undo step.

use std::fmt;

use burnburn_types::CancellationToken;

use crate::node::{NodeEditError, Parts};
use crate::{
    NodeEdit, NodeKind, NodeRef, OptimizeError, Path, ProductionTolerance, RoundError, SegmentKind, SegmentRef,
    Winding, node_ops,
};

/// A multi-node operation (indices stay valid within one operation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeSetOp {
    /// Removes the nodes; the spans around each removed node merge (outer handles are kept).
    Delete,
    /// Cuts the subpath at the nodes: a closed subpath opens, an open one splits (its ends are ignored).
    Break,
    Smooth,
    Symmetric,
    Corner,
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum PathOp {
    /// Node edits applied in order; each index refers to the path after the previous edit.
    Edits(Vec<NodeEdit>),
    NodeSet {
        op: NodeSetOp,
        nodes: Vec<NodeRef>,
    },
    SegmentSet {
        kind: SegmentKind,
        segments: Vec<SegmentRef>,
    },
    /// Fillets at `nodes` (`None`: every straight corner).
    RoundCorners {
        radius: f64,
        nodes: Option<Vec<NodeRef>>,
    },
    Optimize {
        tolerance: ProductionTolerance,
    },
    /// Gives every closed subpath the direction (reverses the others); start nodes are kept.
    SetWinding {
        winding: Winding,
    },
    /// Reverses the listed subpaths (`None`: all). Closed subpaths keep their start node.
    Reverse {
        subpaths: Option<Vec<usize>>,
    },
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum PathOpError {
    Node(NodeEditError),
    Round(RoundError),
    Optimize(OptimizeError),
    /// The operation names nothing to act on, or a parameter is not valid.
    InvalidParameter,
    /// The operation would leave the path as it is.
    NoChange,
}

impl fmt::Display for PathOpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "path operation rejected: {self:?}")
    }
}

impl std::error::Error for PathOpError {}

impl Path {
    /// Applies the operation, returning the new path. The input is never modified; an error
    /// leaves nothing half-done.
    pub fn apply_op(&self, op: &PathOp, cancel: &CancellationToken) -> Result<Path, PathOpError> {
        let out = match op {
            PathOp::Edits(edits) => {
                if edits.is_empty() {
                    return Err(PathOpError::InvalidParameter);
                }
                edits.iter().try_fold(self.clone(), |path, e| path.edit_nodes(e)).map_err(PathOpError::Node)?
            }
            PathOp::NodeSet { op, nodes } => node_set(self, *op, nodes)?,
            PathOp::SegmentSet { kind, segments } => segment_set(self, *kind, segments)?,
            PathOp::RoundCorners { radius, nodes } => {
                self.round_corners(*radius, nodes.as_deref()).map_err(PathOpError::Round)?.path
            }
            PathOp::Optimize { tolerance } => self.optimize(tolerance, cancel).map_err(PathOpError::Optimize)?.path,
            PathOp::SetWinding { winding } => set_winding_all(self, *winding)?,
            PathOp::Reverse { subpaths } => reverse(self, subpaths.as_deref())?,
        };
        if &out == self { Err(PathOpError::NoChange) } else { Ok(out) }
    }
}

fn by_subpath(path: &Path, nodes: &[NodeRef]) -> Result<Vec<Vec<usize>>, PathOpError> {
    let mut per = vec![Vec::new(); path.subpaths().len()];
    for n in nodes {
        per.get_mut(n.subpath).ok_or(PathOpError::Node(NodeEditError::NoSuchSubpath(n.subpath)))?.push(n.node);
    }
    Ok(per)
}

fn node_set(path: &Path, op: NodeSetOp, nodes: &[NodeRef]) -> Result<Path, PathOpError> {
    if nodes.is_empty() {
        return Err(PathOpError::InvalidParameter);
    }
    let kind = match op {
        NodeSetOp::Smooth => Some(NodeKind::Smooth),
        NodeSetOp::Symmetric => Some(NodeKind::Symmetric),
        NodeSetOp::Corner => Some(NodeKind::Corner),
        NodeSetOp::Delete | NodeSetOp::Break => None,
    };
    if let Some(kind) = kind {
        // Kind changes never renumber nodes; nodes that cannot change are skipped.
        let mut current = path.clone();
        let mut changed = false;
        for &node in nodes {
            match current.edit_nodes(&NodeEdit::SetNodeKind { node, kind }) {
                Ok(next) => {
                    current = next;
                    changed = true;
                }
                Err(NodeEditError::NoChange) => {}
                Err(e) => return Err(PathOpError::Node(e)),
            }
        }
        return if changed { Ok(current) } else { Err(PathOpError::NoChange) };
    }
    let per = by_subpath(path, nodes)?;
    let mut subs = Vec::with_capacity(path.subpaths().len());
    let mut changed = false;
    for (si, (sp, list)) in path.subpaths().iter().zip(&per).enumerate() {
        if list.is_empty() {
            subs.push(sp.clone());
            continue;
        }
        let parts = Parts::of(sp);
        let pieces = match op {
            NodeSetOp::Delete => vec![node_ops::delete_nodes(&parts, si, list).map_err(PathOpError::Node)?],
            _ => match node_ops::break_nodes(&parts, si, list) {
                Ok(pieces) => pieces,
                // A subpath none of whose listed nodes can be cut is left alone.
                Err(NodeEditError::NoChange) => {
                    subs.push(sp.clone());
                    continue;
                }
                Err(e) => return Err(PathOpError::Node(e)),
            },
        };
        changed = true;
        for piece in pieces {
            subs.push(piece.build().map_err(PathOpError::Node)?);
        }
    }
    if changed { Ok(Path::from_subpaths(subs)) } else { Err(PathOpError::NoChange) }
}

fn segment_set(path: &Path, kind: SegmentKind, segments: &[SegmentRef]) -> Result<Path, PathOpError> {
    if segments.is_empty() {
        return Err(PathOpError::InvalidParameter);
    }
    let mut current = path.clone();
    let mut changed = false;
    for s in segments {
        match current.edit_nodes(&NodeEdit::SetSegmentKind { subpath: s.subpath, segment: s.segment, kind }) {
            Ok(next) => {
                current = next;
                changed = true;
            }
            Err(NodeEditError::NoChange) => {}
            Err(e) => return Err(PathOpError::Node(e)),
        }
    }
    if changed { Ok(current) } else { Err(PathOpError::NoChange) }
}

fn set_winding_all(path: &Path, winding: Winding) -> Result<Path, PathOpError> {
    if winding == Winding::Degenerate {
        return Err(PathOpError::InvalidParameter);
    }
    let mut current = path.clone();
    for si in 0..path.subpaths().len() {
        match current.edit_nodes(&NodeEdit::SetWinding { subpath: si, winding }) {
            Ok(next) => current = next,
            // Open and zero-area subpaths have no direction to set; already-right ones stay.
            Err(NodeEditError::NoChange | NodeEditError::SubpathOpen(_)) => {}
            Err(e) => return Err(PathOpError::Node(e)),
        }
    }
    Ok(current)
}

fn reverse(path: &Path, which: Option<&[usize]>) -> Result<Path, PathOpError> {
    if let Some(list) = which {
        if list.is_empty() {
            return Err(PathOpError::InvalidParameter);
        }
        if let Some(&bad) = list.iter().find(|&&i| i >= path.subpaths().len()) {
            return Err(PathOpError::Node(NodeEditError::NoSuchSubpath(bad)));
        }
    }
    let subs = path
        .subpaths()
        .iter()
        .enumerate()
        .map(|(i, sp)| {
            if which.is_some_and(|w| !w.contains(&i)) {
                return Ok(sp.clone());
            }
            node_ops::reverse_in_place(&Parts::of(sp)).build()
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(PathOpError::Node)?;
    Ok(Path::from_subpaths(subs))
}
