//! Sketch commands: one document transaction each, through `CommandPayload::Sketch`.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use burnburn_types::IdAllocator;

use super::{ConstraintId, DimensionRole, EntityId, Sketch, SketchConstraint, SketchEntity, SketchId};
use crate::patch::{DerivedOwner, DerivedRecord};

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum SketchCommand {
    /// Adds a sketch definition (built locally with the `Sketch` builders).
    Create {
        sketch: Box<Sketch>,
    },
    /// Removes the sketch and its derived outputs.
    Delete {
        sketch: SketchId,
    },
    AddEntity {
        sketch: SketchId,
        entity: SketchEntity,
    },
    AddConstraint {
        sketch: SketchId,
        constraint: SketchConstraint,
    },
    RemoveConstraint {
        sketch: SketchId,
        constraint: ConstraintId,
    },
    /// Defines or replaces a parameter; the text is stored as typed and checked by the evaluator.
    SetParameter {
        sketch: SketchId,
        name: String,
        text: String,
    },
    RemoveParameter {
        sketch: SketchId,
        name: String,
    },
    SetDimensionValue {
        sketch: SketchId,
        constraint: ConstraintId,
        text: String,
    },
    SetDimensionRole {
        sketch: SketchId,
        constraint: ConstraintId,
        role: DimensionRole,
    },
    /// Projects geometry of another document object into the sketch as a read-only entity, so a
    /// sketch relation (tangent, symmetric, …) can hold against it (IM21 relations between
    /// objects). **Not supported yet:** always refused with `SketchError::ExternalNotSupported`
    /// (design and open points: `docs/coordination/sketch.md`).
    AddExternal {
        sketch: SketchId,
        target: crate::structure::GeometryRef,
    },
}

/// Undo label for sketch commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SketchLabel {
    Create,
    Delete,
    AddEntity,
    AddConstraint,
    RemoveConstraint,
    SetParameter,
    RemoveParameter,
    SetDimensionValue,
    SetDimensionRole,
    AddExternal,
}

impl SketchCommand {
    pub fn label(&self) -> SketchLabel {
        match self {
            SketchCommand::Create { .. } => SketchLabel::Create,
            SketchCommand::Delete { .. } => SketchLabel::Delete,
            SketchCommand::AddEntity { .. } => SketchLabel::AddEntity,
            SketchCommand::AddConstraint { .. } => SketchLabel::AddConstraint,
            SketchCommand::RemoveConstraint { .. } => SketchLabel::RemoveConstraint,
            SketchCommand::SetParameter { .. } => SketchLabel::SetParameter,
            SketchCommand::RemoveParameter { .. } => SketchLabel::RemoveParameter,
            SketchCommand::SetDimensionValue { .. } => SketchLabel::SetDimensionValue,
            SketchCommand::SetDimensionRole { .. } => SketchLabel::SetDimensionRole,
            SketchCommand::AddExternal { .. } => SketchLabel::AddExternal,
        }
    }
}

/// Why a sketch edit was refused. The document is unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SketchError {
    UnknownSketch(SketchId),
    UnknownEntity(EntityId),
    /// A line, arc or circle must be built on point entities.
    WrongEntityKind(EntityId),
    UnknownConstraint(ConstraintId),
    NotADimension(ConstraintId),
    UnknownParameter(String),
    InvalidName(String),
    /// A position or radius is not finite (or a radius is negative).
    InvalidValue,
    /// A stored id is duplicated or not below its counter.
    InvalidId(u32),
    /// A solution refers to entities that are not points/circles of the sketch.
    InvalidSolution,
    IdsExhausted,
    /// External (projected) entities from other objects are not supported yet.
    ExternalNotSupported,
}

impl fmt::Display for SketchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SketchError::UnknownSketch(s) => write!(f, "no sketch {s}"),
            SketchError::UnknownEntity(e) => write!(f, "no sketch entity {}", e.raw()),
            SketchError::WrongEntityKind(e) => write!(f, "sketch entity {} is not a point", e.raw()),
            SketchError::UnknownConstraint(c) => write!(f, "no sketch constraint {}", c.raw()),
            SketchError::NotADimension(c) => write!(f, "sketch constraint {} is not a dimension", c.raw()),
            SketchError::UnknownParameter(n) => write!(f, "no parameter '{n}'"),
            SketchError::InvalidName(n) => write!(f, "'{n}' is not a valid parameter name"),
            SketchError::InvalidValue => f.write_str("sketch value is not finite"),
            SketchError::InvalidId(raw) => write!(f, "invalid stored sketch id {raw}"),
            SketchError::InvalidSolution => f.write_str("sketch solution does not match its entities"),
            SketchError::IdsExhausted => f.write_str("sketch ids exhausted"),
            SketchError::ExternalNotSupported => {
                f.write_str("sketch entities from other objects are not supported yet")
            }
        }
    }
}

impl std::error::Error for SketchError {}

/// All sketches of a document, plus the derived-output bookkeeping per owner (§11).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SketchLayer {
    pub(crate) sketches: BTreeMap<SketchId, Arc<Sketch>>,
    pub(crate) derived: BTreeMap<DerivedOwner, DerivedRecord>,
}

impl SketchLayer {
    pub fn new() -> Self {
        SketchLayer::default()
    }

    pub fn sketch(&self, id: SketchId) -> Option<&Sketch> {
        self.sketches.get(&id).map(|s| s.as_ref())
    }

    pub fn sketches(&self) -> impl ExactSizeIterator<Item = (SketchId, &Sketch)> {
        self.sketches.iter().map(|(k, v)| (*k, v.as_ref()))
    }

    pub fn record(&self, owner: DerivedOwner) -> Option<&DerivedRecord> {
        self.derived.get(&owner)
    }

    pub fn records(&self) -> impl ExactSizeIterator<Item = (DerivedOwner, &DerivedRecord)> {
        self.derived.iter().map(|(k, v)| (*k, v))
    }

    /// Stored layer (open/recovery). Records must belong to sketches of the layer.
    pub fn from_parts(
        sketches: Vec<(SketchId, Sketch)>,
        records: Vec<(DerivedOwner, DerivedRecord)>,
    ) -> Result<Self, SketchError> {
        let mut layer = SketchLayer::new();
        for (id, s) in sketches {
            if layer.sketches.insert(id, Arc::new(s)).is_some() {
                return Err(SketchError::UnknownSketch(id));
            }
        }
        for (owner, r) in records {
            let DerivedOwner::Sketch(id) = owner else {
                // Records of other owners (modifiers, parts) live in the structure layer.
                return Err(SketchError::InvalidSolution);
            };
            if !layer.sketches.contains_key(&id) {
                return Err(SketchError::UnknownSketch(id));
            }
            layer.derived.insert(owner, r);
        }
        Ok(layer)
    }
}

/// What a sketch command may touch.
pub(crate) struct SketchTxn<'a> {
    pub layer: &'a mut SketchLayer,
    pub ids: &'a mut IdAllocator,
    pub changed: &'a mut Vec<SketchId>,
}

/// What the session must do after a sketch command.
pub(crate) enum SketchEffect {
    None,
    /// The sketch is gone: remove its derived outputs.
    Deleted(SketchId),
}

fn edit(
    layer: &mut SketchLayer,
    changed: &mut Vec<SketchId>,
    id: SketchId,
    f: impl FnOnce(&mut Sketch) -> Result<(), SketchError>,
) -> Result<SketchEffect, SketchError> {
    let slot = layer.sketches.get_mut(&id).ok_or(SketchError::UnknownSketch(id))?;
    let mut next = (**slot).clone();
    f(&mut next)?;
    *slot = Arc::new(next);
    if !changed.contains(&id) {
        changed.push(id);
    }
    Ok(SketchEffect::None)
}

pub(crate) fn apply(cmd: SketchCommand, txn: SketchTxn<'_>) -> Result<SketchEffect, SketchError> {
    let SketchTxn { layer, ids, changed } = txn;
    match cmd {
        // Not supported yet (see `docs/coordination/sketch.md`): refused, nothing changes.
        SketchCommand::AddExternal { sketch, .. } => {
            layer.sketches.get(&sketch).ok_or(SketchError::UnknownSketch(sketch))?;
            Err(SketchError::ExternalNotSupported)
        }
        SketchCommand::Create { sketch } => {
            let id: SketchId = ids.allocate().map_err(|_| SketchError::IdsExhausted)?;
            layer.sketches.insert(id, Arc::new(*sketch));
            changed.push(id);
            Ok(SketchEffect::None)
        }
        SketchCommand::Delete { sketch } => {
            layer.sketches.remove(&sketch).ok_or(SketchError::UnknownSketch(sketch))?;
            layer.derived.remove(&DerivedOwner::Sketch(sketch));
            changed.push(sketch);
            Ok(SketchEffect::Deleted(sketch))
        }
        SketchCommand::AddEntity { sketch, entity } => {
            edit(layer, changed, sketch, |s| s.add_entity(entity).map(|_| ()))
        }
        SketchCommand::AddConstraint { sketch, constraint } => {
            edit(layer, changed, sketch, |s| s.add_constraint(constraint).map(|_| ()))
        }
        SketchCommand::RemoveConstraint { sketch, constraint } => {
            edit(layer, changed, sketch, |s| s.remove_constraint(constraint).map(|_| ()))
        }
        SketchCommand::SetParameter { sketch, name, text } => {
            edit(layer, changed, sketch, |s| s.set_parameter(&name, &text))
        }
        SketchCommand::RemoveParameter { sketch, name } => edit(layer, changed, sketch, |s| s.remove_parameter(&name)),
        SketchCommand::SetDimensionValue { sketch, constraint, text } => {
            edit(layer, changed, sketch, |s| s.set_dimension_text(constraint, &text))
        }
        SketchCommand::SetDimensionRole { sketch, constraint, role } => {
            edit(layer, changed, sketch, |s| s.set_dimension_role(constraint, role))
        }
    }
}
