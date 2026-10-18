//! Traces and generators as derived document objects (C-2, C-8, C-9, contract §11): the
//! definition (recipe + engine version) lives in the document, outputs are derived geometry
//! with stable ids per key, one undo step per command.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test helpers outside #[test] fns

use burnburn_document::engine::{EngineError, EngineRecipe, ParamValue};
use burnburn_document::generator::{GeneratorCommand, GeneratorDefinition, GeneratorId, generator_inputs};
use burnburn_document::process::{Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange};
use burnburn_document::process::{ProcessCommand, ProcessSettings, Speed};
use burnburn_document::trace::{TraceCommand, TraceDefinition, TraceId, TraceOutput};
use burnburn_document::{
    CommandError, CommandPayload, DerivedKey, DerivedOutput, DerivedOwner, DerivedOwnerKind, DocumentCommand,
    DocumentPatch, DocumentSession, DocumentSnapshot, EvalDiagnosis, EvalStatus, Evaluator, GeometrySource, ObjectKind,
    OwnerSolution,
};
use burnburn_geometry::{Affine, Path, Point, Rect, Vector};
use burnburn_types::{ContentHash, DocumentId, GeometryId, Length, RequestId, StableId};

fn session() -> DocumentSession {
    DocumentSession::new(DocumentId::from_raw(1))
}

fn cmd(s: &DocumentSession, payload: CommandPayload) -> DocumentCommand {
    DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload }
}

fn square(x: f64, size: f64) -> Path {
    let l = Length::from_mm(size).unwrap();
    Path::rectangle(Rect::new(Point::new(x, 0.0), l, l).unwrap())
}

fn shift(x: f64, y: f64) -> Affine {
    Affine::translate(Vector::new(x, y)).unwrap()
}

fn recipe(tolerance: f64) -> EngineRecipe {
    EngineRecipe::new("burnburn-trace/contour-1")
        .with("fit.tolerance_mm", ParamValue::Number(tolerance))
        .with("image.threshold", ParamValue::Text("otsu".into()))
}

fn definition(key: u8, placement: Affine) -> TraceDefinition {
    TraceDefinition {
        name: "Logo".into(),
        source: ContentHash::from_bytes([7; 32]),
        recipe: recipe(0.05),
        key: ContentHash::from_bytes([key; 32]),
        placement,
    }
}

fn out(local: u64, x: f64) -> TraceOutput {
    TraceOutput { local, kind: ObjectKind::Path, path: square(x, 10.0), name: Some(format!("contour {local}")) }
}

fn outputs_of(snap: &DocumentSnapshot, owner: DerivedOwner) -> Vec<(u64, GeometryId)> {
    snap.objects()
        .filter_map(|(id, o)| match o.source() {
            GeometrySource::Derived(k) if k.owner == owner => Some((k.local, id)),
            _ => None,
        })
        .collect()
}

fn apply_trace(s: &mut DocumentSession) -> TraceId {
    let c = TraceCommand::Create {
        definition: Box::new(definition(1, shift(100.0, 0.0))),
        outputs: vec![out(1, 0.0), out(2, 20.0)],
    };
    s.apply(cmd(s, CommandPayload::Trace(c))).unwrap();
    let snap = s.snapshot();
    snap.engine_layer().traces().next().map(|(id, _)| id).expect("trace stored")
}

#[test]
fn applying_a_trace_creates_placed_derived_outputs_with_recipe_in_one_undo_step() {
    let mut s = session();
    let trace = apply_trace(&mut s);
    let snap = s.snapshot();
    let def = snap.trace(trace).unwrap();
    assert_eq!(def.recipe.engine, "burnburn-trace/contour-1");
    assert_eq!(def.recipe.get("fit.tolerance_mm"), Some(&ParamValue::Number(0.05)));
    let owner = DerivedOwner::Trace(trace);
    let outputs = outputs_of(&snap, owner);
    assert_eq!(outputs.iter().map(|(k, _)| *k).collect::<Vec<_>>(), vec![1, 2]);
    // Delivered trace-local; the document places them.
    let b = snap.object(outputs[0].1).unwrap().bounds().unwrap();
    assert_eq!((b.min().x, b.max().x), (100.0, 110.0));
    let record = snap.derived_record(owner).unwrap();
    assert_eq!(record.status, EvalStatus::Valid);
    assert_eq!(record.inputs, Some(ContentHash::from_bytes([1; 32])));
    assert!(!snap.production_blocked(outputs[0].1));

    s.undo(s.revision()).unwrap();
    assert!(s.snapshot().trace(trace).is_none());
    assert!(s.snapshot().is_empty());
    s.redo(s.revision()).unwrap();
    assert_eq!(outputs_of(&s.snapshot(), owner), outputs, "redo restores the same ids");
}

#[test]
fn traced_output_refuses_direct_transform_and_moves_with_set_placement() {
    let mut s = session();
    let trace = apply_trace(&mut s);
    let owner = DerivedOwner::Trace(trace);
    let ids = outputs_of(&s.snapshot(), owner);
    let err = s
        .apply(cmd(&s, CommandPayload::Transform { targets: vec![ids[0].1], transform: shift(1.0, 0.0) }))
        .unwrap_err();
    assert_eq!(err, CommandError::DerivedObject { owner });

    let c = TraceCommand::SetPlacement { trace, placement: shift(100.0, 50.0) };
    s.apply(cmd(&s, CommandPayload::Trace(c))).unwrap();
    let snap = s.snapshot();
    assert_eq!(outputs_of(&snap, owner), ids, "moving keeps ids");
    let b = snap.object(ids[1].1).unwrap().bounds().unwrap();
    assert_eq!((b.min().x, b.min().y), (120.0, 50.0));
    assert_eq!(snap.trace(trace).unwrap().placement, shift(100.0, 50.0));
}

#[test]
fn retrace_keeps_geometry_id_per_contour_key_and_unbinds_vanished_outputs() {
    let mut s = session();
    let trace = apply_trace(&mut s);
    let owner = DerivedOwner::Trace(trace);
    let before = outputs_of(&s.snapshot(), owner);
    let settings = ProcessSettings {
        speed: Speed::mm_per_s(10.0).unwrap(),
        power: PowerRange::new(PowerPercent::new(10.0).unwrap(), PowerPercent::new(20.0).unwrap()).unwrap(),
        passes: Passes::new(1).unwrap(),
        air_assist: false,
    };
    let draft = OperationDraft {
        name: "Snijden".into(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings,
        output_enabled: true,
    };
    s.apply(cmd(&s, CommandPayload::Process(ProcessCommand::AddOperation { draft, targets: vec![before[1].1] })))
        .unwrap();

    let mut def = definition(2, shift(100.0, 0.0));
    def.recipe = recipe(0.1);
    let c = TraceCommand::Replace { trace, definition: Box::new(def), outputs: vec![out(1, 0.0), out(3, 40.0)] };
    let outcome = s.apply(cmd(&s, CommandPayload::Trace(c))).unwrap();
    let snap = s.snapshot();
    let after = outputs_of(&snap, owner);
    assert_eq!(after[0], before[0], "key 1 keeps its GeometryId");
    assert_eq!(after.iter().map(|(k, _)| *k).collect::<Vec<_>>(), vec![1, 3]);
    assert!(snap.object(before[1].1).is_none());
    assert!(outcome.changes.deleted.contains(&before[1].1));
    assert!(snap.process().operations().all(|op| !op.targets().contains(&before[1].1)), "no dangling target");
    assert_eq!(snap.derived_record(owner).unwrap().inputs, Some(ContentHash::from_bytes([2; 32])));
    assert_eq!(snap.trace(trace).unwrap().recipe.get("fit.tolerance_mm"), Some(&ParamValue::Number(0.1)));
}

#[test]
fn apply_patch_for_a_trace_is_checked_against_its_recipe_key() {
    let mut s = session();
    let trace = apply_trace(&mut s);
    let owner = DerivedOwner::Trace(trace);
    let patch = |key: u8| DocumentPatch {
        owner,
        inputs: ContentHash::from_bytes([key; 32]),
        outputs: vec![DerivedOutput {
            key: DerivedKey { owner, local: 5 },
            kind: ObjectKind::Path,
            path: square(0.0, 5.0),
            name: None,
        }],
        status: EvalStatus::Valid,
        solution: OwnerSolution::Keep,
    };
    let err = s.apply(cmd(&s, CommandPayload::ApplyPatch(patch(9)))).unwrap_err();
    assert_eq!(err, CommandError::StalePatch { owner });
    s.apply(cmd(&s, CommandPayload::ApplyPatch(patch(1)))).unwrap();
    let snap = s.snapshot();
    let outputs = outputs_of(&snap, owner);
    assert_eq!(outputs.iter().map(|(k, _)| *k).collect::<Vec<_>>(), vec![5]);
    let b = snap.object(outputs[0].1).unwrap().bounds().unwrap();
    assert_eq!(b.min().x, 100.0, "patch outputs are trace-local and placed by the document");
}

#[test]
fn trace_with_a_non_finite_recipe_value_is_refused_and_changes_nothing() {
    let mut s = session();
    let mut def = definition(1, Affine::IDENTITY);
    def.recipe = recipe(f64::NAN);
    let c = TraceCommand::Create { definition: Box::new(def), outputs: vec![out(1, 0.0)] };
    let err = s.apply(cmd(&s, CommandPayload::Trace(c))).unwrap_err();
    assert_eq!(err, CommandError::Engine(EngineError::InvalidParam("fit.tolerance_mm".into())));
    assert!(s.snapshot().is_empty());
    assert!(!s.can_undo());
}

/// A square of side `size` (param) at the placement; fails when `fail` is true.
struct FakeGenerator;

impl Evaluator for FakeGenerator {
    fn owner_kind(&self) -> DerivedOwnerKind {
        DerivedOwnerKind::Generator
    }

    fn evaluate(&self, snapshot: &DocumentSnapshot, owner: DerivedOwner) -> DocumentPatch {
        let DerivedOwner::Generator(id) = owner else { panic!("only generators") };
        let def = snapshot.generator(id).unwrap();
        let inputs = generator_inputs(id, def);
        if def.recipe.get("fail") == Some(&ParamValue::Bool(true)) {
            return DocumentPatch {
                owner,
                inputs,
                outputs: vec![],
                status: EvalStatus::Failed { diagnosis: EvalDiagnosis::InvalidDefinition },
                solution: OwnerSolution::Keep,
            };
        }
        let Some(ParamValue::Number(size)) = def.recipe.get("size") else { panic!("size") };
        DocumentPatch {
            owner,
            inputs,
            outputs: vec![DerivedOutput {
                key: DerivedKey { owner, local: 0 },
                kind: ObjectKind::Path,
                path: square(0.0, *size).transform(&def.placement),
                name: Some(def.name.clone()),
            }],
            status: EvalStatus::Valid,
            solution: OwnerSolution::Keep,
        }
    }
}

fn generator(size: f64) -> GeneratorDefinition {
    GeneratorDefinition {
        name: "Doos".into(),
        generator: "square".into(),
        recipe: EngineRecipe::new("test-generators/1").with("size", ParamValue::Number(size)),
        placement: shift(5.0, 0.0),
    }
}

fn first_generator(s: &DocumentSession) -> GeneratorId {
    s.snapshot().engine_layer().generators().next().map(|(id, _)| id).expect("generator stored")
}

#[test]
fn generator_without_an_evaluator_is_pending_and_produces_nothing() {
    let mut s = session();
    let c = GeneratorCommand::Create { definition: Box::new(generator(10.0)) };
    s.apply(cmd(&s, CommandPayload::Generator(c))).unwrap();
    let g = first_generator(&s);
    let snap = s.snapshot();
    assert_eq!(snap.derived_record(DerivedOwner::Generator(g)).unwrap().status, EvalStatus::Pending);
    assert!(outputs_of(&snap, DerivedOwner::Generator(g)).is_empty());
}

#[test]
fn generator_outputs_follow_parameter_edits_in_the_same_transaction_with_stable_ids() {
    let mut s = session();
    let ev: [&dyn Evaluator; 1] = [&FakeGenerator];
    let c = GeneratorCommand::Create { definition: Box::new(generator(10.0)) };
    s.apply_with(cmd(&s, CommandPayload::Generator(c)), &ev).unwrap();
    let g = first_generator(&s);
    let owner = DerivedOwner::Generator(g);
    let first = outputs_of(&s.snapshot(), owner);
    assert_eq!(first.len(), 1);
    let rec = s.snapshot().derived_record(owner).cloned().unwrap();
    assert_eq!(rec.status, EvalStatus::Valid);
    assert_eq!(rec.inputs, Some(generator_inputs(g, s.snapshot().generator(g).unwrap())));

    let c = GeneratorCommand::Set { generator: g, definition: Box::new(generator(20.0)) };
    s.apply_with(cmd(&s, CommandPayload::Generator(c)), &ev).unwrap();
    let snap = s.snapshot();
    assert_eq!(outputs_of(&snap, owner), first, "same GeometryId after a parameter edit");
    let b = snap.object(first[0].1).unwrap().bounds().unwrap();
    assert_eq!((b.min().x, b.max().x), (5.0, 25.0));
    let err = s.apply(cmd(&s, CommandPayload::Transform { targets: vec![first[0].1], transform: shift(1.0, 0.0) }));
    assert_eq!(err.unwrap_err(), CommandError::DerivedObject { owner });

    s.undo(s.revision()).unwrap();
    let b = s.snapshot().object(first[0].1).unwrap().bounds().unwrap();
    assert_eq!(b.max().x, 15.0, "undo restores definition and output together");

    let c = GeneratorCommand::Delete { generator: g };
    s.apply_with(cmd(&s, CommandPayload::Generator(c)), &ev).unwrap();
    assert!(s.snapshot().is_empty());
    assert!(s.snapshot().derived_record(owner).is_none());
}

#[test]
fn failed_generator_evaluation_keeps_last_output_and_blocks_production() {
    let mut s = session();
    let ev: [&dyn Evaluator; 1] = [&FakeGenerator];
    let c = GeneratorCommand::Create { definition: Box::new(generator(10.0)) };
    s.apply_with(cmd(&s, CommandPayload::Generator(c)), &ev).unwrap();
    let g = first_generator(&s);
    let owner = DerivedOwner::Generator(g);
    let mut bad = generator(30.0);
    bad.recipe = bad.recipe.with("fail", ParamValue::Bool(true));
    let c = GeneratorCommand::Set { generator: g, definition: Box::new(bad) };
    s.apply_with(cmd(&s, CommandPayload::Generator(c)), &ev).unwrap();
    let snap = s.snapshot();
    let ids = outputs_of(&snap, owner);
    assert_eq!(snap.object(ids[0].1).unwrap().bounds().unwrap().max().x, 15.0, "last valid output stays");
    assert!(snap.production_blocked(ids[0].1));
    assert!(matches!(snap.derived_record(owner).unwrap().status, EvalStatus::Failed { .. }));
}
