//! Derived geometry whose owner's last evaluation is not valid (a failed generator, a text whose
//! font went missing, a sketch that no longer solves) keeps its last outline on the canvas, but
//! that outline is not the current definition: preflight must refuse it, so it never reaches a
//! job ("CAM must refuse to export it", `DocumentSnapshot::production_blocked`). Found while
//! integrating golf 7 (G7-editor report): compile and preflight did not look at the flag.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)] // test helpers outside #[test] fns

mod common;

use burnburn_cam::{Finding, Severity, compile, preflight};
use burnburn_document::engine::{EngineRecipe, ParamValue};
use burnburn_document::generator::{GeneratorCommand, GeneratorDefinition, GeneratorId, generator_inputs};
use burnburn_document::{
    CommandPayload, DerivedKey, DerivedOutput, DerivedOwner, DerivedOwnerKind, DocumentCommand, DocumentPatch,
    DocumentSession, DocumentSnapshot, EvalDiagnosis, EvalStatus, Evaluator, GeometrySource, ObjectKind, OwnerSolution,
};
use burnburn_geometry::{Affine, Vector};
use burnburn_types::{CancellationToken, GeometryId, RequestId, StableId};
use common::*;

/// A generator that draws a square of `size` mm, or fails when its recipe says `fail`.
struct SquareGenerator;

impl Evaluator for SquareGenerator {
    fn owner_kind(&self) -> DerivedOwnerKind {
        DerivedOwnerKind::Generator
    }

    fn evaluate(&self, snapshot: &DocumentSnapshot, owner: DerivedOwner) -> DocumentPatch {
        let DerivedOwner::Generator(id) = owner else { panic!("only generators") };
        let def = snapshot.generator(id).unwrap();
        let inputs = generator_inputs(id, def);
        if def.recipe.get("fail") == Some(&ParamValue::Bool(true)) {
            let status = EvalStatus::Failed { diagnosis: EvalDiagnosis::InvalidDefinition };
            return DocumentPatch { owner, inputs, outputs: vec![], status, solution: OwnerSolution::Keep };
        }
        let Some(ParamValue::Number(size)) = def.recipe.get("size") else { panic!("size") };
        let path = path_of(&[(0.0, 0.0), (*size, 0.0), (*size, *size), (0.0, *size)]).transform(&def.placement);
        DocumentPatch {
            owner,
            inputs,
            outputs: vec![DerivedOutput {
                key: DerivedKey { owner, local: 0 },
                kind: ObjectKind::Path,
                path,
                name: Some(def.name.clone()),
            }],
            status: EvalStatus::Valid,
            solution: OwnerSolution::Keep,
        }
    }
}

fn generator(size: f64, fail: bool) -> GeneratorDefinition {
    let mut recipe = EngineRecipe::new("test-generators/1").with("size", ParamValue::Number(size));
    if fail {
        recipe = recipe.with("fail", ParamValue::Bool(true));
    }
    GeneratorDefinition {
        name: "Doos".into(),
        generator: "square".into(),
        recipe,
        placement: Affine::translate(Vector::new(100.0, 100.0)).unwrap(),
    }
}

fn apply(s: &mut DocumentSession, payload: CommandPayload) {
    let ev: [&dyn Evaluator; 1] = [&SquareGenerator];
    let cmd = DocumentCommand { document: s.id(), expected: s.revision(), request: RequestId::from_raw(1), payload };
    s.apply_with(cmd, &ev).unwrap();
}

/// A session with a valid generator output cut by one Line operation; returns the generator and
/// the output's id.
fn generated_square(s: &mut DocumentSession) -> (GeneratorId, GeometryId) {
    apply(s, CommandPayload::Generator(GeneratorCommand::Create { definition: Box::new(generator(20.0, false)) }));
    let snap = s.snapshot();
    let g = snap.engine_layer().generators().next().map(|(id, _)| id).unwrap();
    let out = snap
        .objects()
        .find_map(|(id, o)| match o.source() {
            GeometrySource::Derived(k) if k.owner == DerivedOwner::Generator(g) => Some(id),
            _ => None,
        })
        .unwrap();
    (g, out)
}

#[test]
fn the_output_of_a_failed_generator_is_a_preflight_error_named_by_object_and_operation() {
    let mut s = session();
    let (g, out) = generated_square(&mut s);
    let op = add_op(&mut s, line_op("Snijden", 100.0, 20.0), vec![out]);
    let set = GeneratorCommand::Set { generator: g, definition: Box::new(generator(40.0, true)) };
    apply(&mut s, CommandPayload::Generator(set));
    assert!(s.snapshot().production_blocked(out), "fixture: the last outline stays, blocked");

    let plan = compile(&s.snapshot(), &profile(), &options(), &CancellationToken::never()).unwrap();
    let report = preflight(&s.snapshot(), &plan, &profile());
    let blocked = Finding::BlockedGeometry { operation: op, geometry: out };
    assert!(report.findings.contains(&blocked), "{:?}", report.findings);
    assert_eq!(blocked.severity(), Severity::Error);
    assert!(report.has_errors(), "a blocked outline never passes preflight");
}

#[test]
fn valid_derived_output_and_drawn_geometry_pass() {
    let mut s = session();
    let (_, out) = generated_square(&mut s);
    let drawn = rectangle(&mut s, 300.0, 100.0, 20.0, 20.0);
    add_op(&mut s, line_op("Snijden", 100.0, 20.0), vec![out, drawn]);
    let plan = compile(&s.snapshot(), &profile(), &options(), &CancellationToken::never()).unwrap();
    let report = preflight(&s.snapshot(), &plan, &profile());
    assert!(!report.has_errors(), "{:?}", report.findings);
}
