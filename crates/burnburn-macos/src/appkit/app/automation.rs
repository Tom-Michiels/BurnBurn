//! Host side of desktop automation (API01, G5-native): the listener thread that follows the
//! preference "Desktop-automatisering", and the UI-thread pump that carries out its requests
//! through the host's own dispatcher. A child module of `app`, so it can use the host's private
//! state. The protocol and the pump are `crate::automation_host` (shared with the Windows host);
//! nothing here can reach a machine: the pump only sends open, import and commit/cancel-import.

use std::sync::Arc;

use burnburn_presentation::{AppViewModel, HostEffect, UiIntent};

use super::HostApp;
use crate::automation_host::AutomationApp;

/// The host as the pump sees it.
struct Adapter<'a>(&'a HostApp);

impl AutomationApp for Adapter<'_> {
    fn view(&self) -> Arc<AppViewModel> {
        self.0.view()
    }

    fn dispatch(&mut self, intent: UiIntent) -> Vec<HostEffect> {
        self.0.dispatch(intent).effects
    }
}

impl HostApp {
    /// Moves the current automation job on, or starts the next one. Called after finished tasks and
    /// on the listener's wake-up; a call while the pump is already running (an intent it sent
    /// triggered an update) is skipped, the outer call sees the result.
    pub(crate) fn automation_step(&self) {
        let Ok(mut automation) = self.automation.try_borrow_mut() else { return };
        let now_ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        automation.step(&mut Adapter(self), now_ms);
    }

    /// Tells the listener what the view model says now (document, unsaved changes, busy).
    pub(crate) fn automation_publish(&self, vm: &AppViewModel) {
        if let Ok(automation) = self.automation.try_borrow() {
            automation.publish_status(vm);
        }
    }

    /// Records what an update did (opened windows, errors) for the job in progress.
    pub(crate) fn automation_observe(&self, effects: &[HostEffect]) {
        if let Ok(mut automation) = self.automation.try_borrow_mut() {
            automation.observe(effects);
        }
    }

    /// The listener runs exactly while the preference is on. A busy borrow is retried by the next
    /// call (every refresh).
    pub(crate) fn sync_automation(&self, enabled: bool) {
        let Ok(mut automation) = self.automation.try_borrow_mut() else { return };
        if let Err(e) = automation.sync_preference(enabled) {
            eprintln!("burnburn: desktop automation stays off: {e}");
        }
    }

    /// The application quits: the listener ends before the process does.
    pub(crate) fn shutdown_automation(&self) {
        if let Ok(mut automation) = self.automation.try_borrow_mut() {
            automation.shutdown();
        }
    }

    /// Another port for the next start of the listener (tests use a free one; `0`).
    pub fn set_automation_port(&self, port: u16) {
        if let Ok(mut automation) = self.automation.try_borrow_mut() {
            automation.set_port(port);
        }
    }

    /// True while the automation listener thread runs (tests).
    pub fn automation_running(&self) -> bool {
        self.automation.try_borrow().map(|a| a.running()).unwrap_or(false)
    }

    /// Where the automation listener is bound, when it runs (tests).
    pub fn automation_addr(&self) -> Option<std::net::SocketAddr> {
        self.automation.try_borrow().ok().and_then(|a| a.local_addr())
    }
}
