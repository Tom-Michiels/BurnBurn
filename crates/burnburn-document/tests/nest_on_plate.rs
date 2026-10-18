//! A nesting result applied to a plate (IM26 ↔ IM21, cam.md): one command, one undo step; the
//! instances move and belong to the plate, their outputs follow; invalid input changes nothing.

#![allow(clippy::unwrap_used, clippy::panic)]

use std::collections::BTreeMap;

use burnburn_document::GeometrySource;
use burnburn_document::structure::{
    Dim, NamedShape, PartDefinition, PartInstance, PartInstanceId, PartShape, Plate, PlateId, StructureChange,
    StructureCommand, StructureError,
};
use burnburn_document::{CommandError, CommandOutcome, CommandPayload, DerivedOwner, DocumentCommand, DocumentSession};
use burnburn_geometry::{Affine, Point, Vector};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn run(s: &mut DocumentSession, c: StructureCommand) -> Result<CommandOutcome, CommandError> {
    let cmd = DocumentCommand {
        document: s.id(),
        expected: s.revision(),
        request: RequestId::from_raw(1),
        payload: CommandPayload::Structure(c),
    };
    s.apply(cmd)
}

fn instance_ids(o: &CommandOutcome) -> Vec<PartInstanceId> {
    o.changes
        .structure
        .iter()
        .filter_map(|c| if let StructureChange::Instance(i) = c { Some(*i) } else { None })
        .collect()
}

/// A 20 × 10 part, two instances off any plate, one plate.
fn setup() -> (DocumentSession, [PartInstanceId; 2], PlateId) {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let mut shapes = BTreeMap::new();
    shapes.insert(
        0,
        NamedShape {
            name: "Plankje".into(),
            shape: PartShape::Rectangle {
                x: Dim::Mm(0.0),
                y: Dim::Mm(0.0),
                width: Dim::Mm(20.0),
                height: Dim::Mm(10.0),
            },
        },
    );
    let def = PartDefinition { name: "Plankje".into(), shapes, ..PartDefinition::default() };
    let o = run(&mut s, StructureCommand::DefinePart { definition: def }).unwrap();
    let StructureChange::Definition(def) = o.changes.structure[0] else { panic!("definition") };
    let place = |s: &mut DocumentSession, x: f64| {
        let instance = PartInstance {
            definition: def,
            variant: None,
            overrides: BTreeMap::new(),
            transform: Affine::translate(Vector::new(x, 200.0)).unwrap(),
            plate: None,
        };
        instance_ids(&run(s, StructureCommand::PlaceInstance { instance }).unwrap())[0]
    };
    let a = place(&mut s, 0.0);
    let b = place(&mut s, 50.0);
    let plate = Plate {
        name: "Plaat 1".into(),
        width: mm(100.0),
        height: mm(50.0),
        material: "mdf".into(),
        thickness: Some(mm(3.0)),
    };
    let o = run(&mut s, StructureCommand::CreatePlate { plate }).unwrap();
    let StructureChange::Plate(p) = o.changes.structure[0] else { panic!("plate") };
    (s, [a, b], p)
}

fn output_of(s: &DocumentSession, i: PartInstanceId) -> GeometryId {
    s.snapshot()
        .objects()
        .find(|(_, o)| matches!(o.source(), GeometrySource::Derived(k) if k.owner == DerivedOwner::Part(i)))
        .unwrap()
        .0
}

#[test]
fn applying_a_nest_moves_instances_onto_the_plate_in_one_undo_step() {
    let (mut s, [a, b], plate) = setup();
    let ta = Affine::translate(Vector::new(1.0, 1.0)).unwrap();
    let tb = Affine::rotate(burnburn_types::Angle::from_degrees(90.0).unwrap())
        .then(&Affine::translate(Vector::new(40.0, 1.0)).unwrap());
    let before = s.snapshot();
    let (ga, gb) = (output_of(&s, a), output_of(&s, b));
    let o = run(&mut s, StructureCommand::ApplyNest { plate, placements: vec![(a, ta), (b, tb)] }).unwrap();
    assert!(o.changes.structure.contains(&StructureChange::Instance(a)));
    let snap = s.snapshot();
    assert_eq!(snap.structure().instance(a).unwrap().transform, ta);
    assert_eq!(snap.structure().instance(b).unwrap().plate, Some(plate));
    assert_eq!((output_of(&s, a), output_of(&s, b)), (ga, gb), "outputs keep their ids");
    let bounds = snap.object(ga).unwrap().bounds().unwrap();
    assert_eq!((bounds.min(), bounds.max()), (Point::new(1.0, 1.0), Point::new(21.0, 11.0)));
    assert_eq!(snap.plate_of(gb), Some(plate));
    let view = snap.plate_view(plate).unwrap();
    assert_eq!(view.snapshot.len(), 2, "both parts compile with the plate");

    let again = run(&mut s, StructureCommand::ApplyNest { plate, placements: vec![(a, ta), (b, tb)] });
    assert_eq!(again.unwrap_err(), CommandError::Structure(StructureError::NothingChanged), "no empty undo step");

    s.undo(s.revision()).unwrap();
    let back = s.snapshot();
    assert!(back.same_content(&before), "one undo restores transforms, plate and outputs");
}

#[test]
fn apply_nest_refuses_unknown_or_duplicate_instances_and_unknown_plates() {
    let (mut s, [a, _], plate) = setup();
    let t = Affine::IDENTITY;
    let rev = s.revision();
    let ghost = PartInstanceId::from_raw(999);
    let cases = [
        (StructureCommand::ApplyNest { plate, placements: vec![(ghost, t)] }, StructureError::UnknownInstance(ghost)),
        (
            StructureCommand::ApplyNest { plate, placements: vec![(a, t), (a, t)] },
            StructureError::InvalidValue { field: "placements" },
        ),
        (StructureCommand::ApplyNest { plate, placements: vec![] }, StructureError::NothingChanged),
        (
            StructureCommand::ApplyNest { plate: PlateId::from_raw(998), placements: vec![(a, t)] },
            StructureError::UnknownPlate(PlateId::from_raw(998)),
        ),
    ];
    for (c, want) in cases {
        assert_eq!(run(&mut s, c).unwrap_err(), CommandError::Structure(want));
    }
    assert_eq!(s.revision(), rev);
}
