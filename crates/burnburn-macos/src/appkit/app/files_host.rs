//! Host side of the file features (W3-UI-bestanden): the library folder and the layer palette,
//! the recent-templates menu and its stored list, and the native file panels the controller asks
//! for (Save As Template, New from Template, Import Bundle). A child module of `app`, so it can
//! use the host's private state.

use std::path::PathBuf;

use burnburn_types::DocumentId;

use super::{HostApp, fill_recent_templates_menu};
use crate::appkit::panels::files_ui::{open_file, save_file};
use crate::l10n_files::{FilesText as T, files_text};
use crate::panels_files::{
    bundle_open_intent, machine_profile_open_intent, recent_template_intent, template_open_intent,
    template_save_intent, template_scope_titles,
};

/// File extension of a template (the same as `burnburn_io::template::TEMPLATE_EXTENSION`).
const TEMPLATE: &str = "burnburntemplate";
/// File extension of a bundle (the same as `burnburn_io::bundle::BUNDLE_EXTENSION`).
const BUNDLE: &str = "burnburnbundle";
/// File extension of a machine profile file (JSON).
const MACHINE_PROFILE: &str = "json";

/// The layer palette as 8-bit sRGB, slot by slot (the canvas colours, so exports match the screen).
fn layer_palette() -> Vec<[u8; 3]> {
    let channel = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
    burnburn_render::Theme::light().process_layers.iter().map(|c| [channel(c.r), channel(c.g), channel(c.b)]).collect()
}

impl HostApp {
    /// Where the libraries and the recent-templates list live: next to the recent-projects file
    /// (`~/Library/Application Support/BurnBurn/`). Hosts without that file keep everything in memory.
    fn files_folder(&self) -> Option<PathBuf> {
        self.options.recent_projects_file.as_ref().and_then(|f| f.parent()).map(PathBuf::from)
    }

    fn recent_templates_file(&self) -> Option<PathBuf> {
        self.files_folder().map(|d| d.join("recent-templates.json"))
    }

    /// Hand the controller the library folder and the palette, and read the recent templates.
    pub(super) fn init_files(&self) {
        let folder = self.files_folder();
        let paths = self.recent_templates_file().map(|f| crate::recent::read_recent_projects(&f)).unwrap_or_default();
        if let Ok(mut controller) = self.controller.try_borrow_mut() {
            controller.set_library_dir(folder);
            controller.set_layer_palette(layer_palette());
            controller.handle(burnburn_presentation::UiIntent::Files(
                burnburn_presentation::FilesIntent::SetRecentTemplates { paths },
            ));
        }
        self.rebuild_recent_templates_menu(&self.recent_template_paths());
    }

    pub(super) fn recent_template_paths(&self) -> Vec<PathBuf> {
        self.controller.try_borrow().map(|c| c.recent_templates()).unwrap_or_default()
    }

    /// The recent-templates list changed: store it and rebuild Archief ▸ Recente templates.
    pub(super) fn recent_templates_changed(&self, paths: &[PathBuf]) {
        if let Some(file) = self.recent_templates_file()
            && let Err(e) = crate::recent::write_recent_projects(&file, paths)
        {
            eprintln!("burnburn: recent templates {}: {e}", file.display());
        }
        self.rebuild_recent_templates_menu(paths);
    }

    fn rebuild_recent_templates_menu(&self, paths: &[PathBuf]) {
        fill_recent_templates_menu(self.mtm, &self._target, &self.recent_templates_menu, paths, self.locale);
    }

    /// A recent template chosen in the menu.
    pub(super) fn open_recent_template(&self, n: usize) {
        if let Some(intent) = recent_template_intent(n, &self.recent_template_paths()) {
            self.dispatch(intent);
        }
    }

    /// Archief ▸ Bewaar als template…: where, and whether the template keeps the shapes.
    pub(super) fn show_template_save_panel(&self, document: DocumentId, suggested_name: &str) {
        let window = self.window(document);
        let me = self.me.clone();
        let titles = template_scope_titles(self.locale);
        save_file(
            self.mtm,
            window.as_deref(),
            files_text(T::PanelTemplateSave, self.locale),
            suggested_name,
            TEMPLATE,
            Some((files_text(T::PanelTemplateContent, self.locale), &titles)),
            "template save panel completion",
            move |path, scope| {
                let (Some(path), Some(app)) = (path, me.upgrade()) else { return };
                if let Some(intent) = template_save_intent(document, path, scope) {
                    app.dispatch(intent);
                }
            },
        );
    }

    /// Archief ▸ Nieuw van template…: a template to open as an untitled copy.
    pub(super) fn show_template_open_panel(&self) {
        let me = self.me.clone();
        open_file(
            self.mtm,
            None,
            files_text(T::PanelTemplateOpen, self.locale),
            &[TEMPLATE],
            "template open panel completion",
            move |path| {
                if let (Some(path), Some(app)) = (path, me.upgrade()) {
                    app.dispatch(template_open_intent(path));
                }
            },
        );
    }

    /// Archief ▸ Machineprofiel openen…: a profile file (Ruida or GRBL JSON) for the active machine.
    pub(super) fn show_machine_profile_open_panel(&self) {
        let me = self.me.clone();
        open_file(
            self.mtm,
            None,
            files_text(T::PanelMachineProfileOpen, self.locale),
            &[MACHINE_PROFILE],
            "machine profile open panel completion",
            move |path| {
                if let (Some(path), Some(app)) = (path, me.upgrade()) {
                    app.dispatch(machine_profile_open_intent(path));
                }
            },
        );
    }

    /// Archief ▸ Bundels ▸ Importeren…: a bundle file to read and preview.
    pub(super) fn show_bundle_open_panel(&self, document: DocumentId) {
        let window = self.window(document);
        let me = self.me.clone();
        open_file(
            self.mtm,
            window.as_deref(),
            files_text(T::PanelBundleOpen, self.locale),
            &[BUNDLE],
            "bundle open panel completion",
            move |path| {
                if let (Some(path), Some(app)) = (path, me.upgrade()) {
                    app.dispatch(bundle_open_intent(document, path));
                }
            },
        );
    }
}
