//! The tracer window: source image, recipe and the newest trace result.

use std::path::PathBuf;
use std::sync::Arc;

use burnburn_image::SourceImage;
use burnburn_trace::{TraceRecipe, TraceRequest, TraceResult};
use burnburn_types::{CancellationSource, DocumentId, TaskId};

use crate::task::{TaskInput, TaskKind, TaskRequest, revision_key};
use crate::view_model::{MessageKey, TraceSummary, TracerViewModel};

use super::{AppController, AppUpdate, Session, error, submit};

/// The open tracer of one window: decoded source, its stored bytes and the newest request.
#[derive(Clone)]
pub(super) struct Tracer {
    pub(super) source: Arc<SourceImage>,
    pub(super) bytes: Arc<[u8]>,
    pub(super) media: &'static str,
    pub(super) recipe: Option<TraceRecipe>,
    /// Newest submitted trace; only its result may be shown (check_fresh).
    pub(super) latest: Option<TaskId>,
    pub(super) result: Option<Arc<TraceResult>>,
}

impl AppController {
    pub(super) fn open_trace_source(&mut self, document: DocumentId, path: PathBuf) -> AppUpdate {
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::ImageFailed) };
        let (tasks, fs) = (Arc::clone(&self.ports.tasks), Arc::clone(&self.ports.fs));
        self.with_session(document, |s| {
            let source = s.doc.revision();
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id,
                document,
                source,
                key: revision_key(TaskKind::LoadImage, document, source),
                cancel: cancel.token(),
                input: TaskInput::LoadImage { path, fs },
            };
            submit(s, &tasks, request, cancel, document)
        })
    }

    /// Every recipe change is a new trace task; the previous one is cancelled (best effort) and
    /// its result can no longer pass `check_fresh`.
    pub(super) fn set_trace_recipe(&mut self, document: DocumentId, recipe: TraceRecipe) -> AppUpdate {
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::TraceFailed) };
        let tasks = Arc::clone(&self.ports.tasks);
        self.with_session(document, |s| {
            let revision = s.doc.revision();
            let Some(tracer) = s.tracer.as_mut() else { return AppUpdate::default() };
            tracer.recipe = Some(recipe.clone());
            tracer.latest = None;
            for (kind, older) in s.pending.values() {
                if *kind == TaskKind::Trace {
                    older.cancel();
                }
            }
            // T01: without a declared scale there is nothing physical to trace.
            if recipe.image.scale.is_none() {
                return AppUpdate { view_changed: true, ..AppUpdate::default() };
            }
            tracer.latest = Some(id);
            let trace =
                TraceRequest { task: id, source_revision: revision, source: Arc::clone(&tracer.source), recipe };
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id,
                document,
                source: revision,
                key: revision_key(TaskKind::Trace, document, revision),
                cancel: cancel.token(),
                input: TaskInput::Trace(Box::new(trace)),
            };
            submit(s, &tasks, request, cancel, document)
        })
    }
}

pub(super) fn tracer_view(t: &Tracer, s: &Session) -> TracerViewModel {
    let computing = s.pending.values().any(|(kind, _)| *kind == TaskKind::Trace);
    let result = t.result.as_ref().map(|r| {
        let path = r.path();
        let b = path.bounds();
        TraceSummary {
            components: r.stats.closed_shapes,
            holes: r.stats.holes,
            width_mm: b.map_or(0.0, |b| b.width()),
            height_mm: b.map_or(0.0, |b| b.height()),
            mm_per_pixel: r.stats.source_mm_per_pixel,
            fit_tolerance_mm: t.recipe.as_ref().map_or(0.0, |rc| rc.fit.tolerance_mm),
            preview: Arc::new(path),
        }
    });
    TracerViewModel {
        width_px: t.source.width(),
        height_px: t.source.height(),
        needs_scale: t.recipe.as_ref().is_none_or(|r| r.image.scale.is_none()),
        computing,
        result,
    }
}
