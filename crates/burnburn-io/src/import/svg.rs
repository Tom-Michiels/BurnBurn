//! SVG import: paths, basic shapes, transforms, viewBox and units → world millimetres (y-up).
//!
//! The root `width`/`height`/`viewBox` define the physical scale (`preserveAspectRatio` default
//! `xMidYMid meet`, or `none`). The viewport's top-left corner maps to `(0, height)`, so the
//! drawing keeps its visual orientation and windings in the y-up world. Unsupported elements
//! and effects are reported, never silently dropped.

use std::f64::consts::{PI, TAU};

use burnburn_geometry::{Affine, EllipticalArc, Path, Point, Segment, Subpath, Vector, math};
use burnburn_types::{Angle, Unit};

use super::xml::{Attr, Event, Tokenizer};
use super::{
    ImportDraft, ImportedPath, ParseError, ParseLimits, SkipReason, Skipped, SvgUnitBasis, UnitDecision, check_size,
};

/// Millimetres per CSS pixel (1/96 in).
const MM_PER_PX: f64 = 25.4 / 96.0;

/// Fuzz entry point with default limits.
pub fn parse_svg(bytes: &[u8]) -> Result<ImportDraft, ParseError> {
    parse_svg_with(bytes, &ParseLimits::default())
}

/// Parses SVG bytes within `limits`. Never panics.
pub fn parse_svg_with(bytes: &[u8], limits: &ParseLimits) -> Result<ImportDraft, ParseError> {
    check_size(bytes, limits)?;
    let text = std::str::from_utf8(bytes).map_err(|_| ParseError::NotUtf8)?;
    let mut walker = Walker {
        limits,
        stack: Vec::new(),
        items: Vec::new(),
        skipped: Vec::new(),
        units: None,
        elements: 0,
        commands: 0,
    };
    let mut tok = Tokenizer::new(text);
    let mut saw_root = false;
    let mut open: Vec<&str> = Vec::new();
    while let Some(ev) = tok.next_event()? {
        match ev {
            Event::Start { name, attrs, self_closing } => {
                saw_root = true;
                walker.start(local(name), &attrs)?;
                if self_closing {
                    walker.stack.pop();
                } else {
                    open.push(name);
                }
            }
            Event::End { name } => {
                if open.pop() != Some(name) || walker.stack.pop().is_none() {
                    return Err(ParseError::Malformed { at: 0, reason: "end tag does not match the open element" });
                }
            }
            // Never produced by the default tokenizer.
            Event::Text { .. } => {}
        }
    }
    if !saw_root || !walker.stack.is_empty() {
        return Err(ParseError::Malformed { at: text.len(), reason: "unterminated or empty document" });
    }
    let units = walker
        .units
        .unwrap_or(UnitDecision::Svg { mm_per_user_unit: MM_PER_PX, basis: SvgUnitBasis::AssumedCssPixels });
    Ok(ImportDraft::new(walker.items, units, walker.skipped))
}

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

#[derive(Clone, Copy)]
struct Frame {
    ctm: Affine,
    /// Inside defs/clipPath/… or an unsupported element: nothing below is drawn.
    skip: bool,
}

struct Walker<'l> {
    limits: &'l ParseLimits,
    stack: Vec<Frame>,
    items: Vec<ImportedPath>,
    skipped: Vec<Skipped>,
    units: Option<UnitDecision>,
    elements: usize,
    commands: usize,
}

fn attr<'a>(attrs: &'a [Attr<'_>], name: &str) -> Option<&'a str> {
    attrs.iter().find(|a| a.name == name).map(|a| a.value.as_str())
}

/// `style="k: v; …"` declarations, lower-cased keys.
fn style<'a>(attrs: &'a [Attr<'_>], key: &str) -> Option<&'a str> {
    attr(attrs, "style")?
        .split(';')
        .filter_map(|d| d.split_once(':'))
        .find(|(k, _)| k.trim().eq_ignore_ascii_case(key))
        .map(|(_, v)| v.trim())
}

fn property<'a>(attrs: &'a [Attr<'_>], key: &str) -> Option<&'a str> {
    style(attrs, key).or_else(|| attr(attrs, key))
}

const SILENT_SKIP: [&str; 12] = [
    "defs",
    "style",
    "title",
    "desc",
    "metadata",
    "script",
    "linearGradient",
    "radialGradient",
    "filter",
    "clipPath",
    "mask",
    "pattern",
];
const CONTAINERS: [&str; 4] = ["g", "a", "switch", "svg"];
const SHAPES: [&str; 7] = ["path", "rect", "circle", "ellipse", "line", "polyline", "polygon"];
const EFFECTS: [&str; 3] = ["clip-path", "mask", "filter"];

impl Walker<'_> {
    fn start(&mut self, name: &str, attrs: &[Attr<'_>]) -> Result<(), ParseError> {
        self.elements += 1;
        if self.elements > self.limits.max_entities {
            return Err(ParseError::TooManyEntities { limit: self.limits.max_entities });
        }
        if self.stack.len() >= self.limits.max_depth {
            return Err(ParseError::TooDeep { limit: self.limits.max_depth });
        }
        let parent = self.stack.last().copied();
        let entity = attr(attrs, "id").map_or_else(|| format!("{name}#{}", self.elements), str::to_owned);
        let Some(parent) = parent else {
            // The root element must be <svg>.
            if name != "svg" {
                return Err(ParseError::Unsupported("root element is not <svg>"));
            }
            let base = self.root(attrs)?;
            self.stack.push(Frame { ctm: base, skip: false });
            return Ok(());
        };
        if parent.skip {
            self.stack.push(parent);
            return Ok(());
        }
        let own = match attr(attrs, "transform") {
            Some(t) => parse_transform(t)?,
            None => Affine::IDENTITY,
        };
        let mut ctm = own.then(&parent.ctm);
        let hidden = property(attrs, "display").is_some_and(|v| v == "none")
            || property(attrs, "visibility").is_some_and(|v| v == "hidden" || v == "collapse");
        if SILENT_SKIP.contains(&name) {
            self.stack.push(Frame { ctm, skip: true });
            return Ok(());
        }
        if hidden {
            self.report(&entity, name, SkipReason::Hidden);
            self.stack.push(Frame { ctm, skip: true });
            return Ok(());
        }
        if CONTAINERS.contains(&name) {
            if name == "svg" {
                // Nested viewport: only its position is applied.
                let x = attr(attrs, "x").and_then(user_length).unwrap_or(0.0);
                let y = attr(attrs, "y").and_then(user_length).unwrap_or(0.0);
                let m = Affine::translate(Vector::new(x, y)).map_err(|_| bad("nested svg position"))?;
                ctm = m.then(&ctm);
                if attr(attrs, "viewBox").is_some() {
                    self.report(&entity, name, SkipReason::UnsupportedEffect("nested viewBox"));
                }
            }
            self.effects(&entity, name, attrs);
            self.stack.push(Frame { ctm, skip: false });
            return Ok(());
        }
        if SHAPES.contains(&name) {
            self.stack.push(Frame { ctm, skip: true });
            match self.shape(name, attrs)? {
                Some(path) if !path.is_empty() => {
                    self.effects(&entity, name, attrs);
                    let label = attr(attrs, "id").unwrap_or(name).to_owned();
                    self.items.push(ImportedPath { name: label, entity, path: path.transform(&ctm), layer: None });
                }
                _ => self.report(&entity, name, SkipReason::Invalid("degenerate shape")),
            }
            return Ok(());
        }
        // text, image, use, foreignObject, symbol, marker, unknown elements.
        self.report(&entity, name, SkipReason::UnsupportedElement);
        self.stack.push(Frame { ctm, skip: true });
        Ok(())
    }

    fn report(&mut self, entity: &str, kind: &str, reason: SkipReason) {
        self.skipped.push(Skipped { entity: entity.to_owned(), kind: kind.to_owned(), reason });
    }

    fn effects(&mut self, entity: &str, kind: &str, attrs: &[Attr<'_>]) {
        for e in EFFECTS {
            if property(attrs, e).is_some_and(|v| v != "none") {
                self.report(entity, kind, SkipReason::UnsupportedEffect(e));
            }
        }
    }

    /// The root mapping user units → world mm, and the unit decision.
    fn root(&mut self, attrs: &[Attr<'_>]) -> Result<Affine, ParseError> {
        let w = attr(attrs, "width").and_then(physical_mm);
        let h = attr(attrs, "height").and_then(physical_mm);
        let vb = match attr(attrs, "viewBox") {
            Some(v) => {
                let n = numbers(v)?;
                match n.as_slice() {
                    [x, y, vw, vh] if *vw > 0.0 && *vh > 0.0 => Some((*x, *y, *vw, *vh)),
                    _ => return Err(bad("viewBox needs four numbers with positive size")),
                }
            }
            None => None,
        };
        let stretch = attr(attrs, "preserveAspectRatio").is_some_and(|p| p.trim().starts_with("none"));
        let (sx, sy, ox, oy, height, minx, miny, basis) = match (vb, w, h) {
            (Some((x, y, vw, vh)), w, h) if w.is_some() || h.is_some() => {
                // A missing dimension follows the viewBox aspect ratio.
                let wm = w.unwrap_or_else(|| h.unwrap_or(0.0) * vw / vh);
                let hm = h.unwrap_or(wm * vh / vw);
                let (sx, sy) = (wm / vw, hm / vh);
                if stretch {
                    (sx, sy, 0.0, 0.0, hm, x, y, SvgUnitBasis::ViewBoxToPhysicalSize)
                } else {
                    let s = sx.min(sy);
                    (s, s, (wm - vw * s) / 2.0, (hm - vh * s) / 2.0, hm, x, y, SvgUnitBasis::ViewBoxToPhysicalSize)
                }
            }
            (Some((x, y, _, vh)), _, _) => {
                (MM_PER_PX, MM_PER_PX, 0.0, 0.0, vh * MM_PER_PX, x, y, SvgUnitBasis::AssumedCssPixels)
            }
            (None, Some(_), hm) | (None, None, hm @ Some(_)) => {
                (MM_PER_PX, MM_PER_PX, 0.0, 0.0, hm.unwrap_or(0.0), 0.0, 0.0, SvgUnitBasis::CssPixelsBySpec)
            }
            (None, None, None) => (MM_PER_PX, MM_PER_PX, 0.0, 0.0, 0.0, 0.0, 0.0, SvgUnitBasis::AssumedCssPixels),
        };
        if !(sx > 0.0 && sy > 0.0) {
            return Err(bad("viewport has no size"));
        }
        self.units = Some(UnitDecision::Svg { mm_per_user_unit: sx, basis });
        if sx != sy {
            self.report("svg", "svg", SkipReason::UnsupportedEffect("non-uniform preserveAspectRatio scale"));
        }
        Affine::new([sx, 0.0, 0.0, -sy, ox - sx * minx, height - oy + sy * miny]).map_err(|_| bad("viewport mapping"))
    }

    fn count(&mut self, n: usize) -> Result<(), ParseError> {
        self.commands = self.commands.saturating_add(n);
        if self.commands > self.limits.max_path_commands {
            return Err(ParseError::TooManyCommands { limit: self.limits.max_path_commands });
        }
        Ok(())
    }

    /// Geometry of a basic shape in its own user space; `None` when degenerate.
    fn shape(&mut self, name: &str, attrs: &[Attr<'_>]) -> Result<Option<Path>, ParseError> {
        let len = |key: &str| attr(attrs, key).and_then(user_length);
        Ok(match name {
            "path" => {
                let d = attr(attrs, "d").unwrap_or("");
                Some(self.path_data(d)?)
            }
            "rect" => {
                let (x, y) = (len("x").unwrap_or(0.0), len("y").unwrap_or(0.0));
                let (Some(w), Some(h)) = (len("width"), len("height")) else { return Ok(None) };
                if !(w > 0.0 && h > 0.0) {
                    return Ok(None);
                }
                let (rx, ry) = match (len("rx"), len("ry")) {
                    (Some(a), Some(b)) => (a, b),
                    (Some(a), None) | (None, Some(a)) => (a, a),
                    (None, None) => (0.0, 0.0),
                };
                Some(rect_path(x, y, w, h, rx.clamp(0.0, w / 2.0), ry.clamp(0.0, h / 2.0))?)
            }
            "circle" => {
                let r = len("r").unwrap_or(0.0);
                Path::circle(Point::new(len("cx").unwrap_or(0.0), len("cy").unwrap_or(0.0)), r).ok()
            }
            "ellipse" => {
                let c = Point::new(len("cx").unwrap_or(0.0), len("cy").unwrap_or(0.0));
                Path::ellipse(c, len("rx").unwrap_or(0.0), len("ry").unwrap_or(0.0), Angle::ZERO).ok()
            }
            "line" => {
                let a = Point::new(len("x1").unwrap_or(0.0), len("y1").unwrap_or(0.0));
                let b = Point::new(len("x2").unwrap_or(0.0), len("y2").unwrap_or(0.0));
                Path::line(a, b).ok()
            }
            "polyline" | "polygon" => {
                let n = numbers(attr(attrs, "points").unwrap_or(""))?;
                self.count(n.len() / 2)?;
                let pts: Vec<Point> = n.as_chunks::<2>().0.iter().map(|[x, y]| Point::new(*x, *y)).collect();
                Path::polyline(&pts, name == "polygon").ok()
            }
            _ => None,
        })
    }

    fn path_data(&mut self, d: &str) -> Result<Path, ParseError> {
        let mut p = DataScanner { s: d.as_bytes(), pos: 0 };
        let mut b = Builder::default();
        let mut cmd: Option<u8> = None;
        loop {
            p.skip_separators();
            let Some(&c) = p.s.get(p.pos) else { break };
            let letter = if c.is_ascii_alphabetic() {
                p.pos += 1;
                c
            } else {
                // Implicit repetition; after a moveto it is a lineto.
                match cmd {
                    Some(b'M') => b'L',
                    Some(b'm') => b'l',
                    Some(prev) if !matches!(prev, b'Z' | b'z') => prev,
                    _ => {
                        return Err(ParseError::Malformed { at: p.pos, reason: "path data must start with a command" });
                    }
                }
            };
            self.count(1)?;
            let rel = letter.is_ascii_lowercase();
            let cur = b.current;
            let at = |v: Point| if rel { Point::new(cur.x + v.x, cur.y + v.y) } else { v };
            match letter.to_ascii_uppercase() {
                b'M' => {
                    let q = at(p.point()?);
                    b.move_to(q);
                }
                b'L' => {
                    let q = at(p.point()?);
                    b.line_to(q);
                }
                b'H' => {
                    let x = p.number()?;
                    let q = Point::new(if rel { cur.x + x } else { x }, cur.y);
                    b.line_to(q);
                }
                b'V' => {
                    let y = p.number()?;
                    let q = Point::new(cur.x, if rel { cur.y + y } else { y });
                    b.line_to(q);
                }
                b'C' => {
                    let (c1, c2, q) = (at(p.point()?), at(p.point()?), at(p.point()?));
                    b.cubic_to(c1, c2, q);
                }
                b'S' => {
                    let c1 = b.reflected_cubic();
                    let (c2, q) = (at(p.point()?), at(p.point()?));
                    b.cubic_to(c1, c2, q);
                }
                b'Q' => {
                    let (c, q) = (at(p.point()?), at(p.point()?));
                    b.quad_to(c, q);
                }
                b'T' => {
                    let c = b.reflected_quad();
                    let q = at(p.point()?);
                    b.quad_to(c, q);
                }
                b'A' => {
                    let (rx, ry, rot) = (p.number()?, p.number()?, p.number()?);
                    let (large, sweep) = (p.flag()?, p.flag()?);
                    let q = at(p.point()?);
                    b.arc_to(rx, ry, rot, large, sweep, q);
                }
                b'Z' => b.close(),
                _ => return Err(ParseError::Malformed { at: p.pos, reason: "unknown path command" }),
            }
            cmd = Some(letter);
        }
        Ok(b.finish())
    }
}

fn bad(reason: &'static str) -> ParseError {
    ParseError::Malformed { at: 0, reason }
}

/// A plain or rounded rectangle in user space (y-down), as the file draws it.
fn rect_path(x: f64, y: f64, w: f64, h: f64, rx: f64, ry: f64) -> Result<Path, ParseError> {
    if !(rx > 0.0 && ry > 0.0) {
        let pts = [Point::new(x, y), Point::new(x + w, y), Point::new(x + w, y + h), Point::new(x, y + h)];
        return Path::polyline(&pts, true).map_err(|_| bad("rect"));
    }
    let corner = |cx: f64, cy: f64, start_deg: f64| -> Result<Segment, ParseError> {
        let start = Angle::from_degrees(start_deg).map_err(|_| bad("rect"))?;
        let sweep = Angle::from_degrees(90.0).map_err(|_| bad("rect"))?;
        EllipticalArc::new(Point::new(cx, cy), Vector::new(rx, 0.0), Vector::new(0.0, ry), start, sweep)
            .map(Segment::Arc)
            .map_err(|_| bad("rect"))
    };
    let segs = vec![
        Segment::Line { to: Point::new(x + w - rx, y) },
        corner(x + w - rx, y + ry, -90.0)?,
        Segment::Line { to: Point::new(x + w, y + h - ry) },
        corner(x + w - rx, y + h - ry, 0.0)?,
        Segment::Line { to: Point::new(x + rx, y + h) },
        corner(x + rx, y + h - ry, 90.0)?,
        Segment::Line { to: Point::new(x, y + ry) },
        corner(x + rx, y + ry, 180.0)?,
    ];
    Subpath::new(Point::new(x + rx, y), segs, true).map(|s| Path::from_subpaths(vec![s])).map_err(|_| bad("rect"))
}

/// A length in user units: plain numbers and `px` as given; absolute units via CSS pixels.
fn user_length(v: &str) -> Option<f64> {
    let (n, unit) = split_unit(v)?;
    let mm_per = match unit {
        "" | "px" => return Some(n),
        "mm" => 1.0,
        "cm" => 10.0,
        "in" => Unit::Inch.mm_per_unit(),
        "pt" => 25.4 / 72.0,
        "pc" => 25.4 / 6.0,
        _ => return None,
    };
    Some(n * mm_per / MM_PER_PX)
}

/// A root `width`/`height` in millimetres; `None` for relative units (%, em).
fn physical_mm(v: &str) -> Option<f64> {
    let (n, unit) = split_unit(v)?;
    let mm = match unit {
        "" | "px" => n * MM_PER_PX,
        "mm" => n,
        "cm" => n * 10.0,
        "in" => n * Unit::Inch.mm_per_unit(),
        "pt" => n * 25.4 / 72.0,
        "pc" => n * 25.4 / 6.0,
        _ => return None,
    };
    (mm.is_finite() && mm > 0.0).then_some(mm)
}

fn split_unit(v: &str) -> Option<(f64, &str)> {
    let v = v.trim();
    let end = v.find(|c: char| c.is_ascii_alphabetic() || c == '%').unwrap_or(v.len());
    let (num, unit) = v.split_at(end);
    let n: f64 = num.trim().parse().ok()?;
    n.is_finite().then_some((n, unit.trim()))
}

fn numbers(s: &str) -> Result<Vec<f64>, ParseError> {
    let mut p = DataScanner { s: s.as_bytes(), pos: 0 };
    let mut out = Vec::new();
    loop {
        p.skip_separators();
        if p.pos >= p.s.len() {
            return Ok(out);
        }
        out.push(p.number()?);
    }
}

/// `transform` list: matrix, translate, scale, rotate, skewX, skewY (applied right to left).
fn parse_transform(s: &str) -> Result<Affine, ParseError> {
    let mut m = Affine::IDENTITY;
    let mut rest = s.trim();
    while !rest.is_empty() {
        let open = rest.find('(').ok_or(bad("transform function without '('"))?;
        let close = rest.find(')').ok_or(bad("transform function without ')'"))?;
        if close < open {
            return Err(bad("transform syntax"));
        }
        let name = rest.get(..open).unwrap_or("").trim().trim_start_matches(',').trim();
        let args = numbers(rest.get(open + 1..close).unwrap_or(""))?;
        let deg = |a: f64| a * PI / 180.0;
        let t = match (name, args.as_slice()) {
            ("matrix", [a, b, c, d, e, f]) => Affine::new([*a, *b, *c, *d, *e, *f]),
            ("translate", [x]) => Affine::translate(Vector::new(*x, 0.0)),
            ("translate", [x, y]) => Affine::translate(Vector::new(*x, *y)),
            ("scale", [s]) => Affine::scale(*s, *s),
            ("scale", [x, y]) => Affine::scale(*x, *y),
            ("rotate", [a]) => Angle::from_radians(deg(*a))
                .map(Affine::rotate)
                .map_err(|_| burnburn_geometry::GeometryError::NotFinite),
            ("rotate", [a, cx, cy]) => {
                let r = Angle::from_radians(deg(*a)).map(Affine::rotate).map_err(|_| bad("rotate"))?;
                let to = Affine::translate(Vector::new(-cx, -cy)).map_err(|_| bad("rotate"))?;
                let back = Affine::translate(Vector::new(*cx, *cy)).map_err(|_| bad("rotate"))?;
                Ok(to.then(&r).then(&back))
            }
            ("skewX", [a]) => Affine::new([1.0, 0.0, math::tan(deg(*a)), 1.0, 0.0, 0.0]),
            ("skewY", [a]) => Affine::new([1.0, math::tan(deg(*a)), 0.0, 1.0, 0.0, 0.0]),
            _ => return Err(bad("unknown transform function or argument count")),
        }
        .map_err(|_| bad("transform is not finite"))?;
        // "A B" means A(B(p)): the later function applies first.
        m = t.then(&m);
        rest = rest.get(close + 1..).unwrap_or("").trim();
    }
    Ok(m)
}

struct DataScanner<'a> {
    s: &'a [u8],
    pos: usize,
}

impl DataScanner<'_> {
    fn skip_separators(&mut self) {
        while self.s.get(self.pos).is_some_and(|b| b.is_ascii_whitespace() || *b == b',') {
            self.pos += 1;
        }
    }

    fn number(&mut self) -> Result<f64, ParseError> {
        self.skip_separators();
        let start = self.pos;
        let at = |p: usize| self.s.get(p).copied();
        let mut p = start;
        if matches!(at(p), Some(b'+' | b'-')) {
            p += 1;
        }
        let digits = |mut p: usize| {
            while at(p).is_some_and(|b| b.is_ascii_digit()) {
                p += 1;
            }
            p
        };
        let int_end = digits(p);
        let mut end = int_end;
        if at(end) == Some(b'.') {
            end = digits(end + 1);
        }
        if end == p || (end == p + 1 && at(p) == Some(b'.')) {
            return Err(ParseError::Malformed { at: start, reason: "expected a number" });
        }
        if matches!(at(end), Some(b'e' | b'E')) {
            let mut e = end + 1;
            if matches!(at(e), Some(b'+' | b'-')) {
                e += 1;
            }
            let e_end = digits(e);
            if e_end > e {
                end = e_end;
            }
        }
        let text = std::str::from_utf8(self.s.get(start..end).unwrap_or(b"")).map_err(|_| ParseError::NotUtf8)?;
        let v: f64 = text.parse().map_err(|_| ParseError::Malformed { at: start, reason: "expected a number" })?;
        if !v.is_finite() {
            return Err(ParseError::Malformed { at: start, reason: "number out of range" });
        }
        self.pos = end;
        Ok(v)
    }

    fn point(&mut self) -> Result<Point, ParseError> {
        Ok(Point::new(self.number()?, self.number()?))
    }

    /// Arc flags are a single `0`/`1`, possibly without a separator.
    fn flag(&mut self) -> Result<bool, ParseError> {
        self.skip_separators();
        let v = match self.s.get(self.pos) {
            Some(b'0') => false,
            Some(b'1') => true,
            _ => return Err(ParseError::Malformed { at: self.pos, reason: "expected an arc flag" }),
        };
        self.pos += 1;
        Ok(v)
    }
}

/// Assembles subpaths exactly (arcs keep their own start; no connecting line is invented).
#[derive(Default)]
struct Builder {
    done: Vec<Subpath>,
    start: Point,
    segs: Vec<Segment>,
    open: bool,
    current: Point,
    last_cubic_c2: Option<Point>,
    last_quad_c: Option<Point>,
}

impl Builder {
    fn flush(&mut self, closed: bool) {
        if self.open
            && !self.segs.is_empty()
            && let Ok(sp) = Subpath::new(self.start, std::mem::take(&mut self.segs), closed)
        {
            self.done.push(sp);
        }
        self.segs.clear();
        self.open = false;
    }

    fn ensure_open(&mut self) {
        if !self.open {
            self.start = self.current;
            self.open = true;
        }
    }

    fn move_to(&mut self, p: Point) {
        self.flush(false);
        self.current = p;
        self.start = p;
        self.open = true;
        self.last_cubic_c2 = None;
        self.last_quad_c = None;
    }

    fn push(&mut self, seg: Segment) {
        self.ensure_open();
        self.current = seg.end();
        self.segs.push(seg);
    }

    fn line_to(&mut self, p: Point) {
        self.push(Segment::Line { to: p });
        self.last_cubic_c2 = None;
        self.last_quad_c = None;
    }

    fn cubic_to(&mut self, c1: Point, c2: Point, p: Point) {
        self.push(Segment::Cubic { c1, c2, to: p });
        self.last_cubic_c2 = Some(c2);
        self.last_quad_c = None;
    }

    fn reflected_cubic(&self) -> Point {
        self.last_cubic_c2.map_or(self.current, |c| self.current + (self.current - c))
    }

    fn reflected_quad(&self) -> Point {
        self.last_quad_c.map_or(self.current, |c| self.current + (self.current - c))
    }

    /// Quadratics are elevated to cubics exactly.
    fn quad_to(&mut self, c: Point, p: Point) {
        let p0 = self.current;
        let c1 = p0 + (c - p0) * (2.0 / 3.0);
        let c2 = p + (c - p) * (2.0 / 3.0);
        self.push(Segment::Cubic { c1, c2, to: p });
        self.last_cubic_c2 = None;
        self.last_quad_c = Some(c);
    }

    /// SVG endpoint arc (implementation notes F.6.5) as an exact elliptical arc.
    fn arc_to(&mut self, rx: f64, ry: f64, rot_deg: f64, large: bool, sweep: bool, p: Point) {
        self.last_cubic_c2 = None;
        self.last_quad_c = None;
        let p1 = self.current;
        if p1 == p {
            return;
        }
        let (mut rx, mut ry) = (rx.abs(), ry.abs());
        if !(rx > 0.0 && ry > 0.0) {
            self.line_to(p);
            return;
        }
        let phi = rot_deg * PI / 180.0;
        let (sin, cos) = math::sin_cos(phi);
        let (dx, dy) = ((p1.x - p.x) / 2.0, (p1.y - p.y) / 2.0);
        let (x1, y1) = (cos * dx + sin * dy, -sin * dx + cos * dy);
        let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
        if lambda > 1.0 {
            let s = lambda.sqrt();
            rx *= s;
            ry *= s;
        }
        let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
        let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
        let mut coef = if den > 0.0 { (num / den).max(0.0).sqrt() } else { 0.0 };
        if large == sweep {
            coef = -coef;
        }
        let (cx1, cy1) = (coef * rx * y1 / ry, -coef * ry * x1 / rx);
        let c = Point::new(cos * cx1 - sin * cy1 + (p1.x + p.x) / 2.0, sin * cx1 + cos * cy1 + (p1.y + p.y) / 2.0);
        let angle = |ux: f64, uy: f64, vx: f64, vy: f64| math::atan2(ux * vy - uy * vx, ux * vx + uy * vy);
        let (ux, uy) = ((x1 - cx1) / rx, (y1 - cy1) / ry);
        let (vx, vy) = ((-x1 - cx1) / rx, (-y1 - cy1) / ry);
        let theta = angle(1.0, 0.0, ux, uy);
        let mut delta = angle(ux, uy, vx, vy) % TAU;
        if !sweep && delta > 0.0 {
            delta -= TAU;
        } else if sweep && delta < 0.0 {
            delta += TAU;
        }
        let u = Vector::new(rx * cos, rx * sin);
        let v = Vector::new(-ry * sin, ry * cos);
        let arc = Angle::from_radians(theta)
            .and_then(|t| Angle::from_radians(delta).map(|d| (t, d)))
            .ok()
            .and_then(|(t, d)| EllipticalArc::new(c, u, v, t, d).ok());
        match arc {
            Some(a) => {
                self.ensure_open();
                self.segs.push(Segment::Arc(a));
                // The exact endpoint, not the arc's recomputed one, so following segments join.
                self.current = p;
            }
            None => self.line_to(p),
        }
    }

    fn close(&mut self) {
        let start = self.start;
        self.flush(true);
        self.current = start;
        self.last_cubic_c2 = None;
        self.last_quad_c = None;
    }

    fn finish(mut self) -> Path {
        self.flush(false);
        Path::from_subpaths(self.done)
    }
}
