//! Machine profiles that were replaced as the active one stay in the profile library (P07), and
//! the active machine is chosen from the known profiles.
//!
//! A profile to keep waits in `Libraries::kept` until the library file has been read; only then is
//! it merged (it wins over the file's copy of the same machine) and written, so the file's other
//! profiles are never lost. A damaged file is never written (`profiles_unreadable`).

use std::sync::Arc;

use burnburn_ruida::{Magic, OverscanPolicy, RuidaProfile};

use super::super::machine::Machine;
use super::super::{AppController, AppUpdate, error};
use super::LoadState;
use crate::view_model::MessageKey;

impl AppController {
    /// Makes `new` the active machine. The machine it replaces is kept in the profile library,
    /// unless nothing about it changes.
    pub(in crate::controller) fn replace_active_machine(&mut self, new: Machine) {
        // The known machines are listed from the library: read it as soon as a machine is in use.
        self.ensure_libraries();
        if let Some(old) = self.machine.as_ref()
            && old.profile.revision() != new.profile.revision()
        {
            let (profile, grbl) = (Arc::clone(&old.profile), old.grbl.clone());
            self.keep_profile(profile, grbl);
        }
        self.machine = Some(new);
    }

    /// Keeps `profile` in the profile library with its GRBL dialect settings, if it has any
    /// (written once the library has been read).
    pub(in crate::controller) fn keep_profile(
        &mut self,
        profile: Arc<burnburn_job::MachineProfile>,
        grbl: Option<burnburn_gcode::GrblProfile>,
    ) {
        self.libs.kept.push((profile, grbl));
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
        for (p, grbl) in kept {
            match grbl {
                Some(grbl) => self.libs.dialects.insert(p.revision().id.clone(), grbl),
                None => self.libs.dialects.remove(&p.revision().id),
            };
            match self.libs.profiles.iter_mut().find(|k| k.revision().id == p.revision().id) {
                Some(slot) => *slot = p,
                None => self.libs.profiles.push(p),
            }
        }
        self.flush_profiles();
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
        let grbl = match self.machine.as_ref().filter(|m| m.profile.revision().id == id) {
            Some(active) => active.grbl.clone(),
            None => self.libs.dialects.get(id).cloned(),
        };
        self.replace_active_machine(Machine { profile, ruida, grbl });
        AppUpdate { view_changed: true, ..AppUpdate::default() }
    }
}
