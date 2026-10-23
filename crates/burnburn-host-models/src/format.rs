//! The one place that turns numbers, quantities and counts into display text (LF3, Q8): decimal
//! sign and digit grouping follow the UI language, units are a suffix and never part of a label,
//! counts agree with their noun. Fields keep their own round-trippable text
//! (`burnburn_presentation::field`); this module is for labels, summaries and messages.

use crate::l10n::Locale;

/// A unit a quantity can be shown in. Only suffixes live here; the physical value never depends
/// on the display unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quantity {
    Millimeters,
    Inches,
    Degrees,
    Percent,
    Seconds,
    Milliseconds,
    MillimetersPerSecond,
    Pixels,
    DotsPerInch,
    SquareMillimeters,
}

/// The display length unit chosen in the preferences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthPreference {
    Millimeters,
    Inches,
}

/// A countable thing, with its NL/EN singular and plural.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Noun {
    Object,
    Layer,
    Copy,
    Error,
    Warning,
    Operation,
}

/// `value` with at most `max_decimals` decimals (trailing zeros trimmed), the decimal sign of
/// `locale` and digit groups of three from 1000 up.
/// A value that is not finite shows a dash.
pub fn format_number(value: f64, max_decimals: usize, locale: Locale) -> String {
    if !value.is_finite() {
        return "—".to_owned();
    }
    let raw = format!("{value:.max_decimals$}");
    let (negative, unsigned) = match raw.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, raw.as_str()),
    };
    let (int_part, frac_part) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    let frac_part = frac_part.trim_end_matches('0');
    let group = match locale {
        Locale::Nl => '.',
        Locale::En => ',',
    };
    let mut out = String::new();
    let is_zero = int_part.chars().all(|c| c == '0') && frac_part.is_empty();
    if negative && !is_zero {
        out.push('-');
    }
    for (i, digit) in int_part.chars().enumerate() {
        if i > 0 && (int_part.len() - i) % 3 == 0 {
            out.push(group);
        }
        out.push(digit);
    }
    if !frac_part.is_empty() {
        out.push(locale.decimal_sign());
        out.push_str(frac_part);
    }
    out
}

fn suffix(unit: Quantity) -> &'static str {
    match unit {
        Quantity::Millimeters => " mm",
        Quantity::Inches => " in",
        Quantity::Degrees => "°",
        Quantity::Percent => " %",
        Quantity::Seconds => " s",
        Quantity::Milliseconds => " ms",
        Quantity::MillimetersPerSecond => " mm/s",
        Quantity::Pixels => " px",
        Quantity::DotsPerInch => " dpi",
        Quantity::SquareMillimeters => " mm²",
    }
}

/// The unit as it stands after a control: `mm`, `°`, `%` (no leading space).
pub fn unit_label(unit: Quantity) -> &'static str {
    suffix(unit).trim_start()
}

fn decimals(unit: Quantity) -> usize {
    match unit {
        Quantity::Degrees | Quantity::Percent | Quantity::Seconds => 2,
        _ => 3,
    }
}

/// `value` followed by the unit suffix: `1,5 mm`, `45°`, `80 %`.
pub fn format_quantity(value: f64, unit: Quantity, locale: Locale) -> String {
    format!("{}{}", format_number(value, decimals(unit), locale), suffix(unit))
}

const MM_PER_INCH: f64 = 25.4;

fn in_unit(mm: f64, unit: LengthPreference) -> (f64, Quantity) {
    match unit {
        LengthPreference::Millimeters => (mm, Quantity::Millimeters),
        LengthPreference::Inches => (mm / MM_PER_INCH, Quantity::Inches),
    }
}

/// A length given in millimetres, shown in the preferred unit (`in` converts, three decimals).
pub fn format_length(mm: f64, unit: LengthPreference, locale: Locale) -> String {
    let (value, quantity) = in_unit(mm, unit);
    format_quantity(value, quantity, locale)
}

/// `250 × 160 mm`: the unit once, after both numbers.
pub fn format_size(width_mm: f64, height_mm: f64, unit: LengthPreference, locale: Locale) -> String {
    let (w, quantity) = in_unit(width_mm, unit);
    let (h, _) = in_unit(height_mm, unit);
    let d = decimals(quantity);
    format!("{} × {}{}", format_number(w, d, locale), format_number(h, d, locale), suffix(quantity))
}

fn noun_forms(noun: Noun, locale: Locale) -> (&'static str, &'static str) {
    match (noun, locale) {
        (Noun::Object, Locale::Nl) => ("object", "objecten"),
        (Noun::Object, Locale::En) => ("object", "objects"),
        (Noun::Layer, Locale::Nl) => ("laag", "lagen"),
        (Noun::Layer, Locale::En) => ("layer", "layers"),
        (Noun::Copy, Locale::Nl) => ("kopie", "kopieën"),
        (Noun::Copy, Locale::En) => ("copy", "copies"),
        (Noun::Error, Locale::Nl) => ("fout", "fouten"),
        (Noun::Error, Locale::En) => ("error", "errors"),
        (Noun::Warning, Locale::Nl) => ("waarschuwing", "waarschuwingen"),
        (Noun::Warning, Locale::En) => ("warning", "warnings"),
        (Noun::Operation, Locale::Nl) => ("bewerking", "bewerkingen"),
        (Noun::Operation, Locale::En) => ("operation", "operations"),
    }
}

/// `1 object`, `2 objecten`: the count with the noun in the right number.
pub fn format_count(count: usize, noun: Noun, locale: Locale) -> String {
    let (one, many) = noun_forms(noun, locale);
    format!("{} {}", format_number(count as f64, 0, locale), if count == 1 { one } else { many })
}

#[cfg(test)]
mod tests {
    use super::*;
    use burnburn_presentation::parse_length;

    #[test]
    fn dutch_uses_a_comma_and_trims_noise_decimals() {
        assert_eq!(format_quantity(1.5, Quantity::Millimeters, Locale::Nl), "1,5 mm");
        assert_eq!(format_quantity(1.5, Quantity::Millimeters, Locale::En), "1.5 mm");
        assert_eq!(format_quantity(250.0, Quantity::Millimeters, Locale::Nl), "250 mm");
        assert_eq!(format_quantity(160.000_000_4, Quantity::Millimeters, Locale::Nl), "160 mm");
        assert_eq!(format_quantity(-0.000_1, Quantity::Millimeters, Locale::Nl), "0 mm");
    }

    #[test]
    fn thousands_are_grouped_per_language() {
        assert_eq!(format_number(250_000.0, 3, Locale::Nl), "250.000");
        assert_eq!(format_number(250_000.0, 3, Locale::En), "250,000");
        assert_eq!(format_number(1234.5, 3, Locale::Nl), "1.234,5");
        assert_eq!(format_number(999.0, 3, Locale::Nl), "999");
        assert_eq!(format_number(-12345.678, 2, Locale::En), "-12,345.68");
    }

    #[test]
    fn non_finite_numbers_show_a_dash_not_nan() {
        assert_eq!(format_number(f64::NAN, 3, Locale::Nl), "—");
        assert_eq!(format_number(f64::INFINITY, 3, Locale::En), "—");
    }

    #[test]
    fn units_are_suffixes() {
        assert_eq!(format_quantity(45.0, Quantity::Degrees, Locale::Nl), "45°");
        assert_eq!(format_quantity(80.0, Quantity::Percent, Locale::Nl), "80 %");
        assert_eq!(format_quantity(1.6, Quantity::Seconds, Locale::Nl), "1,6 s");
        assert_eq!(format_quantity(300.0, Quantity::MillimetersPerSecond, Locale::En), "300 mm/s");
        assert_eq!(format_quantity(12.0, Quantity::Pixels, Locale::En), "12 px");
    }

    #[test]
    fn inch_preference_converts_and_names_the_unit() {
        assert_eq!(format_length(25.4, LengthPreference::Inches, Locale::En), "1 in");
        assert_eq!(format_length(38.1, LengthPreference::Inches, Locale::Nl), "1,5 in");
        assert_eq!(format_length(38.1, LengthPreference::Millimeters, Locale::Nl), "38,1 mm");
    }

    #[test]
    fn a_formatted_length_parses_back_to_the_same_physical_value() {
        for (mm, unit) in [
            (1.5, LengthPreference::Millimeters),
            (38.1, LengthPreference::Inches),
            (0.25, LengthPreference::Millimeters),
        ] {
            for locale in [Locale::Nl, Locale::En] {
                let text = format_length(mm, unit, locale);
                let back = parse_length(&text).expect("parses").mm();
                assert!((back - mm).abs() < 1e-3, "{text} -> {back} != {mm}");
            }
        }
    }

    #[test]
    fn a_size_shows_the_unit_once_without_noise() {
        assert_eq!(format_size(250.0, 160.0, LengthPreference::Millimeters, Locale::Nl), "250 × 160 mm");
        assert_eq!(format_size(250.5, 160.25, LengthPreference::Millimeters, Locale::Nl), "250,5 × 160,25 mm");
        assert_eq!(format_size(25.4, 50.8, LengthPreference::Inches, Locale::En), "1 × 2 in");
    }

    #[test]
    fn counts_agree_with_their_noun() {
        assert_eq!(format_count(0, Noun::Object, Locale::Nl), "0 objecten");
        assert_eq!(format_count(1, Noun::Object, Locale::Nl), "1 object");
        assert_eq!(format_count(2, Noun::Object, Locale::Nl), "2 objecten");
        assert_eq!(format_count(1, Noun::Object, Locale::En), "1 object");
        assert_eq!(format_count(2, Noun::Object, Locale::En), "2 objects");
        assert_eq!(format_count(1, Noun::Error, Locale::Nl), "1 fout");
        assert_eq!(format_count(3, Noun::Warning, Locale::Nl), "3 waarschuwingen");
        assert_eq!(format_count(1, Noun::Copy, Locale::Nl), "1 kopie");
        assert_eq!(format_count(2, Noun::Copy, Locale::Nl), "2 kopieën");
        assert_eq!(format_count(1200, Noun::Object, Locale::Nl), "1.200 objecten");
    }
}
