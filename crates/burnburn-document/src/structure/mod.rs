//! IM21 document structure: non-destructive modifier chains, associative references and
//! relations, part definitions/instances/variants and plates (contract IM21).
//!
//! Modifier chains and part instances are evaluated by the document itself (pure geometry,
//! deterministic), in the transaction that changed their inputs; their outputs are derived
//! geometry (`DerivedOwner::{Modifier, Part}`) with stable `DerivedKey → GeometryId` reuse.
//! References are resolved through edge provenance and are never rebound automatically.

mod canonical;
mod command;
mod ids;
mod layer;
mod modifier;
mod part;
mod plate;
mod reference;
mod relation;

pub use command::{StructureChange, StructureCommand, StructureError, StructureLabel};
pub(crate) use command::{StructureEffect, StructureTxn, apply, chain_order};
pub use ids::{ModifierId, PartDefinitionId, PartInstanceId, PlateId, ReferenceId, RelationId};
pub use layer::{StructureLayer, StructureParts};
pub(crate) use layer::{relation_state_in, resolve_in};
pub use modifier::{EvalMode, MODIFIER_EVALUATOR_VERSION, Modifier, ModifierChain, ModifierEvaluator, ModifierStep};
pub(crate) use modifier::{chain_inputs, evaluate_chain};
pub use part::{
    Dim, NamedShape, PART_EVALUATOR_VERSION, PartDefinition, PartInstance, PartShape, effective_parameters,
};
pub(crate) use part::{evaluate_instance, instance_inputs};
pub use plate::{Plate, PlateIssue};
pub use reference::{
    AssocReference, BreakReason, EdgeName, EdgeOrigin, GeometryRef, ProvenanceEdge, RefHolder, RefStatus, Resolved,
    SubRef, VertexEnd,
};
pub use relation::{ObjectRelation, RELATION_TOLERANCE_MM, RelationState};
