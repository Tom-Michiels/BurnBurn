//! Host side of the CAM dialogs (W3-UI-cam): one floating panel per open dialog, shown, refreshed
//! and dropped as presentation opens and closes it, and the two things a panel asks of the host —
//! a changed field and a pressed button — turned into intents. Save as RD asks NSSavePanel for a
//! path first; nothing here writes a file or contacts a machine.
//!
//! A child module of `app`, so it can use the host's private state.

use std::collections::BTreeMap;

use block2::RcBlock;
use burnburn_document::process::OrderKey;
use burnburn_presentation::{CamDialog, DialogActionKind, DialogField, DialogValue, DialogViewModel, UiIntent};
use burnburn_types::DocumentId;
use objc2_app_kit::{NSButton, NSControl, NSModalResponse, NSSavePanel, NSTextField};
use objc2_foundation::NSString;

use super::{HostApp, set_file_type};
use crate::appkit::objc_try;
use crate::appkit::panels::cam_dialog::CamPanelUi;
use crate::panels::{cam_action_intent, cam_field_intent, cam_move_intent};

/// The open panels, by window and dialog.
pub(super) type CamPanels = BTreeMap<(DocumentId, CamDialog), CamPanelUi>;

/// The widgets of one open dialog, borrowed for native tests that drive them like a user.
pub struct CamPanelParts<'a> {
    pub title: String,
    pub visible: bool,
    /// Each field with its control and the note beside it (the error of that field).
    pub rows: Vec<(DialogField, &'a NSControl, &'a NSTextField)>,
    /// Why fields are greyed: said once above the fields, not beside each of them.
    pub reasons: String,
    pub notes: &'a NSTextField,
    pub result: &'a NSTextField,
    pub capabilities: &'a NSTextField,
    pub buttons: Vec<(DialogActionKind, &'a NSButton)>,
    /// The up and down arrows of each `Order by` level.
    pub arrows: Vec<(OrderKey, &'a NSButton, &'a NSButton)>,
}

impl HostApp {
    /// The dialogs presentation has open, as the window in front shows them. Device Settings
    /// belongs to the app, not to a window: one panel, on the window in front.
    fn open_cam_dialogs(&self) -> Vec<(DocumentId, CamDialog, DialogViewModel)> {
        let vm = self.view();
        let Ok(controller) = self.controller.try_borrow() else { return Vec::new() };
        let mut open = Vec::new();
        for dialog in CamDialog::ALL {
            let documents: Vec<DocumentId> = if dialog == CamDialog::Device {
                vm.active.into_iter().collect()
            } else {
                vm.documents.iter().map(|d| d.id).collect()
            };
            for document in documents {
                if let Some(model) = controller.cam_dialog(document, dialog) {
                    open.push((document, dialog, model));
                }
            }
        }
        open
    }

    /// Shows, updates and drops the CAM panels to match presentation's open dialogs.
    pub(super) fn refresh_cam_panels(&self) {
        let open = self.open_cam_dialogs();
        let Ok(mut panels) = self.cam_panels.try_borrow_mut() else { return };
        let gone: Vec<(DocumentId, CamDialog)> =
            panels.keys().filter(|k| !open.iter().any(|(d, dialog, _)| (*d, *dialog) == **k)).copied().collect();
        for key in gone {
            if let Some(panel) = panels.remove(&key) {
                panel.clear_targets();
                panel.hide();
            }
        }
        for (document, dialog, model) in &open {
            let panel = panels.entry((*document, *dialog)).or_insert_with(|| {
                let mut panel = CamPanelUi::new(self.mtm, self.me.clone(), *document, model, self.locale);
                panel.refresh(model, self.locale);
                panel.show();
                panel
            });
            panel.refresh(model, self.locale);
        }
    }

    /// A field of a CAM dialog changed; a repeat of what the dialog shows sends nothing.
    pub(crate) fn cam_field_changed(
        &self,
        document: DocumentId,
        dialog: CamDialog,
        field: DialogField,
        value: DialogValue,
    ) {
        let model = self.controller.try_borrow().ok().and_then(|c| c.cam_dialog(document, dialog));
        if let Some(intent) = cam_field_intent(model.as_ref(), document, dialog, field, value) {
            self.dispatch(intent);
        }
    }

    /// An arrow beside an `Order by` level.
    pub(crate) fn cam_order_moved(&self, document: DocumentId, key: OrderKey, up: bool) {
        let model = self.controller.try_borrow().ok().and_then(|c| c.cam_dialog(document, CamDialog::Optimization));
        if let Some(intent) = model.and_then(|m| cam_move_intent(&m, document, key, up)) {
            self.dispatch(intent);
        }
    }

    /// A button of a CAM dialog. Save as RD asks for a path first.
    pub(crate) fn cam_button_pressed(&self, document: DocumentId, dialog: CamDialog, kind: DialogActionKind) {
        let model = self.controller.try_borrow().ok().and_then(|c| c.cam_dialog(document, dialog));
        let Some(model) = model else { return };
        if kind == DialogActionKind::ExportRd {
            if model.action(kind).is_some_and(|a| a.enabled) {
                self.show_test_save_panel(document, dialog);
            }
            return;
        }
        if let Some(intent) = cam_action_intent(&model, document, dialog, kind, None) {
            self.dispatch(intent);
        }
    }

    /// The save sheet of a test's RD file, on the dialog's panel.
    fn show_test_save_panel(&self, document: DocumentId, dialog: CamDialog) {
        let Ok(panels) = self.cam_panels.try_borrow() else { return };
        let Some(ui) = panels.get(&(document, dialog)) else { return };
        let panel = NSSavePanel::savePanel(self.mtm);
        let name = match dialog {
            CamDialog::IntervalTest => "intervaltest.rd",
            CamDialog::FocusTest => "focustest.rd",
            _ => "materiaaltest.rd",
        };
        panel.setNameFieldStringValue(&NSString::from_str(name));
        set_file_type(&panel, "rd");
        let me = self.me.clone();
        let reader = panel.clone();
        let handler = RcBlock::new(move |response: NSModalResponse| {
            crate::appkit::classes::ffi_guard("test save panel completion", (), || {
                let path = (response == crate::adapter::response::OK)
                    .then(|| reader.URL())
                    .flatten()
                    .and_then(|u| u.to_file_path());
                let Some(app) = me.upgrade() else { return };
                let model = app.controller.try_borrow().ok().and_then(|c| c.cam_dialog(document, dialog));
                if let (Some(model), Some(path)) = (model, path)
                    && let Some(intent) =
                        cam_action_intent(&model, document, dialog, DialogActionKind::ExportRd, Some(path))
                {
                    app.dispatch(intent);
                }
            });
        });
        if let Err(e) = objc_try("showing the test save sheet", || {
            panel.beginSheetModalForWindow_completionHandler(&ui.panel, &handler)
        }) {
            eprintln!("burnburn: {e}");
        }
    }

    /// Runs `f` on the widgets of the open dialog (native tests drive them like a user).
    pub fn with_cam_panel<R>(
        &self,
        document: DocumentId,
        dialog: CamDialog,
        f: impl FnOnce(&CamPanelParts<'_>) -> R,
    ) -> Option<R> {
        let panels = self.cam_panels.try_borrow().ok()?;
        let ui = panels.get(&(document, dialog))?;
        let parts = CamPanelParts {
            title: ui.panel.title().to_string(),
            visible: ui.panel.isVisible(),
            rows: ui.rows.iter().map(|r| (r.field, r.control_ref(), &*r.note)).collect(),
            reasons: ui.reasons.clone(),
            notes: &ui.notes,
            result: &ui.result,
            capabilities: &ui.capabilities,
            buttons: ui.buttons.iter().map(|(k, b)| (*k, &**b)).collect(),
            arrows: ui.rows.iter().filter_map(|r| r.arrows()).collect(),
        };
        Some(f(&parts))
    }

    /// The intent a menu choice sends, for tests that press the real menu item.
    #[allow(dead_code)]
    pub(crate) fn cam_menu_intent(&self, dialog: CamDialog) -> Option<UiIntent> {
        let document = self.view().active?;
        Some(UiIntent::OpenCamDialog { document, dialog })
    }
}
