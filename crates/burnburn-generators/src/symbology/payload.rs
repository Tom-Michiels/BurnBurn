//! Typed payload validation: the rules each symbology puts on its text before any module is drawn.
//! Check digits and tables here are computed from the published definitions (GS1 General
//! Specifications 7.9, ISO 2108 / ISBN, 49 CFR 565 for VIN), independently of the encoder library.

use super::{SymbolError, SymbolInputMode, Symbology};

/// Longest payload (characters) any symbology accepts; a guard against accidental huge input.
pub(super) const MAX_PAYLOAD_CHARS: usize = 10_000;

/// Payload bytes for the 2D byte modes: `Data` is one byte per character (U+0000..=U+00FF), `Unicode`
/// is UTF-8.
pub(super) fn payload_bytes(
    symbology: Symbology,
    payload: &str,
    mode: SymbolInputMode,
) -> Result<Vec<u8>, SymbolError> {
    match mode {
        SymbolInputMode::Unicode => Ok(payload.as_bytes().to_vec()),
        SymbolInputMode::Data => payload
            .chars()
            .enumerate()
            .map(|(index, ch)| {
                u8::try_from(u32::from(ch)).map_err(|_| SymbolError::InvalidChar { symbology, ch, index })
            })
            .collect(),
        SymbolInputMode::Gs1 => Err(SymbolError::InputModeUnavailable { symbology, mode }),
    }
}

fn invalid(symbology: Symbology, rule: &'static str) -> SymbolError {
    SymbolError::InvalidPayload { symbology, rule }
}

/// The payload as ASCII digit values; any other character is reported with its position.
fn digits(symbology: Symbology, payload: &str) -> Result<Vec<u8>, SymbolError> {
    payload
        .chars()
        .enumerate()
        .map(|(index, ch)| {
            ch.to_digit(10).filter(|_| ch.is_ascii_digit()).map(|d| d as u8).ok_or(SymbolError::InvalidChar {
                symbology,
                ch,
                index,
            })
        })
        .collect()
}

fn digit_char(d: u8) -> char {
    char::from(b'0' + d)
}

fn text(d: &[u8]) -> String {
    d.iter().map(|&x| digit_char(x)).collect()
}

/// GS1 check digit (modulo 10, weights 3, 1, 3, ... from the right) for the data digits `d`.
pub(super) fn mod10(d: &[u8]) -> u8 {
    let sum: u32 = d.iter().rev().enumerate().map(|(i, &x)| u32::from(x) * if i % 2 == 0 { 3 } else { 1 }).sum();
    ((10 - sum % 10) % 10) as u8
}

/// `data_len` digits get their check digit computed; `data_len + 1` digits have theirs verified.
fn with_check(symbology: Symbology, d: &[u8], data_len: usize) -> Result<Vec<u8>, SymbolError> {
    let want = mod10(&d[..data_len]);
    let mut out = d[..data_len].to_vec();
    if d.len() == data_len + 1 && d[data_len] != want {
        return Err(SymbolError::CheckDigit { symbology, expected: digit_char(want), found: digit_char(d[data_len]) });
    }
    out.push(want);
    Ok(out)
}

/// EAN-13 (12 or 13 digits) and EAN-8 (7 or 8 digits); returns the full digits.
pub(super) fn ean(payload: &str) -> Result<String, SymbolError> {
    let d = digits(Symbology::Ean, payload)?;
    match d.len() {
        7 | 8 => Ok(text(&with_check(Symbology::Ean, &d, 7)?)),
        12 | 13 => Ok(text(&with_check(Symbology::Ean, &d, 12)?)),
        _ => Err(invalid(Symbology::Ean, "EAN needs 7 or 8 digits (EAN-8) or 12 or 13 digits (EAN-13)")),
    }
}

/// UPC-A: 11 digits (check computed) or 12 (verified).
pub(super) fn upc_a(payload: &str) -> Result<String, SymbolError> {
    let d = digits(Symbology::UpcA, payload)?;
    match d.len() {
        11 | 12 => Ok(text(&with_check(Symbology::UpcA, &d, 11)?)),
        _ => Err(invalid(Symbology::UpcA, "UPC-A needs 11 digits or 12 digits with the check digit")),
    }
}

/// Zero-suppressed UPC-E payload digits `d1..d6` expanded to the 11 data digits of the UPC-A it stands for
/// (number system first), following the suppression rules of the UPC symbology specification.
fn upce_expand(ns: u8, p: &[u8; 6]) -> [u8; 11] {
    let [d1, d2, d3, d4, d5, d6] = *p;
    let body: [u8; 10] = match d6 {
        0..=2 => [d1, d2, d6, 0, 0, 0, 0, d3, d4, d5],
        3 => [d1, d2, d3, 0, 0, 0, 0, 0, d4, d5],
        4 => [d1, d2, d3, d4, 0, 0, 0, 0, 0, d5],
        _ => [d1, d2, d3, d4, d5, 0, 0, 0, 0, d6],
    };
    let mut out = [0u8; 11];
    out[0] = ns;
    out[1..].copy_from_slice(&body);
    out
}

/// UPC-E: 6 digits (number system 0 assumed), 7 digits (number system first) or 8 digits (with the check
/// digit of the expanded UPC-A). Returns the canonical 8 digits.
pub(super) fn upc_e(payload: &str) -> Result<String, SymbolError> {
    let d = digits(Symbology::UpcE, payload)?;
    let (ns, body, given): (u8, &[u8], Option<u8>) = match d.len() {
        6 => (0, &d[..], None),
        7 => (d[0], &d[1..], None),
        8 => (d[0], &d[1..7], Some(d[7])),
        _ => return Err(invalid(Symbology::UpcE, "UPC-E needs 6, 7 or 8 digits")),
    };
    if ns > 1 {
        return Err(invalid(Symbology::UpcE, "UPC-E number system must be 0 or 1"));
    }
    let p: [u8; 6] = body.try_into().map_err(|_| invalid(Symbology::UpcE, "UPC-E needs 6 payload digits"))?;
    let check = mod10(&upce_expand(ns, &p));
    if let Some(found) = given
        && found != check
    {
        return Err(SymbolError::CheckDigit {
            symbology: Symbology::UpcE,
            expected: digit_char(check),
            found: digit_char(found),
        });
    }
    let mut out = vec![ns];
    out.extend_from_slice(body);
    out.push(check);
    Ok(text(&out))
}

/// ISBN-13 (978/979 prefix) or ISBN-10 (converted to ISBN-13); hyphens and spaces are separators.
/// Returns the 13 digits.
pub(super) fn isbn(payload: &str) -> Result<String, SymbolError> {
    let symbology = Symbology::Isbn;
    let mut chars: Vec<(usize, char)> = Vec::new();
    for (index, ch) in payload.chars().enumerate() {
        match ch {
            '-' | ' ' => {}
            _ => chars.push((index, ch)),
        }
    }
    let last = chars.len().saturating_sub(1);
    for (pos, &(index, ch)) in chars.iter().enumerate() {
        let x_check = chars.len() == 10 && pos == last && (ch == 'X' || ch == 'x');
        if !ch.is_ascii_digit() && !x_check {
            return Err(SymbolError::InvalidChar { symbology, ch, index });
        }
    }
    match chars.len() {
        13 => {
            let d: Vec<u8> = chars.iter().map(|&(_, c)| c as u8 - b'0').collect();
            if d[..3] != [9, 7, 8] && d[..3] != [9, 7, 9] {
                return Err(invalid(symbology, "ISBN-13 starts with 978 or 979"));
            }
            Ok(text(&with_check(symbology, &d, 12)?))
        }
        10 => {
            let d: Vec<u32> = chars[..9].iter().map(|&(_, c)| c as u32 - u32::from(b'0')).collect();
            let sum: u32 = d.iter().enumerate().map(|(i, &x)| x * (10 - i as u32)).sum();
            let want = (11 - sum % 11) % 11;
            let (_, last_ch) = chars[9];
            let found = if last_ch == 'X' || last_ch == 'x' { 10 } else { last_ch as u32 - u32::from(b'0') };
            if found != want {
                let expected = if want == 10 { 'X' } else { char::from(b'0' + want as u8) };
                return Err(SymbolError::CheckDigit { symbology, expected, found: last_ch });
            }
            let mut d13: Vec<u8> = vec![9, 7, 8];
            d13.extend(d.iter().map(|&x| x as u8));
            d13.push(mod10(&d13));
            Ok(text(&d13))
        }
        _ => Err(invalid(symbology, "ISBN needs 10 or 13 digits")),
    }
}

/// Transliteration value of a VIN character (49 CFR 565.15).
fn vin_value(c: char) -> Option<u32> {
    Some(match c {
        '0'..='9' => c as u32 - '0' as u32,
        'A' | 'J' => 1,
        'B' | 'K' | 'S' => 2,
        'C' | 'L' | 'T' => 3,
        'D' | 'M' | 'U' => 4,
        'E' | 'N' | 'V' => 5,
        'F' | 'W' => 6,
        'G' | 'P' | 'X' => 7,
        'H' | 'Y' => 8,
        'R' | 'Z' => 9,
        _ => return None,
    })
}

/// VIN: 17 characters of A-Z (no I, O, Q) and digits. The position-9 check digit (49 CFR 565.15, weights
/// 8 7 6 5 4 3 2 10 0 9 8 7 6 5 4 3 2, modulo 11, remainder 10 = X) is verified for North American VINs
/// (first character 1-5), where it is mandatory; elsewhere ISO 3779 does not define one.
pub(super) fn vin(payload: &str) -> Result<String, SymbolError> {
    let symbology = Symbology::Vin;
    let chars: Vec<char> = payload.chars().map(|c| c.to_ascii_uppercase()).collect();
    for (index, (&c, orig)) in chars.iter().zip(payload.chars()).enumerate() {
        if vin_value(c).is_none() {
            return Err(SymbolError::InvalidChar { symbology, ch: orig, index });
        }
    }
    if chars.len() != 17 {
        return Err(SymbolError::PayloadLength { symbology, min: 17, max: 17, len: chars.len() });
    }
    if matches!(chars[0], '1'..='5') {
        const WEIGHTS: [u32; 17] = [8, 7, 6, 5, 4, 3, 2, 10, 0, 9, 8, 7, 6, 5, 4, 3, 2];
        let sum: u32 = chars.iter().zip(WEIGHTS).map(|(&c, w)| vin_value(c).unwrap_or(0) * w).sum();
        let want = match sum % 11 {
            10 => 'X',
            r => char::from(b'0' + r as u8),
        };
        if chars[8] != want {
            return Err(SymbolError::CheckDigit { symbology, expected: want, found: chars[8] });
        }
    }
    Ok(chars.into_iter().collect())
}

/// Codabar: a start and a stop letter (A-D) around at least one of `0-9 - $ : / . +`. Returns the
/// upper-cased payload, start and stop letters included.
pub(super) fn codabar(payload: &str) -> Result<String, SymbolError> {
    let symbology = Symbology::Codabar;
    let chars: Vec<char> = payload.chars().map(|c| c.to_ascii_uppercase()).collect();
    let guard = |c: char| matches!(c, 'A'..='D');
    if chars.len() < 3 || !guard(chars[0]) || !guard(chars[chars.len() - 1]) {
        return Err(invalid(symbology, "Codabar needs a start and a stop letter (A-D) around at least one character"));
    }
    for (index, (&c, orig)) in chars.iter().zip(payload.chars()).enumerate() {
        let inner = index > 0 && index + 1 < chars.len();
        if inner && !matches!(c, '0'..='9' | '-' | '$' | ':' | '/' | '.' | '+') {
            return Err(SymbolError::InvalidChar { symbology, ch: orig, index });
        }
    }
    Ok(chars.into_iter().collect())
}

/// Plessey: hexadecimal digits; returns them upper-cased.
pub(super) fn plessey(payload: &str) -> Result<String, SymbolError> {
    let mut out = String::new();
    for (index, ch) in payload.chars().enumerate() {
        if !ch.is_ascii_hexdigit() {
            return Err(SymbolError::InvalidChar { symbology: Symbology::Plessey, ch, index });
        }
        out.push(ch.to_ascii_uppercase());
    }
    Ok(out)
}

/// Digits only (2 of 5 family); returns the digit characters.
pub(super) fn digit_string(symbology: Symbology, payload: &str) -> Result<String, SymbolError> {
    digits(symbology, payload).map(|d| text(&d))
}

/// Code 93 (without the full-ASCII extension): `0-9 A-Z - . space $ / + %`.
pub(super) fn code93(payload: &str) -> Result<String, SymbolError> {
    for (index, ch) in payload.chars().enumerate() {
        if !(ch.is_ascii_digit() || ch.is_ascii_uppercase() || matches!(ch, '-' | '.' | ' ' | '$' | '/' | '+' | '%')) {
            return Err(SymbolError::InvalidChar { symbology: Symbology::Code93, ch, index });
        }
    }
    Ok(payload.to_string())
}

/// A marker id: decimal digits, no sign; leading zeros are dropped.
pub(super) fn marker_id(symbology: Symbology, payload: &str) -> Result<u32, SymbolError> {
    let d = digits(symbology, payload)?;
    let significant: Vec<u8> = d.iter().copied().skip_while(|&x| x == 0).collect();
    if significant.len() > 9 {
        return Err(invalid(symbology, "marker id has more than 9 digits"));
    }
    Ok(significant.iter().fold(0u32, |acc, &x| acc * 10 + u32::from(x)))
}

// ---------------------------------------------------------------------------------------------
// GS1 element strings
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Charset {
    Numeric,
    /// GS1 character set 82.
    Cset82,
}

struct Ai {
    ai: &'static str,
    min: usize,
    max: usize,
    charset: Charset,
    /// Digits `0..` of the AI whose last digit is a check digit (GTIN / SSCC / GLN).
    check: bool,
}

const fn ai(ai: &'static str, min: usize, max: usize, charset: Charset, check: bool) -> Ai {
    Ai { ai, min, max, charset, check }
}

/// The application identifiers this generator knows (lengths and formats of the GS1 General
/// Specifications); everything else is refused as unknown rather than guessed.
const AI_TABLE: &[Ai] = &[
    ai("00", 18, 18, Charset::Numeric, true),
    ai("01", 14, 14, Charset::Numeric, true),
    ai("02", 14, 14, Charset::Numeric, true),
    ai("10", 1, 20, Charset::Cset82, false),
    ai("11", 6, 6, Charset::Numeric, false),
    ai("12", 6, 6, Charset::Numeric, false),
    ai("13", 6, 6, Charset::Numeric, false),
    ai("15", 6, 6, Charset::Numeric, false),
    ai("16", 6, 6, Charset::Numeric, false),
    ai("17", 6, 6, Charset::Numeric, false),
    ai("20", 2, 2, Charset::Numeric, false),
    ai("21", 1, 20, Charset::Cset82, false),
    ai("240", 1, 30, Charset::Cset82, false),
    ai("241", 1, 30, Charset::Cset82, false),
    ai("30", 1, 8, Charset::Numeric, false),
    ai("3100", 6, 6, Charset::Numeric, false),
    ai("3101", 6, 6, Charset::Numeric, false),
    ai("3102", 6, 6, Charset::Numeric, false),
    ai("3103", 6, 6, Charset::Numeric, false),
    ai("3104", 6, 6, Charset::Numeric, false),
    ai("3105", 6, 6, Charset::Numeric, false),
    ai("3200", 6, 6, Charset::Numeric, false),
    ai("3201", 6, 6, Charset::Numeric, false),
    ai("3202", 6, 6, Charset::Numeric, false),
    ai("3203", 6, 6, Charset::Numeric, false),
    ai("3204", 6, 6, Charset::Numeric, false),
    ai("3205", 6, 6, Charset::Numeric, false),
    ai("400", 1, 30, Charset::Cset82, false),
    ai("410", 13, 13, Charset::Numeric, true),
    ai("411", 13, 13, Charset::Numeric, true),
    ai("412", 13, 13, Charset::Numeric, true),
    ai("413", 13, 13, Charset::Numeric, true),
    ai("414", 13, 13, Charset::Numeric, true),
    ai("415", 13, 13, Charset::Numeric, true),
    ai("420", 1, 20, Charset::Cset82, false),
    ai("90", 1, 30, Charset::Cset82, false),
    ai("91", 1, 30, Charset::Cset82, false),
    ai("92", 1, 30, Charset::Cset82, false),
    ai("93", 1, 30, Charset::Cset82, false),
    ai("94", 1, 30, Charset::Cset82, false),
    ai("95", 1, 30, Charset::Cset82, false),
    ai("96", 1, 30, Charset::Cset82, false),
    ai("97", 1, 30, Charset::Cset82, false),
    ai("98", 1, 30, Charset::Cset82, false),
    ai("99", 1, 30, Charset::Cset82, false),
];

/// AIs whose element string has a predefined length need no FNC1 after them: the two-digit prefixes
/// 00-04, 11-20, 31-36 and 41 (GS1 General Specifications 5.4.7.2). Every other element, fixed or
/// variable, is followed by FNC1 when more data follows.
pub(super) fn predefined_length(ai: &str) -> bool {
    matches!(
        ai.get(..2),
        Some(
            "00" | "01"
                | "02"
                | "03"
                | "04"
                | "11"
                | "12"
                | "13"
                | "14"
                | "15"
                | "16"
                | "17"
                | "18"
                | "19"
                | "20"
                | "31"
                | "32"
                | "33"
                | "34"
                | "35"
                | "36"
                | "41"
        )
    )
}

fn cset82(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            '!' | '"'
                | '%'
                | '&'
                | '\''
                | '('
                | ')'
                | '*'
                | '+'
                | ','
                | '-'
                | '.'
                | '/'
                | ':'
                | ';'
                | '<'
                | '='
                | '>'
                | '?'
                | '_'
        )
}

/// One parsed element of a GS1 element string.
pub(super) struct Element {
    pub ai: String,
    pub value: String,
}

/// Maximum data characters of a GS1-128 symbol, application identifiers included (FNC1 excluded).
const GS1_128_MAX_CHARS: usize = 48;

/// Parses `(AI)value(AI)value...`, validating each AI against the table (length, character set, check
/// digit).
pub(super) fn gs1(payload: &str) -> Result<Vec<Element>, SymbolError> {
    let symbology = Symbology::Code128Gs1;
    let chars: Vec<char> = payload.chars().collect();
    if chars.first() != Some(&'(') {
        return Err(invalid(symbology, "GS1 element strings are written (AI)value(AI)value"));
    }
    let mut out: Vec<Element> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '(' {
            return Err(invalid(symbology, "expected '(' before an application identifier"));
        }
        let close = chars[i..].iter().position(|&c| c == ')').ok_or_else(|| invalid(symbology, "unclosed '('"))? + i;
        let ai_text: String = chars[i + 1..close].iter().collect();
        if !(2..=4).contains(&ai_text.len()) || !ai_text.chars().all(|c| c.is_ascii_digit()) {
            return Err(invalid(symbology, "an application identifier is 2 to 4 digits"));
        }
        let spec = AI_TABLE.iter().find(|a| a.ai == ai_text).ok_or(SymbolError::UnknownAi { ai: ai_text.clone() })?;
        let start = close + 1;
        let end = chars[start..].iter().position(|&c| c == '(').map_or(chars.len(), |p| p + start);
        let value: String = chars[start..end].iter().collect();
        let len = end - start;
        if len == 0 {
            return Err(invalid(symbology, "an application identifier needs a value"));
        }
        for (k, &c) in chars[start..end].iter().enumerate() {
            let ok = match spec.charset {
                Charset::Numeric => c.is_ascii_digit(),
                Charset::Cset82 => cset82(c),
            };
            if !ok {
                return Err(SymbolError::InvalidChar { symbology, ch: c, index: start + k });
            }
        }
        if len < spec.min || len > spec.max {
            return Err(invalid(symbology, "value length does not match the application identifier"));
        }
        if spec.check {
            let d: Vec<u8> = value.bytes().map(|b| b - b'0').collect();
            let want = mod10(&d[..d.len() - 1]);
            let found = d[d.len() - 1];
            if want != found {
                return Err(SymbolError::CheckDigit {
                    symbology,
                    expected: digit_char(want),
                    found: digit_char(found),
                });
            }
        }
        out.push(Element { ai: ai_text, value });
        i = end;
    }
    let data_chars: usize = out.iter().map(|e| e.ai.len() + e.value.len()).sum();
    if data_chars > GS1_128_MAX_CHARS {
        return Err(invalid(symbology, "GS1-128 carries at most 48 data characters"));
    }
    Ok(out)
}
