//! Document lifecycle: new, open, save, close and the restore of recovery files.

use std::path::PathBuf;
use std::sync::Arc;

use burnburn_document::DocumentSession;
use burnburn_io::{AssetStore, Project};
use burnburn_types::{CancellationSource, DocumentId, RevisionId, TaskId};

use crate::task::{TaskError, TaskInput, TaskKind, TaskOutcome, TaskOutput, TaskRequest, revision_key};
use crate::view_model::MessageKey;

use super::{AppController, AppUpdate, HostEffect, Session, error, file_title};

impl AppController {
    pub(super) fn opened(
        &mut self,
        document: DocumentId,
        outcome: TaskOutcome,
        restored: Option<PathBuf>,
    ) -> AppUpdate {
        match outcome.result {
            Ok(TaskOutput::Opened { project, path }) => {
                let Project { uuid, document: snapshot, assets, .. } = *project;
                let mut doc = DocumentSession::from_snapshot(document, snapshot);
                let session = match &restored {
                    // A recovered document is unsaved and untitled: Save asks for a location and
                    // never writes over the recovery file or any project file by itself.
                    Some(_) => {
                        let unsaved = RevisionId::from_raw(doc.revision().raw().wrapping_add(1));
                        doc.mark_saved(unsaved);
                        self.untitled += 1;
                        let revision = doc.revision();
                        Session::new(doc, format!("Hersteld {}", self.untitled), None, (uuid, assets)).map(|mut s| {
                            s.autosaved = Some(revision);
                            s.recovery_path = Some(path);
                            s
                        })
                    }
                    None => Session::new(doc, file_title(&path), Some(path), (uuid, assets)),
                };
                let Some(session) = session else {
                    return error(None, MessageKey::OpenFailed);
                };
                let mut session = session;
                session.fonts = self.fonts.as_ref().map(|(f, _)| f.clone());
                self.sessions.insert(document, session);
                self.active = Some(document);
                self.warm_images(document);
                AppUpdate {
                    effects: vec![HostEffect::OpenWindow { document }],
                    view_changed: true,
                    scenes_changed: vec![document],
                }
            }
            Err(TaskError::Cancelled) => AppUpdate::default(),
            _ => error(None, MessageKey::OpenFailed),
        }
    }

    /// Reads the file on a worker; the window opens when the project has been read and checked.
    pub(super) fn open(&mut self, path: PathBuf) -> AppUpdate {
        self.open_task(path).0
    }

    /// Submits the open task; returns its id when submitted.
    pub(super) fn open_task(&mut self, path: PathBuf) -> (AppUpdate, Option<TaskId>) {
        let (Some(document), Some(id)) = (self.next_id::<DocumentId>(), self.next_id::<TaskId>()) else {
            return (error(None, MessageKey::OpenFailed), None);
        };
        let cancel = CancellationSource::new();
        let source = burnburn_types::RevisionId::INITIAL;
        let request = TaskRequest {
            id,
            document,
            source,
            key: revision_key(TaskKind::Open, document, source),
            cancel: cancel.token(),
            input: TaskInput::Open { path, fs: Arc::clone(&self.ports.fs) },
        };
        match self.ports.tasks.submit(request) {
            Ok(()) => {
                self.opening.insert(id, (document, cancel));
                (AppUpdate::default(), Some(id))
            }
            Err(_) => (error(None, MessageKey::OpenFailed), None),
        }
    }
}

impl AppController {
    pub(super) fn new_document(&mut self) -> AppUpdate {
        let Some(id) = self.next_id::<DocumentId>() else {
            return error(None, MessageKey::CommandRejected);
        };
        self.untitled += 1;
        let title = if self.untitled == 1 { "Naamloos".to_owned() } else { format!("Naamloos {}", self.untitled) };
        let uuid = (self.ports.new_project_uuid)();
        let Some(session) = Session::new(DocumentSession::new(id), title, None, (uuid, AssetStore::default())) else {
            return error(None, MessageKey::CommandRejected);
        };
        let mut session = session;
        session.fonts = self.fonts.as_ref().map(|(f, _)| f.clone());
        self.sessions.insert(id, session);
        self.active = Some(id);
        AppUpdate {
            effects: vec![HostEffect::OpenWindow { document: id }],
            view_changed: true,
            scenes_changed: vec![id],
        }
    }

    /// Save with a known or chosen path writes on a worker; an untitled document first asks the
    /// host for a location (native save sheet). The document stays dirty until the write succeeds.
    pub(super) fn save(&mut self, document: DocumentId, path: Option<PathBuf>) -> AppUpdate {
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::SaveFailed) };
        let (tasks, fs) = (Arc::clone(&self.ports.tasks), Arc::clone(&self.ports.fs));
        self.with_session(document, |s| {
            let Some(path) = path.or_else(|| s.path.clone()) else {
                return AppUpdate {
                    effects: vec![HostEffect::ShowSavePanel {
                        document,
                        suggested_name: format!("{}.burnburn", s.title),
                    }],
                    ..AppUpdate::default()
                };
            };
            let snapshot = s.doc.snapshot();
            let source = snapshot.revision();
            let mut project = Project::new(s.uuid, (*snapshot).clone());
            project.assets = s.assets.clone();
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id,
                document,
                source,
                key: revision_key(TaskKind::Save, document, source),
                cancel: cancel.token(),
                input: TaskInput::Save { project: Box::new(project), path, fs },
            };
            match tasks.submit(request) {
                Ok(()) => {
                    s.pending.insert(id, (TaskKind::Save, cancel));
                    AppUpdate { view_changed: true, ..AppUpdate::default() }
                }
                Err(_) => error(Some(document), MessageKey::SaveFailed),
            }
        })
    }

    pub(super) fn close(&mut self, document: DocumentId) -> AppUpdate {
        match self.sessions.get(&document) {
            Some(s) if s.doc.is_dirty() => {
                AppUpdate { effects: vec![HostEffect::ConfirmClose { document }], ..AppUpdate::default() }
            }
            Some(_) => self.close_session(document),
            None => AppUpdate::default(),
        }
    }

    pub(super) fn close_session(&mut self, document: DocumentId) -> AppUpdate {
        let Some(mut s) = self.sessions.remove(&document) else { return AppUpdate::default() };
        s.cancel_all();
        if self.active == Some(document) {
            self.active = self.sessions.keys().next_back().copied();
        }
        AppUpdate {
            effects: vec![HostEffect::CloseWindow { document }],
            view_changed: true,
            scenes_changed: Vec::new(),
        }
    }
}
