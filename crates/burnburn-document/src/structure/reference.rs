//! Associative references (contract IM21): a reference names a *part* of a geometry by its
//! provenance (which source edge it came from), never by index or proximity. When the origin
//! disappears the reference is `Broken`; when it splits it is `Ambiguous`. Nothing rebinds.

use burnburn_geometry::{Path, Point};

use crate::{EdgeKey, GeometryObject};
use burnburn_types::GeometryId;

use super::ReferenceId;

/// Where an output edge came from: one edge (stable [`EdgeKey`], DR-20) of a source object in
/// array copy `copy` (0 without an array).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EdgeOrigin {
    pub source: GeometryId,
    pub edge: EdgeKey,
    pub copy: u32,
}

/// The name of one topological edge of an object: its origin, plus which of the `of` edges
/// with that origin it was when the name was made (an edge split by a boolean has several).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EdgeName {
    pub origin: EdgeOrigin,
    pub index: u32,
    pub of: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VertexEnd {
    Start,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SubRef {
    Edge(EdgeName),
    /// An end point of a named edge.
    Vertex {
        edge: EdgeName,
        end: VertexEnd,
    },
}

/// An associative reference to an object or a part of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GeometryRef {
    pub object: GeometryId,
    pub sub: Option<SubRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BreakReason {
    /// The referenced object no longer exists.
    ObjectMissing,
    /// The object exists but no edge has the referenced origin any more.
    OriginGone,
}

/// What a bound reference points at now (world mm).
#[derive(Debug, Clone, PartialEq)]
pub enum Resolved {
    Object(GeometryId),
    /// Consecutive segments `segments` (wrapping on a closed subpath) of one subpath.
    Edge {
        object: GeometryId,
        subpath: usize,
        segments: Vec<usize>,
    },
    Vertex {
        object: GeometryId,
        point: Point,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum RefStatus {
    Bound(Resolved),
    /// The origin now yields a different number of edges; the user must choose.
    Ambiguous {
        candidates: Vec<EdgeName>,
    },
    Broken {
        reason: BreakReason,
    },
}

impl RefStatus {
    pub fn is_bound(&self) -> bool {
        matches!(self, RefStatus::Bound(_))
    }
}

/// A named, stored associative reference (S06): shown with its source object and edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssocReference {
    pub name: String,
    pub target: GeometryRef,
}

/// Who holds a reference that broke (for `CommandWarning::ReferenceBroken`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RefHolder {
    Reference(ReferenceId),
    /// A modifier chain's source or boolean operand.
    ModifierSource {
        modifier: super::ModifierId,
        geometry: GeometryId,
    },
}

/// A maximal run of consecutive segments of one output subpath that share one origin
/// (`None`: generated, for example a round offset join). Stored with derived outputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProvenanceEdge {
    pub subpath: u32,
    pub first: u32,
    pub count: u32,
    pub origin: Option<EdgeOrigin>,
    /// A second origin when the edge lies on two coincident input edges: references to either
    /// origin resolve `Ambiguous` here (contract IM21).
    pub also: Option<EdgeOrigin>,
}

/// The origin(s) of one output segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Cand {
    pub origin: EdgeOrigin,
    pub also: Option<EdgeOrigin>,
}

pub(crate) use crate::object::drawn_segments;

/// Per subpath, per drawn segment: the origin.
pub(crate) type SegmentOrigins = Vec<Vec<Option<Cand>>>;

/// Every segment is its own origin, named by the object's stable edge key.
pub(crate) fn identity_origins(obj: &GeometryObject) -> SegmentOrigins {
    let source = obj.id();
    obj.edge_keys()
        .subpaths()
        .iter()
        .map(|sub| {
            sub.iter()
                .map(|edge| Some(Cand { origin: EdgeOrigin { source, edge: *edge, copy: 0 }, also: None }))
                .collect()
        })
        .collect()
}

/// Groups consecutive segments with the same origin into edges; on a closed subpath a run
/// that wraps around the start is one edge.
pub(crate) fn runs(path: &Path, origins: &SegmentOrigins) -> Vec<ProvenanceEdge> {
    let mut out = Vec::new();
    for (si, sp) in path.subpaths().iter().enumerate() {
        let Some(per) = origins.get(si) else { continue };
        let mut sub: Vec<ProvenanceEdge> = Vec::new();
        for (gi, o) in per.iter().enumerate() {
            let (origin, also) = (o.map(|c| c.origin), o.and_then(|c| c.also));
            match sub.last_mut() {
                Some(last) if last.origin == origin && last.also == also => last.count += 1,
                _ => sub.push(ProvenanceEdge { subpath: si as u32, first: gi as u32, count: 1, origin, also }),
            }
        }
        if sp.is_closed() && sub.len() > 1 {
            let (first, last) = (sub[0], sub[sub.len() - 1]);
            if first.origin == last.origin && first.also == last.also {
                sub.pop();
                sub[0] = ProvenanceEdge { first: last.first, count: last.count + first.count, ..first };
            }
        }
        out.extend(sub);
    }
    out
}

/// The edges with `origin`, in a fixed order (subpath, first segment).
pub(crate) fn edges_with(edges: &[ProvenanceEdge], origin: EdgeOrigin) -> Vec<ProvenanceEdge> {
    let mut found: Vec<ProvenanceEdge> =
        edges.iter().filter(|e| e.origin == Some(origin) || e.also == Some(origin)).copied().collect();
    found.sort_by_key(|e| (e.subpath, e.first));
    found
}

/// Resolves `name` against the edges of `object` (whose path is `path`).
pub(crate) fn resolve_edge(object: GeometryId, path: &Path, edges: &[ProvenanceEdge], name: &EdgeName) -> RefStatus {
    let found = edges_with(edges, name.origin);
    if found.is_empty() {
        return RefStatus::Broken { reason: BreakReason::OriginGone };
    }
    if found.len() as u32 != name.of {
        let of = found.len() as u32;
        let candidates = (0..of).map(|index| EdgeName { origin: name.origin, index, of }).collect();
        return RefStatus::Ambiguous { candidates };
    }
    let Some(edge) = found.get(name.index as usize) else {
        return RefStatus::Broken { reason: BreakReason::OriginGone };
    };
    if let (Some(a), Some(b)) = (edge.origin, edge.also) {
        let candidates = [a, b].into_iter().map(|origin| EdgeName { origin, index: 0, of: 1 }).collect();
        return RefStatus::Ambiguous { candidates };
    }
    let n = path.subpaths().get(edge.subpath as usize).map_or(0, |sp| drawn_segments(sp).len());
    let segments = (0..edge.count).map(|k| (edge.first + k) as usize % n.max(1)).collect();
    RefStatus::Bound(Resolved::Edge { object, subpath: edge.subpath as usize, segments })
}

/// The start or end point of a resolved edge.
pub(crate) fn edge_vertex(path: &Path, subpath: usize, segments: &[usize], end: VertexEnd) -> Option<Point> {
    let segs = drawn_segments(path.subpaths().get(subpath)?);
    match end {
        VertexEnd::Start => segs.get(*segments.first()?).map(|(from, _)| *from),
        VertexEnd::End => segs.get(*segments.last()?).map(|(_, s)| s.end()),
    }
}
