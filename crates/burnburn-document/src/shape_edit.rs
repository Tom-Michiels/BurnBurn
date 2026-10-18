//! Parametric basic shapes in the document (W1-L2b): create, edit the parameters, convert to a
//! plain path. Their own payload keeps the shared `GeometryCommand` untouched.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use burnburn_geometry::{GeometryError, ParametricShape, ShapeParamError, ShapeParams};
use burnburn_types::{GeometryId, IdAllocator};

use crate::command::{ChangeSet, CommandWarning};
use crate::object::{GeometryObject, GeometrySource, ObjectKind};

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ShapeCommand {
    /// A new basic shape; the object kind follows the parameters.
    Create { shape: ParametricShape, name: String },
    /// New parameters for an existing parametric object; identity and placement stay.
    SetParams { target: GeometryId, params: ShapeParams },
    /// The outline stays; the definition goes. Not undoable except through undo.
    ConvertToPath { targets: Vec<GeometryId> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ShapeLabel {
    Create,
    SetParams,
    ConvertToPath,
}

impl ShapeCommand {
    pub fn label(&self) -> ShapeLabel {
        match self {
            ShapeCommand::Create { .. } => ShapeLabel::Create,
            ShapeCommand::SetParams { .. } => ShapeLabel::SetParams,
            ShapeCommand::ConvertToPath { .. } => ShapeLabel::ConvertToPath,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ShapeError {
    UnknownGeometry(GeometryId),
    /// The target is a plain path (or evaluator output): there is no definition to edit.
    NotParametric(GeometryId),
    Param(ShapeParamError),
    Geometry(GeometryError),
    NothingChanged,
    IdsExhausted,
}

impl fmt::Display for ShapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ShapeError {}

pub(crate) struct ShapeTxn<'a> {
    pub objects: &'a mut BTreeMap<GeometryId, Arc<GeometryObject>>,
    pub ids: &'a mut IdAllocator,
    pub changes: &'a mut ChangeSet,
    pub warnings: &'a mut Vec<CommandWarning>,
}

pub(crate) fn apply(cmd: ShapeCommand, txn: ShapeTxn<'_>) -> Result<(), ShapeError> {
    let ShapeTxn { objects, ids, changes, warnings } = txn;
    match cmd {
        ShapeCommand::Create { shape, name } => {
            let id: GeometryId = ids.allocate().map_err(|_| ShapeError::IdsExhausted)?;
            if !name.is_empty()
                && let Some(other) = objects.values().find(|o| o.name() == name)
            {
                warnings.push(CommandWarning::DuplicateName { name: name.clone(), other: other.id() });
            }
            let kind = ObjectKind::of_shape(shape.params());
            let object = GeometryObject::new_parametric(id, name, kind, GeometrySource::Drawn, shape)
                .map_err(ShapeError::Geometry)?;
            objects.insert(id, Arc::new(object));
            changes.created.push(id);
        }
        ShapeCommand::SetParams { target, params } => {
            let object = objects.get(&target).cloned().ok_or(ShapeError::UnknownGeometry(target))?;
            let shape = object.parametric().ok_or(ShapeError::NotParametric(target))?;
            if *shape.params() == params {
                return Err(ShapeError::NothingChanged);
            }
            let edited = shape.with_params(params).map_err(ShapeError::Param)?;
            let object = object.with_shape(edited).map_err(ShapeError::Geometry)?;
            objects.insert(target, Arc::new(object));
            changes.modified.push(target);
        }
        ShapeCommand::ConvertToPath { targets } => {
            let mut unique: Vec<GeometryId> = Vec::with_capacity(targets.len());
            for id in targets {
                if !unique.contains(&id) {
                    unique.push(id);
                }
            }
            if unique.is_empty() {
                return Err(ShapeError::NothingChanged);
            }
            // Check everything before changing anything: a mixed selection fails as a whole.
            for id in &unique {
                let object = objects.get(id).ok_or(ShapeError::UnknownGeometry(*id))?;
                if object.parametric().is_none() {
                    return Err(ShapeError::NotParametric(*id));
                }
            }
            for id in unique {
                if let Some(object) = objects.get(&id).cloned() {
                    objects.insert(id, Arc::new(object.without_definition()));
                    changes.modified.push(id);
                }
            }
        }
    }
    Ok(())
}
