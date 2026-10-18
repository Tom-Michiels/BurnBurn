//! QR code (ISO/IEC 18004), own pure-Rust encoder: byte mode, error correction L/M/Q/H,
//! versions 1–10, Reed–Solomon over GF(256) (primitive polynomial `0x11D`), the eight data
//! masks chosen by the standard's penalty rules N1–N4.
//!
//! Written from the published description of the symbology (structure, block tables, BCH format
//! and version codes, placement order); no encoder source was used. Geometry is nominal: one
//! square per dark module, no kerf (CAM applies it once).

use burnburn_geometry::{Path, Point, Rect};
use burnburn_types::Length;

use crate::error::{GenError, positive};

/// Error-correction level: roughly 7 %, 15 %, 25 % and 30 % of the codewords can be restored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QrEcc {
    L,
    M,
    Q,
    H,
}

impl QrEcc {
    fn index(self) -> usize {
        match self {
            QrEcc::L => 0,
            QrEcc::M => 1,
            QrEcc::Q => 2,
            QrEcc::H => 3,
        }
    }

    /// The two format-information bits of the level.
    fn format_bits(self) -> u16 {
        match self {
            QrEcc::L => 0b01,
            QrEcc::M => 0b00,
            QrEcc::Q => 0b11,
            QrEcc::H => 0b10,
        }
    }
}

/// An encoded symbol: module matrix without quiet zone, row 0 at the top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QrCode {
    version: u8,
    ecc: QrEcc,
    mask: u8,
    size: usize,
    modules: Vec<bool>,
}

impl QrCode {
    pub fn version(&self) -> u8 {
        self.version
    }
    pub fn ecc(&self) -> QrEcc {
        self.ecc
    }
    pub fn mask(&self) -> u8 {
        self.mask
    }
    /// Modules per side (`17 + 4 · version`), without quiet zone.
    pub fn size(&self) -> usize {
        self.size
    }
    /// Whether the module at `row`, `col` (from the top-left) is dark; outside is light.
    pub fn is_dark(&self, row: usize, col: usize) -> bool {
        row < self.size && col < self.size && self.modules.get(row * self.size + col).copied().unwrap_or(false)
    }
}

/// Largest supported version.
pub const QR_MAX_VERSION: u8 = 10;

/// Per version 1..=10 and level L, M, Q, H: (EC codewords per block, blocks in group 1, data
/// codewords per group-1 block, blocks in group 2, data codewords per group-2 block).
const BLOCKS: [[(usize, usize, usize, usize, usize); 4]; 10] = [
    [(7, 1, 19, 0, 0), (10, 1, 16, 0, 0), (13, 1, 13, 0, 0), (17, 1, 9, 0, 0)],
    [(10, 1, 34, 0, 0), (16, 1, 28, 0, 0), (22, 1, 22, 0, 0), (28, 1, 16, 0, 0)],
    [(15, 1, 55, 0, 0), (26, 1, 44, 0, 0), (18, 2, 17, 0, 0), (22, 2, 13, 0, 0)],
    [(20, 1, 80, 0, 0), (18, 2, 32, 0, 0), (26, 2, 24, 0, 0), (16, 4, 9, 0, 0)],
    [(26, 1, 108, 0, 0), (24, 2, 43, 0, 0), (18, 2, 15, 2, 16), (22, 2, 11, 2, 12)],
    [(18, 2, 68, 0, 0), (16, 4, 27, 0, 0), (24, 4, 19, 0, 0), (28, 4, 15, 0, 0)],
    [(20, 2, 78, 0, 0), (18, 4, 31, 0, 0), (18, 2, 14, 4, 15), (26, 4, 13, 1, 14)],
    [(24, 2, 97, 0, 0), (22, 2, 38, 2, 39), (22, 4, 18, 2, 19), (26, 4, 14, 2, 15)],
    [(30, 2, 116, 0, 0), (22, 3, 36, 2, 37), (20, 4, 16, 4, 17), (24, 4, 12, 4, 13)],
    [(18, 2, 68, 2, 69), (26, 4, 43, 1, 44), (24, 6, 19, 2, 20), (28, 6, 15, 2, 16)],
];

/// Alignment-pattern centre coordinates per version (version 1 has none).
const ALIGNMENT: [&[usize]; 10] =
    [&[], &[6, 18], &[6, 22], &[6, 26], &[6, 30], &[6, 34], &[6, 22, 38], &[6, 24, 42], &[6, 26, 46], &[6, 28, 50]];

fn blocks(version: u8, ecc: QrEcc) -> (usize, usize, usize, usize, usize) {
    BLOCKS[usize::from(version - 1)][ecc.index()]
}

fn data_codewords(version: u8, ecc: QrEcc) -> usize {
    let (_, b1, d1, b2, d2) = blocks(version, ecc);
    b1 * d1 + b2 * d2
}

/// Bits of the byte-mode character count field.
fn count_bits(version: u8) -> usize {
    if version <= 9 { 8 } else { 16 }
}

/// Bytes that fit in `version` at `ecc` in byte mode.
fn byte_capacity(version: u8, ecc: QrEcc) -> usize {
    (data_codewords(version, ecc) * 8 - 4 - count_bits(version)) / 8
}

// ---- GF(256) and Reed–Solomon ----

const fn gf_tables() -> ([u8; 256], [u8; 256]) {
    let mut exp = [0u8; 256];
    let mut log = [0u8; 256];
    let mut x: u16 = 1;
    let mut i = 0;
    while i < 255 {
        exp[i] = x as u8;
        log[x as usize] = i as u8;
        x <<= 1;
        if x & 0x100 != 0 {
            x ^= 0x11D;
        }
        i += 1;
    }
    exp[255] = exp[0];
    (exp, log)
}

const GF: ([u8; 256], [u8; 256]) = gf_tables();

fn gf_mul(a: u8, b: u8) -> u8 {
    if a == 0 || b == 0 {
        return 0;
    }
    let (exp, log) = &GF;
    exp[(usize::from(log[usize::from(a)]) + usize::from(log[usize::from(b)])) % 255]
}

/// `∏_{i<n} (x − αⁱ)`, highest degree first (leading 1 included).
fn generator(n: usize) -> Vec<u8> {
    let mut g = vec![1u8];
    for i in 0..n {
        let root = GF.0[i % 255];
        let mut next = vec![0u8; g.len() + 1];
        for (j, &c) in g.iter().enumerate() {
            next[j] ^= c;
            next[j + 1] ^= gf_mul(c, root);
        }
        g = next;
    }
    g
}

/// The `ec_len` error-correction codewords of `data` (remainder of `data · x^ec_len` by the
/// generator polynomial).
pub fn qr_reed_solomon(data: &[u8], ec_len: usize) -> Vec<u8> {
    let g = generator(ec_len);
    let mut rem = vec![0u8; ec_len];
    for &d in data {
        let factor = d ^ rem.first().copied().unwrap_or(0);
        rem.rotate_left(1);
        if let Some(last) = rem.last_mut() {
            *last = 0;
        }
        for (r, &c) in rem.iter_mut().zip(g.iter().skip(1)) {
            *r ^= gf_mul(c, factor);
        }
    }
    rem
}

/// BCH remainder of `value << degree(poly)` by `poly`.
fn bch(value: u32, poly: u32) -> u32 {
    let degree = 31 - poly.leading_zeros();
    let mut r = value << degree;
    while r != 0 && 31 - r.leading_zeros() >= degree {
        r ^= poly << (31 - r.leading_zeros() - degree);
    }
    r
}

/// The 15 format-information bits (level, mask, BCH(15,5), XOR mask `101010000010010`).
pub fn qr_format_bits(ecc: QrEcc, mask: u8) -> u16 {
    let data = (u32::from(ecc.format_bits()) << 3) | u32::from(mask & 7);
    (((data << 10) | bch(data, 0b101_0011_0111)) ^ 0b101_0100_0001_0010) as u16
}

/// The 18 version-information bits (versions 7 and up; BCH(18,6)).
pub fn qr_version_bits(version: u8) -> Option<u32> {
    if !(7..=40).contains(&version) {
        return None;
    }
    let v = u32::from(version);
    Some((v << 12) | bch(v, 0b1_1111_0010_0101))
}

// ---- data codewords ----

struct Bits(Vec<bool>);

impl Bits {
    fn push(&mut self, value: u32, n: usize) {
        for i in (0..n).rev() {
            self.0.push((value >> i) & 1 == 1);
        }
    }
}

/// Mode, count, bytes, terminator, padding: exactly `data_codewords(version, ecc)` bytes.
fn data_bytes(data: &[u8], version: u8, ecc: QrEcc) -> Vec<u8> {
    let capacity = data_codewords(version, ecc) * 8;
    let mut bits = Bits(Vec::with_capacity(capacity));
    bits.push(0b0100, 4);
    bits.push(data.len() as u32, count_bits(version));
    for &b in data {
        bits.push(u32::from(b), 8);
    }
    let terminator = (capacity - bits.0.len()).min(4);
    bits.push(0, terminator);
    let pad = (8 - bits.0.len() % 8) % 8;
    bits.push(0, pad);
    let mut out: Vec<u8> = bits.0.chunks(8).map(|c| c.iter().fold(0u8, |acc, &b| (acc << 1) | u8::from(b))).collect();
    for pad in [0xEC, 0x11].into_iter().cycle() {
        if out.len() * 8 >= capacity {
            break;
        }
        out.push(pad);
    }
    out
}

/// Splits into blocks, appends error correction and interleaves (data, then EC, column-wise).
fn codewords(data: &[u8], version: u8, ecc: QrEcc) -> Vec<u8> {
    let (ec, b1, d1, b2, d2) = blocks(version, ecc);
    let mut blocks_data = Vec::with_capacity(b1 + b2);
    let mut at = 0;
    for len in std::iter::repeat_n(d1, b1).chain(std::iter::repeat_n(d2, b2)) {
        let end = (at + len).min(data.len());
        blocks_data.push(data.get(at..end).unwrap_or_default().to_vec());
        at = end;
    }
    let blocks_ec: Vec<Vec<u8>> = blocks_data.iter().map(|b| qr_reed_solomon(b, ec)).collect();
    let mut out = Vec::with_capacity(data.len() + ec * blocks_data.len());
    for i in 0..d1.max(d2) {
        out.extend(blocks_data.iter().filter_map(|b| b.get(i)));
    }
    for i in 0..ec {
        out.extend(blocks_ec.iter().filter_map(|b| b.get(i)));
    }
    out
}

// ---- matrix ----

struct Matrix {
    size: usize,
    dark: Vec<bool>,
    function: Vec<bool>,
}

impl Matrix {
    fn set_function(&mut self, row: usize, col: usize, dark: bool) {
        if row < self.size && col < self.size {
            let i = row * self.size + col;
            self.dark[i] = dark;
            self.function[i] = true;
        }
    }

    fn is_function(&self, row: usize, col: usize) -> bool {
        self.function[row * self.size + col]
    }

    fn new(version: u8) -> Self {
        let size = 17 + 4 * usize::from(version);
        let mut m = Matrix { size, dark: vec![false; size * size], function: vec![false; size * size] };
        // Finder patterns with their light separators.
        for (cr, cc) in [(3, 3), (3, size - 4), (size - 4, 3)] {
            for dr in -4i64..=4 {
                for dc in -4i64..=4 {
                    let (r, c) = (cr as i64 + dr, cc as i64 + dc);
                    if r >= 0 && c >= 0 {
                        let d = dr.abs().max(dc.abs());
                        m.set_function(r as usize, c as usize, d != 2 && d != 4);
                    }
                }
            }
        }
        // Alignment patterns, except where they would overlap a finder.
        let centres = ALIGNMENT[usize::from(version - 1)];
        let last = centres.len().saturating_sub(1);
        for (i, &r) in centres.iter().enumerate() {
            for (j, &c) in centres.iter().enumerate() {
                if (i == 0 && j == 0) || (i == 0 && j == last) || (i == last && j == 0) {
                    continue;
                }
                for dr in -2i64..=2 {
                    for dc in -2i64..=2 {
                        let d = dr.abs().max(dc.abs());
                        m.set_function((r as i64 + dr) as usize, (c as i64 + dc) as usize, d != 1);
                    }
                }
            }
        }
        // Timing patterns.
        for i in 0..size {
            if !m.is_function(6, i) {
                m.set_function(6, i, i % 2 == 0);
            }
            if !m.is_function(i, 6) {
                m.set_function(i, 6, i % 2 == 0);
            }
        }
        // Reserved format areas (filled per mask) and the dark module.
        m.draw_format(0);
        // Version information.
        if let Some(bits) = qr_version_bits(version) {
            for i in 0..18 {
                let bit = (bits >> i) & 1 == 1;
                let (a, b) = (size - 11 + i % 3, i / 3);
                m.set_function(b, a, bit);
                m.set_function(a, b, bit);
            }
        }
        m
    }

    /// Both copies of the format bits (bit 0 = least significant) plus the dark module.
    fn draw_format(&mut self, bits: u16) {
        let size = self.size;
        let bit = |i: usize| (bits >> i) & 1 == 1;
        for i in 0..=5 {
            self.set_function(i, 8, bit(i));
        }
        self.set_function(7, 8, bit(6));
        self.set_function(8, 8, bit(7));
        self.set_function(8, 7, bit(8));
        for i in 9..15 {
            self.set_function(8, 14 - i, bit(i));
        }
        for i in 0..8 {
            self.set_function(8, size - 1 - i, bit(i));
        }
        for i in 8..15 {
            self.set_function(size - 15 + i, 8, bit(i));
        }
        self.set_function(size - 8, 8, true);
    }

    /// Codeword bits in the zigzag order from the bottom-right, skipping function modules and
    /// the vertical timing column; remainder bits stay light.
    fn draw_codewords(&mut self, data: &[u8]) {
        let size = self.size;
        let total = data.len() * 8;
        let mut i = 0;
        let mut right = size as i64 - 1;
        while right >= 1 {
            if right == 6 {
                right = 5;
            }
            let upward = ((right + 1) & 2) == 0;
            for vert in 0..size {
                for j in 0..2 {
                    let col = (right - j) as usize;
                    let row = if upward { size - 1 - vert } else { vert };
                    if !self.is_function(row, col) && i < total {
                        let byte = data.get(i >> 3).copied().unwrap_or(0);
                        self.dark[row * size + col] = (byte >> (7 - (i & 7))) & 1 == 1;
                        i += 1;
                    }
                }
            }
            right -= 2;
        }
    }

    fn apply_mask(&mut self, mask: u8) {
        for row in 0..self.size {
            for col in 0..self.size {
                let (r, c) = (row, col);
                let flip = match mask {
                    0 => (r + c) % 2 == 0,
                    1 => r % 2 == 0,
                    2 => c % 3 == 0,
                    3 => (r + c) % 3 == 0,
                    4 => (r / 2 + c / 3) % 2 == 0,
                    5 => (r * c) % 2 + (r * c) % 3 == 0,
                    6 => ((r * c) % 2 + (r * c) % 3) % 2 == 0,
                    _ => ((r + c) % 2 + (r * c) % 3) % 2 == 0,
                };
                let i = row * self.size + col;
                if flip && !self.function[i] {
                    self.dark[i] = !self.dark[i];
                }
            }
        }
    }

    fn at(&self, row: usize, col: usize) -> bool {
        self.dark[row * self.size + col]
    }

    /// Penalty score N1 (runs ≥ 5), N2 (2×2 blocks), N3 (finder-like 1:1:3:1:1 with four light
    /// modules on one side), N4 (dark proportion).
    fn penalty(&self) -> u64 {
        let n = self.size;
        let mut score = 0u64;
        let line = |get: &dyn Fn(usize) -> bool| -> u64 {
            let mut s = 0u64;
            let mut run = 1usize;
            for i in 1..n {
                if get(i) == get(i - 1) {
                    run += 1;
                } else {
                    if run >= 5 {
                        s += 3 + (run - 5) as u64;
                    }
                    run = 1;
                }
            }
            if run >= 5 {
                s += 3 + (run - 5) as u64;
            }
            const A: [bool; 11] = [true, false, true, true, true, false, true, false, false, false, false];
            const B: [bool; 11] = [false, false, false, false, true, false, true, true, true, false, true];
            for start in 0..n.saturating_sub(10) {
                let window: Vec<bool> = (start..start + 11).map(get).collect();
                if window == A || window == B {
                    s += 40;
                }
            }
            s
        };
        for r in 0..n {
            score += line(&|c| self.at(r, c));
        }
        for c in 0..n {
            score += line(&|r| self.at(r, c));
        }
        for r in 0..n - 1 {
            for c in 0..n - 1 {
                let v = self.at(r, c);
                if v == self.at(r, c + 1) && v == self.at(r + 1, c) && v == self.at(r + 1, c + 1) {
                    score += 3;
                }
            }
        }
        let total = (n * n) as u64;
        let dark = self.dark.iter().filter(|d| **d).count() as u64;
        let k = (dark * 20).abs_diff(total * 10) / total;
        score + 10 * k
    }
}

/// Encodes `data` in byte mode at the smallest version that fits, with the best mask.
pub fn qr_encode(data: &[u8], ecc: QrEcc) -> Result<QrCode, GenError> {
    qr_encode_with(data, ecc, None, None)
}

/// Like [`qr_encode`], with an optional fixed version (refused when the data does not fit) and
/// an optional fixed mask (0..=7).
pub fn qr_encode_with(data: &[u8], ecc: QrEcc, version: Option<u8>, mask: Option<u8>) -> Result<QrCode, GenError> {
    if let Some(v) = version
        && !(1..=QR_MAX_VERSION).contains(&v)
    {
        return Err(GenError::Count { param: "version", min: 1, max: u64::from(QR_MAX_VERSION), value: u64::from(v) });
    }
    if let Some(m) = mask
        && m > 7
    {
        return Err(GenError::Count { param: "mask", min: 0, max: 7, value: u64::from(m) });
    }
    let max = byte_capacity(version.unwrap_or(QR_MAX_VERSION), ecc);
    if data.is_empty() || data.len() > max {
        return Err(GenError::Count { param: "data", min: 1, max: max as u64, value: data.len() as u64 });
    }
    let version = match version {
        Some(v) => v,
        None => (1..=QR_MAX_VERSION).find(|v| data.len() <= byte_capacity(*v, ecc)).unwrap_or(QR_MAX_VERSION),
    };
    let words = codewords(&data_bytes(data, version, ecc), version, ecc);
    let mut base = Matrix::new(version);
    base.draw_codewords(&words);
    let build = |m: u8| {
        let mut x = Matrix { size: base.size, dark: base.dark.clone(), function: base.function.clone() };
        x.apply_mask(m);
        x.draw_format(qr_format_bits(ecc, m));
        x
    };
    let (mask, matrix) = match mask {
        Some(m) => (m, build(m)),
        None => {
            let mut best: Option<(u64, u8, Matrix)> = None;
            for m in 0..8u8 {
                let x = build(m);
                let p = x.penalty();
                if best.as_ref().is_none_or(|(bp, _, _)| p < *bp) {
                    best = Some((p, m, x));
                }
            }
            match best {
                Some((_, m, x)) => (m, x),
                None => (0, build(0)),
            }
        }
    };
    Ok(QrCode { version, ecc, mask, size: matrix.size, modules: matrix.dark })
}

/// Physical parameters of a QR symbol.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QrParams {
    /// Module size, mm.
    pub module: f64,
    /// Quiet zone on each side in modules (the standard asks for at least 4).
    pub quiet_modules: u32,
    /// Smallest module the material/laser reproduces (from a named read test).
    pub min_module: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QrSymbol {
    /// Closed rectangles (one per horizontal run of dark modules); y up, origin at the
    /// bottom-left corner of the quiet zone.
    pub path: Path,
    pub code: QrCode,
    /// Side length including quiet zones, mm.
    pub width: f64,
}

/// A QR symbol as geometry.
pub fn qr(data: &[u8], ecc: QrEcc, p: &QrParams) -> Result<QrSymbol, GenError> {
    let module = positive("module", p.module)?;
    let min = positive("min_module", p.min_module)?;
    if module < min {
        return Err(GenError::TooSmall { param: "module", min_mm: min, value_mm: module });
    }
    if !(4..=100).contains(&p.quiet_modules) {
        return Err(GenError::Count { param: "quiet_modules", min: 4, max: 100, value: u64::from(p.quiet_modules) });
    }
    let code = qr_encode(data, ecc)?;
    let n = code.size();
    let q = f64::from(p.quiet_modules);
    let len = |v: f64| Length::from_mm(v).map_err(|_| GenError::NotFinite { param: "module" });
    let mut subpaths = Vec::new();
    for row in 0..n {
        let y = (n - 1 - row) as f64 + q;
        let mut col = 0;
        while col < n {
            if !code.is_dark(row, col) {
                col += 1;
                continue;
            }
            let start = col;
            while col < n && code.is_dark(row, col) {
                col += 1;
            }
            let x = start as f64 + q;
            let r = Rect::new(Point::new(x * module, y * module), len((col - start) as f64 * module)?, len(module)?)?;
            subpaths.extend(Path::rectangle(r).subpaths().iter().cloned());
        }
    }
    Ok(QrSymbol { path: Path::from_subpaths(subpaths), code, width: (n as f64 + 2.0 * q) * module })
}
