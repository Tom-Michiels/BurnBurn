//! Host side of the golf-5 panels (G5-native): the menu items that show or hide the offset/repair
//! and structure panels, and the accessors native tests drive the widgets with. A child module of
//! `app`, so it can use the host's private state.

use burnburn_types::DocumentId;
use objc2::rc::Retained;
use objc2_app_kit::NSView;

use super::HostApp;
use crate::panels::{G5Control, G5Panel};

impl HostApp {
    /// Venster ▸ Offset en reparatie / Structuur: shows or hides that panel of the active window.
    pub(crate) fn toggle_g5_panel(&self, panel: G5Panel) {
        let Some(document) = self.view().active else { return };
        if let Ok(mut windows) = self.windows.try_borrow_mut()
            && let Some(w) = windows.get_mut(&document)
        {
            w.toggle_g5(panel);
        }
        self.refresh();
    }

    /// True while `panel` of `document` is on screen.
    pub fn g5_panel_shown(&self, document: DocumentId, panel: G5Panel) -> bool {
        self.windows
            .try_borrow()
            .ok()
            .and_then(|w| w.get(&document).map(|w| w.g5.iter().any(|s| s.panel_kind() == panel && s.shown)))
            .unwrap_or(false)
    }

    /// The widget of `control` in `panel` of `document`, if it has one now. A note is its text view.
    pub fn g5_control(&self, document: DocumentId, panel: G5Panel, control: G5Control) -> Option<Retained<NSView>> {
        self.windows
            .try_borrow()
            .ok()?
            .get(&document)?
            .g5
            .iter()
            .find(|s| s.panel_kind() == panel)?
            .control_view(control)
    }

    /// The texts (headings, notes, problems) of `panel` of `document`, top to bottom.
    pub fn g5_labels(&self, document: DocumentId, panel: G5Panel) -> Vec<String> {
        self.windows
            .try_borrow()
            .ok()
            .and_then(|w| w.get(&document).and_then(|w| w.g5.iter().find(|s| s.panel_kind() == panel).map(|s| s.labels())))
            .unwrap_or_default()
    }
}
