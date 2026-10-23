//! The Material Library panel (M217): a floating utility panel per document window with the
//! search (material, thickness, machine), the entries found, the layers of the project with
//! where their settings came from, and the buttons that copy library settings into a layer,
//! bring a changed library entry into the project on request, and store a layer as an entry.
//!
//! The panel copies `panels_files::material_panel` into widgets; every control calls one of the
//! intent builders next to it. Copied and linked settings are shown in the layer pop-up's titles.
//! The widgets come from a form (`files_form::material_form`, LF6b): sections with a heading, the
//! thickness with its unit as a suffix, the buttons of a section in one row.

#![allow(unsafe_code)] // a panel is owned by its Retained (SAFETY below)

use std::rc::{Rc, Weak};

use burnburn_host_models::form::ButtonRole;
use burnburn_host_models::panels::files_form::{material as id, material_form};
use burnburn_presentation::AppViewModel;
use burnburn_types::DocumentId;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSButton, NSPanel, NSPopUpButton, NSTextField};
use objc2_foundation::NSString;

use super::files_ui::{FilesTarget, Link, set_popup};
use super::form_surface::{FormSurface, form_panel};
use super::widgets::set_text;
use crate::appkit::app::HostApp;
use crate::appkit::window::DocWindow;
use crate::l10n::Locale;
use crate::l10n_files::{FilesText as T, files_text};
use crate::panels_files::{
    MaterialActions, material_actions, material_apply_intent, material_filter_intent, material_overwrite_intent,
    material_panel, material_remove_intent, material_save_intent, material_select_intent, material_update_intent,
};

pub(crate) const WIDTH: f64 = 580.0;

pub(crate) struct MaterialSection {
    pub(crate) panel: Retained<NSPanel>,
    surface: FormSurface,
    pub(crate) shown: bool,
    pub(crate) status: Retained<NSTextField>,
    pub(crate) material: Retained<NSPopUpButton>,
    pub(crate) thickness: Retained<NSTextField>,
    pub(crate) machine: Retained<NSPopUpButton>,
    pub(crate) entries: Retained<NSPopUpButton>,
    pub(crate) detail: Retained<NSTextField>,
    pub(crate) layer: Retained<NSPopUpButton>,
    pub(crate) apply: Retained<NSButton>,
    pub(crate) update: Retained<NSButton>,
    pub(crate) update_apply: Retained<NSButton>,
    pub(crate) overwrite: Retained<NSButton>,
    pub(crate) remove: Retained<NSButton>,
    pub(crate) save: Retained<NSButton>,
    pub(crate) name: Retained<NSTextField>,
    pub(crate) store_material: Retained<NSTextField>,
    pub(crate) store_thickness: Retained<NSTextField>,
    pub(crate) close: Retained<NSButton>,
}

pub(crate) fn material_section(
    mtm: MainThreadMarker,
    target: &FilesTarget,
    app: Weak<HostApp>,
    document: DocumentId,
    locale: Locale,
) -> MaterialSection {
    let spec = material_form(locale);
    let surface = FormSurface::new(mtm, &[(&spec, WIDTH)]);
    let panel = form_panel(mtm, &surface, files_text(T::MaterialTitle, locale));
    let section = MaterialSection {
        panel,
        shown: false,
        status: surface.text(id::STATUS),
        material: surface.popup(id::MATERIAL),
        thickness: surface.text(id::THICKNESS),
        machine: surface.popup(id::MACHINE),
        entries: surface.popup(id::ENTRIES),
        detail: surface.text(id::DETAIL),
        layer: surface.popup(id::LAYER),
        apply: surface.button(id::LAYER_ACTIONS),
        update: surface.button(id::LAYER_ACTIONS + 1),
        update_apply: surface.button(id::LAYER_ACTIONS + 2),
        save: surface.button(id::STORE_ACTIONS),
        overwrite: surface.button(id::STORE_ACTIONS + 1),
        remove: surface.button(id::STORE_ACTIONS + 2),
        name: surface.text(id::NAME),
        store_material: surface.text(id::STORE_MATERIAL),
        store_thickness: surface.text(id::STORE_THICKNESS),
        close: surface.dialog_button(ButtonRole::Cancel),
        surface,
    };
    section.wire(target, app, document);
    section
}

impl MaterialSection {
    fn wire(&self, target: &FilesTarget, app: Weak<HostApp>, document: DocumentId) {
        let link = Rc::new(Link { app, document });
        // Search: any of the three controls sends the whole search.
        let search = {
            let (link, material, thickness, machine) =
                (link.clone(), self.material.clone(), self.thickness.clone(), self.machine.clone());
            move || {
                link.run(|app, doc, vm| {
                    let intent = material_filter_intent(
                        doc,
                        vm,
                        material.indexOfSelectedItem(),
                        &thickness.stringValue().to_string(),
                        machine.indexOfSelectedItem(),
                    );
                    if let Some(intent) = intent {
                        app.dispatch(intent);
                    }
                });
            }
        };
        target.wire(&self.material, search.clone());
        target.wire(&self.machine, search.clone());
        target.wire(&self.thickness, search);
        {
            let (link, entries) = (link.clone(), self.entries.clone());
            target.wire(&self.entries, move || {
                link.run(|app, doc, vm| {
                    if let Some(intent) = material_select_intent(doc, vm, entries.indexOfSelectedItem()) {
                        app.dispatch(intent);
                    }
                });
            });
        }
        // The layer pop-up only changes which layer the buttons act on.
        {
            let (link, widgets) = (link.clone(), self.buttons());
            let layer = self.layer.clone();
            target.wire(&self.layer, move || {
                link.run(|_, doc, vm| widgets.enable(material_actions(vm, doc, layer.indexOfSelectedItem())));
            });
        }
        let on_layer =
            |button: &NSButton,
             build: fn(DocumentId, &AppViewModel, isize) -> Option<burnburn_presentation::UiIntent>| {
                let (link, layer) = (link.clone(), self.layer.clone());
                target.wire(button, move || {
                    link.run(|app, doc, vm| {
                        if let Some(intent) = build(doc, vm, layer.indexOfSelectedItem()) {
                            app.dispatch(intent);
                        }
                    });
                });
            };
        on_layer(&self.apply, material_apply_intent);
        on_layer(&self.overwrite, material_overwrite_intent);
        on_layer(&self.update, |d, vm, i| material_update_intent(d, vm, i, false));
        on_layer(&self.update_apply, |d, vm, i| material_update_intent(d, vm, i, true));
        {
            let (link, layer) = (link.clone(), self.layer.clone());
            let (name, material, thickness) =
                (self.name.clone(), self.store_material.clone(), self.store_thickness.clone());
            target.wire(&self.save, move || {
                link.run(|app, doc, vm| {
                    let intent = material_save_intent(
                        doc,
                        vm,
                        layer.indexOfSelectedItem(),
                        &name.stringValue().to_string(),
                        &material.stringValue().to_string(),
                        &thickness.stringValue().to_string(),
                    );
                    if let Some(intent) = intent {
                        app.dispatch(intent);
                    }
                });
            });
        }
        {
            let link = link.clone();
            target.wire(&self.remove, move || {
                link.run(|app, doc, vm| {
                    if let Some(intent) = material_remove_intent(doc, vm) {
                        app.dispatch(intent);
                    }
                });
            });
        }
        {
            let link = link.clone();
            target.wire(&self.close, move || {
                link.run(|app, doc, _| {
                    app.dispatch(burnburn_presentation::UiIntent::Invoke {
                        document: Some(doc),
                        command: burnburn_presentation::CommandKey::ShowMaterialLibrary,
                    });
                });
            });
        }
    }

    fn buttons(&self) -> ActionButtons {
        ActionButtons {
            apply: self.apply.clone(),
            update: self.update.clone(),
            update_apply: self.update_apply.clone(),
            overwrite: self.overwrite.clone(),
            remove: self.remove.clone(),
            save: self.save.clone(),
        }
    }

    pub(crate) fn clear_targets(&self) {
        use super::files_ui::unwire;
        for c in [&*self.material as &objc2_app_kit::NSControl, &self.machine, &self.entries, &self.layer] {
            unwire(c);
        }
        for c in
            [&*self.thickness as &objc2_app_kit::NSControl, &self.name, &self.store_material, &self.store_thickness]
        {
            unwire(c);
        }
        for c in [&self.apply, &self.update, &self.update_apply, &self.overwrite, &self.remove, &self.save, &self.close]
        {
            unwire(c);
        }
        if self.shown {
            self.panel.orderOut(None);
        }
    }
}

/// The buttons whose enablement follows the chosen layer.
#[derive(Clone)]
struct ActionButtons {
    apply: Retained<NSButton>,
    update: Retained<NSButton>,
    update_apply: Retained<NSButton>,
    overwrite: Retained<NSButton>,
    remove: Retained<NSButton>,
    save: Retained<NSButton>,
}

impl ActionButtons {
    fn enable(&self, a: MaterialActions) {
        self.apply.setEnabled(a.apply);
        self.update.setEnabled(a.update);
        self.update_apply.setEnabled(a.update);
        self.overwrite.setEnabled(a.overwrite);
        self.remove.setEnabled(a.remove);
        self.save.setEnabled(a.save);
    }
}

impl DocWindow {
    /// Show, update or hide the Material Library panel as presentation's panel comes and goes.
    pub(crate) fn refresh_material(&mut self, vm: &AppViewModel, document: DocumentId) {
        let locale = self.locale;
        let Some(model) = material_panel(vm, document, locale) else {
            if self.files.material.shown {
                self.files.material.shown = false;
                let panel = self.files.material.panel.clone();
                if let Err(e) = crate::appkit::objc_try("hiding the material panel", || panel.orderOut(None)) {
                    eprintln!("burnburn: {e}");
                }
            }
            return;
        };
        let m = &mut self.files.material;
        set_text(&m.status, &model.status);
        set_popup(&m.material, &model.materials, true);
        set_popup(&m.machine, &model.machines, true);
        if m.thickness.stringValue().to_string() != model.thickness
            && m.panel.firstResponder().is_none_or(|r| {
                let object: &objc2::runtime::AnyObject = &r;
                object.downcast_ref::<objc2_app_kit::NSText>().is_none()
            })
        {
            m.thickness.setStringValue(&NSString::from_str(&model.thickness));
        }
        set_popup(&m.entries, &model.entries, true);
        set_text(&m.detail, &model.detail);
        // The layer the user picked stays picked unless the list shrinks under it.
        set_popup(&m.layer, &model.operations, false);
        if m.layer.indexOfSelectedItem() < 0 && !model.operations.items.is_empty() {
            m.layer.selectItemAtIndex(0);
        }
        let actions = material_actions(vm, document, m.layer.indexOfSelectedItem());
        m.buttons().enable(actions);
        m.surface.fit_window(&m.panel);
        if !m.shown {
            m.shown = true;
            let panel = m.panel.clone();
            let frame = self.window.frame();
            if let Err(e) = crate::appkit::objc_try("showing the material panel", || {
                panel.setFrameTopLeftPoint(objc2_foundation::NSPoint::new(
                    frame.origin.x + frame.size.width - WIDTH - 24.0,
                    frame.origin.y + frame.size.height - 60.0,
                ));
                panel.orderFront(None);
            }) {
                eprintln!("burnburn: {e}");
            }
        }
    }
}
