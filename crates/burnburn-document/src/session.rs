use std::collections::BTreeMap;
use std::sync::Arc;

use burnburn_geometry::{Affine, Path, Point};
use burnburn_types::{DocumentId, GeometryId, IdAllocator, RevisionId};

use crate::arrange::ArrangeLayer;
use crate::command::{
    Anchor, Axis, ChangeSet, CommandError, CommandOutcome, CommandPayload, CommandWarning, DocumentCommand, UndoLabel,
    ValueError,
};
use crate::engine::EngineLayer;
use crate::generator::{GeneratorEffect, generator_inputs};
use crate::object::{GeometryObject, GeometrySource, ObjectKind};
use crate::process::{self, ProcessLayer, ProcessTxn};
use crate::sketch::{self, SketchEffect, SketchLayer, SketchTxn};
use crate::snapshot::{DocumentSnapshot, Objects};
use crate::structure::{
    self, EvalMode, RefHolder, RefStatus, StructureEffect, StructureLayer, StructureTxn, chain_inputs, evaluate_chain,
    evaluate_instance, instance_inputs,
};
use crate::text::{self, TextEffect, TextLayer, TextTxn};
use crate::trace::{TraceEffect, TraceId, TraceOutput};
use crate::{DerivedOwner, DerivedRecord, DocumentPatch, EvalStatus, Evaluator, OwnerSolution};

/// One undoable step: the content before and after, plus what changed.
#[derive(Debug, Clone)]
struct HistoryEntry {
    before: DocumentSnapshot,
    after: DocumentSnapshot,
    changes: ChangeSet,
    label: UndoLabel,
    /// Selection before the command and the one the user ended with (P02).
    selection_before: Vec<GeometryId>,
    selection_after: Option<Vec<GeometryId>>,
    /// Estimated memory this step retains (see [`step_bytes`]).
    bytes: usize,
}

/// Rough size of a stored object: its struct, name, groups and path segments.
fn object_bytes(o: &GeometryObject) -> usize {
    let segments: usize = o.path().subpaths().iter().map(|sp| sp.segments().len()).sum();
    std::mem::size_of::<GeometryObject>()
        + o.name().len()
        + o.groups().len() * 8
        + std::mem::size_of_val(o.path().subpaths())
        + segments * std::mem::size_of::<burnburn_geometry::Segment>()
}

/// Estimated memory one step adds: the object-map copy it made plus the object versions it
/// created or modified (older versions were counted by the step that made them).
fn step_bytes(after: &DocumentSnapshot, changes: &ChangeSet) -> usize {
    const MAP_ENTRY: usize = 48;
    let versions: usize =
        changes.created.iter().chain(&changes.modified).filter_map(|id| after.object(*id)).map(object_bytes).sum();
    after.len() * MAP_ENTRY + versions
}

/// Memory budget of the undo history, in estimated bytes retained by the steps (X04).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryBudget {
    pub max_bytes: usize,
}

impl HistoryBudget {
    /// 512 MiB: far above ordinary documents, a guard for 100k-object designs edited all day.
    pub const DEFAULT: HistoryBudget = HistoryBudget { max_bytes: 512 * 1024 * 1024 };
}

/// The mutable owner of one open document: current snapshot plus undo/redo history.
/// Lives on the UI thread; hands out immutable snapshots to everything else.
#[derive(Debug)]
pub struct DocumentSession {
    id: DocumentId,
    current: Arc<DocumentSnapshot>,
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    /// Content last marked as saved; compared by pointer, since undo restores the same `Arc`s.
    saved: Option<Saved>,
    budget: HistoryBudget,
    /// The selection the host last reported, and whether it belongs to the newest undo entry.
    selection: Vec<GeometryId>,
    selection_tracks_top: bool,
}

type Saved = (
    Objects,
    Arc<ProcessLayer>,
    Arc<SketchLayer>,
    Arc<StructureLayer>,
    Arc<TextLayer>,
    Arc<EngineLayer>,
    Arc<ArrangeLayer>,
);

fn content_of(s: &DocumentSnapshot) -> Saved {
    (
        Arc::clone(&s.objects),
        Arc::clone(&s.process),
        Arc::clone(&s.sketches),
        Arc::clone(&s.structure),
        Arc::clone(&s.texts),
        Arc::clone(&s.engines),
        Arc::clone(&s.arrange),
    )
}

/// Working copy of one transaction. Nothing reaches the session unless it completes.
struct Txn {
    objects: BTreeMap<GeometryId, Arc<GeometryObject>>,
    process: ProcessLayer,
    sketches: SketchLayer,
    structure: StructureLayer,
    texts: TextLayer,
    engines: EngineLayer,
    arrange: ArrangeLayer,
    ids: IdAllocator,
    changes: ChangeSet,
    warnings: Vec<CommandWarning>,
}

impl DocumentSession {
    pub fn new(id: DocumentId) -> Self {
        Self::from_snapshot(id, DocumentSnapshot::empty())
    }

    /// A session over a loaded or recovered snapshot. History starts empty.
    pub fn from_snapshot(id: DocumentId, snapshot: DocumentSnapshot) -> Self {
        let saved = Some(content_of(&snapshot));
        DocumentSession {
            id,
            current: Arc::new(snapshot),
            undo: Vec::new(),
            redo: Vec::new(),
            saved,
            budget: HistoryBudget::DEFAULT,
            selection: Vec::new(),
            selection_tracks_top: false,
        }
    }

    pub fn set_history_budget(&mut self, budget: HistoryBudget) {
        self.budget = budget;
    }

    pub fn history_budget(&self) -> HistoryBudget {
        self.budget
    }

    /// Estimated memory retained by the undo and redo steps.
    pub fn history_bytes(&self) -> usize {
        self.undo.iter().chain(&self.redo).map(|e| e.bytes).sum()
    }

    /// Forgets the oldest undo steps while the history exceeds its budget; the newest step is
    /// always kept. Returns how many were dropped.
    fn trim_history(&mut self) -> usize {
        let mut dropped = 0;
        while self.undo.len() > 1 && self.history_bytes() > self.budget.max_bytes {
            self.undo.remove(0);
            dropped += 1;
        }
        dropped
    }

    /// Tells the session what is selected now. A command applied later records this as the
    /// selection to come back to on undo; while the newest history entry is the live state, the
    /// value also becomes the selection that redo restores. Plain ids: no native or editor types.
    pub fn set_selection(&mut self, ids: &[GeometryId]) {
        if self.selection != ids {
            self.selection = ids.to_vec();
        }
        if self.selection_tracks_top
            && let Some(entry) = self.undo.last_mut()
        {
            entry.selection_after = Some(self.selection.clone());
        }
    }

    pub fn id(&self) -> DocumentId {
        self.id
    }

    pub fn revision(&self) -> RevisionId {
        self.current.revision()
    }

    pub fn snapshot(&self) -> Arc<DocumentSnapshot> {
        Arc::clone(&self.current)
    }

    /// Applies one command as one transaction and one history entry.
    pub fn apply(&mut self, cmd: DocumentCommand) -> Result<CommandOutcome, CommandError> {
        self.apply_with(cmd, &[])
    }

    /// Applies `cmd`, then re-evaluates every derived-geometry owner whose definition the
    /// command changed, inside the same transaction: one history entry, one new revision, so
    /// undo restores definition and derived outputs together (contract §11). A failed
    /// evaluation still commits the definition; the outputs stay the last valid ones and the
    /// owner's record says why. Owners without a matching evaluator are marked `Pending`.
    pub fn apply_with(
        &mut self,
        cmd: DocumentCommand,
        evaluators: &[&dyn Evaluator],
    ) -> Result<CommandOutcome, CommandError> {
        if cmd.document != self.id {
            return Err(CommandError::WrongDocument { expected: cmd.document, actual: self.id });
        }
        self.check_expected(cmd.expected)?;
        let previous = self.revision();
        let revision = previous.next().ok_or(CommandError::RevisionsExhausted)?;
        let mut txn = Txn {
            objects: (*self.current.objects).clone(),
            process: (*self.current.process).clone(),
            sketches: (*self.current.sketches).clone(),
            structure: (*self.current.structure).clone(),
            texts: (*self.current.texts).clone(),
            engines: (*self.current.engines).clone(),
            arrange: (*self.current.arrange).clone(),
            ids: IdAllocator::resume_at(self.current.next_id),
            changes: ChangeSet::default(),
            warnings: Vec::new(),
        };
        let label = cmd.payload.undo_label();
        // On error the working copy is dropped: snapshot, revision, ids and history stay untouched.
        txn.execute(cmd.payload)?;
        {
            let objects = &txn.objects;
            txn.arrange.retain_existing(|id| objects.contains_key(&id));
        }
        txn.evaluate(revision, evaluators)?;
        let mark = (txn.changes.created.len(), txn.changes.modified.len(), txn.changes.deleted.len());
        txn.evaluate_structure()?;
        txn.evaluate_following_texts(revision, evaluators, mark)?;
        // Keep sharing the process layer when the command did not touch it.
        let process = if txn.process == *self.current.process {
            Arc::clone(&self.current.process)
        } else {
            Arc::new(txn.process)
        };
        let sketches = if txn.sketches == *self.current.sketches {
            Arc::clone(&self.current.sketches)
        } else {
            Arc::new(txn.sketches)
        };
        let structure = if txn.structure == *self.current.structure {
            Arc::clone(&self.current.structure)
        } else {
            Arc::new(txn.structure)
        };
        let texts =
            if txn.texts == *self.current.texts { Arc::clone(&self.current.texts) } else { Arc::new(txn.texts) };
        let engines = if txn.engines == *self.current.engines {
            Arc::clone(&self.current.engines)
        } else {
            Arc::new(txn.engines)
        };
        let arrange = if txn.arrange == *self.current.arrange {
            Arc::clone(&self.current.arrange)
        } else {
            Arc::new(txn.arrange)
        };
        let after = DocumentSnapshot::from_txn(
            revision,
            Arc::new(txn.objects),
            txn.ids.peek_next(),
            process,
            sketches,
            structure,
            texts,
            engines,
        )
        .with_arrange_layer(arrange);
        reference_warnings(&self.current, &after, &mut txn.warnings);
        let before = (*self.current).clone();
        self.current = Arc::new(after.clone());
        self.redo.clear();
        let bytes = step_bytes(&after, &txn.changes);
        self.undo.push(HistoryEntry {
            before,
            after,
            changes: txn.changes.clone(),
            label,
            selection_before: self.selection.clone(),
            selection_after: None,
            bytes,
        });
        self.selection_tracks_top = true;
        let dropped_steps = self.trim_history();
        if dropped_steps > 0 {
            txn.warnings.push(CommandWarning::HistoryTrimmed { dropped_steps });
        }
        Ok(CommandOutcome {
            previous,
            revision,
            changes: txn.changes,
            warnings: txn.warnings,
            undo_label: label,
            selection: None,
        })
    }

    /// Restores the content and identities before the last command, as a new revision.
    pub fn undo(&mut self, expected: RevisionId) -> Result<CommandOutcome, CommandError> {
        self.check_expected(expected)?;
        let revision = self.revision().next().ok_or(CommandError::RevisionsExhausted)?;
        let entry = self.undo.pop().ok_or(CommandError::NothingToUndo)?;
        let changes = inverse(&entry.changes);
        let mut outcome = self.restore(&entry.before, revision, changes, entry.label);
        self.selection = entry.selection_before.clone();
        self.selection_tracks_top = false;
        outcome.selection = Some(entry.selection_before.clone());
        self.redo.push(entry);
        Ok(outcome)
    }

    /// Re-applies the last undone command's content, as a new revision.
    pub fn redo(&mut self, expected: RevisionId) -> Result<CommandOutcome, CommandError> {
        self.check_expected(expected)?;
        let revision = self.revision().next().ok_or(CommandError::RevisionsExhausted)?;
        let entry = self.redo.pop().ok_or(CommandError::NothingToRedo)?;
        let mut outcome = self.restore(&entry.after, revision, entry.changes.clone(), entry.label);
        if let Some(after) = &entry.selection_after {
            self.selection = after.clone();
            outcome.selection = Some(after.clone());
        }
        self.selection_tracks_top = true;
        self.undo.push(entry);
        Ok(outcome)
    }

    fn restore(
        &mut self,
        content: &DocumentSnapshot,
        revision: RevisionId,
        changes: ChangeSet,
        label: UndoLabel,
    ) -> CommandOutcome {
        let previous = self.revision();
        // The id counter never goes back: undoing a create must not let its id be reissued.
        let next_id = content.next_id.max(self.current.next_id);
        self.current = Arc::new(
            DocumentSnapshot::from_txn(
                revision,
                Arc::clone(&content.objects),
                next_id,
                Arc::clone(&content.process),
                Arc::clone(&content.sketches),
                Arc::clone(&content.structure),
                Arc::clone(&content.texts),
                Arc::clone(&content.engines),
            )
            .with_arrange_layer(Arc::clone(&content.arrange)),
        );
        CommandOutcome { previous, revision, changes, warnings: Vec::new(), undo_label: label, selection: None }
    }

    fn check_expected(&self, expected: RevisionId) -> Result<(), CommandError> {
        let actual = self.revision();
        if expected == actual { Ok(()) } else { Err(CommandError::StaleRevision { expected, actual }) }
    }

    pub fn undo_label(&self) -> Option<UndoLabel> {
        self.undo.last().map(|e| e.label)
    }

    pub fn redo_label(&self) -> Option<UndoLabel> {
        self.redo.last().map(|e| e.label)
    }

    /// Labels of the undo stack, oldest step first (the last one is what `undo` reverts).
    pub fn undo_labels(&self) -> Vec<UndoLabel> {
        self.undo.iter().map(|e| e.label).collect()
    }

    /// Labels of the redo stack, the step `redo` applies next first.
    pub fn redo_labels(&self) -> Vec<UndoLabel> {
        self.redo.iter().rev().map(|e| e.label).collect()
    }

    /// True when the current content differs from the last content marked as saved.
    pub fn is_dirty(&self) -> bool {
        self.saved.as_ref().is_none_or(|(objects, process, sketches, structure, texts, engines, arrange)| {
            !Arc::ptr_eq(objects, &self.current.objects)
                || !Arc::ptr_eq(process, &self.current.process)
                || !Arc::ptr_eq(sketches, &self.current.sketches)
                || !Arc::ptr_eq(structure, &self.current.structure)
                || !Arc::ptr_eq(texts, &self.current.texts)
                || !Arc::ptr_eq(engines, &self.current.engines)
                || !Arc::ptr_eq(arrange, &self.current.arrange)
        })
    }

    /// Records that the snapshot at `at` was written. Only the current revision can be verified;
    /// an older `at` (a save that finished after further edits) leaves the session dirty.
    pub fn mark_saved(&mut self, at: RevisionId) {
        self.saved = (at == self.revision()).then(|| content_of(&self.current));
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

impl Txn {
    /// C-8: derived geometry is edited through its definition, never directly.
    fn refuse_derived(&self, payload: &CommandPayload) -> Result<(), CommandError> {
        let targets: Vec<GeometryId> = match payload {
            CommandPayload::Transform { targets, .. } => targets.clone(),
            CommandPayload::SetDimension { target, .. } => vec![*target],
            CommandPayload::Edit(crate::edit::GeometryCommand::EditNodes { target, .. }) => vec![*target],
            _ => Vec::new(),
        };
        for id in targets {
            if let Some(GeometrySource::Derived(key)) = self.objects.get(&id).map(|o| o.source()) {
                return Err(CommandError::DerivedObject { owner: key.owner });
            }
        }
        Ok(())
    }

    /// Runs the steps on this working copy; the first failing step drops the whole transaction.
    fn execute_compound(&mut self, compound: crate::compound::CompoundCommand) -> Result<(), CommandError> {
        use crate::compound::{CompoundError, CompoundStep};
        if compound.steps.is_empty() {
            return Err(CommandError::Compound(CompoundError::Empty));
        }
        let first_created = self.changes.created.len();
        for step in compound.steps {
            match step {
                CompoundStep::Run(CommandPayload::Compound(_)) => {
                    return Err(CommandError::Compound(CompoundError::Nested));
                }
                CompoundStep::Run(payload) => self.execute(payload)?,
                CompoundStep::AddOperationForItems { draft, items } => {
                    let created = &self.changes.created[first_created..];
                    let targets: Vec<GeometryId> = items.iter().filter_map(|i| created.get(*i).copied()).collect();
                    if items.is_empty() || targets.len() != items.len() {
                        return Err(CommandError::Compound(CompoundError::UnknownItem));
                    }
                    self.execute(CommandPayload::Process(crate::process::ProcessCommand::AddOperation {
                        draft,
                        targets,
                    }))?;
                }
                CompoundStep::AddOperationForCreated { draft } => {
                    let targets = self.changes.created[first_created..].to_vec();
                    if targets.is_empty() {
                        return Err(CommandError::Compound(CompoundError::NothingCreated));
                    }
                    self.execute(CommandPayload::Process(crate::process::ProcessCommand::AddOperation {
                        draft,
                        targets,
                    }))?;
                }
            }
        }
        Ok(())
    }

    fn execute(&mut self, payload: CommandPayload) -> Result<(), CommandError> {
        self.refuse_derived(&payload)?;
        match payload {
            CommandPayload::CreateRectangle { rect, name } => {
                let id: GeometryId = self.ids.allocate().map_err(|_| CommandError::IdsExhausted)?;
                self.warn_duplicate_name(&name, id);
                let obj =
                    GeometryObject::new(id, name, ObjectKind::Rectangle, GeometrySource::Drawn, Path::rectangle(rect));
                self.objects.insert(id, Arc::new(obj));
                self.changes.created.push(id);
                Ok(())
            }
            CommandPayload::SetDimension { target, axis, value, anchor } => {
                let field = match axis {
                    Axis::Width => "width",
                    Axis::Height => "height",
                };
                let invalid = |reason| CommandError::InvalidValue { field, reason };
                if !(value.mm() > 0.0) {
                    return Err(invalid(ValueError::NotPositive));
                }
                let bounds = self.get(target)?.bounds().ok_or(invalid(ValueError::Empty))?;
                let (min, max, current) = match axis {
                    Axis::Width => (bounds.min().x, bounds.max().x, bounds.width()),
                    Axis::Height => (bounds.min().y, bounds.max().y, bounds.height()),
                };
                if !(current > 0.0) {
                    return Err(invalid(ValueError::ZeroExtent));
                }
                let factor = value.mm() / current;
                let fixed = anchor_point(min, max, anchor);
                let (sx, sy, at) = match axis {
                    Axis::Width => (factor, 1.0, Point::new(fixed, bounds.min().y)),
                    Axis::Height => (1.0, factor, Point::new(bounds.min().x, fixed)),
                };
                let m = Affine::scale_about(at, sx, sy).map_err(|e| invalid(ValueError::Geometry(e)))?;
                self.transform(target, &m)
            }
            CommandPayload::Transform { targets, transform } => {
                // One lookup set for the whole command: a per-object `contains` on the change
                // list is quadratic for a 100k-object selection (X04).
                let mut seen: std::collections::HashSet<GeometryId> = self.changes.modified.iter().copied().collect();
                for id in targets {
                    self.transform_object(id, &transform)?;
                    if seen.insert(id) {
                        self.changes.modified.push(id);
                    }
                }
                Ok(())
            }
            CommandPayload::Delete { targets } => {
                for id in targets {
                    self.objects.remove(&id).ok_or(CommandError::UnknownGeometry(id))?;
                    self.changes.deleted.push(id);
                    // Operations never keep a dangling reference; undo restores both together.
                    self.changes.operations.extend(self.process.remove_geometry(id));
                }
                Ok(())
            }
            CommandPayload::Compound(compound) => self.execute_compound(compound),
            CommandPayload::Process(cmd) => {
                let objects = &self.objects;
                process::apply(
                    cmd,
                    ProcessTxn {
                        layer: &mut self.process,
                        geometry_exists: &|g| objects.contains_key(&g),
                        ids: &mut self.ids,
                        operations: &mut self.changes.operations,
                        recipes: &mut self.changes.recipes,
                    },
                )
            }
            CommandPayload::Sketch(cmd) => {
                let cmd = match cmd {
                    sketch::SketchCommand::CreateFrom { sketch, replace } => {
                        // The drawn objects the sketch replaces go in the same transaction.
                        for id in replace {
                            self.objects.remove(&id).ok_or(CommandError::UnknownGeometry(id))?;
                            self.changes.deleted.push(id);
                            self.changes.operations.extend(self.process.remove_geometry(id));
                        }
                        sketch::SketchCommand::Create { sketch }
                    }
                    other => other,
                };
                let effect = sketch::apply(
                    cmd,
                    SketchTxn { layer: &mut self.sketches, ids: &mut self.ids, changed: &mut self.changes.sketches },
                )
                .map_err(CommandError::Sketch)?;
                if let SketchEffect::Deleted(id) = effect {
                    self.remove_outputs(DerivedOwner::Sketch(id), &[]);
                }
                Ok(())
            }
            CommandPayload::Edit(cmd) => {
                let first_deleted = self.changes.deleted.len();
                crate::edit::apply(
                    cmd,
                    crate::edit::EditTxn {
                        objects: &mut self.objects,
                        ids: &mut self.ids,
                        changes: &mut self.changes,
                        warnings: &mut self.warnings,
                        cache: Default::default(),
                    },
                )
                .map_err(CommandError::Edit)?;
                // As with Delete: operations never keep a dangling reference.
                let process = &mut self.process;
                let orphaned: Vec<_> =
                    self.changes.deleted[first_deleted..].iter().flat_map(|id| process.remove_geometry(*id)).collect();
                self.changes.operations.extend(orphaned);
                Ok(())
            }
            CommandPayload::Structure(cmd) => {
                let effect = structure::apply(
                    cmd,
                    StructureTxn {
                        layer: &mut self.structure,
                        ids: &mut self.ids,
                        objects: &self.objects,
                        changed: &mut self.changes.structure,
                    },
                )
                .map_err(CommandError::Structure)?;
                match effect {
                    StructureEffect::None => {}
                    StructureEffect::RemoveOwner(owner) => {
                        self.remove_outputs(owner, &[]);
                        self.structure.derived.remove(&owner);
                    }
                    StructureEffect::Detach(owner) => {
                        self.detach_owner_outputs(owner);
                        self.structure.derived.remove(&owner);
                    }
                }
                Ok(())
            }
            CommandPayload::Text(cmd) => {
                let effect = text::apply(
                    cmd,
                    TextTxn {
                        layer: &mut self.texts,
                        ids: &mut self.ids,
                        changed: &mut self.changes.texts,
                        objects: &self.objects,
                    },
                )
                .map_err(CommandError::Text)?;
                match effect {
                    TextEffect::None => {}
                    TextEffect::Deleted(id) => self.remove_outputs(DerivedOwner::Text(id), &[]),
                    TextEffect::Detached(id) => self.detach_outputs(id)?,
                }
                Ok(())
            }
            CommandPayload::ApplyPatch(patch) => {
                let owner = patch.owner;
                let current = self.current_inputs(owner).ok_or(CommandError::UnknownOwner(owner))?;
                if current != patch.inputs {
                    return Err(CommandError::StalePatch { owner });
                }
                match owner {
                    DerivedOwner::Trace(_) | DerivedOwner::Generator(_) => self.apply_engine_patch(patch),
                    _ => self.apply_structure_patch(patch),
                }
            }
            CommandPayload::Trace(c) => {
                let effect = crate::trace::apply(c, &mut self.engines, &mut self.ids, &mut self.changes.traces)
                    .map_err(CommandError::Engine)?;
                match effect {
                    TraceEffect::Outputs(id, outputs) => self.write_trace_outputs(id, outputs),
                    TraceEffect::Move(id, delta) => {
                        self.move_outputs(DerivedOwner::Trace(id), &delta);
                        Ok(())
                    }
                    TraceEffect::Deleted(id) => {
                        self.remove_outputs(DerivedOwner::Trace(id), &[]);
                        Ok(())
                    }
                }
            }
            CommandPayload::Generator(c) => {
                let effect = crate::generator::apply(c, &mut self.engines, &mut self.ids, &mut self.changes.generators)
                    .map_err(CommandError::Engine)?;
                if let GeneratorEffect::Deleted(id) = effect {
                    self.remove_outputs(DerivedOwner::Generator(id), &[]);
                }
                Ok(())
            }
            CommandPayload::Shape(cmd) => crate::shape_edit::apply(
                cmd,
                crate::shape_edit::ShapeTxn {
                    objects: &mut self.objects,
                    ids: &mut self.ids,
                    changes: &mut self.changes,
                    warnings: &mut self.warnings,
                },
            )
            .map_err(CommandError::Shape),
            CommandPayload::Arrange(cmd) => crate::arrange_edit::apply(
                cmd,
                crate::arrange_edit::ArrangeTxn {
                    objects: &mut self.objects,
                    ids: &mut self.ids,
                    changes: &mut self.changes,
                    process: &mut self.process,
                    arrange: &mut self.arrange,
                },
            )
            .map_err(CommandError::Arrange),
            CommandPayload::Rename { target, name } => {
                let obj = self.get(target)?;
                self.warn_duplicate_name(&name, target);
                let renamed = obj.with_name(name);
                self.objects.insert(target, Arc::new(renamed));
                self.changes.modified.push(target);
                Ok(())
            }
        }
    }

    fn get(&self, id: GeometryId) -> Result<Arc<GeometryObject>, CommandError> {
        self.objects.get(&id).cloned().ok_or(CommandError::UnknownGeometry(id))
    }

    fn transform_object(&mut self, id: GeometryId, m: &Affine) -> Result<(), CommandError> {
        let obj = self.get(id)?;
        let moved = obj.transformed(m);
        self.objects.insert(id, Arc::new(moved));
        Ok(())
    }

    fn transform(&mut self, id: GeometryId, m: &Affine) -> Result<(), CommandError> {
        self.transform_object(id, m)?;
        if !self.changes.modified.contains(&id) {
            self.changes.modified.push(id);
        }
        Ok(())
    }

    fn warn_duplicate_name(&mut self, name: &str, own: GeometryId) {
        if name.is_empty() {
            return;
        }
        if let Some(other) = self.objects.values().find(|o| o.id() != own && o.name() == name) {
            self.warnings.push(CommandWarning::DuplicateName { name: name.to_owned(), other: other.id() });
        }
    }
}

fn anchor_point(min: f64, max: f64, anchor: Anchor) -> f64 {
    match anchor {
        Anchor::Min => min,
        Anchor::Center => 0.5 * (min + max),
        Anchor::Max => max,
    }
}

/// What undoing a change set does: creations become deletions and vice versa.
fn inverse(changes: &ChangeSet) -> ChangeSet {
    ChangeSet {
        created: changes.deleted.clone(),
        modified: changes.modified.clone(),
        deleted: changes.created.clone(),
        operations: changes.operations.clone(),
        recipes: changes.recipes.clone(),
        sketches: changes.sketches.clone(),
        groups: changes.groups.clone(),
        structure: changes.structure.clone(),
        texts: changes.texts.clone(),
        traces: changes.traces.clone(),
        generators: changes.generators.clone(),
    }
}

impl Txn {
    /// Re-evaluates the owners whose definition this transaction changed.
    fn evaluate(&mut self, revision: RevisionId, evaluators: &[&dyn Evaluator]) -> Result<(), CommandError> {
        for id in self.changes.sketches.clone() {
            let owner = DerivedOwner::Sketch(id);
            if self.sketches.sketch(id).is_none() {
                continue; // deleted in this transaction
            }
            match evaluators.iter().find(|e| e.owner_kind() == owner.kind()) {
                Some(ev) => {
                    let patch = ev.evaluate(&self.snapshot_at(revision), owner);
                    self.apply_patch(patch)?;
                }
                None => {
                    let inputs = self.sketches.derived.get(&owner).and_then(|r| r.inputs);
                    self.sketches.derived.insert(owner, DerivedRecord { inputs, status: EvalStatus::Pending });
                }
            }
        }
        let mut texts = self.changes.texts.clone();
        for id in self.texts_following(&self.changes.created, &self.changes.modified, &self.changes.deleted) {
            if !texts.contains(&id) {
                texts.push(id);
            }
        }
        for id in texts {
            self.evaluate_text(id, revision, evaluators)?;
        }
        for id in self.changes.generators.clone() {
            let owner = DerivedOwner::Generator(id);
            if self.engines.generator(id).is_none() {
                continue; // deleted in this transaction
            }
            match evaluators.iter().find(|e| e.owner_kind() == owner.kind()) {
                Some(ev) => {
                    let patch = ev.evaluate(&self.snapshot_at(revision), owner);
                    if patch.owner != owner {
                        return Err(CommandError::InvalidPatch(owner));
                    }
                    self.apply_engine_patch(patch)?;
                }
                None => {
                    let inputs = self.engines.derived.get(&owner).and_then(|r| r.inputs);
                    self.engines.derived.insert(owner, DerivedRecord { inputs, status: EvalStatus::Pending });
                }
            }
        }
        Ok(())
    }

    /// Evaluates one text again (not when it is gone): its outlines follow in this transaction.
    fn evaluate_text(
        &mut self,
        id: crate::text::TextId,
        revision: RevisionId,
        evaluators: &[&dyn Evaluator],
    ) -> Result<(), CommandError> {
        let owner = DerivedOwner::Text(id);
        if self.texts.text(id).is_none() {
            return Ok(()); // deleted in this transaction
        }
        match evaluators.iter().find(|e| e.owner_kind() == owner.kind()) {
            Some(ev) => {
                let patch = ev.evaluate(&self.snapshot_at(revision), owner);
                self.apply_text_patch(patch)?;
            }
            None => {
                let inputs = self.texts.derived.get(&owner).and_then(|r| r.inputs);
                self.texts.derived.insert(owner, DerivedRecord { inputs, status: EvalStatus::Pending });
            }
        }
        Ok(())
    }

    /// The texts bound to a guide that appears in any of the three lists (G7-editor, E08).
    fn texts_following(
        &self,
        created: &[GeometryId],
        modified: &[GeometryId],
        deleted: &[GeometryId],
    ) -> Vec<crate::text::TextId> {
        let touched = |g: GeometryId| created.contains(&g) || modified.contains(&g) || deleted.contains(&g);
        self.texts.texts().filter(|(_, t)| t.path.is_some_and(|p| touched(p.guide))).map(|(id, _)| id).collect()
    }

    /// Texts whose guide the structure evaluation (modifier chains, part instances) changed after
    /// the texts were evaluated: they follow the recomputed guide in the same transaction.
    fn evaluate_following_texts(
        &mut self,
        revision: RevisionId,
        evaluators: &[&dyn Evaluator],
        mark: (usize, usize, usize),
    ) -> Result<(), CommandError> {
        let c = &self.changes;
        let followers = self.texts_following(&c.created[mark.0..], &c.modified[mark.1..], &c.deleted[mark.2..]);
        for id in followers {
            self.evaluate_text(id, revision, evaluators)?;
        }
        Ok(())
    }

    /// Writes a trace's complete output set (trace-local, placed here) and marks it valid
    /// for its recipe key.
    fn write_trace_outputs(&mut self, id: TraceId, outputs: Vec<TraceOutput>) -> Result<(), CommandError> {
        let owner = DerivedOwner::Trace(id);
        let def = self.engines.trace(id).ok_or(CommandError::UnknownOwner(owner))?;
        let (placement, key) = (def.placement, def.key);
        let outputs = outputs
            .into_iter()
            .map(|o| crate::DerivedOutput {
                key: crate::DerivedKey { owner, local: o.local },
                kind: o.kind,
                path: o.path.transform(&placement),
                name: o.name,
            })
            .collect();
        self.upsert_outputs(owner, outputs)?;
        self.engines.derived.insert(owner, DerivedRecord { inputs: Some(key), status: EvalStatus::Valid });
        Ok(())
    }

    /// Applies a trace or generator patch. Trace outputs are trace-local and placed here;
    /// generator outputs are placed by the evaluator. On failure only the record changes.
    fn apply_engine_patch(&mut self, patch: DocumentPatch) -> Result<(), CommandError> {
        let owner = patch.owner;
        if patch.outputs.iter().any(|o| o.key.owner != owner) {
            return Err(CommandError::InvalidPatch(owner));
        }
        match owner {
            DerivedOwner::Trace(id) => {
                if patch.status == EvalStatus::Valid {
                    let outputs = patch
                        .outputs
                        .into_iter()
                        .map(|o| TraceOutput { local: o.key.local, kind: o.kind, path: o.path, name: o.name })
                        .collect();
                    return self.write_trace_outputs(id, outputs);
                }
            }
            DerivedOwner::Generator(id) => {
                if self.engines.generator(id).is_none() {
                    return Err(CommandError::UnknownOwner(owner));
                }
                if patch.status == EvalStatus::Valid {
                    self.upsert_outputs(owner, patch.outputs)?;
                }
            }
            _ => return Err(CommandError::InvalidPatch(owner)),
        }
        self.engines.derived.insert(owner, DerivedRecord { inputs: Some(patch.inputs), status: patch.status });
        Ok(())
    }

    /// Transforms every output of `owner` by `m` (a placement change of its definition).
    fn move_outputs(&mut self, owner: DerivedOwner, m: &Affine) {
        let ids: Vec<GeometryId> = self
            .objects
            .iter()
            .filter(|(_, o)| matches!(o.source(), GeometrySource::Derived(k) if k.owner == owner))
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            if let Some(obj) = self.objects.get(&id) {
                let moved = obj.with_path(obj.path().transform(m));
                self.objects.insert(id, Arc::new(moved));
                self.changes.modified.push(id);
            }
        }
    }

    /// The transaction's working state as a snapshot for an evaluator.
    fn snapshot_at(&self, revision: RevisionId) -> DocumentSnapshot {
        DocumentSnapshot::from_txn(
            revision,
            Arc::new(self.objects.clone()),
            self.ids.peek_next(),
            Arc::new(self.process.clone()),
            Arc::new(self.sketches.clone()),
            Arc::new(self.structure.clone()),
            Arc::new(self.texts.clone()),
            Arc::new(self.engines.clone()),
        )
        .with_arrange_layer(Arc::new(self.arrange.clone()))
    }

    /// Applies a text evaluator patch: outputs and missing glyphs on success; only the record on
    /// failure, so the last valid outlines stay (missing font after reopening elsewhere).
    fn apply_text_patch(&mut self, patch: DocumentPatch) -> Result<(), CommandError> {
        let owner = patch.owner;
        let DerivedOwner::Text(text_id) = owner else {
            return Err(CommandError::InvalidPatch(owner));
        };
        if self.texts.text(text_id).is_none() {
            return Err(CommandError::UnknownOwner(owner));
        }
        if patch.status == EvalStatus::Valid {
            if patch.outputs.iter().any(|o| o.key.owner != owner) {
                return Err(CommandError::InvalidPatch(owner));
            }
            match patch.solution {
                OwnerSolution::Text { missing } if !missing.is_empty() => {
                    self.texts.missing.insert(text_id, missing);
                }
                _ => {
                    self.texts.missing.remove(&text_id);
                }
            }
            self.upsert_outputs(owner, patch.outputs)?;
        }
        self.texts.derived.insert(owner, DerivedRecord { inputs: Some(patch.inputs), status: patch.status });
        Ok(())
    }

    /// Applies an evaluator patch: on success the complete output set (ids reused per key) and
    /// the new solution; on failure only the record, so the last valid outputs stay.
    fn apply_patch(&mut self, patch: DocumentPatch) -> Result<(), CommandError> {
        let owner = patch.owner;
        let DerivedOwner::Sketch(sketch_id) = owner else {
            return Err(CommandError::InvalidPatch(owner));
        };
        if self.sketches.sketch(sketch_id).is_none() {
            return Err(CommandError::UnknownOwner(owner));
        }
        if patch.status == EvalStatus::Valid {
            if patch.outputs.iter().any(|o| o.key.owner != owner) {
                return Err(CommandError::InvalidPatch(owner));
            }
            if let OwnerSolution::Sketch(solution) = patch.solution {
                let slot = self.sketches.sketches.get_mut(&sketch_id).ok_or(CommandError::UnknownOwner(owner))?;
                let mut next = (**slot).clone();
                next.set_solution(solution).map_err(|_| CommandError::InvalidPatch(owner))?;
                *slot = Arc::new(next);
            }
            self.upsert_outputs(owner, patch.outputs)?;
        }
        self.sketches.derived.insert(owner, DerivedRecord { inputs: Some(patch.inputs), status: patch.status });
        Ok(())
    }

    /// Convert to Path: the derived objects of text `id` become drawn objects (same ids, same
    /// outlines); nothing is left that could re-evaluate them.
    fn detach_outputs(&mut self, id: crate::text::TextId) -> Result<(), CommandError> {
        if self.detach_owner_outputs(DerivedOwner::Text(id)) == 0 {
            return Err(CommandError::Text(crate::text::TextError::NothingToConvert(id)));
        }
        Ok(())
    }

    /// The derived objects of `owner` become drawn objects (same ids, same outlines); returns how
    /// many. Nothing is left that could re-evaluate them.
    fn detach_owner_outputs(&mut self, owner: DerivedOwner) -> usize {
        let derived: Vec<GeometryId> = self
            .objects
            .iter()
            .filter(|(_, o)| matches!(o.source(), GeometrySource::Derived(k) if k.owner == owner))
            .map(|(gid, _)| *gid)
            .collect();
        let count = derived.len();
        for gid in derived {
            if let Some(obj) = self.objects.get(&gid).cloned() {
                if let GeometrySource::Derived(key) = obj.source() {
                    self.structure.provenance.remove(key);
                }
                self.objects.insert(gid, Arc::new(obj.detached()));
                self.changes.modified.push(gid);
            }
        }
        count
    }

    /// Removes `owner`'s derived objects whose key is not in `keep`.
    fn remove_outputs(&mut self, owner: DerivedOwner, keep: &[u64]) {
        let gone: Vec<GeometryId> = self
            .objects
            .iter()
            .filter(|(_, o)| matches!(o.source(), GeometrySource::Derived(k) if k.owner == owner && !keep.contains(&k.local)))
            .map(|(id, _)| *id)
            .collect();
        for id in gone {
            if let Some(obj) = self.objects.remove(&id)
                && let GeometrySource::Derived(key) = obj.source()
            {
                self.structure.provenance.remove(key);
            }
            self.structure.placement.remove(&id);
            self.changes.deleted.push(id);
            self.changes.operations.extend(self.process.remove_geometry(id));
        }
    }

    /// Writes the complete output set of `owner` (ids reused per key); keys not listed go.
    fn upsert_outputs(&mut self, owner: DerivedOwner, outputs: Vec<crate::DerivedOutput>) -> Result<(), CommandError> {
        let keys: Vec<u64> = outputs.iter().map(|o| o.key.local).collect();
        for out in outputs {
            let existing = self
                .objects
                .iter()
                .find(|(_, o)| matches!(o.source(), GeometrySource::Derived(k) if *k == out.key))
                .map(|(id, o)| (*id, Arc::clone(o)));
            match existing {
                Some((id, obj)) => {
                    // The name only applies to new objects: a user rename survives re-evaluation.
                    if **obj.path() != out.path {
                        self.objects.insert(id, Arc::new(obj.with_path(out.path)));
                        self.changes.modified.push(id);
                    }
                }
                None => {
                    let id: GeometryId = self.ids.allocate().map_err(|_| CommandError::IdsExhausted)?;
                    let name = out.name.unwrap_or_default();
                    let obj = GeometryObject::new(id, name, out.kind, GeometrySource::Derived(out.key), out.path);
                    self.objects.insert(id, Arc::new(obj));
                    self.changes.created.push(id);
                }
            }
        }
        self.remove_outputs(owner, &keys);
        Ok(())
    }

    /// The inputs hash an evaluation of `owner` would have now (`None`: unknown owner, or an
    /// owner whose inputs the document cannot compute, such as a sketch).
    fn current_inputs(&self, owner: DerivedOwner) -> Option<burnburn_types::ContentHash> {
        let lookup = |g: GeometryId| self.objects.get(&g).map(|o| o.as_ref());
        match owner {
            DerivedOwner::Modifier(m) => self.structure.modifiers.get(&m).map(|c| chain_inputs(m, c, &lookup)),
            DerivedOwner::Part(i) => {
                let inst = self.structure.instances.get(&i)?;
                let def = self.structure.definitions.get(&inst.definition)?;
                Some(instance_inputs(i, def, inst))
            }
            DerivedOwner::Trace(t) => self.engines.trace(t).map(|d| d.key),
            DerivedOwner::Generator(g) => self.engines.generator(g).map(|d| generator_inputs(g, d)),
            _ => None,
        }
    }

    /// Applies a modifier/part patch: outputs, provenance and record on success; only the
    /// record on failure (the last valid outputs stay, production is blocked).
    fn apply_structure_patch(&mut self, patch: DocumentPatch) -> Result<(), CommandError> {
        let owner = patch.owner;
        if patch.status == EvalStatus::Valid {
            if patch.outputs.iter().any(|o| o.key.owner != owner) {
                return Err(CommandError::InvalidPatch(owner));
            }
            self.upsert_outputs(owner, patch.outputs)?;
            if let OwnerSolution::Provenance(per_key) = patch.solution {
                for (local, edges) in per_key {
                    self.structure.provenance.insert(crate::DerivedKey { owner, local }, Arc::from(edges));
                }
            }
        }
        self.structure.derived.insert(owner, DerivedRecord { inputs: Some(patch.inputs), status: patch.status });
        Ok(())
    }

    /// Re-evaluates every part instance and modifier chain whose inputs changed: parts first
    /// (their outputs may be modifier sources), then chains in id order. Background chains are
    /// marked pending and wait for `ApplyPatch`.
    fn evaluate_structure(&mut self) -> Result<(), CommandError> {
        // Placements of objects that no longer exist are dropped with them.
        let objects = &self.objects;
        self.structure.placement.retain(|g, _| objects.contains_key(g));
        let instances: Vec<_> = self.structure.instances.keys().copied().collect();
        for i in instances {
            let owner = DerivedOwner::Part(i);
            let Some(inputs) = self.current_inputs(owner) else { continue };
            if self.structure.derived.get(&owner).and_then(|r| r.inputs) == Some(inputs) {
                continue;
            }
            let (Some(inst), Some(def)) = (
                self.structure.instances.get(&i),
                self.structure.instances.get(&i).and_then(|x| self.structure.definitions.get(&x.definition)),
            ) else {
                continue;
            };
            let patch = evaluate_instance(i, def, inst);
            self.apply_structure_patch(patch)?;
        }
        let order = structure::chain_order(&self.structure, &self.objects);
        let chains: Vec<_> =
            order.into_iter().filter_map(|m| self.structure.modifiers.get(&m).map(|c| (m, Arc::clone(c)))).collect();
        for (m, chain) in chains {
            let owner = DerivedOwner::Modifier(m);
            let lookup = |g: GeometryId| self.objects.get(&g).map(|o| o.as_ref());
            let inputs = chain_inputs(m, &chain, &lookup);
            let record = self.structure.derived.get(&owner).cloned();
            if record.as_ref().and_then(|r| r.inputs) == Some(inputs) {
                continue;
            }
            match chain.mode {
                EvalMode::Inline => {
                    let patch = evaluate_chain(m, &chain, &lookup);
                    let newly_missing = match (&patch.status, record.as_ref().map(|r| &r.status)) {
                        (EvalStatus::Failed { diagnosis: crate::EvalDiagnosis::MissingSources { objects } }, prev) => {
                            let was_missing = matches!(
                                prev,
                                Some(EvalStatus::Failed { diagnosis: crate::EvalDiagnosis::MissingSources { .. } })
                            );
                            if was_missing { Vec::new() } else { objects.clone() }
                        }
                        _ => Vec::new(),
                    };
                    for geometry in newly_missing {
                        self.warnings.push(CommandWarning::ReferenceBroken {
                            holder: RefHolder::ModifierSource { modifier: m, geometry },
                            reason: crate::structure::BreakReason::ObjectMissing,
                        });
                    }
                    self.apply_structure_patch(patch)?;
                }
                EvalMode::Background => {
                    let inputs = record.and_then(|r| r.inputs);
                    self.structure.derived.insert(owner, DerivedRecord { inputs, status: EvalStatus::Pending });
                }
            }
        }
        Ok(())
    }
}

/// Warns about every stored reference that resolved before the transaction and does not now.
fn reference_warnings(before: &DocumentSnapshot, after: &DocumentSnapshot, warnings: &mut Vec<CommandWarning>) {
    for (id, _) in after.structure().references() {
        let was_bound = before.reference_status(id).is_some_and(|s| s.is_bound());
        if !was_bound {
            continue;
        }
        match after.reference_status(id) {
            Some(RefStatus::Broken { reason }) => {
                warnings.push(CommandWarning::ReferenceBroken { holder: RefHolder::Reference(id), reason })
            }
            Some(RefStatus::Ambiguous { candidates }) => {
                warnings.push(CommandWarning::ReferenceAmbiguous { reference: id, candidates })
            }
            _ => {}
        }
    }
}
