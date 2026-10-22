//! Document lifecycle: new, open, save, close and the restore of recovery files.

use std::path::PathBuf;
use std::sync::Arc;

use burnburn_document::DocumentSession;
use burnburn_io::template::{TEMPLATE_EXTENSION, TemplateContent, TemplateError};
use burnburn_io::{AssetStore, Project};
use burnburn_types::{CancellationSource, DocumentId, RevisionId, TaskId};

use crate::files::TemplateScope;
use crate::files_task::{FilesError, FilesTask};
use crate::task::{TaskError, TaskInput, TaskKind, TaskOutcome, TaskOutput, TaskRequest, revision_key};
use crate::view_model::{MessageKey, StatusMessage};

use super::library::FilesJob;
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
                let Project { uuid, document: snapshot, assets, library_links, layer_slots, .. } = *project;
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
                session.library_links = library_links;
                session.layers.restore(&layer_slots);
                session.snap = self.app.prefs.snap_settings();
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
            Err(TaskError::Open(burnburn_io::LoadError::NewerSchema { .. })) => {
                error(None, MessageKey::OpenNewerSchema)
            }
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
        let path_for_recent = path.clone();
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
                self.opening_paths.insert(id, path_for_recent);
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
        session.snap = self.app.prefs.snap_settings();
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
            project.library_links = s.library_links.clone();
        project.layer_slots = super::layers::export(s);
            project.layer_slots = super::layers::export(s);
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

/// File name of a new template: the document's title without its project extension.
fn template_name(title: &str) -> String {
    let stem = title.strip_suffix(".burnburn").unwrap_or(title);
    format!("{stem}.{TEMPLATE_EXTENSION}")
}

fn has_template_extension(path: &std::path::Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case(TEMPLATE_EXTENSION))
}

impl AppController {
    /// Archief ▸ Bewaar als template…: asks the host for the file (and what the template holds).
    pub(super) fn ask_template_file(&mut self, document: DocumentId) -> AppUpdate {
        self.with_session(document, |s| AppUpdate {
            effects: vec![HostEffect::ShowTemplateSavePanel { document, suggested_name: template_name(&s.title) }],
            ..AppUpdate::default()
        })
    }

    /// Writes the template on a worker. The project, its title and its dirty state stay as they are.
    pub(super) fn save_as_template(&mut self, document: DocumentId, path: PathBuf, scope: TemplateScope) -> AppUpdate {
        if !has_template_extension(&path) {
            return self.with_session(document, |s| {
                s.messages
                    .push(StatusMessage { key: MessageKey::Files(crate::files::FilesMessage::TemplateWrongExtension) });
                AppUpdate { view_changed: true, ..AppUpdate::default() }
            });
        }
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::CommandRejected) };
        let (tasks, fs) = (Arc::clone(&self.ports.tasks), Arc::clone(&self.ports.fs));
        let Some(s) = self.sessions.get(&document) else { return AppUpdate::default() };
        let snapshot = s.doc.snapshot();
        let source = snapshot.revision();
        let mut project = Project::new(s.uuid, (*snapshot).clone());
        project.assets = s.assets.clone();
        project.library_links = s.library_links.clone();
        project.layer_slots = super::layers::export(s);
        let content = match scope {
            TemplateScope::Everything => TemplateContent::Everything,
            TemplateScope::SettingsOnly => TemplateContent::SettingsOnly,
        };
        let request = TaskRequest {
            id,
            document,
            source,
            key: revision_key(TaskKind::Files, document, source),
            cancel: CancellationSource::new().token(),
            input: TaskInput::Files(FilesTask::SaveTemplate { project: Box::new(project), path, content, fs }),
        };
        match tasks.submit(request) {
            Ok(()) => {
                self.files_jobs.insert(id, FilesJob::SaveTemplate { document });
                AppUpdate { view_changed: true, ..AppUpdate::default() }
            }
            Err(_) => error(Some(document), MessageKey::Files(crate::files::FilesMessage::TemplateSaveFailed)),
        }
    }

    /// Reads a template on a worker; an untitled window opens with its content when it is read.
    pub(super) fn new_from_template(&mut self, path: PathBuf) -> AppUpdate {
        let (Some(document), Some(id)) = (self.next_id::<DocumentId>(), self.next_id::<TaskId>()) else {
            return error(None, MessageKey::Files(crate::files::FilesMessage::TemplateOpenFailed));
        };
        let uuid = (self.ports.new_project_uuid)();
        let source = burnburn_types::RevisionId::INITIAL;
        let request = TaskRequest {
            id,
            document,
            source,
            key: revision_key(TaskKind::Files, document, source),
            cancel: CancellationSource::new().token(),
            input: TaskInput::Files(FilesTask::OpenTemplate {
                path: path.clone(),
                uuid,
                fs: Arc::clone(&self.ports.fs),
            }),
        };
        match self.ports.tasks.submit(request) {
            Ok(()) => {
                self.files_jobs.insert(id, FilesJob::OpenTemplate { document, path });
                AppUpdate::default()
            }
            Err(_) => error(None, MessageKey::Files(crate::files::FilesMessage::TemplateOpenFailed)),
        }
    }

    pub(super) fn template_saved(&mut self, document: DocumentId, outcome: TaskOutcome) -> AppUpdate {
        match outcome.result {
            Ok(TaskOutput::Files(crate::files_task::FilesOutput::TemplateSaved { path })) => {
                let effects = self.note_recent_template(&path);
                AppUpdate { effects, view_changed: true, ..AppUpdate::default() }
            }
            Err(TaskError::Cancelled) => AppUpdate::default(),
            result => {
                let key = match result {
                    Err(TaskError::Files(FilesError::Template(TemplateError::WrongExtension { .. }))) => {
                        crate::files::FilesMessage::TemplateWrongExtension
                    }
                    _ => crate::files::FilesMessage::TemplateSaveFailed,
                };
                self.with_session(document, |s| {
                    s.messages.push(StatusMessage { key: MessageKey::Files(key) });
                    AppUpdate {
                        effects: vec![HostEffect::ShowError {
                            document: Some(document),
                            message: MessageKey::Files(key),
                        }],
                        view_changed: true,
                        ..AppUpdate::default()
                    }
                })
            }
        }
    }

    /// The template was read: an untitled, unsaved window with its content (Save asks for a location).
    pub(super) fn template_opened(&mut self, document: DocumentId, path: PathBuf, outcome: TaskOutcome) -> AppUpdate {
        match outcome.result {
            Ok(TaskOutput::Files(crate::files_task::FilesOutput::TemplateOpened { project, .. })) => {
                let Project { uuid, document: snapshot, assets, library_links, layer_slots, .. } = *project;
                let mut doc = DocumentSession::from_snapshot(document, snapshot);
                let unsaved = RevisionId::from_raw(doc.revision().raw().wrapping_add(1));
                doc.mark_saved(unsaved);
                self.untitled += 1;
                let title =
                    if self.untitled == 1 { "Naamloos".to_owned() } else { format!("Naamloos {}", self.untitled) };
                let Some(mut session) = Session::new(doc, title, None, (uuid, assets)) else {
                    return error(None, MessageKey::Files(crate::files::FilesMessage::TemplateOpenFailed));
                };
                session.fonts = self.fonts.as_ref().map(|(f, _)| f.clone());
                session.library_links = library_links;
                session.layers.restore(&layer_slots);
                self.sessions.insert(document, session);
                self.active = Some(document);
                self.warm_images(document);
                let mut effects = vec![HostEffect::OpenWindow { document }];
                effects.extend(self.note_recent_template(&path));
                AppUpdate { effects, view_changed: true, scenes_changed: vec![document] }
            }
            Err(TaskError::Cancelled) => AppUpdate::default(),
            _ => {
                let mut update = error(None, MessageKey::Files(crate::files::FilesMessage::TemplateOpenFailed));
                update.effects.extend(self.forget_recent_template(&path));
                update
            }
        }
    }
}
