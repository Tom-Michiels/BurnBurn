//! Limited SVG and DXF export (IM10, P05). Output is in millimetres; anything that cannot be
//! written exactly is named in the [`ExportReport`] before the user commits to the file.

use std::fmt;

use std::f64::consts::PI;
use std::fmt::Write as _;

use burnburn_geometry::{Affine, Bounds, EllipticalArc, Path, Point, Segment, Subpath, math};

/// One object to export: its name (SVG `id`, DXF layer) and world geometry.
#[derive(Debug, Clone, Copy)]
pub struct ExportItem<'a> {
    pub name: &'a str,
    pub path: &'a Path,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum ExportLoss {
    /// A non-circular arc was written as cubic spline spans; deviation bound in mm.
    EllipticalArcAsSpline { item: usize, max_deviation_mm: f64 },
    /// A circular arc inside a curved subpath was written as cubic spline spans.
    CircularArcAsSpline { item: usize, max_deviation_mm: f64 },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExportReport {
    pub losses: Vec<ExportLoss>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExportError {
    /// No geometry to write.
    Empty,
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("nothing to export")
    }
}

impl std::error::Error for ExportError {}

/// Radial error bound of [`EllipticalArc::to_cubics`] per unit of the largest semi-axis
/// (quarter-turn pieces, `k = 4/3·tan(Δ/4)`).
const QUARTER_CUBIC_REL_ERROR: f64 = 2.8e-4;

fn bounds_of(items: &[ExportItem<'_>]) -> Result<Bounds, ExportError> {
    items.iter().filter_map(|i| i.path.bounds()).reduce(Bounds::union).ok_or(ExportError::Empty)
}

/// Largest semi-axis of an elliptical arc (largest singular value of `[u v]`).
fn sigma_max(arc: &EllipticalArc) -> f64 {
    let (u, v) = arc.semi_diameters();
    let (p, q, r) = (u.dot(u), u.dot(v), v.dot(v));
    (0.5 * (p + r + ((p - r) * (p - r) + 4.0 * q * q).sqrt())).sqrt()
}

fn is_circular(arc: &EllipticalArc) -> bool {
    let (u, v) = arc.semi_diameters();
    let (lu, lv) = (u.length(), v.length());
    (lu - lv).abs() <= 1e-12 * lu.max(lv) && u.dot(v).abs() <= 1e-12 * lu * lv
}

/// Arc pieces of at most a quarter turn (exact splits of the same ellipse).
fn arc_pieces(arc: &EllipticalArc) -> Vec<EllipticalArc> {
    let n = (arc.sweep_parameter().abs() / (PI / 2.0) - 1e-12).ceil().max(1.0) as u32;
    let mut out = Vec::with_capacity(n as usize);
    let mut rest = *arc;
    for i in 0..n {
        if i + 1 == n {
            out.push(rest);
        } else {
            let (a, b) = rest.split_at_fraction(1.0 / f64::from(n - i));
            out.push(a);
            rest = b;
        }
    }
    out
}

/// Writes the items as one SVG in millimetres; the drawing's bounds become the viewBox
/// `0 0 width height` (y flipped back to SVG's y-down). Arcs stay arcs.
pub fn export_svg(items: &[ExportItem<'_>]) -> Result<(String, ExportReport), ExportError> {
    let b = bounds_of(items)?;
    let (w, h) = (b.width(), b.height());
    // World (y-up) → SVG output (y-down, origin at the drawing's top-left).
    let flip = Affine::new([1.0, 0.0, 0.0, -1.0, -b.min().x, b.max().y]).map_err(|_| ExportError::Empty)?;
    let mut out = String::new();
    let _ = writeln!(out, r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    let _ =
        writeln!(out, r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}mm" height="{h}mm" viewBox="0 0 {w} {h}">"#);
    for item in items {
        if item.path.is_empty() {
            continue;
        }
        let p = item.path.transform(&flip);
        let mut d = String::new();
        for sp in p.subpaths() {
            let s = sp.start();
            let _ = write!(d, "M{} {}", s.x, s.y);
            for seg in sp.segments() {
                match seg {
                    Segment::Line { to } => {
                        let _ = write!(d, " L{} {}", to.x, to.y);
                    }
                    Segment::Cubic { c1, c2, to } => {
                        let _ = write!(d, " C{} {} {} {} {} {}", c1.x, c1.y, c2.x, c2.y, to.x, to.y);
                    }
                    Segment::Arc(arc) => {
                        for piece in arc_pieces(arc) {
                            let (u, v) = piece.semi_diameters();
                            // Principal axes: |u cos t + v sin t| is extremal at tan 2t = 2u·v / (|u|² − |v|²).
                            let t0 = 0.5 * math::atan2(2.0 * u.dot(v), u.dot(u) - v.dot(v));
                            let (st, ct) = math::sin_cos(t0);
                            let a1 = u * ct + v * st;
                            let a2 = v * ct - u * st;
                            let rot = math::atan2(a1.y, a1.x) * 180.0 / PI;
                            let sweep = u.cross(v) * piece.sweep_parameter() > 0.0;
                            let e = piece.end_point();
                            let _ = write!(
                                d,
                                " A{} {} {} 0 {} {} {}",
                                a1.length(),
                                a2.length(),
                                rot,
                                u8::from(sweep),
                                e.x,
                                e.y
                            );
                        }
                    }
                }
            }
            if sp.is_closed() {
                d.push_str(" Z");
            }
            d.push(' ');
        }
        let _ = writeln!(out, r#"  <path id="{}" d="{}"/>"#, xml_escape(item.name), d.trim_end());
    }
    out.push_str("</svg>\n");
    Ok((out, ExportReport::default()))
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn layer_name(s: &str) -> String {
    let clean: String = s.chars().map(|c| if c.is_control() || c == '<' || c == '>' { '_' } else { c }).collect();
    if clean.trim().is_empty() { "0".to_owned() } else { clean }
}

struct DxfWriter {
    out: String,
    handle: u32,
}

impl DxfWriter {
    fn pair(&mut self, code: i32, value: impl fmt::Display) {
        let _ = write!(self.out, "{code}\n{value}\n");
    }

    fn entity(&mut self, kind: &str, layer: &str, subclass: &str) {
        self.handle += 1;
        self.pair(0, kind);
        self.pair(5, format!("{:X}", self.handle));
        self.pair(100, "AcDbEntity");
        self.pair(8, layer);
        self.pair(100, subclass);
    }
}

/// Writes the items as an ASCII DXF (R2000, `$INSUNITS` = mm). Subpaths of lines and circular
/// arcs become LWPOLYLINEs with bulges (exact, closed flag kept); subpaths with cubics or
/// elliptical arcs become one degree-3 SPLINE each (closed flag kept), which approximates arcs.
pub fn export_dxf(items: &[ExportItem<'_>]) -> Result<(String, ExportReport), ExportError> {
    bounds_of(items)?;
    let mut report = ExportReport::default();
    let mut w = DxfWriter { out: String::new(), handle: 0x10 };
    w.pair(0, "SECTION");
    w.pair(2, "HEADER");
    w.pair(9, "$ACADVER");
    w.pair(1, "AC1015");
    w.pair(9, "$INSUNITS");
    w.pair(70, 4);
    w.pair(0, "ENDSEC");
    w.pair(0, "SECTION");
    w.pair(2, "ENTITIES");
    for (index, item) in items.iter().enumerate() {
        let layer = layer_name(item.name);
        for sp in item.path.subpaths() {
            let polyline_ok = sp.segments().iter().all(|s| match s {
                Segment::Line { .. } => true,
                Segment::Arc(a) => is_circular(a),
                Segment::Cubic { .. } => false,
            });
            if polyline_ok {
                write_lwpolyline(&mut w, &layer, sp);
            } else {
                let (dev, elliptical, circular) = write_spline(&mut w, &layer, sp);
                if elliptical {
                    report.losses.push(ExportLoss::EllipticalArcAsSpline { item: index, max_deviation_mm: dev });
                } else if circular {
                    report.losses.push(ExportLoss::CircularArcAsSpline { item: index, max_deviation_mm: dev });
                }
            }
        }
    }
    w.pair(0, "ENDSEC");
    w.pair(0, "EOF");
    Ok((w.out, report))
}

fn write_lwpolyline(w: &mut DxfWriter, layer: &str, sp: &Subpath) {
    // (vertex, bulge of the segment that starts there)
    let mut vs: Vec<(Point, f64)> = vec![(sp.start(), 0.0)];
    for seg in sp.segments() {
        match seg {
            Segment::Line { to } => vs.push((*to, 0.0)),
            Segment::Arc(arc) => {
                for piece in arc_pieces(arc) {
                    let (u, v) = piece.semi_diameters();
                    let ccw = u.cross(v) * piece.sweep_parameter() > 0.0;
                    let b = math::tan(piece.sweep_parameter().abs() / 4.0);
                    if let Some(last) = vs.last_mut() {
                        last.1 = if ccw { b } else { -b };
                    }
                    vs.push((piece.end_point(), 0.0));
                }
            }
            Segment::Cubic { to, .. } => vs.push((*to, 0.0)),
        }
    }
    // A closed contour whose last segment returns to the start: the closing vertex is implicit.
    if sp.is_closed() && vs.len() > 2 && vs.last().is_some_and(|(p, _)| p.distance(sp.start()) <= 1e-9) {
        vs.pop();
    }
    w.entity("LWPOLYLINE", layer, "AcDbPolyline");
    w.pair(90, vs.len());
    w.pair(70, u8::from(sp.is_closed()));
    for (p, bulge) in vs {
        w.pair(10, p.x);
        w.pair(20, p.y);
        if bulge != 0.0 {
            w.pair(42, bulge);
        }
    }
}

/// Returns (deviation bound, had elliptical arcs, had circular arcs).
fn write_spline(w: &mut DxfWriter, layer: &str, sp: &Subpath) -> (f64, bool, bool) {
    let mut ctrl = vec![sp.start()];
    let (mut dev, mut elliptical, mut circular) = (0.0f64, false, false);
    let mut from = sp.start();
    let mut segs: Vec<Segment> = sp.segments().to_vec();
    if sp.is_closed() && sp.end().distance(sp.start()) > 1e-9 {
        segs.push(Segment::Line { to: sp.start() });
    }
    for seg in &segs {
        let cubics: Vec<Segment> = match seg {
            // Degree elevation of a line is exact.
            Segment::Line { to } => {
                vec![Segment::Cubic { c1: from.lerp(*to, 1.0 / 3.0), c2: from.lerp(*to, 2.0 / 3.0), to: *to }]
            }
            Segment::Cubic { .. } => vec![*seg],
            Segment::Arc(arc) => {
                dev = dev.max(QUARTER_CUBIC_REL_ERROR * sigma_max(arc));
                if is_circular(arc) {
                    circular = true
                } else {
                    elliptical = true
                }
                arc.to_cubics()
            }
        };
        for c in cubics {
            if let Segment::Cubic { c1, c2, to } = c {
                ctrl.extend([c1, c2, to]);
            }
        }
        from = seg.end();
    }
    let spans = (ctrl.len() - 1) / 3;
    w.entity("SPLINE", layer, "AcDbSpline");
    w.pair(70, if sp.is_closed() { 8 | 1 } else { 8 });
    w.pair(71, 3);
    w.pair(72, spans * 3 + 5);
    w.pair(73, ctrl.len());
    w.pair(74, 0);
    for _ in 0..4 {
        w.pair(40, 0);
    }
    for s in 1..spans {
        for _ in 0..3 {
            w.pair(40, s);
        }
    }
    for _ in 0..4 {
        w.pair(40, spans);
    }
    for p in ctrl {
        w.pair(10, p.x);
        w.pair(20, p.y);
        w.pair(30, 0);
    }
    (dev, elliptical, circular)
}
