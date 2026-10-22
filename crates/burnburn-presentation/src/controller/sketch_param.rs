//! Typed text of sketch parameters and dimensions (S02): the decimal comma a Dutch user types, and
//! which parameters are still in use. Pure functions over text and the sketch definition.

use burnburn_document::sketch::{Sketch, SketchConstraint};
use burnburn_sketch::param::Expression;

/// Reads a decimal comma as a decimal point: `100,5 mm` becomes `100.5 mm`.
///
/// Only a comma directly between two digits is touched, and only where it cannot separate function
/// arguments: at the top level or inside grouping parentheses. A comma inside the parentheses of a
/// call (`max(80 mm, 90 mm)`, `min(1,5)`) stays an argument separator; to use a decimal comma
/// there, write the point (assumption: the evaluator's call syntax wins over the locale).
pub(crate) fn normalise_decimal_commas(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    // One entry per open parenthesis: `true` when it opens a function call.
    let mut calls: Vec<bool> = Vec::new();
    let mut out = String::with_capacity(text.len());
    for (i, &c) in chars.iter().enumerate() {
        match c {
            '(' => {
                let previous = chars[..i].iter().rev().find(|p| !p.is_whitespace());
                calls.push(previous.is_some_and(|p| p.is_alphanumeric() || *p == '_'));
                out.push(c);
            }
            ')' => {
                calls.pop();
                out.push(c);
            }
            ',' if calls.last().copied() != Some(true)
                && i > 0
                && chars[i - 1].is_ascii_digit()
                && chars.get(i + 1).is_some_and(char::is_ascii_digit) =>
            {
                out.push('.');
            }
            _ => out.push(c),
        }
    }
    out
}

/// Whether a dimension of `sketch`, or a parameter other than `name`, refers to parameter `name`.
pub(super) fn parameter_in_use(sketch: &Sketch, name: &str) -> bool {
    let uses = |text: &str| Expression::parse(text).is_ok_and(|e| e.references().contains(&name));
    sketch.constraints().any(|(_, c)| matches!(c, SketchConstraint::Dimension(d) if uses(&d.text)))
        || sketch.parameters().iter().any(|(other, text)| other != name && uses(text))
}

#[cfg(test)]
mod tests {
    use super::normalise_decimal_commas as n;

    #[test]
    fn a_comma_between_digits_outside_a_call_becomes_a_point() {
        assert_eq!(n("100,5 mm"), "100.5 mm");
        assert_eq!(n("breedte + 2,25 mm"), "breedte + 2.25 mm");
        assert_eq!(n("(1,5 mm + 2,5 mm) * 2"), "(1.5 mm + 2.5 mm) * 2");
    }

    #[test]
    fn commas_that_separate_arguments_or_are_not_between_digits_stay() {
        assert_eq!(n("max(80 mm, 90 mm)"), "max(80 mm, 90 mm)");
        assert_eq!(n("min(1,5)"), "min(1,5)");
        assert_eq!(n("max((1,5 mm), 2 mm)"), "max((1.5 mm), 2 mm)");
        assert_eq!(n("1, 5"), "1, 5");
        assert_eq!(n(",5"), ",5");
        assert_eq!(n("5,"), "5,");
        assert_eq!(n(""), "");
    }
}
