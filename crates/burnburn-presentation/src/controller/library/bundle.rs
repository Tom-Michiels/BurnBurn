//! Bundles (M032/M033, P07): choose what goes in, and see what an incoming bundle would change
//! before it changes anything.
//!
//! Importing is two steps. Reading the bundle changes nothing; the dialog shows, per item, whether
//! it is new, identical or in conflict, and for machine profiles the names of the differing
//! fields. `ApplyBundleImport` then changes copies of the libraries and swaps them in only when
//! the whole import worked. A profile that differs locally is replaced only when the user said so.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use burnburn_io::bundle::{
    Bundle, BundleTemplate, Change, ConflictChoice, ImportChoices, ImportPreview, ImportTarget, LocalState,
    ProfileChange, apply_import, preview_import,
};
use burnburn_job::MachineProfile;
use burnburn_types::DocumentId;

use super::{AppController, AppUpdate, FilesJob, Libraries, LoadState};
use crate::controller::Session;
use crate::controller::machine::Machine;
use crate::field::format_mm;
use crate::files::{
    BundleChange, BundleChoice, BundleConflict, BundleDialogViewModel, BundleExportView, BundleImportView, BundleItem,
    BundleKind, BundleMode, BundleProfileChange, BundleProfileRow, BundleReport, BundleRow, FilesIntent, FilesMessage,
};
use crate::files_task::{FilesOutput, FilesTask, WriteBundle};
use crate::task::{TaskError, TaskOutcome, TaskOutput};
use crate::view_model::MessageKey;

/// Folder of the templates a bundle brings in, inside the library folder.
const TEMPLATES_DIR: &str = "templates";

/// Window state of the dialog.
#[derive(Debug)]
pub(in crate::controller) struct BundleUi {
    /// Identifies this opening of the dialog: a worker result for an older one is dropped.
    serial: u64,
    busy: bool,
    issue: Option<MessageKey>,
    state: State,
}

#[derive(Debug)]
enum State {
    Export(ExportState),
    Import(ImportState),
}

#[derive(Debug, Default)]
struct ExportState {
    materials: BTreeSet<u64>,
    art: BTreeSet<u64>,
    profiles: BTreeSet<String>,
    templates: BTreeSet<String>,
    written: Option<PathBuf>,
}

#[derive(Debug)]
struct ImportState {
    file: String,
    data: Option<ImportData>,
    conflict: BundleConflict,
    replace_profiles: BTreeSet<String>,
    report: Option<BundleReport>,
}

#[derive(Debug)]
struct ImportData {
    bundle: Arc<Bundle>,
    local_templates: Vec<BundleTemplate>,
}

fn flip<T: Ord>(set: &mut BTreeSet<T>, id: T, selected: bool) {
    if selected {
        set.insert(id);
    } else {
        set.remove(&id);
    }
}

fn template_name(path: &Path) -> Option<String> {
    path.file_stem().map(|n| n.to_string_lossy().into_owned())
}

fn choice_of(c: BundleConflict) -> ConflictChoice {
    match c {
        BundleConflict::Skip => ConflictChoice::Skip,
        BundleConflict::Replace => ConflictChoice::Replace,
        BundleConflict::KeepBoth => ConflictChoice::KeepBoth,
    }
}

fn change_of(c: Change) -> BundleChange {
    match c {
        Change::New => BundleChange::New,
        Change::Identical => BundleChange::Identical,
        _ => BundleChange::Conflict,
    }
}

impl AppController {
    /// The machine profiles this app knows: the active one first, then those a bundle brought in.
    pub fn known_profiles(&self) -> Vec<Arc<MachineProfile>> {
        let mut out: Vec<Arc<MachineProfile>> = self.machine.iter().map(|m| Arc::clone(&m.profile)).collect();
        for p in &self.libs.profiles {
            if !out.iter().any(|k| k.revision().id == p.revision().id) {
                out.push(Arc::clone(p));
            }
        }
        out
    }

    pub(super) fn bundle_intent(&mut self, intent: FilesIntent) -> AppUpdate {
        match intent {
            FilesIntent::OpenBundleExport { document } => {
                self.ensure_libraries();
                self.open_bundle(document, State::Export(ExportState::default()))
            }
            FilesIntent::CloseBundle { document } => self.with_session(document, |s| {
                s.files.bundle = None;
                AppUpdate { view_changed: true, ..AppUpdate::default() }
            }),
            FilesIntent::ToggleBundleItem { document, item, selected } => self.with_export(document, |e| match item {
                BundleItem::Material(id) => flip(&mut e.materials, id, selected),
                BundleItem::Art(id) => flip(&mut e.art, id, selected),
                BundleItem::Profile(id) => flip(&mut e.profiles, id, selected),
                BundleItem::Template(name) => flip(&mut e.templates, name, selected),
            }),
            FilesIntent::SetBundleKind { document, kind, selected } => {
                let materials: BTreeSet<u64> =
                    self.libs.materials.iter().flat_map(|l| l.entries().iter().map(|e| e.id())).collect();
                let art: BTreeSet<u64> = self.libs.art.iter().flat_map(|l| l.items().iter().map(|i| i.id())).collect();
                let profiles: BTreeSet<String> =
                    self.known_profiles().iter().map(|p| p.revision().id.clone()).collect();
                let templates: BTreeSet<String> =
                    self.recent_templates.iter().filter_map(|p| template_name(p)).collect();
                self.with_export(document, |e| match kind {
                    BundleKind::Materials => e.materials = if selected { materials } else { BTreeSet::new() },
                    BundleKind::Art => e.art = if selected { art } else { BTreeSet::new() },
                    BundleKind::Profiles => e.profiles = if selected { profiles } else { BTreeSet::new() },
                    BundleKind::Templates => e.templates = if selected { templates } else { BTreeSet::new() },
                })
            }
            FilesIntent::ExportBundle { document, path } => self.export_bundle(document, path),
            FilesIntent::OpenBundleImport { document, path } => {
                self.ensure_libraries();
                let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let state = State::Import(ImportState {
                    file,
                    data: None,
                    conflict: BundleConflict::default(),
                    replace_profiles: BTreeSet::new(),
                    report: None,
                });
                let update = self.open_bundle(document, state);
                let serial = self.sessions.get(&document).and_then(|s| s.files.bundle.as_ref()).map(|b| b.serial);
                let Some(serial) = serial else { return update };
                let task = FilesTask::ReadBundle {
                    path,
                    templates_dir: self.libs.dir.as_ref().map(|d| d.join(TEMPLATES_DIR)),
                    fs: Arc::clone(&self.ports.fs),
                };
                self.submit_files(task, FilesJob::ReadBundle { document, serial });
                update
            }
            FilesIntent::SetBundleConflict { document, choice } => self.with_import(document, |i| {
                i.conflict = choice;
            }),
            FilesIntent::SetBundleProfileReplace { document, profile, replace } => self.with_import(document, |i| {
                if replace {
                    i.replace_profiles.insert(profile);
                } else {
                    i.replace_profiles.remove(&profile);
                }
            }),
            FilesIntent::ApplyBundleImport { document } => self.apply_bundle_import(document),
            _ => AppUpdate::default(),
        }
    }

    fn open_bundle(&mut self, document: DocumentId, state: State) -> AppUpdate {
        self.bundle_serial += 1;
        let serial = self.bundle_serial;
        self.with_session(document, |s| {
            s.files.bundle = Some(BundleUi { serial, busy: false, issue: None, state });
            AppUpdate { view_changed: true, ..AppUpdate::default() }
        })
    }

    fn with_export(&mut self, document: DocumentId, f: impl FnOnce(&mut ExportState)) -> AppUpdate {
        self.with_session(document, |s| match s.files.bundle.as_mut() {
            Some(BundleUi { state: State::Export(e), issue, .. }) => {
                f(e);
                *issue = None;
                e.written = None;
                AppUpdate { view_changed: true, ..AppUpdate::default() }
            }
            _ => AppUpdate::default(),
        })
    }

    fn with_import(&mut self, document: DocumentId, f: impl FnOnce(&mut ImportState)) -> AppUpdate {
        self.with_session(document, |s| match s.files.bundle.as_mut() {
            Some(BundleUi { state: State::Import(i), issue, .. }) if i.report.is_none() => {
                f(i);
                *issue = None;
                AppUpdate { view_changed: true, ..AppUpdate::default() }
            }
            _ => AppUpdate::default(),
        })
    }

    fn bundle_issue(&mut self, document: DocumentId, message: FilesMessage) -> AppUpdate {
        self.with_session(document, |s| {
            if let Some(b) = s.files.bundle.as_mut() {
                b.issue = Some(MessageKey::Files(message));
                b.busy = false;
            }
            AppUpdate { view_changed: true, ..AppUpdate::default() }
        })
    }

    fn export_bundle(&mut self, document: DocumentId, path: PathBuf) -> AppUpdate {
        let Some(BundleUi { state: State::Export(e), serial, .. }) =
            self.sessions.get(&document).and_then(|s| s.files.bundle.as_ref())
        else {
            return AppUpdate::default();
        };
        let serial = *serial;
        let (Some(materials), Some(art)) = (self.libs.materials.as_ref(), self.libs.art.as_ref()) else {
            return AppUpdate::default();
        };
        let materials =
            (!e.materials.is_empty()).then(|| materials.subset(&e.materials.iter().copied().collect::<Vec<_>>()));
        let art = (!e.art.is_empty()).then(|| art.subset(&e.art.iter().copied().collect::<Vec<_>>()));
        let profiles: Vec<MachineProfile> = self
            .known_profiles()
            .iter()
            .filter(|p| e.profiles.contains(&p.revision().id))
            .map(|p| (**p).clone())
            .collect();
        let mut templates: Vec<(String, PathBuf)> = Vec::new();
        for file in &self.recent_templates {
            if let Some(name) = template_name(file)
                && e.templates.contains(&name)
                && !templates.iter().any(|(n, _)| *n == name)
            {
                templates.push((name, file.clone()));
            }
        }
        if materials.is_none() && art.is_none() && profiles.is_empty() && templates.is_empty() {
            return self.bundle_issue(document, FilesMessage::BundleNothingSelected);
        }
        let task = FilesTask::WriteBundle(Box::new(WriteBundle {
            materials,
            art,
            profiles,
            templates,
            path,
            fs: Arc::clone(&self.ports.fs),
        }));
        self.submit_files(task, FilesJob::WriteBundle { document, serial });
        self.with_session(document, |s| {
            if let Some(b) = s.files.bundle.as_mut() {
                b.busy = true;
                b.issue = None;
            }
            AppUpdate { view_changed: true, ..AppUpdate::default() }
        })
    }

    pub(super) fn bundle_read(&mut self, document: DocumentId, serial: u64, outcome: TaskOutcome) -> AppUpdate {
        let result = outcome.result;
        self.with_session(document, |s| {
            let Some(b) = s.files.bundle.as_mut().filter(|b| b.serial == serial) else { return AppUpdate::default() };
            let State::Import(import) = &mut b.state else { return AppUpdate::default() };
            match result {
                Ok(TaskOutput::Files(FilesOutput::BundleRead { bundle, local_templates })) => {
                    import.data = Some(ImportData { bundle: Arc::new(*bundle), local_templates });
                }
                Err(TaskError::Cancelled) => {}
                _ => b.issue = Some(MessageKey::Files(FilesMessage::BundleUnreadable)),
            }
            AppUpdate { view_changed: true, ..AppUpdate::default() }
        })
    }

    pub(super) fn bundle_written(&mut self, document: DocumentId, serial: u64, outcome: TaskOutcome) -> AppUpdate {
        let result = outcome.result;
        self.with_session(document, |s| {
            let Some(b) = s.files.bundle.as_mut().filter(|b| b.serial == serial) else { return AppUpdate::default() };
            b.busy = false;
            match (result, &mut b.state) {
                (Ok(TaskOutput::Files(FilesOutput::BundleWritten { path })), State::Export(e)) => {
                    e.written = Some(path)
                }
                (Err(TaskError::Cancelled), _) => {}
                _ => b.issue = Some(MessageKey::Files(FilesMessage::BundleWriteFailed)),
            }
            AppUpdate { view_changed: true, ..AppUpdate::default() }
        })
    }

    pub(super) fn bundle_templates_written(
        &mut self,
        document: DocumentId,
        serial: u64,
        outcome: TaskOutcome,
    ) -> AppUpdate {
        let result = outcome.result;
        self.with_session(document, |s| {
            let Some(b) = s.files.bundle.as_mut().filter(|b| b.serial == serial) else { return AppUpdate::default() };
            b.busy = false;
            match (result, &mut b.state) {
                (Ok(TaskOutput::Files(FilesOutput::TemplatesWritten { count })), State::Import(i)) => {
                    if let Some(report) = i.report.as_mut() {
                        report.templates_written = count;
                    }
                }
                (Err(TaskError::Cancelled), _) => {}
                _ => b.issue = Some(MessageKey::Files(FilesMessage::BundleWriteFailed)),
            }
            AppUpdate { view_changed: true, ..AppUpdate::default() }
        })
    }

    /// Thumbnails of art items a bundle brought in.
    pub(super) fn thumbnails_made(&mut self, outcome: TaskOutcome) -> AppUpdate {
        if let Ok(TaskOutput::Files(FilesOutput::ArtThumbnails { thumbnails })) = outcome.result {
            for (id, t) in thumbnails {
                self.libs.thumbnails.insert(id, Arc::new(t));
            }
        }
        AppUpdate { view_changed: true, ..AppUpdate::default() }
    }

    fn apply_bundle_import(&mut self, document: DocumentId) -> AppUpdate {
        let Some(BundleUi { state: State::Import(import), serial, .. }) =
            self.sessions.get(&document).and_then(|s| s.files.bundle.as_ref())
        else {
            return AppUpdate::default();
        };
        let (serial, conflict, replace) = (*serial, import.conflict, import.replace_profiles.clone());
        let Some(data) = import.data.as_ref().filter(|_| import.report.is_none()) else { return AppUpdate::default() };
        let (bundle, local_templates) = (Arc::clone(&data.bundle), data.local_templates.clone());
        let (Some(mut materials), Some(mut art)) = (self.libs.materials.clone(), self.libs.art.clone()) else {
            return AppUpdate::default();
        };
        let profiles: Vec<MachineProfile> = self.known_profiles().iter().map(|p| (**p).clone()).collect();
        let choices = ImportChoices { conflicts: choice_of(conflict), replace_profiles: replace };
        let target =
            ImportTarget { materials: &mut materials, art: &mut art, profiles: &profiles, templates: &local_templates };
        let Ok(outcome) = apply_import(&bundle, target, &choices) else {
            return self.bundle_issue(document, FilesMessage::BundleApplyFailed);
        };

        // The whole import worked: swap the changed copies in.
        let mut effects = Vec::new();
        if !outcome.materials_added.is_empty() || !outcome.materials_replaced.is_empty() {
            self.libs.materials = Some(materials);
            effects.extend(self.flush_materials());
        }
        let new_art: Vec<u64> = outcome.art_added.clone();
        if !outcome.art_added.is_empty() || !outcome.art_replaced.is_empty() {
            self.libs.art_generation += 1;
            for gone in &outcome.art_replaced {
                self.libs.thumbnails.remove(gone);
            }
            self.libs.art = Some(art);
            effects.extend(self.flush_art());
            if let Some(library) = self.libs.art.as_ref().filter(|_| !new_art.is_empty()) {
                let task = FilesTask::ArtThumbnails { library: Box::new(library.subset(&new_art)), items: new_art };
                self.submit_files(task, FilesJob::Thumbnails);
            }
        }
        let mut report = BundleReport {
            materials_added: outcome.materials_added.len(),
            materials_replaced: outcome.materials_replaced.len(),
            art_added: outcome.art_added.len(),
            art_replaced: outcome.art_replaced.len(),
            templates_written: 0,
            profiles_added: outcome.profiles_to_add.len(),
            profiles_replaced: outcome.profiles_to_replace.len(),
            profiles_pending: outcome.profiles_pending.clone(),
            skipped: outcome.skipped,
        };
        let profiles_changed = !outcome.profiles_to_add.is_empty();
        for p in outcome.profiles_to_add {
            self.libs.profiles.push(Arc::new(p));
        }
        if profiles_changed {
            self.flush_profiles();
        }
        for p in outcome.profiles_to_replace {
            let id = p.revision().id.clone();
            let p = Arc::new(p);
            match self.machine.as_ref().filter(|m| m.profile.revision().id == id) {
                // The active machine: existing plans become stale, exactly as when the profile is
                // chosen; the revision that is replaced stays in the profile library (P07).
                Some(active) => {
                    let ruida = active.ruida;
                    self.replace_active_machine(super::super::machine::Machine { profile: p, ruida });
                }
                None => {
                    match self.libs.profiles.iter_mut().find(|k| k.revision().id == id) {
                        Some(slot) => *slot = p,
                        None => self.libs.profiles.push(p),
                    }
                    self.flush_profiles();
                }
            }
        }
        let mut write_templates = None;
        if !outcome.templates_to_write.is_empty() {
            match self.libs.dir.clone() {
                Some(dir) => {
                    write_templates = Some(FilesTask::WriteTemplates {
                        dir: dir.join(TEMPLATES_DIR),
                        templates: outcome.templates_to_write,
                        fs: Arc::clone(&self.ports.fs),
                    });
                }
                None => report.skipped += outcome.templates_to_write.len(),
            }
        }
        let busy = write_templates.is_some();
        if let Some(task) = write_templates {
            self.submit_files(task, FilesJob::WriteTemplates { document, serial });
        }
        let mut update = self.with_session(document, |s| {
            if let Some(b) = s.files.bundle.as_mut()
                && let State::Import(i) = &mut b.state
            {
                i.report = Some(report);
                b.busy = busy;
                b.issue = None;
            }
            AppUpdate { view_changed: true, ..AppUpdate::default() }
        });
        update.effects.extend(effects);
        update
    }
}

fn labelled<T: Clone>(
    items: impl IntoIterator<Item = (T, String)>,
    selected: impl Fn(&T) -> bool,
) -> Vec<BundleChoice<T>> {
    items.into_iter().map(|(id, label)| BundleChoice { selected: selected(&id), id, label }).collect()
}

pub(super) fn panel_view(
    s: &Session,
    machine: Option<&Machine>,
    libs: &Libraries,
    recent_templates: &[PathBuf],
) -> Option<BundleDialogViewModel> {
    let ui = s.files.bundle.as_ref()?;
    let loading = libs.state != LoadState::Ready;
    let mode = match &ui.state {
        State::Export(e) => {
            let materials = libs.materials.iter().flat_map(|l| l.entries()).map(|m| {
                let d = m.draft();
                (m.id(), format!("{} — {} {} mm", d.name, d.material, format_mm(d.thickness.mm())))
            });
            let art =
                libs.art.iter().flat_map(|l| l.items()).map(|i| (i.id(), format!("{} / {}", i.group(), i.name())));
            let mut profiles: Vec<String> = machine.iter().map(|m| m.profile.revision().id.clone()).collect();
            for p in &libs.profiles {
                let id = p.revision().id.clone();
                if !profiles.contains(&id) {
                    profiles.push(id);
                }
            }
            let mut names: Vec<String> = Vec::new();
            for name in recent_templates.iter().filter_map(|p| template_name(p)) {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
            BundleMode::Export(BundleExportView {
                materials: labelled(materials, |id| e.materials.contains(id)),
                art: labelled(art, |id| e.art.contains(id)),
                profiles: labelled(profiles.into_iter().map(|p| (p.clone(), p)), |id| e.profiles.contains(id)),
                templates: labelled(names.into_iter().map(|n| (n.clone(), n)), |id| e.templates.contains(id)),
                written: e.written.clone(),
            })
        }
        State::Import(i) => {
            let mut view = BundleImportView {
                file: i.file.clone(),
                conflict: i.conflict,
                report: i.report.clone(),
                ..BundleImportView::default()
            };
            if let (Some(data), Some(materials), Some(art)) = (&i.data, libs.materials.as_ref(), libs.art.as_ref()) {
                let mut profiles: Vec<MachineProfile> = machine.iter().map(|m| (*m.profile).clone()).collect();
                for p in &libs.profiles {
                    if !profiles.iter().any(|k| k.revision().id == p.revision().id) {
                        profiles.push((**p).clone());
                    }
                }
                let local = LocalState {
                    materials: Some(materials),
                    art: Some(art),
                    profiles: &profiles,
                    templates: data.local_templates.as_slice(),
                };
                let preview: ImportPreview = preview_import(&data.bundle, &local);
                view.materials = preview
                    .materials
                    .iter()
                    .map(|m| BundleRow {
                        label: format!("{} — {} {} mm", m.name, m.material, format_mm(m.thickness_mm)),
                        change: change_of(m.change),
                    })
                    .collect();
                view.art = preview
                    .art
                    .iter()
                    .map(|a| BundleRow { label: format!("{} / {}", a.group, a.name), change: change_of(a.change) })
                    .collect();
                view.templates = preview
                    .templates
                    .iter()
                    .map(|t| BundleRow { label: t.name.clone(), change: change_of(t.change) })
                    .collect();
                view.profiles = preview
                    .profiles
                    .iter()
                    .map(|p| BundleProfileRow {
                        id: p.id.clone(),
                        bundle_revision: p.bundle_revision,
                        local_revision: p.local_revision,
                        change: match &p.change {
                            ProfileChange::New => BundleProfileChange::New,
                            ProfileChange::Identical => BundleProfileChange::Identical,
                            ProfileChange::Differs { fields } => BundleProfileChange::Differs {
                                fields: fields.iter().map(|f| (*f).to_owned()).collect(),
                            },
                        },
                        replace: i.replace_profiles.contains(&p.id),
                    })
                    .collect();
                view.has_conflicts = view
                    .materials
                    .iter()
                    .chain(&view.art)
                    .chain(&view.templates)
                    .any(|r| r.change == BundleChange::Conflict);
            }
            BundleMode::Import(view)
        }
    };
    Some(BundleDialogViewModel {
        loading: loading || matches!(&ui.state, State::Import(i) if i.data.is_none() && ui.issue.is_none()),
        busy: ui.busy,
        issue: ui.issue,
        mode,
    })
}
