//! ASCII DXF import: LINE, LWPOLYLINE, POLYLINE/VERTEX, ARC, CIRCLE, ELLIPSE, SPLINE and
//! `$INSUNITS`. Geometry is kept in drawing units until a scale is known; a unitless drawing
//! cannot be committed without an explicit [`DxfScale`] (P03).
//!
//! Non-rational clamped B-splines up to degree 3 become exact Bézier spans by knot insertion.
//! Other curve types, block references, text and hatches are reported, not approximated.

use std::f64::consts::TAU;

use burnburn_document::GeometryCommand;
use burnburn_geometry::{Affine, EllipticalArc, Path, Point, Segment, Subpath, Vector, math};
use burnburn_types::{Angle, ContentHash, Unit};

use super::{
    ImportDraft, ImportError, ImportedPath, ParseError, ParseLimits, SkipReason, Skipped, UnitDecision, check_size,
};

/// An explicit drawing-unit → millimetre factor chosen by the user.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DxfScale {
    mm_per_unit: f64,
}

impl DxfScale {
    pub fn unit(unit: Unit) -> Self {
        DxfScale { mm_per_unit: unit.mm_per_unit() }
    }

    /// A custom factor; `None` unless finite and positive.
    pub fn mm_per_unit(mm: f64) -> Option<Self> {
        (mm.is_finite() && mm > 0.0).then_some(DxfScale { mm_per_unit: mm })
    }

    pub fn factor(&self) -> f64 {
        self.mm_per_unit
    }
}

#[derive(Debug, Clone, PartialEq)]
struct RawItem {
    name: String,
    entity: String,
    path: Path,
}

/// A parsed DXF in drawing units; the scale is applied by [`DxfDocument::to_draft`].
#[derive(Debug, Clone, PartialEq)]
pub struct DxfDocument {
    insunits: Option<i32>,
    items: Vec<RawItem>,
    skipped: Vec<Skipped>,
}

/// Millimetres per `$INSUNITS` unit; `None` for unitless and astronomical codes.
fn insunits_mm(code: i32) -> Option<f64> {
    Some(match code {
        1 => 25.4,
        2 => 304.8,
        3 => 1_609_344.0,
        4 => 1.0,
        5 => 10.0,
        6 => 1000.0,
        7 => 1.0e6,
        8 => 25.4e-6,
        9 => 0.0254,
        10 => 914.4,
        11 => 1.0e-7,
        12 => 1.0e-6,
        13 => 1.0e-3,
        14 => 100.0,
        15 => 1.0e4,
        16 => 1.0e5,
        _ => return None,
    })
}

impl DxfDocument {
    /// The declared `$INSUNITS` code; `None` when absent or 0 (unitless).
    pub fn declared_unit(&self) -> Option<i32> {
        self.insunits
    }

    /// Scales to millimetres: an explicit `scale` wins; otherwise the declared unit; otherwise
    /// [`ImportError::ScaleRequired`].
    pub fn to_draft(&self, scale: Option<DxfScale>) -> Result<ImportDraft, ImportError> {
        let (factor, units) = match (scale, self.insunits.and_then(|c| insunits_mm(c).map(|f| (c, f)))) {
            (Some(s), _) => (s.factor(), UnitDecision::DxfExplicit { mm_per_unit: s.factor() }),
            (None, Some((code, f))) => (f, UnitDecision::DxfDeclared { insunits: code, mm_per_unit: f }),
            (None, None) => return Err(ImportError::ScaleRequired),
        };
        let m = Affine::scale(factor, factor).map_err(|_| ImportError::ScaleRequired)?;
        let items = self
            .items
            .iter()
            .map(|i| ImportedPath {
                name: i.name.clone(),
                entity: i.entity.clone(),
                path: i.path.transform(&m),
                layer: None,
            })
            .collect();
        Ok(ImportDraft::new(items, units, self.skipped.clone()))
    }

    pub fn import_command(&self, asset: ContentHash, scale: Option<DxfScale>) -> Result<GeometryCommand, ImportError> {
        self.to_draft(scale)?.into_command(asset)
    }
}

/// Fuzz entry point with default limits.
pub fn parse_dxf(bytes: &[u8]) -> Result<DxfDocument, ParseError> {
    parse_dxf_with(bytes, &ParseLimits::default())
}

struct Entity<'a> {
    kind: &'a str,
    pairs: Vec<(i32, &'a str)>,
    index: usize,
}

impl Entity<'_> {
    fn f(&self, code: i32) -> Option<f64> {
        self.pairs
            .iter()
            .find(|(c, _)| *c == code)
            .and_then(|(_, v)| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite())
    }

    fn i(&self, code: i32) -> Option<i64> {
        self.pairs.iter().find(|(c, _)| *c == code).and_then(|(_, v)| v.trim().parse::<i64>().ok())
    }

    fn s(&self, code: i32) -> Option<&str> {
        self.pairs.iter().find(|(c, _)| *c == code).map(|(_, v)| v.trim())
    }

    fn all(&self, code: i32) -> Vec<f64> {
        self.pairs.iter().filter(|(c, _)| *c == code).filter_map(|(_, v)| v.trim().parse::<f64>().ok()).collect()
    }

    fn name(&self) -> String {
        match self.s(5) {
            Some(h) => format!("{}#{h}", self.kind),
            None => format!("{}#{}", self.kind, self.index),
        }
    }

    fn layer(&self) -> String {
        self.s(8).unwrap_or("0").to_owned()
    }
}

/// Parses ASCII DXF within `limits`. Never panics.
pub fn parse_dxf_with(bytes: &[u8], limits: &ParseLimits) -> Result<DxfDocument, ParseError> {
    check_size(bytes, limits)?;
    if bytes.starts_with(b"AutoCAD Binary DXF") {
        return Err(ParseError::Unsupported("binary DXF"));
    }
    let text = String::from_utf8_lossy(bytes);
    let lines: Vec<&str> = text.lines().collect();
    let mut pairs: Vec<(i32, &str, usize)> = Vec::with_capacity(lines.len() / 2);
    let mut i = 0;
    while i + 1 < lines.len() {
        let code = lines[i].trim();
        if code.is_empty() && i + 1 == lines.len() - 1 && lines[i + 1].trim().is_empty() {
            break;
        }
        let code: i32 =
            code.parse().map_err(|_| ParseError::Malformed { at: i + 1, reason: "group code is not an integer" })?;
        pairs.push((code, lines[i + 1], i + 1));
        i += 2;
    }
    if i < lines.len() && !lines[i].trim().is_empty() {
        return Err(ParseError::Malformed { at: i + 1, reason: "dangling group code" });
    }
    // Numeric group codes must hold numbers (10–59 coordinates/reals, 60–99 integers, 210–239 vectors).
    for (code, value, line) in &pairs {
        let numeric = (10..=59).contains(code) || (60..=99).contains(code) || (210..=239).contains(code);
        if numeric && value.trim().parse::<f64>().map_or(true, |v| !v.is_finite()) {
            return Err(ParseError::Malformed { at: *line, reason: "numeric group value is not a finite number" });
        }
    }

    let mut doc = DxfDocument { insunits: None, items: Vec::new(), skipped: Vec::new() };
    let mut section: Option<&str> = None;
    let mut k = 0;
    let mut entities: Vec<Entity<'_>> = Vec::new();
    while k < pairs.len() {
        let (code, value, _) = pairs[k];
        let value = value.trim();
        if code == 0 && value == "SECTION" {
            section = pairs.get(k + 1).filter(|p| p.0 == 2).map(|p| p.1.trim());
            k += 2;
            continue;
        }
        if code == 0 && (value == "ENDSEC" || value == "EOF") {
            section = None;
            k += 1;
            continue;
        }
        match section {
            Some("HEADER") => {
                if code == 9
                    && value == "$INSUNITS"
                    && let Some((70, v, _)) = pairs.get(k + 1)
                {
                    let c: i32 = v.trim().parse().unwrap_or(0);
                    doc.insunits = (c != 0).then_some(c);
                }
                k += 1;
            }
            Some("ENTITIES") if code == 0 => {
                let start = k;
                k += 1;
                while k < pairs.len() && pairs[k].0 != 0 {
                    k += 1;
                }
                entities.push(Entity {
                    kind: value,
                    pairs: pairs[start + 1..k].iter().map(|(c, v, _)| (*c, *v)).collect(),
                    index: entities.len() + 1,
                });
                if entities.len() > limits.max_entities {
                    return Err(ParseError::TooManyEntities { limit: limits.max_entities });
                }
            }
            _ => k += 1,
        }
    }
    let mut commands = 0usize;
    let mut e = 0;
    while e < entities.len() {
        let ent = &entities[e];
        commands = commands.saturating_add(ent.pairs.len());
        if commands > limits.max_path_commands {
            return Err(ParseError::TooManyCommands { limit: limits.max_path_commands });
        }
        if ent.kind == "POLYLINE" {
            // VERTEX entities follow until SEQEND.
            let mut end = e + 1;
            while end < entities.len() && entities[end].kind == "VERTEX" {
                end += 1;
            }
            let result = polyline(ent, &entities[e + 1..end]);
            doc.push(ent, result);
            e = if entities.get(end).is_some_and(|x| x.kind == "SEQEND") { end + 1 } else { end };
            continue;
        }
        let result = match ent.kind {
            "LINE" => line(ent),
            "CIRCLE" => circle(ent),
            "ARC" => arc(ent),
            "LWPOLYLINE" => lwpolyline(ent),
            "ELLIPSE" => ellipse(ent),
            "SPLINE" => spline(ent),
            _ => Err(SkipReason::UnsupportedEntity),
        };
        doc.push(ent, result);
        e += 1;
    }
    Ok(doc)
}

impl DxfDocument {
    fn push(&mut self, ent: &Entity<'_>, result: Result<Path, SkipReason>) {
        match result {
            Ok(path) if !path.is_empty() => {
                self.items.push(RawItem { name: ent.layer(), entity: ent.name(), path });
            }
            Ok(_) => self.skipped.push(Skipped {
                entity: ent.name(),
                kind: ent.kind.to_owned(),
                reason: SkipReason::Invalid("empty"),
            }),
            Err(reason) => self.skipped.push(Skipped { entity: ent.name(), kind: ent.kind.to_owned(), reason }),
        }
    }
}

const MISSING: SkipReason = SkipReason::Invalid("missing or invalid coordinates");

fn pt(ent: &Entity<'_>, x: i32, y: i32) -> Result<Point, SkipReason> {
    Ok(Point::new(ent.f(x).ok_or(MISSING)?, ent.f(y).ok_or(MISSING)?))
}

/// Object coordinate system: only the ±Z extrusion is supported; −Z mirrors x.
fn ocs(ent: &Entity<'_>) -> Result<Option<Affine>, SkipReason> {
    let (ex, ey, ez) = (ent.f(210).unwrap_or(0.0), ent.f(220).unwrap_or(0.0), ent.f(230).unwrap_or(1.0));
    if ex.abs() > 1e-12 || ey.abs() > 1e-12 {
        return Err(SkipReason::Invalid("extrusion direction is not ±Z"));
    }
    Ok((ez < 0.0).then(Affine::mirror_x))
}

fn in_ocs(ent: &Entity<'_>, path: Path) -> Result<Path, SkipReason> {
    Ok(match ocs(ent)? {
        Some(m) => path.transform(&m),
        None => path,
    })
}

fn line(ent: &Entity<'_>) -> Result<Path, SkipReason> {
    Path::line(pt(ent, 10, 20)?, pt(ent, 11, 21)?).map_err(|_| MISSING)
}

fn circle(ent: &Entity<'_>) -> Result<Path, SkipReason> {
    let r = ent.f(40).ok_or(MISSING)?;
    let p = Path::circle(pt(ent, 10, 20)?, r).map_err(|_| SkipReason::Invalid("radius must be positive"))?;
    in_ocs(ent, p)
}

fn arc(ent: &Entity<'_>) -> Result<Path, SkipReason> {
    let (c, r) = (pt(ent, 10, 20)?, ent.f(40).ok_or(MISSING)?);
    let (a0, a1) = (ent.f(50).ok_or(MISSING)?, ent.f(51).ok_or(MISSING)?);
    let mut sweep = (a1 - a0).rem_euclid(360.0);
    if sweep == 0.0 {
        sweep = 360.0;
    }
    let arc = EllipticalArc::circle(
        c,
        r,
        Angle::from_degrees(a0).map_err(|_| MISSING)?,
        Angle::from_degrees(sweep).map_err(|_| MISSING)?,
    )
    .map_err(|_| SkipReason::Invalid("radius must be positive"))?;
    let sp = Subpath::new(arc.start_point(), vec![Segment::Arc(arc)], false).map_err(|_| MISSING)?;
    in_ocs(ent, Path::from_subpaths(vec![sp]))
}

/// The segment from `a` to `b` with DXF bulge `tan(sweep/4)` (positive = counter-clockwise).
fn bulge_segment(a: Point, b: Point, bulge: f64) -> Segment {
    let chord = b - a;
    let c = chord.length();
    if bulge.abs() < 1e-15 || !(c > 0.0) {
        return Segment::Line { to: b };
    }
    let sweep = 4.0 * math::atan2(bulge, 1.0);
    let perp = Vector::new(-chord.y / c, chord.x / c);
    let center = a.lerp(b, 0.5) + perp * (c / 2.0 * (1.0 - bulge * bulge) / (2.0 * bulge));
    let r = center.distance(a);
    let start = math::atan2(a.y - center.y, a.x - center.x);
    match (Angle::from_radians(start), Angle::from_radians(sweep)) {
        (Ok(s), Ok(w)) => match EllipticalArc::circle(center, r, s, w) {
            Ok(arc) => Segment::Arc(arc),
            Err(_) => Segment::Line { to: b },
        },
        _ => Segment::Line { to: b },
    }
}

fn vertices_path(vs: &[(Point, f64)], closed: bool) -> Result<Path, SkipReason> {
    let (first, _) = *vs.first().ok_or(SkipReason::Invalid("no vertices"))?;
    let mut segs = Vec::with_capacity(vs.len());
    for w in vs.windows(2) {
        segs.push(bulge_segment(w[0].0, w[1].0, w[0].1));
    }
    if closed
        && let Some(&(last, bulge)) = vs.last()
        && bulge.abs() >= 1e-15
    {
        segs.push(bulge_segment(last, first, bulge));
    }
    if segs.is_empty() {
        return Err(SkipReason::Invalid("fewer than two vertices"));
    }
    let sp = Subpath::new(first, segs, closed).map_err(|_| MISSING)?;
    Ok(Path::from_subpaths(vec![sp]))
}

fn lwpolyline(ent: &Entity<'_>) -> Result<Path, SkipReason> {
    let closed = ent.i(70).unwrap_or(0) & 1 == 1;
    let mut vs: Vec<(Point, f64)> = Vec::new();
    let mut pending_x: Option<f64> = None;
    for (code, v) in &ent.pairs {
        let n = v.trim().parse::<f64>().ok();
        match code {
            10 => pending_x = n,
            20 => {
                let (x, y) = (pending_x.take().ok_or(MISSING)?, n.ok_or(MISSING)?);
                vs.push((Point::new(x, y), 0.0));
            }
            42 => {
                if let Some(last) = vs.last_mut() {
                    last.1 = n.ok_or(MISSING)?;
                }
            }
            _ => {}
        }
    }
    in_ocs(ent, vertices_path(&vs, closed)?)
}

fn polyline(head: &Entity<'_>, vertices: &[Entity<'_>]) -> Result<Path, SkipReason> {
    let flags = head.i(70).unwrap_or(0);
    if flags & (8 | 16 | 64) != 0 {
        return Err(SkipReason::UnsupportedCurve("3D polyline or mesh"));
    }
    let mut vs = Vec::with_capacity(vertices.len());
    for v in vertices {
        // Spline frame control points are not on the curve.
        if v.i(70).unwrap_or(0) & 16 != 0 {
            continue;
        }
        vs.push((pt(v, 10, 20)?, v.f(42).unwrap_or(0.0)));
    }
    in_ocs(head, vertices_path(&vs, flags & 1 == 1)?)
}

fn ellipse(ent: &Entity<'_>) -> Result<Path, SkipReason> {
    let c = pt(ent, 10, 20)?;
    let major = Vector::new(ent.f(11).ok_or(MISSING)?, ent.f(21).ok_or(MISSING)?);
    let ratio = ent.f(40).ok_or(MISSING)?;
    if !(ratio > 0.0 && major.length() > 0.0) {
        return Err(SkipReason::Invalid("degenerate ellipse"));
    }
    let mut v = Vector::new(-major.y, major.x) * ratio;
    if ent.f(230).unwrap_or(1.0) < 0.0 {
        v = -v;
    }
    let (t0, t1) = (ent.f(41).unwrap_or(0.0), ent.f(42).unwrap_or(TAU));
    let mut sweep = (t1 - t0).rem_euclid(TAU);
    let full = sweep < 1e-12 || (TAU - sweep) < 1e-9;
    if full {
        sweep = TAU;
    }
    // Quarter pieces keep node editing and bounds simple; each is the exact same ellipse.
    let pieces = if full { 4 } else { 1 };
    let mut segs = Vec::with_capacity(pieces);
    for i in 0..pieces {
        let start = t0 + sweep * (i as f64) / (pieces as f64);
        let arc = EllipticalArc::new(
            c,
            major,
            v,
            Angle::from_radians(start).map_err(|_| MISSING)?,
            Angle::from_radians(sweep / pieces as f64).map_err(|_| MISSING)?,
        )
        .map_err(|_| SkipReason::Invalid("degenerate ellipse"))?;
        segs.push(Segment::Arc(arc));
    }
    let start = match segs.first() {
        Some(Segment::Arc(a)) => a.start_point(),
        _ => c,
    };
    let sp = Subpath::new(start, segs, full).map_err(|_| MISSING)?;
    Ok(Path::from_subpaths(vec![sp]))
}

fn spline(ent: &Entity<'_>) -> Result<Path, SkipReason> {
    let flags = ent.i(70).unwrap_or(0);
    let degree = usize::try_from(ent.i(71).unwrap_or(3)).map_err(|_| SkipReason::Invalid("degree"))?;
    let knots = ent.all(40);
    let weights = ent.all(41);
    let xs = ent.all(10);
    let ys = ent.all(20);
    if xs.is_empty() {
        return Err(if ent.all(11).is_empty() {
            SkipReason::Invalid("no control points")
        } else {
            SkipReason::UnsupportedCurve("fit-point spline without control points")
        });
    }
    if xs.len() != ys.len() {
        return Err(MISSING);
    }
    if flags & 2 != 0 {
        return Err(SkipReason::UnsupportedCurve("periodic spline"));
    }
    if weights.iter().any(|w| (w - weights[0]).abs() > 1e-12) {
        return Err(SkipReason::UnsupportedCurve("rational spline"));
    }
    if !(1..=3).contains(&degree) {
        return Err(SkipReason::UnsupportedCurve("spline degree above 3"));
    }
    let ctrl: Vec<Point> = xs.iter().zip(&ys).map(|(x, y)| Point::new(*x, *y)).collect();
    if knots.len() != ctrl.len() + degree + 1 || ctrl.len() < degree + 1 {
        return Err(SkipReason::Invalid("knot count does not match control points"));
    }
    if knots.windows(2).any(|w| w[1] < w[0]) {
        return Err(SkipReason::Invalid("knots decrease"));
    }
    let clamped = knots[..=degree].iter().all(|k| *k == knots[0])
        && knots[knots.len() - degree - 1..].iter().all(|k| *k == knots[knots.len() - 1]);
    if !clamped {
        return Err(SkipReason::UnsupportedCurve("unclamped spline"));
    }
    let beziers = bezier_spans(&knots, &ctrl, degree);
    let mut segs = Vec::with_capacity(beziers.len());
    for b in &beziers {
        segs.push(match degree {
            1 => Segment::Line { to: b[1] },
            2 => {
                let (p0, q, p2) = (b[0], b[1], b[2]);
                Segment::Cubic { c1: p0 + (q - p0) * (2.0 / 3.0), c2: p2 + (q - p2) * (2.0 / 3.0), to: p2 }
            }
            _ => Segment::Cubic { c1: b[1], c2: b[2], to: b[3] },
        });
    }
    let (first, last) = (ctrl[0], ctrl[ctrl.len() - 1]);
    let closed = flags & 1 == 1 && first.distance(last) <= 1e-12 * (1.0 + first.to_vector().length());
    let sp = Subpath::new(first, segs, closed).map_err(|_| MISSING)?;
    Ok(Path::from_subpaths(vec![sp]))
}

/// Bézier control polygons of a clamped B-spline, by inserting every interior knot up to
/// multiplicity `p` (Boehm). Each returned span has `p + 1` points.
fn bezier_spans(knots: &[f64], ctrl: &[Point], p: usize) -> Vec<Vec<Point>> {
    let mut t = knots.to_vec();
    let mut c = ctrl.to_vec();
    let (lo, hi) = (t[0], t[t.len() - 1]);
    let mut distinct: Vec<f64> = t.iter().copied().filter(|u| *u > lo && *u < hi).collect();
    distinct.dedup();
    for u in distinct {
        let mult = t.iter().filter(|k| **k == u).count();
        for _ in mult..p {
            insert_knot(&mut t, &mut c, p, u);
        }
    }
    // Clamped ends + all interior knots at multiplicity p: control points are p·spans + 1.
    let spans = (c.len() - 1) / p;
    (0..spans).map(|s| c[s * p..=s * p + p].to_vec()).collect()
}

/// Inserts `u` once into the knot vector `t` of a degree-`p` spline with control points `c`.
fn insert_knot(t: &mut Vec<f64>, c: &mut Vec<Point>, p: usize, u: f64) {
    // k: last index with t[k] <= u (u is strictly inside, so k is a valid span).
    let Some(k) = t.iter().rposition(|x| *x <= u) else { return };
    let s = t.iter().filter(|x| **x == u).count();
    let mut q = Vec::with_capacity(c.len() + 1);
    for i in 0..=c.len() {
        let point = if i + p <= k {
            c[i]
        } else if i > k - s {
            c[i - 1]
        } else {
            let a = (u - t[i]) / (t[i + p] - t[i]);
            c[i - 1].lerp(c[i], a)
        };
        q.push(point);
    }
    t.insert(k + 1, u);
    *c = q;
}
