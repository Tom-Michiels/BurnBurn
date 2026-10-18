//! Canonical projection of document objects (ADR 0004). Revision and session state are not content.

use burnburn_geometry::ShapeParams;
use burnburn_types::{CanonicalEncode, CanonicalWriter, StableId};

use crate::{GeometryObject, GeometrySource, ObjectKind};

impl CanonicalEncode for GeometryObject {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.u64(self.id().raw());
        out.str(self.name());
        out.tag(match self.kind() {
            ObjectKind::Rectangle => "rectangle",
            ObjectKind::Path => "path",
            ObjectKind::Line => "line",
            ObjectKind::Circle => "circle",
            ObjectKind::Ellipse => "ellipse",
            ObjectKind::Polygon => "polygon",
            ObjectKind::Star => "star",
            ObjectKind::Polyline => "polyline",
        });
        match self.source() {
            GeometrySource::Drawn => out.tag("drawn"),
            GeometrySource::Derived(key) => {
                out.tag("derived");
                match key.owner {
                    crate::DerivedOwner::Sketch(id) => {
                        out.tag("sketch");
                        out.u64(id.raw());
                    }
                    crate::DerivedOwner::Modifier(id) => {
                        out.tag("modifier");
                        out.u64(id.raw());
                    }
                    crate::DerivedOwner::Part(id) => {
                        out.tag("part");
                        out.u64(id.raw());
                    }
                    crate::DerivedOwner::Text(id) => {
                        out.tag("text");
                        out.u64(id.raw());
                    }
                    crate::DerivedOwner::Trace(id) => {
                        out.tag("trace");
                        out.u64(id.raw());
                    }
                    crate::DerivedOwner::Generator(id) => {
                        out.tag("generator");
                        out.u64(id.raw());
                    }
                }
                out.u64(key.local);
            }
            GeometrySource::Imported { asset, entity } => {
                out.tag("imported");
                out.bytes(asset.as_bytes());
                match entity {
                    None => out.u64(0),
                    Some(e) => {
                        out.u64(1);
                        out.str(e);
                    }
                }
            }
        }
        self.path().as_ref().encode(out);
        // Optional trailing field (IM09): absent for ungrouped objects, so existing hashes stay.
        if !self.groups().is_empty() {
            out.tag("groups");
            out.u64(self.groups().len() as u64);
            for g in self.groups() {
                out.u64(g.raw());
            }
        }
        // Optional trailing field (DR-20, C-4 order: groups, sub-refs, edge-keys): only when the
        // keys differ from the default numbering, so existing hashes stay.
        if let Some(keys) = self.stored_edge_keys() {
            out.tag("edge-keys");
            out.u64(u64::from(keys.next()));
            out.u64(keys.subpaths().len() as u64);
            for sub in keys.subpaths() {
                out.u64(sub.len() as u64);
                for k in sub {
                    out.u64(u64::from(k.0));
                }
            }
        }
        // Optional trailing field (W1-L2b, after edge-keys): only for parametric shapes, so the
        // hashes of plain objects stay.
        if let Some(shape) = self.parametric() {
            out.tag("parametric");
            encode_params(shape.params(), out);
            for c in shape.frame().coefficients() {
                out.f64(c);
            }
        }
    }
}

fn encode_params(params: &ShapeParams, out: &mut CanonicalWriter) {
    match *params {
        ShapeParams::Polygon { sides } => {
            out.tag("polygon");
            out.u64(u64::from(sides));
        }
        ShapeParams::Star { points, ratio } => {
            out.tag("star");
            out.u64(u64::from(points));
            out.f64(ratio);
        }
        ShapeParams::DoubleStar { points, ratio } => {
            out.tag("double-star");
            out.u64(u64::from(points));
            out.f64(ratio);
        }
        ShapeParams::RoundedRectangle { width, height, radius } => {
            out.tag("rounded-rectangle");
            out.f64(width);
            out.f64(height);
            out.f64(radius);
        }
        // Parameter kinds are closed by `ShapeParams::validate`; a future kind needs its own tag.
        _ => out.tag("unknown-shape"),
    }
}
