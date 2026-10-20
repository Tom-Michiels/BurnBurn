//! The document core: immutable snapshots, transactional commands with an expected revision,
//! and undo/redo that restores content and identity.
//!
//! One [`DocumentSession::apply`] is one transaction and one history entry. A failed command
//! changes nothing. Machine actions are never document commands, so undo can never replay them.
//! This crate has no GUI, GPU, filesystem or network dependency.

mod canonical;
mod command;
mod edit;
pub mod engine;
pub mod generator;
mod object;
mod patch;
pub mod process;
mod session;
mod shape_edit;
pub mod sketch;
mod snapshot;
pub mod structure;
pub mod text;
pub mod trace;

pub use command::{
    Anchor, Axis, ChangeSet, CommandError, CommandOutcome, CommandPayload, CommandWarning, DocumentCommand, UndoLabel,
    ValueError,
};
pub use edit::{
    AlignTarget, EditError, EditLabel, GeometryCommand, ImportItem, MoveAnchor, OpsCommand, PlaceCommand, PlaceUnits,
    ReshapeCommand,
};
pub use object::{
    DEFINITION_TOLERANCE_MM, DefinitionError, EdgeKey, EdgeKeyError, EdgeKeys, GeometryObject, GeometrySource,
    ObjectKind,
};
pub use patch::{
    DerivedKey, DerivedOutput, DerivedOwner, DerivedOwnerKind, DerivedRecord, DocumentPatch, EvalDiagnosis, EvalStatus,
    Evaluator, ModifierFailure, OwnerSolution,
};
pub use session::DocumentSession;
pub use shape_edit::{ShapeCommand, ShapeError, ShapeLabel};
pub use snapshot::{DocumentSnapshot, PlateView, SnapshotError};
