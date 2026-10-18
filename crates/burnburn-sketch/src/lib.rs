//! Parametric sketches for BurnBurn: the parameter parser (units, expressions, cycles), a
//! deterministic constraint solver behind [`solver::Solver`], and the document evaluator.
//!
//! The sketch *data* lives in `burnburn_document::sketch` (DR-12), so undo, save/reopen and CAM
//! see it. This crate reads a snapshot and returns a `DocumentPatch` ([`SketchEvaluator`],
//! contract §11); it never owns a session. A failed evaluation keeps the last valid geometry and
//! the document then reports `production_blocked` for its outputs.
//!
//! NOTICE: some patterns (starting each solve from the last valid solution, measuring driven
//! dimensions on the solved geometry) follow the MIT/Apache-licensed `solidcraft-sketch` crate
//! by the same owner. The numeric solver is `sketch-solver` 0.1.0 (MIT OR Apache-2.0) behind
//! [`solver::SketchSolverAdapter`].

mod evaluate;
mod evaluator;
mod output;
pub mod param;
pub mod solver;

pub use evaluate::{Diagnosis, DimensionError, Evaluation, Health, SolvedSketch, evaluate_sketch, measure};
pub use evaluator::{EVALUATOR_VERSION, SketchEvaluator, inputs_hash};
