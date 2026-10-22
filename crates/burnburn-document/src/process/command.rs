use std::sync::Arc;

use burnburn_types::{GeometryId, IdAllocator, OperationId};

use super::{
    AdvancedOptions, CutPriority, PathLock, MaterialRecipe, Operation, OperationDraft, OptimizationSettings, OrderConstraint,
    ProcessError, ProcessLayer, RecipeDraft, RecipeId, RecipeRef, ToolLayer,
};
use crate::command::CommandError;

/// Process-layer edits. Each one is a single document transaction and history entry.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ProcessCommand {
    /// Appends an operation (at the end of the default order) that references `targets`.
    AddOperation {
        draft: OperationDraft,
        targets: Vec<GeometryId>,
    },
    /// Replaces name, mode, settings and output flag; targets and identity stay.
    UpdateOperation {
        operation: OperationId,
        draft: OperationDraft,
    },
    /// Removes the operation and every order constraint that mentions it. Geometry is untouched.
    RemoveOperation {
        operation: OperationId,
    },
    /// Adds references (appended, in order). A geometry already referenced is an error.
    AssignGeometry {
        operation: OperationId,
        targets: Vec<GeometryId>,
    },
    /// Removes references; geometry itself is untouched.
    UnassignGeometry {
        operation: OperationId,
        targets: Vec<GeometryId>,
    },
    /// `before` must finish before `after` starts. A constraint that closes a cycle is rejected
    /// with the operations on the cycle (C06).
    AddOrderConstraint {
        before: OperationId,
        after: OperationId,
    },
    RemoveOrderConstraint {
        before: OperationId,
        after: OperationId,
    },
    AddRecipe {
        draft: RecipeDraft,
    },
    /// New content for a recipe; its revision increases. Operations that applied an older
    /// revision keep their copied settings.
    UpdateRecipe {
        recipe: RecipeId,
        draft: RecipeDraft,
    },
    /// Copies the recipe's mode and settings into the operation and records the revision used.
    ApplyRecipe {
        recipe: RecipeId,
        operation: OperationId,
    },
    /// Replaces the advanced options (tabs, leads, sub-operations, …) of an operation (IM25).
    SetAdvanced {
        operation: OperationId,
        advanced: AdvancedOptions,
    },
    /// Replaces the document's optimization settings (order hierarchy, inner-first, overlap
    /// removal, …). Every operation's plan can change, so all of them are reported as changed.
    SetOptimization {
        settings: OptimizationSettings,
    },
    /// Sets the cut priority of a shape (lower is cut first when ordering by priority). The
    /// default priority removes the entry.
    SetPriority {
        geometry: GeometryId,
        priority: CutPriority,
    },
    /// Locks (or frees) the start point and the direction of these shapes, in one undo step. A
    /// lock that locks nothing removes the entries.
    SetPathLock {
        geometries: Vec<GeometryId>,
        lock: PathLock,
    },
    /// Replaces the project notes (File > Show Notes). Touches no operation.
    SetNotes {
        text: String,
    },
    /// Appends a tool layer (T1/T2: guides, registration; never output) that references `targets`.
    AddToolLayer {
        tool: ToolLayer,
        targets: Vec<GeometryId>,
    },
}

impl ProcessCommand {
    /// Longest notes text, in characters.
    pub const MAX_NOTES_CHARS: usize = 20_000;
}

/// Localisation key for undo/redo of a process command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ProcessLabel {
    AddOperation,
    UpdateOperation,
    RemoveOperation,
    AssignGeometry,
    UnassignGeometry,
    AddOrderConstraint,
    RemoveOrderConstraint,
    AddRecipe,
    UpdateRecipe,
    ApplyRecipe,
    SetAdvanced,
    SetOptimization,
    SetPriority,
    SetPathLock,
    SetNotes,
    AddToolLayer,
}

impl ProcessCommand {
    pub fn label(&self) -> ProcessLabel {
        match self {
            ProcessCommand::AddOperation { .. } => ProcessLabel::AddOperation,
            ProcessCommand::UpdateOperation { .. } => ProcessLabel::UpdateOperation,
            ProcessCommand::RemoveOperation { .. } => ProcessLabel::RemoveOperation,
            ProcessCommand::AssignGeometry { .. } => ProcessLabel::AssignGeometry,
            ProcessCommand::UnassignGeometry { .. } => ProcessLabel::UnassignGeometry,
            ProcessCommand::AddOrderConstraint { .. } => ProcessLabel::AddOrderConstraint,
            ProcessCommand::RemoveOrderConstraint { .. } => ProcessLabel::RemoveOrderConstraint,
            ProcessCommand::AddRecipe { .. } => ProcessLabel::AddRecipe,
            ProcessCommand::UpdateRecipe { .. } => ProcessLabel::UpdateRecipe,
            ProcessCommand::ApplyRecipe { .. } => ProcessLabel::ApplyRecipe,
            ProcessCommand::SetAdvanced { .. } => ProcessLabel::SetAdvanced,
            ProcessCommand::SetOptimization { .. } => ProcessLabel::SetOptimization,
            ProcessCommand::SetPriority { .. } => ProcessLabel::SetPriority,
            ProcessCommand::SetPathLock { .. } => ProcessLabel::SetPathLock,
            ProcessCommand::SetNotes { .. } => ProcessLabel::SetNotes,
            ProcessCommand::AddToolLayer { .. } => ProcessLabel::AddToolLayer,
        }
    }
}

/// What a process transaction may touch. Built by the session's transaction.
pub(crate) struct ProcessTxn<'a> {
    pub layer: &'a mut ProcessLayer,
    pub geometry_exists: &'a dyn Fn(GeometryId) -> bool,
    pub ids: &'a mut IdAllocator,
    pub operations: &'a mut Vec<OperationId>,
    pub recipes: &'a mut Vec<RecipeId>,
}

pub(crate) fn apply(cmd: ProcessCommand, t: ProcessTxn<'_>) -> Result<(), CommandError> {
    let err = CommandError::Process;
    match cmd {
        ProcessCommand::AddOperation { draft, targets } => {
            check_geometry(&t, &targets)?;
            let id: OperationId = t.ids.allocate().map_err(|_| CommandError::IdsExhausted)?;
            let op = Operation::new(id, draft, targets, None).map_err(err)?;
            t.layer.operations.push(Arc::new(op));
            t.operations.push(id);
        }
        ProcessCommand::UpdateOperation { operation, draft } => {
            let (i, op) = find(t.layer, operation)?;
            if op.tool().is_some() && draft.output_enabled {
                return Err(err(ProcessError::ToolLayerNoOutput(operation)));
            }
            t.layer.operations[i] = Arc::new(op.with_draft(draft));
            t.operations.push(operation);
        }
        ProcessCommand::RemoveOperation { operation } => {
            let (i, _) = find(t.layer, operation)?;
            t.layer.operations.remove(i);
            t.layer.constraints.retain(|c| c.before != operation && c.after != operation);
            t.operations.push(operation);
        }
        ProcessCommand::AssignGeometry { operation, targets } => {
            check_geometry(&t, &targets)?;
            let (i, op) = find(t.layer, operation)?;
            let mut all = op.targets().to_vec();
            all.extend(targets);
            // `Operation::new` rejects a geometry that is already referenced.
            let checked = Operation::new(operation, op.draft(), all, op.recipe()).map_err(err)?;
            t.layer.operations[i] = Arc::new(op.with_targets(checked.targets().to_vec()));
            t.operations.push(operation);
        }
        ProcessCommand::UnassignGeometry { operation, targets } => {
            let (i, op) = find(t.layer, operation)?;
            if let Some(g) = targets.iter().find(|g| !op.targets().contains(g)) {
                return Err(err(ProcessError::UnknownGeometry(*g)));
            }
            let kept = op.targets().iter().copied().filter(|g| !targets.contains(g)).collect();
            t.layer.operations[i] = Arc::new(op.with_targets(kept));
            t.operations.push(operation);
        }
        ProcessCommand::AddOrderConstraint { before, after } => {
            let c = OrderConstraint { before, after };
            t.layer.check_new_constraint(c).map_err(err)?;
            t.layer.constraints.push(c);
            t.operations.extend([before, after]);
        }
        ProcessCommand::RemoveOrderConstraint { before, after } => {
            let c = OrderConstraint { before, after };
            let i = t.layer.constraints.iter().position(|x| *x == c);
            let i = i.ok_or(err(ProcessError::UnknownOperation(after)))?;
            t.layer.constraints.remove(i);
            t.operations.extend([before, after]);
        }
        ProcessCommand::AddRecipe { draft } => {
            let id: RecipeId = t.ids.allocate().map_err(|_| CommandError::IdsExhausted)?;
            let recipe = MaterialRecipe::new(id, 1, draft).map_err(err)?;
            t.layer.recipes.insert(id, Arc::new(recipe));
            t.recipes.push(id);
        }
        ProcessCommand::UpdateRecipe { recipe, draft } => {
            let old = t.layer.recipe(recipe).ok_or(err(ProcessError::UnknownRecipe(recipe)))?;
            let revision = old.revision().checked_add(1).ok_or(err(ProcessError::OutOfRange { field: "revision" }))?;
            let new = MaterialRecipe::new(recipe, revision, draft).map_err(err)?;
            t.layer.recipes.insert(recipe, Arc::new(new));
            t.recipes.push(recipe);
        }
        ProcessCommand::ApplyRecipe { recipe, operation } => {
            let r = t.layer.recipe(recipe).ok_or(err(ProcessError::UnknownRecipe(recipe)))?;
            let (mode, settings, rev) = (r.mode(), r.settings(), RecipeRef { recipe, revision: r.revision() });
            let (i, op) = find(t.layer, operation)?;
            t.layer.operations[i] = Arc::new(op.with_recipe(mode, settings, rev));
            t.operations.push(operation);
        }
        ProcessCommand::SetAdvanced { operation, advanced } => {
            let (i, op) = find(t.layer, operation)?;
            t.layer.operations[i] = Arc::new(op.with_advanced(advanced).map_err(err)?);
            t.operations.push(operation);
        }
        ProcessCommand::SetOptimization { settings } => {
            t.layer.optimization = settings;
            t.operations.extend(t.layer.operations().map(|o| o.id()));
        }
        ProcessCommand::AddToolLayer { tool, targets } => {
            check_geometry(&t, &targets)?;
            let id: OperationId = t.ids.allocate().map_err(|_| CommandError::IdsExhausted)?;
            let op = Operation::new(id, tool_draft(tool).map_err(err)?, targets, None)
                .and_then(|op| op.with_advanced(AdvancedOptions { tool: Some(tool), ..AdvancedOptions::default() }))
                .map_err(err)?;
            t.layer.operations.push(Arc::new(op));
            t.operations.push(id);
        }
        ProcessCommand::SetPriority { geometry, priority } => {
            check_geometry(&t, &[geometry])?;
            if priority == CutPriority::default() {
                t.layer.priorities.remove(&geometry);
            } else {
                t.layer.priorities.insert(geometry, priority);
            }
            t.operations.extend(t.layer.operations_using(geometry));
        }
        ProcessCommand::SetPathLock { geometries, lock } => {
            check_geometry(&t, &geometries)?;
            for geometry in geometries {
                if lock.is_none() {
                    t.layer.path_locks.remove(&geometry);
                } else {
                    t.layer.path_locks.insert(geometry, lock);
                }
                t.operations.extend(t.layer.operations_using(geometry));
            }
        }
        ProcessCommand::SetNotes { text } => {
            if text.chars().count() > ProcessCommand::MAX_NOTES_CHARS {
                return Err(err(ProcessError::OutOfRange { field: "notes" }));
            }
            t.layer.notes = text;
        }
    }
    Ok(())
}

fn check_geometry(t: &ProcessTxn<'_>, targets: &[GeometryId]) -> Result<(), CommandError> {
    match targets.iter().find(|g| !(t.geometry_exists)(**g)) {
        Some(g) => Err(CommandError::Process(ProcessError::UnknownGeometry(*g))),
        None => Ok(()),
    }
}

fn find(layer: &ProcessLayer, id: OperationId) -> Result<(usize, Arc<Operation>), CommandError> {
    let i = layer.operations.iter().position(|o| o.id() == id);
    let i = i.ok_or(CommandError::Process(ProcessError::UnknownOperation(id)))?;
    Ok((i, Arc::clone(&layer.operations[i])))
}

/// The draft of a tool layer: named after it, a plain line layer without power, never output.
fn tool_draft(tool: ToolLayer) -> Result<OperationDraft, ProcessError> {
    use super::{Kerf, OperationMode, Passes, PowerPercent, PowerRange, ProcessSettings, Speed};
    let off = PowerPercent::new(0.0)?;
    Ok(OperationDraft {
        name: tool.name().to_owned(),
        mode: OperationMode::Line { kerf: Kerf::NONE },
        settings: ProcessSettings {
            speed: Speed::mm_per_s(100.0)?,
            power: PowerRange::new(off, off)?,
            passes: Passes::new(1)?,
            air_assist: false,
        },
        output_enabled: false,
    })
}
