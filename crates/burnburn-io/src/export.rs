//! Limited SVG and DXF export (IM10, P05). Output is in millimetres; anything that cannot be
//! written exactly is named in the [`ExportReport`] before the user commits to the file.

use std::fmt;

use std::f64::consts::PI;
use std::fmt::Write as _;

use burnburn_geometry::{Affine, Bounds, EllipticalArc, Path, Point, ProductionTolerance, Segment, Subpath, math};
use burnburn_types::Length;

/// What the caller already flattened before it handed over plain geometry: relations of a
/// sketch, modifiers (arrays) that were expanded, a parametric shape or text that became
/// outlines. The exporter names them in the report; constraints and modifiers must be
/// acknowledged before anything is written (P05).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LostSemantics {
    /// Sketch constraints/relations the file cannot carry.
    pub constraints: u32,
    /// Modifiers (array, mirror, …) whose results were written as plain copies.
    pub modifiers: u32,
    /// A parametric shape written as a plain path.
    pub parametric: bool,
    /// Text written as outlines.
    pub text_outlined: bool,
}

/// One object to export: its name (SVG `id`, DXF layer) and world geometry.
#[derive(Debug, Clone, Copy)]
pub struct ExportItem<'a> {
    pub name: &'a str,
    pub path: &'a Path,
    /// Layer name for grouping (SVG `<g>`) and DXF layers; the name when `None`.
    pub layer: Option<&'a str>,
    /// Stroke colour of the object's layer.
    pub color: Option<[u8; 3]>,
    pub lost: LostSemantics,
}

impl<'a> ExportItem<'a> {
    pub fn new(name: &'a str, path: &'a Path) -> Self {
        ExportItem { name, path, layer: None, color: None, lost: LostSemantics::default() }
    }

    pub fn with_layer(mut self, layer: &'a str) -> Self {
        self.layer = Some(layer);
        self
    }

    pub fn with_color(mut self, rgb: [u8; 3]) -> Self {
        self.color = Some(rgb);
        self
    }

    pub fn with_lost(mut self, lost: LostSemantics) -> Self {
        self.lost = lost;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum ExportLoss {
    /// A non-circular arc was written as cubic spline spans; deviation bound in mm.
    EllipticalArcAsSpline { item: usize, max_deviation_mm: f64 },
    /// A circular arc inside a curved subpath was written as cubic spline spans.
    CircularArcAsSpline { item: usize, max_deviation_mm: f64 },
    /// Sketch constraints are not part of the file format (acknowledgement required).
    ConstraintsDropped { item: usize, count: u32 },
    /// Modifiers were written as plain copies (acknowledgement required).
    ModifiersApplied { item: usize, count: u32 },
    /// A parametric shape was written as a plain path.
    ParametricFlattened { item: usize },
    /// Text was written as outlines.
    TextAsOutlines { item: usize },
    /// DXF: curves were written as chords within this deviation (`DxfCurves::Polylines`).
    CurvesFlattened { item: usize, max_deviation_mm: f64 },
    /// DXF: the layer colour was mapped to the nearest AutoCAD colour index.
    ColorApproximated { item: usize },
}

impl ExportLoss {
    /// Losses of meaning the user must acknowledge before the file is written.
    pub fn needs_acknowledgement(&self) -> bool {
        matches!(self, ExportLoss::ConstraintsDropped { .. } | ExportLoss::ModifiersApplied { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExportReport {
    pub losses: Vec<ExportLoss>,
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ExportError {
    /// No geometry to write.
    Empty,
    /// Constraints or modifiers would be lost and the caller did not acknowledge it; nothing
    /// was written. Show the list, then export again with `acknowledge_lost_semantics`.
    LossesNotAcknowledged(Vec<ExportLoss>),
    /// An option value that cannot be honoured (for example a zero curve tolerance).
    InvalidOption(&'static str),
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExportError::Empty => f.write_str("nothing to export"),
            ExportError::LossesNotAcknowledged(l) => write!(f, "export would lose {} kinds of information", l.len()),
            ExportError::InvalidOption(what) => write!(f, "invalid export option: {what}"),
        }
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

/// Unit of the SVG's `width`/`height` and of its user space (`viewBox`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SvgUnit {
    #[default]
    Mm,
    Inch,
    /// CSS pixels, 96 per inch.
    Pixel,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SvgOptions {
    pub unit: SvgUnit,
    /// Stroke width of every path in mm (hairline); fills are always `none`.
    pub stroke_mm: f64,
    /// Stroke colour of items without a colour.
    pub default_color: [u8; 3],
    pub use_item_colors: bool,
    /// Wrap the items of one layer in `<g id="layer">`.
    pub group_layers: bool,
    /// Decimals of every coordinate (`None`: shortest exact text).
    pub decimals: Option<u8>,
    /// Write `width`/`height` in pixels of this size instead of the physical size (thumbnails);
    /// the `viewBox` keeps the drawing's proportions.
    pub fit_px: Option<(f64, f64)>,
    pub acknowledge_lost_semantics: bool,
}

impl Default for SvgOptions {
    fn default() -> Self {
        SvgOptions {
            unit: SvgUnit::Mm,
            stroke_mm: 0.1,
            default_color: [0, 0, 0],
            use_item_colors: true,
            group_layers: false,
            decimals: None,
            fit_px: None,
            acknowledge_lost_semantics: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DxfUnit {
    #[default]
    Mm,
    Inch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DxfLayers {
    /// One layer per object, named after the object.
    #[default]
    ItemName,
    /// The item's layer name, or its own name when it has none.
    ItemLayerOrName,
    /// Everything on layer `0`.
    Single,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum DxfCurves {
    /// Cubics as degree-3 SPLINEs, circular arcs as bulges (exact where possible).
    #[default]
    Splines,
    /// Curves as chords within `tolerance_mm` (for programs without SPLINE support).
    Polylines { tolerance_mm: f64 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct DxfOptions {
    pub unit: DxfUnit,
    pub layers: DxfLayers,
    pub curves: DxfCurves,
    /// Give layers the AutoCAD colour index nearest to the item colour.
    pub use_item_colors: bool,
    pub acknowledge_lost_semantics: bool,
}

impl Default for DxfOptions {
    fn default() -> Self {
        DxfOptions {
            unit: DxfUnit::Mm,
            layers: DxfLayers::ItemName,
            curves: DxfCurves::Splines,
            use_item_colors: true,
            acknowledge_lost_semantics: false,
        }
    }
}

/// What an export would lose, computed without writing anything (P05): lost constraints and
/// modifiers first (they need acknowledgement), then approximations and informational items.
pub fn preflight_svg(items: &[ExportItem<'_>], options: &SvgOptions) -> Result<Vec<ExportLoss>, ExportError> {
    build_svg(items, options).map(|(_, r)| r.losses)
}

/// What an export would lose, computed without writing anything (P05).
pub fn preflight_dxf(items: &[ExportItem<'_>], options: &DxfOptions) -> Result<Vec<ExportLoss>, ExportError> {
    build_dxf(items, options).map(|(_, r)| r.losses)
}

/// [`export_svg`] with options; refuses to write while constraints or modifiers would be lost
/// and `acknowledge_lost_semantics` is off.
pub fn export_svg_with(items: &[ExportItem<'_>], options: &SvgOptions) -> Result<(String, ExportReport), ExportError> {
    let (text, report) = build_svg(items, options)?;
    acknowledged(&report, options.acknowledge_lost_semantics)?;
    Ok((text, report))
}

/// [`export_dxf`] with options; refuses like [`export_svg_with`].
pub fn export_dxf_with(items: &[ExportItem<'_>], options: &DxfOptions) -> Result<(String, ExportReport), ExportError> {
    let (text, report) = build_dxf(items, options)?;
    acknowledged(&report, options.acknowledge_lost_semantics)?;
    Ok((text, report))
}

fn acknowledged(report: &ExportReport, ack: bool) -> Result<(), ExportError> {
    if !ack && report.losses.iter().any(ExportLoss::needs_acknowledgement) {
        return Err(ExportError::LossesNotAcknowledged(report.losses.clone()));
    }
    Ok(())
}

/// Losses the caller flattened before export, in item order (constraints, modifiers, parametric, text).
fn semantic_losses(items: &[ExportItem<'_>]) -> Vec<ExportLoss> {
    let mut out = Vec::new();
    for (item, it) in items.iter().enumerate() {
        if it.lost.constraints > 0 {
            out.push(ExportLoss::ConstraintsDropped { item, count: it.lost.constraints });
        }
        if it.lost.modifiers > 0 {
            out.push(ExportLoss::ModifiersApplied { item, count: it.lost.modifiers });
        }
        if it.lost.parametric {
            out.push(ExportLoss::ParametricFlattened { item });
        }
        if it.lost.text_outlined {
            out.push(ExportLoss::TextAsOutlines { item });
        }
    }
    out
}

/// Layer of each item under `policy`.
fn layer_of<'a>(it: &ExportItem<'a>, policy: DxfLayers) -> &'a str {
    match policy {
        DxfLayers::ItemName => it.name,
        DxfLayers::ItemLayerOrName => it.layer.unwrap_or(it.name),
        DxfLayers::Single => "0",
    }
}

fn number(v: f64, decimals: Option<u8>) -> String {
    match decimals {
        None => format!("{v}"),
        Some(d) => {
            let t = format!("{v:.*}", usize::from(d.min(12)));
            let t = if t.contains('.') { t.trim_end_matches('0').trim_end_matches('.').to_owned() } else { t };
            if t == "-0" { "0".to_owned() } else { t }
        }
    }
}

/// Writes the items as one SVG in millimetres with default [`SvgOptions`]; the drawing's bounds
/// become the viewBox `0 0 width height` (y flipped back to SVG's y-down). Arcs stay arcs.
pub fn export_svg(items: &[ExportItem<'_>]) -> Result<(String, ExportReport), ExportError> {
    export_svg_with(items, &SvgOptions::default())
}

fn build_svg(items: &[ExportItem<'_>], opt: &SvgOptions) -> Result<(String, ExportReport), ExportError> {
    let b = bounds_of(items)?;
    let scale = match opt.unit {
        SvgUnit::Mm => 1.0,
        SvgUnit::Inch => 1.0 / 25.4,
        SvgUnit::Pixel => 96.0 / 25.4,
    };
    let suffix = match opt.unit {
        SvgUnit::Mm => "mm",
        SvgUnit::Inch => "in",
        SvgUnit::Pixel => "px",
    };
    let (w, h) = (b.width() * scale, b.height() * scale);
    let num = |v: f64| number(v, opt.decimals);
    // World (y-up, mm) → SVG output (y-down, origin at the drawing's top-left, chosen unit).
    let flip = Affine::new([scale, 0.0, 0.0, -scale, -b.min().x * scale, b.max().y * scale])
        .map_err(|_| ExportError::Empty)?;
    let mut out = String::new();
    let _ = writeln!(out, r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    let (size_w, size_h) = match opt.fit_px {
        Some((pw, ph)) if pw > 0.0 && ph > 0.0 && pw.is_finite() && ph.is_finite() => {
            (format!("{}px", number(pw, None)), format!("{}px", number(ph, None)))
        }
        _ => (format!("{}{suffix}", num(w)), format!("{}{suffix}", num(h))),
    };
    let _ = writeln!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{size_w}" height="{size_h}" viewBox="0 0 {} {}">"#,
        num(w),
        num(h)
    );
    // Layers in order of first appearance; without grouping everything is one anonymous run.
    let mut order: Vec<&str> = Vec::new();
    if opt.group_layers {
        for it in items {
            let layer = it.layer.unwrap_or(it.name);
            if !order.contains(&layer) {
                order.push(layer);
            }
        }
    }
    let groups: Vec<Option<&str>> =
        if opt.group_layers { order.iter().map(|l| Some(*l)).collect() } else { vec![None] };
    for group in groups {
        let indent = if group.is_some() { "    " } else { "  " };
        if let Some(g) = group {
            let _ = writeln!(out, r#"  <g id="{}">"#, xml_escape(g));
        }
        for it in items.iter().filter(|it| group.is_none_or(|g| it.layer.unwrap_or(it.name) == g)) {
            if it.path.is_empty() {
                continue;
            }
            let rgb = if opt.use_item_colors { it.color.unwrap_or(opt.default_color) } else { opt.default_color };
            let d = svg_path_data(&it.path.transform(&flip), &num);
            let _ = writeln!(
                out,
                r##"{indent}<path id="{}" d="{d}" fill="none" stroke="#{:02x}{:02x}{:02x}" stroke-width="{}"/>"##,
                xml_escape(it.name),
                rgb[0],
                rgb[1],
                rgb[2],
                num(opt.stroke_mm * scale),
            );
        }
        if group.is_some() {
            out.push_str("  </g>\n");
        }
    }
    out.push_str("</svg>\n");
    Ok((out, ExportReport { losses: semantic_losses(items) }))
}

fn svg_path_data(p: &Path, num: &dyn Fn(f64) -> String) -> String {
    let mut d = String::new();
    for sp in p.subpaths() {
        let s = sp.start();
        let _ = write!(d, "M{} {}", num(s.x), num(s.y));
        for seg in sp.segments() {
            match seg {
                Segment::Line { to } => {
                    let _ = write!(d, " L{} {}", num(to.x), num(to.y));
                }
                Segment::Cubic { c1, c2, to } => {
                    let _ = write!(
                        d,
                        " C{} {} {} {} {} {}",
                        num(c1.x),
                        num(c1.y),
                        num(c2.x),
                        num(c2.y),
                        num(to.x),
                        num(to.y)
                    );
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
                            num(a1.length()),
                            num(a2.length()),
                            num(rot),
                            u8::from(sweep),
                            num(e.x),
                            num(e.y)
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
    d.trim_end().to_owned()
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

/// Writes the items as an ASCII DXF (R2000, `$INSUNITS` = mm) with default [`DxfOptions`].
/// Subpaths of lines and circular arcs become LWPOLYLINEs with bulges (exact, closed flag kept);
/// subpaths with cubics or elliptical arcs become one degree-3 SPLINE each (closed flag kept),
/// which approximates arcs. A LAYER table lists every layer with its AutoCAD colour index.
pub fn export_dxf(items: &[ExportItem<'_>]) -> Result<(String, ExportReport), ExportError> {
    export_dxf_with(items, &DxfOptions::default())
}

/// AutoCAD colour indices 1–9 and their RGB; index 7 is "black or white, whichever contrasts".
const ACI_PALETTE: [(u8, [u8; 3]); 9] = [
    (1, [255, 0, 0]),
    (2, [255, 255, 0]),
    (3, [0, 255, 0]),
    (4, [0, 255, 255]),
    (5, [0, 0, 255]),
    (6, [255, 0, 255]),
    (7, [255, 255, 255]),
    (8, [128, 128, 128]),
    (9, [192, 192, 192]),
];

/// The nearest AutoCAD colour index and whether it is exact. Pure black maps to 7.
fn aci_for(rgb: [u8; 3]) -> (u8, bool) {
    if rgb == [0, 0, 0] {
        return (7, true);
    }
    let dist = |p: [u8; 3]| -> i32 { (0..3).map(|i| (i32::from(p[i]) - i32::from(rgb[i])).pow(2)).sum() };
    let (index, palette) = ACI_PALETTE.iter().copied().min_by_key(|(_, p)| dist(*p)).unwrap_or((7, [255; 3]));
    (index, palette == rgb)
}

fn build_dxf(items: &[ExportItem<'_>], opt: &DxfOptions) -> Result<(String, ExportReport), ExportError> {
    bounds_of(items)?;
    if let DxfCurves::Polylines { tolerance_mm } = opt.curves
        && !(tolerance_mm.is_finite() && tolerance_mm > 0.0)
    {
        return Err(ExportError::InvalidOption("curve tolerance must be positive"));
    }
    let (scale, insunits) = match opt.unit {
        DxfUnit::Mm => (1.0, 4),
        DxfUnit::Inch => (1.0 / 25.4, 1),
    };
    let to_unit = Affine::scale(scale, scale).map_err(|_| ExportError::Empty)?;
    let mut report = ExportReport { losses: semantic_losses(items) };

    // Layers in order of first appearance; the colour comes from the first coloured item on it.
    let mut layers: Vec<(String, u8)> = Vec::new();
    for (index, it) in items.iter().enumerate() {
        let name = layer_name(layer_of(it, opt.layers));
        if layers.iter().any(|(n, _)| *n == name) {
            continue;
        }
        let coloured = items
            .iter()
            .enumerate()
            .find(|(_, other)| layer_name(layer_of(other, opt.layers)) == name && other.color.is_some());
        let aci = match coloured {
            Some((i, other)) if opt.use_item_colors => {
                let (aci, exact) = aci_for(other.color.unwrap_or([0; 3]));
                if !exact {
                    report.losses.push(ExportLoss::ColorApproximated { item: i });
                }
                aci
            }
            _ => 7,
        };
        let _ = index;
        layers.push((name, aci));
    }

    let mut w = DxfWriter { out: String::new(), handle: 0x10 };
    w.pair(0, "SECTION");
    w.pair(2, "HEADER");
    w.pair(9, "$ACADVER");
    w.pair(1, "AC1015");
    w.pair(9, "$INSUNITS");
    w.pair(70, insunits);
    w.pair(0, "ENDSEC");
    w.pair(0, "SECTION");
    w.pair(2, "TABLES");
    w.pair(0, "TABLE");
    w.pair(2, "LTYPE");
    w.pair(5, "5");
    w.pair(100, "AcDbSymbolTable");
    w.pair(70, 1);
    w.pair(0, "LTYPE");
    w.pair(5, "6");
    w.pair(100, "AcDbSymbolTableRecord");
    w.pair(100, "AcDbLinetypeTableRecord");
    w.pair(2, "CONTINUOUS");
    w.pair(70, 0);
    w.pair(3, "Solid line");
    w.pair(72, 65);
    w.pair(73, 0);
    w.pair(40, 0);
    w.pair(0, "ENDTAB");
    w.pair(0, "TABLE");
    w.pair(2, "LAYER");
    w.pair(5, "2");
    w.pair(100, "AcDbSymbolTable");
    w.pair(70, layers.len());
    for (name, aci) in &layers {
        w.handle += 1;
        let handle = format!("{:X}", w.handle);
        w.pair(0, "LAYER");
        w.pair(5, handle);
        w.pair(100, "AcDbSymbolTableRecord");
        w.pair(100, "AcDbLayerTableRecord");
        w.pair(2, name);
        w.pair(70, 0);
        w.pair(62, aci);
        w.pair(6, "CONTINUOUS");
    }
    w.pair(0, "ENDTAB");
    w.pair(0, "ENDSEC");
    w.pair(0, "SECTION");
    w.pair(2, "ENTITIES");
    for (index, item) in items.iter().enumerate() {
        let layer = layer_name(layer_of(item, opt.layers));
        let path = if scale == 1.0 { item.path.clone() } else { item.path.transform(&to_unit) };
        let mut flattened_dev = 0.0f64;
        for sp in path.subpaths() {
            let polyline_ok = sp.segments().iter().all(|s| match s {
                Segment::Line { .. } => true,
                Segment::Arc(a) => is_circular(a),
                Segment::Cubic { .. } => false,
            });
            if polyline_ok {
                write_lwpolyline(&mut w, &layer, sp);
                continue;
            }
            match opt.curves {
                DxfCurves::Polylines { tolerance_mm } => {
                    let tol = tolerance_mm * scale;
                    if let Some(chords) = flatten_to_lines(sp, tol) {
                        write_lwpolyline(&mut w, &layer, &chords);
                        flattened_dev = flattened_dev.max(tolerance_mm);
                    }
                }
                DxfCurves::Splines => {
                    let (dev, elliptical, circular) = write_spline(&mut w, &layer, sp);
                    let dev_mm = dev / scale;
                    if elliptical {
                        report.losses.push(ExportLoss::EllipticalArcAsSpline { item: index, max_deviation_mm: dev_mm });
                    } else if circular {
                        report.losses.push(ExportLoss::CircularArcAsSpline { item: index, max_deviation_mm: dev_mm });
                    }
                }
            }
        }
        if flattened_dev > 0.0 {
            report.losses.push(ExportLoss::CurvesFlattened { item: index, max_deviation_mm: flattened_dev });
        }
    }
    w.pair(0, "ENDSEC");
    w.pair(0, "EOF");
    Ok((w.out, report))
}

/// `sp` as straight chords within `tol` (a line-only subpath with the same closed flag).
fn flatten_to_lines(sp: &Subpath, tol: f64) -> Option<Subpath> {
    let tolerance = ProductionTolerance::new(Length::from_mm(tol).ok()?).ok()?;
    let poly = Path::from_subpaths(vec![sp.clone()]).flatten(&tolerance).into_iter().next()?;
    let (first, rest) = poly.points.split_first()?;
    Subpath::new(*first, rest.iter().map(|to| Segment::Line { to: *to }).collect(), poly.closed).ok()
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
