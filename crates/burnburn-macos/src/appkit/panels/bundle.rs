//! The bundle sheet (M032/M033, P07): choosing what goes into a bundle, and reading what an
//! incoming bundle would change before anything changes.
//!
//! Export shows four check boxes (materials, art, profiles, templates); import shows the preview
//! line by line (new / identical / differs, profiles with the names of the differing fields), the
//! choice for differing items and one check box per differing profile ("Vervang profiel …"). The
//! controls call the intent builders in `crate::panels_files`; nothing is applied but by a click
//! on Importeer.
//!
//! One form (`files_form::bundle_form`, LF6b) with two sections the host swaps: what goes in (an
//! export) and what would change (an import).

use std::rc::{Rc, Weak};

use burnburn_host_models::form::ButtonRole;
use burnburn_host_models::panels::files_form::{bundle as id, bundle_form};
use burnburn_presentation::AppViewModel;
use burnburn_types::DocumentId;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSButton, NSPopUpButton, NSTextField, NSTextView, NSWindow};
use objc2_foundation::NSString;

use super::files_ui::{FilesTarget, Link, is_on, save_file, set_check, set_popup, set_report, unwire};
use super::form_surface::{FormSurface, form_sheet};
use super::image::{begin_sheet, end_sheet};
use super::widgets::set_text;
use crate::appkit::app::HostApp;
use crate::appkit::window::DocWindow;
use crate::l10n::Locale;
use crate::l10n_files::{FilesText as T, files_text};
use crate::panels_files::{
    bundle_apply_intent, bundle_close_intent, bundle_conflict_intent, bundle_kind_toggle_intent, bundle_panel,
    bundle_profile_intent, bundle_write_intent,
};

const WIDTH: f64 = 540.0;
/// Check boxes for the profiles of an incoming bundle (more are not offered: a bundle holds few).
pub(crate) const PROFILE_SLOTS: usize = id::PROFILE_SLOTS;

pub(crate) struct BundleSheet {
    pub(crate) sheet: Retained<NSWindow>,
    surface: FormSurface,
    pub(crate) shown: bool,
    pub(crate) title: Retained<NSTextField>,
    pub(crate) status: Retained<NSTextField>,
    pub(crate) kinds: Vec<Retained<NSButton>>,
    pub(crate) preview: Retained<NSTextView>,
    pub(crate) conflict: Retained<NSPopUpButton>,
    pub(crate) profiles: Vec<Retained<NSButton>>,
    pub(crate) report: Retained<NSTextField>,
    pub(crate) primary: Retained<NSButton>,
    pub(crate) close: Retained<NSButton>,
}

pub(crate) fn bundle_sheet(
    mtm: MainThreadMarker,
    target: &FilesTarget,
    app: Weak<HostApp>,
    document: DocumentId,
    locale: Locale,
) -> BundleSheet {
    let spec = bundle_form(locale);
    let surface = FormSurface::new(mtm, &[(&spec, WIDTH)]);
    let sheet = form_sheet(mtm, &surface, files_text(T::BundleExportTitle, locale));
    // A click never decides the mixed state of a kind: the intent follows the view model.
    let kinds: Vec<Retained<NSButton>> = (0..4).map(|i| surface.button(id::KIND + i)).collect();
    kinds.iter().for_each(|b| b.setAllowsMixedState(true));
    let profiles = (0..PROFILE_SLOTS as u32).map(|i| surface.button(id::PROFILE + i)).collect();
    let this = BundleSheet {
        sheet,
        shown: false,
        title: surface.text(id::TITLE),
        status: surface.text(id::STATUS),
        kinds,
        preview: surface.report(id::PREVIEW),
        conflict: surface.popup(id::CONFLICT),
        profiles,
        report: surface.text(id::REPORT),
        primary: surface.dialog_button(ButtonRole::Primary),
        close: surface.dialog_button(ButtonRole::Cancel),
        surface,
    };
    let link = Rc::new(Link { app, document });
    for (row, b) in this.kinds.iter().enumerate() {
        let link = link.clone();
        target.wire(b, move || {
            link.run(|app, doc, vm| {
                if let Some(i) = bundle_kind_toggle_intent(doc, vm, row as isize) {
                    app.dispatch(i);
                }
            });
        });
    }
    {
        let (link, conflict) = (link.clone(), this.conflict.clone());
        target.wire(&this.conflict, move || {
            link.run(|app, doc, _| {
                if let Some(i) = bundle_conflict_intent(doc, conflict.indexOfSelectedItem()) {
                    app.dispatch(i);
                }
            });
        });
    }
    for (slot, b) in this.profiles.iter().enumerate() {
        let (link, b2) = (link.clone(), b.clone());
        target.wire(b, move || {
            link.run(|app, doc, vm| {
                if let Some(i) = bundle_profile_intent(doc, vm, slot, is_on(&b2)) {
                    app.dispatch(i);
                }
            });
        });
    }
    {
        let link = link.clone();
        target.wire(&this.primary, move || {
            link.run(|app, doc, vm| {
                let Some(panel) = bundle_panel(vm, doc, locale) else { return };
                if panel.import {
                    app.dispatch(bundle_apply_intent(doc));
                    return;
                }
                let weak = link.app.clone();
                save_file(
                    mtm,
                    None,
                    files_text(T::PanelBundleSave, locale),
                    "bundel.burnburnbundle",
                    "burnburnbundle",
                    None,
                    "bundle save panel completion",
                    move |path, _| {
                        if let (Some(path), Some(app)) = (path, weak.upgrade()) {
                            app.dispatch(bundle_write_intent(doc, path));
                        }
                    },
                );
            });
        });
    }
    {
        let link = link.clone();
        target.wire(&this.close, move || {
            link.run(|app, doc, _| {
                app.dispatch(bundle_close_intent(doc));
            });
        });
    }
    this
}

impl BundleSheet {
    pub(crate) fn clear_targets(&self) {
        for c in self.kinds.iter().chain(self.profiles.iter()).chain([&self.primary, &self.close]) {
            unwire(c);
        }
        unwire(&self.conflict);
    }

    pub(crate) fn end(&self, window: &NSWindow) {
        if self.shown {
            end_sheet(window, &self.sheet, "ending the bundle sheet");
        }
    }
}

impl DocWindow {
    /// Show, update or end the bundle sheet as presentation's dialog comes and goes.
    pub(crate) fn refresh_bundle(&mut self, vm: &AppViewModel, document: DocumentId) {
        let locale = self.locale;
        let Some(model) = bundle_panel(vm, document, locale) else {
            if self.files.bundle.shown {
                self.files.bundle.shown = false;
                end_sheet(&self.window, &self.files.bundle.sheet, "ending the bundle sheet");
            }
            return;
        };
        let b = &mut self.files.bundle;
        b.sheet.setTitle(&NSString::from_str(&model.title));
        set_text(&b.title, &model.title);
        set_text(&b.status, &model.status);
        b.surface.set_hidden(id::STATUS, model.status.is_empty());
        let export = !model.import;
        b.surface.set_section_hidden(0, id::EXPORT_SECTION, !export);
        b.surface.set_section_hidden(0, id::IMPORT_SECTION, export);
        for (i, button) in b.kinds.iter().enumerate() {
            let row = model.kinds.get(i).filter(|_| export);
            b.surface.set_hidden(id::KIND + i as u32, row.is_none());
            if let Some(row) = row {
                if button.title().to_string() != row.title {
                    button.setTitle(&NSString::from_str(&row.title));
                }
                set_check(button, row.state);
                button.setEnabled(row.enabled);
            }
        }
        if !export {
            set_report(&b.preview, &model.preview);
            set_popup(&b.conflict, &model.conflict, true);
        }
        for (i, button) in b.profiles.iter().enumerate() {
            let row = model.profiles.get(i).filter(|_| !export);
            b.surface.set_hidden(id::PROFILE + i as u32, row.is_none());
            if let Some(row) = row {
                if button.title().to_string() != row.title {
                    button.setTitle(&NSString::from_str(&row.title));
                }
                set_check(
                    button,
                    if row.replace {
                        crate::panels_files::CheckState::On
                    } else {
                        crate::panels_files::CheckState::Off
                    },
                );
                button.setEnabled(row.enabled);
            }
        }
        set_text(&b.report, &model.report);
        b.surface.set_hidden(id::REPORT, model.report.is_empty());
        b.primary.setEnabled(if export { model.can_export } else { model.can_apply });
        let title = if export { files_text(T::ButtonExport, locale) } else { files_text(T::ButtonApplyBundle, locale) };
        if b.primary.title().to_string() != title {
            b.primary.setTitle(&NSString::from_str(title));
        }
        b.surface.fit_window(&b.sheet);
        if !b.shown {
            b.shown = true;
            begin_sheet(&self.window, &b.sheet, "showing the bundle sheet");
        }
    }
}
