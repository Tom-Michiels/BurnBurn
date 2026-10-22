use burnburn_types::{GeometryId, OperationId};

use super::{AdvancedOptions, OperationMode, ProcessError, ProcessSettings, RecipeRef};

/// One process operation: a mode with settings, applied to design geometry by reference.
///
/// The operation never copies geometry. Several operations may reference the same `GeometryId`
/// (mark first, cut later); editing the source changes every operation that uses it (C02).
#[derive(Debug, Clone, PartialEq)]
pub struct Operation {
    id: OperationId,
    name: String,
    mode: OperationMode,
    settings: ProcessSettings,
    /// Ordered, without duplicates.
    targets: Vec<GeometryId>,
    output_enabled: bool,
    recipe: Option<RecipeRef>,
    advanced: AdvancedOptions,
}

/// The editable fields of an operation (everything except identity and targets).
#[derive(Debug, Clone, PartialEq)]
pub struct OperationDraft {
    pub name: String,
    pub mode: OperationMode,
    pub settings: ProcessSettings,
    pub output_enabled: bool,
}

impl Operation {
    /// An operation from parts. Rejects duplicate targets; existence of targets is checked by
    /// the process layer against the document.
    pub fn new(
        id: OperationId,
        draft: OperationDraft,
        targets: Vec<GeometryId>,
        recipe: Option<RecipeRef>,
    ) -> Result<Self, ProcessError> {
        for (i, g) in targets.iter().enumerate() {
            if targets[..i].contains(g) {
                return Err(ProcessError::DuplicateTarget { operation: id, geometry: *g });
            }
        }
        Ok(Operation {
            id,
            name: draft.name,
            mode: draft.mode,
            settings: draft.settings,
            targets,
            output_enabled: draft.output_enabled,
            recipe,
            advanced: AdvancedOptions::default(),
        })
    }

    /// The same operation with other advanced options (IM25). Validated; capability checks
    /// happen at compile time against the machine profile.
    pub fn with_advanced(&self, advanced: AdvancedOptions) -> Result<Operation, ProcessError> {
        advanced.validate()?;
        Ok(Operation { advanced, ..self.clone() })
    }

    pub fn advanced(&self) -> &AdvancedOptions {
        &self.advanced
    }

    /// The tool layer (T1/T2) this operation is, if any.
    pub fn tool(&self) -> Option<super::ToolLayer> {
        self.advanced.tool
    }

    pub fn id(&self) -> OperationId {
        self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn mode(&self) -> OperationMode {
        self.mode
    }
    pub fn settings(&self) -> ProcessSettings {
        self.settings
    }
    pub fn targets(&self) -> &[GeometryId] {
        &self.targets
    }
    /// Disabled operations stay in the document but produce no output.
    pub fn output_enabled(&self) -> bool {
        self.output_enabled && self.advanced.tool.is_none()
    }
    pub fn recipe(&self) -> Option<RecipeRef> {
        self.recipe
    }

    pub fn draft(&self) -> OperationDraft {
        OperationDraft {
            name: self.name.clone(),
            mode: self.mode,
            settings: self.settings,
            output_enabled: self.output_enabled(),
        }
    }

    pub(crate) fn with_draft(&self, draft: OperationDraft) -> Operation {
        Operation {
            name: draft.name,
            mode: draft.mode,
            settings: draft.settings,
            output_enabled: draft.output_enabled,
            ..self.clone()
        }
    }

    pub(crate) fn with_targets(&self, targets: Vec<GeometryId>) -> Operation {
        Operation { targets, ..self.clone() }
    }

    pub(crate) fn with_recipe(&self, mode: OperationMode, settings: ProcessSettings, recipe: RecipeRef) -> Operation {
        Operation { mode, settings, recipe: Some(recipe), ..self.clone() }
    }
}
