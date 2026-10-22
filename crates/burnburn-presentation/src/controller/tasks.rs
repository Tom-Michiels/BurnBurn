//! Delivery of worker results (`AppController::task_finished`).

use std::sync::Arc;

use burnburn_io::{Project, SaveError};
use burnburn_types::{CancellationSource, TaskId};

use crate::field::format_mm;
use crate::production::{JobInputs, PreparedJob};
use crate::task::{TaskError, TaskInput, TaskKind, TaskOutcome, TaskOutput, TaskRequest, revision_key};
use crate::view_model::{BooleanPreview, ExportViewModel, MeasureViewModel, MessageKey, StatusMessage};

use super::import::PendingImport;
use super::machine::Prepared;
use super::process::Compiled;
use super::trace::Tracer;
use super::{AppController, AppUpdate, HostEffect, file_title};

impl AppController {
    /// Delivers a worker result. Applied only while the session is open and the task is still
    /// pending. Revision-sensitive results must match the current revision; a save records
    /// exactly the revision it wrote (an older one leaves the document dirty).
    pub fn task_finished(&mut self, outcome: TaskOutcome) -> AppUpdate {
        if let Some((document, _)) = self.opening.remove(&outcome.id) {
            let restored = self.restoring.remove(&outcome.id);
            let requested = self.opening_paths.remove(&outcome.id);
            let (succeeded, cancelled) = (outcome.result.is_ok(), matches!(outcome.result, Err(TaskError::Cancelled)));
            let recent = match requested {
                // A recovered document is unsaved and untitled: it is not a recent project.
                Some(path) if restored.is_none() && succeeded => self.note_recent(&path),
                Some(path) if restored.is_none() && !cancelled => self.forget_recent(&path),
                _ => Vec::new(),
            };
            let mut update = self.opened(document, outcome, restored);
            update.effects.extend(recent);
            return update;
        }
        if let Some(job) = self.files_jobs.remove(&outcome.id) {
            return self.files_finished(job, outcome);
        }
        let saved_path = match &outcome.result {
            Ok(TaskOutput::Saved { path } | TaskOutput::SavedWithBackup { path, .. }) => Some(path.clone()),
            _ => None,
        };
        let recent = saved_path.map(|p| self.note_recent(&p)).unwrap_or_default();
        if let Some(update) = self.app_panel_task_finished(&outcome) {
            return update;
        }
        if self.app_tasks.remove(&outcome.id).is_some() {
            return self.app_task_finished(outcome);
        }
        let Some(s) = self.sessions.get_mut(&outcome.document) else { return AppUpdate::default() };
        let Some((kind, _)) = s.pending.remove(&outcome.id) else { return AppUpdate::default() };
        let document = outcome.document;
        let record = s.variable.take_record(outcome.id);
        let mut update = AppUpdate { view_changed: true, effects: recent, ..AppUpdate::default() };
        if kind.is_revision_sensitive() && outcome.source != s.doc.revision() {
            return update;
        }
        match kind {
            TaskKind::LoadBitmap | TaskKind::ImagePreview | TaskKind::SaveBitmap => {
                super::image::finished(s, document, kind, outcome.id, outcome.result, &mut update);
                return update;
            }
            TaskKind::Centerline => {
                super::trace::centerline_finished(s, outcome.id, outcome.result, &mut update);
                return update;
            }
            TaskKind::TestJob => {
                super::tests_gen::finished(s, outcome.id, outcome.result, &mut update);
                return update;
            }
            TaskKind::Nest => {
                super::nest::finished(s, document, outcome.id, outcome.source, outcome.result, &mut update);
                return update;
            }
            _ => {}
        }
        match outcome.result {
            Ok(TaskOutput::Measured(m)) => {
                s.measure = Some(MeasureViewModel {
                    revision: outcome.source,
                    objects: m.objects,
                    width_text: m.bounds.map(|b| format_mm(b.width())),
                    height_text: m.bounds.map(|b| format_mm(b.height())),
                });
                update.effects.push(HostEffect::Redraw { document });
            }
            Ok(saved @ (TaskOutput::Saved { .. } | TaskOutput::SavedWithBackup { .. })) => {
                let (path, backup) = match saved {
                    TaskOutput::SavedWithBackup { path, backup } => (path, Some(backup)),
                    TaskOutput::Saved { path } => (path, None),
                    _ => return update,
                };
                if let Some(backup) = backup {
                    s.previous_version = Some(backup);
                    s.messages.push(StatusMessage { key: MessageKey::PreviousVersionKept });
                }
                s.doc.mark_saved(outcome.source);
                if let Some(recovery) = s.recovery_path.take().filter(|_| !s.doc.is_dirty()) {
                    s.autosaved = None;
                    if let Ok(id) = self.ids.allocate::<TaskId>() {
                        let request = TaskRequest {
                            id,
                            document,
                            source: outcome.source,
                            key: revision_key(TaskKind::Recovery, document, outcome.source),
                            cancel: CancellationSource::new().token(),
                            input: TaskInput::RemoveRecovery { path: recovery, fs: Arc::clone(&self.ports.fs) },
                        };
                        if self.ports.tasks.submit(request).is_ok() {
                            self.app_tasks.insert(id, TaskKind::Recovery);
                        }
                    }
                }
                s.title = file_title(&path);
                s.path = Some(path);
                if s.close_after_save && !s.doc.is_dirty() {
                    return self.close_session(document);
                }
            }
            Ok(TaskOutput::Boolean { op, operands, result }) => {
                s.boolean_preview =
                    Some(BooleanPreview { revision: outcome.source, op, operands, result: Arc::new(result) });
                update.scenes_changed.push(document);
                update.effects.push(HostEffect::Redraw { document });
            }
            Err(TaskError::Boolean(_) | TaskError::UnknownObject(_)) => {
                s.messages.push(StatusMessage { key: MessageKey::BooleanFailed });
            }
            Ok(TaskOutput::ImageLoaded { source, bytes, .. }) => {
                let media = super::image::media_of(&bytes);
                let mut tracer = Tracer::new(source, bytes, media);
                if let Some((trace, recipe)) = s.trace_link.take(outcome.id) {
                    tracer.reopen(trace, *recipe);
                }
                s.tracer = Some(tracer);
            }
            Ok(TaskOutput::Traced(result)) => {
                if let Some(t) = s.tracer.as_mut() {
                    let current = t.recipe.as_ref().and_then(|r| r.key(t.source.content_hash()).ok());
                    let fresh = matches!((t.latest, current), (Some(latest), Some(key)) if result.check_fresh(latest, key).is_ok());
                    if fresh {
                        t.result = Some(result);
                        update.scenes_changed.push(document);
                    }
                }
            }
            Err(TaskError::Read(_) | TaskError::Decode(_)) => {
                s.messages.push(StatusMessage { key: MessageKey::ImageFailed });
                update
                    .effects
                    .push(HostEffect::ShowError { document: Some(document), message: MessageKey::ImageFailed });
            }
            Err(TaskError::Trace(_)) => s.messages.push(StatusMessage { key: MessageKey::TraceFailed }),
            Ok(TaskOutput::Compiled { plan, preview, report }) => {
                s.compiled = Some(Compiled { plan, preview, report, placement: None, setup_key: 0, outdated: false, record });
                update.scenes_changed.push(document);
            }
            Ok(TaskOutput::CompiledPlaced { plan, preview, report, placement }) => {
                let setup_key = placement.setup_key;
                s.compiled = Some(Compiled { plan, preview, report, placement: Some(placement), setup_key, outdated: false, record });
                update.scenes_changed.push(document);
            }
            Err(TaskError::Placement(why)) => {
                s.messages.push(StatusMessage { key: MessageKey::LaserRefused(why) });
                update
                    .effects
                    .push(HostEffect::ShowError { document: Some(document), message: MessageKey::LaserRefused(why) });
            }
            Err(TaskError::Compile(_)) => s.messages.push(StatusMessage { key: MessageKey::CompileFailed }),
            Ok(TaskOutput::ExportedRd { plan, path, rd_hash, bytes }) => {
                let plan_revision = s
                    .compiled
                    .as_ref()
                    .filter(|c| c.plan.id() == plan)
                    .map_or(outcome.source, |c| c.plan.source().document_revision);
                s.export = Some(ExportViewModel { plan, plan_revision, path, rd_hash, bytes });
            }
            Err(e @ (TaskError::Lower(_) | TaskError::Encode(_) | TaskError::Write(_))) => {
                // A feature without a proven RD form is said in words (cross-hatch, dot, ramp, Z).
                let key = super::process::export_failure(&e);
                s.messages.push(StatusMessage { key });
                update.effects.push(HostEffect::ShowError { document: Some(document), message: key });
            }
            Ok(TaskOutput::ImportParsed { path, bytes, parsed }) => {
                s.import = Some(PendingImport::new(file_title(&path), bytes, parsed));
            }
            Ok(TaskOutput::ImportPagesDrafted { pages, draft }) => {
                super::import::pages_drafted(s, outcome.id, pages, *draft);
            }
            Err(TaskError::Parse(e)) => {
                let message = super::import::parse_failure(&e);
                super::import::pages_failed(s, outcome.id);
                s.messages.push(StatusMessage { key: message });
                update.effects.push(HostEffect::ShowError { document: Some(document), message });
            }
            Err(TaskError::UnsupportedFormat) => {
                s.messages.push(StatusMessage { key: MessageKey::ImportFailed });
                update
                    .effects
                    .push(HostEffect::ShowError { document: Some(document), message: MessageKey::ImportFailed });
            }
            Ok(TaskOutput::Prepared { plan, rd, rd_hash }) => {
                if let Some(source) = s.preparing.take().filter(|p| p.id() == plan) {
                    let mut project = Project::new(s.uuid, (*s.doc.snapshot()).clone());
                    project.assets = s.assets.clone();
                    let inputs = JobInputs {
                        plan: Arc::clone(&source),
                        project: Arc::new(project),
                        project_path: s.path.clone(),
                    };
                    let job = PreparedJob {
                        plan,
                        plan_revision: source.source().document_revision,
                        profile: source.source().machine_profile.clone(),
                        rd_hash,
                        rd,
                        name: s.title.clone(),
                        inputs: Some(Arc::new(inputs)),
                    };
                    let placed_from = s
                        .compiled
                        .as_ref()
                        .filter(|c| c.plan.id() == source.id())
                        .and_then(|c| c.placement.as_ref())
                        .and_then(|p| p.from_position_um);
                    let record =
                        s.compiled.as_ref().filter(|c| c.plan.id() == source.id()).and_then(|c| c.record.clone());
                    s.prepared = Some(Prepared { job: Arc::new(job), approved: false, placed_from, record });
                }
            }
            Ok(TaskOutput::RecoverySaved { path }) => {
                s.autosaved = Some(outcome.source);
                s.recovery_path = Some(path);
            }
            Ok(
                TaskOutput::Opened { .. }
                | TaskOutput::RecoveryFound { .. }
                | TaskOutput::RecoveryRemoved { .. }
                | TaskOutput::TextWritten { .. }
                | TaskOutput::Nested(_)
                | TaskOutput::Centerlined(_)
                | TaskOutput::ImagePreview { .. }
                | TaskOutput::BitmapSaved { .. }
                // File jobs are delivered by `files_finished` before a session is looked up.
                | TaskOutput::Files(_),
            )
            | Err(
                TaskError::Cancelled
                | TaskError::Raster(_)
                | TaskError::SourceMissing
                | TaskError::Nest(_)
                | TaskError::Files(_),
            ) => {}
            Err(TaskError::Save(SaveError::Unconfirmed(_))) => {
                // The replace happened: this revision is on disk, only durability is unconfirmed.
                s.doc.mark_saved(outcome.source);
                s.messages.push(StatusMessage { key: MessageKey::SaveNotConfirmedDurable });
                if s.close_after_save && !s.doc.is_dirty() {
                    return self.close_session(document);
                }
            }
            Err(TaskError::Save(e)) => {
                s.close_after_save = false;
                let key = match e {
                    SaveError::NewerFileOnDisk { .. } => MessageKey::SaveNewerFileOnDisk,
                    _ => MessageKey::SaveFailed,
                };
                s.messages.push(StatusMessage { key });
                update.effects.push(HostEffect::ShowError { document: Some(document), message: key });
            }
            Err(TaskError::Open(_)) => {}
        }
        // The file's own resolution is enough for a first trace.
        if s.tracer.as_mut().is_some_and(|t| std::mem::take(&mut t.autostart)) {
            let started = self.submit_trace(document);
            update.effects.extend(started.effects);
        }
        update
    }
}
