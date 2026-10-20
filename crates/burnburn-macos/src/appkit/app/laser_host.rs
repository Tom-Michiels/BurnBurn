//! Host side of the laser window (W2-L1-laser): attaching the machine service and history, the
//! laser panel's menu actions, the delete confirmation for a controller file and the diagnostic
//! export's save panel. A child module of `app`, so it can use the host's private state.
//!
//! Nothing here contacts a machine: the real app attaches no machine service (the panel then says
//! "Geen machine verbonden"); tests and hosts that have a service attach it explicitly.

#![allow(unsafe_code)] // one checkbox without target/action (SAFETY below)

use std::path::PathBuf;
use std::sync::Arc;

use block2::RcBlock;
use burnburn_presentation::{
    DiagnosticOptions, HistoryStore, HistoryStoreError, LaserViewModel, MachineAction, MachinePort, UiIntent,
};
use burnburn_types::DocumentId;
use objc2::MainThreadOnly;
use objc2_app_kit::{
    NSAlert, NSAlertStyle, NSButton, NSControlStateValueOff, NSControlStateValueOn, NSModalResponse, NSSavePanel,
    NSView,
};
use objc2_foundation::NSString;

use super::HostApp;
use crate::adapter::NativeEvent;
use crate::appkit::objc_try;
use crate::appkit::panels::widgets::rect;
use crate::panels::{LaserText, laser_text, machine_intent};

impl HostApp {
    /// Attach the machine service (one owner of the connection, shared by every window).
    pub fn attach_machine(&self, port: Arc<dyn MachinePort>) {
        if let Ok(mut controller) = self.controller.try_borrow_mut() {
            controller.attach_machine(port);
        }
        self.refresh();
    }

    /// Attach the production history (orders, executions, the diagnostic export).
    pub fn attach_history(&self, history: Arc<dyn HistoryStore>) -> Result<(), HistoryStoreError> {
        let result = match self.controller.try_borrow_mut() {
            Ok(mut controller) => controller.attach_history(history),
            Err(_) => Ok(()),
        };
        self.refresh();
        result
    }

    /// The laser window model of `document`'s window; `None` without a machine service.
    pub(crate) fn laser_view_for(&self, document: DocumentId) -> Option<LaserViewModel> {
        self.controller.try_borrow().ok()?.laser_view_for(Some(document))
    }

    /// The machine panel's control of `kind` (e.g. the software stop), sent with its key and seen
    /// state unchanged; a disabled or missing one sends nothing.
    pub(crate) fn press_machine_control(&self, document: DocumentId, kind: MachineAction) {
        let machine = self.controller.try_borrow().ok().and_then(|c| c.machine_view());
        let intent = machine.and_then(|m| m.button(kind).and_then(|b| machine_intent(Some(document), b)));
        if let Some(intent) = intent {
            self.dispatch(intent);
        }
    }

    /// Laser ▸ Laservenster: shows or hides the active window's laser panel.
    pub(crate) fn toggle_laser_panel(&self) {
        let Some(document) = self.view().active else { return };
        let shown = match self.windows.try_borrow_mut() {
            Ok(mut windows) => windows.get_mut(&document).map(|w| w.toggle_laser()),
            Err(_) => None,
        };
        if shown == Some(false) {
            // A hidden panel must not leave a key held.
            self.dispatch(crate::panels::jog_release_intent());
        }
        self.refresh();
    }

    /// True while `document`'s laser panel is on screen (it is only refreshed then).
    pub(crate) fn laser_shown_in(&self, document: DocumentId) -> bool {
        self.laser_panel_shown(document)
    }

    /// Runs `f` on the widgets of `document`'s laser panel (native tests drive them like a user).
    pub fn with_laser<R>(
        &self,
        document: DocumentId,
        f: impl FnOnce(&crate::appkit::panels::laser::LaserParts<'_>) -> R,
    ) -> Option<R> {
        let windows = self.windows.try_borrow().ok()?;
        let w = windows.get(&document)?;
        Some(f(&w.laser.parts()))
    }

    /// Whether `document`'s laser panel is on screen (evidence for tests).
    pub fn laser_panel_shown(&self, document: DocumentId) -> bool {
        self.windows.try_borrow().ok().and_then(|w| w.get(&document).map(|w| w.laser.shown)).unwrap_or(false)
    }

    /// Delete a file on the controller: a sheet first, because it cannot be undone and the file is
    /// not in any project. Only "Verwijder" sends the intent.
    pub(crate) fn confirm_controller_file_delete(&self, document: DocumentId, intent: UiIntent) {
        let Some(window) = self.window(document) else { return };
        let locale = self.locale;
        let alert = NSAlert::new(self.mtm);
        alert.setAlertStyle(NSAlertStyle::Warning);
        alert.setMessageText(&NSString::from_str(laser_text(LaserText::DeleteTitle, locale)));
        alert.setInformativeText(&NSString::from_str(laser_text(LaserText::DeleteBody, locale)));
        alert.addButtonWithTitle(&NSString::from_str(laser_text(LaserText::DeleteConfirm, locale)));
        alert.addButtonWithTitle(&NSString::from_str(laser_text(LaserText::Cancel, locale)));
        let me = self.me.clone();
        let handler = RcBlock::new(move |response: NSModalResponse| {
            crate::appkit::classes::ffi_guard("controller file delete sheet", (), || {
                if response == crate::adapter::response::FIRST_BUTTON
                    && let Some(app) = me.upgrade()
                {
                    app.dispatch(intent.clone());
                }
            });
        });
        if let Err(e) = objc_try("showing the delete sheet", || {
            alert.beginSheetModalForWindow_completionHandler(&window, Some(&handler))
        }) {
            eprintln!("burnburn: {e}");
        }
    }

    /// Help ▸ Diagnose exporteren…: a save panel with the two privacy choices (off by default).
    pub(crate) fn show_diagnostics_panel(&self) {
        let Some(document) = self.view().active else { return };
        let Some(window) = self.window(document) else { return };
        let locale = self.locale;
        let panel = NSSavePanel::savePanel(self.mtm);
        panel.setNameFieldStringValue(&NSString::from_str("burnburn-diagnose.txt"));
        panel.setMessage(Some(&NSString::from_str(laser_text(LaserText::DiagnosticsMessage, locale))));
        panel.setTitle(Some(&NSString::from_str(laser_text(LaserText::DiagnosticsTitle, locale))));
        let accessory = NSView::initWithFrame(NSView::alloc(self.mtm), rect(0.0, 0.0, 360.0, 54.0));
        let check = |text: LaserText, y: f64| {
            // SAFETY: no target and no action; the state is read when the panel closes.
            let b = unsafe {
                NSButton::checkboxWithTitle_target_action(
                    &NSString::from_str(laser_text(text, locale)),
                    None,
                    None,
                    self.mtm,
                )
            };
            b.setFrame(rect(12.0, y, 336.0, 22.0));
            b.setState(NSControlStateValueOff);
            accessory.addSubview(&b);
            b
        };
        let paths = check(LaserText::DiagnosticsPaths, 28.0);
        let ids = check(LaserText::DiagnosticsIds, 4.0);
        panel.setAccessoryView(Some(&accessory));
        let me = self.me.clone();
        let reader = panel.clone();
        let handler = RcBlock::new(move |response: NSModalResponse| {
            crate::appkit::classes::ffi_guard("diagnostics panel completion", (), || {
                let path: Option<PathBuf> = (response == crate::adapter::response::OK)
                    .then(|| reader.URL())
                    .flatten()
                    .and_then(|u| u.to_file_path());
                let options = DiagnosticOptions {
                    include_project_paths: paths.state() == NSControlStateValueOn,
                    include_machine_ids: ids.state() == NSControlStateValueOn,
                };
                if let Some(app) = me.upgrade() {
                    app.native(NativeEvent::DiagnosticsPanelFinished { options, path });
                }
            });
        });
        if let Err(e) = objc_try("showing the diagnostics panel", || {
            panel.beginSheetModalForWindow_completionHandler(&window, &handler)
        }) {
            eprintln!("burnburn: {e}");
        }
    }
}
