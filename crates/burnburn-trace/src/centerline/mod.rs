//! Centerline tracing (IM23): a topology-preserving skeleton (Zhang–Suen thinning) of the binary
//! mask becomes a graph of ends, junctions and chains. Short side branches are pruned by a length
//! rule relative to the local stroke width, junction clusters are merged, chains are fitted within
//! a distance budget and get line/arc/circle candidates with residuals. Which branches continue
//! through a junction is never decided automatically: junctions stay `Ambiguous` with ranked
//! candidate pairings until the operator resolves them. Primitive candidates replace free geometry
//! only after an explicit confirmation.
//!
//! Algorithms after their published descriptions: Zhang & Suen (CACM 1984) thinning, Felzenszwalb &
//! Huttenlocher (2012) 1-D squared distance transform, Kåsa (1976) algebraic circle fit refined by
//! Gauss–Newton. No tracer source code was used.

mod graph;
mod primitive;
mod skeleton;

use std::sync::Arc;

use burnburn_geometry::{Point, Segment, Subpath};
use burnburn_image::{ImageRecipe, PixelScale, SourceImage};
use burnburn_types::{
    CancellationToken, Cancelled, CanonicalEncode, CanonicalWriter, ContentHash, HashDomain, NonFiniteValue,
    RevisionId, TaskId,
};

use crate::error::{StaleTrace, TraceError};
use crate::fit::{self, FitParams};

pub const CENTERLINE_ENGINE_VERSION: &str = "burnburn-trace/centerline-1";

/// Centerline parameters. Lengths in millimetres unless named `_px`; factors are relative to the
/// local stroke width.
#[derive(Debug, Clone, PartialEq)]
pub struct CenterlineSettings {
    /// A side branch (end ↔ junction) shorter than `max(min_spur_mm, spur_width_factor · width)` is
    /// pruned and reported.
    pub min_spur_mm: f64,
    pub spur_width_factor: f64,
    /// Spurs passing within one pixel of one of these points (adjusted pixels) are never pruned.
    pub keep_spurs_near_px: Vec<[f64; 2]>,
    /// Two junctions joined by a chain shorter than `junction_merge_factor · width` are one node.
    pub junction_merge_factor: f64,
    /// Distance budget of the free chain geometry.
    pub tolerance_mm: f64,
    /// A primitive candidate is offered for replacement only when its max residual is below this.
    pub primitive_tolerance_mm: f64,
    /// Different components closer than `near_contact_factor · width` (gap between stroke edges)
    /// are reported as near contacts.
    pub near_contact_factor: f64,
    /// Endpoints farther apart than `max_join_factor · width` cannot be joined by a repair chain.
    pub max_join_factor: f64,
}

impl Default for CenterlineSettings {
    fn default() -> Self {
        CenterlineSettings {
            min_spur_mm: 0.0,
            spur_width_factor: 1.0,
            keep_spurs_near_px: Vec::new(),
            junction_merge_factor: 1.0,
            tolerance_mm: 0.08,
            primitive_tolerance_mm: 0.12,
            near_contact_factor: 0.75,
            max_join_factor: 6.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CenterlineRequest {
    pub task: TaskId,
    pub source_revision: RevisionId,
    pub source: Arc<SourceImage>,
    pub image: ImageRecipe,
    pub settings: CenterlineSettings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChainId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeKind {
    /// Degree 1 (or 0 for an isolated dot).
    End,
    /// Degree 2 after an explicit join.
    Joint,
    /// Degree ≥ 3.
    Junction,
}

/// One end of a chain: `at_start` = the chain's `from` end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChainEnd {
    pub chain: ChainId,
    pub at_start: bool,
}

/// A candidate continuation through a junction: the paired branch ends continue each other,
/// unpaired branches end at the junction. `cost_deg` sums each pair's deviation from straight.
#[derive(Debug, Clone, PartialEq)]
pub struct Pairing {
    pub pairs: Vec<(ChainEnd, ChainEnd)>,
    pub cost_deg: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Connections {
    /// End node: nothing to decide.
    None,
    /// Junction whose continuation the tracer does not decide; candidates sorted by cost.
    Ambiguous { candidates: Vec<Pairing> },
    /// Chosen by the operator (or by an explicit join).
    Resolved { pairs: Vec<(ChainEnd, ChainEnd)> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphNode {
    pub id: NodeId,
    pub kind: NodeKind,
    /// Millimetres (y-up), same frame as contour traces.
    pub at: Point,
    pub ends: Vec<ChainEnd>,
    /// Local stroke width at the node (mm).
    pub width_mm: f64,
    pub connections: Connections,
}

impl GraphNode {
    pub fn degree(&self) -> usize {
        self.ends.len()
    }
}

/// Whether a chain was seen in the image or added by an approved repair.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChainOrigin {
    Observed,
    Repair,
}

/// Exact technical primitives (mm).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Primitive {
    Line {
        from: Point,
        to: Point,
    },
    /// Counter-clockwise for positive `sweep_rad`.
    Arc {
        center: Point,
        radius: f64,
        start_rad: f64,
        sweep_rad: f64,
    },
    Circle {
        center: Point,
        radius: f64,
    },
}

/// A primitive fitted to a chain's skeleton points, with its residuals.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrimitiveCandidate {
    pub primitive: Primitive,
    pub max_residual_mm: f64,
    pub rms_residual_mm: f64,
    /// `max_residual_mm <= settings.primitive_tolerance_mm`.
    pub within_tolerance: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chain {
    pub id: ChainId,
    /// `None` for a closed loop without nodes.
    pub from: Option<NodeId>,
    pub to: Option<NodeId>,
    pub closed: bool,
    /// Skeleton points (pixel centres) in mm, node positions at the ends.
    pub points: Vec<Point>,
    /// Median local stroke width (mm); metadata, never a reason to output the path twice.
    pub width_mm: f64,
    /// Free fitted geometry, or the confirmed primitive.
    pub path: Subpath,
    pub origin: ChainOrigin,
    /// Sorted by max residual.
    pub candidates: Vec<PrimitiveCandidate>,
    pub confirmed: Option<Primitive>,
}

/// A pruned side branch, reported (never silent).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RemovedSpur {
    pub tip: Point,
    pub base: Point,
    pub length_mm: f64,
}

/// Two components whose strokes nearly touch: they stay separate, but the place is shown.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NearContact {
    pub components: (usize, usize),
    pub at: Point,
    pub gap_mm: f64,
}

/// One drawable stroke: chains walked through resolved connections only.
#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    pub chains: Vec<(ChainId, bool)>,
    pub path: Subpath,
}

/// Errors of operator edits on a centerline result.
#[derive(Debug, Clone, PartialEq)]
pub enum CenterlineEditError {
    UnknownNode(NodeId),
    UnknownChain(ChainId),
    NotAJunction(NodeId),
    NotAnEnd(NodeId),
    InvalidPairing,
    TooFar {
        distance_mm: f64,
        max_mm: f64,
    },
    NoCandidate(ChainId),
    /// The proposal was made for another state of this result.
    Stale,
    NonFinite,
}

/// A replacement offered to the operator; applying it needs [`CenterlineResult::confirm_replacement`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReplacementProposal {
    pub inputs: ContentHash,
    pub revision: u64,
    pub chain: ChainId,
    pub candidate: PrimitiveCandidate,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CenterlineResult {
    pub task: TaskId,
    pub source_revision: RevisionId,
    pub inputs: ContentHash,
    /// Incremented by every operator edit; proposals carry it.
    pub revision: u64,
    pub engine: &'static str,
    pub scale: PixelScale,
    pub width_px: u32,
    pub height_px: u32,
    /// Foreground components of the mask (8-connected).
    pub components: usize,
    pub nodes: Vec<GraphNode>,
    pub chains: Vec<Chain>,
    pub spurs_removed: Vec<RemovedSpur>,
    pub near_contacts: Vec<NearContact>,
    pub(crate) settings: CenterlineSettings,
}

impl CenterlineRequest {
    /// Inputs key: engine, source pixels, image recipe and every centerline setting (DR-14/17).
    pub fn key(&self) -> Result<ContentHash, NonFiniteValue> {
        let mut w = CanonicalWriter::new();
        w.str(CENTERLINE_ENGINE_VERSION);
        w.bytes(self.source.content_hash().as_bytes());
        self.image.encode(&mut w);
        let s = &self.settings;
        w.tag("centerline-settings-v1");
        for v in [
            s.min_spur_mm,
            s.spur_width_factor,
            s.junction_merge_factor,
            s.tolerance_mm,
            s.primitive_tolerance_mm,
            s.near_contact_factor,
            s.max_join_factor,
        ] {
            w.f64(v);
        }
        w.u64(s.keep_spurs_near_px.len() as u64);
        for k in &s.keep_spurs_near_px {
            w.f64(k[0]);
            w.f64(k[1]);
        }
        Ok(ContentHash::of(HashDomain::TraceKey, &w.finish()?))
    }
}

impl CenterlineSettings {
    pub fn validate(&self) -> Result<(), TraceError> {
        let non_negative = |v: f64| v.is_finite() && v >= 0.0;
        let positive = |v: f64| v.is_finite() && v > 0.0;
        if !non_negative(self.min_spur_mm) {
            return Err(TraceError::InvalidRecipe("centerline.min_spur_mm"));
        }
        if !non_negative(self.spur_width_factor) {
            return Err(TraceError::InvalidRecipe("centerline.spur_width_factor"));
        }
        if !non_negative(self.junction_merge_factor) {
            return Err(TraceError::InvalidRecipe("centerline.junction_merge_factor"));
        }
        if !positive(self.tolerance_mm) {
            return Err(TraceError::InvalidRecipe("centerline.tolerance_mm"));
        }
        if !positive(self.primitive_tolerance_mm) {
            return Err(TraceError::InvalidRecipe("centerline.primitive_tolerance_mm"));
        }
        if !non_negative(self.near_contact_factor) || !non_negative(self.max_join_factor) {
            return Err(TraceError::InvalidRecipe("centerline.factors"));
        }
        if self.keep_spurs_near_px.iter().any(|k| !(k[0].is_finite() && k[1].is_finite())) {
            return Err(TraceError::InvalidRecipe("centerline.keep_spurs_near_px"));
        }
        Ok(())
    }
}

impl CenterlineResult {
    /// The settings this result was computed with.
    pub fn settings(&self) -> &CenterlineSettings {
        &self.settings
    }

    pub fn node(&self, id: NodeId) -> Option<&GraphNode> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn chain(&self, id: ChainId) -> Option<&Chain> {
        self.chains.iter().find(|c| c.id == id)
    }

    /// Same rule as [`crate::TraceResult::check_fresh`]: latest task and unchanged inputs.
    pub fn check_fresh(&self, latest_task: TaskId, current_inputs: ContentHash) -> Result<(), StaleTrace> {
        if self.task != latest_task {
            return Err(StaleTrace::Superseded { result: self.task, latest: latest_task });
        }
        if self.inputs != current_inputs {
            return Err(StaleTrace::InputsChanged { traced: self.inputs, current: current_inputs });
        }
        Ok(())
    }

    fn continuation(&self, end: ChainEnd) -> Option<ChainEnd> {
        let chain = self.chain(end.chain)?;
        let node = self.node(if end.at_start { chain.from? } else { chain.to? })?;
        let Connections::Resolved { pairs } = &node.connections else { return None };
        pairs.iter().find_map(|&(a, b)| {
            if a == end {
                Some(b)
            } else if b == end {
                Some(a)
            } else {
                None
            }
        })
    }

    /// Drawable strokes: chains are walked through *resolved* connections only; an ambiguous
    /// junction ends every stroke that reaches it, so no continuity is invented.
    pub fn strokes(&self) -> Vec<Stroke> {
        let mut used = vec![false; self.chains.len()];
        let index = |id: ChainId| self.chains.iter().position(|c| c.id == id);
        let mut out = Vec::new();
        let walk = |start: usize, entry_at_start: bool, used: &mut Vec<bool>| -> Option<Stroke> {
            let mut seq = Vec::new();
            let (mut ci, mut entry) = (start, entry_at_start);
            loop {
                if used[ci] {
                    break;
                }
                used[ci] = true;
                seq.push((self.chains[ci].id, !entry));
                let exit = ChainEnd { chain: self.chains[ci].id, at_start: !entry };
                match self.continuation(exit).and_then(|next| Some((index(next.chain)?, next.at_start))) {
                    Some((nci, nentry)) if !used[nci] => {
                        ci = nci;
                        entry = nentry;
                    }
                    _ => break,
                }
            }
            stroke_path(self, &seq).map(|path| Stroke { chains: seq, path })
        };
        for (ci, c) in self.chains.iter().enumerate() {
            if used[ci] {
                continue;
            }
            if c.closed && c.from.is_none() {
                used[ci] = true;
                out.push(Stroke { chains: vec![(c.id, false)], path: c.path.clone() });
                continue;
            }
            let start_open = self.continuation(ChainEnd { chain: c.id, at_start: true }).is_none();
            let end_open = self.continuation(ChainEnd { chain: c.id, at_start: false }).is_none();
            if start_open {
                out.extend(walk(ci, true, &mut used));
            } else if end_open {
                out.extend(walk(ci, false, &mut used));
            }
        }
        // Whatever is left runs in cycles through resolved nodes.
        for ci in 0..self.chains.len() {
            if !used[ci] {
                out.extend(walk(ci, true, &mut used));
            }
        }
        out
    }

    fn edited(&self) -> CenterlineResult {
        let mut next = self.clone();
        next.revision += 1;
        next
    }

    /// Operator decision at a junction: which branch ends continue each other (any subset of
    /// disjoint pairs; unpaired branches end there).
    pub fn resolve_junction(
        &self,
        node: NodeId,
        pairs: Vec<(ChainEnd, ChainEnd)>,
    ) -> Result<CenterlineResult, CenterlineEditError> {
        let n = self.node(node).ok_or(CenterlineEditError::UnknownNode(node))?;
        if n.kind != NodeKind::Junction {
            return Err(CenterlineEditError::NotAJunction(node));
        }
        let mut seen = std::collections::BTreeSet::new();
        for &(a, b) in &pairs {
            if a == b || !n.ends.contains(&a) || !n.ends.contains(&b) || !seen.insert(a) || !seen.insert(b) {
                return Err(CenterlineEditError::InvalidPairing);
            }
        }
        let mut next = self.edited();
        if let Some(m) = next.nodes.iter_mut().find(|m| m.id == node) {
            m.connections = Connections::Resolved { pairs };
        }
        Ok(next)
    }

    /// Explicitly approved repair: a straight chain between two stroke ends, marked
    /// [`ChainOrigin::Repair`] so it stays distinguishable from observed lines.
    pub fn join_endpoints(&self, a: NodeId, b: NodeId) -> Result<CenterlineResult, CenterlineEditError> {
        let na = self.node(a).ok_or(CenterlineEditError::UnknownNode(a))?;
        let nb = self.node(b).ok_or(CenterlineEditError::UnknownNode(b))?;
        for n in [na, nb] {
            if n.kind != NodeKind::End || n.degree() != 1 {
                return Err(CenterlineEditError::NotAnEnd(n.id));
            }
        }
        if a == b {
            return Err(CenterlineEditError::InvalidPairing);
        }
        let distance_mm = na.at.distance(nb.at);
        let max_mm = self.settings.max_join_factor * na.width_mm.max(nb.width_mm);
        if distance_mm > max_mm {
            return Err(CenterlineEditError::TooFar { distance_mm, max_mm });
        }
        let path = Subpath::new(na.at, vec![Segment::Line { to: nb.at }], false)
            .map_err(|_| CenterlineEditError::NonFinite)?;
        let id = ChainId(self.chains.iter().map(|c| c.id.0 + 1).max().unwrap_or(0));
        let (ea, eb) = (na.ends[0], nb.ends[0]);
        let mut next = self.edited();
        next.chains.push(Chain {
            id,
            from: Some(a),
            to: Some(b),
            closed: false,
            points: vec![na.at, nb.at],
            width_mm: 0.5 * (na.width_mm + nb.width_mm),
            path,
            origin: ChainOrigin::Repair,
            candidates: Vec::new(),
            confirmed: None,
        });
        for (node, own, repair) in
            [(a, ea, ChainEnd { chain: id, at_start: true }), (b, eb, ChainEnd { chain: id, at_start: false })]
        {
            if let Some(m) = next.nodes.iter_mut().find(|m| m.id == node) {
                m.ends.push(repair);
                m.kind = NodeKind::Joint;
                m.connections = Connections::Resolved { pairs: vec![(own, repair)] };
            }
        }
        Ok(next)
    }

    /// The best candidate within the primitive tolerance (line before circle before arc).
    pub fn propose_replacement(&self, chain: ChainId) -> Result<ReplacementProposal, CenterlineEditError> {
        let c = self.chain(chain).ok_or(CenterlineEditError::UnknownChain(chain))?;
        let rank = |p: &Primitive| match p {
            Primitive::Line { .. } => 0,
            Primitive::Circle { .. } => 1,
            Primitive::Arc { .. } => 2,
        };
        let best = c
            .candidates
            .iter()
            .filter(|k| k.within_tolerance)
            .min_by(|x, y| {
                rank(&x.primitive).cmp(&rank(&y.primitive)).then(x.max_residual_mm.total_cmp(&y.max_residual_mm))
            })
            .ok_or(CenterlineEditError::NoCandidate(chain))?;
        Ok(ReplacementProposal { inputs: self.inputs, revision: self.revision, chain, candidate: *best })
    }

    /// Applies a proposal the operator confirmed; refused when this result changed since.
    pub fn confirm_replacement(&self, proposal: &ReplacementProposal) -> Result<CenterlineResult, CenterlineEditError> {
        if proposal.inputs != self.inputs || proposal.revision != self.revision {
            return Err(CenterlineEditError::Stale);
        }
        let c = self.chain(proposal.chain).ok_or(CenterlineEditError::UnknownChain(proposal.chain))?;
        if !c.candidates.contains(&proposal.candidate) {
            return Err(CenterlineEditError::Stale);
        }
        let path = primitive::path_of(&proposal.candidate.primitive).ok_or(CenterlineEditError::NonFinite)?;
        let mut next = self.edited();
        if let Some(c) = next.chains.iter_mut().find(|c| c.id == proposal.chain) {
            c.confirmed = Some(proposal.candidate.primitive);
            c.path = path;
        }
        Ok(next)
    }
}

/// Concatenates chain paths in walking order (`true` = traversed backwards), bridging any gap
/// left by a confirmed primitive with a line.
fn stroke_path(r: &CenterlineResult, seq: &[(ChainId, bool)]) -> Option<Subpath> {
    let mut start: Option<Point> = None;
    let mut segments: Vec<Segment> = Vec::new();
    let mut at = Point::ORIGIN;
    for &(id, backwards) in seq {
        let c = r.chain(id)?;
        let (s, segs) = if backwards { reversed(&c.path) } else { (c.path.start(), c.path.segments().to_vec()) };
        match start {
            None => start = Some(s),
            Some(_) if at.distance(s) > 1e-9 => segments.push(Segment::Line { to: s }),
            Some(_) => {}
        }
        segments.extend(segs);
        at = segments.last().map_or(s, |g| g.end());
    }
    let start = start?;
    let closed = seq.len() > 1 && at.distance(start) <= 1e-9;
    Subpath::new(start, segments, closed).ok()
}

fn reversed(path: &Subpath) -> (Point, Vec<Segment>) {
    let segs = path.segments();
    let mut froms = Vec::with_capacity(segs.len());
    let mut from = path.start();
    for s in segs {
        froms.push(from);
        from = s.end();
    }
    let out = segs.iter().zip(&froms).rev().map(|(s, &f)| s.reversed(f)).collect();
    (path.end(), out)
}

pub fn trace_centerline(req: &CenterlineRequest, cancel: &CancellationToken) -> Result<CenterlineResult, TraceError> {
    req.settings.validate()?;
    cancel.check()?;
    let inputs = req.key().map_err(|_| TraceError::NonFinite)?;
    let prepared = burnburn_image::prepare(&req.source, &req.image, cancel)?;
    let (sx, sy) = (prepared.scale.mm_per_pixel_x(), prepared.scale.mm_per_pixel_y());
    let mm_per_px = 0.5 * (sx + sy);
    let height = f64::from(prepared.height());
    let to_mm = |p: graph::P| Point::new(p.x * sx, (height - p.y) * sy);

    let mask = skeleton::Raster::from_mask(&prepared.mask);
    let (labels, components) = skeleton::components(&mask);
    let d2 = skeleton::distance_sq(&mask, cancel)?;
    let thin = skeleton::thin(&prepared.mask, cancel)?;
    let mut g = graph::build(&thin, &d2, cancel)?;
    let s = &req.settings;
    let params = graph::GraphParams {
        min_spur_px: s.min_spur_mm / mm_per_px,
        spur_width_factor: s.spur_width_factor,
        keep_spurs_near: s.keep_spurs_near_px.iter().map(|k| graph::P { x: k[0], y: k[1] }).collect(),
        junction_merge_factor: s.junction_merge_factor,
    };
    for _ in 0..8 {
        cancel.check()?;
        let before = (g.chains.iter().filter(|c| c.alive).count(), g.nodes.iter().filter(|n| n.alive).count());
        g.prune_spurs(&params);
        g.refine_junctions();
        g.merge_junctions(&params);
        let after = (g.chains.iter().filter(|c| c.alive).count(), g.nodes.iter().filter(|n| n.alive).count());
        if before == after {
            break;
        }
    }
    g.refine_junctions();
    cancel.check()?;

    // Renumber the live graph.
    let mut node_id = vec![None; g.nodes.len()];
    let mut nodes: Vec<GraphNode> = Vec::new();
    for (i, n) in g.nodes.iter().enumerate() {
        if n.alive && (g.degree(i) > 0 || !g.chains.iter().any(|c| c.alive)) {
            node_id[i] = Some(NodeId(nodes.len() as u32));
            nodes.push(GraphNode {
                id: NodeId(nodes.len() as u32),
                kind: NodeKind::End,
                at: to_mm(n.at),
                ends: Vec::new(),
                width_mm: n.width * mm_per_px,
                connections: Connections::None,
            });
        }
    }
    let fit = FitParams { tol: s.tolerance_mm, corner_cos: -1.0, window: 2.0 * mm_per_px, curves: true };
    let mut chains: Vec<Chain> = Vec::new();
    let mut graph_chain: Vec<usize> = Vec::new();
    for (gi, c) in g.chains.iter().enumerate() {
        if !c.alive || c.pts.len() < 2 {
            continue;
        }
        cancel.check()?;
        let id = ChainId(chains.len() as u32);
        let from = c.from.and_then(|n| node_id[n]);
        let to = c.to.and_then(|n| node_id[n]);
        let closed = c.from.is_none();
        let points: Vec<Point> = c.pts.iter().map(|&p| to_mm(p)).collect();
        if !points.iter().all(|p| p.is_finite()) {
            return Err(TraceError::NonFinite);
        }
        let path = free_path(&points, closed, &fit).ok_or(TraceError::NonFinite)?;
        let width_mm = c.median_width() * mm_per_px;
        let candidates = primitive::candidates(&points, closed, s.primitive_tolerance_mm);
        if let Some(f) = from {
            nodes[f.0 as usize].ends.push(ChainEnd { chain: id, at_start: true });
        }
        if let Some(t) = to {
            nodes[t.0 as usize].ends.push(ChainEnd { chain: id, at_start: false });
        }
        chains.push(Chain {
            id,
            from,
            to,
            closed: closed || (from.is_some() && from == to),
            points,
            width_mm,
            path,
            origin: ChainOrigin::Observed,
            candidates,
            confirmed: None,
        });
        graph_chain.push(gi);
    }
    for n in &mut nodes {
        n.kind = if n.ends.len() >= 3 { NodeKind::Junction } else { NodeKind::End };
    }
    // Candidate continuations at every junction.
    for n in &mut nodes {
        if n.kind != NodeKind::Junction {
            continue;
        }
        let dirs: Vec<Option<(f64, f64)>> = n
            .ends
            .iter()
            .map(|e| g.branch_direction(graph_chain[e.chain.0 as usize], e.at_start).map(|(_, u)| u))
            .collect();
        n.connections = Connections::Ambiguous { candidates: pairings(&n.ends, &dirs) };
    }
    let near_contacts = near_contacts(&mask, &thin, &labels, &d2, s.near_contact_factor, cancel)?
        .into_iter()
        .map(|(a, b, at, gap)| NearContact { components: (a, b), at: to_mm(at), gap_mm: gap * mm_per_px })
        .collect();
    let spurs_removed = g
        .spurs
        .iter()
        .map(|sp| RemovedSpur { tip: to_mm(sp.tip), base: to_mm(sp.base), length_mm: sp.length * mm_per_px })
        .collect();
    Ok(CenterlineResult {
        task: req.task,
        source_revision: req.source_revision,
        inputs,
        revision: 0,
        engine: CENTERLINE_ENGINE_VERSION,
        scale: prepared.scale,
        width_px: prepared.width(),
        height_px: prepared.height(),
        components,
        nodes,
        chains,
        spurs_removed,
        near_contacts,
        settings: req.settings.clone(),
    })
}

/// Free chain geometry: light smoothing of the pixel staircase (ends fixed), then the open fit.
fn free_path(points: &[Point], closed: bool, fit: &FitParams) -> Option<Subpath> {
    let mut sm = points.to_vec();
    for _ in 0..2 {
        let prev = sm.clone();
        for i in 1..prev.len().saturating_sub(1) {
            sm[i] = Point::new(
                0.25 * prev[i - 1].x + 0.5 * prev[i].x + 0.25 * prev[i + 1].x,
                0.25 * prev[i - 1].y + 0.5 * prev[i].y + 0.25 * prev[i + 1].y,
            );
        }
    }
    let data: Vec<(Point, usize)> = sm.iter().copied().zip(0..).collect();
    let mut out = fit::Fitted { start: sm[0], segments: Vec::new(), spans: Vec::new() };
    fit::fit_open_indexed(&data, &[], fit, &mut out);
    if out.segments.is_empty() {
        out.segments.push(Segment::Line { to: sm[sm.len() - 1] });
    }
    Subpath::new(out.start, out.segments, closed).ok()
}

/// Maximal sets of disjoint pairs of branch ends, cheapest (straightest) first. Enumerated up to
/// degree 6; above that only the greedy pairing is offered.
fn pairings(ends: &[ChainEnd], dirs: &[Option<(f64, f64)>]) -> Vec<Pairing> {
    let cost = |i: usize, j: usize| -> f64 {
        match (dirs[i], dirs[j]) {
            (Some(a), Some(b)) => {
                let dot = (a.0 * b.0 + a.1 * b.1).clamp(-1.0, 1.0);
                // Deviation from a straight continuation (opposite directions).
                180.0 - dot.acos().to_degrees()
            }
            _ => 180.0,
        }
    };
    let n = ends.len();
    let mut out: Vec<Pairing> = Vec::new();
    if n <= 6 {
        let mut stack: Vec<(Vec<bool>, Vec<(usize, usize)>)> = vec![(vec![false; n], Vec::new())];
        while let Some((used, pairs)) = stack.pop() {
            let free: Vec<usize> = (0..n).filter(|&i| !used[i]).collect();
            if free.len() < 2 {
                let c = pairs.iter().map(|&(i, j)| cost(i, j)).sum();
                out.push(Pairing { pairs: pairs.iter().map(|&(i, j)| (ends[i], ends[j])).collect(), cost_deg: c });
                continue;
            }
            // Odd counts: the first free end may also stay unpaired (once).
            let first = free[0];
            let unpaired_allowed = free.len() % 2 == 1;
            for &j in &free[1..] {
                let mut u = used.clone();
                u[first] = true;
                u[j] = true;
                let mut p = pairs.clone();
                p.push((first, j));
                stack.push((u, p));
            }
            if unpaired_allowed {
                let mut u = used.clone();
                u[first] = true;
                stack.push((u, pairs.clone()));
            }
        }
    } else {
        let mut used = vec![false; n];
        let mut pairs = Vec::new();
        let mut total = 0.0;
        loop {
            let best = (0..n)
                .flat_map(|i| (i + 1..n).map(move |j| (i, j)))
                .filter(|&(i, j)| !used[i] && !used[j])
                .min_by(|&(a, b), &(c, d)| cost(a, b).total_cmp(&cost(c, d)));
            let Some((i, j)) = best else { break };
            used[i] = true;
            used[j] = true;
            total += cost(i, j);
            pairs.push((ends[i], ends[j]));
        }
        out.push(Pairing { pairs, cost_deg: total });
    }
    out.sort_by(|a, b| a.cost_deg.total_cmp(&b.cost_deg).then_with(|| a.pairs.cmp(&b.pairs)));
    out.dedup();
    out
}

/// For every pair of components: the closest pair of boundary pixels of the mask; reported when
/// the gap between the stroke edges is below `factor · width` (width = median skeleton width of
/// the wider component). Returns (a, b, midpoint px, gap px).
fn near_contacts(
    mask: &skeleton::Raster,
    thin: &skeleton::Raster,
    labels: &[u32],
    d2: &[f64],
    factor: f64,
    cancel: &CancellationToken,
) -> Result<Vec<(usize, usize, graph::P, f64)>, Cancelled> {
    let w = mask.w;
    let mut boundary: std::collections::BTreeMap<u32, Vec<usize>> = std::collections::BTreeMap::new();
    let mut widths: std::collections::BTreeMap<u32, Vec<f64>> = std::collections::BTreeMap::new();
    for (i, &label) in labels.iter().enumerate() {
        if label == u32::MAX {
            continue;
        }
        let (x, y) = ((i % w) as i64, (i / w) as i64);
        if [(0, -1), (1, 0), (0, 1), (-1, 0)].iter().any(|&(dx, dy)| !mask.get(x + dx, y + dy)) {
            boundary.entry(label).or_default().push(i);
        }
        if thin.on[i] {
            widths.entry(label).or_default().push((2.0 * d2[i].sqrt() - 1.0).max(1.0));
        }
    }
    let width_of = |label: u32| -> f64 {
        let mut v = widths.get(&label).cloned().unwrap_or_default();
        v.sort_by(f64::total_cmp);
        v.get(v.len() / 2).copied().unwrap_or(1.0)
    };
    let comps: Vec<(u32, Vec<usize>)> = boundary.into_iter().collect();
    let bbox = |px: &[usize]| {
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        for &i in px {
            let (x, y) = (i % w, i / w);
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
        (x0 as f64, y0 as f64, x1 as f64, y1 as f64)
    };
    let mut out = Vec::new();
    for a in 0..comps.len() {
        cancel.check()?;
        let ba = bbox(&comps[a].1);
        for b in a + 1..comps.len() {
            let width = width_of(comps[a].0).max(width_of(comps[b].0));
            let reach = factor * width + 1.0;
            let bb = bbox(&comps[b].1);
            if ba.0 - reach > bb.2 || bb.0 - reach > ba.2 || ba.1 - reach > bb.3 || bb.1 - reach > ba.3 {
                continue;
            }
            let mut best: Option<(f64, usize, usize)> = None;
            for &i in &comps[a].1 {
                for &j in &comps[b].1 {
                    let (dx, dy) = ((i % w) as f64 - (j % w) as f64, (i / w) as f64 - (j / w) as f64);
                    // Pixel centres one apart touch: the edge gap is the centre distance minus one.
                    let gap = dx.hypot(dy) - 1.0;
                    if best.is_none_or(|(g, _, _)| gap < g) {
                        best = Some((gap, i, j));
                    }
                }
            }
            if let Some((gap, i, j)) = best
                && gap < factor * width
            {
                let mid = graph::P {
                    x: 0.5 * ((i % w) as f64 + (j % w) as f64) + 0.5,
                    y: 0.5 * ((i / w) as f64 + (j / w) as f64) + 0.5,
                };
                out.push((a, b, mid, gap.max(0.0)));
            }
        }
    }
    Ok(out)
}
