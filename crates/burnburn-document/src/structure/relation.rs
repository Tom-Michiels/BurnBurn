//! Geometric relations between objects (S05) over associative references. The document checks
//! them after every transaction and reports the residual; enforcing them is the sketch
//! solver's job (see docs/coordination/sketch.md), so a relation never moves geometry here.

use super::ReferenceId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ObjectRelation {
    /// Edge `a` touches edge `b` tangentially (line–arc or arc–arc).
    Tangent { a: ReferenceId, b: ReferenceId },
    /// Vertex `b` is the mirror image of vertex `a` about the line edge `axis`.
    Symmetric { a: ReferenceId, b: ReferenceId, axis: ReferenceId },
}

impl ObjectRelation {
    pub fn references(&self) -> Vec<ReferenceId> {
        match *self {
            ObjectRelation::Tangent { a, b } => vec![a, b],
            ObjectRelation::Symmetric { a, b, axis } => vec![a, b, axis],
        }
    }
}

/// Distance below which a relation counts as satisfied.
pub const RELATION_TOLERANCE_MM: f64 = 1e-6;

#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum RelationState {
    Satisfied {
        residual_mm: f64,
    },
    Violated {
        residual_mm: f64,
    },
    /// A reference of the relation is broken or ambiguous; nothing is rebound.
    Broken {
        reference: ReferenceId,
    },
    /// The referenced entities are not of a kind this relation is defined for (for example a
    /// tangent between two lines, or an edge made of several segments).
    Unsupported,
}
