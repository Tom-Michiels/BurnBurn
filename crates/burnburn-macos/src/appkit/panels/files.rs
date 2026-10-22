//! The file panels of one window (W3-UI-bestanden): Material Library, Art Library, the bundle
//! sheet and the export dialog, with the one action target all their controls share.

use std::rc::Weak;

use burnburn_presentation::AppViewModel;
use burnburn_types::DocumentId;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSButton, NSImageView, NSPanel, NSPopUpButton, NSTextField, NSTextView, NSWindow};

use super::art::{ArtSection, art_section};
use super::bundle::{BundleSheet, bundle_sheet};
use super::export::{ExportSheet, export_sheet};
use super::files_ui::{FilesTarget, Link, unwire};
use super::material::{MaterialSection, material_section};
use crate::appkit::app::HostApp;
use crate::appkit::window::DocWindow;
use crate::l10n::Locale;

pub(crate) struct FilesSection {
    pub(crate) material: MaterialSection,
    pub(crate) art: ArtSection,
    pub(crate) bundle: BundleSheet,
    pub(crate) export: ExportSheet,
    target: Retained<FilesTarget>,
    /// The import sheet's page pop-up (PDF/AI), wired to this section's target.
    import_pages: Retained<NSPopUpButton>,
    /// The import sheet's check for the proposed layers.
    import_layers: Retained<objc2_app_kit::NSButton>,
}

pub(crate) fn files_section(
    mtm: MainThreadMarker,
    window: &Retained<NSWindow>,
    app: Weak<HostApp>,
    document: DocumentId,
    locale: Locale,
    import_pages: &Retained<NSPopUpButton>,
    import_layers: &Retained<objc2_app_kit::NSButton>,
) -> FilesSection {
    let target = FilesTarget::new(mtm);
    let app_for_pages = app.clone();
    let app_for_layers = app.clone();
    let material = material_section(mtm, &target, app.clone(), document, locale);
    let art = art_section(mtm, window.clone(), &target, app.clone(), document, locale);
    let bundle = bundle_sheet(mtm, &target, app.clone(), document, locale);
    let export = export_sheet(mtm, &target, app, document, locale);
    let link = std::rc::Rc::new(Link { app: app_for_pages, document });
    let pages = import_pages.clone();
    target.wire(import_pages, move || {
        link.run(|app, doc, vm| {
            if let Some(intent) = crate::panels_files::import_pages_intent(doc, vm, pages.indexOfSelectedItem()) {
                app.dispatch(intent);
            }
        });
    });
    let layers = import_layers.clone();
    let link = std::rc::Rc::new(Link { app: app_for_layers, document });
    target.wire(import_layers, move || {
        link.run(|app, doc, vm| {
            let on = layers.state() == objc2_app_kit::NSControlStateValueOn;
            if let Some(intent) = crate::panels_files::import_layers_intent(doc, vm, on) {
                app.dispatch(intent);
            }
        });
    });
    FilesSection {
        material,
        art,
        bundle,
        export,
        target,
        import_pages: import_pages.clone(),
        import_layers: import_layers.clone(),
    }
}

impl FilesSection {
    /// Clear every weak target (before the target goes) and hide the floating panels.
    pub(crate) fn clear_targets(&self) {
        self.material.clear_targets();
        self.art.clear_targets();
        self.bundle.clear_targets();
        self.export.clear_targets();
        unwire(&self.import_pages);
        unwire(&self.import_layers);
        let _ = &self.target;
    }

    /// End every sheet that is still up (window teardown).
    pub(crate) fn end_all(&self, window: &NSWindow) {
        self.bundle.end(window);
        self.export.end(window);
    }
}

impl DocWindow {
    /// Copy the file panels' view models into their widgets.
    pub(crate) fn refresh_files(&mut self, vm: &AppViewModel, document: DocumentId) {
        self.refresh_material(vm, document);
        self.refresh_art(vm, document);
        self.refresh_bundle(vm, document);
        self.refresh_export(vm, document);
    }
}

/// The native parts of a window's file panels, for tests. A panel or sheet is `Some` only while
/// it is up.
pub struct FilesParts<'a> {
    pub material_panel: Option<&'a NSPanel>,
    pub material_status: &'a NSTextField,
    pub material_material: &'a NSPopUpButton,
    pub material_thickness: &'a NSTextField,
    pub material_entries: &'a NSPopUpButton,
    pub material_detail: &'a NSTextField,
    pub material_layer: &'a NSPopUpButton,
    pub material_apply: &'a NSButton,
    pub material_update: &'a NSButton,
    pub material_update_apply: &'a NSButton,
    pub material_overwrite: &'a NSButton,
    pub material_remove: &'a NSButton,
    pub material_save: &'a NSButton,
    /// Name, material and thickness fields for a new entry.
    pub material_inputs: [&'a NSTextField; 3],
    pub art_panel: Option<&'a NSPanel>,
    pub art_status: &'a NSTextField,
    pub art_groups: &'a NSPopUpButton,
    pub art_items: &'a NSPopUpButton,
    pub art_thumbnail: &'a NSImageView,
    pub art_place: &'a NSButton,
    pub art_name: &'a NSTextField,
    pub art_new_group: &'a NSButton,
    pub art_add_selection: &'a NSButton,
    pub art_scale_prompt: &'a NSTextField,
    pub bundle_sheet: Option<&'a NSWindow>,
    pub bundle_status: &'a NSTextField,
    pub bundle_kinds: &'a [Retained<NSButton>],
    pub bundle_preview: &'a NSTextView,
    pub bundle_conflict: &'a NSPopUpButton,
    pub bundle_profiles: &'a [Retained<NSButton>],
    pub bundle_report: &'a NSTextField,
    pub bundle_primary: &'a NSButton,
    pub export_sheet: Option<&'a NSWindow>,
    pub export_title: &'a NSTextField,
    pub export_objects: &'a NSTextField,
    pub export_unit: &'a NSPopUpButton,
    pub export_layers: &'a NSPopUpButton,
    pub export_stroke: &'a NSTextField,
    pub export_losses: &'a NSTextView,
    pub export_acknowledge: &'a NSButton,
    pub export_status: &'a NSTextField,
    pub export_primary: &'a NSButton,
}

impl DocWindow {
    pub(crate) fn files_parts(&self) -> FilesParts<'_> {
        let f = &self.files;
        FilesParts {
            material_panel: f.material.shown.then_some(&*f.material.panel),
            material_status: &f.material.status,
            material_material: &f.material.material,
            material_thickness: &f.material.thickness,
            material_entries: &f.material.entries,
            material_detail: &f.material.detail,
            material_layer: &f.material.layer,
            material_apply: &f.material.apply,
            material_update: &f.material.update,
            material_update_apply: &f.material.update_apply,
            material_overwrite: &f.material.overwrite,
            material_remove: &f.material.remove,
            material_save: &f.material.save,
            material_inputs: [&f.material.name, &f.material.store_material, &f.material.store_thickness],
            art_panel: f.art.shown.then_some(&*f.art.panel),
            art_status: &f.art.status,
            art_groups: &f.art.groups,
            art_items: &f.art.items,
            art_thumbnail: &f.art.thumbnail,
            art_place: &f.art.place,
            art_name: &f.art.name,
            art_new_group: &f.art.new_group,
            art_add_selection: &f.art.add_selection,
            art_scale_prompt: &f.art.scale_prompt,
            bundle_sheet: f.bundle.shown.then_some(&*f.bundle.sheet),
            bundle_status: &f.bundle.status,
            bundle_kinds: &f.bundle.kinds,
            bundle_preview: &f.bundle.preview,
            bundle_conflict: &f.bundle.conflict,
            bundle_profiles: &f.bundle.profiles,
            bundle_report: &f.bundle.report,
            bundle_primary: &f.bundle.primary,
            export_sheet: f.export.shown.then_some(&*f.export.sheet),
            export_title: &f.export.title,
            export_objects: &f.export.objects,
            export_unit: &f.export.unit,
            export_layers: &f.export.layers,
            export_stroke: &f.export.stroke,
            export_losses: &f.export.losses,
            export_acknowledge: &f.export.acknowledge,
            export_status: &f.export.status,
            export_primary: &f.export.primary,
        }
    }
}
