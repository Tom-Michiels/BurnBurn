//! G8-editor S07: a number of a part shape can be a formula over the parameters of the part
//! (`breedte / 2 + 3`). Oracles are the arithmetic worked out by hand; an instance follows its
//! parameters through the formula, and a formula that cannot be computed fails that instance
//! only, naming the parameter at fault.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns; let-else on expected variants

use std::collections::BTreeMap;

use burnburn_document::structure::{
    Dim, FormulaError, NamedShape, PartDefinition, PartDefinitionId, PartInstance, PartInstanceId, PartShape,
    StructureChange, StructureCommand, parse_formula,
};
use burnburn_document::{
    CommandOutcome, CommandPayload, DerivedOwner, DocumentCommand, DocumentSession, EvalDiagnosis, EvalStatus,
};
use burnburn_geometry::{Affine, Vector};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn values(pairs: &[(&str, f64)]) -> BTreeMap<String, Length> {
    pairs.iter().map(|(k, v)| ((*k).to_owned(), mm(*v))).collect()
}

fn eval(text: &str, pairs: &[(&str, f64)]) -> Result<f64, FormulaError> {
    parse_formula(text)?.eval(&values(pairs))
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn arithmetic_follows_the_usual_precedence() {
    assert!(near(eval("10 + 2 * 3", &[]).unwrap(), 16.0));
    assert!(near(eval("(10 + 2) * 3", &[]).unwrap(), 36.0));
    assert!(near(eval("-4 + 10 / 4", &[]).unwrap(), -1.5));
    assert!(near(eval("2 - 3 - 4", &[]).unwrap(), -5.0), "left to right");
    assert!(near(eval("100 / 5 / 2", &[]).unwrap(), 10.0));
    assert!(near(eval("1,5 * 4", &[]).unwrap(), 6.0), "a decimal comma, as in the fields");
    assert!(near(eval("10 mm * 2", &[]).unwrap(), 20.0), "a unit after a number is millimetres");
    assert!(near(eval("--3", &[]).unwrap(), 3.0));
    assert!(near(eval("+7", &[]).unwrap(), 7.0));
}

#[test]
fn names_are_the_parameters_of_the_part() {
    assert!(near(eval("breedte / 2 + 3", &[("breedte", 40.0)]).unwrap(), 23.0));
    assert!(near(eval("2 * (a + b)", &[("a", 3.0), ("b", 4.0)]).unwrap(), 14.0));
    assert!(near(eval("rand_links", &[("rand_links", 5.0)]).unwrap(), 5.0), "underscores belong to a name");
    assert_eq!(parse_formula("breedte / 2 + hoogte * breedte").unwrap().names(), ["breedte", "hoogte"]);
}

#[test]
fn what_cannot_be_computed_is_a_typed_error() {
    assert_eq!(eval("breedte / 0", &[("breedte", 4.0)]), Err(FormulaError::DivisionByZero));
    assert_eq!(eval("x + 1", &[]), Err(FormulaError::UnknownName("x".into())));
    for bad in ["", "   ", "3 +", "(3", "3)", "3 3", "a b", "2 ** 3", "* 3", "3 $ 4", "1.2.3", "()"] {
        assert_eq!(parse_formula(bad).map(|_| ()), Err(FormulaError::Syntax), "{bad:?}");
    }
    // A number that overflows is not a number.
    assert_eq!(eval("1e999 * 1e999", &[]).map(|_| ()), Err(FormulaError::Syntax));
}

// --- in a part ---------------------------------------------------------------------------------------------

fn run(s: &mut DocumentSession, payload: CommandPayload) -> CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap()
}

/// A plate `breedte × hoogte` with a hole in the middle: width `breedte / 2 + 3` and the
/// radius a fifth of the height.
fn part() -> PartDefinition {
    let mut shapes = BTreeMap::new();
    shapes.insert(
        1,
        NamedShape {
            name: "omtrek".into(),
            shape: PartShape::Rectangle {
                x: Dim::Mm(0.0),
                y: Dim::Mm(0.0),
                width: Dim::Expr("breedte / 2 + 3".into()),
                height: Dim::Param("hoogte".into()),
            },
        },
    );
    shapes.insert(
        2,
        NamedShape {
            name: "gat".into(),
            shape: PartShape::Circle {
                x: Dim::Expr("(breedte / 2 + 3) / 2".into()),
                y: Dim::Expr("hoogte / 2".into()),
                radius: Dim::Expr("hoogte / 5".into()),
            },
        },
    );
    PartDefinition {
        name: "Plaatje".into(),
        parameters: BTreeMap::from([("breedte".to_owned(), mm(40.0)), ("hoogte".to_owned(), mm(20.0))]),
        shapes,
        variants: BTreeMap::new(),
    }
}

fn define_and_place(s: &mut DocumentSession) -> (PartDefinitionId, PartInstanceId, Vec<GeometryId>) {
    let out = run(s, CommandPayload::Structure(StructureCommand::DefinePart { definition: part() }));
    let StructureChange::Definition(d) = out.changes.structure[0] else { panic!() };
    let inst = PartInstance {
        definition: d,
        variant: None,
        overrides: BTreeMap::new(),
        transform: Affine::translate(Vector::new(0.0, 0.0)).unwrap(),
        plate: None,
    };
    let out = run(s, CommandPayload::Structure(StructureCommand::PlaceInstance { instance: inst }));
    let StructureChange::Instance(i) = out.changes.structure[0] else { panic!() };
    (d, i, out.changes.created)
}

fn bounds(s: &DocumentSession, id: GeometryId) -> (f64, f64, f64, f64) {
    let b = s.snapshot().object(id).unwrap().bounds().unwrap();
    (b.min().x, b.min().y, b.max().x, b.max().y)
}

#[test]
fn shapes_follow_their_formulas_and_the_parameters() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let (_, inst, out) = define_and_place(&mut s);
    // breedte 40: width 23, hole at (11.5, 10) with radius 4.
    assert_eq!(bounds(&s, out[0]), (0.0, 0.0, 23.0, 20.0));
    let (x0, y0, x1, y1) = bounds(&s, out[1]);
    assert!(near(x0, 7.5) && near(x1, 15.5) && near(y0, 6.0) && near(y1, 14.0), "{:?}", (x0, y0, x1, y1));
    // An override on the instance moves everything through the formulas.
    let mut next = s.snapshot().structure().instance(inst).unwrap().clone();
    next.overrides.insert("breedte".into(), mm(100.0));
    next.overrides.insert("hoogte".into(), mm(50.0));
    run(&mut s, CommandPayload::Structure(StructureCommand::SetInstance { instance: inst, value: next }));
    assert_eq!(bounds(&s, out[0]), (0.0, 0.0, 53.0, 50.0));
    let (x0, y0, x1, y1) = bounds(&s, out[1]);
    assert!(near(x0, 16.5) && near(x1, 36.5) && near(y0, 15.0) && near(y1, 35.0), "{:?}", (x0, y0, x1, y1));
}

#[test]
fn a_formula_that_fails_fails_that_instance_and_names_the_parameter() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let (_, inst, out) = define_and_place(&mut s);
    // breedte = -10: the width -2 is not a size; the formula's own parameter is named.
    let mut next = s.snapshot().structure().instance(inst).unwrap().clone();
    next.overrides.insert("breedte".into(), mm(-10.0));
    run(&mut s, CommandPayload::Structure(StructureCommand::SetInstance { instance: inst, value: next }));
    let snap = s.snapshot();
    assert_eq!(
        snap.derived_record(DerivedOwner::Part(inst)).unwrap().status,
        EvalStatus::Failed { diagnosis: EvalDiagnosis::InvalidParameters { parameter: Some("breedte".into()) } }
    );
    assert_eq!(bounds(&s, out[0]), (0.0, 0.0, 23.0, 20.0), "the last valid outline stays");
    assert!(snap.production_blocked(out[0]));
}

#[test]
fn a_formula_must_parse_and_name_parameters_of_the_part_when_the_part_is_defined() {
    let define = |expr: &str| {
        let mut s = DocumentSession::new(DocumentId::from_raw(1));
        let mut def = part();
        if let Some(shape) = def.shapes.get_mut(&1) {
            shape.shape = PartShape::Rectangle {
                x: Dim::Mm(0.0),
                y: Dim::Mm(0.0),
                width: Dim::Expr(expr.into()),
                height: Dim::Mm(20.0),
            };
        }
        let cmd = DocumentCommand {
            document: s.id(),
            expected: s.revision(),
            request: RequestId::from_raw(1),
            payload: CommandPayload::Structure(StructureCommand::DefinePart { definition: def }),
        };
        s.apply(cmd).map(|_| ())
    };
    assert!(define("breedte / 2").is_ok());
    let unknown = format!("{:?}", define("breed / 2").unwrap_err());
    assert!(unknown.contains("UnknownParameter") && unknown.contains("breed"), "{unknown}");
    let syntax = format!("{:?}", define("((").unwrap_err());
    assert!(syntax.contains("InvalidValue") && syntax.contains("width"), "{syntax}");
    // The formula text is the numbers of a shape only: an empty formula is not one either.
    assert!(define("").is_err());
}

#[test]
fn a_formula_changes_the_inputs_hash_and_a_plain_part_keeps_its_old_one() {
    // Part definitions without formulas hash as they always did: the tag only appears with one.
    let plain = {
        let mut p = part();
        for shape in p.shapes.values_mut() {
            shape.shape = PartShape::Circle { x: Dim::Mm(1.0), y: Dim::Mm(2.0), radius: Dim::Mm(3.0) };
        }
        p
    };
    let mut other = plain.clone();
    if let Some(shape) = other.shapes.get_mut(&1) {
        shape.shape = PartShape::Circle { x: Dim::Mm(1.0), y: Dim::Mm(2.0), radius: Dim::Expr("3".into()) };
    }
    let hash = |def: &PartDefinition| {
        let mut w = burnburn_types::CanonicalWriter::new();
        burnburn_types::CanonicalEncode::encode(def, &mut w);
        w.finish().unwrap()
    };
    assert_ne!(hash(&plain), hash(&other), "the formula is part of the definition's identity");
    assert_ne!(
        hash(&other),
        {
            let mut again = other.clone();
            if let Some(shape) = again.shapes.get_mut(&1) {
                shape.shape = PartShape::Circle { x: Dim::Mm(1.0), y: Dim::Mm(2.0), radius: Dim::Expr("4".into()) };
            }
            hash(&again)
        },
        "and so is its text"
    );
}
