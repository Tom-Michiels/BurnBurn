//! Canonical projection of paths (ADR 0004): explicit tags, coordinates as normalised f64 bits.

use burnburn_types::{CanonicalEncode, CanonicalWriter};

use crate::{Path, Point, Segment, Subpath};

fn point(out: &mut CanonicalWriter, p: Point) {
    out.f64(p.x);
    out.f64(p.y);
}

impl CanonicalEncode for Segment {
    fn encode(&self, out: &mut CanonicalWriter) {
        match *self {
            Segment::Line { to } => {
                out.tag("line");
                point(out, to);
            }
            Segment::Cubic { c1, c2, to } => {
                out.tag("cubic");
                point(out, c1);
                point(out, c2);
                point(out, to);
            }
            Segment::Arc(arc) => {
                out.tag("arc");
                let (u, v) = arc.semi_diameters();
                point(out, arc.center());
                point(out, Point::new(u.x, u.y));
                point(out, Point::new(v.x, v.y));
                out.f64(arc.start_parameter());
                out.f64(arc.sweep_parameter());
            }
        }
    }
}

impl CanonicalEncode for Subpath {
    fn encode(&self, out: &mut CanonicalWriter) {
        point(out, self.start());
        out.bool(self.is_closed());
        out.seq(self.segments().iter());
    }
}

impl CanonicalEncode for Path {
    fn encode(&self, out: &mut CanonicalWriter) {
        out.seq(self.subpaths().iter());
    }
}
