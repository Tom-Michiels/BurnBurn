//! IM21/S07/P06: part definitions with instances and variants, and plates.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns; let-else on expected variants

use std::collections::BTreeMap;

use burnburn_document::process::{
    Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessCommand, ProcessSettings, Speed,
};
use burnburn_document::structure::{
    Dim, NamedShape, PartDefinition, PartDefinitionId, PartInstance, PartInstanceId, PartShape, Plate, PlateId,
    PlateIssue, StructureChange, StructureCommand, effective_parameters,
};
use burnburn_document::{
    CommandOutcome, CommandPayload, DerivedOwner, DocumentCommand, DocumentSession, EvalDiagnosis, EvalStatus,
};
use burnburn_geometry::{Affine, Point, Rect, Vector};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn run(s: &mut DocumentSession, payload: CommandPayload) -> CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap()
}

fn st(cmd: StructureCommand) -> CommandPayload {
    CommandPayload::Structure(cmd)
}

/// A bracket: rectangle `breedte × 20` with a hole r 3 at (10, 10); variants "klein" (25) and
/// "kapot" (−5, invalid on purpose).
fn bracket(width: f64) -> PartDefinition {
    let mut shapes = BTreeMap::new();
    shapes.insert(
        1,
        NamedShape {
            name: "omtrek".into(),
            shape: PartShape::Rectangle {
                x: Dim::Mm(0.0),
                y: Dim::Mm(0.0),
                width: Dim::Param("breedte".into()),
                height: Dim::Mm(20.0),
            },
        },
    );
    shapes.insert(
        2,
        NamedShape {
            name: "gat".into(),
            shape: PartShape::Circle { x: Dim::Mm(10.0), y: Dim::Mm(10.0), radius: Dim::Mm(3.0) },
        },
    );
    let mut variants = BTreeMap::new();
    variants.insert("klein".to_owned(), BTreeMap::from([("breedte".to_owned(), mm(25.0))]));
    variants.insert("kapot".to_owned(), BTreeMap::from([("breedte".to_owned(), mm(-5.0))]));
    PartDefinition {
        name: "Beugel".into(),
        parameters: BTreeMap::from([("breedte".to_owned(), mm(width))]),
        shapes,
        variants,
    }
}

fn define(s: &mut DocumentSession, def: PartDefinition) -> PartDefinitionId {
    let out = run(s, st(StructureCommand::DefinePart { definition: def }));
    let StructureChange::Definition(d) = out.changes.structure[0] else { panic!() };
    d
}

fn instance(definition: PartDefinitionId, x: f64, variant: Option<&str>, plate: Option<PlateId>) -> PartInstance {
    PartInstance {
        definition,
        variant: variant.map(str::to_owned),
        overrides: BTreeMap::new(),
        transform: Affine::translate(Vector::new(x, 0.0)).unwrap(),
        plate,
    }
}

fn place(s: &mut DocumentSession, inst: PartInstance) -> (PartInstanceId, Vec<GeometryId>) {
    let out = run(s, st(StructureCommand::PlaceInstance { instance: inst }));
    let StructureChange::Instance(i) = out.changes.structure[0] else { panic!() };
    (i, out.changes.created)
}

fn width_of(s: &DocumentSession, id: GeometryId) -> f64 {
    s.snapshot().object(id).unwrap().bounds().unwrap().width()
}

#[test]
fn editing_definition_updates_every_instance_with_stable_ids() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let d = define(&mut s, bracket(40.0));
    let (_, a) = place(&mut s, instance(d, 0.0, None, None));
    let (_, b) = place(&mut s, instance(d, 100.0, None, None));
    assert_eq!((a.len(), b.len()), (2, 2), "one output per shape");
    assert_eq!((width_of(&s, a[0]), width_of(&s, b[0])), (40.0, 40.0));
    assert_eq!(s.snapshot().object(b[0]).unwrap().bounds().unwrap().min().x, 100.0, "own transform");

    let out = run(&mut s, st(StructureCommand::SetPartDefinition { definition: d, value: bracket(60.0) }));
    assert_eq!((width_of(&s, a[0]), width_of(&s, b[0])), (60.0, 60.0), "every instance follows");
    let mut modified = out.changes.modified.clone();
    modified.sort();
    assert_eq!(modified, vec![a[0], b[0]], "same GeometryIds; the unchanged holes are untouched");
    s.undo(s.revision()).unwrap();
    assert_eq!((width_of(&s, a[0]), width_of(&s, b[0])), (40.0, 40.0));
}

#[test]
fn variants_fail_individually_and_overrides_stay_local() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let d = define(&mut s, bracket(40.0));
    let (small, small_out) = place(&mut s, instance(d, 0.0, Some("klein"), None));
    let (broken, broken_out) = place(&mut s, instance(d, 100.0, Some("kapot"), None));
    let (plain, plain_out) = place(&mut s, instance(d, 200.0, None, None));
    assert_eq!(width_of(&s, small_out[0]), 25.0);
    assert!(broken_out.is_empty(), "an invalid variant produces nothing the first time");
    let snap = s.snapshot();
    assert_eq!(
        snap.derived_record(DerivedOwner::Part(broken)).unwrap().status,
        EvalStatus::Failed { diagnosis: EvalDiagnosis::InvalidParameters { parameter: Some("breedte".into()) } }
    );
    assert_eq!(snap.derived_record(DerivedOwner::Part(small)).unwrap().status, EvalStatus::Valid);

    // A local override on one instance: visible on the instance, the definition is untouched.
    let mut over = snap.structure().instance(plain).unwrap().clone();
    over.overrides.insert("breedte".into(), mm(80.0));
    run(&mut s, st(StructureCommand::SetInstance { instance: plain, value: over }));
    let snap = s.snapshot();
    assert_eq!(width_of(&s, plain_out[0]), 80.0);
    assert_eq!(snap.structure().definition(d).unwrap().parameters["breedte"], mm(40.0));
    let inst = snap.structure().instance(plain).unwrap();
    assert_eq!(inst.overrides["breedte"], mm(80.0));
    assert_eq!(effective_parameters(snap.structure().definition(d).unwrap(), inst)["breedte"], mm(80.0));

    // Switching a valid instance to the broken variant keeps its last valid outputs, blocked.
    let mut to_broken = snap.structure().instance(small).unwrap().clone();
    to_broken.variant = Some("kapot".into());
    run(&mut s, st(StructureCommand::SetInstance { instance: small, value: to_broken }));
    assert_eq!(width_of(&s, small_out[0]), 25.0);
    assert!(s.snapshot().production_blocked(small_out[0]));
    assert!(!s.snapshot().production_blocked(plain_out[0]), "the others are unaffected");
}

fn cut(targets: Vec<GeometryId>) -> CommandPayload {
    let settings = ProcessSettings {
        speed: Speed::mm_per_s(10.0).unwrap(),
        power: PowerRange::new(PowerPercent::new(50.0).unwrap(), PowerPercent::new(60.0).unwrap()).unwrap(),
        passes: Passes::new(1).unwrap(),
        air_assist: true,
    };
    let draft = OperationDraft {
        name: "Snijden".into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings,
        output_enabled: true,
    };
    CommandPayload::Process(ProcessCommand::AddOperation { draft, targets })
}

#[test]
fn plate_view_contains_only_its_objects_and_reports_what_it_left_out() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let plate = |name: &str, material: &str| Plate {
        name: name.into(),
        width: mm(600.0),
        height: mm(400.0),
        material: material.into(),
        thickness: Some(mm(3.0)),
    };
    let new_plate = |s: &mut DocumentSession, p| {
        let out = run(s, st(StructureCommand::CreatePlate { plate: p }));
        let StructureChange::Plate(id) = out.changes.structure[0] else { panic!() };
        id
    };
    let p1 = new_plate(&mut s, plate("Plaat 1", "populier"));
    let p2 = new_plate(&mut s, plate("Plaat 2", "acryl"));
    let rect = |x| Rect::new(Point::new(x, 0.0), mm(10.0), mm(10.0)).unwrap();
    let a = run(&mut s, CommandPayload::CreateRectangle { rect: rect(0.0), name: "A".into() }).changes.created[0];
    let b = run(&mut s, CommandPayload::CreateRectangle { rect: rect(50.0), name: "B".into() }).changes.created[0];
    run(&mut s, st(StructureCommand::AssignToPlate { objects: vec![a], plate: Some(p1) }));
    run(&mut s, st(StructureCommand::AssignToPlate { objects: vec![b], plate: Some(p2) }));
    // The same part on both plates, without a copy of the definition.
    let d = define(&mut s, bracket(40.0));
    let (_, on1) = place(&mut s, instance(d, 100.0, None, Some(p1)));
    let (_, on2) = place(&mut s, instance(d, 200.0, None, Some(p2)));
    let op = run(&mut s, cut(vec![a, b])).changes.operations[0];

    let snap = s.snapshot();
    assert_eq!((snap.plate_of(a), snap.plate_of(on2[0])), (Some(p1), Some(p2)));
    let view = snap.plate_view(p1).unwrap();
    let mut ids: Vec<GeometryId> = view.snapshot.objects().map(|(id, _)| id).collect();
    ids.sort();
    let mut expected = vec![a, on1[0], on1[1]];
    expected.sort();
    assert_eq!(ids, expected);
    let ops: Vec<_> = view.snapshot.process().operations().collect();
    assert_eq!(ops[0].targets(), &[a]);
    assert_eq!(view.issues, vec![PlateIssue::TargetOffPlate { operation: op, geometry: b }]);

    // Moving A to plate 2 takes it off plate 1: an object is on at most one plate.
    run(&mut s, st(StructureCommand::AssignToPlate { objects: vec![a], plate: Some(p2) }));
    let snap = s.snapshot();
    assert_eq!(snap.plate_of(a), Some(p2));
    assert!(snap.plate_view(p1).unwrap().snapshot.object(a).is_none());
    assert!(snap.plate_view(PlateId::from_raw(999)).is_err());
}
