//! IM21/S05: tangent and symmetric relations between objects over associative references.
//! The document checks and reports them; it never moves geometry to satisfy them.

#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns; let-else on expected variants

use burnburn_document::structure::{
    AssocReference, GeometryRef, ObjectRelation, ReferenceId, RelationId, RelationState, StructureChange,
    StructureCommand, SubRef, VertexEnd,
};
use burnburn_document::{CommandOutcome, CommandPayload, DocumentCommand, DocumentSession, GeometryCommand};
use burnburn_geometry::{Affine, Point, Rect, Shape, Vector};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn run(s: &mut DocumentSession, payload: CommandPayload) -> CommandOutcome {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd).unwrap()
}

fn shape(s: &mut DocumentSession, shape: Shape) -> GeometryId {
    run(s, CommandPayload::Edit(GeometryCommand::CreateShape { shape, name: String::new() })).changes.created[0]
}

fn edge_at(s: &DocumentSession, id: GeometryId, x: f64, y: f64) -> GeometryRef {
    let snap = s.snapshot();
    let at = snap.object(id).unwrap().path().nearest(Point::new(x, y)).unwrap().at;
    snap.edge_ref(id, at).unwrap()
}

fn reference(s: &mut DocumentSession, target: GeometryRef) -> ReferenceId {
    let reference = AssocReference { name: String::new(), target };
    let out = run(s, CommandPayload::Structure(StructureCommand::AddReference { reference }));
    let StructureChange::Reference(r) = out.changes.structure[0] else { panic!() };
    r
}

fn relation(s: &mut DocumentSession, relation: ObjectRelation) -> RelationId {
    let out = run(s, CommandPayload::Structure(StructureCommand::AddRelation { relation }));
    let StructureChange::Relation(r) = out.changes.structure[0] else { panic!() };
    r
}

fn residual(state: Option<RelationState>) -> (bool, f64) {
    match state {
        Some(RelationState::Satisfied { residual_mm }) => (true, residual_mm),
        Some(RelationState::Violated { residual_mm }) => (false, residual_mm),
        other => panic!("{other:?}"),
    }
}

#[test]
fn tangent_relation_reports_residual_and_breaks_with_its_reference() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let rect = Rect::new(Point::new(0.0, 0.0), mm(100.0), mm(50.0)).unwrap();
    let plate = run(&mut s, CommandPayload::CreateRectangle { rect, name: "Plaat".into() }).changes.created[0];
    let wheel = shape(&mut s, Shape::Circle { center: Point::new(50.0, 60.0), radius: mm(10.0) });
    let target = edge_at(&s, plate, 50.0, 50.0);
    let top = reference(&mut s, target);
    let target = edge_at(&s, wheel, 50.0, 50.0);
    let arc = reference(&mut s, target);
    let t = relation(&mut s, ObjectRelation::Tangent { a: top, b: arc });
    let (ok, r) = residual(s.snapshot().relation_state(t));
    assert!(ok && r < 1e-9, "circle rests on the top edge: {r}");

    let up = Affine::translate(Vector::new(0.0, 5.0)).unwrap();
    run(&mut s, CommandPayload::Transform { targets: vec![wheel], transform: up });
    let (ok, r) = residual(s.snapshot().relation_state(t));
    assert!(!ok && (r - 5.0).abs() < 1e-9, "reported, not corrected: {r}");
    assert_eq!(s.snapshot().object(wheel).unwrap().bounds().unwrap().min().y, 55.0, "geometry not moved");

    run(&mut s, CommandPayload::Delete { targets: vec![wheel] });
    assert_eq!(s.snapshot().relation_state(t), Some(RelationState::Broken { reference: arc }));
}

#[test]
fn symmetric_relation_checks_mirrored_vertices_about_a_line() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let a = shape(&mut s, Shape::Rectangle(Rect::new(Point::new(0.0, 0.0), mm(10.0), mm(10.0)).unwrap()));
    let b = shape(&mut s, Shape::Rectangle(Rect::new(Point::new(30.0, 0.0), mm(10.0), mm(10.0)).unwrap()));
    let axis = shape(&mut s, Shape::Line { from: Point::new(20.0, -5.0), to: Point::new(20.0, 15.0) });
    let vertex = |s: &DocumentSession, id, x, y, end| {
        let GeometryRef { sub: Some(SubRef::Edge(edge)), .. } = edge_at(s, id, x, y) else { panic!() };
        GeometryRef { object: id, sub: Some(SubRef::Vertex { edge, end }) }
    };
    // End of A's bottom edge (10, 0) and start of B's bottom edge (30, 0).
    let target = vertex(&s, a, 5.0, 0.0, VertexEnd::End);
    let va = reference(&mut s, target);
    let target = vertex(&s, b, 35.0, 0.0, VertexEnd::Start);
    let vb = reference(&mut s, target);
    let target = edge_at(&s, axis, 20.0, 0.0);
    let ax = reference(&mut s, target);
    let rel = relation(&mut s, ObjectRelation::Symmetric { a: va, b: vb, axis: ax });
    let (ok, r) = residual(s.snapshot().relation_state(rel));
    assert!(ok && r < 1e-9, "{r}");
    let right = Affine::translate(Vector::new(3.0, 0.0)).unwrap();
    run(&mut s, CommandPayload::Transform { targets: vec![b], transform: right });
    let (ok, r) = residual(s.snapshot().relation_state(rel));
    assert!(!ok && (r - 3.0).abs() < 1e-9, "{r}");
    // A tangent between two straight edges is not defined.
    let t = relation(&mut s, ObjectRelation::Tangent { a: ax, b: ax });
    assert_eq!(s.snapshot().relation_state(t), Some(RelationState::Unsupported));
}
