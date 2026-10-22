//! G9-editor S07: functions in formulas (`min`, `max`, `abs`, `round`, `sqrt`, ...). Oracles are
//! values worked out by hand; failures are typed.

#![allow(clippy::unwrap_used)] // test helpers

use std::collections::BTreeMap;

use burnburn_document::structure::{FormulaError, parse_formula};
use burnburn_types::Length;

fn eval(text: &str, pairs: &[(&str, f64)]) -> Result<f64, FormulaError> {
    let values: BTreeMap<String, Length> =
        pairs.iter().map(|(k, v)| ((*k).to_owned(), Length::from_mm(*v).unwrap())).collect();
    parse_formula(text)?.eval(&values)
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn min_and_max_take_any_number_of_arguments() {
    assert!(near(eval("min(3, 7)", &[]).unwrap(), 3.0));
    assert!(near(eval("max(3, 7, 5)", &[]).unwrap(), 7.0));
    assert!(near(eval("min(breedte, hoogte) / 2", &[("breedte", 40.0), ("hoogte", 18.0)]).unwrap(), 9.0));
    assert!(near(eval("max(1)", &[]).unwrap(), 1.0));
    assert!(near(eval("MAX(2, 9)", &[]).unwrap(), 9.0), "function names ignore case");
}

#[test]
fn abs_round_floor_ceil_sqrt_behave_like_the_textbook() {
    assert!(near(eval("abs(2 - 9)", &[]).unwrap(), 7.0));
    assert!(near(eval("round(2.5)", &[]).unwrap(), 3.0), "halves round away from zero");
    assert!(near(eval("round(-2.5)", &[]).unwrap(), -3.0));
    assert!(near(eval("round(2.71828, 2)", &[]).unwrap(), 2.72), "a second argument is the number of decimals");
    assert!(near(eval("floor(2.9)", &[]).unwrap(), 2.0));
    assert!(near(eval("ceil(2.1)", &[]).unwrap(), 3.0));
    assert!(near(eval("sqrt(16)", &[]).unwrap(), 4.0));
    assert!(near(eval("sqrt(a*a + b*b)", &[("a", 3.0), ("b", 4.0)]).unwrap(), 5.0), "the diagonal");
}

#[test]
fn pow_and_clamp_and_the_constant_pi() {
    assert!(near(eval("pow(2, 10)", &[]).unwrap(), 1024.0));
    assert!(near(eval("clamp(15, 0, 10)", &[]).unwrap(), 10.0));
    assert!(near(eval("clamp(-3, 0, 10)", &[]).unwrap(), 0.0));
    assert!(near(eval("clamp(4, 0, 10)", &[]).unwrap(), 4.0));
    assert!(near(eval("pi * 2", &[]).unwrap(), std::f64::consts::TAU));
}

#[test]
fn functions_nest_and_combine_with_the_operators() {
    assert!(near(eval("max(min(10, 4), 2) * 3 + abs(-1)", &[]).unwrap(), 13.0));
    assert!(near(eval("-abs(-4)", &[]).unwrap(), -4.0));
    assert!(near(eval("sqrt(max(a, 0))", &[("a", -4.0)]).unwrap(), 0.0));
}

#[test]
fn a_parameter_may_share_a_function_name_when_no_bracket_follows() {
    assert!(near(eval("max + 1", &[("max", 4.0)]).unwrap(), 5.0));
    assert_eq!(parse_formula("min(a, b) + max").unwrap().names(), ["a", "b", "max"], "functions are not parameters");
}

#[test]
fn bad_calls_are_typed_errors() {
    assert_eq!(eval("foo(1)", &[]).map(|_| ()), Err(FormulaError::UnknownFunction("foo".into())));
    for bad in ["min()", "abs(1, 2)", "sqrt()", "round(1, 2, 3)", "pow(2)", "clamp(1, 2)", "max(1,)"] {
        assert!(
            matches!(eval(bad, &[]), Err(FormulaError::Arity(_)) | Err(FormulaError::Syntax)),
            "{bad:?} must not compute"
        );
    }
    assert_eq!(eval("abs(1, 2)", &[]).map(|_| ()), Err(FormulaError::Arity("abs".into())));
    assert_eq!(eval("sqrt(-1)", &[]).map(|_| ()), Err(FormulaError::Domain("sqrt".into())));
    assert_eq!(eval("pow(0, -1)", &[]).map(|_| ()), Err(FormulaError::Domain("pow".into())));
    assert_eq!(eval("min(1, x)", &[]).map(|_| ()), Err(FormulaError::UnknownName("x".into())));
    assert_eq!(eval("min(1", &[]).map(|_| ()), Err(FormulaError::Syntax));
}

#[test]
fn a_comma_before_a_digit_is_a_decimal_comma_and_a_semicolon_always_separates() {
    assert!(near(eval("max(1;5)", &[]).unwrap(), 5.0));
    assert!(near(eval("max(1,5)", &[]).unwrap(), 1.5), "one argument: the number 1,5");
    assert!(near(eval("max(1, 5)", &[]).unwrap(), 5.0));
    assert!(near(eval("max(2,5; 1)", &[]).unwrap(), 2.5));
}
