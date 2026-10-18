//! The document evaluator (contract §11): reads a sketch from a snapshot and returns the patch
//! with its derived contours, the new last valid solution, or the failure diagnosis.

use burnburn_document::sketch::Sketch;
use burnburn_document::sketch::canonical::Definition;
use burnburn_document::{
    DerivedKey, DerivedOutput, DerivedOwner, DerivedOwnerKind, DocumentPatch, DocumentSnapshot, EvalDiagnosis,
    EvalStatus, Evaluator, ObjectKind, OwnerSolution,
};
use burnburn_types::{CancellationToken, CanonicalEncode, CanonicalWriter, ContentHash, HashDomain};

use crate::evaluate::evaluate_sketch;
use crate::solver::Solver;

/// Bumped when the evaluator's output for the same definition can change.
pub const EVALUATOR_VERSION: u64 = 1;

/// Hash over the evaluator inputs: the sketch definition and the evaluator version. The stored
/// solution is a starting point, not an input: re-evaluating from it reproduces the outputs.
pub fn inputs_hash(sketch: &Sketch) -> Option<ContentHash> {
    let mut w = CanonicalWriter::new();
    w.tag("sketch-evaluator");
    w.u64(EVALUATOR_VERSION);
    Definition(sketch).encode(&mut w);
    w.finish().ok().map(|bytes| ContentHash::of(HashDomain::EvalSketch, &bytes))
}

/// Synchronous sketch evaluator for `DocumentSession::apply_with`. Deterministic; bounded by
/// the solver's iteration limits (never wall-clock).
#[derive(Debug, Clone, Default)]
pub struct SketchEvaluator<S> {
    solver: S,
}

impl<S: Solver> SketchEvaluator<S> {
    pub fn new(solver: S) -> Self {
        SketchEvaluator { solver }
    }
}

fn failed(owner: DerivedOwner, inputs: ContentHash, diagnosis: EvalDiagnosis) -> DocumentPatch {
    DocumentPatch {
        owner,
        inputs,
        outputs: Vec::new(),
        status: EvalStatus::Failed { diagnosis },
        solution: OwnerSolution::Keep,
    }
}

impl<S: Solver> Evaluator for SketchEvaluator<S> {
    fn owner_kind(&self) -> DerivedOwnerKind {
        DerivedOwnerKind::Sketch
    }

    fn evaluate(&self, snapshot: &DocumentSnapshot, owner: DerivedOwner) -> DocumentPatch {
        let none = ContentHash::from_bytes([0; 32]);
        let DerivedOwner::Sketch(id) = owner else {
            return failed(owner, none, EvalDiagnosis::InvalidDefinition);
        };
        let Some(sketch) = snapshot.sketch(id) else {
            return failed(owner, none, EvalDiagnosis::InvalidDefinition);
        };
        let Some(inputs) = inputs_hash(sketch) else {
            return failed(owner, none, EvalDiagnosis::InvalidDefinition);
        };
        let evaluation = match evaluate_sketch(sketch, &self.solver, &CancellationToken::never()) {
            Ok(e) => e,
            Err(_) => return failed(owner, inputs, EvalDiagnosis::NotConverged { unmet: Vec::new() }),
        };
        if let Some(diagnosis) = evaluation.failure() {
            return failed(owner, inputs, diagnosis);
        }
        let Some(solved) = evaluation.solved.as_ref() else {
            return failed(owner, inputs, EvalDiagnosis::InvalidDefinition);
        };
        let Ok(contours) = solved.outputs() else {
            return failed(owner, inputs, EvalDiagnosis::InvalidDefinition);
        };
        let outputs = contours
            .into_iter()
            .map(|(local, path)| DerivedOutput {
                key: DerivedKey { owner, local },
                kind: ObjectKind::Path,
                path,
                name: Some(sketch.name().to_owned()),
            })
            .collect();
        DocumentPatch {
            owner,
            inputs,
            outputs,
            status: EvalStatus::Valid,
            solution: OwnerSolution::Sketch(solved.solution(evaluation.dof)),
        }
    }
}
