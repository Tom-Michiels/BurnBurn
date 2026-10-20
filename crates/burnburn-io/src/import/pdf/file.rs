//! PDF file structure (ISO 32000-1 §7.5): header offset, cross-reference tables and streams,
//! object streams, stream filters (Flate with PNG/TIFF predictors, ASCIIHex, ASCII85) and a
//! rebuild-by-scanning fallback for damaged files. Read-only and bounded.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use super::inflate::{InflateError, zlib_inflate};
use super::object::{Dict, Lexer, Obj, Tok};
use crate::import::{ParseError, ParseLimits};

#[derive(Debug, Clone, Copy)]
enum Entry {
    /// Absolute offset of `n g obj` (header offset already added).
    At(usize),
    /// Index inside object stream number `.0`.
    InStream(u32, u32),
}

/// Why a stream could not be decoded; reported per page, never fatal for the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StreamError {
    TooLarge,
    Damaged,
    UnsupportedFilter,
}

pub(super) struct PdfFile<'a> {
    data: &'a [u8],
    base: usize,
    xref: BTreeMap<u32, Entry>,
    pub(super) trailer: Dict,
    cache: RefCell<HashMap<u32, Rc<Obj>>>,
    objstm_cache: RefCell<HashMap<u32, Rc<Vec<(u32, Obj)>>>>,
    decoded_total: Cell<usize>,
    stream_cap: usize,
    total_cap: usize,
}

const MAX_XREF_CHAIN: usize = 64;
const MAX_REF_CHAIN: usize = 16;

fn find(data: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    data.get(from..)?.windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}

fn rfind(data: &[u8], needle: &[u8]) -> Option<usize> {
    data.windows(needle.len()).rposition(|w| w == needle)
}

impl<'a> PdfFile<'a> {
    pub(super) fn open(data: &'a [u8], limits: &ParseLimits) -> Result<Self, ParseError> {
        let head = &data[..data.len().min(1024)];
        let base = find(head, b"%PDF-", 0).ok_or(ParseError::Malformed { at: 0, reason: "no %PDF header" })?;
        let mut file = PdfFile {
            data,
            base,
            xref: BTreeMap::new(),
            trailer: Dict::new(),
            cache: RefCell::default(),
            objstm_cache: RefCell::default(),
            decoded_total: Cell::new(0),
            stream_cap: limits.max_bytes.saturating_mul(4),
            total_cap: limits.max_bytes.saturating_mul(8),
        };
        if file.read_xref_chain().is_err() || file.root_ref().is_none() {
            file.rebuild();
        }
        if file.trailer.contains_key("Encrypt") {
            return Err(ParseError::Unsupported("encrypted PDF"));
        }
        if file.root_ref().is_none() {
            return Err(ParseError::Malformed { at: 0, reason: "no document catalog" });
        }
        Ok(file)
    }

    fn root_ref(&self) -> Option<u32> {
        match self.trailer.get("Root") {
            Some(Obj::Ref(n)) if self.xref.contains_key(n) => Some(*n),
            _ => None,
        }
    }

    pub(super) fn catalog(&self) -> Option<Rc<Obj>> {
        self.object(self.root_ref()?)
    }

    fn read_xref_chain(&mut self) -> Result<(), ()> {
        let tail_start = self.data.len().saturating_sub(2048);
        let at = rfind(&self.data[tail_start..], b"startxref").ok_or(())? + tail_start;
        let mut lex = Lexer::new(self.data, at + 9);
        let Tok::Int(first) = lex.token() else { return Err(()) };
        let mut next = Some(usize::try_from(first).map_err(|_| ())?.checked_add(self.base).ok_or(())?);
        let mut seen = HashSet::new();
        let mut hops = 0;
        let mut first_trailer = true;
        while let Some(offset) = next.take() {
            hops += 1;
            if hops > MAX_XREF_CHAIN || !seen.insert(offset) || offset >= self.data.len() {
                return Err(());
            }
            let trailer = self.read_xref_section(offset)?;
            if let Some(Obj::Int(stm)) = trailer.get("XRefStm") {
                let off = usize::try_from(*stm).map_err(|_| ())?.checked_add(self.base).ok_or(())?;
                if seen.insert(off) && off < self.data.len() {
                    self.read_xref_section(off)?;
                }
            }
            next = match trailer.get("Prev").and_then(Obj::as_i64) {
                Some(p) if p >= 0 => Some((p as usize).checked_add(self.base).ok_or(())?),
                _ => None,
            };
            if first_trailer {
                self.trailer = trailer;
                first_trailer = false;
            } else {
                for (k, v) in trailer {
                    self.trailer.entry(k).or_insert(v);
                }
            }
        }
        Ok(())
    }

    /// Reads one table or stream section at `offset`; entries already known (newer) win.
    fn read_xref_section(&mut self, offset: usize) -> Result<Dict, ()> {
        let mut lex = Lexer::new(self.data, offset);
        match lex.token() {
            Tok::Kw(b"xref") => self.read_xref_table(lex),
            Tok::Int(_) => self.read_xref_stream(offset),
            _ => Err(()),
        }
    }

    fn read_xref_table(&mut self, mut lex: Lexer<'a>) -> Result<Dict, ()> {
        loop {
            match lex.token() {
                Tok::Int(first) => {
                    let Tok::Int(count) = lex.token() else { return Err(()) };
                    if count < 0 || first < 0 {
                        return Err(());
                    }
                    for i in 0..count {
                        let (Tok::Int(off), Tok::Int(_gen), Tok::Kw(kind)) = (lex.token(), lex.token(), lex.token())
                        else {
                            return Err(());
                        };
                        let num = u32::try_from(first + i).map_err(|_| ())?;
                        if kind == b"n" && off > 0 {
                            let at = usize::try_from(off).map_err(|_| ())?.checked_add(self.base).ok_or(())?;
                            self.xref.entry(num).or_insert(Entry::At(at));
                        }
                    }
                }
                Tok::Kw(b"trailer") => break,
                _ => return Err(()),
            }
        }
        match lex.object() {
            Some(Obj::Dict(d)) => Ok(d),
            _ => Err(()),
        }
    }

    fn read_xref_stream(&mut self, offset: usize) -> Result<Dict, ()> {
        let (_, obj) = self.parse_indirect_at(offset, 0).ok_or(())?;
        let Obj::Stream(dict, range) = &obj else { return Err(()) };
        if dict.get("Type").and_then(Obj::as_name) != Some("XRef") {
            return Err(());
        }
        let bytes = self.decode_range(dict, range.clone()).map_err(|_| ())?;
        let w: Vec<usize> = match dict.get("W") {
            Some(Obj::Array(a)) if a.len() == 3 => a
                .iter()
                .map(|o| o.as_i64().and_then(|v| usize::try_from(v).ok()).filter(|v| *v <= 8))
                .collect::<Option<_>>()
                .ok_or(())?,
            _ => return Err(()),
        };
        let size = dict.get("Size").and_then(Obj::as_i64).ok_or(())?;
        let index: Vec<i64> = match dict.get("Index") {
            Some(Obj::Array(a)) => a.iter().filter_map(Obj::as_i64).collect(),
            _ => vec![0, size],
        };
        let row = w.iter().sum::<usize>();
        if row == 0 {
            return Err(());
        }
        let field = |chunk: &[u8]| chunk.iter().fold(0u64, |acc, b| (acc << 8) | u64::from(*b));
        let mut rows = bytes.chunks_exact(row);
        for pair in index.as_chunks::<2>().0 {
            let (first, count) = (pair[0], pair[1]);
            if first < 0 || count < 0 {
                return Err(());
            }
            for i in 0..count {
                let Some(r) = rows.next() else { return Ok(dict.clone()) };
                let (t, f2, f3) = (&r[..w[0]], &r[w[0]..w[0] + w[1]], &r[w[0] + w[1]..]);
                let kind = if w[0] == 0 { 1 } else { field(t) };
                let num = u32::try_from(first + i).map_err(|_| ())?;
                match kind {
                    1 => {
                        let at = usize::try_from(field(f2)).map_err(|_| ())?.checked_add(self.base).ok_or(())?;
                        self.xref.entry(num).or_insert(Entry::At(at));
                    }
                    2 => {
                        let stm = u32::try_from(field(f2)).map_err(|_| ())?;
                        let idx = u32::try_from(field(f3)).map_err(|_| ())?;
                        self.xref.entry(num).or_insert(Entry::InStream(stm, idx));
                    }
                    _ => {}
                }
            }
        }
        Ok(dict.clone())
    }

    /// Rebuilds the table by scanning for `n g obj`, trailers and object streams.
    fn rebuild(&mut self) {
        self.xref.clear();
        self.trailer = Dict::new();
        self.cache.borrow_mut().clear();
        self.objstm_cache.borrow_mut().clear();
        let data = self.data;
        let mut pos = 0;
        let mut stms: Vec<u32> = Vec::new();
        while let Some(at) = find(data, b"obj", pos) {
            pos = at + 3;
            // Must be `n g obj` with `obj` not part of `endobj`.
            if at >= 1 && (data[at - 1].is_ascii_alphabetic()) {
                continue;
            }
            let mut back = at;
            let mut fields = Vec::new();
            while fields.len() < 2 {
                while back > 0 && data[back - 1].is_ascii_whitespace() {
                    back -= 1;
                }
                let end = back;
                while back > 0 && data[back - 1].is_ascii_digit() {
                    back -= 1;
                }
                if back == end {
                    break;
                }
                fields.push((back, std::str::from_utf8(&data[back..end]).ok().and_then(|t| t.parse::<u64>().ok())));
            }
            if let [(_, Some(_gen)), (start, Some(num))] = fields[..]
                && let Ok(num) = u32::try_from(num)
            {
                self.xref.insert(num, Entry::At(start));
                if let Some((_, Obj::Stream(d, _))) = self.parse_indirect_at(start, 0)
                    && d.get("Type").and_then(Obj::as_name) == Some("ObjStm")
                {
                    stms.push(num);
                }
            }
        }
        // Trailers: classic `trailer << … >>` and XRef-stream dictionaries.
        let mut tpos = 0;
        while let Some(at) = find(data, b"trailer", tpos) {
            tpos = at + 7;
            if let Some(Obj::Dict(d)) = Lexer::new(data, tpos).object() {
                for (k, v) in d {
                    self.trailer.insert(k, v);
                }
            }
        }
        let entries: Vec<(u32, usize)> =
            self.xref.iter().filter_map(|(n, e)| if let Entry::At(o) = e { Some((*n, *o)) } else { None }).collect();
        for (_, off) in &entries {
            if let Some((_, Obj::Stream(d, _))) = self.parse_indirect_at(*off, 0)
                && d.get("Type").and_then(Obj::as_name) == Some("XRef")
            {
                for (k, v) in d {
                    self.trailer.insert(k, v);
                }
            }
        }
        // Objects inside object streams (later definitions elsewhere in the file keep priority).
        for stm in stms {
            if let Some(list) = self.object_stream(stm) {
                for (i, (num, _)) in list.iter().enumerate() {
                    self.xref.entry(*num).or_insert(Entry::InStream(stm, i as u32));
                }
            }
        }
        if !matches!(self.trailer.get("Root"), Some(Obj::Ref(n)) if self.xref.contains_key(n)) {
            let catalog = self.xref.keys().copied().find(|n| {
                self.object(*n)
                    .is_some_and(|o| o.dict().and_then(|d| d.get("Type")).and_then(Obj::as_name) == Some("Catalog"))
            });
            if let Some(n) = catalog {
                self.trailer.insert("Root".into(), Obj::Ref(n));
            }
        }
    }

    /// Parses `n g obj … [stream]` at `offset`; the object number is returned.
    fn parse_indirect_at(&self, offset: usize, depth: usize) -> Option<(u32, Obj)> {
        let mut lex = Lexer::new(self.data, offset);
        let Tok::Int(num) = lex.token() else { return None };
        let Tok::Int(_) = lex.token() else { return None };
        if lex.token() != Tok::Kw(b"obj") {
            return None;
        }
        let obj = lex.object()?;
        let num = u32::try_from(num).ok()?;
        if let Obj::Dict(dict) = obj {
            if lex.token() == Tok::Kw(b"stream") {
                let mut start = lex.pos;
                if self.data.get(start) == Some(&b'\r') {
                    start += 1;
                }
                if self.data.get(start) == Some(&b'\n') {
                    start += 1;
                }
                let declared = match dict.get("Length") {
                    Some(Obj::Int(n)) => usize::try_from(*n).ok(),
                    Some(Obj::Ref(r)) if depth < 2 => self
                        .object_at_depth(*r, depth + 1)
                        .and_then(|o| o.as_i64())
                        .and_then(|n| usize::try_from(n).ok()),
                    _ => None,
                };
                let end = declared
                    .and_then(|n| start.checked_add(n))
                    .filter(|e| *e <= self.data.len() && self.endstream_follows(*e))
                    .or_else(|| {
                        let e = find(self.data, b"endstream", start)?;
                        let mut e2 = e;
                        if e2 > start && self.data[e2 - 1] == b'\n' {
                            e2 -= 1;
                        }
                        if e2 > start && self.data[e2 - 1] == b'\r' {
                            e2 -= 1;
                        }
                        Some(e2)
                    })?;
                return Some((num, Obj::Stream(dict, start..end)));
            }
            return Some((num, Obj::Dict(dict)));
        }
        Some((num, obj))
    }

    fn endstream_follows(&self, at: usize) -> bool {
        let mut p = at;
        while self.data.get(p).is_some_and(|c| matches!(c, 10 | 13 | 32)) {
            p += 1;
        }
        self.data.get(p..p + 9) == Some(b"endstream")
    }

    pub(super) fn object(&self, num: u32) -> Option<Rc<Obj>> {
        self.object_at_depth(num, 0).map(Rc::new)
    }

    fn object_at_depth(&self, num: u32, depth: usize) -> Option<Obj> {
        if let Some(o) = self.cache.borrow().get(&num) {
            return Some((**o).clone());
        }
        let obj = match *self.xref.get(&num)? {
            Entry::At(off) => {
                let (found, obj) = self.parse_indirect_at(off, depth)?;
                if found != num {
                    return None;
                }
                obj
            }
            Entry::InStream(stm, idx) => {
                let list = self.object_stream(stm)?;
                let (n, o) = list.get(idx as usize)?;
                if *n != num {
                    return None;
                }
                o.clone()
            }
        };
        self.cache.borrow_mut().insert(num, Rc::new(obj.clone()));
        Some(obj)
    }

    fn object_stream(&self, stm: u32) -> Option<Rc<Vec<(u32, Obj)>>> {
        if let Some(l) = self.objstm_cache.borrow().get(&stm) {
            return Some(l.clone());
        }
        let Entry::At(off) = *self.xref.get(&stm)? else { return None };
        let (_, obj) = self.parse_indirect_at(off, 0)?;
        let Obj::Stream(dict, range) = obj else { return None };
        if dict.get("Type").and_then(Obj::as_name) != Some("ObjStm") {
            return None;
        }
        let n = usize::try_from(dict.get("N")?.as_i64()?).ok()?.min(1 << 20);
        let first = usize::try_from(dict.get("First")?.as_i64()?).ok()?;
        let bytes = self.decode_range(&dict, range).ok()?;
        let mut head = Lexer::new(&bytes, 0);
        let mut pairs = Vec::new();
        for _ in 0..n {
            let (Tok::Int(num), Tok::Int(off)) = (head.token(), head.token()) else { break };
            pairs.push((u32::try_from(num).ok()?, usize::try_from(off).ok()?));
        }
        let mut list = Vec::with_capacity(pairs.len());
        for (num, off) in pairs {
            let obj = Lexer::new(&bytes, first.checked_add(off)?).object().unwrap_or(Obj::Null);
            list.push((num, obj));
        }
        let rc = Rc::new(list);
        self.objstm_cache.borrow_mut().insert(stm, rc.clone());
        Some(rc)
    }

    /// Follows references (bounded) and returns the target; non-references pass through.
    pub(super) fn resolve(&self, obj: &Obj) -> Option<Rc<Obj>> {
        let mut current = Rc::new(obj.clone());
        for _ in 0..MAX_REF_CHAIN {
            match &*current {
                Obj::Ref(n) => current = self.object(*n)?,
                _ => return Some(current),
            }
        }
        None
    }

    pub(super) fn get(&self, dict: &Dict, key: &str) -> Option<Rc<Obj>> {
        self.resolve(dict.get(key)?)
    }

    /// Decodes the stream data of `obj` through its filters.
    pub(super) fn decode_stream(&self, obj: &Obj) -> Result<Vec<u8>, StreamError> {
        let Obj::Stream(dict, range) = obj else { return Err(StreamError::Damaged) };
        self.decode_range(dict, range.clone())
    }

    fn decode_range(&self, dict: &Dict, range: std::ops::Range<usize>) -> Result<Vec<u8>, StreamError> {
        let mut data = self.data.get(range).ok_or(StreamError::Damaged)?.to_vec();
        let filters: Vec<String> = match dict.get("Filter").or_else(|| dict.get("F")).and_then(|f| self.resolve(f)) {
            None => Vec::new(),
            Some(f) => match &*f {
                Obj::Name(n) => vec![n.clone()],
                Obj::Array(a) => {
                    a.iter().filter_map(|o| self.resolve(o)).filter_map(|o| o.as_name().map(str::to_owned)).collect()
                }
                _ => Vec::new(),
            },
        };
        let params: Vec<Option<Dict>> =
            match dict.get("DecodeParms").or_else(|| dict.get("DP")).and_then(|p| self.resolve(p)) {
                None => Vec::new(),
                Some(p) => match &*p {
                    Obj::Dict(d) => vec![Some(d.clone())],
                    Obj::Array(a) => a.iter().map(|o| self.resolve(o).and_then(|o| o.dict().cloned())).collect(),
                    _ => Vec::new(),
                },
            };
        for (i, filter) in filters.iter().enumerate() {
            let parms = params.get(i).and_then(Option::as_ref);
            data = match filter.as_str() {
                "FlateDecode" | "Fl" => {
                    let out = zlib_inflate(&data, self.stream_cap).map_err(|e| match e {
                        InflateError::TooLarge => StreamError::TooLarge,
                        _ => StreamError::Damaged,
                    })?;
                    self.predict(out, parms)?
                }
                "ASCIIHexDecode" | "AHx" => ascii_hex(&data),
                "ASCII85Decode" | "A85" => ascii85(&data).ok_or(StreamError::Damaged)?,
                _ => return Err(StreamError::UnsupportedFilter),
            };
            if data.len() > self.stream_cap {
                return Err(StreamError::TooLarge);
            }
        }
        let total = self.decoded_total.get().saturating_add(data.len());
        if total > self.total_cap {
            return Err(StreamError::TooLarge);
        }
        self.decoded_total.set(total);
        Ok(data)
    }

    fn predict(&self, data: Vec<u8>, parms: Option<&Dict>) -> Result<Vec<u8>, StreamError> {
        let Some(parms) = parms else { return Ok(data) };
        let int = |k: &str, d: i64| parms.get(k).and_then(|o| self.resolve(o)).and_then(|o| o.as_i64()).unwrap_or(d);
        let predictor = int("Predictor", 1);
        if predictor <= 1 {
            return Ok(data);
        }
        let colors = usize::try_from(int("Colors", 1)).map_err(|_| StreamError::Damaged)?.clamp(1, 32);
        let bpc = usize::try_from(int("BitsPerComponent", 8)).map_err(|_| StreamError::Damaged)?;
        let columns = usize::try_from(int("Columns", 1)).map_err(|_| StreamError::Damaged)?.clamp(1, 1 << 24);
        if !matches!(bpc, 1 | 2 | 4 | 8 | 16) {
            return Err(StreamError::Damaged);
        }
        let row = (colors * bpc * columns).div_ceil(8);
        let bpp = (colors * bpc).div_ceil(8);
        if predictor == 2 {
            if bpc != 8 {
                return Err(StreamError::UnsupportedFilter);
            }
            let mut out = data;
            for line in out.chunks_mut(row) {
                for i in bpp..line.len() {
                    line[i] = line[i].wrapping_add(line[i - bpp]);
                }
            }
            return Ok(out);
        }
        // PNG predictors: every row starts with its filter type.
        let mut out: Vec<u8> = Vec::with_capacity(data.len());
        let mut prev = vec![0u8; row];
        for chunk in data.chunks(row + 1) {
            let (ft, raw) = chunk.split_first().ok_or(StreamError::Damaged)?;
            let mut cur = raw.to_vec();
            cur.resize(row, 0);
            for i in 0..row {
                let a = if i >= bpp { cur[i - bpp] } else { 0 };
                let b = prev[i];
                let c = if i >= bpp { prev[i - bpp] } else { 0 };
                let add = match ft {
                    0 => 0,
                    1 => a,
                    2 => b,
                    3 => ((u16::from(a) + u16::from(b)) / 2) as u8,
                    4 => paeth(a, b, c),
                    _ => return Err(StreamError::Damaged),
                };
                cur[i] = cur[i].wrapping_add(add);
            }
            out.extend_from_slice(&cur);
            prev = cur;
        }
        Ok(out)
    }
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let (ia, ib, ic) = (i32::from(a), i32::from(b), i32::from(c));
    let p = ia + ib - ic;
    let (pa, pb, pc) = ((p - ia).abs(), (p - ib).abs(), (p - ic).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

fn ascii_hex(data: &[u8]) -> Vec<u8> {
    let digits: Vec<u8> =
        data.iter().take_while(|c| **c != b'>').filter_map(|c| (*c as char).to_digit(16).map(|d| d as u8)).collect();
    digits.chunks(2).map(|p| p[0] * 16 + p.get(1).copied().unwrap_or(0)).collect()
}

fn ascii85(data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut group = [0u8; 5];
    let mut n = 0;
    let mut i = 0;
    while i < data.len() {
        let c = data[i];
        i += 1;
        match c {
            b'~' => break,
            b'z' if n == 0 => out.extend_from_slice(&[0, 0, 0, 0]),
            b'!'..=b'u' => {
                group[n] = c - b'!';
                n += 1;
                if n == 5 {
                    let v = group.iter().fold(0u64, |acc, d| acc * 85 + u64::from(*d));
                    out.extend_from_slice(&u32::try_from(v).ok()?.to_be_bytes());
                    n = 0;
                }
            }
            c if c.is_ascii_whitespace() => {}
            _ => return None,
        }
    }
    if n > 1 {
        for slot in group.iter_mut().skip(n) {
            *slot = 84;
        }
        let v = group.iter().fold(0u64, |acc, d| acc * 85 + u64::from(*d));
        let bytes = u32::try_from(v).ok()?.to_be_bytes();
        out.extend_from_slice(&bytes[..n - 1]);
    }
    Some(out)
}
