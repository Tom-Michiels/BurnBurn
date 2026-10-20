//! PDF tokens and objects (ISO 32000-1 §7.2–7.3): a lenient, bounded lexer shared by file
//! structure and content streams. Malformed input yields `None`/`Tok::Kw`, never a panic.

use std::collections::BTreeMap;
use std::ops::Range;

pub(super) type Dict = BTreeMap<String, Obj>;

/// Maximum array/dict nesting and maximum elements per container.
const MAX_NEST: usize = 32;
const MAX_ELEMENTS: usize = 1 << 20;

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Obj {
    Null,
    Bool(bool),
    Int(i64),
    Real(f64),
    Name(String),
    Str(Vec<u8>),
    Array(Vec<Obj>),
    Dict(Dict),
    /// `dict` plus the raw (still encoded) data range inside the file bytes.
    Stream(Dict, Range<usize>),
    Ref(u32),
}

impl Obj {
    pub(super) fn as_f64(&self) -> Option<f64> {
        match self {
            Obj::Int(i) => Some(*i as f64),
            Obj::Real(r) => Some(*r),
            _ => None,
        }
    }

    pub(super) fn as_i64(&self) -> Option<i64> {
        match self {
            Obj::Int(i) => Some(*i),
            Obj::Real(r) if r.fract() == 0.0 && r.abs() < 9e15 => Some(*r as i64),
            _ => None,
        }
    }

    pub(super) fn as_name(&self) -> Option<&str> {
        match self {
            Obj::Name(n) => Some(n),
            _ => None,
        }
    }

    pub(super) fn dict(&self) -> Option<&Dict> {
        match self {
            Obj::Dict(d) | Obj::Stream(d, _) => Some(d),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Tok<'a> {
    Int(i64),
    Real(f64),
    Name(String),
    Str(Vec<u8>),
    ArrOpen,
    ArrClose,
    DictOpen,
    DictClose,
    /// A bare keyword or operator (`obj`, `R`, `re`, `BT`, …); also any other stray byte run.
    Kw(&'a [u8]),
    Eof,
}

pub(super) struct Lexer<'a> {
    pub(super) data: &'a [u8],
    pub(super) pos: usize,
    /// Whether `n g R` is read as a reference (off in content streams, where it cannot occur).
    pub(super) refs: bool,
}

fn is_ws(c: u8) -> bool {
    matches!(c, 0 | 9 | 10 | 12 | 13 | 32)
}

fn is_delim(c: u8) -> bool {
    matches!(c, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

impl<'a> Lexer<'a> {
    pub(super) fn new(data: &'a [u8], pos: usize) -> Self {
        Lexer { data, pos, refs: true }
    }

    pub(super) fn skip_ws(&mut self) {
        while let Some(&c) = self.data.get(self.pos) {
            if is_ws(c) {
                self.pos += 1;
            } else if c == b'%' {
                while self.data.get(self.pos).is_some_and(|c| !matches!(c, 10 | 13)) {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    pub(super) fn token(&mut self) -> Tok<'a> {
        self.skip_ws();
        let Some(&c) = self.data.get(self.pos) else { return Tok::Eof };
        match c {
            b'[' => {
                self.pos += 1;
                Tok::ArrOpen
            }
            b']' => {
                self.pos += 1;
                Tok::ArrClose
            }
            b'<' if self.data.get(self.pos + 1) == Some(&b'<') => {
                self.pos += 2;
                Tok::DictOpen
            }
            b'>' if self.data.get(self.pos + 1) == Some(&b'>') => {
                self.pos += 2;
                Tok::DictClose
            }
            b'<' => self.hex_string(),
            b'(' => self.literal_string(),
            b'/' => self.name(),
            b')' | b'>' | b'{' | b'}' => {
                self.pos += 1;
                Tok::Kw(&self.data[self.pos - 1..self.pos])
            }
            _ => self.regular(),
        }
    }

    fn regular(&mut self) -> Tok<'a> {
        let start = self.pos;
        while self.data.get(self.pos).is_some_and(|c| !is_ws(*c) && !is_delim(*c)) {
            self.pos += 1;
        }
        let word = &self.data[start..self.pos];
        number(word).unwrap_or(Tok::Kw(word))
    }

    fn name(&mut self) -> Tok<'a> {
        self.pos += 1;
        let mut out = Vec::new();
        while let Some(&c) = self.data.get(self.pos) {
            if is_ws(c) || is_delim(c) {
                break;
            }
            self.pos += 1;
            if c == b'#'
                && let Some(v) = self.data.get(self.pos..self.pos + 2).and_then(hex_pair)
            {
                out.push(v);
                self.pos += 2;
            } else {
                out.push(c);
            }
        }
        Tok::Name(String::from_utf8_lossy(&out).into_owned())
    }

    fn hex_string(&mut self) -> Tok<'a> {
        self.pos += 1;
        let mut digits = Vec::new();
        while let Some(&c) = self.data.get(self.pos) {
            self.pos += 1;
            if c == b'>' {
                break;
            }
            if let Some(d) = (c as char).to_digit(16) {
                digits.push(d as u8);
            }
        }
        if digits.len() % 2 == 1 {
            digits.push(0);
        }
        Tok::Str(digits.chunks(2).map(|p| p[0] * 16 + p[1]).collect())
    }

    fn literal_string(&mut self) -> Tok<'a> {
        self.pos += 1;
        let (mut depth, mut out) = (1usize, Vec::new());
        while let Some(&c) = self.data.get(self.pos) {
            self.pos += 1;
            match c {
                b'(' => {
                    depth += 1;
                    out.push(c);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    out.push(c);
                }
                b'\\' => {
                    let Some(&e) = self.data.get(self.pos) else { break };
                    self.pos += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'0'..=b'7' => {
                            let mut v = u32::from(e - b'0');
                            for _ in 0..2 {
                                match self.data.get(self.pos) {
                                    Some(d @ b'0'..=b'7') => {
                                        v = v * 8 + u32::from(d - b'0');
                                        self.pos += 1;
                                    }
                                    _ => break,
                                }
                            }
                            out.push(v as u8);
                        }
                        b'\r' => {
                            if self.data.get(self.pos) == Some(&b'\n') {
                                self.pos += 1;
                            }
                        }
                        b'\n' => {}
                        other => out.push(other),
                    }
                }
                _ => out.push(c),
            }
        }
        Tok::Str(out)
    }

    /// Parses one object starting at the next token. `None` when the next token is a keyword
    /// that does not start an object (the lexer has consumed it).
    pub(super) fn object(&mut self) -> Option<Obj> {
        let t = self.token();
        self.object_from(t, 0)
    }

    pub(super) fn object_from(&mut self, first: Tok<'a>, depth: usize) -> Option<Obj> {
        match first {
            Tok::Int(i) => {
                // `n g R` reference.
                if self.refs && (0..=i64::from(u32::MAX)).contains(&i) {
                    let save = self.pos;
                    if let Tok::Int(g) = self.token()
                        && g >= 0
                        && self.token() == Tok::Kw(b"R")
                    {
                        return Some(Obj::Ref(i as u32));
                    }
                    self.pos = save;
                }
                Some(Obj::Int(i))
            }
            Tok::Real(r) => Some(Obj::Real(r)),
            Tok::Name(n) => Some(Obj::Name(n)),
            Tok::Str(s) => Some(Obj::Str(s)),
            Tok::ArrOpen => {
                if depth >= MAX_NEST {
                    return None;
                }
                let mut items = Vec::new();
                loop {
                    match self.token() {
                        Tok::ArrClose | Tok::Eof => return Some(Obj::Array(items)),
                        t => {
                            if items.len() >= MAX_ELEMENTS {
                                return None;
                            }
                            if let Some(o) = self.object_from(t, depth + 1) {
                                items.push(o);
                            }
                        }
                    }
                }
            }
            Tok::DictOpen => {
                if depth >= MAX_NEST {
                    return None;
                }
                let mut dict = Dict::new();
                loop {
                    match self.token() {
                        Tok::DictClose | Tok::Eof => return Some(Obj::Dict(dict)),
                        Tok::Name(key) => {
                            let t = self.token();
                            if matches!(t, Tok::DictClose) {
                                return Some(Obj::Dict(dict));
                            }
                            if dict.len() >= MAX_ELEMENTS {
                                return None;
                            }
                            if let Some(v) = self.object_from(t, depth + 1) {
                                dict.insert(key, v);
                            }
                        }
                        _ => {}
                    }
                }
            }
            Tok::Kw(b"true") => Some(Obj::Bool(true)),
            Tok::Kw(b"false") => Some(Obj::Bool(false)),
            Tok::Kw(b"null") => Some(Obj::Null),
            _ => None,
        }
    }
}

fn hex_pair(b: &[u8]) -> Option<u8> {
    let hi = (*b.first()? as char).to_digit(16)?;
    let lo = (*b.get(1)? as char).to_digit(16)?;
    Some((hi * 16 + lo) as u8)
}

/// A PDF number: `[+-]digits[.digits]` with the usual sloppy forms (`-.5`, `4.`, `--3`).
fn number<'a>(word: &[u8]) -> Option<Tok<'a>> {
    let mut i = 0;
    let mut negative = false;
    while let Some(&c) = word.get(i) {
        match c {
            b'-' => negative = true,
            b'+' => {}
            _ => break,
        }
        i += 1;
    }
    let mut body = &word[i..];
    if body.is_empty() || !body.iter().all(|c| c.is_ascii_digit() || *c == b'.') {
        return None;
    }
    // `1.2.3`: take the leading valid prefix like most readers.
    if let Some(first) = body.iter().position(|c| *c == b'.')
        && let Some(second) = body[first + 1..].iter().position(|c| *c == b'.')
    {
        body = &body[..first + 1 + second];
    }
    if body == b"." {
        return Some(Tok::Int(0));
    }
    if body.contains(&b'.') {
        return real(body, negative);
    }
    let text = std::str::from_utf8(body).ok()?;
    match text.parse::<i64>() {
        Ok(v) => Some(Tok::Int(if negative { -v } else { v })),
        Err(_) => real(body, negative),
    }
}

fn real<'a>(body: &[u8], negative: bool) -> Option<Tok<'a>> {
    let v: f64 = std::str::from_utf8(body).ok()?.parse().ok().filter(|v: &f64| v.is_finite())?;
    Some(Tok::Real(if negative { -v } else { v }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Option<Obj> {
        Lexer::new(s.as_bytes(), 0).object()
    }

    #[test]
    fn numbers_names_strings_and_containers() {
        assert_eq!(parse("-12"), Some(Obj::Int(-12)));
        assert_eq!(parse("-.5"), Some(Obj::Real(-0.5)));
        assert_eq!(parse("4."), Some(Obj::Real(4.0)));
        assert_eq!(parse("--3"), Some(Obj::Int(-3)));
        assert_eq!(parse("/A#20B"), Some(Obj::Name("A B".into())));
        assert_eq!(parse("(a\\(b\\)(c)\\101\\n)"), Some(Obj::Str(b"a(b)(c)A\n".to_vec())));
        assert_eq!(parse("<4 1 4>"), Some(Obj::Str(b"A@".to_vec())));
        assert_eq!(parse("12 0 R"), Some(Obj::Ref(12)));
        assert_eq!(
            parse("[1 2 R /N (s)]"),
            Some(Obj::Array(vec![Obj::Ref(1), Obj::Name("N".into()), Obj::Str(b"s".to_vec())]))
        );
        let Some(Obj::Dict(d)) = parse("<< /A 1 /B [true null] /C << /D 2.5 >> >>") else { panic!() };
        assert_eq!(d["A"], Obj::Int(1));
        assert_eq!(d["C"].dict().unwrap()["D"], Obj::Real(2.5));
        assert_eq!(parse("% comment\n 7"), Some(Obj::Int(7)));
    }

    #[test]
    fn nesting_is_bounded_and_garbage_does_not_panic() {
        // 100 open brackets: the parser stops descending at MAX_NEST instead of recursing without bound.
        let deep = "[".repeat(100);
        let mut depth = 0;
        let mut cursor = parse(&deep);
        while let Some(Obj::Array(mut items)) = cursor {
            depth += 1;
            cursor = items.pop();
        }
        assert!(depth <= MAX_NEST + 1, "depth {depth}");
        for s in ["<<", "[", "(", "<", "/", ")", ">>", "1 2", "<< /A", "<</A/B/C>>"] {
            let _ = parse(s);
        }
    }
}
