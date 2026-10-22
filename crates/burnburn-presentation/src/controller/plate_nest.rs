//! "Distribute and nest" (G9-editor, P06): the row plan of G8 chooses the plate of every instance
//! without one, and the existing nest then decides where it lies on that plate. The nest runs in the
//! background with the normal nest task; its answer is applied as one compound command, and only when
//! the document is still at the revision the job was made for.

use std::sync::Arc;

use burnburn_document::structure::{PartInstanceId, PlateId, StructureCommand};
use burnburn_document::{
    CommandPayload, CompoundCommand, CompoundLabel, CompoundStep, DerivedOwner, GeometrySource,
};
use burnburn_types::{CancellationSource, DocumentId, GeometryId, RequestId, TaskId};

use crate::nest_task::{PlateSheet, build_plate_job};
use crate::structure_panel::StructureProblem;
use crate::task::{TaskError, TaskInput, TaskKind, TaskOutput, TaskRequest, revision_key};
use crate::view_model::MessageKey;

use super::structure::plan_distribution;
use super::{AppController, AppUpdate, Session, apply_edit, error, nest, submit};

/// The space between two plates in the document (mm).
pub(crate) const SHEET_GAP_MM: f64 = crate::nest_task::SHEET_GAP_MM;

/// A distribute-and-nest run that awaits its answer.
#[derive(Debug, Clone)]
pub(super) struct PlateRun {
    task: TaskId,
    request: RequestId,
    /// Which instance goes to which plate.
    assigned: Vec<(PartInstanceId, PlateId)>,
}

fn refusal(s: &mut Session, document: DocumentId, message: MessageKey) -> AppUpdate {
    s.problem = Some(crate::view_model::ProblemViewModel {
        message,
        object: None,
        at: burnburn_geometry::Point::new(0.0, 0.0),
    });
    let mut update = error(Some(document), message);
    update.view_changed = true;
    update
}

fn problem(p: StructureProblem) -> MessageKey {
    MessageKey::Structure(p)
}

impl AppController {
    /// Plans the plates and hands the nest of every plate to the worker.
    pub(super) fn distribute_and_nest(&mut self, document: DocumentId, gap: String) -> AppUpdate {
        let (Some(task), Some(request)) = (self.next_id::<TaskId>(), self.next_id::<RequestId>()) else {
            return error(Some(document), MessageKey::CommandRejected);
        };
        let tasks = Arc::clone(&self.ports.tasks);
        self.with_session(document, |s| {
            let plan = match plan_distribution(s, &gap) {
                Ok(plan) => plan,
                Err(message) => return refusal(s, document, message),
            };
            let snapshot = s.doc.snapshot();
            let layer = snapshot.structure();
            // The plates lie side by side from the origin; an instance's parts move as one body.
            let mut x = 0.0;
            let mut sheets = Vec::new();
            for (plate_id, plate) in layer.plates() {
                let mut bodies: Vec<Vec<GeometryId>> = Vec::new();
                for (instance, _) in plan.assigned.iter().filter(|(_, to)| *to == plate_id) {
                    let owner = DerivedOwner::Part(*instance);
                    let body: Vec<GeometryId> = snapshot
                        .objects()
                        .filter(|(_, o)| matches!(o.source(), GeometrySource::Derived(k) if k.owner == owner))
                        .map(|(id, _)| id)
                        .collect();
                    if body.is_empty() {
                        // An instance without an outline has no size to lay out.
                        return refusal(s, document, problem(StructureProblem::NoRoomOnPlates));
                    }
                    bodies.push(body);
                }
                sheets.push(PlateSheet { origin_x: x, width: plate.width.mm(), height: plate.height.mm(), bodies });
                x += plate.width.mm() + SHEET_GAP_MM;
            }
            let Some(mut settings) = nest::plate_settings(plan.gap) else {
                return error(Some(document), MessageKey::CommandRejected);
            };
            settings.sheets = 1;
            let Ok(job) = build_plate_job(&snapshot, &sheets, &settings) else {
                return error(Some(document), MessageKey::CommandRejected);
            };
            nest::cancel_running(s);
            s.problem = None;
            s.plate_nest = Some(PlateRun { task, request, assigned: plan.assigned });
            let source = snapshot.revision();
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id: task,
                document,
                source,
                key: revision_key(TaskKind::Nest, document, source),
                cancel: cancel.token(),
                input: TaskInput::Nest(Box::new(job)),
            };
            submit(s, &tasks, request, cancel, document)
        })
    }
}

/// Whether `task` is the pending distribute-and-nest run of `s`.
pub(super) fn owns(s: &Session, task: TaskId) -> bool {
    s.plate_nest.as_ref().is_some_and(|run| run.task == task)
}

/// Delivery of the worker's answer for the run `owns` recognised. The caller has already dropped
/// an answer for another revision.
pub(super) fn finished(
    s: &mut Session,
    document: DocumentId,
    result: Result<TaskOutput, TaskError>,
    update: &mut AppUpdate,
) {
    let Some(run) = s.plate_nest.take() else { return };
    let refused = |s: &mut Session, update: &mut AppUpdate, message| {
        let more = refusal(s, document, message);
        update.effects.extend(more.effects);
        update.view_changed = true;
    };
    let output = match result {
        Ok(TaskOutput::Nested(output)) => output,
        Err(TaskError::Cancelled) => return,
        _ => return refused(s, update, MessageKey::CommandRejected),
    };
    let snapshot = s.doc.snapshot();
    let layer = snapshot.structure();
    let owner_of = |id: GeometryId| match snapshot.object(id)?.source() {
        GeometrySource::Derived(k) => match k.owner {
            DerivedOwner::Part(instance) => Some(instance),
            _ => None,
        },
        _ => None,
    };
    let moved: std::collections::BTreeMap<PartInstanceId, burnburn_geometry::Affine> = output
        .placements
        .iter()
        .filter(|p| p.copy == 0)
        .filter_map(|p| Some((p.objects.first().copied().and_then(owner_of)?, p.transform)))
        .collect();
    // An instance the nest could not place makes the whole run a refusal: nothing is changed.
    if !output.unplaced.is_empty() || run.assigned.iter().any(|(instance, _)| !moved.contains_key(instance)) {
        return refused(s, update, problem(StructureProblem::NoRoomOnPlates));
    }
    let mut steps = Vec::with_capacity(run.assigned.len());
    for (instance, plate) in &run.assigned {
        let (Some(current), Some(by)) = (layer.instance(*instance), moved.get(instance)) else {
            return refused(s, update, MessageKey::CommandRejected);
        };
        let mut value = current.clone();
        value.plate = Some(*plate);
        value.transform = current.transform.then(by);
        steps.push(CompoundStep::Run(CommandPayload::Structure(StructureCommand::SetInstance {
            instance: *instance,
            value,
        })));
    }
    s.problem = None;
    let payload = CommandPayload::Compound(CompoundCommand { label: CompoundLabel::DistributeOverPlates, steps });
    let applied = apply_edit(s, document, payload, Some(run.request));
    update.effects.extend(applied.effects);
    update.view_changed = true;
    update.scenes_changed.extend(applied.scenes_changed);
}
