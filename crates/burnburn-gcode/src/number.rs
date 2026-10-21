//! Fixed-point number formatting for G-code words.
//!
//! Coordinates are quantised once (round half away from zero) to a multiple of `10^-decimals` mm
//! and always written as absolute values, so rounding never accumulates along a path.

/// Largest quantised magnitude (`10^15`): keeps `i64` arithmetic and the decimal text exact.
const MAX_QUANTUM: f64 = 1.0e15;

/// `10^decimals`.
pub(crate) fn scale(decimals: u8) -> i64 {
    10i64.pow(u32::from(decimals.min(9)))
}

/// Quantises `mm` to units of `10^-decimals` mm. `None` for non-finite or absurdly large values.
pub(crate) fn quantise(mm: f64, decimals: u8) -> Option<i64> {
    let q = (mm * scale(decimals) as f64).round();
    if q.is_finite() && q.abs() < MAX_QUANTUM { Some(q as i64) } else { None }
}

/// Decimal text of a quantised value without trailing zeros (`12000` at 3 decimals -> `12`;
/// `-250` -> `-0.25`). Zero is `0`, never `-0`.
pub(crate) fn format(q: i64, decimals: u8) -> String {
    let s = scale(decimals);
    let (sign, a) = if q < 0 { ("-", q.unsigned_abs()) } else { ("", q.unsigned_abs()) };
    let (whole, frac) = (a / s as u64, a % s as u64);
    if frac == 0 {
        return if whole == 0 { "0".into() } else { format!("{sign}{whole}") };
    }
    let digits = format!("{frac:0width$}", width = usize::from(decimals));
    format!("{sign}{whole}.{}", digits.trim_end_matches('0'))
}
