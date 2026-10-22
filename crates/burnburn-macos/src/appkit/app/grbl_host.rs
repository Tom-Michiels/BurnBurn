//! Host side of the GRBL laser window (G5-grbl-ui): handing in the serial port, showing the window
//! from Laser ▸ Laservenster, forwarding its widgets as intents, the status tick and the macro file.
//! A child module of `app`, so it can use the host's private state.
//!
//! Nothing here opens a serial device: the real app attaches no port (the window then says "geen
//! seriële poort verbonden"); tests and hosts that have a port attach it explicitly.

use std::path::PathBuf;

use burnburn_presentation::{GrblControlKind, GrblIntent, GrblSerialPort, UiIntent};

use super::HostApp;
use crate::appkit::panels::grbl_window::{GrblParts, grbl_window};
use crate::panels::{
    GrblNumber, grbl_control_intent, grbl_input_command_intent, grbl_macro_remove_intent, grbl_macro_run_intent,
    grbl_macro_save_intent, grbl_number_intent, grbl_send_intent, grbl_window_panel,
};

/// How often the window asks a connected controller for its status (seconds).
pub(crate) const GRBL_TICK_S: f64 = 0.5;

impl HostApp {
    /// Hand the app the serial port of the GRBL machine (the host owns the device).
    pub fn attach_grbl_port(&self, port: Box<dyn GrblSerialPort>) {
        if let Ok(mut controller) = self.controller.try_borrow_mut() {
            controller.attach_grbl_port(port);
        }
        self.refresh();
    }

    fn grbl_view(&self) -> Option<burnburn_presentation::GrblViewModel> {
        self.controller.try_borrow().ok()?.grbl_view()
    }

    /// Laser ▸ Laservenster on a GRBL machine: shows or hides the GRBL window.
    pub(crate) fn toggle_grbl_window(&self) {
        if let Ok(mut slot) = self.grbl_window.try_borrow_mut() {
            let window = slot.get_or_insert_with(|| grbl_window(self.mtm, self.me.clone(), self.locale));
            window.shown = !window.shown;
            let panel = window.panel.clone();
            let result = if window.shown {
                crate::appkit::objc_try("showing the GRBL window", || panel.orderFront(None))
            } else {
                crate::appkit::objc_try("hiding the GRBL window", || panel.orderOut(None))
            };
            if let Err(e) = result {
                eprintln!("burnburn: {e}");
            }
        }
        self.refresh();
    }

    /// True while the GRBL window is on screen (it is only refreshed then).
    pub fn grbl_window_shown(&self) -> bool {
        self.grbl_window.try_borrow().ok().and_then(|w| w.as_ref().map(|w| w.shown)).unwrap_or(false)
    }

    /// Runs `f` on the widgets of the GRBL window (native tests drive them like a user).
    pub fn with_grbl_window<R>(&self, f: impl FnOnce(&GrblParts<'_>) -> R) -> Option<R> {
        let slot = self.grbl_window.try_borrow().ok()?;
        Some(f(&slot.as_ref()?.parts()))
    }

    /// Return in the console input: the line goes to presentation; an accepted line leaves the
    /// field empty, a refused one stays in it with the refusal in words.
    pub(crate) fn grbl_send_typed(&self, text: &str) {
        let Some(intent) = grbl_send_intent(text) else { return };
        self.dispatch(intent);
        let accepted = self.grbl_view().is_some_and(|v| v.last_refusal.is_none());
        if accepted
            && let Ok(slot) = self.grbl_window.try_borrow()
            && let Some(w) = slot.as_ref()
        {
            crate::appkit::panels::widgets::set_text(w.parts().input, "");
        }
        self.refresh();
    }

    pub(crate) fn grbl_control_pressed(&self, kind: GrblControlKind) {
        let Some(intent) = self.grbl_view().and_then(|v| grbl_control_intent(&v, kind)) else { return };
        self.dispatch(intent);
        self.refresh();
    }

    pub(crate) fn grbl_number_committed(&self, which: GrblNumber, text: &str) {
        let Some(intent) = self.grbl_view().and_then(|v| grbl_number_intent(&v, which, text)) else { return };
        self.dispatch(intent);
        self.refresh();
    }

    pub(crate) fn grbl_macro_run(&self, row: Option<usize>) {
        let Some(intent) = row.and_then(|r| self.grbl_view().and_then(|v| grbl_macro_run_intent(&v, r))) else {
            return;
        };
        self.dispatch(intent);
        self.refresh();
    }

    pub(crate) fn grbl_macro_remove(&self, row: Option<usize>) {
        let Some(intent) = row.and_then(|r| self.grbl_view().and_then(|v| grbl_macro_remove_intent(&v, r))) else {
            return;
        };
        self.dispatch(intent);
        self.refresh();
    }

    pub(crate) fn grbl_macro_save(&self, name: &str, program: &str) {
        let Some(intent) = self.grbl_view().and_then(|v| grbl_macro_save_intent(&v, name, program)) else { return };
        self.dispatch(intent);
        self.refresh();
    }

    /// A key command of the console input; true when it was ours (history up and down).
    pub(crate) fn grbl_input_command(&self, selector: &str) -> bool {
        let Some(intent) = grbl_input_command_intent(selector) else { return false };
        self.dispatch(intent);
        self.refresh();
        true
    }

    /// A row of the plan's GRBL refusals was chosen: its object becomes the selection.
    pub(crate) fn grbl_refusal_chosen(&self, document: burnburn_types::DocumentId, row: usize) {
        let vm = self.view();
        let panel = crate::panels::plan_panel(&vm, document, self.locale);
        if let Some(intent) = panel.select_intent(document, row) {
            self.dispatch(intent);
            self.refresh();
        }
    }

    /// Copies the GRBL model into the window. A window that is not on screen is not touched.
    pub(crate) fn refresh_grbl_window(&self) {
        if !self.grbl_window_shown() {
            return;
        }
        let vm = self.grbl_view();
        let Some(model) = grbl_window_panel(vm.as_ref(), self.locale) else { return };
        if let Ok(slot) = self.grbl_window.try_borrow()
            && let Some(window) = slot.as_ref()
        {
            window.refresh(&model);
        }
    }

    /// The timer: while the window is on screen and a port is attached, read the controller.
    pub(crate) fn grbl_tick(&self) {
        if self.grbl_window_shown() && self.grbl_view().is_some_and(|v| v.connected) {
            self.dispatch(UiIntent::Grbl(GrblIntent::Tick));
            self.refresh();
        }
    }

    /// Where the macros are kept: next to the recent-projects list (none: not persisted).
    fn grbl_macros_file(&self) -> Option<PathBuf> {
        self.options.recent_projects_file.as_ref().and_then(|f| f.parent()).map(|dir| dir.join("grbl-macros.txt"))
    }

    /// The controller changed the macros: keep the text form (a failure is said, never silent).
    pub(crate) fn store_grbl_macros(&self, text: &str) {
        let Some(file) = self.grbl_macros_file() else { return };
        if let Some(dir) = file.parent()
            && let Err(e) = std::fs::create_dir_all(dir)
        {
            eprintln!("burnburn: GRBL macros {}: {e}", dir.display());
            return;
        }
        if let Err(e) = std::fs::write(&file, text) {
            eprintln!("burnburn: GRBL macros {}: {e}", file.display());
        }
    }

    /// At start: the macros the last session kept. A missing file is no macros; an unreadable one
    /// is reported by the controller (the window shows the refusal) and replaces nothing.
    pub(crate) fn load_grbl_macros(&self) {
        let Some(file) = self.grbl_macros_file() else { return };
        let Ok(text) = std::fs::read_to_string(&file) else { return };
        if let Ok(mut controller) = self.controller.try_borrow_mut() {
            controller.handle(UiIntent::Grbl(GrblIntent::LoadMacros(text)));
        }
    }
}
