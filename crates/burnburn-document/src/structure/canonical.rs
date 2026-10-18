//! Canonical projection of the structure layer (ADR 0004), used by the project hash. Evaluation
//! records and provenance are derived state and not part of the meaning.

use burnburn_types::{CanonicalEncode, CanonicalWriter, StableId};

use super::{GeometryRef, ObjectRelation, StructureLayer, SubRef, VertexEnd};

fn geometry_ref(out: &mut CanonicalWriter, r: &GeometryRef) {
    out.u64(r.object.raw());
    let edge = |out: &mut CanonicalWriter, e: &super::EdgeName| {
        out.u64(e.origin.source.raw());
        out.u64(u64::from(e.origin.edge.0));
        out.u64(u64::from(e.origin.copy));
        out.u64(u64::from(e.index));
        out.u64(u64::from(e.of));
    };
    match &r.sub {
        None => out.tag("object"),
        Some(SubRef::Edge(e)) => {
            out.tag("edge");
            edge(out, e);
        }
        Some(SubRef::Vertex { edge: e, end }) => {
            out.tag(match end {
                VertexEnd::Start => "vertex-start",
                VertexEnd::End => "vertex-end",
            });
            edge(out, e);
        }
    }
}

impl CanonicalEncode for StructureLayer {
    /// `seq`-like counts, then items in id order: modifiers, references, relations,
    /// definitions, instances, plates, placements.
    fn encode(&self, out: &mut CanonicalWriter) {
        out.tag("modifiers");
        out.u64(self.modifiers.len() as u64);
        for (id, c) in &self.modifiers {
            out.u64(id.raw());
            c.encode(out);
        }
        out.tag("references");
        out.u64(self.references.len() as u64);
        for (id, r) in &self.references {
            out.u64(id.raw());
            out.str(&r.name);
            geometry_ref(out, &r.target);
        }
        out.tag("relations");
        out.u64(self.relations.len() as u64);
        for (id, r) in &self.relations {
            out.u64(id.raw());
            match r {
                ObjectRelation::Tangent { a, b } => {
                    out.tag("tangent");
                    out.u64(a.raw());
                    out.u64(b.raw());
                }
                ObjectRelation::Symmetric { a, b, axis } => {
                    out.tag("symmetric");
                    out.u64(a.raw());
                    out.u64(b.raw());
                    out.u64(axis.raw());
                }
            }
        }
        out.tag("definitions");
        out.u64(self.definitions.len() as u64);
        for (id, d) in &self.definitions {
            out.u64(id.raw());
            d.encode(out);
        }
        out.tag("instances");
        out.u64(self.instances.len() as u64);
        for (id, i) in &self.instances {
            out.u64(id.raw());
            i.encode(out);
        }
        out.tag("plates");
        out.u64(self.plates.len() as u64);
        for (id, p) in &self.plates {
            out.u64(id.raw());
            p.encode(out);
        }
        out.tag("placement");
        out.u64(self.placement.len() as u64);
        for (g, p) in &self.placement {
            out.u64(g.raw());
            out.u64(p.raw());
        }
    }
}
