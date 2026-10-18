//! Parameter parser: units, expressions, functions, cycles, unit checks and positioned errors.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test helpers outside #[test] fns

use burnburn_sketch::param::{Dim, ExprErrorKind, Expression, ParameterError, Parameters, Span, Value};
use proptest::prelude::*;

fn mm(params: &Parameters, name: &str) -> f64 {
    match params.evaluate().expect("evaluates").get(name) {
        Some(Value::Length(l)) => l.mm(),
        other => panic!("{name} is not a length: {other:?}"),
    }
}

fn table(lines: &[&str]) -> Parameters {
    let mut p = Parameters::new();
    for l in lines {
        p.define(l).expect(l);
    }
    p
}

fn expr_err(text: &str) -> (ExprErrorKind, Span) {
    let e = Expression::parse(text).and_then(|x| x.evaluate(&|_| None)).expect_err("must fail");
    (e.kind, e.span)
}

#[test]
fn lengths_with_units_are_canonical_millimetres() {
    let p = table(&["width = 120 mm", "a = 1in", "b = 12cm", "c = 0.12 m", "d = 2.5e1mm"]);
    assert_eq!(mm(&p, "width"), 120.0);
    assert_eq!(mm(&p, "a"), 25.4);
    assert_eq!(mm(&p, "b"), 120.0);
    assert_eq!(mm(&p, "c"), 120.0);
    assert_eq!(mm(&p, "d"), 25.0);
}

#[test]
fn expressions_reference_other_parameters() {
    let mut p = table(&["width = 120 mm", "hole_offset = width/2 - 15 mm"]);
    assert_eq!(mm(&p, "hole_offset"), 45.0);
    p.set("width", "150 mm").expect("set");
    assert_eq!(mm(&p, "hole_offset"), 60.0);
    // Precedence and parentheses.
    let p = table(&["x = 2 mm + 3 * 4 mm", "y = (2 mm + 3 mm) * 4", "z = -x + 20mm"]);
    assert_eq!(mm(&p, "x"), 14.0);
    assert_eq!(mm(&p, "y"), 20.0);
    assert_eq!(mm(&p, "z"), 6.0);
}

#[test]
fn min_max_and_sqrt_respect_dimensions() {
    let p = table(&["a = min(3 mm, 2 mm, 5 mm)", "b = max(3 mm, 1in)", "c = sqrt(9 mm * 4 mm)", "d = sqrt(2) * 10 mm"]);
    assert_eq!(mm(&p, "a"), 2.0);
    assert_eq!(mm(&p, "b"), 25.4);
    assert_eq!(mm(&p, "c"), 6.0);
    assert_eq!(mm(&p, "d"), 2f64.sqrt() * 10.0);
    let (k, _) = expr_err("sqrt(9 mm)");
    assert!(matches!(k, ExprErrorKind::OddDimensionSqrt(_)), "{k:?}");
    let (k, _) = expr_err("min(3 mm, 2 deg)");
    assert!(matches!(k, ExprErrorKind::UnitMismatch { .. }), "{k:?}");
    let (k, s) = expr_err("max(3 mm)");
    assert!(matches!(k, ExprErrorKind::WrongArgumentCount { .. }), "{k:?}");
    assert_eq!(s, Span { start: 0, end: 9 });
    let (k, s) = expr_err("foo(3 mm)");
    assert_eq!(k, ExprErrorKind::UnknownFunction("foo".into()));
    assert_eq!(s, Span { start: 0, end: 3 });
}

#[test]
fn angles_accept_degrees_and_radians() {
    let p = table(&["a = 45 deg", "b = 0.5 rad", "c = a * 2"]);
    let v = p.evaluate().expect("ok");
    let Some(Value::Angle(a)) = v.get("a") else { panic!("angle") };
    assert!((a.radians() - std::f64::consts::FRAC_PI_4).abs() < 1e-15);
    let Some(Value::Angle(b)) = v.get("b") else { panic!("angle") };
    assert_eq!(b.radians(), 0.5);
    let Some(Value::Angle(c)) = v.get("c") else { panic!("angle") };
    assert!((c.degrees() - 90.0).abs() < 1e-12);
    let p = table(&["n = 3 * 4"]);
    assert_eq!(p.evaluate().expect("ok").get("n"), Some(Value::Number(12.0)));
}

#[test]
fn adding_millimetres_to_degrees_is_a_unit_error_at_the_operator() {
    let (k, s) = expr_err("10 mm + 5 deg");
    assert_eq!(k, ExprErrorKind::UnitMismatch { left: Dim::LENGTH, right: Dim::ANGLE });
    assert_eq!(s, Span { start: 6, end: 7 });
    // A bare number is dimensionless; adding it to a length is an error too.
    let (k, _) = expr_err("10 mm + 5");
    assert_eq!(k, ExprErrorKind::UnitMismatch { left: Dim::LENGTH, right: Dim::NONE });
}

#[test]
fn syntax_errors_carry_positions() {
    assert_eq!(expr_err("120 mm *"), (ExprErrorKind::UnexpectedEnd, Span { start: 8, end: 8 }));
    assert_eq!(expr_err("12 $"), (ExprErrorKind::UnexpectedCharacter('$'), Span { start: 3, end: 4 }));
    assert_eq!(expr_err("2 foo"), (ExprErrorKind::UnknownUnit("foo".into()), Span { start: 2, end: 5 }));
    assert_eq!(expr_err("(1 mm"), (ExprErrorKind::UnexpectedEnd, Span { start: 5, end: 5 }));
    assert_eq!(expr_err("1 mm 2"), (ExprErrorKind::UnexpectedToken, Span { start: 5, end: 6 }));
    assert_eq!(expr_err("1e999 mm"), (ExprErrorKind::NotFinite, Span { start: 0, end: 8 }));
    assert_eq!(expr_err("4 mm / (2 - 2)"), (ExprErrorKind::DivisionByZero, Span { start: 5, end: 6 }));
    assert_eq!(expr_err("sqrt(0 - 4)"), (ExprErrorKind::NegativeSqrt, Span { start: 0, end: 11 }));
}

#[test]
fn unknown_parameter_is_rejected_with_its_position_and_nothing_changes() {
    let mut p = table(&["width = 120 mm"]);
    let err = p.define("hole = widht / 2").expect_err("typo");
    match err {
        ParameterError::Expression { parameter, error } => {
            assert_eq!(parameter, "hole");
            assert_eq!(error.kind, ExprErrorKind::UnknownParameter("widht".into()));
            assert_eq!(error.span, Span { start: 7, end: 12 }, "position within the defining line");
        }
        other => panic!("{other:?}"),
    }
    assert!(p.expression("hole").is_none());
}

#[test]
fn cycles_are_detected_and_leave_the_table_unchanged() {
    let mut p = table(&["a = 1 mm", "b = a + 1 mm", "c = b * 2"]);
    let err = p.set("a", "c - 1 mm").expect_err("cycle");
    assert_eq!(err, ParameterError::Cycle { cycle: vec!["a".into(), "c".into(), "b".into(), "a".into()] });
    assert_eq!(mm(&p, "c"), 4.0, "old definitions still apply");
    let err = p.set("a", "a + 1 mm").expect_err("self cycle");
    assert_eq!(err, ParameterError::Cycle { cycle: vec!["a".into(), "a".into()] });
}

#[test]
fn names_are_checked() {
    let mut p = Parameters::new();
    assert!(matches!(p.define("mm = 3 mm"), Err(ParameterError::InvalidName { .. })));
    assert!(matches!(p.define("sqrt = 3 mm"), Err(ParameterError::InvalidName { .. })));
    assert!(matches!(p.define("3x = 3 mm"), Err(ParameterError::InvalidName { .. })));
    assert!(matches!(p.define("width 3 mm"), Err(ParameterError::Expression { .. })));
    assert!(matches!(p.set("nope", "3 mm"), Err(ParameterError::UnknownParameter(_))));
    p.define("w = 3 mm").expect("ok");
    p.define("v = w").expect("ok");
    assert!(matches!(p.remove("w"), Err(ParameterError::StillReferenced { .. })));
    p.remove("v").expect("unreferenced");
}

#[test]
fn redefining_keeps_order_and_updates_dependents() {
    let mut p = table(&["w = 3 mm", "v = w * 2"]);
    p.define("w = 5 mm").expect("redefine");
    assert_eq!(mm(&p, "v"), 10.0);
    let names: Vec<_> = p.names().collect();
    assert_eq!(names, ["w", "v"]);
}

proptest! {
    #[test]
    fn printed_lengths_parse_back_exactly(x in -1.0e6f64..1.0e6) {
        let p = table(&[&format!("a = {x} mm")]);
        prop_assert_eq!(mm(&p, "a"), x);
    }

    #[test]
    fn evaluation_is_deterministic(w in 1.0f64..1000.0, o in 0.0f64..100.0) {
        let lines = [format!("w = {w} mm"), format!("o = {o} mm"), "h = w/2 - o".to_owned()];
        let a = table(&lines.iter().map(String::as_str).collect::<Vec<_>>());
        let b = table(&lines.iter().map(String::as_str).collect::<Vec<_>>());
        prop_assert_eq!(mm(&a, "h").to_bits(), mm(&b, "h").to_bits());
    }
}

#[test]
fn stored_definitions_load_in_any_dependency_order() {
    let p = Parameters::from_definitions([("hole", "width/2 - 15 mm"), ("width", "120 mm")]).expect("valid");
    assert_eq!(mm(&p, "hole"), 45.0);
    assert!(matches!(Parameters::from_definitions([("a", "b"), ("b", "a")]), Err(ParameterError::Cycle { .. })));
    assert!(matches!(Parameters::from_definitions([("in", "1 mm")]), Err(ParameterError::InvalidName { .. })));
}
