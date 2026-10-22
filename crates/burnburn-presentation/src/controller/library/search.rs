//! The project search panel (P08, G6-editor). The folder is indexed on a worker (names, objects,
//! materials, frozen revisions, a preview of each project); a query filters the index in memory.
//! Opening a found project goes through the ordinary open; a frozen production revision opens as an
//! unsaved copy with a new identity next to the current design. Everything here only reads: no
//! compile, no upload and no production intent is created, and nothing is written.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use burnburn_io::search::{MatchField, ProjectIndex};
use burnburn_io::{Project, ProjectUuid};
use burnburn_types::{CancellationSource, DocumentId, RevisionId, TaskId};

use super::{AppController, FilesJob};
use crate::controller::{AppUpdate, HostEffect, Session, error};
use crate::files::{ArtThumbnail, FilesIntent, FilesMessage};
use crate::files_task::{FilesError, FilesOutput, FilesTask};
use crate::search_panel::{SearchField, SearchIssueRow, SearchPanelViewModel, SearchResultRow, SearchRevisionRow};
use crate::task::{TaskError, TaskInput, TaskKind, TaskOutcome, TaskOutput, TaskRequest, revision_key};
use crate::view_model::MessageKey;

/// Window state of the panel.
#[derive(Debug, Default)]
pub(in crate::controller) struct SearchUi {
    folder: String,
    query: String,
    index: Option<Arc<ProjectIndex>>,
    thumbnails: BTreeMap<PathBuf, Arc<ArtThumbnail>>,
    /// The scan in flight (newest wins) and the source that cancels an older one.
    serial: u64,
    scanning: bool,
    cancel: Option<CancellationSource>,
    unreadable: bool,
}

fn field_of(f: MatchField) -> Option<SearchField> {
    Some(match f {
        MatchField::FileName => SearchField::FileName,
        MatchField::Object => SearchField::Object,
        MatchField::Operation => SearchField::Operation,
        MatchField::Material => SearchField::Material,
        MatchField::Part => SearchField::Part,
        MatchField::Plate => SearchField::Plate,
        MatchField::Revision => SearchField::Revision,
        _ => return None,
    })
}

impl AppController {
    pub(super) fn search_intent(&mut self, intent: FilesIntent) -> AppUpdate {
        match intent {
            FilesIntent::OpenSearch { document } => {
                // The folder of the project that is open, else the folder of the most recent one.
                let folder = self
                    .sessions
                    .get(&document)
                    .and_then(|s| s.path.clone())
                    .or_else(|| self.recent.first().cloned())
                    .and_then(|p| p.parent().map(Path::to_path_buf))
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                self.with_session(document, |s| {
                    if s.files.search.is_none() {
                        s.files.search = Some(SearchUi { folder: folder.clone(), ..SearchUi::default() });
                    }
                    AppUpdate::default()
                });
                self.scan(document)
            }
            FilesIntent::CloseSearch { document } => self.with_session(document, |s| {
                if let Some(ui) = s.files.search.take()
                    && let Some(cancel) = ui.cancel
                {
                    cancel.cancel();
                }
                AppUpdate { view_changed: true, ..AppUpdate::default() }
            }),
            FilesIntent::SetSearchFolder { document, folder } => {
                self.with_session(document, |s| {
                    if let Some(ui) = s.files.search.as_mut() {
                        ui.folder = folder;
                    }
                    AppUpdate::default()
                });
                self.scan(document)
            }
            FilesIntent::SetSearchQuery { document, query } => self.with_session(document, |s| {
                if let Some(ui) = s.files.search.as_mut() {
                    ui.query = query;
                }
                AppUpdate { view_changed: true, ..AppUpdate::default() }
            }),
            FilesIntent::RescanSearch { document } => self.scan(document),
            FilesIntent::OpenFoundProject { path, .. } => self.open(path),
            FilesIntent::OpenFoundRevision { document, path, number } => self.open_revision(document, path, number),
            _ => AppUpdate::default(),
        }
    }

    /// Reads the folder again; a scan still running is cancelled and its answer ignored.
    fn scan(&mut self, document: DocumentId) -> AppUpdate {
        let (Some(id), Some(request_document)) = (self.next_id::<TaskId>(), self.next_id::<DocumentId>()) else {
            return error(Some(document), MessageKey::CommandRejected);
        };
        let fs = Arc::clone(&self.ports.fs);
        let tasks = Arc::clone(&self.ports.tasks);
        let mut submitted: Option<u64> = None;
        let update = self.with_session(document, |s| {
            let Some(ui) = s.files.search.as_mut() else { return AppUpdate::default() };
            if let Some(cancel) = ui.cancel.take() {
                cancel.cancel();
            }
            ui.serial += 1;
            ui.unreadable = false;
            if ui.folder.trim().is_empty() {
                ui.scanning = false;
                ui.index = None;
                return AppUpdate { view_changed: true, ..AppUpdate::default() };
            }
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id,
                document: request_document,
                source: RevisionId::INITIAL,
                key: revision_key(TaskKind::Files, request_document, RevisionId::INITIAL),
                cancel: cancel.token(),
                input: TaskInput::Files(FilesTask::SearchProjects { dir: PathBuf::from(ui.folder.trim()), fs }),
            };
            if tasks.submit(request).is_err() {
                return error(Some(document), MessageKey::CommandRejected);
            }
            ui.cancel = Some(cancel);
            ui.scanning = true;
            submitted = Some(ui.serial);
            AppUpdate { view_changed: true, ..AppUpdate::default() }
        });
        if let Some(serial) = submitted {
            self.files_jobs.insert(id, FilesJob::SearchProjects { document, serial });
        }
        update
    }

    pub(super) fn search_scanned(&mut self, document: DocumentId, serial: u64, outcome: TaskOutcome) -> AppUpdate {
        self.with_session(document, |s| {
            let Some(ui) = s.files.search.as_mut() else { return AppUpdate::default() };
            if ui.serial != serial {
                return AppUpdate::default(); // an older scan: the newest answer counts
            }
            ui.scanning = false;
            ui.cancel = None;
            match outcome.result {
                Ok(TaskOutput::Files(FilesOutput::ProjectsSearched { index, thumbnails, .. })) => {
                    ui.index = Some(index);
                    ui.thumbnails = thumbnails.into_iter().map(|(p, t)| (p, Arc::new(t))).collect();
                    ui.unreadable = false;
                }
                Err(TaskError::Cancelled) => {}
                _ => {
                    ui.index = None;
                    ui.thumbnails.clear();
                    ui.unreadable = true;
                }
            }
            AppUpdate { view_changed: true, ..AppUpdate::default() }
        })
    }

    fn open_revision(&mut self, document: DocumentId, path: PathBuf, number: u32) -> AppUpdate {
        let (Some(new_document), Some(id)) = (self.next_id::<DocumentId>(), self.next_id::<TaskId>()) else {
            return error(Some(document), MessageKey::OpenFailed);
        };
        let uuid: ProjectUuid = (self.ports.new_project_uuid)();
        let request = TaskRequest {
            id,
            document: new_document,
            source: RevisionId::INITIAL,
            key: revision_key(TaskKind::Files, new_document, RevisionId::INITIAL),
            cancel: CancellationSource::new().token(),
            input: TaskInput::Files(FilesTask::OpenRevision { path, number, uuid, fs: Arc::clone(&self.ports.fs) }),
        };
        match self.ports.tasks.submit(request) {
            Ok(()) => {
                self.files_jobs.insert(id, FilesJob::OpenRevision { document: new_document, from: document });
                AppUpdate::default()
            }
            Err(_) => error(Some(document), MessageKey::OpenFailed),
        }
    }

    /// The frozen revision was read: an unsaved window of its own (Save asks for a location), titled
    /// with the project and the revision. The current design is untouched.
    pub(super) fn search_revision_opened(
        &mut self,
        document: DocumentId,
        from: DocumentId,
        outcome: TaskOutcome,
    ) -> AppUpdate {
        match outcome.result {
            Ok(TaskOutput::Files(FilesOutput::RevisionOpened { project, path, number, label })) => {
                let Project { uuid, document: snapshot, assets, library_links, layer_slots, .. } = *project;
                let mut doc = burnburn_document::DocumentSession::from_snapshot(document, snapshot);
                let unsaved = RevisionId::from_raw(doc.revision().raw().wrapping_add(1));
                doc.mark_saved(unsaved);
                let name = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let title = format!("{name} - {label} (revisie {number})");
                let Some(mut session) = Session::new(doc, title, None, (uuid, assets)) else {
                    return error(Some(from), MessageKey::OpenFailed);
                };
                session.fonts = self.fonts.as_ref().map(|(f, _)| f.clone());
                session.library_links = library_links;
                session.layers.restore(&layer_slots);
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
            result => {
                let message = match result {
                    Err(TaskError::Files(FilesError::RevisionMissing(_))) => FilesMessage::RevisionMissing,
                    _ => FilesMessage::RevisionOpenFailed,
                };
                error(Some(from), MessageKey::Files(message))
            }
        }
    }
}

/// What the panel of this window shows.
pub(super) fn panel_view(s: &Session) -> Option<SearchPanelViewModel> {
    let ui = s.files.search.as_ref()?;
    let row = |entry: &burnburn_io::search::IndexEntry, matched: Vec<SearchField>| SearchResultRow {
        path: entry.path.clone(),
        file_name: entry.file_name.clone(),
        matched,
        modified: entry.modified,
        thumbnail: ui.thumbnails.get(&entry.path).cloned(),
        revisions: entry
            .revisions
            .iter()
            .map(|(number, label)| SearchRevisionRow { number: *number, label: label.clone() })
            .collect(),
        objects: entry.objects.len(),
    };
    let (results, issues) = match &ui.index {
        None => (Vec::new(), Vec::new()),
        Some(index) => {
            let results = if ui.query.trim().is_empty() {
                index.entries().iter().map(|e| row(e, Vec::new())).collect()
            } else {
                index
                    .search(&ui.query)
                    .into_iter()
                    .map(|hit| row(hit.entry, hit.fields.iter().filter_map(|f| field_of(*f)).collect()))
                    .collect()
            };
            let issues = index
                .issues()
                .iter()
                .map(|i| SearchIssueRow {
                    path: i.path.clone(),
                    file_name: i.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                })
                .collect();
            (results, issues)
        }
    };
    Some(SearchPanelViewModel {
        folder: ui.folder.clone(),
        query: ui.query.clone(),
        scanning: ui.scanning,
        scanned: ui.index.as_ref().map(|i| i.entries().len()),
        folder_unreadable: ui.unreadable,
        results,
        issues,
    })
}
