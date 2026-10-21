//! An independent, minimal G-code interpreter used as test oracle.
//!
//! It is written from the GRBL 1.1 behaviour description (laser mode `$32=1`), not from the
//! lowering code, and knows only the words a BurnBurn program may contain. Anything else is a
//! hard error, so an unexpected word in generated output fails a test instead of passing silently.
//!
//! Semantics implemented:
//! - units: only `G21` (mm); `G20` is an error. Distance: `G90` absolute only.
//! - `G0`: rapid, laser off. `G1`: feed move; laser on iff a spindle mode (`M3`/`M4`) is active
//!   and the modal `S` is > 0 (GRBL laser mode: `S0` is "off").
//! - `S`, `F` are modal. `M3`/`M4` spindle (laser) on, `M5` off, `M8`/`M9` air assist on/off,
//!   `M2` program end (laser off, nothing may follow).
//! - Comments: `;` to end of line, `( ... )`.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    pub from: (f64, f64),
    pub to: (f64, f64),
    pub rapid: bool,
    /// Laser actually emitting during this segment.
    pub laser_on: bool,
    /// Spindle word value in effect (0 when off).
    pub s: f64,
    /// Feed in mm/min (0 for rapids).
    pub feed: f64,
    /// Air assist (M8) active.
    pub air: bool,
    pub line: usize,
}

impl Segment {
    pub fn length(&self) -> f64 {
        (self.to.0 - self.from.0).hypot(self.to.1 - self.from.1)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub segments: Vec<Segment>,
    pub ended: bool,
    pub laser_on_at_end: bool,
    pub air_on_at_end: bool,
    pub work_system: Option<u8>,
}

impl Run {
    pub fn tool_path_length(&self) -> f64 {
        self.segments.iter().map(Segment::length).sum()
    }
    pub fn laser_on_length(&self) -> f64 {
        self.segments.iter().filter(|s| s.laser_on).map(Segment::length).sum()
    }
    /// Sum over laser-on segments of `S / s_max * length / (feed / 60)`: ratio-seconds of burn.
    pub fn energy_ratio_seconds(&self, s_max: f64) -> f64 {
        self.segments.iter().filter(|s| s.laser_on).map(|s| (s.s / s_max) * s.length() / (s.feed / 60.0)).sum()
    }
    /// Bounds of every segment endpoint: `(min_x, min_y, max_x, max_y)`.
    pub fn bounds(&self) -> Option<(f64, f64, f64, f64)> {
        let mut it = self.segments.iter().flat_map(|s| [s.from, s.to]);
        let first = it.next()?;
        Some(it.fold((first.0, first.1, first.0, first.1), |b, p| {
            (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1))
        }))
    }
    /// Bounds of the laser-on endpoints only.
    pub fn burn_bounds(&self) -> Option<(f64, f64, f64, f64)> {
        let mut it = self.segments.iter().filter(|s| s.laser_on).flat_map(|s| [s.from, s.to]);
        let first = it.next()?;
        Some(it.fold((first.0, first.1, first.0, first.1), |b, p| {
            (b.0.min(p.0), b.1.min(p.1), b.2.max(p.0), b.3.max(p.1))
        }))
    }
    pub fn burn_segments(&self) -> Vec<&Segment> {
        self.segments.iter().filter(|s| s.laser_on).collect()
    }
}

fn strip_comments(line: &str) -> String {
    let mut out = String::new();
    let mut depth = 0u32;
    for c in line.chars() {
        match c {
            ';' if depth == 0 => break,
            '(' => depth += 1,
            ')' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

fn words(line: &str) -> Result<Vec<(char, f64)>, String> {
    let chars: Vec<char> = line.chars().filter(|c| !c.is_whitespace()).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let letter = chars[i].to_ascii_uppercase();
        if !letter.is_ascii_alphabetic() {
            return Err(format!("expected a word letter, found {:?}", chars[i]));
        }
        i += 1;
        let start = i;
        while i < chars.len() && (chars[i].is_ascii_digit() || matches!(chars[i], '.' | '-' | '+')) {
            i += 1;
        }
        let text: String = chars[start..i].iter().collect();
        let v: f64 = text.parse().map_err(|_| format!("bad number {text:?} after {letter}"))?;
        out.push((letter, v));
    }
    Ok(out)
}

pub fn run(text: &str) -> Result<Run, String> {
    let mut at = (0.0f64, 0.0f64);
    let mut spindle = false;
    let (mut s, mut f) = (0.0f64, 0.0f64);
    let mut air = false;
    let mut mm = false;
    let mut abs = false;
    let mut ended = false;
    let mut work = None;
    let mut segments = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let n = n + 1;
        let line = strip_comments(raw);
        if line.trim().is_empty() {
            continue;
        }
        if ended {
            return Err(format!("line {n}: code after program end"));
        }
        let ws = words(&line).map_err(|e| format!("line {n}: {e}"))?;
        // Order of execution within a block is irrelevant for our programs except that S/F/M words of
        // the block apply to its motion (GRBL laser mode), so apply them first.
        let mut motion: Option<u8> = None;
        let (mut x, mut y) = (None, None);
        for &(l, v) in &ws {
            match l {
                'G' => match v as i32 {
                    0 => motion = Some(0),
                    1 => motion = Some(1),
                    17 | 94 => {}
                    21 => mm = true,
                    90 => abs = true,
                    54..=59 => work = Some(v as u8),
                    other => return Err(format!("line {n}: unsupported G{other}")),
                },
                'M' => match v as i32 {
                    3 | 4 => spindle = true,
                    5 => spindle = false,
                    8 => air = true,
                    9 => air = false,
                    2 => {
                        ended = true;
                        spindle = false;
                    }
                    other => return Err(format!("line {n}: unsupported M{other}")),
                },
                'S' => s = v,
                'F' => f = v,
                'X' => x = Some(v),
                'Y' => y = Some(v),
                other => return Err(format!("line {n}: unsupported word {other}")),
            }
        }
        if (x.is_some() || y.is_some()) && motion.is_none() {
            // Modal motion is not used by BurnBurn output; refuse rather than guess.
            return Err(format!("line {n}: axis words without G0/G1"));
        }
        if let Some(g) = motion {
            if !mm || !abs {
                return Err(format!("line {n}: motion before G21/G90"));
            }
            let to = (x.unwrap_or(at.0), y.unwrap_or(at.1));
            let rapid = g == 0;
            if !rapid && f <= 0.0 {
                return Err(format!("line {n}: G1 without a feed rate"));
            }
            let laser_on = !rapid && spindle && s > 0.0;
            segments.push(Segment {
                from: at,
                to,
                rapid,
                laser_on,
                s: if spindle { s } else { 0.0 },
                feed: if rapid { 0.0 } else { f },
                air,
                line: n,
            });
            at = to;
        }
    }
    Ok(Run { segments, ended, laser_on_at_end: spindle, air_on_at_end: air, work_system: work })
}
