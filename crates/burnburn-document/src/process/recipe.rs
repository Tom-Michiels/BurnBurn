use std::fmt;

use burnburn_types::{Length, StableId};

use super::{OperationMode, ProcessError, ProcessSettings};

/// Identity of a material recipe. Allocated from the project's shared id counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RecipeId(u64);

impl StableId for RecipeId {
    fn from_raw(raw: u64) -> Self {
        RecipeId(raw)
    }
    fn raw(self) -> u64 {
        self.0
    }
}

impl fmt::Display for RecipeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "rcp{}", self.0)
    }
}

/// Which recipe revision an operation's settings were copied from (provenance only: a later
/// recipe update never changes the operation, C08).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecipeRef {
    pub recipe: RecipeId,
    pub revision: u32,
}

/// A measured, machine-bound set of process settings for one material and thickness.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialRecipe {
    id: RecipeId,
    name: String,
    material: String,
    thickness: Length,
    /// Machine profile this recipe was measured on (profile id, not a transport address).
    machine: String,
    revision: u32,
    notes: String,
    mode: OperationMode,
    settings: ProcessSettings,
}

/// The user-editable part of a recipe.
#[derive(Debug, Clone, PartialEq)]
pub struct RecipeDraft {
    pub name: String,
    pub material: String,
    pub thickness: Length,
    pub machine: String,
    pub notes: String,
    pub mode: OperationMode,
    pub settings: ProcessSettings,
}

impl MaterialRecipe {
    /// A recipe from parts (new or loaded). Thickness must be positive; revision starts at 1.
    pub fn new(id: RecipeId, revision: u32, draft: RecipeDraft) -> Result<Self, ProcessError> {
        if !(draft.thickness.mm() > 0.0) {
            return Err(ProcessError::NotPositive { field: "thickness" });
        }
        if revision == 0 {
            return Err(ProcessError::OutOfRange { field: "revision" });
        }
        Ok(MaterialRecipe {
            id,
            name: draft.name,
            material: draft.material,
            thickness: draft.thickness,
            machine: draft.machine,
            revision,
            notes: draft.notes,
            mode: draft.mode,
            settings: draft.settings,
        })
    }

    pub fn id(&self) -> RecipeId {
        self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn material(&self) -> &str {
        &self.material
    }
    pub fn thickness(&self) -> Length {
        self.thickness
    }
    pub fn machine(&self) -> &str {
        &self.machine
    }
    pub fn revision(&self) -> u32 {
        self.revision
    }
    pub fn notes(&self) -> &str {
        &self.notes
    }
    pub fn mode(&self) -> OperationMode {
        self.mode
    }
    pub fn settings(&self) -> ProcessSettings {
        self.settings
    }

    pub fn draft(&self) -> RecipeDraft {
        RecipeDraft {
            name: self.name.clone(),
            material: self.material.clone(),
            thickness: self.thickness,
            machine: self.machine.clone(),
            notes: self.notes.clone(),
            mode: self.mode,
            settings: self.settings,
        }
    }
}
