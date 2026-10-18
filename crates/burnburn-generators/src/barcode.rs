//! Code 128 (subsets B and C) as bar rectangles. Symbol patterns from the Code 128 symbology
//! table (ISO/IEC 15417); checked by structural invariants and hand-computed vectors in tests.

use burnburn_geometry::{Path, Point, Rect};
use burnburn_types::Length;

use crate::error::{GenError, positive};

#[rustfmt::skip]
const PATTERNS: [&[u8]; 107] = [
    &[2, 1, 2, 2, 2, 2], &[2, 2, 2, 1, 2, 2], &[2, 2, 2, 2, 2, 1], &[1, 2, 1, 2, 2, 3], &[1, 2, 1, 3, 2, 2], &[1, 3, 1, 2, 2, 2],
    &[1, 2, 2, 2, 1, 3], &[1, 2, 2, 3, 1, 2], &[1, 3, 2, 2, 1, 2], &[2, 2, 1, 2, 1, 3], &[2, 2, 1, 3, 1, 2], &[2, 3, 1, 2, 1, 2],
    &[1, 1, 2, 2, 3, 2], &[1, 2, 2, 1, 3, 2], &[1, 2, 2, 2, 3, 1], &[1, 1, 3, 2, 2, 2], &[1, 2, 3, 1, 2, 2], &[1, 2, 3, 2, 2, 1],
    &[2, 2, 3, 2, 1, 1], &[2, 2, 1, 1, 3, 2], &[2, 2, 1, 2, 3, 1], &[2, 1, 3, 2, 1, 2], &[2, 2, 3, 1, 1, 2], &[3, 1, 2, 1, 3, 1],
    &[3, 1, 1, 2, 2, 2], &[3, 2, 1, 1, 2, 2], &[3, 2, 1, 2, 2, 1], &[3, 1, 2, 2, 1, 2], &[3, 2, 2, 1, 1, 2], &[3, 2, 2, 2, 1, 1],
    &[2, 1, 2, 1, 2, 3], &[2, 1, 2, 3, 2, 1], &[2, 3, 2, 1, 2, 1], &[1, 1, 1, 3, 2, 3], &[1, 3, 1, 1, 2, 3], &[1, 3, 1, 3, 2, 1],
    &[1, 1, 2, 3, 1, 3], &[1, 3, 2, 1, 1, 3], &[1, 3, 2, 3, 1, 1], &[2, 1, 1, 3, 1, 3], &[2, 3, 1, 1, 1, 3], &[2, 3, 1, 3, 1, 1],
    &[1, 1, 2, 1, 3, 3], &[1, 1, 2, 3, 3, 1], &[1, 3, 2, 1, 3, 1], &[1, 1, 3, 1, 2, 3], &[1, 1, 3, 3, 2, 1], &[1, 3, 3, 1, 2, 1],
    &[3, 1, 3, 1, 2, 1], &[2, 1, 1, 3, 3, 1], &[2, 3, 1, 1, 3, 1], &[2, 1, 3, 1, 1, 3], &[2, 1, 3, 3, 1, 1], &[2, 1, 3, 1, 3, 1],
    &[3, 1, 1, 1, 2, 3], &[3, 1, 1, 3, 2, 1], &[3, 3, 1, 1, 2, 1], &[3, 1, 2, 1, 1, 3], &[3, 1, 2, 3, 1, 1], &[3, 3, 2, 1, 1, 1],
    &[3, 1, 4, 1, 1, 1], &[2, 2, 1, 4, 1, 1], &[4, 3, 1, 1, 1, 1], &[1, 1, 1, 2, 2, 4], &[1, 1, 1, 4, 2, 2], &[1, 2, 1, 1, 2, 4],
    &[1, 2, 1, 4, 2, 1], &[1, 4, 1, 1, 2, 2], &[1, 4, 1, 2, 2, 1], &[1, 1, 2, 2, 1, 4], &[1, 1, 2, 4, 1, 2], &[1, 2, 2, 1, 1, 4],
    &[1, 2, 2, 4, 1, 1], &[1, 4, 2, 1, 1, 2], &[1, 4, 2, 2, 1, 1], &[2, 4, 1, 2, 1, 1], &[2, 2, 1, 1, 1, 4], &[4, 1, 3, 1, 1, 1],
    &[2, 4, 1, 1, 1, 2], &[1, 3, 4, 1, 1, 1], &[1, 1, 1, 2, 4, 2], &[1, 2, 1, 1, 4, 2], &[1, 2, 1, 2, 4, 1], &[1, 1, 4, 2, 1, 2],
    &[1, 2, 4, 1, 1, 2], &[1, 2, 4, 2, 1, 1], &[4, 1, 1, 2, 1, 2], &[4, 2, 1, 1, 1, 2], &[4, 2, 1, 2, 1, 1], &[2, 1, 2, 1, 4, 1],
    &[2, 1, 4, 1, 2, 1], &[4, 1, 2, 1, 2, 1], &[1, 1, 1, 1, 4, 3], &[1, 1, 1, 3, 4, 1], &[1, 3, 1, 1, 4, 1], &[1, 1, 4, 1, 1, 3],
    &[1, 1, 4, 3, 1, 1], &[4, 1, 1, 1, 1, 3], &[4, 1, 1, 3, 1, 1], &[1, 1, 3, 1, 4, 1], &[1, 1, 4, 1, 3, 1], &[3, 1, 1, 1, 4, 1],
    &[4, 1, 1, 1, 3, 1], &[2, 1, 1, 4, 1, 2], &[2, 1, 1, 2, 1, 4], &[2, 1, 1, 2, 3, 2], &[2, 3, 3, 1, 1, 1, 2],
];

const START_B: u8 = 104;
const START_C: u8 = 105;
const STOP: u8 = 106;
/// Longest accepted input (characters).
const MAX_DATA: usize = 256;

/// The 107 symbol patterns as bar/space module widths (bar first); index = symbol value.
/// Value 106 is the stop pattern (7 elements, 13 modules).
pub fn code128_patterns() -> &'static [&'static [u8]] {
    &PATTERNS
}

/// Symbol values: start, data, check symbol, stop. Subset C for an all-digit input of even
/// length, otherwise subset B (ASCII 32..=127).
pub fn code128_symbols(data: &str) -> Result<Vec<u8>, GenError> {
    let count = data.chars().count();
    if count == 0 || count > MAX_DATA {
        return Err(GenError::Count { param: "data", min: 1, max: MAX_DATA as u64, value: count as u64 });
    }
    let digits = data.bytes().all(|b| b.is_ascii_digit());
    let mut symbols = Vec::with_capacity(count + 3);
    if digits && count.is_multiple_of(2) {
        symbols.push(START_C);
        for pair in data.as_bytes().chunks(2) {
            if let [a, b] = pair {
                symbols.push((a - b'0') * 10 + (b - b'0'));
            }
        }
    } else {
        symbols.push(START_B);
        for (index, ch) in data.chars().enumerate() {
            let v = u32::from(ch);
            if !(32..=127).contains(&v) {
                return Err(GenError::UnsupportedChar { ch, index });
            }
            symbols.push((v - 32) as u8);
        }
    }
    let weighted: u64 = symbols.iter().enumerate().map(|(i, &v)| (i.max(1) as u64) * u64::from(v)).sum();
    symbols.push((weighted % 103) as u8);
    symbols.push(STOP);
    Ok(symbols)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Code128Params {
    /// Narrowest bar, mm.
    pub module: f64,
    pub height: f64,
    /// Quiet zone on each side in modules (the standard asks for at least 10).
    pub quiet_modules: u32,
    /// Smallest module the material/laser reproduces (from a named read test).
    pub min_module: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Barcode {
    /// Closed bar rectangles, x from 0 (left edge of the quiet zone).
    pub path: Path,
    pub symbols: Vec<u8>,
    /// Bar/space widths in modules, bar first, without quiet zones.
    pub widths: Vec<u8>,
    /// Total width including quiet zones, mm.
    pub width: f64,
}

pub fn code128(data: &str, p: &Code128Params) -> Result<Barcode, GenError> {
    let module = positive("module", p.module)?;
    let height = positive("height", p.height)?;
    let min = positive("min_module", p.min_module)?;
    if module < min {
        return Err(GenError::TooSmall { param: "module", min_mm: min, value_mm: module });
    }
    if !(10..=100).contains(&p.quiet_modules) {
        return Err(GenError::Count { param: "quiet_modules", min: 10, max: 100, value: u64::from(p.quiet_modules) });
    }
    let symbols = code128_symbols(data)?;
    let widths: Vec<u8> =
        symbols.iter().filter_map(|&v| PATTERNS.get(usize::from(v))).flat_map(|p| p.iter().copied()).collect();
    let quiet = f64::from(p.quiet_modules);
    let mut x = quiet;
    let mut bars = Vec::new();
    let len = |v: f64| Length::from_mm(v).map_err(|_| GenError::NotFinite { param: "module" });
    for (i, &w) in widths.iter().enumerate() {
        let w = f64::from(w);
        if i % 2 == 0 {
            let r = Rect::new(Point::new(x * module, 0.0), len(w * module)?, len(height)?)?;
            bars.extend(Path::rectangle(r).subpaths().iter().cloned());
        }
        x += w;
    }
    Ok(Barcode { path: Path::from_subpaths(bars), symbols, widths, width: (x + quiet) * module })
}
