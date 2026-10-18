//! Parametric sketch *data* (contract §12, DR-12). Module owner: Sketch role.
//!
//! The definition (entities, relations, dimensions with their expression text, parameters) and
//! the last valid solution live in the document, so undo, save/reopen and CAM see them. Parsing,
//! solving and turning a solution into geometry is `burnburn-sketch`'s evaluator, which reads a
//! snapshot and returns a [`crate::DocumentPatch`].

pub mod canonical;
mod command;
mod model;

pub use command::{SketchCommand, SketchError, SketchLabel, SketchLayer};
pub(crate) use command::{SketchEffect, SketchTxn, apply};
pub use model::{
    Along, ConstraintId, Dimension, DimensionKind, DimensionRole, Entity, EntityId, ParameterTable, Relation, Sketch,
    SketchConstraint, SketchEntity, SketchId, SketchSolution,
};
