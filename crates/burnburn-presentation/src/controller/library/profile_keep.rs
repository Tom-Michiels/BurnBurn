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
        if let Some(old) = self.machine.as_ref()
            && old.profile.revision() != new.profile.revision()
        {
            let old = Arc::clone(&old.profile);
            self.keep_profile(old);
        }
        self.machine = Some(new);
    }

    /// Keeps `profile` in the profile library (written once the library has been read).
    pub(in crate::controller) fn keep_profile(&mut self, profile: Arc<burnburn_job::MachineProfile>) {
        self.libs.kept.push(profile);
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
        for p in kept {
            match self.libs.profiles.iter_mut().find(|k| k.revision().id == p.revision().id) {
                Some(slot) => *slot = p,
                None => self.libs.profiles.push(p),
            }
        }
        self.flush_profiles();
    }

    /// Makes the known profile `id` the active machine; the dialect stays as it was.
    pub(in crate::controller) fn choose_machine_profile(&mut self, id: &str) -> AppUpdate {
        let Some(profile) = self.known_profiles().into_iter().find(|p| p.revision().id == id) else {
            return error(None, MessageKey::UnknownMachineProfile);
        };
        let ruida =
            self.machine.as_ref().map_or(
                RuidaProfile { magic: Magic::FIXTURE_0X88, overscan: OverscanPolicy::ControllerManaged },
                |m| m.ruida,
            );
        // The profile library keeps machine profiles only, no GRBL dialect settings: a chosen
        // library profile is driven as Ruida until GRBL settings are given again (open, G5).
        self.replace_active_machine(Machine { profile, ruida, grbl: None });
        AppUpdate { view_changed: true, ..AppUpdate::default() }
    }
}
