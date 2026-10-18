//! Node and handle editing (E02). Every edit returns a new [`Path`]; segments it does not touch
//! are copied unchanged, so a local edit never moves an unselected curve range.
//!
//! Node numbering per subpath: node 0 is the start, node `i` (≥ 1) is the end of segment `i − 1`.
//! A cubic segment `i` carries the *out* handle of node `i` (`c1`) and the *in* handle of node
//! `i + 1` (`c2`).

use std::fmt;

use crate::{Path, Point, Segment, Subpath};

/// One node of one subpath.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeRef {
    pub subpath: usize,
    pub node: usize,
}

/// Which handle of a node: `In` belongs to the segment arriving at the node, `Out` to the one leaving it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HandleSide {
    In,
    Out,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HandleRef {
    pub node: NodeRef,
    pub side: HandleSide,
}

/// How the opposite handle of the same node follows a handle move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HandleMode {
    /// The opposite handle stays where it is.
    Corner,
    /// The opposite handle becomes collinear (tangent continuity) and keeps its length.
    Smooth,
    /// The opposite handle mirrors the moved one through the node.
    Symmetric,
}

/// An end of an open subpath.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EndpointRef {
    Start(usize),
    End(usize),
}

/// A node-level edit as data, so documents can validate and record it as one command.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum NodeEdit {
    /// Moves a node; its own handles move with it.
    MoveNode {
        node: NodeRef,
        to: Point,
    },
    MoveHandle {
        handle: HandleRef,
        to: Point,
        mode: HandleMode,
    },
    /// Splits `segment` at parameter `t ∈ (0, 1)` without changing the curve (De Casteljau for cubics).
    InsertNode {
        subpath: usize,
        segment: usize,
        t: f64,
    },
    /// Removes a node; neighbouring spans merge (outer handles are kept).
    DeleteNode {
        node: NodeRef,
    },
    CloseSubpath {
        subpath: usize,
    },
    /// Opens a closed subpath at `node`, or splits an open one into two at an interior node.
    BreakAt {
        node: NodeRef,
    },
    /// Connects two open ends; coincident ends merge, a gap is bridged by a line. The two ends of
    /// one subpath close it.
    JoinEndpoints {
        a: EndpointRef,
        b: EndpointRef,
    },
    ReverseSubpath {
        subpath: usize,
    },
    /// Replaces every arc in the subpath by cubics (at most a quarter turn each).
    ConvertToCubic {
        subpath: usize,
    },
}

/// Why a node edit was rejected. The input path is never partially modified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NodeEditError {
    NoSuchSubpath(usize),
    NoSuchNode(NodeRef),
    NoSuchSegment {
        subpath: usize,
        segment: usize,
    },
    /// The handle does not exist (the adjacent segment is not a cubic).
    NoSuchHandle(HandleRef),
    /// A split parameter outside `(0, 1)` or not finite.
    InvalidParameter,
    NotFinite,
    /// The edit would move the end of an arc; convert the subpath with `ConvertToCubic` first.
    ArcSegment {
        subpath: usize,
        segment: usize,
    },
    /// Removing the node would leave a subpath without a segment.
    TooFewNodes,
    /// The referenced end belongs to a closed subpath.
    SubpathClosed(usize),
    /// The subpath is already in the requested state (for example closing a closed subpath).
    NoChange,
}

impl fmt::Display for NodeEditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "node edit rejected: {self:?}")
    }
}

impl std::error::Error for NodeEditError {}

/// A mutable working copy of one subpath.
#[derive(Clone)]
struct Parts {
    start: Point,
    segs: Vec<Segment>,
    closed: bool,
}

impl Parts {
    fn of(sp: &Subpath) -> Parts {
        Parts { start: sp.start(), segs: sp.segments().to_vec(), closed: sp.is_closed() }
    }

    fn build(self) -> Result<Subpath, NodeEditError> {
        Subpath::new(self.start, self.segs, self.closed).map_err(|_| NodeEditError::NotFinite)
    }

    fn node_point(&self, i: usize) -> Point {
        if i == 0 { self.start } else { self.segs[i - 1].end() }
    }

    fn end(&self) -> Point {
        self.segs.last().map_or(self.start, Segment::end)
    }

    /// For a closed subpath whose last segment returns exactly to the start, nodes 0 and `n`
    /// are the same point.
    fn alias(&self, i: usize) -> Option<usize> {
        let n = self.segs.len();
        if !self.closed || n == 0 || self.end() != self.start {
            return None;
        }
        match i {
            0 => Some(n),
            _ if i == n => Some(0),
            _ => None,
        }
    }

    /// The segment index that starts at node `i`, if any.
    fn outgoing(&self, i: usize) -> Option<usize> {
        if i < self.segs.len() { Some(i) } else { self.alias(i).filter(|&a| a < self.segs.len()) }
    }

    /// The segment index that ends at node `i`, if any.
    fn incoming(&self, i: usize) -> Option<usize> {
        if i > 0 { Some(i - 1) } else { self.alias(i).map(|a| a - 1) }
    }

    /// The closed subpath as a cycle of segments with an explicit closing line.
    fn ring(&self) -> Vec<Segment> {
        let mut ring = self.segs.clone();
        if self.end() != self.start {
            ring.push(Segment::Line { to: self.start });
        }
        ring
    }

    /// The cycle rotated to start at node `k` (an index into `ring`).
    fn rotated(start: Point, ring: &[Segment], k: usize) -> (Point, Vec<Segment>) {
        let begin = if k == 0 { start } else { ring[k - 1].end() };
        let mut segs = ring[k..].to_vec();
        segs.extend_from_slice(&ring[..k]);
        (begin, segs)
    }

    /// Back from a ring: a trailing line to the start becomes the implicit closing line.
    fn from_ring(start: Point, mut segs: Vec<Segment>) -> Parts {
        if segs.len() > 1 && matches!(segs.last(), Some(Segment::Line { to }) if *to == start) {
            segs.pop();
        }
        Parts { start, segs, closed: true }
    }

    fn reversed(&self) -> Parts {
        let mut segs = Vec::with_capacity(self.segs.len());
        let mut from = self.start;
        let mut forward = Vec::with_capacity(self.segs.len());
        for seg in &self.segs {
            forward.push((from, *seg));
            from = seg.end();
        }
        for (from, seg) in forward.iter().rev() {
            segs.push(seg.reversed(*from));
        }
        Parts { start: self.end(), segs, closed: self.closed }
    }
}

fn merge(a: Segment, b: Segment) -> Segment {
    match (a, b) {
        (Segment::Cubic { c1, .. }, Segment::Cubic { c2, to, .. }) => Segment::Cubic { c1, c2, to },
        _ => Segment::Line { to: b.end() },
    }
}

impl Path {
    /// Applies one node edit, returning the edited path. Untouched segments are copied
    /// bit-identically; on error nothing is returned but the error.
    pub fn edit_nodes(&self, edit: &NodeEdit) -> Result<Path, NodeEditError> {
        let mut subs: Vec<Subpath> = self.subpaths().to_vec();
        let get = |s: usize| self.subpaths().get(s).map(Parts::of).ok_or(NodeEditError::NoSuchSubpath(s));
        match *edit {
            NodeEdit::MoveNode { node, to } => {
                if !to.is_finite() {
                    return Err(NodeEditError::NotFinite);
                }
                let mut p = get(node.subpath)?;
                if node.node > p.segs.len() {
                    return Err(NodeEditError::NoSuchNode(node));
                }
                let delta = to - p.node_point(node.node);
                let mut nodes = vec![node.node];
                nodes.extend(p.alias(node.node));
                for i in nodes {
                    move_node(&mut p, node.subpath, i, to, delta)?;
                }
                subs[node.subpath] = p.build()?;
            }
            NodeEdit::MoveHandle { handle, to, mode } => {
                if !to.is_finite() {
                    return Err(NodeEditError::NotFinite);
                }
                let mut p = get(handle.node.subpath)?;
                let i = handle.node.node;
                if i > p.segs.len() {
                    return Err(NodeEditError::NoSuchNode(handle.node));
                }
                let node = p.node_point(i);
                let (own, other) = match handle.side {
                    HandleSide::Out => (p.outgoing(i), p.incoming(i)),
                    HandleSide::In => (p.incoming(i), p.outgoing(i)),
                };
                let own = own.ok_or(NodeEditError::NoSuchHandle(handle))?;
                set_handle(&mut p.segs[own], handle.side, to).ok_or(NodeEditError::NoSuchHandle(handle))?;
                if let Some(other) = other.filter(|&o| o != own) {
                    let opposite = match handle.side {
                        HandleSide::Out => HandleSide::In,
                        HandleSide::In => HandleSide::Out,
                    };
                    if let Some(current) = handle_point(&p.segs[other], opposite) {
                        let arm = to - node;
                        let new = match mode {
                            HandleMode::Corner => current,
                            HandleMode::Symmetric => node - arm,
                            HandleMode::Smooth => {
                                let len = arm.length();
                                if len > 0.0 { node - arm * ((current - node).length() / len) } else { current }
                            }
                        };
                        set_handle(&mut p.segs[other], opposite, new);
                    }
                }
                subs[handle.node.subpath] = p.build()?;
            }
            NodeEdit::InsertNode { subpath, segment, t } => {
                if !(t > 0.0 && t < 1.0) {
                    return Err(NodeEditError::InvalidParameter);
                }
                let mut p = get(subpath)?;
                if segment >= p.segs.len() {
                    return Err(NodeEditError::NoSuchSegment { subpath, segment });
                }
                let (a, b) = p.segs[segment].split(p.node_point(segment), t);
                p.segs.splice(segment..=segment, [a, b]);
                subs[subpath] = p.build()?;
            }
            NodeEdit::DeleteNode { node } => {
                let p = get(node.subpath)?;
                let n = p.segs.len();
                if node.node > n {
                    return Err(NodeEditError::NoSuchNode(node));
                }
                subs[node.subpath] = delete_node(p, node.node)?.build()?;
            }
            NodeEdit::CloseSubpath { subpath } => {
                let mut p = get(subpath)?;
                if p.closed {
                    return Err(NodeEditError::NoChange);
                }
                if p.segs.is_empty() {
                    return Err(NodeEditError::TooFewNodes);
                }
                p.closed = true;
                subs[subpath] = p.build()?;
            }
            NodeEdit::BreakAt { node } => {
                let p = get(node.subpath)?;
                let n = p.segs.len();
                if node.node > n {
                    return Err(NodeEditError::NoSuchNode(node));
                }
                if p.closed {
                    let ring = p.ring();
                    let k = if node.node >= ring.len() { 0 } else { node.node };
                    let (start, segs) = Parts::rotated(p.start, &ring, k);
                    subs[node.subpath] = Parts { start, segs, closed: false }.build()?;
                } else {
                    if node.node == 0 || node.node == n {
                        return Err(NodeEditError::NoChange);
                    }
                    let first = Parts { start: p.start, segs: p.segs[..node.node].to_vec(), closed: false };
                    let second =
                        Parts { start: p.node_point(node.node), segs: p.segs[node.node..].to_vec(), closed: false };
                    subs[node.subpath] = first.build()?;
                    subs.insert(node.subpath + 1, second.build()?);
                }
            }
            NodeEdit::JoinEndpoints { a, b } => {
                let (sa, sb) = (endpoint_subpath(a), endpoint_subpath(b));
                let (pa, pb) = (get(sa)?, get(sb)?);
                for (s, p) in [(sa, &pa), (sb, &pb)] {
                    if p.closed {
                        return Err(NodeEditError::SubpathClosed(s));
                    }
                }
                if sa == sb {
                    if a == b {
                        return Err(NodeEditError::NoChange);
                    }
                    let mut p = pa;
                    p.closed = true;
                    subs[sa] = p.build()?;
                } else {
                    // Orient so that `first` ends at the joint and `second` starts there.
                    let first = if matches!(a, EndpointRef::Start(_)) { pa.reversed() } else { pa };
                    let second = if matches!(b, EndpointRef::End(_)) { pb.reversed() } else { pb };
                    let mut segs = first.segs.clone();
                    if first.end() != second.start {
                        segs.push(Segment::Line { to: second.start });
                    }
                    segs.extend_from_slice(&second.segs);
                    subs[sa] = Parts { start: first.start, segs, closed: false }.build()?;
                    subs.remove(sb);
                }
            }
            NodeEdit::ReverseSubpath { subpath } => {
                subs[subpath] = get(subpath)?.reversed().build()?;
            }
            NodeEdit::ConvertToCubic { subpath } => {
                let mut p = get(subpath)?;
                if !p.segs.iter().any(|s| matches!(s, Segment::Arc(_))) {
                    return Err(NodeEditError::NoChange);
                }
                p.segs = p
                    .segs
                    .iter()
                    .flat_map(|s| match s {
                        Segment::Arc(arc) => arc.to_cubics(),
                        other => vec![*other],
                    })
                    .collect();
                subs[subpath] = p.build()?;
            }
        }
        Ok(Path::from_subpaths(subs))
    }
}

fn endpoint_subpath(e: EndpointRef) -> usize {
    match e {
        EndpointRef::Start(s) | EndpointRef::End(s) => s,
    }
}

fn handle_point(seg: &Segment, side: HandleSide) -> Option<Point> {
    match (*seg, side) {
        (Segment::Cubic { c1, .. }, HandleSide::Out) => Some(c1),
        (Segment::Cubic { c2, .. }, HandleSide::In) => Some(c2),
        _ => None,
    }
}

fn set_handle(seg: &mut Segment, side: HandleSide, to: Point) -> Option<()> {
    match (seg, side) {
        (Segment::Cubic { c1, .. }, HandleSide::Out) => *c1 = to,
        (Segment::Cubic { c2, .. }, HandleSide::In) => *c2 = to,
        _ => return None,
    }
    Some(())
}

fn move_node(p: &mut Parts, subpath: usize, i: usize, to: Point, delta: crate::Vector) -> Result<(), NodeEditError> {
    let n = p.segs.len();
    if i > 0 {
        match &mut p.segs[i - 1] {
            Segment::Line { to: end } => *end = to,
            Segment::Cubic { c2, to: end, .. } => {
                *c2 = *c2 + delta;
                *end = to;
            }
            Segment::Arc(_) => return Err(NodeEditError::ArcSegment { subpath, segment: i - 1 }),
        }
    } else {
        p.start = to;
    }
    if i < n {
        match &mut p.segs[i] {
            Segment::Line { .. } => {}
            Segment::Cubic { c1, .. } => *c1 = *c1 + delta,
            Segment::Arc(_) => return Err(NodeEditError::ArcSegment { subpath, segment: i }),
        }
    }
    Ok(())
}

fn delete_node(p: Parts, i: usize) -> Result<Parts, NodeEditError> {
    let n = p.segs.len();
    if p.closed {
        let ring = p.ring();
        if ring.len() <= 2 {
            return Err(NodeEditError::TooFewNodes);
        }
        let k = if i >= ring.len() { 0 } else { i };
        // Rotate so the node is the end of ring[0], then merge ring[0] and ring[1].
        let r = (k + ring.len() - 1) % ring.len();
        let (start, mut segs) = Parts::rotated(p.start, &ring, r);
        let merged = merge(segs[0], segs[1]);
        segs.splice(0..2, [merged]);
        return Ok(Parts::from_ring(start, segs));
    }
    if n <= 1 {
        return Err(NodeEditError::TooFewNodes);
    }
    let mut p = p;
    if i == 0 {
        p.start = p.segs[0].end();
        p.segs.remove(0);
    } else if i == n {
        p.segs.pop();
    } else {
        let merged = merge(p.segs[i - 1], p.segs[i]);
        p.segs.splice(i - 1..=i, [merged]);
    }
    Ok(p)
}
