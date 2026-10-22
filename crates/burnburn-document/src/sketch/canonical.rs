//! Canonical projection of a sketch (ADR 0004): evaluator input hashes and project hashes.

use burnburn_types::{CanonicalEncode, CanonicalWriter};

use super::{Along, DimensionKind, DimensionRole, Entity, Relation, Sketch, SketchConstraint, SketchSolution};

fn along(out: &mut CanonicalWriter, a: Along) {
    out.tag(match a {
        Along::Direct => "direct",
        Along::X => "x",
        Along::Y => "y",
    });
}

fn ids(out: &mut CanonicalWriter, tag: &str, ids: &[u32]) {
    out.tag(tag);
    for i in ids {
        out.u64(u64::from(*i));
    }
}

/// The definition only (no solution).
pub struct Definition<'a>(pub &'a Sketch);

impl CanonicalEncode for Definition<'_> {
    fn encode(&self, out: &mut CanonicalWriter) {
        let s = self.0;
        out.str(s.name());
        out.u64(u64::from(s.next_entity_id().raw()));
        out.u64(u64::from(s.next_constraint_id().raw()));
        out.u64(s.entities().len() as u64);
        for (id, e) in s.entities() {
            out.u64(u64::from(id.raw()));
            out.bool(e.construction);
            match e.entity {
                Entity::Point { at } => {
                    out.tag("point");
                    out.f64(at.x);
                    out.f64(at.y);
                }
                Entity::Line { start, end } => ids(out, "line", &[start.raw(), end.raw()]),
                Entity::Arc { center, start, end, clockwise } => {
                    ids(out, "arc", &[center.raw(), start.raw(), end.raw()]);
                    out.bool(clockwise);
                }
                Entity::Circle { center, radius } => {
                    ids(out, "circle", &[center.raw()]);
                    out.f64(radius);
                }
                Entity::Ellipse { center, major, minor } => {
                    ids(out, "ellipse", &[center.raw(), major.raw()]);
                    out.f64(minor);
                }
            }
        }
        out.u64(s.constraints().len() as u64);
        for (id, c) in s.constraints() {
            out.u64(u64::from(id.raw()));
            match c {
                SketchConstraint::Relation(r) => {
                    let r = *r;
                    let (tag, refs): (&str, Vec<u32>) = match r {
                        Relation::Coincident(a, b) => ("coincident", vec![a.raw(), b.raw()]),
                        Relation::PointOn { point, curve } => ("point_on", vec![point.raw(), curve.raw()]),
                        Relation::Horizontal(l) => ("horizontal", vec![l.raw()]),
                        Relation::Vertical(l) => ("vertical", vec![l.raw()]),
                        Relation::Parallel(a, b) => ("parallel", vec![a.raw(), b.raw()]),
                        Relation::Perpendicular(a, b) => ("perpendicular", vec![a.raw(), b.raw()]),
                        Relation::Tangent(a, b) => ("tangent", vec![a.raw(), b.raw()]),
                        Relation::Equal(a, b) => ("equal", vec![a.raw(), b.raw()]),
                        Relation::Midpoint { point, line } => ("midpoint", vec![point.raw(), line.raw()]),
                        Relation::Symmetric { a, b, about } => ("symmetric", vec![a.raw(), b.raw(), about.raw()]),
                        Relation::Concentric(a, b) => ("concentric", vec![a.raw(), b.raw()]),
                        Relation::Fix { point, .. } => ("fix", vec![point.raw()]),
                    };
                    ids(out, tag, &refs);
                    if let Relation::Fix { at, .. } = r {
                        out.f64(at.x);
                        out.f64(at.y);
                    }
                }
                SketchConstraint::Dimension(d) => {
                    match d.kind {
                        DimensionKind::Distance { a, b, along: al } => {
                            ids(out, "distance", &[a.raw(), b.raw()]);
                            along(out, al);
                        }
                        DimensionKind::Length { line } => ids(out, "length", &[line.raw()]),
                        DimensionKind::Radius { curve } => ids(out, "radius", &[curve.raw()]),
                        DimensionKind::Diameter { curve } => ids(out, "diameter", &[curve.raw()]),
                        DimensionKind::Angle { from, to } => ids(out, "angle", &[from.raw(), to.raw()]),
                    }
                    out.str(&d.text);
                    out.tag(match d.role {
                        DimensionRole::Driving => "driving",
                        DimensionRole::Driven => "driven",
                    });
                }
            }
        }
        out.u64(s.parameters().iter().len() as u64);
        for (name, text) in s.parameters().iter() {
            out.str(name);
            out.str(text);
        }
    }
}

impl CanonicalEncode for SketchSolution {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.u64(self.points.len() as u64);
        for (id, p) in &self.points {
            out.u64(u64::from(id.raw()));
            out.f64(p.x);
            out.f64(p.y);
        }
        out.u64(self.radii.len() as u64);
        for (id, r) in &self.radii {
            out.u64(u64::from(id.raw()));
            out.f64(*r);
        }
        out.u64(u64::from(self.dof));
    }
}

impl CanonicalEncode for Sketch {
    fn encode(&self, out: &mut CanonicalWriter) {
        Definition(self).encode(out);
        match self.solution() {
            None => out.u64(0),
            Some(sol) => {
                out.u64(1);
                sol.encode(out);
            }
        }
    }
}
