//! G6-werk S07: a part definition made from existing objects. Rectangles and circles become
//! parametric shapes (their size is a named parameter); anything else is kept as fixed geometry;
//! the definition is in its own frame, and placing an instance at the returned origin lays the
//! shapes exactly where the objects were.

#![allow(clippy::unwrap_used, clippy::panic)]

use burnburn_document::structure::{
    Dim, PartDefinition, PartFromObjectsError, PartInstance, PartShape, StructureChange, StructureCommand,
    part_from_objects,
};
use burnburn_document::{CommandOutcome, CommandPayload, DocumentCommand, DocumentSession, GeometryObject};
use burnburn_geometry::{Affine, Bounds, Path, Point, Rect, Shape, Vector};
use burnburn_types::{Angle, DocumentId, GeometryId, Length, RequestId, StableId};
use std::collections::BTreeMap;

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn run(s: &mut DocumentSession, payload: CommandPayload) -> CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap()
}

fn rectangle(s: &mut DocumentSession, x: f64, y: f64, w: f64, h: f64, name: &str) -> GeometryId {
    let rect = Rect::new(Point::new(x, y), mm(w), mm(h)).unwrap();
    run(s, CommandPayload::CreateRectangle { rect, name: name.into() }).changes.created[0]
}

fn circle(s: &mut DocumentSession, cx: f64, cy: f64, r: f64) -> GeometryId {
    let shape = Shape::Circle { center: Point::new(cx, cy), radius: mm(r) };
    run(s, CommandPayload::Edit(burnburn_document::GeometryCommand::CreateShape { shape, name: "Gat".into() }))
        .changes
        .created[0]
}

fn objects(s: &DocumentSession, ids: &[GeometryId]) -> Vec<GeometryObject> {
    let snap = s.snapshot();
    let _ = s;
    ids.iter().map(|id| snap.object(*id).unwrap().clone()).collect()
}

fn made(
    s: &DocumentSession,
    ids: &[GeometryId],
) -> Result<burnburn_document::structure::PartFromObjects, PartFromObjectsError> {
    let owned = objects(s, ids);
    let refs: Vec<&GeometryObject> = owned.iter().collect();
    part_from_objects("Beugel", &refs)
}

fn bounds_of(path: &Path) -> Bounds {
    path.bounds().unwrap()
}

#[test]
fn a_rectangle_and_a_circle_become_parametric_shapes_in_their_own_frame() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let plate = rectangle(&mut s, 100.0, 50.0, 30.0, 20.0, "Plaat");
    let hole = circle(&mut s, 125.0, 60.0, 5.0);
    let part = made(&s, &[plate, hole]).unwrap();
    // The union of the objects starts at (100, 50): that corner is the origin of the part.
    assert_eq!(part.origin, Point::new(100.0, 50.0));
    let def = &part.definition;
    assert_eq!(def.name, "Beugel");
    assert_eq!(def.parameters.len(), 3, "breedte, hoogte, straal: {:?}", def.parameters);
    let values: Vec<(String, f64)> = def.parameters.iter().map(|(k, v)| (k.clone(), v.mm())).collect();
    assert!(values.contains(&("breedte".into(), 30.0)) && values.contains(&("hoogte".into(), 20.0)), "{values:?}");
    assert!(values.iter().any(|(k, v)| k.starts_with("straal") && *v == 5.0), "{values:?}");
    let shapes: Vec<&PartShape> = def.shapes.values().map(|n| &n.shape).collect();
    match shapes[0] {
        PartShape::Rectangle { x, y, width, height } => {
            assert_eq!((x, y), (&Dim::Mm(0.0), &Dim::Mm(0.0)));
            assert_eq!((width, height), (&Dim::Param("breedte".into()), &Dim::Param("hoogte".into())));
        }
        other => panic!("{other:?}"),
    }
    match shapes[1] {
        PartShape::Circle { x, y, radius } => {
            assert_eq!((x, y), (&Dim::Mm(25.0), &Dim::Mm(10.0)), "centre relative to the origin");
            assert!(matches!(radius, Dim::Param(_)));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(def.shapes.values().map(|n| n.name.as_str()).collect::<Vec<_>>(), ["Plaat", "Gat"]);
    assert!(def.variants.is_empty());
}

#[test]
fn an_instance_at_the_origin_lays_the_shapes_exactly_where_the_objects_were() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let plate = rectangle(&mut s, 100.0, 50.0, 30.0, 20.0, "Plaat");
    let hole = circle(&mut s, 125.0, 60.0, 5.0);
    let part = made(&s, &[plate, hole]).unwrap();
    let before: Vec<Bounds> =
        [plate, hole].iter().map(|id| bounds_of(s.snapshot().object(*id).unwrap().path())).collect();

    let out = run(&mut s, CommandPayload::Structure(StructureCommand::DefinePart { definition: part.definition }));
    let StructureChange::Definition(def) = out.changes.structure[0] else { panic!() };
    let instance = PartInstance {
        definition: def,
        variant: None,
        overrides: BTreeMap::new(),
        transform: Affine::translate(Vector::new(part.origin.x, part.origin.y)).unwrap(),
        plate: None,
    };
    let out = run(&mut s, CommandPayload::Structure(StructureCommand::PlaceInstance { instance }));
    let derived: Vec<Bounds> =
        out.changes.created.iter().map(|id| bounds_of(s.snapshot().object(*id).unwrap().path())).collect();
    assert_eq!(derived.len(), 2);
    for (got, want) in derived.iter().zip(&before) {
        assert!(got.min().distance(want.min()) < 1e-6 && got.max().distance(want.max()) < 1e-6, "{got:?} vs {want:?}");
    }
}

#[test]
fn anything_that_is_not_a_plain_rectangle_or_circle_is_kept_as_fixed_geometry() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    // A hexagon is neither a rectangle nor a circle.
    let rounded = run(
        &mut s,
        CommandPayload::Edit(burnburn_document::GeometryCommand::CreateShape {
            shape: Shape::Polygon { center: Point::new(30.0, 25.0), radius: mm(15.0), sides: 6, rotation: Angle::ZERO },
            name: "Zeshoek".into(),
        }),
    )
    .changes
    .created[0];
    // A rectangle turned by 30 degrees is no longer axis-aligned.
    let turned = rectangle(&mut s, 80.0, 10.0, 20.0, 10.0, "Gedraaid");
    run(
        &mut s,
        CommandPayload::Transform {
            targets: vec![turned],
            transform: Affine::rotate(Angle::from_degrees(30.0).unwrap()),
        },
    );
    let part = made(&s, &[rounded, turned]).unwrap();
    assert!(part.definition.parameters.is_empty(), "{:?}", part.definition.parameters);
    for shape in part.definition.shapes.values() {
        assert!(matches!(shape.shape, PartShape::Fixed(_)), "{shape:?}");
    }
    // The fixed outlines are in the part's frame: the union starts at its origin.
    let union = part
        .definition
        .shapes
        .values()
        .filter_map(|n| match &n.shape {
            PartShape::Fixed(p) => p.bounds(),
            _ => None,
        })
        .reduce(|a, b| a.union(b))
        .unwrap();
    assert!(union.min().distance(Point::new(0.0, 0.0)) < 1e-6, "{union:?}");
}

#[test]
fn a_part_needs_objects_a_name_and_geometry() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let a = rectangle(&mut s, 0.0, 0.0, 10.0, 10.0, "A");
    assert_eq!(made(&s, &[]).unwrap_err(), PartFromObjectsError::NoObjects);
    let owned = objects(&s, &[a]);
    let refs: Vec<&GeometryObject> = owned.iter().collect();
    assert_eq!(part_from_objects("   ", &refs).unwrap_err(), PartFromObjectsError::EmptyName);
    let _: PartDefinition = part_from_objects("ok", &refs).unwrap().definition;
}
