//! The file features of the window (W3-UI-bestanden): import pages, templates, the Material and
//! Art Library panels, bundles and the export dialog.
//!
//! The libraries belong to the app, not to a window: one copy is read once (on a worker), changed
//! on the UI thread and written back on a worker, one write at a time. A library file that cannot
//! be read is never overwritten.

mod art;
mod bundle;
mod export;
mod material;
mod profile_keep;
mod search;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use burnburn_geometry::Point;
use burnburn_io::art_library::ArtLibrary;
use burnburn_io::material_library::MaterialLibrary;
use burnburn_types::{CancellationSource, DocumentId, RevisionId, TaskId};

use super::machine::Machine;
use super::{AppController, AppUpdate, HostEffect, Session};
use crate::files::{ArtThumbnail, FilesIntent, FilesMessage, FilesViewModel};
use crate::files_task::{
    ART_LIBRARY_FILE, FilesOutput, FilesTask, Loaded, MATERIAL_LIBRARY_FILE, PROFILE_LIBRARY_FILE,
};
use crate::task::{TaskError, TaskInput, TaskKind, TaskOutcome, TaskOutput, TaskRequest, revision_key};
use crate::view_model::MessageKey;

/// A worker job of the file features that is waiting for its result.
#[derive(Debug, Clone)]
pub(super) enum FilesJob {
    SaveTemplate {
        document: DocumentId,
    },
    OpenTemplate {
        document: DocumentId,
        path: PathBuf,
    },
    LoadLibraries,
    SaveMaterials,
    SaveArt,
    SaveProfiles,
    /// An item is being added to the art library; kept whole so it can be repeated on a newer library.
    AddArt {
        document: DocumentId,
        request: Box<art::AddRequest>,
    },
    PlaceArt {
        document: DocumentId,
        at: Option<Point>,
    },
    Thumbnails,
    ExportCheck {
        document: DocumentId,
        generation: u64,
    },
    ExportWrite {
        document: DocumentId,
    },
    ReadBundle {
        document: DocumentId,
        serial: u64,
    },
    WriteBundle {
        document: DocumentId,
        serial: u64,
    },
    WriteTemplates {
        document: DocumentId,
        serial: u64,
    },
    /// The project search panel read a folder (the newest `serial` counts).
    SearchProjects {
        document: DocumentId,
        serial: u64,
    },
    /// A frozen revision opens as a window `document`, asked for from window `from`.
    OpenRevision {
        document: DocumentId,
        from: DocumentId,
    },
}

/// Window state of the file panels (never document data).
#[derive(Debug, Default)]
pub(super) struct FilesUi {
    pub(in crate::controller) material: Option<material::MaterialUi>,
    pub(in crate::controller) art: Option<art::ArtUi>,
    pub(in crate::controller) bundle: Option<bundle::BundleUi>,
    pub(in crate::controller) export: Option<export::ExportUi>,
    pub(in crate::controller) search: Option<search::SearchUi>,
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
    art: Option<ArtLibrary>,
    art_unreadable: bool,
    art_writer: Writer,
    thumbnails: BTreeMap<u64, Arc<ArtThumbnail>>,
    /// Counts every change of the art library, so a worker result made from an older copy is noticed.
    art_generation: u64,
    /// Workers that add or place art (the panel shows it is busy).
    art_busy: usize,
    /// Machine profiles a bundle brought in (the active machine is kept by the controller).
    profiles: Vec<Arc<burnburn_job::MachineProfile>>,
    /// The profile library file could not be read: it is kept as it is and never written (P07).
    profiles_unreadable: bool,
    profile_writer: Writer,
    /// Replaced active profiles waiting for the library file to be read before they are merged
    /// into `profiles` and written (P07, `profile_keep`).
    kept: Vec<Arc<burnburn_job::MachineProfile>>,
    /// Colours of the layer palette, slot by slot (the host owns the palette).
    palette: Vec<[u8; 3]>,
}

impl Libraries {
    fn material_persisted(&self) -> bool {
        self.dir.is_some() && !self.materials_unreadable && !self.material_writer.failed
    }

    fn art_persisted(&self) -> bool {
        self.dir.is_some() && !self.art_unreadable && !self.art_writer.failed
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
            intent @ (FilesIntent::OpenArtLibrary { .. }
            | FilesIntent::CloseArtLibrary { .. }
            | FilesIntent::SelectArtGroup { .. }
            | FilesIntent::SetArtSearch { .. }
            | FilesIntent::SelectArtItem { .. }
            | FilesIntent::AddArtGroup { .. }
            | FilesIntent::RenameArtGroup { .. }
            | FilesIntent::RemoveArtGroup { .. }
            | FilesIntent::AddArtFromFile { .. }
            | FilesIntent::AddArtFromSelection { .. }
            | FilesIntent::RenameArtItem { .. }
            | FilesIntent::MoveArtItem { .. }
            | FilesIntent::RemoveArtItem { .. }
            | FilesIntent::PlaceArt { .. }) => self.art_intent(intent),
            intent @ (FilesIntent::OpenSearch { .. }
            | FilesIntent::CloseSearch { .. }
            | FilesIntent::SetSearchFolder { .. }
            | FilesIntent::SetSearchQuery { .. }
            | FilesIntent::RescanSearch { .. }
            | FilesIntent::OpenFoundProject { .. }
            | FilesIntent::OpenFoundRevision { .. }) => self.search_intent(intent),
            intent @ (FilesIntent::OpenExport { .. }
            | FilesIntent::SetExportOptions { .. }
            | FilesIntent::SetExportAcknowledged { .. }
            | FilesIntent::ExportDrawing { .. }
            | FilesIntent::CloseExport { .. }) => self.export_intent(intent),
            intent @ (FilesIntent::OpenBundleExport { .. }
            | FilesIntent::ToggleBundleItem { .. }
            | FilesIntent::SetBundleKind { .. }
            | FilesIntent::ExportBundle { .. }
            | FilesIntent::OpenBundleImport { .. }
            | FilesIntent::SetBundleConflict { .. }
            | FilesIntent::SetBundleProfileReplace { .. }
            | FilesIntent::ApplyBundleImport { .. }
            | FilesIntent::CloseBundle { .. }) => self.bundle_intent(intent),
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
            FilesJob::SaveArt => self.art_saved(outcome),
            FilesJob::SaveProfiles => self.profiles_saved(outcome),
            FilesJob::AddArt { document, request } => self.art_added(document, *request, outcome),
            FilesJob::PlaceArt { document, at } => self.art_placed(document, at, outcome),
            FilesJob::Thumbnails => self.thumbnails_made(outcome),
            FilesJob::ExportCheck { document, generation } => self.export_checked(document, generation, outcome),
            FilesJob::ExportWrite { document } => self.export_written(document, outcome),
            FilesJob::ReadBundle { document, serial } => self.bundle_read(document, serial, outcome),
            FilesJob::WriteBundle { document, serial } => self.bundle_written(document, serial, outcome),
            FilesJob::WriteTemplates { document, serial } => self.bundle_templates_written(document, serial, outcome),
            FilesJob::SearchProjects { document, serial } => self.search_scanned(document, serial, outcome),
            FilesJob::OpenRevision { document, from } => self.search_revision_opened(document, from, outcome),
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
            self.libs.art = Some(ArtLibrary::new(&self.new_library_id()));
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
        match job {
            FilesJob::SaveMaterials => self.libs.material_writer.in_flight = Some(id),
            FilesJob::SaveArt => self.libs.art_writer.in_flight = Some(id),
            FilesJob::SaveProfiles => self.libs.profile_writer.in_flight = Some(id),
            _ => {}
        }
        self.files_jobs.insert(id, job);
        true
    }

    fn libraries_loaded(&mut self, outcome: TaskOutcome) -> AppUpdate {
        let mut effects = Vec::new();
        match outcome.result {
            Ok(TaskOutput::Files(FilesOutput::LibrariesLoaded { profiles, materials, art, thumbnails })) => {
                match profiles {
                    // Profiles of this session (an import before the first read) stay; the file's win on equal ids.
                    Loaded::Ready(stored) => {
                        let session = std::mem::take(&mut self.libs.profiles);
                        self.libs.profiles = stored.into_iter().map(Arc::new).collect();
                        for p in session {
                            if !self.libs.profiles.iter().any(|k| k.revision().id == p.revision().id) {
                                self.libs.profiles.push(p);
                            }
                        }
                    }
                    Loaded::Missing => {}
                    Loaded::Unreadable(_) => {
                        self.libs.profiles_unreadable = true;
                        effects.push(unreadable());
                    }
                }
                match materials {
                    Loaded::Ready(library) => self.libs.materials = Some(library),
                    Loaded::Missing => self.libs.materials = Some(MaterialLibrary::new(&self.new_library_id())),
                    Loaded::Unreadable(_) => {
                        self.libs.materials = Some(MaterialLibrary::new(&self.new_library_id()));
                        self.libs.materials_unreadable = true;
                        effects.push(unreadable());
                    }
                }
                match art {
                    Loaded::Ready(library) => self.libs.art = Some(library),
                    Loaded::Missing => self.libs.art = Some(ArtLibrary::new(&self.new_library_id())),
                    Loaded::Unreadable(_) => {
                        self.libs.art = Some(ArtLibrary::new(&self.new_library_id()));
                        self.libs.art_unreadable = true;
                        effects.push(unreadable());
                    }
                }
                self.libs.thumbnails = thumbnails.into_iter().map(|(id, t)| (id, Arc::new(t))).collect();
                self.libs.state = LoadState::Ready;
                self.merge_kept_profiles();
            }
            // Nothing could be read (cancelled or failed): try again at the next open.
            _ => self.libs.state = LoadState::NotLoaded,
        }
        AppUpdate { effects, view_changed: true, ..AppUpdate::default() }
    }

    /// Writes the material library when it changed and no write is running.
    pub(super) fn flush_materials(&mut self) -> Vec<HostEffect> {
        self.libs.material_writer.dirty = true;
        if self.libs.material_writer.in_flight.is_some() || self.libs.materials_unreadable {
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

    /// Writes the profiles a bundle brought in when they changed and no write is running (P07).
    pub(super) fn flush_profiles(&mut self) -> Vec<HostEffect> {
        self.libs.profile_writer.dirty = true;
        if self.libs.profile_writer.in_flight.is_some() || self.libs.profiles_unreadable {
            return Vec::new();
        }
        let Some(dir) = self.libs.dir.clone() else { return Vec::new() };
        let task = FilesTask::SaveProfileLibrary {
            profiles: self.libs.profiles.iter().map(|p| (**p).clone()).collect(),
            path: dir.join(PROFILE_LIBRARY_FILE),
            fs: Arc::clone(&self.ports.fs),
        };
        if self.submit_files(task, FilesJob::SaveProfiles) {
            self.libs.profile_writer.dirty = false;
        }
        Vec::new()
    }

    fn profiles_saved(&mut self, outcome: TaskOutcome) -> AppUpdate {
        self.libs.profile_writer.in_flight = None;
        let mut effects = Vec::new();
        match outcome.result {
            Ok(_) => self.libs.profile_writer.failed = false,
            Err(TaskError::Cancelled) => {}
            Err(_) => {
                self.libs.profile_writer.failed = true;
                effects.push(save_failed());
            }
        }
        if self.libs.profile_writer.dirty && !self.libs.profile_writer.failed {
            effects.extend(self.flush_profiles());
        }
        AppUpdate { effects, view_changed: true, ..AppUpdate::default() }
    }

    /// Writes the art library when it changed and no write is running.
    pub(super) fn flush_art(&mut self) -> Vec<HostEffect> {
        self.libs.art_writer.dirty = true;
        if self.libs.art_writer.in_flight.is_some() || self.libs.art_unreadable {
            return Vec::new();
        }
        let (Some(dir), Some(library)) = (self.libs.dir.clone(), self.libs.art.clone()) else {
            return Vec::new();
        };
        let task = FilesTask::SaveArtLibrary {
            library: Box::new(library),
            path: dir.join(ART_LIBRARY_FILE),
            fs: Arc::clone(&self.ports.fs),
        };
        if self.submit_files(task, FilesJob::SaveArt) {
            self.libs.art_writer.dirty = false;
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
                effects.push(save_failed());
            }
        }
        if self.libs.material_writer.dirty && !self.libs.material_writer.failed {
            effects.extend(self.flush_materials());
        }
        AppUpdate { effects, view_changed: true, ..AppUpdate::default() }
    }

    fn art_saved(&mut self, outcome: TaskOutcome) -> AppUpdate {
        self.libs.art_writer.in_flight = None;
        let mut effects = Vec::new();
        match outcome.result {
            Ok(_) => self.libs.art_writer.failed = false,
            Err(TaskError::Cancelled) => {}
            Err(_) => {
                self.libs.art_writer.failed = true;
                effects.push(save_failed());
            }
        }
        if self.libs.art_writer.dirty && !self.libs.art_writer.failed {
            effects.extend(self.flush_art());
        }
        AppUpdate { effects, view_changed: true, ..AppUpdate::default() }
    }
}

fn unreadable() -> HostEffect {
    HostEffect::ShowError { document: None, message: MessageKey::Files(FilesMessage::LibraryUnreadable) }
}

fn save_failed() -> HostEffect {
    HostEffect::ShowError { document: None, message: MessageKey::Files(FilesMessage::LibrarySaveFailed) }
}

/// What the file panels of one window show.
pub(super) fn files_view(
    s: &Session,
    machine: Option<&Machine>,
    libs: &Libraries,
    recent_templates: &[PathBuf],
) -> FilesViewModel {
    FilesViewModel {
        material: material::panel_view(s, machine, libs),
        art: art::panel_view(s, libs),
        bundle: bundle::panel_view(s, machine, libs, recent_templates),
        export: export::panel_view(s),
        search: search::panel_view(s),
    }
}
