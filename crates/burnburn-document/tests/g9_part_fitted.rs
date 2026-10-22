//! G9-editor S07: fixed geometry in a part whose box follows parameters and formulas. The outline
//! keeps its own shape and is stretched to the box `x, y, width, height`.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns; let-else on expected variants

use std::collections::BTreeMap;

use burnburn_document::structure::{
    Dim, NamedShape, PartDefinition, PartDefinitionId, PartInstance, PartInstanceId, PartShape, StructureChange,
    StructureCommand,
};
use burnburn_document::{
    CommandOutcome, CommandPayload, DerivedOwner, DocumentCommand, DocumentSession, EvalDiagnosis, EvalStatus,
};
use burnburn_geometry::{Affine, Path, Point, Vector};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

fn run(s: &mut DocumentSession, payload: CommandPayload) -> CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap()
}

/// The triangle (2,3) (22,3) (12,13): box 20 x 10 at (2, 3), area 100.
fn triangle() -> Path {
    Path::polyline(&[Point::new(2.0, 3.0), Point::new(22.0, 3.0), Point::new(12.0, 13.0)], true).unwrap()
}

fn fitted(x: Dim, y: Dim, width: Dim, height: Dim) -> PartShape {
    PartShape::Fitted { path: triangle(), x, y, width, height }
}

fn definition(shape: PartShape) -> PartDefinition {
    PartDefinition {
        name: "Driehoek".into(),
        parameters: BTreeMap::from([("breedte".to_owned(), mm(40.0)), ("px".to_owned(), mm(10.0))]),
        shapes: BTreeMap::from([(1, NamedShape { name: "vorm".into(), shape })]),
        variants: BTreeMap::new(),
    }
}

fn define_and_place(s: &mut DocumentSession, shape: PartShape) -> (PartDefinitionId, PartInstanceId, GeometryId) {
    let out = run(s, CommandPayload::Structure(StructureCommand::DefinePart { definition: definition(shape) }));
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
    // A failed instance has no output: the id is then a placeholder nobody reads.
    (d, i, out.changes.created.first().copied().unwrap_or(GeometryId::from_raw(0)))
}

fn bounds(s: &DocumentSession, id: GeometryId) -> (f64, f64, f64, f64) {
    let b = s.snapshot().object(id).unwrap().bounds().unwrap();
    (b.min().x, b.min().y, b.max().x, b.max().y)
}

fn area(s: &DocumentSession, id: GeometryId) -> f64 {
    s.snapshot().object(id).unwrap().path().signed_area().abs()
}

#[test]
fn the_outline_is_stretched_to_a_box_that_follows_parameters_and_formulas() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let shape = fitted(Dim::Param("px".into()), Dim::Mm(5.0), Dim::Expr("breedte / 2".into()), Dim::Mm(10.0));
    let (_, inst, id) = define_and_place(&mut s, shape);
    // breedte 40 -> width 20 = the outline's own width: only moved to (10, 5).
    assert_eq!(bounds(&s, id), (10.0, 5.0, 30.0, 15.0));
    assert!(near(area(&s, id), 100.0));
    // breedte 20 -> width 10: half as wide, so half the area (the shape is stretched, not cut).
    let mut next = s.snapshot().structure().instance(inst).unwrap().clone();
    next.overrides.insert("breedte".into(), mm(20.0));
    run(&mut s, CommandPayload::Structure(StructureCommand::SetInstance { instance: inst, value: next }));
    let (x0, y0, x1, y1) = bounds(&s, id);
    assert!(near(x0, 10.0) && near(y0, 5.0) && near(x1, 20.0) && near(y1, 15.0), "{:?}", (x0, y0, x1, y1));
    assert!(near(area(&s, id), 50.0), "{}", area(&s, id));
}

#[test]
fn a_box_that_is_no_size_fails_that_instance_and_names_the_parameter() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let shape = fitted(Dim::Mm(0.0), Dim::Mm(0.0), Dim::Expr("breedte - 100".into()), Dim::Mm(10.0));
    let (_, inst, _) = define_and_place(&mut s, shape);
    assert_eq!(
        s.snapshot().derived_record(DerivedOwner::Part(inst)).unwrap().status,
        EvalStatus::Failed { diagnosis: EvalDiagnosis::InvalidParameters { parameter: Some("breedte".into()) } }
    );
}

#[test]
fn the_definition_rejects_unknown_parameters_and_outlines_without_a_box() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let unknown = fitted(Dim::Param("nergens".into()), Dim::Mm(0.0), Dim::Mm(10.0), Dim::Mm(10.0));
    let cmd = DocumentCommand {
        document: s.id(),
        expected: s.revision(),
        request: RequestId::from_raw(1),
        payload: CommandPayload::Structure(StructureCommand::DefinePart { definition: definition(unknown) }),
    };
    assert!(s.apply(cmd).is_err(), "an unknown parameter is refused");
    // A straight line has no height: it cannot be fitted to a box.
    let flat = Path::polyline(&[Point::new(0.0, 0.0), Point::new(10.0, 0.0)], false).unwrap();
    let line =
        PartShape::Fitted { path: flat, x: Dim::Mm(0.0), y: Dim::Mm(0.0), width: Dim::Mm(5.0), height: Dim::Mm(5.0) };
    let cmd = DocumentCommand {
        document: s.id(),
        expected: s.revision(),
        request: RequestId::from_raw(2),
        payload: CommandPayload::Structure(StructureCommand::DefinePart { definition: definition(line) }),
    };
    assert!(s.apply(cmd).is_err(), "an outline without a box is refused");
}
