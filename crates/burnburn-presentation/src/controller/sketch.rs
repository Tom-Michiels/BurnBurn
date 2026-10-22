//! Sketches (B8, S01–S04): the constraint list and parameters (contract §11) and sketch mode —
//! the open sketch, its selection, drawing into it with relation suggestions, relations and
//! dimensions on the selection, and the overlays that show them. Window state only: every change
//! to the sketch is one `SketchCommand` and one history entry.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, MutexGuard};

use burnburn_document::sketch::{
    Along, ConstraintId, DimensionKind, DimensionRole, Entity, EntityId, Relation, Sketch, SketchCommand,
    SketchConstraint, SketchId, SketchSolution, SketchStep,
};
use burnburn_document::{
    CommandError, CommandOutcome, CommandPayload, DerivedOwner, DocumentCommand, DocumentSnapshot, EvalDiagnosis,
    EvalStatus, GeometrySource,
};
use burnburn_editor::{
    CLICK_TOLERANCE_PT, ConstraintKind, EditCommit, Overlay, PlanError, PointerEvent, PointerPhase, Selection,
    SketchGeometry, SketchGlyph, SketchOverlays, SketchView, SuggestionKind, ToolKind, ToolPreview,
    constraint_relations, pick_constraint, pick_entity, plan_commit, sketch_from_paths, sketch_overlays,
    suggest_preview,
};
use burnburn_geometry::Point;
use burnburn_sketch::param::Value;
use burnburn_sketch::solver::SketchSolverAdapter;
use burnburn_sketch::{Diagnosis, evaluate_sketch, measure};
use burnburn_types::{CancellationToken, DocumentId, RequestId, RevisionId};

use super::{AppController, AppUpdate, HostEffect, Session, after_change, apply_command, apply_edit, error};
use crate::field::format_mm;
use crate::intent::{CommandKey, SelectMode, SketchDimensionKind};
use crate::view_model::{
    ConstraintHealth, ConstraintRow, MessageKey, SketchConstraintKind, SketchModeViewModel, SketchStatus,
    SketchViewModel,
};

/// A pick within this many points of a relation glyph or dimension label hits it.
const GLYPH_HIT_PT: f64 = 8.0;
const LABEL_HIT_PT: f64 = 14.0;

/// Sketch mode of one window: which sketch is open and what is selected in it.
#[derive(Default)]
pub(super) struct SketchState {
    pub(super) active: Option<SketchId>,
    pub(super) entities: BTreeSet<EntityId>,
    pub(super) constraints: BTreeSet<ConstraintId>,
    /// The suppress modifier (Cmd+Shift) was down at the last pointer event: no relation is
    /// suggested or applied (S01).
    pub(super) suppress: bool,
    /// Why the last sketch input changed nothing.
    pub(super) refused: Option<MessageKey>,
    analysis: Mutex<Option<(RevisionId, SketchId, Arc<Analysis>)>>,
}

impl SketchState {
    fn cache(&self) -> MutexGuard<'_, Option<(RevisionId, SketchId, Arc<Analysis>)>> {
        self.analysis.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// What the solver says about the open sketch, computed once per revision.
#[derive(Debug, Default)]
pub(super) struct Analysis {
    /// Points with a coordinate that can still move.
    free_points: BTreeSet<EntityId>,
    /// Each dimension's value on the solved geometry (mm, or degrees for an angle).
    values: BTreeMap<ConstraintId, String>,
    health: BTreeMap<ConstraintId, ConstraintHealth>,
    /// Curves that ended on another geometric branch than the previous revision had them (S05).
    branch_flips: usize,
    /// The sketch's stored solution at this revision: the next revision is judged against it.
    solution: Option<SketchSolution>,
}

/// `sketch` as it was solved before: its definition with `previous`' positions as the starting
/// point (entries for entities that are gone or changed kind are dropped).
fn seeded(sketch: &Sketch, previous: &SketchSolution) -> Option<Sketch> {
    let is = |id: &EntityId, want: fn(&Entity) -> bool| sketch.entity(*id).is_some_and(|e| want(&e.entity));
    let solution = SketchSolution {
        points: previous
            .points
            .iter()
            .filter(|(id, _)| is(id, |e| matches!(e, Entity::Point { .. })))
            .map(|(i, p)| (*i, *p))
            .collect(),
        radii: previous
            .radii
            .iter()
            .filter(|(id, _)| is(id, |e| matches!(e, Entity::Circle { .. })))
            .map(|(i, r)| (*i, *r))
            .collect(),
        minors: previous
            .minors
            .iter()
            .filter(|(id, _)| is(id, |e| matches!(e, Entity::Ellipse { .. })))
            .map(|(i, m)| (*i, *m))
            .collect(),
        dof: previous.dof,
    };
    Sketch::from_parts(
        sketch.name().to_owned(),
        sketch.entities().map(|(i, e)| (i, *e)).collect(),
        sketch.constraints().map(|(i, c)| (i, c.clone())).collect(),
        sketch.parameters().clone(),
        sketch.next_entity_id().raw(),
        sketch.next_constraint_id().raw(),
        Some(solution),
    )
    .ok()
}

/// How many curves the current definition moves to another geometric branch when it is solved
/// from the previous revision's solution: the S05 notice, so a jump is never silent.
fn count_branch_flips(sketch: &Sketch, previous: Option<&SketchSolution>) -> usize {
    let Some(start) = previous.and_then(|p| seeded(sketch, p)) else { return 0 };
    evaluate_sketch(&start, &SketchSolverAdapter, &CancellationToken::never()).map_or(0, |ev| ev.flipped.len())
}

fn analyse(sketch: &Sketch, previous: Option<&SketchSolution>) -> Analysis {
    let Ok(ev) = evaluate_sketch(sketch, &SketchSolverAdapter, &CancellationToken::never()) else {
        return Analysis::default();
    };
    let free_points = ev
        .free
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| matches!(sketch.entity(*id).map(|e| e.entity), Some(Entity::Point { .. })))
        .collect();
    let mut values = BTreeMap::new();
    if let Some(solved) = &ev.solved {
        for (id, c) in sketch.constraints() {
            if let SketchConstraint::Dimension(d) = c {
                match measure(solved, d.kind) {
                    Some(Value::Length(l)) => {
                        values.insert(id, format_mm(l.mm()));
                    }
                    Some(Value::Angle(a)) => {
                        values.insert(id, format_mm(a.degrees()));
                    }
                    _ => {}
                }
            }
        }
    }
    let health = ev
        .diagnostics
        .iter()
        .map(|(id, d)| {
            let h = match d {
                Diagnosis::Satisfied => ConstraintHealth::Satisfied,
                Diagnosis::Redundant { .. } => ConstraintHealth::Redundant,
                Diagnosis::Conflicting { .. } => ConstraintHealth::Conflicting,
                Diagnosis::Unmet { .. } => ConstraintHealth::Unmet,
                Diagnosis::Measured(_) => ConstraintHealth::Measured,
                Diagnosis::Invalid(_) => ConstraintHealth::Invalid,
                _ => ConstraintHealth::Unknown,
            };
            (*id, h)
        })
        .collect();
    Analysis {
        free_points,
        values,
        health,
        branch_flips: count_branch_flips(sketch, previous),
        solution: sketch.solution().cloned(),
    }
}

/// The analysis of the open sketch at the current revision (cached).
fn analysis(s: &Session) -> Option<(SketchId, Arc<Analysis>)> {
    let id = s.sketch.active?;
    let snapshot = s.doc.snapshot();
    let sketch = snapshot.sketch(id)?;
    let mut cache = s.sketch.cache();
    if let Some((revision, cached, a)) = cache.as_ref()
        && *revision == snapshot.revision()
        && *cached == id
    {
        return Some((id, Arc::clone(a)));
    }
    // The previous revision of this sketch (when it was analysed) is what a jump is judged against.
    let previous = cache.as_ref().filter(|(_, cached, _)| *cached == id).and_then(|(_, _, a)| a.solution.clone());
    let a = Arc::new(analyse(sketch, previous.as_ref()));
    *cache = Some((snapshot.revision(), id, Arc::clone(&a)));
    Some((id, a))
}

// ---------------------------------------------------------------------------------------------
// Entering and leaving
// ---------------------------------------------------------------------------------------------

fn enter(s: &mut Session, id: SketchId) {
    s.sketch.active = Some(id);
    s.sketch.entities.clear();
    s.sketch.constraints.clear();
    s.sketch.refused = None;
    s.selection = Selection::default();
}

fn leave(s: &mut Session) {
    s.sketch.active = None;
    s.sketch.entities.clear();
    s.sketch.constraints.clear();
    s.sketch.refused = None;
    s.sketch.suppress = false;
    *s.sketch.cache() = None;
}

/// After every document change: what no longer exists is no longer selected, and a sketch that
/// is gone (undo of its creation) ends sketch mode.
pub(super) fn sync(s: &mut Session) {
    let Some(id) = s.sketch.active else { return };
    let snapshot = s.doc.snapshot();
    match snapshot.sketch(id) {
        None => leave(s),
        Some(sketch) => {
            // The sketch is edited through its entities: no object handles on its derived contours.
            s.selection = Selection::default();
            s.sketch.entities.retain(|e| sketch.entity(*e).is_some());
            s.sketch.constraints.retain(|c| sketch.constraint(*c).is_some());
        }
    }
}

fn refuse(s: &mut Session, document: DocumentId, key: MessageKey) -> AppUpdate {
    s.sketch.refused = Some(key);
    let mut update = AppUpdate::redraw(document);
    update.view_changed = true;
    update
}

/// Applies a sketch payload and returns the outcome too (the new ids).
fn apply_capturing(
    s: &mut Session,
    document: DocumentId,
    payload: CommandPayload,
    request: Option<RequestId>,
) -> (AppUpdate, Option<CommandOutcome>) {
    let Some(request) = request else { return (error(Some(document), MessageKey::CommandRejected), None) };
    let cmd = DocumentCommand { document, expected: s.doc.revision(), request, payload };
    let result: Result<CommandOutcome, CommandError> = apply_command(&s.fonts, &mut s.doc, cmd);
    let outcome = result.as_ref().ok().cloned();
    (after_change(s, document, result), outcome)
}

/// Schets ▸ Nieuwe schets.
pub(super) fn new_sketch(s: &mut Session, document: DocumentId, request: Option<RequestId>) -> AppUpdate {
    let name = format!("Schets {}", s.doc.snapshot().sketch_layer().sketches().len() + 1);
    let payload = CommandPayload::Sketch(SketchCommand::Create { sketch: Box::new(Sketch::new(name)) });
    let (update, outcome) = apply_capturing(s, document, payload, request);
    if let Some(id) = outcome.and_then(|o| o.changes.sketches.first().copied()) {
        enter(s, id);
        s.trim_mode = false;
        s.editor.select_tool(ToolKind::Line);
    }
    update
}

/// Schets ▸ Maak schets van selectie: the selected drawn objects become a sketch.
pub(super) fn from_selection(s: &mut Session, document: DocumentId, request: Option<RequestId>) -> AppUpdate {
    let snapshot = s.doc.snapshot();
    let targets = s.selection.ids().to_vec();
    let objects: Vec<_> = targets.iter().filter_map(|id| snapshot.object(*id)).collect();
    if objects.is_empty() || objects.len() != targets.len() {
        return AppUpdate::default();
    }
    if objects.iter().any(|o| matches!(o.source(), GeometrySource::Derived(_))) {
        return error(Some(document), MessageKey::SketchConversionImpossible);
    }
    let name = format!("Schets {}", snapshot.sketch_layer().sketches().len() + 1);
    let paths: Vec<&burnburn_geometry::Path> = objects.iter().map(|o| o.path().as_ref()).collect();
    let Some(sketch) = sketch_from_paths(&name, &paths) else {
        return error(Some(document), MessageKey::SketchConversionImpossible);
    };
    let payload = CommandPayload::Sketch(SketchCommand::CreateFrom { sketch: Box::new(sketch), replace: targets });
    let (update, outcome) = apply_capturing(s, document, payload, request);
    if let Some(id) = outcome.and_then(|o| o.changes.sketches.first().copied()) {
        enter(s, id);
        s.selection = Selection::default();
    }
    update
}

/// The sketch the single selected object was derived from.
fn selected_sketch(s: &Session) -> Option<SketchId> {
    let snapshot = s.doc.snapshot();
    let id = s.selection.single()?;
    match snapshot.object(id)?.source() {
        GeometrySource::Derived(key) => match key.owner {
            DerivedOwner::Sketch(sketch) => Some(sketch),
            _ => None,
        },
        _ => None,
    }
}

/// Schets ▸ Bewerk schets.
pub(super) fn edit_selected(s: &mut Session, document: DocumentId) -> AppUpdate {
    match selected_sketch(s) {
        Some(id) => {
            enter(s, id);
            s.editor.select_tool(ToolKind::Select);
            AppUpdate::redraw(document)
        }
        None => AppUpdate::default(),
    }
}

fn status_of(s: &Session, id: SketchId) -> Option<SketchStatus> {
    sketch_rows(&s.doc.snapshot()).into_iter().find(|r| r.id == id).map(|r| r.status)
}

/// "Herstel laatste geldige" (S04): undo step by step until the sketch solves again. The failed
/// edits stay on the redo stack. When the history holds no solving state the steps are redone
/// and the user is told (nothing is lost, nothing guessed).
fn restore_last_valid(s: &mut Session, document: DocumentId, id: SketchId) -> AppUpdate {
    if s.sketch.active != Some(id) || status_of(s, id) != Some(SketchStatus::Failed) {
        return AppUpdate::default();
    }
    // A gesture in progress is cancelled first; it never reaches history (A4).
    s.editor.cancel();
    let mut undone = 0usize;
    let mut last = None;
    while status_of(s, id) == Some(SketchStatus::Failed) {
        let expected = s.doc.revision();
        match s.doc.undo(expected) {
            Ok(outcome) => {
                undone += 1;
                last = Some(outcome);
            }
            Err(_) => break,
        }
    }
    if status_of(s, id) == Some(SketchStatus::Valid) {
        s.sketch.refused = None;
        return match last {
            Some(outcome) => after_change(s, document, Ok(outcome)),
            None => AppUpdate::default(),
        };
    }
    let mut back = None;
    for _ in 0..undone {
        let expected = s.doc.revision();
        match s.doc.redo(expected) {
            Ok(outcome) => back = Some(outcome),
            Err(_) => break,
        }
    }
    let mut update = match back {
        Some(outcome) => after_change(s, document, Ok(outcome)),
        None => AppUpdate::default(),
    };
    let refused = refuse(s, document, MessageKey::SketchNothingToRestore);
    update.view_changed |= refused.view_changed;
    update.scenes_changed.extend(refused.scenes_changed);
    update
}

/// Schets ▸ Sluit schets.
pub(super) fn finish(s: &mut Session, document: DocumentId) -> AppUpdate {
    if s.sketch.active.is_none() {
        return AppUpdate::default();
    }
    leave(s);
    s.editor.select_tool(ToolKind::Select);
    AppUpdate::redraw(document)
}

// ---------------------------------------------------------------------------------------------
// Selection
// ---------------------------------------------------------------------------------------------

fn toggle<T: Ord + Copy>(set: &mut BTreeSet<T>, id: T) {
    if !set.remove(&id) {
        set.insert(id);
    }
}

fn select(
    s: &mut Session,
    document: DocumentId,
    entities: Vec<EntityId>,
    constraints: Vec<ConstraintId>,
    mode: SelectMode,
) -> AppUpdate {
    let Some(id) = s.sketch.active else { return AppUpdate::default() };
    let snapshot = s.doc.snapshot();
    let Some(sketch) = snapshot.sketch(id) else { return AppUpdate::default() };
    let entities: Vec<EntityId> = entities.into_iter().filter(|e| sketch.entity(*e).is_some()).collect();
    let constraints: Vec<ConstraintId> = constraints.into_iter().filter(|c| sketch.constraint(*c).is_some()).collect();
    match mode {
        SelectMode::Replace => {
            s.sketch.entities = entities.into_iter().collect();
            s.sketch.constraints = constraints.into_iter().collect();
        }
        SelectMode::Toggle => {
            for e in entities {
                toggle(&mut s.sketch.entities, e);
            }
            for c in constraints {
                toggle(&mut s.sketch.constraints, c);
            }
        }
    }
    s.sketch.refused = None;
    AppUpdate::redraw(document)
}

impl AppController {
    pub(super) fn sketch_select(
        &mut self,
        document: DocumentId,
        entities: Vec<EntityId>,
        constraints: Vec<ConstraintId>,
        mode: SelectMode,
    ) -> AppUpdate {
        self.with_session(document, |s| select(s, document, entities, constraints, mode))
    }

    /// The conflict message was clicked: select the constraints the diagnosis names.
    pub(super) fn select_sketch_problem(&mut self, document: DocumentId) -> AppUpdate {
        self.with_session(document, |s| {
            let Some(id) = s.sketch.active else { return AppUpdate::default() };
            let (_, problem) = problem_of(&s.doc.snapshot(), id);
            s.sketch.constraints = problem.into_iter().collect();
            AppUpdate::redraw(document)
        })
    }

    /// A parameter field committed: the same text again is no history entry (Return and the end
    /// of editing both deliver it).
    pub(super) fn set_sketch_parameter(
        &mut self,
        document: DocumentId,
        sketch: SketchId,
        name: String,
        text: String,
    ) -> AppUpdate {
        let unchanged = self
            .sessions
            .get(&document)
            .and_then(|s| s.doc.snapshot().sketch(sketch).map(|def| def.parameters().get(&name) == Some(text.as_str())))
            .unwrap_or(false);
        if unchanged {
            return AppUpdate::default();
        }
        self.sketch_edit(document, SketchCommand::SetParameter { sketch, name, text })
    }

    /// Parameter ▸ Verwijder (S02): refused while something still uses the parameter.
    pub(super) fn remove_sketch_parameter(
        &mut self,
        document: DocumentId,
        sketch: SketchId,
        name: String,
    ) -> AppUpdate {
        let request = self.next_id::<RequestId>();
        self.with_session(document, |s| {
            let snapshot = s.doc.snapshot();
            let Some(def) = snapshot.sketch(sketch) else { return AppUpdate::default() };
            if def.parameters().get(&name).is_none() || super::sketch_param::parameter_in_use(def, &name) {
                return refuse(s, document, MessageKey::SketchParameterInUse);
            }
            s.sketch.refused = None;
            apply_edit(s, document, CommandPayload::Sketch(SketchCommand::RemoveParameter { sketch, name }), request)
        })
    }

    /// "Herstel laatste geldige" (S04).
    pub(super) fn restore_last_valid_sketch(&mut self, document: DocumentId, sketch: SketchId) -> AppUpdate {
        self.with_session(document, |s| restore_last_valid(s, document, sketch))
    }

    pub(super) fn add_sketch_dimension(
        &mut self,
        document: DocumentId,
        kind: SketchDimensionKind,
        text: String,
    ) -> AppUpdate {
        let text = super::sketch_param::normalise_decimal_commas(&text);
        let request = self.next_id::<RequestId>();
        self.with_session(document, |s| add_dimension(s, document, kind, &text, request))
    }
}

// ---------------------------------------------------------------------------------------------
// Pointer
// ---------------------------------------------------------------------------------------------

/// A pointer event in sketch mode. `None`: not ours, the normal tool handles it (drawing tools).
/// The suppress modifier is read from every event so a drawing tool's release sees it (S01).
pub(super) fn pointer(
    s: &mut Session,
    document: DocumentId,
    ev: &PointerEvent,
    _request: RequestId,
) -> Option<AppUpdate> {
    s.sketch.active?;
    s.sketch.suppress = ev.modifiers.command && ev.modifiers.shift;
    if s.editor.tool() != ToolKind::Select {
        return None;
    }
    let snapshot = s.doc.snapshot();
    let mm_per_point = s.camera.mm_per_point();
    let sketch = s.sketch.active.and_then(|id| snapshot.sketch(id).map(|sk| (id, sk)));
    let Some((id, sketch)) = sketch else { return Some(AppUpdate::default()) };
    let anchors = anchors(sketch, mm_per_point);
    match (ev.phase, ev.click_count >= 2) {
        (PointerPhase::Down, true) => {
            let hit = pick_constraint(&anchors.labels, ev.position, LABEL_HIT_PT * mm_per_point)?;
            let text = match sketch.constraint(hit) {
                Some(SketchConstraint::Dimension(d)) => d.text.clone(),
                _ => return Some(AppUpdate::default()),
            };
            s.sketch.constraints = [hit].into();
            let mut update = AppUpdate::redraw(document);
            update.effects.push(HostEffect::EditSketchDimension { document, sketch: id, constraint: hit, text });
            Some(update)
        }
        (PointerPhase::Up, false) if ev.click_count < 2 => {
            let tol = CLICK_TOLERANCE_PT * mm_per_point;
            let constraint = pick_constraint(&anchors.labels, ev.position, LABEL_HIT_PT * mm_per_point)
                .or_else(|| pick_constraint(&anchors.glyphs, ev.position, GLYPH_HIT_PT * mm_per_point));
            let entity =
                if constraint.is_none() { pick_entity(&SketchGeometry::new(sketch), ev.position, tol) } else { None };
            let mode = if ev.modifiers.shift { SelectMode::Toggle } else { SelectMode::Replace };
            let (entities, constraints) = (entity.into_iter().collect(), constraint.into_iter().collect());
            Some(select(s, document, entities, constraints, mode))
        }
        _ => Some(AppUpdate::default()),
    }
}

/// Where relation glyphs and dimension labels are drawn, for picking.
fn anchors(sketch: &Sketch, mm_per_point: f64) -> SketchOverlays {
    let none_e = BTreeSet::new();
    let none_c = BTreeSet::new();
    let values = BTreeMap::new();
    sketch_overlays(&SketchView {
        sketch,
        selected_entities: &none_e,
        selected_constraints: &none_c,
        conflicts: &none_c,
        free_points: None,
        values: &values,
        mm_per_point,
    })
}

/// A drawing tool finished a gesture in sketch mode: its shape goes into the sketch together with
/// the relations it suggested (unless suppressed), as one history entry.
pub(super) fn commit_shape(s: &mut Session, document: DocumentId, edit: &EditCommit, request: RequestId) -> AppUpdate {
    let snapshot = s.doc.snapshot();
    let Some(id) = s.sketch.active else { return AppUpdate::default() };
    let Some(sketch) = snapshot.sketch(id) else { return AppUpdate::default() };
    let tol = CLICK_TOLERANCE_PT * s.camera.mm_per_point();
    match plan_commit(sketch, edit, tol, s.sketch.suppress) {
        Err(PlanError::Unsupported) => refuse(s, document, MessageKey::SketchShapeUnsupported),
        Ok(plan) => {
            let batch = SketchCommand::Batch { sketch: id, steps: plan.steps };
            let update = apply_edit(s, document, CommandPayload::Sketch(batch), Some(request));
            s.selection = Selection::default();
            s.sketch.refused = None;
            update
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Relations and dimensions on the selection
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Point,
    Line,
    Circle,
    Arc,
    Ellipse,
}

fn kind_of(sketch: &Sketch, id: EntityId) -> Option<Kind> {
    Some(match sketch.entity(id)?.entity {
        Entity::Point { .. } => Kind::Point,
        Entity::Line { .. } => Kind::Line,
        Entity::Circle { .. } => Kind::Circle,
        Entity::Arc { .. } => Kind::Arc,
        Entity::Ellipse { .. } => Kind::Ellipse,
        _ => return None,
    })
}

/// Radius and diameter dimensions apply to round curves and, along the major axis, to ellipses.
fn is_curve(k: Kind) -> bool {
    matches!(k, Kind::Circle | Kind::Arc | Kind::Ellipse)
}

/// The constraint a Schets-menu / panel relation command adds.
fn constraint_of(command: CommandKey) -> Option<ConstraintKind> {
    Some(match command {
        CommandKey::SketchCoincident => ConstraintKind::Coincident,
        CommandKey::SketchHorizontal => ConstraintKind::Horizontal,
        CommandKey::SketchVertical => ConstraintKind::Vertical,
        CommandKey::SketchParallel => ConstraintKind::Parallel,
        CommandKey::SketchPerpendicular => ConstraintKind::Perpendicular,
        CommandKey::SketchEqual => ConstraintKind::Equal,
        CommandKey::SketchTangent => ConstraintKind::Tangent,
        CommandKey::SketchFix => ConstraintKind::Fix,
        _ => return None,
    })
}

/// The relations `command` adds for the selection, or `None` when the selection does not fit.
/// The table is the editor's: the constraint tools of the toolbar read the same one.
pub(super) fn relations_for(
    command: CommandKey,
    sketch: &Sketch,
    selected: &BTreeSet<EntityId>,
) -> Option<Vec<Relation>> {
    let entities: Vec<EntityId> = selected.iter().copied().collect();
    constraint_relations(constraint_of(command)?, sketch, &entities)
}

/// Adds the relations of `command` for the selection.
pub(super) fn apply_relation(
    s: &mut Session,
    document: DocumentId,
    command: CommandKey,
    request: Option<RequestId>,
) -> AppUpdate {
    match constraint_of(command) {
        Some(kind) => add_relations(s, document, kind, request),
        None => AppUpdate::default(),
    }
}

/// A constraint tool chosen in the toolbar while entities are already selected in the sketch:
/// when they fit, the relation is added at once (the same route as the panel buttons) and the
/// tool is not needed; otherwise the tool starts and the entities are picked on the canvas.
pub(super) fn constraint_for_selection(
    s: &mut Session,
    document: DocumentId,
    tool: ToolKind,
    request: Option<RequestId>,
) -> Option<AppUpdate> {
    let kind = ConstraintKind::of_tool(tool)?;
    let id = s.sketch.active?;
    let snapshot = s.doc.snapshot();
    let entities: Vec<EntityId> = s.sketch.entities.iter().copied().collect();
    constraint_relations(kind, snapshot.sketch(id)?, &entities)?;
    Some(add_relations(s, document, kind, request))
}

fn add_relations(s: &mut Session, document: DocumentId, kind: ConstraintKind, request: Option<RequestId>) -> AppUpdate {
    let Some(id) = s.sketch.active else { return AppUpdate::default() };
    let snapshot = s.doc.snapshot();
    let Some(sketch) = snapshot.sketch(id) else { return AppUpdate::default() };
    if s.sketch.entities.is_empty() {
        return refuse(s, document, MessageKey::SketchNothingSelected);
    }
    let entities: Vec<EntityId> = s.sketch.entities.iter().copied().collect();
    let Some(relations) = constraint_relations(kind, sketch, &entities) else {
        return refuse(s, document, MessageKey::SketchSelectionDoesNotFit);
    };
    let mut constraints: Vec<SketchConstraint> = relations.into_iter().map(SketchConstraint::Relation).collect();
    let edit = if constraints.len() == 1 {
        SketchCommand::AddConstraint { sketch: id, constraint: constraints.remove(0) }
    } else {
        SketchCommand::Batch { sketch: id, steps: constraints.into_iter().map(SketchStep::AddConstraint).collect() }
    };
    s.sketch.refused = None;
    apply_edit(s, document, CommandPayload::Sketch(edit), request)
}

/// The dimension "Maat" adds for the selection, or `None` when the selection does not fit.
fn dimension_for(kind: SketchDimensionKind, sketch: &Sketch, selected: &BTreeSet<EntityId>) -> Option<DimensionKind> {
    let items: Vec<(EntityId, Kind)> =
        selected.iter().map(|id| Some((*id, kind_of(sketch, *id)?))).collect::<Option<_>>()?;
    let ends = |line: EntityId| match sketch.entity(line)?.entity {
        Entity::Line { start, end } => Some((start, end)),
        _ => None,
    };
    let two_points = |along: Along| match items.as_slice() {
        [(a, Kind::Point), (b, Kind::Point)] => Some(DimensionKind::Distance { a: *a, b: *b, along }),
        [(l, Kind::Line)] => ends(*l).map(|(a, b)| DimensionKind::Distance { a, b, along }),
        _ => None,
    };
    match (kind, items.as_slice()) {
        (SketchDimensionKind::Auto, [(l, Kind::Line)]) | (SketchDimensionKind::Length, [(l, Kind::Line)]) => {
            Some(DimensionKind::Length { line: *l })
        }
        (SketchDimensionKind::Auto, [(_, Kind::Point), (_, Kind::Point)]) | (SketchDimensionKind::Distance, _) => {
            two_points(Along::Direct)
        }
        (SketchDimensionKind::DistanceX, _) => two_points(Along::X),
        (SketchDimensionKind::DistanceY, _) => two_points(Along::Y),
        (SketchDimensionKind::Auto | SketchDimensionKind::Radius, [(c, k)]) if is_curve(*k) => {
            Some(DimensionKind::Radius { curve: *c })
        }
        (SketchDimensionKind::Diameter, [(c, k)]) if is_curve(*k) => Some(DimensionKind::Diameter { curve: *c }),
        (SketchDimensionKind::MinorDiameter, [(c, Kind::Ellipse)]) => {
            Some(DimensionKind::MinorDiameter { ellipse: *c })
        }
        (SketchDimensionKind::Auto | SketchDimensionKind::Angle, [(a, Kind::Line), (b, Kind::Line)]) => {
            Some(DimensionKind::Angle { from: *a, to: *b })
        }
        _ => None,
    }
}

/// Radius of a circle or arc; of an ellipse its semi-major radius (what the solver drives).
fn radius_of_curve(g: &SketchGeometry<'_>, curve: EntityId) -> Option<f64> {
    match g.circle(curve) {
        Some((_, r)) => Some(r),
        None => g.ellipse(curve).map(|(c, m, _)| c.distance(m)),
    }
}

/// The dimension's value as drawn, as expression text (`40.05 mm`, `30 deg`).
fn measured_text(kind: DimensionKind, g: &SketchGeometry<'_>) -> Option<String> {
    let mm = |v: f64| Some(format!("{} mm", format_mm(v)));
    match kind {
        DimensionKind::Length { line } => {
            let (a, b) = g.line(line)?;
            mm(a.distance(b))
        }
        DimensionKind::Distance { a, b, along } => {
            let (pa, pb): (Point, Point) = (g.point(a)?, g.point(b)?);
            match along {
                Along::X => mm(pb.x - pa.x),
                Along::Y => mm(pb.y - pa.y),
                _ => mm(pa.distance(pb)),
            }
        }
        DimensionKind::Radius { curve } => mm(radius_of_curve(g, curve)?),
        DimensionKind::Diameter { curve } => mm(2.0 * radius_of_curve(g, curve)?),
        DimensionKind::MinorDiameter { ellipse } => mm(2.0 * g.ellipse(ellipse)?.2),
        DimensionKind::Angle { from, to } => {
            let ((a0, a1), (b0, b1)) = (g.line(from)?, g.line(to)?);
            let ta = (a1.y - a0.y).atan2(a1.x - a0.x);
            let tb = (b1.y - b0.y).atan2(b1.x - b0.x);
            let mut d = tb - ta;
            if d <= -std::f64::consts::PI {
                d += std::f64::consts::TAU;
            } else if d > std::f64::consts::PI {
                d -= std::f64::consts::TAU;
            }
            Some(format!("{} deg", format_mm(d.to_degrees())))
        }
        _ => None,
    }
}

fn add_dimension(
    s: &mut Session,
    document: DocumentId,
    kind: SketchDimensionKind,
    text: &str,
    request: Option<RequestId>,
) -> AppUpdate {
    let Some(id) = s.sketch.active else { return AppUpdate::default() };
    let snapshot = s.doc.snapshot();
    let Some(sketch) = snapshot.sketch(id) else { return AppUpdate::default() };
    if s.sketch.entities.is_empty() {
        return refuse(s, document, MessageKey::SketchNothingSelected);
    }
    let Some(dimension) = dimension_for(kind, sketch, &s.sketch.entities) else {
        return refuse(s, document, MessageKey::SketchSelectionDoesNotFit);
    };
    let typed = text.trim();
    let text = if typed.is_empty() {
        match measured_text(dimension, &SketchGeometry::new(sketch)) {
            Some(t) => t,
            None => return refuse(s, document, MessageKey::SketchSelectionDoesNotFit),
        }
    } else {
        typed.to_owned()
    };
    let new_id = sketch.next_constraint_id();
    let constraint = SketchConstraint::Dimension(burnburn_document::sketch::Dimension {
        kind: dimension,
        text: text.clone(),
        role: DimensionRole::Driving,
    });
    let edit = SketchCommand::AddConstraint { sketch: id, constraint };
    let revision = s.doc.revision();
    let mut update = apply_edit(s, document, CommandPayload::Sketch(edit), request);
    if s.doc.revision() != revision {
        s.sketch.refused = None;
        s.sketch.constraints = [new_id].into();
        update.effects.push(HostEffect::EditSketchDimension { document, sketch: id, constraint: new_id, text });
    }
    update
}

/// Schets ▸ Maat: a dimension on the selection with the value as drawn.
pub(super) fn apply_dimension_command(s: &mut Session, document: DocumentId, request: Option<RequestId>) -> AppUpdate {
    add_dimension(s, document, SketchDimensionKind::Auto, "", request)
}

/// Removes the selected constraints (one entry, however many).
pub(super) fn remove_selected(s: &mut Session, document: DocumentId, request: Option<RequestId>) -> AppUpdate {
    let Some(id) = s.sketch.active else { return AppUpdate::default() };
    let ids: Vec<ConstraintId> = s.sketch.constraints.iter().copied().collect();
    let edit = match ids.as_slice() {
        [] => return AppUpdate::default(),
        [one] => SketchCommand::RemoveConstraint { sketch: id, constraint: *one },
        many => {
            SketchCommand::Batch { sketch: id, steps: many.iter().map(|c| SketchStep::RemoveConstraint(*c)).collect() }
        }
    };
    let update = apply_edit(s, document, CommandPayload::Sketch(edit), request);
    s.sketch.constraints.clear();
    update
}

/// Whether menu item `key` applies now.
pub(super) fn command_enabled(key: CommandKey, s: &Session) -> bool {
    let active = s.sketch.active;
    let snapshot = s.doc.snapshot();
    let sketch = active.and_then(|id| snapshot.sketch(id));
    match key {
        CommandKey::SketchNew => true,
        CommandKey::SketchFromSelection => {
            active.is_none()
                && !s.selection.is_empty()
                && s.selection
                    .ids()
                    .iter()
                    .all(|id| snapshot.object(*id).is_some_and(|o| !matches!(o.source(), GeometrySource::Derived(_))))
        }
        CommandKey::SketchEdit => selected_sketch(s).is_some(),
        CommandKey::SketchFinish => active.is_some(),
        CommandKey::SketchDimension => {
            sketch.is_some_and(|sk| dimension_for(SketchDimensionKind::Auto, sk, &s.sketch.entities).is_some())
        }
        CommandKey::SketchRemoveConstraint => active.is_some() && !s.sketch.constraints.is_empty(),
        other => sketch.is_some_and(|sk| relations_for(other, sk, &s.sketch.entities).is_some()),
    }
}

// ---------------------------------------------------------------------------------------------
// What the window shows
// ---------------------------------------------------------------------------------------------

/// The failure of a sketch in words, and the constraints the diagnosis names.
fn problem_of(snapshot: &DocumentSnapshot, id: SketchId) -> (Option<MessageKey>, Vec<ConstraintId>) {
    match snapshot.derived_record(DerivedOwner::Sketch(id)).map(|r| &r.status) {
        Some(EvalStatus::Failed { diagnosis }) => match diagnosis {
            EvalDiagnosis::Conflict { constraints } => (Some(MessageKey::SketchConflict), constraints.clone()),
            EvalDiagnosis::NotConverged { unmet } => (Some(MessageKey::SketchNotConverged), unmet.clone()),
            EvalDiagnosis::InvalidDimensions { constraints } => {
                (Some(MessageKey::SketchInvalidDimensions), constraints.clone())
            }
            EvalDiagnosis::InvalidParameters { .. } => (Some(MessageKey::SketchInvalidParameters), Vec::new()),
            _ => (Some(MessageKey::SketchInvalidDefinition), Vec::new()),
        },
        _ => (None, Vec::new()),
    }
}

fn kind_label(c: &SketchConstraint) -> Option<SketchConstraintKind> {
    use SketchConstraintKind as K;
    Some(match c {
        SketchConstraint::Relation(r) => match r {
            Relation::Coincident(..) => K::Coincident,
            Relation::PointOn { .. } => K::PointOn,
            Relation::Horizontal(_) => K::Horizontal,
            Relation::Vertical(_) => K::Vertical,
            Relation::Parallel(..) => K::Parallel,
            Relation::Perpendicular(..) => K::Perpendicular,
            Relation::Tangent(..) => K::Tangent,
            Relation::Equal(..) => K::Equal,
            Relation::Midpoint { .. } => K::Midpoint,
            Relation::Symmetric { .. } => K::Symmetric,
            Relation::Concentric(..) => K::Concentric,
            Relation::Fix { .. } => K::Fixed,
            _ => return None,
        },
        SketchConstraint::Dimension(d) => match d.kind {
            DimensionKind::Distance { along: Along::X, .. } => K::DistanceX,
            DimensionKind::Distance { along: Along::Y, .. } => K::DistanceY,
            DimensionKind::Distance { .. } => K::Distance,
            DimensionKind::Length { .. } => K::Length,
            DimensionKind::Radius { .. } => K::Radius,
            DimensionKind::Diameter { .. } => K::Diameter,
            DimensionKind::MinorDiameter { .. } => K::MinorDiameter,
            DimensionKind::Angle { .. } => K::Angle,
            _ => return None,
        },
    })
}

/// The constraint list and diagnosis of the open sketch.
pub(super) fn mode_view(s: &Session, snapshot: &DocumentSnapshot) -> Option<SketchModeViewModel> {
    let id = s.sketch.active?;
    let sketch = snapshot.sketch(id)?;
    let row = sketch_rows(snapshot).into_iter().find(|r| r.id == id)?;
    let (_, analysis) = analysis(s)?;
    let (problem, problem_constraints) = problem_of(snapshot, id);
    let rows = sketch
        .constraints()
        .filter_map(|(cid, c)| {
            let (text, role) = match c {
                SketchConstraint::Dimension(d) => (Some(d.text.clone()), Some(d.role)),
                SketchConstraint::Relation(_) => (None, None),
            };
            Some(ConstraintRow {
                id: cid,
                kind: kind_label(c)?,
                entities: c.entities(),
                text,
                value: analysis.values.get(&cid).cloned(),
                role,
                health: analysis.health.get(&cid).copied().unwrap_or(ConstraintHealth::Unknown),
                selected: s.sketch.constraints.contains(&cid),
            })
        })
        .collect();
    Some(SketchModeViewModel {
        sketch: id,
        name: row.name,
        status: row.status,
        problem,
        problem_constraints,
        dof: row.dof,
        rows,
        parameters: row.parameters,
        selected_entities: s.sketch.entities.iter().copied().collect(),
        selected_constraints: s.sketch.constraints.iter().copied().collect(),
        refused: s.sketch.refused,
        can_restore: row.status == SketchStatus::Failed && s.doc.can_undo(),
        branch_flips: analysis.branch_flips,
    })
}

/// The sketch overlays of this window: relations, dimensions, freedom, selection, conflicts and
/// the relations a drawing gesture would apply.
pub(super) fn scene_overlays(s: &Session, preview: Option<&ToolPreview>) -> Vec<Overlay> {
    let Some(id) = s.sketch.active else { return Vec::new() };
    let snapshot = s.doc.snapshot();
    let Some(sketch) = snapshot.sketch(id) else { return Vec::new() };
    let Some((_, analysis)) = analysis(s) else { return Vec::new() };
    let (_, problem) = problem_of(&snapshot, id);
    let conflicts: BTreeSet<ConstraintId> = problem.into_iter().collect();
    let mm_per_point = s.camera.mm_per_point();
    let mut overlays = sketch_overlays(&SketchView {
        sketch,
        selected_entities: &s.sketch.entities,
        selected_constraints: &s.sketch.constraints,
        conflicts: &conflicts,
        free_points: Some(&analysis.free_points),
        values: &analysis.values,
        mm_per_point,
    })
    .overlays;
    if let Some(preview) = preview {
        let tol = CLICK_TOLERANCE_PT * mm_per_point;
        overlays.extend(suggest_preview(sketch, preview, tol, s.sketch.suppress).into_iter().map(|g| {
            let glyph = match g.kind {
                SuggestionKind::Horizontal => SketchGlyph::Horizontal,
                SuggestionKind::Vertical => SketchGlyph::Vertical,
                SuggestionKind::Coincident => SketchGlyph::Coincident,
            };
            Overlay::SketchSuggestion { at: g.at, glyph }
        }));
    }
    overlays
}

pub(super) fn sketch_rows(snapshot: &burnburn_document::DocumentSnapshot) -> Vec<SketchViewModel> {
    snapshot
        .sketch_layer()
        .sketches()
        .map(|(id, sketch)| {
            let record = snapshot.derived_record(DerivedOwner::Sketch(id)).map(|r| &r.status);
            let status = match record {
                Some(EvalStatus::Valid) => SketchStatus::Valid,
                Some(EvalStatus::Failed { .. }) => SketchStatus::Failed,
                _ => SketchStatus::Pending,
            };
            let problem_constraints = match record {
                Some(EvalStatus::Failed {
                    diagnosis:
                        EvalDiagnosis::Conflict { constraints }
                        | EvalDiagnosis::InvalidDimensions { constraints }
                        | EvalDiagnosis::NotConverged { unmet: constraints },
                }) => constraints.clone(),
                _ => Vec::new(),
            };
            let parameters = sketch.parameters().iter().map(|(n, t)| (n.to_owned(), t.to_owned())).collect();
            SketchViewModel {
                id,
                name: sketch.name().to_owned(),
                status,
                parameters,
                constraints: sketch.constraints().len(),
                dof: sketch.solution().filter(|_| status == SketchStatus::Valid).map(|sol| sol.dof),
                problem_constraints,
                last_constraint: sketch.constraints().map(|(c, _)| c).max(),
            }
        })
        .collect()
}

impl AppController {
    /// One sketch edit as one command (one undo entry, derived geometry in the same transaction).
    pub(super) fn sketch_edit(&mut self, document: DocumentId, edit: SketchCommand) -> AppUpdate {
        let request = self.next_id::<RequestId>();
        self.with_session(document, |s| apply_edit(s, document, CommandPayload::Sketch(edit), request))
    }
}
