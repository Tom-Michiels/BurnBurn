//! Traces as a re-editable source of the document (T02): "Pas toe als trace" keeps the source
//! asset, the recipe and the engine version as a `TraceCommand`, and "Bewerk trace" opens the
//! tracer again on exactly that source and recipe; applying again replaces the outputs of the
//! same trace in place. Window state only; every document change is one trace command.

use std::collections::BTreeMap;
use std::sync::Arc;

use burnburn_document::trace::TraceId;
use burnburn_document::{CommandPayload, DerivedOwner, GeometrySource};
use burnburn_geometry::Affine;
use burnburn_trace::{TraceRecipe, centerline_command, centerline_from_engine, recipe_from_engine, trace_command};
use burnburn_types::{CancellationSource, DocumentId, RequestId, TaskId};

use crate::image_panel::TraceMode;
use crate::task::{TaskInput, TaskKind, TaskRequest, revision_key};
use crate::view_model::MessageKey;

use super::{AppController, AppUpdate, Session, after_change, apply_command, error, submit};

/// Tracers being re-opened: the task that decodes the stored source, and what to show with it.
#[derive(Default)]
pub(super) struct TraceLinkState {
    reopening: BTreeMap<TaskId, (TraceId, Box<TraceRecipe>, TraceMode)>,
}

impl TraceLinkState {
    /// The trace and recipe a finished decode task was started for.
    pub(super) fn take(&mut self, task: TaskId) -> Option<(TraceId, Box<TraceRecipe>, TraceMode)> {
        self.reopening.remove(&task)
    }
}

/// The trace the single selected object belongs to.
pub(super) fn selected_trace(s: &Session) -> Option<TraceId> {
    let snapshot = s.doc.snapshot();
    let object = snapshot.object(s.selection.single()?)?;
    match object.source() {
        GeometrySource::Derived(key) => match key.owner {
            DerivedOwner::Trace(trace) => Some(trace),
            _ => None,
        },
        _ => None,
    }
}

/// "Pas toe als trace" is possible: a result of the mode in force is on screen.
pub(super) fn apply_enabled(s: &Session) -> bool {
    s.tracer.as_ref().is_some_and(|t| match t.form.mode {
        TraceMode::Contour => t.result.is_some(),
        TraceMode::Centerline => t.centerline.is_some(),
    })
}

/// Applies the contour result as a trace: `Create`, or `Replace` when the tracer edits one.
pub(super) fn apply(s: &mut Session, document: DocumentId, request: Option<RequestId>) -> AppUpdate {
    let Some(request) = request else { return error(Some(document), MessageKey::CommandRejected) };
    let Some(t) = s.tracer.as_ref() else { return AppUpdate::default() };
    let (media, bytes, existing) = (t.media, Arc::clone(&t.bytes), t.editing);
    // The source image travels with the project; the trace refers to it by content.
    let command = match t.form.mode {
        TraceMode::Contour => {
            let Some(result) = t.result.clone() else { return AppUpdate::default() };
            let asset = s.assets.add(media, &bytes);
            trace_command(&result, asset, "Trace", Affine::IDENTITY, existing)
        }
        TraceMode::Centerline => {
            let (Some(c), Some(recipe)) = (t.centerline.as_ref(), t.recipe.as_ref()) else {
                return AppUpdate::default();
            };
            let (result, image) = (Arc::clone(&c.result), recipe.image.clone());
            let asset = s.assets.add(media, &bytes);
            centerline_command(&result, &image, asset, "Trace", Affine::IDENTITY, existing)
        }
    };
    let Ok(command) = command else { return error(Some(document), MessageKey::TraceFailed) };
    let cmd = burnburn_document::DocumentCommand {
        document,
        expected: s.doc.revision(),
        request,
        payload: CommandPayload::Trace(command),
    };
    let result = apply_command(&s.fonts, &mut s.doc, cmd);
    // A second apply from the same tracer replaces this trace instead of adding another.
    if let Ok(outcome) = &result
        && let Some(trace) = outcome.changes.traces.first().copied()
        && let Some(t) = s.tracer.as_mut()
    {
        t.editing = Some(trace);
    }
    after_change(s, document, result)
}

impl AppController {
    /// "Bewerk trace": decode the stored source on a worker, then open the tracer with the stored
    /// recipe (the first trace starts by itself, like for a file with its own resolution).
    pub(super) fn trace_edit(&mut self, document: DocumentId) -> AppUpdate {
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::TraceFailed) };
        let tasks = Arc::clone(&self.ports.tasks);
        self.with_session(document, |s| {
            let Some(trace) = selected_trace(s) else { return AppUpdate::default() };
            let snapshot = s.doc.snapshot();
            let Some(definition) = snapshot.trace(trace) else { return AppUpdate::default() };
            let Some(asset) = s.assets.get(&definition.source) else {
                return error(Some(document), MessageKey::TraceSourceMissing);
            };
            let stored = if definition.recipe.engine == burnburn_trace::CENTERLINE_ENGINE_VERSION {
                // The tracer's form knows the image recipe and the fit budget; the rest of the
                // centre-line settings are the tracer's defaults (the panel has no fields for them).
                centerline_from_engine(&definition.recipe).map(|(image, settings)| {
                    let mut recipe = TraceRecipe { image, ..TraceRecipe::default() };
                    recipe.fit.tolerance_mm = settings.tolerance_mm;
                    (recipe, TraceMode::Centerline)
                })
            } else {
                recipe_from_engine(&definition.recipe).map(|recipe| (recipe, TraceMode::Contour))
            };
            let Ok((recipe, mode)) = stored else { return error(Some(document), MessageKey::TraceFailed) };
            let revision = s.doc.revision();
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id,
                document,
                source: revision,
                key: revision_key(TaskKind::LoadImage, document, revision),
                cancel: cancel.token(),
                input: TaskInput::DecodeImage { bytes: Arc::clone(&asset.bytes) },
            };
            let update = submit(s, &tasks, request, cancel, document);
            if s.pending.contains_key(&id) {
                s.trace_link.reopening.insert(id, (trace, Box::new(recipe), mode));
            }
            update
        })
    }
}
