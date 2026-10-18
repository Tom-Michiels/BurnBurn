//! Composite sketch commands (IM11, W1-L1b): drawing a shape with its suggested relations, removing
//! several constraints and turning drawn objects into a sketch are each one transaction and one
//! history entry; a bad step changes nothing.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test helpers outside #[test] fns

use burnburn_document::sketch::{
    ConstraintId, Entity, EntityId, Relation, Sketch, SketchCommand, SketchConstraint, SketchEntity, SketchError,
    SketchId, SketchLabel, SketchStep,
};
use burnburn_document::{CommandError, CommandPayload, DocumentCommand, DocumentSession, UndoLabel};
use burnburn_geometry::{Point, Rect};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId, StableId};

fn cmd(s: &DocumentSession, payload: CommandPayload) -> DocumentCommand {
    DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload }
}

fn session_with_empty_sketch() -> (DocumentSession, SketchId) {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let c = cmd(&s, CommandPayload::Sketch(SketchCommand::Create { sketch: Box::new(Sketch::new("Schets")) }));
    let out = s.apply(c).unwrap();
    (s, out.changes.sketches[0])
}

fn point(x: f64, y: f64) -> SketchStep {
    SketchStep::AddEntity(SketchEntity { entity: Entity::Point { at: Point::new(x, y) }, construction: false })
}

#[test]
fn batch_adds_entities_and_constraints_in_one_undo_entry() {
    let (mut s, sketch) = session_with_empty_sketch();
    let line = SketchStep::AddEntity(SketchEntity {
        entity: Entity::Line { start: EntityId::from_raw(0), end: EntityId::from_raw(1) },
        construction: false,
    });
    let horizontal = SketchStep::AddConstraint(SketchConstraint::Relation(Relation::Horizontal(EntityId::from_raw(2))));
    let steps = vec![point(0.0, 0.0), point(10.0, 1.0), line, horizontal];
    let c = cmd(&s, CommandPayload::Sketch(SketchCommand::Batch { sketch, steps }));
    let out = s.apply(c).unwrap();

    assert_eq!(out.undo_label, UndoLabel::Sketch(SketchLabel::Batch));
    let snapshot = s.snapshot();
    let sk = snapshot.sketch(sketch).unwrap();
    assert_eq!(sk.entities().len(), 3, "two points and the line");
    assert_eq!(sk.constraints().len(), 1, "the relation");
    assert!(out.changes.sketches.contains(&sketch));

    s.undo(s.revision()).unwrap();
    let snapshot = s.snapshot();
    let sk = snapshot.sketch(sketch).unwrap();
    assert_eq!((sk.entities().len(), sk.constraints().len()), (0, 0), "one undo undoes the whole batch");
}

#[test]
fn batch_with_a_bad_reference_changes_nothing() {
    let (mut s, sketch) = session_with_empty_sketch();
    let revision = s.revision();
    let bad = SketchStep::AddConstraint(SketchConstraint::Relation(Relation::Horizontal(EntityId::from_raw(99))));
    let c = cmd(&s, CommandPayload::Sketch(SketchCommand::Batch { sketch, steps: vec![point(0.0, 0.0), bad] }));
    assert_eq!(
        s.apply(c),
        Err(CommandError::Sketch(SketchError::UnknownEntity(EntityId::from_raw(99)))),
        "refused as a whole"
    );
    assert_eq!(s.revision(), revision);
    assert_eq!(s.snapshot().sketch(sketch).unwrap().entities().len(), 0, "the point before the bad step is gone too");
}

#[test]
fn batch_removes_several_constraints_in_one_entry() {
    let (mut s, sketch) = session_with_empty_sketch();
    let line = SketchStep::AddEntity(SketchEntity {
        entity: Entity::Line { start: EntityId::from_raw(0), end: EntityId::from_raw(1) },
        construction: false,
    });
    let h = SketchStep::AddConstraint(SketchConstraint::Relation(Relation::Horizontal(EntityId::from_raw(2))));
    let v = SketchStep::AddConstraint(SketchConstraint::Relation(Relation::Vertical(EntityId::from_raw(2))));
    let steps = vec![point(0.0, 0.0), point(10.0, 0.0), line, h, v];
    let c = cmd(&s, CommandPayload::Sketch(SketchCommand::Batch { sketch, steps }));
    s.apply(c).unwrap();
    let ids: Vec<ConstraintId> = s.snapshot().sketch(sketch).unwrap().constraints().map(|(id, _)| id).collect();
    assert_eq!(ids.len(), 2);

    let steps = ids.iter().map(|id| SketchStep::RemoveConstraint(*id)).collect();
    let c = cmd(&s, CommandPayload::Sketch(SketchCommand::Batch { sketch, steps }));
    s.apply(c).unwrap();
    assert_eq!(s.snapshot().sketch(sketch).unwrap().constraints().len(), 0);
    s.undo(s.revision()).unwrap();
    assert_eq!(s.snapshot().sketch(sketch).unwrap().constraints().len(), 2, "one undo brings both back");
}

#[test]
fn create_from_replaces_the_drawn_objects_in_one_entry() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let rect = Rect::new(Point::new(0.0, 0.0), Length::from_mm(20.0).unwrap(), Length::from_mm(10.0).unwrap()).unwrap();
    let c = cmd(&s, CommandPayload::CreateRectangle { rect, name: "Rechthoek 1".into() });
    let drawn: GeometryId = s.apply(c).unwrap().changes.created[0];
    let c = cmd(&s, CommandPayload::CreateRectangle { rect, name: "Rechthoek 2".into() });
    let kept: GeometryId = s.apply(c).unwrap().changes.created[0];
    let revision = s.revision();

    let c = cmd(
        &s,
        CommandPayload::Sketch(SketchCommand::CreateFrom {
            sketch: Box::new(Sketch::new("Van rechthoek")),
            replace: vec![drawn],
        }),
    );
    let out = s.apply(c).unwrap();
    assert_eq!(out.undo_label, UndoLabel::Sketch(SketchLabel::CreateFrom));
    let snapshot = s.snapshot();
    assert!(snapshot.object(drawn).is_none(), "the drawn object is replaced");
    assert!(snapshot.object(kept).is_some(), "other objects stay");
    assert_eq!(snapshot.sketch_layer().sketches().len(), 1);
    assert_eq!(out.changes.deleted, vec![drawn]);

    s.undo(s.revision()).unwrap();
    let snapshot = s.snapshot();
    assert!(snapshot.object(drawn).is_some(), "undo brings the drawn object back");
    assert_eq!(snapshot.sketch_layer().sketches().len(), 0, "and removes the sketch");
    assert!(s.revision() > revision);
}

#[test]
fn create_from_an_unknown_object_changes_nothing() {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let revision = s.revision();
    let c = cmd(
        &s,
        CommandPayload::Sketch(SketchCommand::CreateFrom {
            sketch: Box::new(Sketch::new("Niets")),
            replace: vec![GeometryId::from_raw(42)],
        }),
    );
    assert_eq!(s.apply(c), Err(CommandError::UnknownGeometry(GeometryId::from_raw(42))));
    assert_eq!(s.revision(), revision);
    assert_eq!(s.snapshot().sketch_layer().sketches().len(), 0);
}
