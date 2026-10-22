//! HP-GL / PLT import (W2-L6, P03).
//!
//! Plotter units are 0.025 mm (1016 per inch, 40 per mm); `IP` + `SC` map user units onto
//! them (anisotropic scaling only). Supported: `IN`/`DF`, `SP`, `PU`/`PD`, `PA`/`PR`, `CI`,
//! `AA`/`AR`, `IP`, `SC`; `PJL`/escape wrappers are skipped. Everything else is named in the
//! import report, once per command kind: labels (`LB`), encoded polylines (`PE`), polygon
//! mode (drawn as outline), pen/line attributes, unknown mnemonics. Pen-down runs become one
//! [`ImportedPath`] each, named after the pen; a run that ends on its start point is closed.

use burnburn_geometry::{EllipticalArc, Path, Point, Segment, Subpath, math};
use burnburn_types::Angle;

use super::check_size;
use super::{
    HpglUnitBasis, ImportDraft, ImportLayer, ImportedPath, ParseError, ParseLimits, SkipReason, Skipped, UnitDecision,
};

/// One plotter unit in millimetres.
const MM_PER_UNIT: f64 = 0.025;
/// Larger coordinates (in the file's units) are rejected as garbage.
const COORD_LIMIT: f64 = 1.0e9;
/// A run whose end is this close to its start is closed.
const CLOSE_EPS_MM: f64 = 1e-9;

/// Fuzz entry point with default limits.
pub fn parse_hpgl(bytes: &[u8]) -> Result<ImportDraft, ParseError> {
    parse_hpgl_with(bytes, &ParseLimits::default())
}

/// Parses HP-GL bytes within `limits`. Never panics.
pub fn parse_hpgl_with(bytes: &[u8], limits: &ParseLimits) -> Result<ImportDraft, ParseError> {
    check_size(bytes, limits)?;
    let mut st = State::new(limits);
    st.run(bytes)?;
    Ok(st.finish())
}

struct Run {
    pen: u32,
    start: Point,
    segments: Vec<Segment>,
}

struct State<'l> {
    limits: &'l ParseLimits,
    pen: u32,
    down: bool,
    relative: bool,
    /// Current position in mm.
    pos: Point,
    /// `IP` reference points in plotter units: `[p1x, p1y, p2x, p2y]`.
    ip: Option<[f64; 4]>,
    /// `SC` user window: `[xmin, xmax, ymin, ymax]`.
    sc: Option<[f64; 4]>,
    scaled_used: bool,
    label_end: u8,
    run: Option<Run>,
    items: Vec<ImportedPath>,
    skipped: Vec<Skipped>,
    commands: usize,
    points: usize,
    runs: usize,
}

impl<'l> State<'l> {
    fn new(limits: &'l ParseLimits) -> Self {
        State {
            limits,
            pen: 1,
            down: false,
            relative: false,
            pos: Point::new(0.0, 0.0),
            ip: None,
            sc: None,
            scaled_used: false,
            label_end: 3,
            run: None,
            items: Vec::new(),
            skipped: Vec::new(),
            commands: 0,
            points: 0,
            runs: 0,
        }
    }

    fn report(&mut self, kind: &str, reason: SkipReason) {
        if !self.skipped.iter().any(|s| s.kind == kind && s.reason == reason) {
            self.skipped.push(Skipped { entity: format!("{kind}#{}", self.commands), kind: kind.to_owned(), reason });
        }
    }

    fn run(&mut self, b: &[u8]) -> Result<(), ParseError> {
        let mut i = 0usize;
        while i < b.len() {
            let c = b[i];
            if c.is_ascii_whitespace() || c == b';' || c == 0 {
                i += 1;
            } else if c == 0x1b {
                i = skip_escape(b, i);
            } else if c == b'@' && b[i..].starts_with(b"@PJL") {
                i = b[i..].iter().position(|x| *x == b'\n').map_or(b.len(), |n| i + n + 1);
            } else if c.is_ascii_alphabetic() {
                let Some(second) = b.get(i + 1).copied().filter(u8::is_ascii_alphabetic) else {
                    self.report("?", SkipReason::Invalid("stray letter"));
                    i += 1;
                    continue;
                };
                let mnemonic = [c.to_ascii_uppercase(), second.to_ascii_uppercase()];
                i += 2;
                self.commands += 1;
                if self.commands > self.limits.max_entities {
                    return Err(ParseError::TooManyEntities { limit: self.limits.max_entities });
                }
                i = self.command(mnemonic, b, i)?;
            } else {
                self.report("?", SkipReason::Invalid("stray character"));
                i += 1;
            }
        }
        Ok(())
    }

    /// Reads the numeric parameter list starting at `i`; returns the values (`None` for a
    /// malformed or out-of-range number) and the index after the list.
    fn params(&mut self, b: &[u8], mut i: usize, kind: &str) -> (Vec<Option<f64>>, usize) {
        let mut out = Vec::new();
        loop {
            while b.get(i).is_some_and(|c| c.is_ascii_whitespace() || *c == b',') {
                i += 1;
            }
            match b.get(i) {
                Some(c) if c.is_ascii_digit() || matches!(c, b'+' | b'-' | b'.') => {
                    let start = i;
                    i += 1;
                    while b.get(i).is_some_and(|c| c.is_ascii_digit() || *c == b'.') {
                        i += 1;
                    }
                    let value = std::str::from_utf8(&b[start..i])
                        .ok()
                        .and_then(|t| t.parse::<f64>().ok())
                        .filter(|v| v.is_finite());
                    match value {
                        Some(v) if v.abs() <= COORD_LIMIT => out.push(Some(v)),
                        Some(_) => {
                            self.report(kind, SkipReason::Invalid("coordinate out of range"));
                            out.push(None);
                        }
                        None => {
                            self.report(kind, SkipReason::Invalid("malformed number"));
                            out.push(None);
                        }
                    }
                }
                _ => return (out, i),
            }
        }
    }

    fn command(&mut self, m: [u8; 2], b: &[u8], i: usize) -> Result<usize, ParseError> {
        let name = String::from_utf8_lossy(&m).into_owned();
        match &m {
            b"LB" => {
                let end = b[i..].iter().position(|x| *x == self.label_end).map_or(b.len(), |n| i + n + 1);
                self.report("LB", SkipReason::UnsupportedElement);
                return Ok(end);
            }
            b"PE" => {
                let end = b[i..].iter().position(|x| *x == b';').map_or(b.len(), |n| i + n + 1);
                self.report("PE", SkipReason::UnsupportedElement);
                return Ok(end);
            }
            b"DT" => {
                if let Some(c) = b.get(i).copied().filter(|c| *c != b';') {
                    self.label_end = c;
                    return Ok(i + 1);
                }
                return Ok(i);
            }
            _ => {}
        }
        let (args, next) = self.params(b, i, &name);
        match &m {
            b"IN" | b"DF" => {
                self.flush();
                let (limits, label_end) = (self.limits, self.label_end);
                let keep = std::mem::take(&mut self.items);
                let skipped = std::mem::take(&mut self.skipped);
                let (commands, points, runs, scaled) = (self.commands, self.points, self.runs, self.scaled_used);
                *self = State::new(limits);
                self.label_end = label_end;
                (self.items, self.skipped) = (keep, skipped);
                (self.commands, self.points, self.runs, self.scaled_used) = (commands, points, runs, scaled);
            }
            b"SP" => {
                self.flush();
                match args.first() {
                    Some(Some(n)) if *n >= 0.0 && n.fract() == 0.0 && *n <= f64::from(u32::MAX) => self.pen = *n as u32,
                    Some(_) => self.report("SP", SkipReason::Invalid("pen number")),
                    None => self.pen = 0,
                }
            }
            b"PU" => {
                self.flush();
                self.down = false;
                self.moves(&args, "PU")?;
            }
            b"PD" => {
                self.down = true;
                self.moves(&args, "PD")?;
            }
            b"PA" => {
                self.relative = false;
                self.moves(&args, "PA")?;
            }
            b"PR" => {
                self.relative = true;
                self.moves(&args, "PR")?;
            }
            b"CI" => self.circle(&args),
            b"AA" | b"AR" => self.arc(&args, m[1] == b'R')?,
            b"IP" => self.set_ip(&args),
            b"SC" => self.set_sc(&args),
            b"PM" | b"EP" | b"FP" => {
                self.report(&name, SkipReason::UnsupportedEffect("polygon mode is drawn as outline"))
            }
            b"LT" | b"PW" | b"VS" | b"FS" | b"LA" | b"NP" | b"PC" | b"CT" | b"SM" | b"SL" | b"SS" | b"SA" | b"CS"
            | b"SI" | b"SR" | b"DI" | b"DR" | b"TD" | b"WU" | b"RO" | b"PG" | b"BP" | b"PS" | b"FT" | b"RP" | b"IW"
            | b"EW" | b"ER" | b"EA" | b"RA" | b"RR" | b"WG" | b"CP" | b"SD" | b"AD" | b"CC" | b"MC" | b"MG" | b"TR"
            | b"ES" | b"PP" | b"UL" => {
                self.report(&name, SkipReason::UnsupportedEffect("attribute is not applied"));
            }
            _ => self.report(&name, SkipReason::UnsupportedElement),
        }
        Ok(next)
    }

    /// Maps one user-unit coordinate pair to mm (absolute) or one delta (relative).
    fn scale_point(&mut self, x: f64, y: f64, delta: bool) -> Point {
        if let (Some(ip), Some(sc)) = (self.ip, self.sc) {
            let (dx, dy) = (sc[1] - sc[0], sc[3] - sc[2]);
            if dx != 0.0 && dy != 0.0 {
                self.scaled_used = true;
                let (sx, sy) = ((ip[2] - ip[0]) / dx, (ip[3] - ip[1]) / dy);
                return if delta {
                    Point::new(x * sx * MM_PER_UNIT, y * sy * MM_PER_UNIT)
                } else {
                    Point::new((ip[0] + (x - sc[0]) * sx) * MM_PER_UNIT, (ip[1] + (y - sc[2]) * sy) * MM_PER_UNIT)
                };
            }
        }
        Point::new(x * MM_PER_UNIT, y * MM_PER_UNIT)
    }

    fn moves(&mut self, args: &[Option<f64>], kind: &str) -> Result<(), ParseError> {
        if !args.len().is_multiple_of(2) {
            self.report(kind, SkipReason::Invalid("odd number of coordinates"));
        }
        for pair in args.as_chunks::<2>().0 {
            let [Some(x), Some(y)] = *pair else { continue };
            self.points += 1;
            if self.points > self.limits.max_path_commands {
                return Err(ParseError::TooManyCommands { limit: self.limits.max_path_commands });
            }
            let target = if self.relative {
                let d = self.scale_point(x, y, true);
                Point::new(self.pos.x + d.x, self.pos.y + d.y)
            } else {
                self.scale_point(x, y, false)
            };
            if !(target.x.is_finite() && target.y.is_finite()) {
                self.report(kind, SkipReason::Invalid("coordinate out of range"));
                continue;
            }
            self.step(Segment::Line { to: target });
            self.pos = target;
        }
        Ok(())
    }

    /// Adds `seg` to the open run when the pen is down.
    fn step(&mut self, seg: Segment) {
        if !self.down {
            return;
        }
        let (pen, from) = (self.pen, self.pos);
        self.run.get_or_insert_with(|| Run { pen, start: from, segments: Vec::new() }).segments.push(seg);
    }

    fn circle(&mut self, args: &[Option<f64>]) {
        let Some(Some(r)) = args.first() else {
            return self.report("CI", SkipReason::Invalid("radius"));
        };
        let r_mm = self.scale_point(*r, *r, true).x.abs();
        match Path::circle(self.pos, r_mm) {
            Ok(path) => {
                self.runs += 1;
                self.items.push(ImportedPath {
                    name: format!("Pen {}", self.pen),
                    entity: format!("CI#{}", self.commands),
                    path,
                    layer: Some(ImportLayer::Pen(self.pen)),
                });
            }
            Err(_) => self.report("CI", SkipReason::Invalid("radius must be positive")),
        }
    }

    fn arc(&mut self, args: &[Option<f64>], relative: bool) -> Result<(), ParseError> {
        let kind = if relative { "AR" } else { "AA" };
        let (Some(Some(cx)), Some(Some(cy)), Some(Some(sweep))) = (args.first(), args.get(1), args.get(2)) else {
            self.report(kind, SkipReason::Invalid("centre and sweep angle required"));
            return Ok(());
        };
        let centre = if relative {
            let d = self.scale_point(*cx, *cy, true);
            Point::new(self.pos.x + d.x, self.pos.y + d.y)
        } else {
            self.scale_point(*cx, *cy, false)
        };
        let radius = ((self.pos.x - centre.x).powi(2) + (self.pos.y - centre.y).powi(2)).sqrt();
        let sweep = if sweep.abs() > 360.0 { sweep % 360.0 } else { *sweep };
        if !(radius > 0.0 && radius.is_finite()) || sweep == 0.0 {
            self.report(kind, SkipReason::Invalid("degenerate arc"));
            return Ok(());
        }
        self.points += 1;
        if self.points > self.limits.max_path_commands {
            return Err(ParseError::TooManyCommands { limit: self.limits.max_path_commands });
        }
        let start = math::atan2(self.pos.y - centre.y, self.pos.x - centre.x).to_degrees();
        let built = Angle::from_degrees(start)
            .and_then(|a| Angle::from_degrees(sweep).map(|s| (a, s)))
            .ok()
            .and_then(|(a, s)| EllipticalArc::circle(centre, radius, a, s).ok());
        let Some(arc) = built else {
            self.report(kind, SkipReason::Invalid("degenerate arc"));
            return Ok(());
        };
        let end = arc.end_point();
        self.step(Segment::Arc(arc));
        self.pos = end;
        Ok(())
    }

    fn set_ip(&mut self, args: &[Option<f64>]) {
        match args {
            [] => {
                self.ip = None;
            }
            [Some(a), Some(b), Some(c), Some(d)] => self.ip = Some([*a, *b, *c, *d]),
            _ => self.report("IP", SkipReason::UnsupportedEffect("only the four-value form is supported")),
        }
    }

    fn set_sc(&mut self, args: &[Option<f64>]) {
        match args {
            [] => self.sc = None,
            [Some(a), Some(b), Some(c), Some(d)] | [Some(a), Some(b), Some(c), Some(d), Some(_)]
                if args.get(4).copied().flatten().is_none_or(|t| t == 0.0) =>
            {
                if a == b || c == d {
                    self.report("SC", SkipReason::Invalid("empty user window"));
                } else {
                    self.sc = Some([*a, *b, *c, *d]);
                }
            }
            _ => self.report("SC", SkipReason::UnsupportedEffect("only anisotropic four-value scaling is supported")),
        }
    }

    fn flush(&mut self) {
        let Some(run) = self.run.take() else { return };
        let mut segments = run.segments;
        let end = segments.last().map_or(run.start, Segment::end);
        let closed = segments.len() >= 2 && end.distance(run.start) <= CLOSE_EPS_MM;
        if closed && matches!(segments.last(), Some(Segment::Line { .. })) {
            segments.pop();
        }
        if let Ok(sp) = Subpath::new(run.start, segments, closed) {
            self.runs += 1;
            self.items.push(ImportedPath {
                name: format!("Pen {}", run.pen),
                entity: format!("run#{}", self.runs),
                path: Path::from_subpaths(vec![sp]),
                layer: Some(ImportLayer::Pen(run.pen)),
            });
        }
    }

    fn finish(mut self) -> ImportDraft {
        self.flush();
        if self.sc.is_some() && self.ip.is_none() {
            self.report("SC", SkipReason::UnsupportedEffect("no IP reference points: plotter units stay in force"));
        }
        let basis = if self.scaled_used { HpglUnitBasis::ScaledByIpSc } else { HpglUnitBasis::PlotterUnits };
        ImportDraft::new(self.items, UnitDecision::Hpgl { mm_per_unit: MM_PER_UNIT, basis }, self.skipped)
    }
}

/// Skips a PCL/PJL escape sequence starting at `b[i] == ESC`; returns the index after it.
fn skip_escape(b: &[u8], i: usize) -> usize {
    match b.get(i + 1) {
        // `ESC % …` universal exit/enter: parameters until an upper-case terminator.
        Some(b'%') => b[i + 2..].iter().position(u8::is_ascii_uppercase).map_or(b.len(), |n| i + 2 + n + 1),
        // Device control `ESC . x…`: until ':' or ';'.
        Some(b'.') => b[i + 2..].iter().position(|c| matches!(c, b':' | b';')).map_or(b.len(), |n| i + 2 + n + 1),
        _ => i + 1,
    }
}
