//! G7-editor E06: applying (baking) a modifier chain. The outputs become ordinary objects with the
//! same ids and outlines, nothing re-evaluates them any more, the source stays editable, and one
//! undo brings the chain back with its derived outputs.

#![allow(clippy::unwrap_used, clippy::panic)]

use burnburn_document::structure::{
    EvalMode, Modifier, ModifierChain, ModifierId, ModifierStep, StructureCommand, StructureError,
};
use burnburn_document::{
    CommandError, CommandOutcome, CommandPayload, DocumentCommand, DocumentSession, GeometrySource,
};
use burnburn_geometry::{Affine, Point, ProductionTolerance, Rect, Vector};
use burnburn_types::{DocumentId, GeometryId, Length, RequestId, StableId};

fn mm(v: f64) -> Length {
    Length::from_mm(v).unwrap()
}

fn run(s: &mut DocumentSession, payload: CommandPayload) -> Result<CommandOutcome, CommandError> {
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply(cmd)
}

fn square(s: &mut DocumentSession) -> GeometryId {
    let rect = Rect::new(Point::new(0.0, 0.0), mm(10.0), mm(10.0)).unwrap();
    run(s, CommandPayload::CreateRectangle { rect, name: "Bron".into() }).unwrap().changes.created[0]
}

fn array_chain(source: GeometryId) -> ModifierChain {
    ModifierChain {
        name: "Reeks".into(),
        source,
        steps: vec![ModifierStep {
            modifier: Modifier::LinearArray { count: 3, step: Vector::new(20.0, 0.0) },
            enabled: true,
        }],
        tolerance: ProductionTolerance::new(mm(0.01)).unwrap(),
        mode: EvalMode::Inline,
    }
}

fn setup() -> (DocumentSession, GeometryId, ModifierId, GeometryId) {
    let mut s = DocumentSession::new(DocumentId::from_raw(1));
    let src = square(&mut s);
    let out =
        run(&mut s, CommandPayload::Structure(StructureCommand::CreateModifier { chain: array_chain(src) })).unwrap();
    let output = out.changes.created[0];
    let modifier = s.snapshot().structure().modifiers().next().map(|(id, _)| id).unwrap();
    (s, src, modifier, output)
}

#[test]
fn baking_keeps_the_outputs_as_drawn_objects_and_removes_the_chain() {
    let (mut s, src, modifier, output) = setup();
    let before = s.snapshot().object(output).unwrap().path().clone();
    assert!(matches!(s.snapshot().object(output).unwrap().source(), GeometrySource::Derived(_)));

    run(&mut s, CommandPayload::Structure(StructureCommand::BakeModifier { modifier })).unwrap();

    let snap = s.snapshot();
    assert!(snap.structure().modifier(modifier).is_none(), "the chain is gone");
    let baked = snap.object(output).expect("the output stays, with its id");
    assert_eq!(baked.source(), &GeometrySource::Drawn, "and is an ordinary object now");
    assert_eq!(baked.path(), &before, "with the very same outline");
    assert!(snap.object(src).is_some());
}

#[test]
fn a_baked_object_no_longer_follows_its_source_and_can_be_edited_directly() {
    let (mut s, src, modifier, output) = setup();
    run(&mut s, CommandPayload::Structure(StructureCommand::BakeModifier { modifier })).unwrap();
    let frozen = s.snapshot().object(output).unwrap().path().clone();

    let shift = Affine::translate(Vector::new(0.0, 50.0)).unwrap();
    run(&mut s, CommandPayload::Transform { targets: vec![src], transform: shift }).unwrap();
    assert_eq!(s.snapshot().object(output).unwrap().path(), &frozen, "the source moved, the baked copy did not");

    // Derived outputs refuse direct edits (C-8); a baked one is just an object.
    run(&mut s, CommandPayload::Transform { targets: vec![output], transform: shift }).unwrap();
}

#[test]
fn baking_is_one_undo_step_that_brings_the_chain_and_its_derived_outputs_back() {
    let (mut s, src, modifier, output) = setup();
    let derived = s.snapshot().object(output).unwrap().clone();
    run(&mut s, CommandPayload::Structure(StructureCommand::BakeModifier { modifier })).unwrap();
    s.undo(s.revision()).unwrap();

    let snap = s.snapshot();
    assert!(snap.structure().modifier(modifier).is_some());
    assert!(matches!(snap.object(output).unwrap().source(), GeometrySource::Derived(_)));
    assert_eq!(snap.object(output).unwrap().path(), derived.path());

    // And it follows its source again.
    let shift = Affine::translate(Vector::new(0.0, 50.0)).unwrap();
    run(&mut s, CommandPayload::Transform { targets: vec![src], transform: shift }).unwrap();
    assert_eq!(s.snapshot().object(output).unwrap().bounds().unwrap().min().y, 50.0);
}

#[test]
fn baking_an_unknown_chain_is_refused_and_changes_nothing() {
    let (mut s, _, _, _) = setup();
    let revision = s.revision();
    let err =
        run(&mut s, CommandPayload::Structure(StructureCommand::BakeModifier { modifier: ModifierId::from_raw(99) }))
            .unwrap_err();
    assert!(matches!(err, CommandError::Structure(StructureError::UnknownModifier(_))), "{err:?}");
    assert_eq!(s.revision(), revision);
}
