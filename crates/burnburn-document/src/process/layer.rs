use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use burnburn_types::{GeometryId, OperationId, StableId};

use super::{CutPriority, MaterialRecipe, Operation, OptimizationSettings, ProcessError, RecipeId};

/// A hard ordering requirement: `before` must be completely executed before `after` starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OrderConstraint {
    pub before: OperationId,
    pub after: OperationId,
}

/// The process layer of a document: operations (in their default execution order), hard
/// order constraints and the material recipe library. Separate from the design layer: it only
/// refers to geometry by `GeometryId`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProcessLayer {
    pub(crate) operations: Vec<Arc<Operation>>,
    pub(crate) constraints: Vec<OrderConstraint>,
    pub(crate) recipes: BTreeMap<RecipeId, Arc<MaterialRecipe>>,
    pub(crate) optimization: OptimizationSettings,
    /// Cut priority per geometry; absent = the default priority 0.
    pub(crate) priorities: BTreeMap<GeometryId, CutPriority>,
    /// What the planner may not change about a shape's start and direction; absent = nothing.
    pub(crate) path_locks: BTreeMap<GeometryId, super::PathLock>,
    /// Free-text project notes (File > Show Notes); empty = none.
    pub(crate) notes: String,
}

impl ProcessLayer {
    /// A layer from stored parts. Checks internal consistency (unique ids, known operations in
    /// constraints, acyclic order); references to geometry are checked by the snapshot.
    pub fn from_parts(
        operations: Vec<Operation>,
        constraints: Vec<OrderConstraint>,
        recipes: Vec<MaterialRecipe>,
    ) -> Result<Self, ProcessError> {
        let mut layer = ProcessLayer::default();
        let mut seen = BTreeSet::new();
        for op in operations {
            if !seen.insert(op.id().raw()) {
                return Err(ProcessError::InvalidId { raw: op.id().raw() });
            }
            layer.operations.push(Arc::new(op));
        }
        for r in recipes {
            if !seen.insert(r.id().raw()) {
                return Err(ProcessError::InvalidId { raw: r.id().raw() });
            }
            layer.recipes.insert(r.id(), Arc::new(r));
        }
        for c in constraints {
            layer.check_new_constraint(c)?;
            layer.constraints.push(c);
        }
        Ok(layer)
    }

    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
            && self.constraints.is_empty()
            && self.recipes.is_empty()
            && self.optimization.is_default()
            && self.priorities.is_empty()
            && self.path_locks.is_empty()
            && self.notes.is_empty()
    }

    /// Project notes: free text the user keeps with the project (empty = none).
    pub fn notes(&self) -> &str {
        &self.notes
    }

    /// The same layer with stored notes (open/recovery); too long notes are cut at the limit.
    pub fn with_notes(mut self, notes: String) -> Self {
        self.notes = notes.chars().take(super::ProcessCommand::MAX_NOTES_CHARS).collect();
        self
    }

    /// Planner options of the document (order hierarchy, inner-first, overlap removal, …).
    pub fn optimization(&self) -> &OptimizationSettings {
        &self.optimization
    }

    /// Cut priority of `geometry` (default 0; lower is cut first when ordering by priority).
    pub fn priority(&self, geometry: GeometryId) -> CutPriority {
        self.priorities.get(&geometry).copied().unwrap_or_default()
    }

    /// Geometries with a non-default priority, in id order.
    /// What may not change about how `geometry` is cut (default: nothing locked).
    pub fn path_lock(&self, geometry: GeometryId) -> super::PathLock {
        self.path_locks.get(&geometry).copied().unwrap_or_default()
    }

    /// The shapes with something locked, in id order.
    pub fn path_locks(&self) -> impl Iterator<Item = (GeometryId, super::PathLock)> + '_ {
        self.path_locks.iter().map(|(g, l)| (*g, *l))
    }

    /// The same layer with stored locks; a lock that locks nothing is not stored.
    pub fn with_path_locks(mut self, locks: impl IntoIterator<Item = (GeometryId, super::PathLock)>) -> Self {
        self.path_locks = locks.into_iter().filter(|(_, l)| !l.is_none()).collect();
        self
    }

    pub fn priorities(&self) -> impl Iterator<Item = (GeometryId, CutPriority)> + '_ {
        self.priorities.iter().map(|(g, p)| (*g, *p))
    }

    /// The same layer with stored optimization settings (open/recovery). Priorities must name
    /// geometry that exists; the snapshot checks that.
    pub fn with_optimization(mut self, settings: OptimizationSettings) -> Self {
        self.optimization = settings;
        self
    }

    /// The same layer with stored priorities; the default priority is not stored.
    pub fn with_priorities(mut self, priorities: impl IntoIterator<Item = (GeometryId, CutPriority)>) -> Self {
        self.priorities = priorities.into_iter().filter(|(_, p)| *p != CutPriority::default()).collect();
        self
    }

    /// Copies the optimization settings and the priorities of `other` for the geometry in
    /// `keep` (a plate view of the same document).
    pub(crate) fn inherit_settings(mut self, other: &ProcessLayer, keep: impl Fn(GeometryId) -> bool) -> Self {
        self.optimization = other.optimization.clone();
        self.priorities = other.priorities.iter().filter(|(g, _)| keep(**g)).map(|(g, p)| (*g, *p)).collect();
        self.path_locks = other.path_locks.iter().filter(|(g, _)| keep(**g)).map(|(g, l)| (*g, *l)).collect();
        self
    }

    /// Operations in their default (list) order.
    pub fn operations(&self) -> impl ExactSizeIterator<Item = &Operation> {
        self.operations.iter().map(|o| o.as_ref())
    }

    pub fn operation(&self, id: OperationId) -> Option<&Operation> {
        self.operations.iter().find(|o| o.id() == id).map(|o| o.as_ref())
    }

    pub fn constraints(&self) -> &[OrderConstraint] {
        &self.constraints
    }

    pub fn recipes(&self) -> impl ExactSizeIterator<Item = &MaterialRecipe> {
        self.recipes.values().map(|r| r.as_ref())
    }

    pub fn recipe(&self, id: RecipeId) -> Option<&MaterialRecipe> {
        self.recipes.get(&id).map(|r| r.as_ref())
    }

    /// Operations that reference `geometry`, in list order (C02: multiply linked geometry is inspectable).
    pub fn operations_using(&self, geometry: GeometryId) -> Vec<OperationId> {
        self.operations.iter().filter(|o| o.targets().contains(&geometry)).map(|o| o.id()).collect()
    }

    /// Drops `geometry` from every operation's targets; returns the operations that changed.
    pub(crate) fn remove_geometry(&mut self, geometry: GeometryId) -> Vec<OperationId> {
        self.priorities.remove(&geometry);
        self.path_locks.remove(&geometry);
        let mut changed = Vec::new();
        for op in &mut self.operations {
            if op.targets().contains(&geometry) {
                let kept = op.targets().iter().copied().filter(|g| *g != geometry).collect();
                *op = Arc::new(op.with_targets(kept));
                changed.push(op.id());
            }
        }
        changed
    }

    /// Every raw id used by this layer (operations and recipes).
    pub(crate) fn ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.operations.iter().map(|o| o.id().raw()).chain(self.recipes.keys().map(|r| r.raw()))
    }

    /// The execution order: list order, except that every hard constraint is honoured
    /// (stable topological sort, so free operations keep their list position).
    pub fn execution_order(&self) -> Result<Vec<OperationId>, ProcessError> {
        topological_order(&self.operations.iter().map(|o| o.id()).collect::<Vec<_>>(), &self.constraints)
    }

    /// Rejects a constraint that is unknown, reflexive, duplicated or would close a cycle.
    pub(crate) fn check_new_constraint(&self, c: OrderConstraint) -> Result<(), ProcessError> {
        for id in [c.before, c.after] {
            if self.operation(id).is_none() {
                return Err(ProcessError::UnknownOperation(id));
            }
        }
        if c.before == c.after {
            return Err(ProcessError::SelfConstraint(c.before));
        }
        if self.constraints.contains(&c) {
            return Err(ProcessError::DuplicateConstraint { before: c.before, after: c.after });
        }
        let mut with = self.constraints.clone();
        with.push(c);
        topological_order(&self.operations.iter().map(|o| o.id()).collect::<Vec<_>>(), &with).map(|_| ())
    }
}

/// Kahn's algorithm, always taking the earliest ready operation in `order`. On a cycle, the
/// error names the operations on one cycle, in list order.
fn topological_order(order: &[OperationId], constraints: &[OrderConstraint]) -> Result<Vec<OperationId>, ProcessError> {
    let mut remaining: Vec<OperationId> = order.to_vec();
    let mut out = Vec::with_capacity(order.len());
    while !remaining.is_empty() {
        let ready = remaining
            .iter()
            .position(|op| !constraints.iter().any(|c| c.after == *op && remaining.contains(&c.before)));
        match ready {
            Some(i) => out.push(remaining.remove(i)),
            None => return Err(ProcessError::OrderCycle { operations: cycle_in(&remaining, constraints) }),
        }
    }
    Ok(out)
}

/// Operations on one cycle among `blocked` (every one of them has a blocked predecessor).
fn cycle_in(blocked: &[OperationId], constraints: &[OrderConstraint]) -> Vec<OperationId> {
    // Walk predecessors from the first blocked operation until an operation repeats.
    let mut path: Vec<OperationId> = Vec::new();
    let mut current = blocked.first().copied();
    while let Some(op) = current {
        if let Some(start) = path.iter().position(|p| *p == op) {
            let mut cycle = path.split_off(start);
            cycle.sort_by_key(|id| blocked.iter().position(|b| b == id));
            return cycle;
        }
        path.push(op);
        current = constraints.iter().find(|c| c.after == op && blocked.contains(&c.before)).map(|c| c.before);
    }
    path
}
