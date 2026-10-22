//! Machine profiles that were replaced as the active one stay in the profile library (P07), and
//! the active machine is chosen from the known profiles.
//!
//! A profile to keep waits in `Libraries::kept` until the library file has been read; only then is
//! it merged (it wins over the file's copy of the same machine) and written, so the file's other
//! profiles are never lost. A damaged file is never written (`profiles_unreadable`).

use std::sync::Arc;

use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};

use super::super::machine::{Declared, Extras, Machine};
use super::super::{AppController, AppUpdate, error};
use super::{FilesJob, LoadState};
use crate::files_task::{FilesOutput, FilesTask};
use crate::task::{TaskError, TaskOutcome, TaskOutput};
use crate::view_model::MessageKey;

impl AppController {
    /// Makes `new` the active machine. The machine it replaces is kept in the profile library,
    /// unless nothing about it changes.
    pub(in crate::controller) fn replace_active_machine(&mut self, mut new: Machine) {
        // The known machines are listed from the library: read it as soon as a machine is in use.
        self.ensure_libraries();
        // Whatever profile comes in carries the measurements of its machine (W09).
        let calibration = self.calibration_of(&new.profile.revision().id);
        if calibration != *new.profile.calibration()
            && let Ok(profile) = (*new.profile).clone().with_calibration(calibration)
        {
            new.profile = Arc::new(profile);
        }
        if let Some(old) = self.machine.as_ref()
            && old.profile.revision() != new.profile.revision()
        {
            let profile = Arc::clone(&old.profile);
            let extras = Extras { grbl: old.grbl.clone(), declared: old.declared.clone() };
            self.keep_profile(profile, extras);
        }
        self.machine = Some(new);
    }

    /// Keeps `profile` in the profile library with its GRBL dialect settings and declared
    /// identity, if it has any (written once the library has been read).
    pub(in crate::controller) fn keep_profile(&mut self, profile: Arc<burnburn_job::MachineProfile>, extras: Extras) {
        self.libs.kept.push((profile, extras));
        self.ensure_libraries();
        if self.libs.state == LoadState::Ready {
            self.merge_kept_profiles();
        }
    }

    /// Moves the profiles waiting to be kept into the library and writes it.
    pub(super) fn merge_kept_profiles(&mut self) {
        let kept = std::mem::take(&mut self.libs.kept);
        if kept.is_empty() {
            return;
        }
        for (p, extras) in kept {
            if extras == Extras::default() {
                self.libs.extras.remove(&p.revision().id);
            } else {
                self.libs.extras.insert(p.revision().id.clone(), extras);
            }
            match self.libs.profiles.iter_mut().find(|k| k.revision().id == p.revision().id) {
                Some(slot) => *slot = p,
                None => self.libs.profiles.push(p),
            }
        }
        self.flush_profiles();
    }

    /// Reads a machine profile file and makes it the active machine.
    pub(in crate::controller) fn open_machine_profile(&mut self, path: std::path::PathBuf) -> AppUpdate {
        let task = FilesTask::ReadMachineProfile { path, fs: Arc::clone(&self.ports.fs) };
        if self.submit_files(task, FilesJob::OpenMachineProfile) {
            AppUpdate::default()
        } else {
            error(None, MessageKey::MachineProfileFileUnusable)
        }
    }

    /// The file was read: its machine becomes the active one (the replaced one is kept), with the
    /// dialect and the model and firmware the file declares. An unusable file changes nothing.
    pub(super) fn machine_profile_read(&mut self, outcome: TaskOutcome) -> AppUpdate {
        match outcome.result {
            Ok(TaskOutput::Files(FilesOutput::MachineProfileRead(file))) => {
                let crate::profile_file::ProfileFile { profile, ruida, backend, identity } = *file;
                let grbl = match backend {
                    crate::profile_file::Backend::Grbl(g) => Some(g),
                    crate::profile_file::Backend::Ruida => None,
                };
                let declared = identity.map_or_else(Declared::default, |i| {
                    Declared::of(&i.model.unwrap_or_default(), &i.firmware.unwrap_or_default())
                });
                self.replace_active_machine(Machine { profile: Arc::new(profile), ruida, grbl, declared });
                AppUpdate { view_changed: true, ..AppUpdate::default() }
            }
            Err(TaskError::Cancelled) => AppUpdate::default(),
            _ => error(None, MessageKey::MachineProfileFileUnusable),
        }
    }

    /// Makes the known profile `id` the active machine. A GRBL machine of the library comes back
    /// with its dialect settings; any other keeps the Ruida settings of the machine in use.
    pub(in crate::controller) fn choose_machine_profile(&mut self, id: &str) -> AppUpdate {
        let Some(profile) = self.known_profiles().into_iter().find(|p| p.revision().id == id) else {
            return error(None, MessageKey::UnknownMachineProfile);
        };
        let ruida =
            self.machine.as_ref().map_or(
                RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged },
                |m| m.ruida,
            );
        // The active machine listed first is the machine in use: its own settings are current.
        let extras = match self.machine.as_ref().filter(|m| m.profile.revision().id == id) {
            Some(active) => Extras { grbl: active.grbl.clone(), declared: active.declared.clone() },
            None => self.libs.extras.get(id).cloned().unwrap_or_default(),
        };
        self.replace_active_machine(Machine { profile, ruida, grbl: extras.grbl, declared: extras.declared });
        AppUpdate { view_changed: true, ..AppUpdate::default() }
    }
}
