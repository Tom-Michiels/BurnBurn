//! Content-stream interpreter (ISO 32000-1 §8–9): path construction and painting operators with
//! the graphics-state matrix, form XObjects, optional-content visibility and a report for
//! everything that is not vector geometry. Bounded by operator, segment and nesting limits.

use std::collections::HashSet;

use burnburn_geometry::{Path, Point, Segment, Subpath};

use super::file::{PdfFile, StreamError};
use super::object::{Dict, Lexer, Obj, Tok};
use crate::import::{ImportLayer, ImportedPath, ParseError, ParseLimits, SkipReason, Skipped};

pub(super) type Matrix = [f64; 6];

pub(super) const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// `a` first, then `b` (PDF row-vector convention: `a × b`).
pub(super) fn then(a: &Matrix, b: &Matrix) -> Matrix {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

fn apply(m: &Matrix, x: f64, y: f64) -> Point {
    Point::new(m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

#[derive(Clone, Copy)]
struct Gs {
    ctm: Matrix,
    stroke: [u8; 3],
    fill: [u8; 3],
}

struct Building {
    start: Point,
    segments: Vec<Segment>,
    closed: bool,
}

pub(super) struct Interp<'f, 'a> {
    file: &'f PdfFile<'a>,
    limits: &'f ParseLimits,
    page: usize,
    hidden_ocgs: &'f HashSet<u32>,
    pub(super) items: Vec<ImportedPath>,
    pub(super) skipped: Vec<Skipped>,
    ops: usize,
    segs: usize,
    active_forms: Vec<u32>,
}

impl<'f, 'a> Interp<'f, 'a> {
    pub(super) fn new(
        file: &'f PdfFile<'a>,
        limits: &'f ParseLimits,
        page: usize,
        hidden_ocgs: &'f HashSet<u32>,
    ) -> Self {
        Interp {
            file,
            limits,
            page,
            hidden_ocgs,
            items: Vec::new(),
            skipped: Vec::new(),
            ops: 0,
            segs: 0,
            active_forms: Vec::new(),
        }
    }

    pub(super) fn report(&mut self, kind: &str, reason: SkipReason) {
        if !self.skipped.iter().any(|s| s.kind == kind && s.reason == reason) {
            self.skipped.push(Skipped { entity: format!("p{}:{kind}", self.page), kind: kind.to_owned(), reason });
        }
    }

    pub(super) fn stream_problem(&mut self, e: StreamError) {
        let reason = match e {
            StreamError::TooLarge => SkipReason::Invalid("content stream exceeds the decoded size limit"),
            StreamError::Damaged => SkipReason::Invalid("content stream could not be decoded"),
            StreamError::UnsupportedFilter => SkipReason::Invalid("content stream uses an unsupported filter"),
        };
        self.report("content stream", reason);
    }

    /// Runs the page's content with `base` mapping default user space to world mm.
    pub(super) fn run_page(&mut self, content: &[u8], resources: &Dict, base: Matrix) -> Result<(), ParseError> {
        let gs = Gs { ctm: base, stroke: [0; 3], fill: [0; 3] };
        self.run(content, resources, gs, 0)
    }

    fn count_op(&mut self) -> Result<(), ParseError> {
        self.ops += 1;
        if self.ops > self.limits.max_entities {
            return Err(ParseError::TooManyEntities { limit: self.limits.max_entities });
        }
        Ok(())
    }

    fn count_seg(&mut self) -> Result<(), ParseError> {
        self.segs += 1;
        if self.segs > self.limits.max_path_commands {
            return Err(ParseError::TooManyCommands { limit: self.limits.max_path_commands });
        }
        Ok(())
    }

    fn run(&mut self, content: &[u8], resources: &Dict, start: Gs, depth: usize) -> Result<(), ParseError> {
        let mut lex = Lexer::new(content, 0);
        lex.refs = false;
        let mut gs = start;
        let mut stack: Vec<Gs> = Vec::new();
        let mut operands: Vec<Obj> = Vec::new();
        let mut subpaths: Vec<Subpath> = Vec::new();
        let mut current: Option<Building> = None;
        let mut cur_pt: Option<Point> = None;
        let mut mc: Vec<bool> = Vec::new();
        let mut hidden_depth = 0usize;
        let mut clip_pending = false;
        loop {
            let tok = lex.token();
            let op: &[u8] = match tok {
                Tok::Eof => break,
                Tok::Kw(k) if !matches!(k, b"true" | b"false" | b"null") => k,
                t => {
                    if let Some(o) = lex.object_from(t, 0) {
                        if operands.len() >= 64 {
                            operands.remove(0);
                        }
                        operands.push(o);
                    }
                    continue;
                }
            };
            self.count_op()?;
            let n = |i: usize| operands.get(i).and_then(Obj::as_f64);
            let nums = operands.iter().filter_map(Obj::as_f64).collect::<Vec<_>>();
            match op {
                b"q" => {
                    if stack.len() < 256 {
                        stack.push(gs);
                    } else {
                        self.report("q", SkipReason::Invalid("graphics state nested too deep"));
                    }
                }
                b"Q" => {
                    if let Some(s) = stack.pop() {
                        gs = s;
                    }
                }
                b"cm" => {
                    if let [a, b, c, d, e, f] = nums[..] {
                        gs.ctm = then(&[a, b, c, d, e, f], &gs.ctm);
                    }
                }
                b"m" => {
                    self.count_seg()?;
                    if let (Some(x), Some(y)) = (n(0), n(1)) {
                        finish(&mut current, &mut subpaths);
                        let p = apply(&gs.ctm, x, y);
                        cur_pt = Some(p);
                        current = Some(Building { start: p, segments: Vec::new(), closed: false });
                    }
                }
                b"l" => {
                    self.count_seg()?;
                    if let (Some(x), Some(y)) = (n(0), n(1)) {
                        let to = apply(&gs.ctm, x, y);
                        push_seg(&mut current, &mut cur_pt, Segment::Line { to });
                    }
                }
                b"c" | b"v" | b"y" => {
                    self.count_seg()?;
                    let q = |i: usize| (n(i), n(i + 1));
                    let pts = match (op, nums.len()) {
                        (b"c", 6) => Some((q(0), q(2), q(4))),
                        (b"v", 4) => Some(((None, None), q(0), q(2))),
                        (b"y", 4) => Some((q(0), q(2), q(2))),
                        _ => None,
                    };
                    if let Some((p1, p2, p3)) = pts
                        && let ((x2, y2), (x3, y3)) = (p2, p3)
                        && let (Some(x2), Some(y2), Some(x3), Some(y3)) = (x2, y2, x3, y3)
                    {
                        let to = apply(&gs.ctm, x3, y3);
                        let c2 = apply(&gs.ctm, x2, y2);
                        let c1 = match p1 {
                            (Some(x1), Some(y1)) => Some(apply(&gs.ctm, x1, y1)),
                            _ => cur_pt,
                        };
                        if let Some(c1) = c1 {
                            push_seg(&mut current, &mut cur_pt, Segment::Cubic { c1, c2, to });
                        }
                    }
                }
                b"h" => {
                    if let Some(mut b) = current.take()
                        && !b.segments.is_empty()
                    {
                        b.closed = true;
                        cur_pt = Some(b.start);
                        if let Ok(sp) = Subpath::new(b.start, b.segments, true) {
                            subpaths.push(sp);
                        }
                    } else {
                        current = None;
                    }
                }
                b"re" => {
                    self.count_seg()?;
                    if let [x, y, w, h] = nums[..] {
                        finish(&mut current, &mut subpaths);
                        let p = [
                            apply(&gs.ctm, x, y),
                            apply(&gs.ctm, x + w, y),
                            apply(&gs.ctm, x + w, y + h),
                            apply(&gs.ctm, x, y + h),
                        ];
                        let segs =
                            vec![Segment::Line { to: p[1] }, Segment::Line { to: p[2] }, Segment::Line { to: p[3] }];
                        if let Ok(sp) = Subpath::new(p[0], segs, true) {
                            subpaths.push(sp);
                        }
                        cur_pt = Some(p[0]);
                    }
                }
                b"W" | b"W*" => clip_pending = true,
                b"S" | b"s" | b"f" | b"F" | b"f*" | b"B" | b"B*" | b"b" | b"b*" | b"n" => {
                    if matches!(op, b"s" | b"b" | b"b*")
                        && let Some(mut b) = current.take()
                        && !b.segments.is_empty()
                    {
                        b.closed = true;
                        if let Ok(sp) = Subpath::new(b.start, b.segments, true) {
                            subpaths.push(sp);
                        }
                    }
                    finish(&mut current, &mut subpaths);
                    let taken = std::mem::take(&mut subpaths);
                    cur_pt = None;
                    if clip_pending {
                        self.report("clip", SkipReason::UnsupportedEffect("clipping path is not applied"));
                        clip_pending = false;
                    }
                    if op == b"n" || taken.is_empty() {
                        continue;
                    }
                    if hidden_depth > 0 {
                        self.report("optional content", SkipReason::Hidden);
                        continue;
                    }
                    let stroke = matches!(op, b"S" | b"s" | b"B" | b"B*" | b"b" | b"b*");
                    let (kind, rgb) = if stroke { ("stroke", gs.stroke) } else { ("fill", gs.fill) };
                    let layer = if stroke { ImportLayer::Stroke(rgb) } else { ImportLayer::Fill(rgb) };
                    let name = format!("{kind} #{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
                    let entity = format!("p{}#{}", self.page, self.items.len() + 1);
                    self.items.push(ImportedPath {
                        name,
                        entity,
                        path: Path::from_subpaths(taken),
                        layer: Some(layer),
                    });
                }
                b"g" | b"G" => {
                    if let Some(v) = n(0) {
                        let c = unit_to_u8(v);
                        set_colour(&mut gs, op == b"G", [c; 3]);
                    }
                }
                b"rg" | b"RG" => {
                    if let [r, g, b] = nums[..] {
                        set_colour(&mut gs, op == b"RG", [unit_to_u8(r), unit_to_u8(g), unit_to_u8(b)]);
                    }
                }
                b"k" | b"K" => {
                    if let [c, m, y, k] = nums[..] {
                        let f = |v: f64| unit_to_u8((1.0 - v.clamp(0.0, 1.0)) * (1.0 - k.clamp(0.0, 1.0)));
                        set_colour(&mut gs, op == b"K", [f(c), f(m), f(y)]);
                    }
                }
                b"BMC" => mc.push(false),
                b"BDC" => {
                    let hidden = operands.first().and_then(Obj::as_name) == Some("OC")
                        && self.oc_hidden(operands.get(1), resources);
                    if hidden {
                        hidden_depth += 1;
                    }
                    mc.push(hidden);
                }
                b"EMC" => {
                    if mc.pop() == Some(true) {
                        hidden_depth = hidden_depth.saturating_sub(1);
                    }
                }
                b"BT" => self.report("text", SkipReason::UnsupportedElement),
                b"Do" => {
                    if let Some(name) = operands.first().and_then(Obj::as_name).map(str::to_owned) {
                        self.xobject(&name, resources, &gs, hidden_depth > 0, depth)?;
                    }
                }
                b"sh" => self.report("shading", SkipReason::UnsupportedElement),
                b"BI" => {
                    self.report("image", SkipReason::UnsupportedElement);
                    skip_inline_image(&mut lex);
                }
                // Text and state operators that do not influence vector geometry.
                b"ET" | b"Tj" | b"TJ" | b"'" | b"\"" | b"Td" | b"TD" | b"Tm" | b"T*" | b"Tc" | b"Tw" | b"Tz"
                | b"TL" | b"Tf" | b"Tr" | b"Ts" | b"w" | b"J" | b"j" | b"M" | b"d" | b"ri" | b"i" | b"gs" | b"cs"
                | b"CS" | b"sc" | b"SC" | b"scn" | b"SCN" | b"MP" | b"DP" | b"BX" | b"EX" | b"d0" | b"d1" => {}
                other => {
                    let name = String::from_utf8_lossy(other).chars().take(8).collect::<String>();
                    self.report(&name, SkipReason::UnsupportedElement);
                }
            }
            operands.clear();
        }
        Ok(())
    }

    fn oc_hidden(&self, prop: Option<&Obj>, resources: &Dict) -> bool {
        let Some(Obj::Name(name)) = prop else { return false };
        let Some(props) = self.file.get(resources, "Properties") else { return false };
        let Some(Obj::Ref(r)) = props.dict().and_then(|d| d.get(name.as_str())) else { return false };
        self.ocg_group_hidden(*r)
    }

    /// An OCG is hidden when it is in the off set; an OCMD when all its groups are.
    fn ocg_group_hidden(&self, r: u32) -> bool {
        if self.hidden_ocgs.contains(&r) {
            return true;
        }
        let Some(obj) = self.file.object(r) else { return false };
        let Some(d) = obj.dict() else { return false };
        if d.get("Type").and_then(Obj::as_name) != Some("OCMD") {
            return false;
        }
        let refs: Vec<u32> = match d.get("OCGs") {
            Some(Obj::Ref(n)) => vec![*n],
            Some(Obj::Array(a)) => a.iter().filter_map(|o| if let Obj::Ref(n) = o { Some(*n) } else { None }).collect(),
            _ => Vec::new(),
        };
        !refs.is_empty() && refs.iter().all(|n| self.hidden_ocgs.contains(n))
    }

    fn xobject(&mut self, name: &str, resources: &Dict, gs: &Gs, hidden: bool, depth: usize) -> Result<(), ParseError> {
        let Some(xobjects) = self.file.get(resources, "XObject") else { return Ok(()) };
        let Some(entry) = xobjects.dict().and_then(|d| d.get(name)) else { return Ok(()) };
        let number = if let Obj::Ref(n) = entry { Some(*n) } else { None };
        let Some(obj) = self.file.resolve(entry) else { return Ok(()) };
        let Obj::Stream(dict, _) = &*obj else { return Ok(()) };
        if let Some(Obj::Ref(oc)) = dict.get("OC")
            && self.ocg_group_hidden(*oc)
        {
            self.report("optional content", SkipReason::Hidden);
            return Ok(());
        }
        match dict.get("Subtype").and_then(Obj::as_name) {
            Some("Image") => self.report("image", SkipReason::UnsupportedElement),
            Some("Form") => {
                if depth >= self.limits.max_depth || number.is_some_and(|n| self.active_forms.contains(&n)) {
                    self.report("form", SkipReason::Invalid("form XObject recursion"));
                    return Ok(());
                }
                let matrix = match dict.get("Matrix").and_then(|m| self.file.resolve(m)) {
                    Some(m) => match &*m {
                        Obj::Array(a) => {
                            let v: Vec<f64> = a.iter().filter_map(Obj::as_f64).collect();
                            <Matrix>::try_from(v).unwrap_or(IDENTITY)
                        }
                        _ => IDENTITY,
                    },
                    None => IDENTITY,
                };
                let content = match self.file.decode_stream(&obj) {
                    Ok(c) => c,
                    Err(e) => {
                        self.stream_problem(e);
                        return Ok(());
                    }
                };
                let own = self.file.get(dict, "Resources");
                let res = own.as_deref().and_then(Obj::dict).unwrap_or(resources);
                let inner = Gs { ctm: then(&matrix, &gs.ctm), ..*gs };
                if let Some(n) = number {
                    self.active_forms.push(n);
                }
                // A hidden call site hides the whole form: run it with a hidden marker by skipping it.
                let result = if hidden {
                    self.report("optional content", SkipReason::Hidden);
                    Ok(())
                } else {
                    self.run(&content, res, inner, depth + 1)
                };
                if number.is_some() {
                    self.active_forms.pop();
                }
                result?;
            }
            _ => self.report("xobject", SkipReason::UnsupportedElement),
        }
        Ok(())
    }
}

fn unit_to_u8(v: f64) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn set_colour(gs: &mut Gs, stroke: bool, rgb: [u8; 3]) {
    if stroke {
        gs.stroke = rgb;
    } else {
        gs.fill = rgb;
    }
}

fn push_seg(current: &mut Option<Building>, cur_pt: &mut Option<Point>, seg: Segment) {
    let end = seg.end();
    match current {
        Some(b) => b.segments.push(seg),
        None => {
            // A segment after `h`/painting continues from the current point.
            if let Some(start) = *cur_pt {
                *current = Some(Building { start, segments: vec![seg], closed: false });
            }
        }
    }
    *cur_pt = Some(end);
}

fn finish(current: &mut Option<Building>, subpaths: &mut Vec<Subpath>) {
    if let Some(b) = current.take()
        && !b.segments.is_empty()
        && let Ok(sp) = Subpath::new(b.start, b.segments, b.closed)
    {
        subpaths.push(sp);
    }
}

/// Skips `… ID <data> EI` after `BI`: the end is whitespace + `EI` + whitespace/delimiter/end.
fn skip_inline_image(lex: &mut Lexer<'_>) {
    loop {
        match lex.token() {
            Tok::Kw(b"ID") => break,
            Tok::Eof => return,
            _ => {}
        }
    }
    let data = lex.data;
    let mut i = lex.pos + 1;
    while i + 1 < data.len() {
        let before = i == 0 || data[i - 1].is_ascii_whitespace() || data[i - 1] == 0;
        let after = data.get(i + 2).is_none_or(|c| c.is_ascii_whitespace() || *c == 0 || *c == b'Q');
        if data[i] == b'E' && data[i + 1] == b'I' && before && after {
            lex.pos = i + 2;
            return;
        }
        i += 1;
    }
    lex.pos = data.len();
}
