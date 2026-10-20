//! The file features of the window (W3-UI-bestanden): import pages, templates, the Material and
//! Art Library panels, bundles and the export dialog.
//!
//! The libraries belong to the app, not to a window: one copy is read once (on a worker), changed
//! on the UI thread and written back on a worker, one write at a time. A library file that cannot
//! be read is never overwritten.

mod material;

use std::path::PathBuf;
use std::sync::Arc;

use burnburn_io::material_library::MaterialLibrary;
use burnburn_types::{CancellationSource, DocumentId, RevisionId, TaskId};

use super::machine::Machine;
use super::{AppController, AppUpdate, HostEffect, Session};
use crate::files::{FilesIntent, FilesMessage, FilesViewModel};
use crate::files_task::{FilesOutput, FilesTask, Loaded, MATERIAL_LIBRARY_FILE};
use crate::task::{TaskError, TaskInput, TaskKind, TaskOutcome, TaskOutput, TaskRequest, revision_key};
use crate::view_model::MessageKey;

/// A worker job of the file features that is waiting for its result.
#[derive(Debug, Clone)]
pub(super) enum FilesJob {
    SaveTemplate { document: DocumentId },
    OpenTemplate { document: DocumentId, path: PathBuf },
    LoadLibraries,
    SaveMaterials,
}

/// Window state of the file panels (never document data).
#[derive(Debug, Default)]
pub(super) struct FilesUi {
    pub(in crate::controller) material: Option<material::MaterialUi>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum LoadState {
    #[default]
    NotLoaded,
    Loading,
    Ready,
}

/// Writes of one library file go one at a time: a change that arrives while a write runs is
/// written after it, so the last change always wins.
#[derive(Debug, Default)]
struct Writer {
    dirty: bool,
    in_flight: Option<TaskId>,
    failed: bool,
}

/// The libraries of the app.
#[derive(Debug, Default)]
pub(super) struct Libraries {
    dir: Option<PathBuf>,
    state: LoadState,
    materials: Option<MaterialLibrary>,
    /// The material library file could not be read: it is kept as it is and never written.
    materials_unreadable: bool,
    material_writer: Writer,
}

impl Libraries {
    fn persisted(&self) -> bool {
        self.dir.is_some() && !self.materials_unreadable && !self.material_writer.failed
    }
}

impl AppController {
    pub(super) fn files(&mut self, intent: FilesIntent) -> AppUpdate {
        match intent {
            FilesIntent::SetImportPages { document, pages } => self.set_import_pages(document, pages),
            FilesIntent::SaveAsTemplate { document, path, scope } => self.save_as_template(document, path, scope),
            FilesIntent::NewFromTemplate { path } => self.new_from_template(path),
            FilesIntent::SetRecentTemplates { paths } => {
                self.restore_recent_templates(paths);
                AppUpdate { view_changed: true, ..AppUpdate::default() }
            }
            FilesIntent::ClearRecentTemplates => {
                AppUpdate { effects: self.clear_recent_templates(), view_changed: true, ..AppUpdate::default() }
            }
            intent => self.material_intent(intent),
        }
    }

    /// A worker result of a file-feature job.
    pub(super) fn files_finished(&mut self, job: FilesJob, outcome: TaskOutcome) -> AppUpdate {
        match job {
            FilesJob::SaveTemplate { document } => self.template_saved(document, outcome),
            FilesJob::OpenTemplate { document, path } => self.template_opened(document, path, outcome),
            FilesJob::LoadLibraries => self.libraries_loaded(outcome),
            FilesJob::SaveMaterials => self.material_saved(outcome),
        }
    }

    /// The folder the libraries are stored in (`material-library.json`, `art-library.json`). Without
    /// one the libraries live in memory only. Set it before the first panel is opened.
    pub fn set_library_dir(&mut self, dir: Option<PathBuf>) {
        self.libs.dir = dir;
    }

    /// Identity for a new library file (project links name it).
    fn new_library_id(&self) -> String {
        (self.ports.new_project_uuid)().as_bytes().iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Reads the libraries once. With no folder they start empty in memory.
    pub(super) fn ensure_libraries(&mut self) {
        if self.libs.state != LoadState::NotLoaded {
            return;
        }
        let Some(dir) = self.libs.dir.clone() else {
            self.libs.materials = Some(MaterialLibrary::new(&self.new_library_id()));
            self.libs.state = LoadState::Ready;
            return;
        };
        let task = FilesTask::LoadLibraries { dir, fs: Arc::clone(&self.ports.fs) };
        if self.submit_files(task, FilesJob::LoadLibraries) {
            self.libs.state = LoadState::Loading;
        }
    }

    /// Submits a file job that belongs to the app, not to a window.
    fn submit_files(&mut self, task: FilesTask, job: FilesJob) -> bool {
        let (Some(document), Some(id)) = (self.next_id::<DocumentId>(), self.next_id::<TaskId>()) else {
            return false;
        };
        let request = TaskRequest {
            id,
            document,
            source: RevisionId::INITIAL,
            key: revision_key(TaskKind::Files, document, RevisionId::INITIAL),
            cancel: CancellationSource::new().token(),
            input: TaskInput::Files(task),
        };
        if self.ports.tasks.submit(request).is_err() {
            return false;
        }
        if let FilesJob::SaveMaterials = job {
            self.libs.material_writer.in_flight = Some(id);
        }
        self.files_jobs.insert(id, job);
        true
    }

    fn libraries_loaded(&mut self, outcome: TaskOutcome) -> AppUpdate {
        let mut effects = Vec::new();
        match outcome.result {
            Ok(TaskOutput::Files(FilesOutput::LibrariesLoaded { materials })) => {
                let fresh = |this: &Self| MaterialLibrary::new(&this.new_library_id());
                match materials {
                    Loaded::Ready(library) => self.libs.materials = Some(library),
                    Loaded::Missing => self.libs.materials = Some(fresh(self)),
                    Loaded::Unreadable(_) => {
                        self.libs.materials = Some(fresh(self));
                        self.libs.materials_unreadable = true;
                        effects.push(HostEffect::ShowError {
                            document: None,
                            message: MessageKey::Files(FilesMessage::LibraryUnreadable),
                        });
                    }
                }
                self.libs.state = LoadState::Ready;
            }
            // Nothing could be read (cancelled or failed): try again at the next open.
            _ => self.libs.state = LoadState::NotLoaded,
        }
        AppUpdate { effects, view_changed: true, ..AppUpdate::default() }
    }

    /// Writes the material library when it changed and no write is running.
    pub(super) fn flush_materials(&mut self) -> Vec<HostEffect> {
        let writer = &mut self.libs.material_writer;
        writer.dirty = true;
        if writer.in_flight.is_some() || self.libs.materials_unreadable {
            return Vec::new();
        }
        let (Some(dir), Some(library)) = (self.libs.dir.clone(), self.libs.materials.clone()) else {
            return Vec::new();
        };
        let task = FilesTask::SaveMaterialLibrary {
            library: Box::new(library),
            path: dir.join(MATERIAL_LIBRARY_FILE),
            fs: Arc::clone(&self.ports.fs),
        };
        if self.submit_files(task, FilesJob::SaveMaterials) {
            self.libs.material_writer.dirty = false;
        }
        Vec::new()
    }

    fn material_saved(&mut self, outcome: TaskOutcome) -> AppUpdate {
        self.libs.material_writer.in_flight = None;
        let mut effects = Vec::new();
        match outcome.result {
            Ok(_) => self.libs.material_writer.failed = false,
            Err(TaskError::Cancelled) => {}
            Err(_) => {
                self.libs.material_writer.failed = true;
                effects.push(HostEffect::ShowError {
                    document: None,
                    message: MessageKey::Files(FilesMessage::LibrarySaveFailed),
                });
            }
        }
        if self.libs.material_writer.dirty && !self.libs.material_writer.failed {
            effects.extend(self.flush_materials());
        }
        AppUpdate { effects, view_changed: true, ..AppUpdate::default() }
    }
}

/// What the file panels of one window show.
pub(super) fn files_view(s: &Session, machine: Option<&Machine>, libs: &Libraries) -> FilesViewModel {
    FilesViewModel { material: material::panel_view(s, machine, libs) }
}
