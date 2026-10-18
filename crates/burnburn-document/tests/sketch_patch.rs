//! Sketch data in the document (contract §12) and the synchronous evaluator hook (§11):
//! definition and derived outputs in one transaction, stable output ids, last valid result.
//! Owner: Sketch role. The evaluator here is a fake; the real one is `burnburn-sketch`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test helpers outside #[test] fns

use burnburn_document::sketch::{Sketch, SketchCommand, SketchError, SketchId, SketchSolution};
use burnburn_document::{
    CommandError, CommandPayload, DerivedKey, DerivedOutput, DerivedOwner, DerivedOwnerKind, DocumentCommand,
    DocumentPatch, DocumentSession, DocumentSnapshot, EvalDiagnosis, EvalStatus, Evaluator, GeometrySource, ObjectKind,
    OwnerSolution,
};
use burnburn_geometry::{Path, Point, Rect};
use burnburn_types::{ContentHash, DocumentId, GeometryId, Length, RequestId, StableId};

/// Output 0: a `w` × 10 rectangle. Output 1 exists while parameter `two` is defined.
struct FakeEvaluator;

impl Evaluator for FakeEvaluator {
    fn owner_kind(&self) -> DerivedOwnerKind {
        DerivedOwnerKind::Sketch
    }

    fn evaluate(&self, snapshot: &DocumentSnapshot, owner: DerivedOwner) -> DocumentPatch {
        let DerivedOwner::Sketch(id) = owner else { panic!("only sketch owners") };
        let sketch = snapshot.sketch(id).unwrap();
        let inputs = ContentHash::from_bytes([sketch.parameters().iter().len() as u8; 32]);
        let w = sketch.parameters().get("w").and_then(|t| t.parse::<f64>().ok());
        let Some(w) = w else {
            return DocumentPatch {
                owner,
                inputs,
                outputs: vec![],
                status: EvalStatus::Failed {
                    diagnosis: EvalDiagnosis::InvalidParameters { parameter: Some("w".into()) },
                },
                solution: OwnerSolution::Keep,
            };
        };
        let rect = |x: f64, w: f64| {
            Path::rectangle(
                Rect::new(Point::new(x, 0.0), Length::from_mm(w).unwrap(), Length::from_mm(10.0).unwrap()).unwrap(),
            )
        };
        let mut outputs = vec![DerivedOutput {
            key: DerivedKey { owner, local: 0 },
            kind: ObjectKind::Path,
            path: rect(0.0, w),
            name: Some("outline".into()),
        }];
        if sketch.parameters().get("two").is_some() {
            outputs.push(DerivedOutput {
                key: DerivedKey { owner, local: 1 },
                kind: ObjectKind::Path,
                path: rect(500.0, 5.0),
                name: Some("extra".into()),
            });
        }
        DocumentPatch {
            owner,
            inputs,
            outputs,
            status: EvalStatus::Valid,
            solution: OwnerSolution::Sketch(SketchSolution::default()),
        }
    }
}

fn session() -> DocumentSession {
    DocumentSession::new(DocumentId::from_raw(1))
}

fn cmd(s: &DocumentSession, payload: CommandPayload) -> DocumentCommand {
    DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload }
}

fn sk(s: &DocumentSession, c: SketchCommand) -> DocumentCommand {
    cmd(s, CommandPayload::Sketch(c))
}

fn set_w(s: &mut DocumentSession, id: SketchId, text: &str) {
    let c = sk(s, SketchCommand::SetParameter { sketch: id, name: "w".into(), text: text.into() });
    s.apply_with(c, &[&FakeEvaluator]).unwrap();
}

fn create(s: &mut DocumentSession) -> SketchId {
    let mut def = Sketch::new("Plaat");
    def.set_parameter("w", "120").unwrap();
    let out = s.apply_with(sk(s, SketchCommand::Create { sketch: Box::new(def) }), &[&FakeEvaluator]).unwrap();
    out.changes.sketches[0]
}

fn outputs(snap: &DocumentSnapshot, id: SketchId) -> Vec<(GeometryId, u64, f64)> {
    snap.objects()
        .filter_map(|(gid, o)| match o.source() {
            GeometrySource::Derived(k) if k.owner == DerivedOwner::Sketch(id) => {
                Some((gid, k.local, o.bounds().unwrap().width()))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn creating_a_sketch_evaluates_it_in_the_same_revision() {
    let mut s = session();
    let before = s.revision();
    let id = create(&mut s);
    let snap = s.snapshot();
    assert_eq!(snap.revision(), before.next().unwrap(), "one transaction");
    assert_eq!(snap.sketch(id).unwrap().name(), "Plaat");
    let outs = outputs(&snap, id);
    assert_eq!(outs.len(), 1);
    assert_eq!(outs[0].2, 120.0);
    assert_eq!(snap.derived_record(DerivedOwner::Sketch(id)).unwrap().status, EvalStatus::Valid);
    assert!(!snap.production_blocked(outs[0].0));
    assert!(snap.sketch(id).unwrap().solution().is_some(), "solution stored");
}

#[test]
fn re_evaluation_keeps_geometry_ids_per_key_and_removes_vanished_keys() {
    let mut s = session();
    let id = create(&mut s);
    let first = outputs(&s.snapshot(), id)[0].0;
    set_w(&mut s, id, "150");
    let outs = outputs(&s.snapshot(), id);
    assert_eq!(outs, vec![(first, 0, 150.0)], "same GeometryId, new geometry");

    let c = sk(&s, SketchCommand::SetParameter { sketch: id, name: "two".into(), text: "1".into() });
    let out = s.apply_with(c, &[&FakeEvaluator]).unwrap();
    assert_eq!(out.changes.created.len(), 1);
    let extra = out.changes.created[0];
    let c = sk(&s, SketchCommand::RemoveParameter { sketch: id, name: "two".into() });
    let out = s.apply_with(c, &[&FakeEvaluator]).unwrap();
    assert_eq!(out.changes.deleted, vec![extra]);
    assert_eq!(outputs(&s.snapshot(), id).len(), 1);
}

#[test]
fn failed_evaluation_keeps_last_valid_outputs_and_blocks_production() {
    let mut s = session();
    let id = create(&mut s);
    let gid = outputs(&s.snapshot(), id)[0].0;
    set_w(&mut s, id, "not a number");
    let snap = s.snapshot();
    assert_eq!(snap.sketch(id).unwrap().parameters().get("w"), Some("not a number"), "definition kept as typed");
    assert_eq!(outputs(&snap, id), vec![(gid, 0, 120.0)], "last valid geometry stays");
    assert!(matches!(snap.derived_record(DerivedOwner::Sketch(id)).unwrap().status, EvalStatus::Failed { .. }));
    assert!(snap.production_blocked(gid));
    set_w(&mut s, id, "90");
    assert!(!s.snapshot().production_blocked(gid));
    assert_eq!(outputs(&s.snapshot(), id)[0].2, 90.0);
}

#[test]
fn editing_without_an_evaluator_marks_outputs_pending() {
    let mut s = session();
    let id = create(&mut s);
    let gid = outputs(&s.snapshot(), id)[0].0;
    let c = sk(&s, SketchCommand::SetParameter { sketch: id, name: "w".into(), text: "130".into() });
    s.apply(c).unwrap();
    let snap = s.snapshot();
    assert_eq!(snap.derived_record(DerivedOwner::Sketch(id)).unwrap().status, EvalStatus::Pending);
    assert!(snap.production_blocked(gid));
}

#[test]
fn undo_restores_definition_and_outputs_together() {
    let mut s = session();
    let id = create(&mut s);
    set_w(&mut s, id, "150");
    s.undo(s.revision()).unwrap();
    let snap = s.snapshot();
    assert_eq!(snap.sketch(id).unwrap().parameters().get("w"), Some("120"));
    assert_eq!(outputs(&snap, id)[0].2, 120.0);
    s.undo(s.revision()).unwrap();
    assert!(s.snapshot().sketch(id).is_none());
    assert!(outputs(&s.snapshot(), id).is_empty());
}

#[test]
fn invalid_sketch_edit_is_rejected_and_changes_nothing() {
    let mut s = session();
    let id = create(&mut s);
    let rev = s.revision();
    let bad = burnburn_document::sketch::SketchConstraint::Relation(burnburn_document::sketch::Relation::Horizontal(
        burnburn_document::sketch::EntityId::from_raw(99),
    ));
    let err = s.apply_with(sk(&s, SketchCommand::AddConstraint { sketch: id, constraint: bad }), &[&FakeEvaluator]);
    assert_eq!(
        err,
        Err(CommandError::Sketch(SketchError::UnknownEntity(burnburn_document::sketch::EntityId::from_raw(99))))
    );
    assert_eq!(s.revision(), rev);
    let missing = SketchId::from_raw(4242);
    let err = s.apply(sk(&s, SketchCommand::Delete { sketch: missing }));
    assert_eq!(err, Err(CommandError::Sketch(SketchError::UnknownSketch(missing))));
}

#[test]
fn deleting_a_sketch_removes_its_outputs() {
    let mut s = session();
    let id = create(&mut s);
    let gid = outputs(&s.snapshot(), id)[0].0;
    let out = s.apply_with(sk(&s, SketchCommand::Delete { sketch: id }), &[&FakeEvaluator]).unwrap();
    assert_eq!(out.changes.deleted, vec![gid]);
    assert!(s.snapshot().object(gid).is_none());
    assert!(s.snapshot().derived_record(DerivedOwner::Sketch(id)).is_none());
}
