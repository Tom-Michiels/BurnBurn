//! Host side of the Schikken panel (W3-UI-schikken): the menu item that shows or hides its options
//! and the accessors native tests drive its widgets with. A child module of `app`, so it can use the
//! host's private state.

use burnburn_types::DocumentId;
use objc2::rc::Retained;
use objc2_app_kit::NSView;

use super::HostApp;
use crate::panels::SchikkenControl;

impl HostApp {
    /// Schikken ▸ Schikkenpaneel: shows or hides the options panel of the active window.
    pub(crate) fn toggle_schikken_panel(&self) {
        let Some(document) = self.view().active else { return };
        if let Ok(mut windows) = self.windows.try_borrow_mut()
            && let Some(w) = windows.get_mut(&document)
        {
            w.toggle_schikken();
        }
        self.refresh();
    }

    /// True while `document`'s Schikken panel is on screen.
    pub fn schikken_panel_shown(&self, document: DocumentId) -> bool {
        self.windows.try_borrow().ok().and_then(|w| w.get(&document).map(|w| w.schikken.shown)).unwrap_or(false)
    }

    /// The widget of `control` in `document`'s Schikken panel, if it has one now.
    pub fn schikken_control(&self, document: DocumentId, control: SchikkenControl) -> Option<Retained<NSView>> {
        self.windows.try_borrow().ok()?.get(&document)?.schikken.control_view(control)
    }

    /// The texts (headings, notes, problems) of `document`'s Schikken panel, top to bottom.
    pub fn schikken_labels(&self, document: DocumentId) -> Vec<String> {
        self.windows.try_borrow().ok().and_then(|w| w.get(&document).map(|w| w.schikken.labels())).unwrap_or_default()
    }
}
