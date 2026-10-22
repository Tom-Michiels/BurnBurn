//! Host side of the machine hub window (G5-machine-rest): showing it, copying the hub view model
//! into it after every refresh and turning a used control into the presentation intent. A child
//! module of `app`, so it can use the host's private state.

use crate::appkit::panels::hub::{
    TAG_CANCEL, TAG_CONFIRM, TAG_DEVICE, TAG_RECORD, TAG_REMOVE, TAG_REPEAT, TAG_SECTION, TAG_STOP, build_hub,
};
use crate::hub_panel::{HubEvent, hub_intent, hub_panel};

use super::HostApp;

impl HostApp {
    /// Venster ▸ Machine en productie…
    pub fn show_machine_hub(&self) {
        let (mtm, locale, float) = (self.mtm, self.locale, self.float());
        let app = self.me.clone();
        self.with_panels(|p| {
            if p.hub.is_none() {
                p.hub = Some(build_hub(mtm, app, locale, float));
            }
            if let Some(w) = &p.hub {
                crate::appkit::app_panels::windows::present(&w.window, float);
            }
        });
        self.refresh_hub();
    }

    /// Copies the hub view model into the window (only while it exists).
    pub(crate) fn refresh_hub(&self) {
        let Ok(controller) = self.controller.try_borrow() else { return };
        let vm = controller.machine_hub_view();
        drop(controller);
        let panel = hub_panel(&vm, self.active_document().is_some(), self.locale);
        let locale = self.locale;
        self.with_panels(|p| {
            if let Some(w) = p.hub.as_mut() {
                w.apply(&panel, locale);
            }
        });
    }

    /// A control of the hub window was used.
    pub(crate) fn hub_action(&self, tag: isize) {
        if tag == TAG_SECTION {
            return self.hub_section_changed();
        }
        let vm = match self.controller.try_borrow() {
            Ok(c) => c.machine_hub_view(),
            Err(_) => return,
        };
        let event = self
            .with_panels(|p| {
                let w = p.hub.as_ref()?;
                Some(match tag {
                    TAG_DEVICE => HubEvent::DeviceChosen(w.selected_device()?),
                    TAG_STOP => HubEvent::StopUpload,
                    TAG_REPEAT => HubEvent::Repeat(w.selected_history()?),
                    TAG_CONFIRM => HubEvent::ConfirmRepeat,
                    TAG_CANCEL => HubEvent::CancelRepeat,
                    TAG_RECORD => HubEvent::RecordProof(w.form()),
                    TAG_REMOVE => HubEvent::RemoveProof(w.selected_proof()?),
                    _ => return None,
                })
            })
            .flatten();
        if let Some(intent) = event.and_then(|e| hub_intent(&vm, self.active_document(), e)) {
            self.dispatch(intent);
            self.refresh();
        }
    }

    fn hub_section_changed(&self) {
        self.with_panels(|p| {
            if let Some(w) = p.hub.as_mut() {
                let selected = w.section_selected();
                w.section_popup_changed(selected);
            }
        });
    }

    /// Whether the hub window exists (evidence for tests).
    pub fn machine_hub_open(&self) -> bool {
        self.app_panels.try_borrow().map(|p| p.hub.is_some()).unwrap_or(false)
    }
}
