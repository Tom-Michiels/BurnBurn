//! Autosave and crash recovery: recovery files per document and the window-less recovery tasks.

use std::path::PathBuf;
use std::sync::Arc;

use burnburn_io::Project;
use burnburn_types::{CancellationSource, DocumentId, RevisionId, TaskId};

use crate::task::{TaskError, TaskInput, TaskKind, TaskOutcome, TaskOutput, TaskRequest, revision_key};
use crate::view_model::MessageKey;

use super::{AppController, AppUpdate, HostEffect, error, submit};

impl AppController {
    /// Where autosave writes recovery files (one per document, `<project uuid>.burnburn`).
    pub fn set_recovery_dir(&mut self, dir: PathBuf) {
        self.recovery_dir = Some(dir);
    }

    /// Writes a recovery file for every dirty document changed since its last autosave.
    pub(super) fn autosave(&mut self) -> AppUpdate {
        let Some(dir) = self.recovery_dir.clone() else { return AppUpdate::default() };
        let (tasks, fs) = (Arc::clone(&self.ports.tasks), Arc::clone(&self.ports.fs));
        let due: Vec<DocumentId> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.doc.is_dirty() && s.autosaved != Some(s.doc.revision()))
            .filter(|(_, s)| !s.pending.values().any(|(k, _)| *k == TaskKind::Autosave))
            .map(|(id, _)| *id)
            .collect();
        let mut update = AppUpdate::default();
        for document in due {
            let Some(id) = self.next_id::<TaskId>() else { break };
            let (tasks, fs, dir) = (Arc::clone(&tasks), Arc::clone(&fs), dir.clone());
            let u = self.with_session(document, |s| {
                let snapshot = s.doc.snapshot();
                let source = snapshot.revision();
                let mut project = Project::new(s.uuid, (*snapshot).clone());
                project.assets = s.assets.clone();
                project.library_links = s.library_links.clone();
                project.layer_slots = super::layers::export(s);
                project.part_name = s.part_name.clone();
                let path = dir.join(format!(
                    "{}.burnburn",
                    s.uuid.as_bytes().iter().map(|b| format!("{b:02x}")).collect::<String>()
                ));
                let cancel = CancellationSource::new();
                let request = TaskRequest {
                    id,
                    document,
                    source,
                    key: revision_key(TaskKind::Autosave, document, source),
                    cancel: cancel.token(),
                    input: TaskInput::SaveRecovery { project: Box::new(project), path, fs },
                };
                submit(s, &tasks, request, cancel, document)
            });
            update.effects.extend(u.effects);
        }
        update
    }

    /// A task that belongs to no window (recovery list/remove).
    pub(super) fn app_task(&mut self, input: TaskInput) -> AppUpdate {
        let (Some(document), Some(id)) = (self.next_id::<DocumentId>(), self.next_id::<TaskId>()) else {
            return error(None, MessageKey::CommandRejected);
        };
        let kind = input.kind();
        let source = RevisionId::INITIAL;
        let request = TaskRequest {
            id,
            document,
            source,
            key: revision_key(kind, document, source),
            cancel: CancellationSource::new().token(),
            input,
        };
        match self.ports.tasks.submit(request) {
            Ok(()) => {
                self.app_tasks.insert(id, kind);
                AppUpdate::default()
            }
            Err(_) => error(None, MessageKey::CommandRejected),
        }
    }

    pub(super) fn app_task_finished(&mut self, outcome: TaskOutcome) -> AppUpdate {
        match outcome.result {
            Ok(TaskOutput::RecoveryFound { files }) if !files.is_empty() => {
                AppUpdate { effects: vec![HostEffect::OfferRecovery { files }], ..AppUpdate::default() }
            }
            Err(TaskError::Write(_)) => error(None, MessageKey::DiagnosticsFailed),
            _ => AppUpdate::default(),
        }
    }
}

impl AppController {
    pub(super) fn check_recovery(&mut self) -> AppUpdate {
        match self.recovery_dir.clone() {
            Some(dir) => self.app_task(TaskInput::ListRecovery { dir, fs: Arc::clone(&self.ports.fs) }),
            None => AppUpdate::default(),
        }
    }

    pub(super) fn restore_recovery(&mut self, path: PathBuf) -> AppUpdate {
        let (update, task) = self.open_task(path.clone());
        if let Some(task) = task {
            self.restoring.insert(task, path);
        }
        update
    }

    pub(super) fn discard_recovery(&mut self, path: PathBuf) -> AppUpdate {
        self.app_task(TaskInput::RemoveRecovery { path, fs: Arc::clone(&self.ports.fs) })
    }
}
