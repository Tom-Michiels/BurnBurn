use std::fmt;

use burnburn_types::Length;

/// Why the text of a length field cannot be committed. Shown next to the field; no command is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FieldError {
    Empty,
    NotANumber,
    /// Both `,` and `.` present (`1,000.5`): ambiguous between locales, so rejected.
    AmbiguousSeparator,
    NotFinite,
    NotPositive,
}

impl fmt::Display for FieldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            FieldError::Empty => "empty value",
            FieldError::NotANumber => "not a number",
            FieldError::AmbiguousSeparator => "ambiguous decimal separator",
            FieldError::NotFinite => "value out of range",
            FieldError::NotPositive => "value must be greater than zero",
        })
    }
}

impl std::error::Error for FieldError {}

/// Parses a positive length typed in a native field: `150`, `150 mm`, `15cm`, `5,9 in`, `5.9"`.
/// A number without unit is millimetres (the v1 document display unit). Either `,` or `.` is
/// the decimal separator; exponents, `inf` and `NaN` are rejected. Expressions follow in IM11.
pub fn parse_length(text: &str) -> Result<Length, FieldError> {
    let length = parse_signed(text)?;
    if length.mm() > 0.0 { Ok(length) } else { Err(FieldError::NotPositive) }
}

/// Parses a coordinate typed in the X/Y field: like [`parse_length`], but zero and negative
/// values are valid positions.
pub fn parse_position(text: &str) -> Result<Length, FieldError> {
    parse_signed(text)
}

fn parse_signed(text: &str) -> Result<Length, FieldError> {
    let t = text.trim();
    if t.is_empty() {
        return Err(FieldError::Empty);
    }
    let lower = t.to_ascii_lowercase();
    let (number, mm_per_unit) = [("mm", 1.0), ("cm", 10.0), ("in", 25.4), ("\"", 25.4)]
        .iter()
        .find_map(|(suffix, f)| lower.strip_suffix(suffix).map(|n| (n.trim_end().to_owned(), *f)))
        .unwrap_or((lower.clone(), 1.0));
    if number.is_empty() {
        return Err(FieldError::Empty);
    }
    if number.contains(',') && number.contains('.') {
        return Err(FieldError::AmbiguousSeparator);
    }
    let normalised = number.replace(',', ".");
    let well_formed = normalised.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '+' || c == '-')
        && normalised.chars().filter(|c| *c == '.').count() <= 1
        && normalised.chars().any(|c| c.is_ascii_digit());
    if !well_formed {
        return Err(FieldError::NotANumber);
    }
    let value: f64 = normalised.parse().map_err(|_| FieldError::NotANumber)?;
    // `-0` is the position zero.
    let value = if value == 0.0 { 0.0 } else { value };
    Length::from_mm(value * mm_per_unit).map_err(|_| FieldError::NotFinite)
}

/// Parses an angle in degrees typed in the rotation field: `30`, `-45,5`, `90°`, `90 deg`.
/// Zero and negative angles are valid; the separator rules are those of [`parse_length`].
pub fn parse_angle(text: &str) -> Result<f64, FieldError> {
    let t = text.trim();
    let lower = t.to_ascii_lowercase();
    let number = ["°", "deg"].iter().find_map(|suffix| lower.strip_suffix(suffix)).unwrap_or(&lower).trim_end();
    // Same number grammar as a length in millimetres; the value is the angle in degrees.
    let value = parse_signed(number)?.mm();
    if value.abs() > 1e6 { Err(FieldError::NotFinite) } else { Ok(value) }
}

/// Parses a plain number with an optional unit suffix (`100 mm/s`, `35,5 %`); same number
/// grammar as a length (decimal comma or point, no exponent, finite).
pub fn parse_number(text: &str, suffixes: &[&str]) -> Result<f64, FieldError> {
    let lower = text.trim().to_ascii_lowercase();
    let number = suffixes.iter().find_map(|suffix| lower.strip_suffix(suffix)).unwrap_or(&lower).trim_end();
    parse_signed(number).map(Length::mm)
}

/// Field display text in millimetres: at most three decimals, trailing zeros trimmed.
pub fn format_mm(mm: f64) -> String {
    let s = format!("{mm:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".to_owned() } else { s.to_owned() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mm(text: &str) -> Result<f64, FieldError> {
        parse_length(text).map(Length::mm)
    }

    #[test]
    fn units_and_decimal_separators_give_the_same_physical_value() {
        assert_eq!(mm("150"), Ok(150.0));
        assert_eq!(mm(" 150 mm "), Ok(150.0));
        assert_eq!(mm("15cm"), Ok(150.0));
        assert_eq!(mm("5,9 in"), mm("5.9in"));
        assert!((mm("5.9\"").unwrap() - 149.86).abs() < 1e-9);
        assert_eq!(mm("0,5"), Ok(0.5));
    }

    #[test]
    fn invalid_text_is_rejected_with_a_reason() {
        assert_eq!(mm(""), Err(FieldError::Empty));
        assert_eq!(mm("mm"), Err(FieldError::Empty));
        assert_eq!(mm("abc"), Err(FieldError::NotANumber));
        assert_eq!(mm("NaN"), Err(FieldError::NotANumber));
        assert_eq!(mm("inf"), Err(FieldError::NotANumber));
        assert_eq!(mm("1e999"), Err(FieldError::NotANumber));
        assert_eq!(mm("1.2.3"), Err(FieldError::NotANumber));
        assert_eq!(mm("1,000.5"), Err(FieldError::AmbiguousSeparator));
        assert_eq!(mm("0"), Err(FieldError::NotPositive));
        assert_eq!(mm("-4"), Err(FieldError::NotPositive));
        let huge = "9".repeat(400);
        assert_eq!(mm(&huge), Err(FieldError::NotFinite));
    }

    #[test]
    fn positions_accept_zero_and_negative_values_with_units() {
        let pos = |t: &str| parse_position(t).map(Length::mm);
        assert_eq!(pos("0"), Ok(0.0));
        assert_eq!(pos("-12,5"), Ok(-12.5));
        assert_eq!(pos("-1 in"), Ok(-25.4));
        assert_eq!(pos("abc"), Err(FieldError::NotANumber));
        assert_eq!(pos(""), Err(FieldError::Empty));
        assert_eq!(pos("1,000.5"), Err(FieldError::AmbiguousSeparator));
    }

    #[test]
    fn formats_millimetres_without_noise() {
        assert_eq!(format_mm(120.0), "120");
        assert_eq!(format_mm(149.86), "149.86");
        assert_eq!(format_mm(0.12345), "0.123");
        assert_eq!(format_mm(-0.0001), "0");
    }
}
