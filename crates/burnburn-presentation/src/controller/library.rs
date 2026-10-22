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
    ACTIVE_MACHINE_FILE, ART_LIBRARY_FILE, CALIBRATION_LIBRARY_FILE, CAMERA_CALIBRATION_FILE, FilesOutput, FilesTask,
    Loaded, MATERIAL_LIBRARY_FILE, PROFILE_LIBRARY_FILE,
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
    /// A machine profile file chosen by the host is being read.
    OpenMachineProfile,
    SaveMaterials,
    SaveArt,
    SaveProfiles,
    SaveCalibration,
    /// The camera calibrations (lens, workspace alignment) are being written (W04).
    SaveCameraCalibrations,
    /// The machine in use is being written (M01).
    SaveActiveMachine,
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
    /// What belongs to those machines besides the profile (GRBL dialect, declared model and
    /// firmware), by machine id. No entry: a Ruida machine without a declaration.
    extras: BTreeMap<String, super::machine::Extras>,
    /// The profile library file could not be read: it is kept as it is and never written (P07).
    profiles_unreadable: bool,
    profile_writer: Writer,
    /// The calibration file could not be read: it is kept as it is and never written (W09).
    calibration_unreadable: bool,
    calibration_writer: Writer,
    /// The camera calibration file could not be read: it is kept as it is and never written (W04).
    camera_calibrations_unreadable: bool,
    camera_writer: Writer,
    /// The active-machine file could not be read: it is kept as it is and never written (M01).
    active_unreadable: bool,
    active_writer: Writer,
    /// The machine as it was last handed to the writer (or read), to see when it changed.
    active_seen: Option<super::machine::ActiveSeen>,
    /// Replaced active profiles waiting for the library file to be read before they are merged
    /// into `profiles` and written (P07, `profile_keep`).
    kept: Vec<(Arc<burnburn_job::MachineProfile>, super::machine::Extras)>,
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
            FilesJob::OpenMachineProfile => self.machine_profile_read(outcome),
            FilesJob::SaveMaterials => self.material_saved(outcome),
            FilesJob::SaveArt => self.art_saved(outcome),
            FilesJob::SaveProfiles => self.profiles_saved(outcome),
            FilesJob::SaveCalibration => self.calibration_saved(outcome),
            FilesJob::SaveCameraCalibrations => self.camera_calibrations_saved(outcome),
            FilesJob::SaveActiveMachine => self.active_machine_saved(outcome),
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
        // The libraries are read at once: the machine of the last start and the camera calibrations
        // are wanted before any panel asks for them (M01, W04).
        self.ensure_libraries_if_folder();
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

    /// Reads the libraries when there is a folder to read them from (the hosts set it before they
    /// attach the camera); without one nothing is decided yet.
    pub(super) fn ensure_libraries_if_folder(&mut self) {
        if self.libs.dir.is_some() {
            self.ensure_libraries();
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
            FilesJob::SaveCalibration => self.libs.calibration_writer.in_flight = Some(id),
            FilesJob::SaveCameraCalibrations => self.libs.camera_writer.in_flight = Some(id),
            FilesJob::SaveActiveMachine => self.libs.active_writer.in_flight = Some(id),
            _ => {}
        }
        self.files_jobs.insert(id, job);
        true
    }

    fn libraries_loaded(&mut self, outcome: TaskOutcome) -> AppUpdate {
        let mut effects = Vec::new();
        match outcome.result {
            Ok(TaskOutput::Files(FilesOutput::LibrariesLoaded {
                profiles,
                calibration,
                camera_calibrations,
                active_machine,
                materials,
                art,
                thumbnails,
            })) => {
                match active_machine {
                    Loaded::Ready(entries) => self.restore_active_machine(entries),
                    Loaded::Missing => {}
                    Loaded::Unreadable(_) => {
                        self.libs.active_unreadable = true;
                        effects.push(unreadable());
                    }
                }
                match camera_calibrations {
                    Loaded::Ready(stored) => {
                        if self.camera_calibrations_loaded(stored) {
                            self.libs.camera_writer.dirty = true;
                        }
                    }
                    Loaded::Missing => {}
                    Loaded::Unreadable(_) => {
                        self.libs.camera_calibrations_unreadable = true;
                        effects.push(unreadable());
                    }
                }
                match calibration {
                    Loaded::Ready(library) => {
                        if self.proofs_loaded(library) {
                            self.libs.calibration_writer.dirty = true;
                        }
                    }
                    Loaded::Missing => {}
                    Loaded::Unreadable(_) => {
                        self.libs.calibration_unreadable = true;
                        effects.push(unreadable());
                    }
                }
                match profiles {
                    // Profiles of this session (an import before the first read) stay; the file's win on equal ids.
                    Loaded::Ready(stored) => {
                        let session = std::mem::take(&mut self.libs.profiles);
                        let session_extras = std::mem::take(&mut self.libs.extras);
                        for entry in stored {
                            let extras = super::machine::Extras {
                                grbl: entry.grbl,
                                declared: super::machine::Declared { model: entry.model, firmware: entry.firmware },
                            };
                            self.libs.extras.insert(entry.profile.revision().id.clone(), extras);
                            self.libs.profiles.push(Arc::new(entry.profile));
                        }
                        for p in session {
                            if !self.libs.profiles.iter().any(|k| k.revision().id == p.revision().id) {
                                if let Some(extras) = session_extras.get(&p.revision().id) {
                                    self.libs.extras.insert(p.revision().id.clone(), extras.clone());
                                }
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
                // Proofs recorded before the file was read wait for it, then both are written.
                if self.libs.calibration_writer.dirty {
                    effects.extend(self.flush_calibration());
                }
                if self.libs.camera_writer.dirty {
                    effects.extend(self.flush_camera_calibrations());
                }
                self.refresh_calibration();
                self.rebuild_layers();
                self.persist_active_machine();
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
            profiles: self
                .libs
                .profiles
                .iter()
                .map(|p| {
                    let extras = self.libs.extras.get(&p.revision().id).cloned().unwrap_or_default();
                    burnburn_io::profile_library::LibraryProfile {
                        profile: (**p).clone(),
                        grbl: extras.grbl,
                        model: extras.declared.model,
                        firmware: extras.declared.firmware,
                    }
                })
                .collect(),
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

    /// Writes the calibration proofs when they changed and no write is running. Nothing is written
    /// before the file has been read (it would drop what is in it) or when it could not be read.
    pub(super) fn flush_calibration(&mut self) -> Vec<HostEffect> {
        self.libs.calibration_writer.dirty = true;
        self.ensure_libraries();
        if self.libs.state != LoadState::Ready
            || self.libs.calibration_writer.in_flight.is_some()
            || self.libs.calibration_unreadable
        {
            return Vec::new();
        }
        let Some(dir) = self.libs.dir.clone() else { return Vec::new() };
        let task = FilesTask::SaveCalibrationLibrary {
            library: Box::new(self.calibration_file()),
            path: dir.join(CALIBRATION_LIBRARY_FILE),
            fs: Arc::clone(&self.ports.fs),
        };
        if self.submit_files(task, FilesJob::SaveCalibration) {
            self.libs.calibration_writer.dirty = false;
        }
        Vec::new()
    }

    /// Writes the camera calibrations when they changed and no write is running. Nothing is written
    /// before the file has been read (it would drop what is in it) or when it could not be read.
    pub(super) fn flush_camera_calibrations(&mut self) -> Vec<HostEffect> {
        self.libs.camera_writer.dirty = true;
        self.ensure_libraries();
        if self.libs.state != LoadState::Ready
            || self.libs.camera_writer.in_flight.is_some()
            || self.libs.camera_calibrations_unreadable
        {
            return Vec::new();
        }
        let Some(dir) = self.libs.dir.clone() else { return Vec::new() };
        let task = FilesTask::SaveCameraCalibrations {
            store: Box::new(self.camera_ws.store.clone()),
            path: dir.join(CAMERA_CALIBRATION_FILE),
            fs: Arc::clone(&self.ports.fs),
        };
        if self.submit_files(task, FilesJob::SaveCameraCalibrations) {
            self.libs.camera_writer.dirty = false;
        }
        Vec::new()
    }

    /// What was read from the camera calibration file joins what the session already holds; the
    /// session's entries win for the same camera configuration (they are newer). True when the
    /// session held something the file did not, so the file has to be written again.
    fn camera_calibrations_loaded(&mut self, stored: burnburn_image::calib::CalibrationStore) -> bool {
        let session = std::mem::replace(&mut self.camera_ws.store, stored);
        let mut newer = false;
        for lens in session.lenses() {
            newer |= self.camera_ws.store.lens_for(&lens.valid_for).map_or(true, |kept| kept != lens);
            self.camera_ws.store.insert_lens(lens.clone());
        }
        for alignment in session.alignments() {
            newer |= self
                .camera_ws
                .store
                .alignment_for(alignment.config(), alignment.material_height_mm)
                .map_or(true, |kept| kept != alignment);
            self.camera_ws.store.insert_alignment(alignment.clone());
        }
        newer
    }

    /// The machine of the last start, read from its file: it becomes the active machine unless one was
    /// chosen before the file was read (that choice is newer and is written instead).
    fn restore_active_machine(&mut self, entries: Vec<burnburn_io::profile_library::LibraryProfile>) {
        let Some(entry) = entries.into_iter().next() else { return };
        let machine = super::machine::Machine {
            profile: Arc::new(entry.profile),
            // The wire dialect details of a Ruida controller are the host's; the neutral default is
            // what choosing a machine from the library uses too.
            ruida: burnburn_ruida::RuidaProfile {
                magic: burnburn_ruida::Magic::FIXTURE_0X88,
                overscan: burnburn_ruida::OverscanPolicy::ControllerManaged,
            },
            grbl: entry.grbl,
            declared: super::machine::Declared::of(
                entry.model.as_deref().unwrap_or_default(),
                entry.firmware.as_deref().unwrap_or_default(),
            ),
        };
        if self.machine.is_none() {
            self.libs.active_seen = Some(super::machine::ActiveSeen::of(&machine));
            self.replace_active_machine(machine);
        }
    }

    /// Writes the machine in use when it changed since it was last written (or read) and no write is
    /// running. Nothing is written before the file was read, or when it could not be read.
    pub(super) fn persist_active_machine(&mut self) {
        let Some(machine) = self.machine.as_ref() else { return };
        if self.libs.dir.is_none() || self.libs.active_unreadable {
            return;
        }
        let unchanged = self.libs.active_seen.as_ref().is_some_and(|seen| seen.is(machine));
        if unchanged && !self.libs.active_writer.dirty {
            return;
        }
        if self.libs.state != LoadState::Ready {
            // Written once the libraries were read (`libraries_loaded` asks again).
            self.ensure_libraries();
            return;
        }
        if self.libs.active_writer.in_flight.is_some() {
            self.libs.active_writer.dirty = true;
            return;
        }
        let Some(dir) = self.libs.dir.clone() else { return };
        let entry = burnburn_io::profile_library::LibraryProfile {
            profile: (*machine.profile).clone(),
            grbl: machine.grbl.clone(),
            model: machine.declared.model.clone(),
            firmware: machine.declared.firmware.clone(),
        };
        let seen = super::machine::ActiveSeen::of(machine);
        let task = FilesTask::SaveActiveMachine {
            entry: Box::new(entry),
            path: dir.join(ACTIVE_MACHINE_FILE),
            fs: Arc::clone(&self.ports.fs),
        };
        if self.submit_files(task, FilesJob::SaveActiveMachine) {
            self.libs.active_seen = Some(seen);
            self.libs.active_writer.dirty = false;
        }
    }

    fn active_machine_saved(&mut self, outcome: TaskOutcome) -> AppUpdate {
        self.libs.active_writer.in_flight = None;
        let mut effects = Vec::new();
        match outcome.result {
            Ok(_) => self.libs.active_writer.failed = false,
            Err(TaskError::Cancelled) => {}
            Err(_) => {
                self.libs.active_writer.failed = true;
                effects.push(save_failed());
            }
        }
        if !self.libs.active_writer.failed {
            // The machine may have changed while the write ran.
            self.persist_active_machine();
        }
        AppUpdate { effects, view_changed: true, ..AppUpdate::default() }
    }

    fn camera_calibrations_saved(&mut self, outcome: TaskOutcome) -> AppUpdate {
        self.libs.camera_writer.in_flight = None;
        let mut effects = Vec::new();
        match outcome.result {
            Ok(_) => self.libs.camera_writer.failed = false,
            Err(TaskError::Cancelled) => {}
            Err(_) => {
                self.libs.camera_writer.failed = true;
                effects.push(save_failed());
            }
        }
        if self.libs.camera_writer.dirty && !self.libs.camera_writer.failed {
            effects.extend(self.flush_camera_calibrations());
        }
        AppUpdate { effects, view_changed: true, ..AppUpdate::default() }
    }

    fn calibration_saved(&mut self, outcome: TaskOutcome) -> AppUpdate {
        self.libs.calibration_writer.in_flight = None;
        let mut effects = Vec::new();
        match outcome.result {
            Ok(_) => self.libs.calibration_writer.failed = false,
            Err(TaskError::Cancelled) => {}
            Err(_) => {
                self.libs.calibration_writer.failed = true;
                effects.push(save_failed());
            }
        }
        if self.libs.calibration_writer.dirty && !self.libs.calibration_writer.failed {
            effects.extend(self.flush_calibration());
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
