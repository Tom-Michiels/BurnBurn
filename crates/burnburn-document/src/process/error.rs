use std::fmt;

use burnburn_types::{GeometryId, OperationId};

use super::RecipeId;

/// Why a process value, command or stored process layer was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProcessError {
    /// Must be finite and strictly positive.
    NotPositive {
        field: &'static str,
    },
    /// Outside the documented range of the field.
    OutOfRange {
        field: &'static str,
    },
    MinPowerAboveMax,
    UnknownOperation(OperationId),
    UnknownGeometry(GeometryId),
    UnknownRecipe(RecipeId),
    /// The same geometry twice in one operation (one reference is enough; no duplicate source).
    DuplicateTarget {
        operation: OperationId,
        geometry: GeometryId,
    },
    /// An operation would have to run before itself.
    SelfConstraint(OperationId),
    DuplicateConstraint {
        before: OperationId,
        after: OperationId,
    },
    /// Hard order constraints form a cycle; `operations` are the operations on that cycle.
    OrderCycle {
        operations: Vec<OperationId>,
    },
    /// Stored layer: an id appears twice, clashes with another entity or is not below `next_id`.
    InvalidId {
        raw: u64,
    },
}

impl fmt::Display for ProcessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProcessError::NotPositive { field } => write!(f, "{field} must be a finite, positive number"),
            ProcessError::OutOfRange { field } => write!(f, "{field} is out of range"),
            ProcessError::MinPowerAboveMax => f.write_str("minimum power is above maximum power"),
            ProcessError::UnknownOperation(id) => write!(f, "no operation {id}"),
            ProcessError::UnknownGeometry(id) => write!(f, "no geometry {id}"),
            ProcessError::UnknownRecipe(id) => write!(f, "no material recipe {id}"),
            ProcessError::DuplicateTarget { operation, geometry } => {
                write!(f, "{geometry} is already a target of {operation}")
            }
            ProcessError::SelfConstraint(id) => write!(f, "{id} cannot run before itself"),
            ProcessError::DuplicateConstraint { before, after } => {
                write!(f, "{before} is already ordered before {after}")
            }
            ProcessError::OrderCycle { operations } => {
                f.write_str("order constraints form a cycle through")?;
                for op in operations {
                    write!(f, " {op}")?;
                }
                Ok(())
            }
            ProcessError::InvalidId { raw } => write!(f, "id {raw} is duplicated, clashes or is not below next_id"),
        }
    }
}

impl std::error::Error for ProcessError {}
