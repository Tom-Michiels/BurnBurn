//! The bundle sheet (M032/M033, P07): choosing what goes into a bundle, and reading what an
//! incoming bundle would change before anything changes.
//!
//! Export shows four check boxes (materials, art, profiles, templates); import shows the preview
//! line by line (new / identical / differs, profiles with the names of the differing fields), the
//! choice for differing items and one check box per differing profile ("Vervang profiel …"). The
//! controls call the intent builders in `crate::panels_files`; nothing is applied but by a click
//! on Importeer.

use std::rc::{Rc, Weak};

use burnburn_presentation::AppViewModel;
use burnburn_types::DocumentId;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSButton, NSColor, NSPopUpButton, NSTextField, NSTextView, NSWindow};
use objc2_foundation::NSString;

use super::files_ui::{
    FilesTarget, Link, checkbox, is_on, report_view, save_file, set_check, set_popup, set_report, tri_state_checkbox,
    unwire,
};
use super::image::{begin_sheet, caption, end_sheet, heading, note, popup, sheet_window};
use super::widgets::{push_button, rect, set_text};
use crate::appkit::app::HostApp;
use crate::appkit::window::DocWindow;
use crate::l10n::Locale;
use crate::l10n_files::{FilesText as T, files_text};
use crate::panels_files::{
    bundle_apply_intent, bundle_close_intent, bundle_conflict_intent, bundle_kind_toggle_intent, bundle_panel,
    bundle_profile_intent, bundle_write_intent,
};

const SIZE: (f64, f64) = (540.0, 600.0);
/// Check boxes for the profiles of an incoming bundle (more are not offered: a bundle holds few).
pub(crate) const PROFILE_SLOTS: usize = 4;

pub(crate) struct BundleSheet {
    pub(crate) sheet: Retained<NSWindow>,
    pub(crate) shown: bool,
    pub(crate) title: Retained<NSTextField>,
    pub(crate) status: Retained<NSTextField>,
    pub(crate) kinds: Vec<Retained<NSButton>>,
    pub(crate) choose: Retained<NSTextField>,
    pub(crate) preview: Retained<NSTextView>,
    pub(crate) conflict_label: Retained<NSTextField>,
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
    let t = |text: T| files_text(text, locale);
    let (w, h) = SIZE;
    let (sheet, content) = sheet_window(mtm, SIZE, t(T::BundleExportTitle));
    let inner = w - 32.0;
    let mut y = h - 16.0;
    let title = heading(mtm, &content, rect(16.0, y - 20.0, inner, 20.0));
    y -= 30.0;
    let status = note(mtm, &content, rect(16.0, y - 34.0, inner, 34.0), &NSColor::secondaryLabelColor());
    y -= 44.0;
    // Export: what goes in.
    let choose = caption(mtm, &content, t(T::BundleChoose), rect(16.0, y - 20.0, inner, 20.0));
    let mut kinds = Vec::new();
    for i in 0..4 {
        let b = tri_state_checkbox(mtm, rect(24.0, y - 50.0 - 28.0 * i as f64, inner - 16.0, 22.0));
        content.addSubview(&b);
        kinds.push(b);
    }
    // Import: the preview, the choice and the profile check boxes.
    let preview = report_view(mtm, &content, rect(16.0, 230.0, inner, y - 230.0 - 4.0));
    let conflict_label = caption(mtm, &content, t(T::LabelConflict), rect(16.0, 198.0, 220.0, 20.0));
    let conflict = popup(mtm, &content, t(T::LabelConflict), &[], rect(240.0, 194.0, inner - 224.0, 26.0));
    let mut profiles = Vec::new();
    for i in 0..PROFILE_SLOTS {
        let b = checkbox(mtm, "", rect(24.0, 168.0 - 24.0 * i as f64, inner - 16.0, 20.0));
        content.addSubview(&b);
        profiles.push(b);
    }
    let report = note(mtm, &content, rect(16.0, 56.0, inner, 48.0), &NSColor::labelColor());
    let primary = push_button(mtm, t(T::ButtonExport));
    primary.setFrame(rect(w - 128.0, 12.0, 112.0, 28.0));
    primary.setKeyEquivalent(&NSString::from_str("\r"));
    content.addSubview(&primary);
    let close = push_button(mtm, t(T::ButtonClose));
    close.setFrame(rect(w - 232.0, 12.0, 96.0, 28.0));
    close.setKeyEquivalent(&NSString::from_str("\u{1b}"));
    content.addSubview(&close);
    sheet.setContentView(Some(&content));

    let this = BundleSheet {
        sheet,
        shown: false,
        title,
        status,
        kinds,
        choose,
        preview,
        conflict_label,
        conflict,
        profiles,
        report,
        primary,
        close,
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
        let b = &self.files.bundle;
        b.sheet.setTitle(&NSString::from_str(&model.title));
        set_text(&b.title, &model.title);
        set_text(&b.status, &model.status);
        let export = !model.import;
        b.choose.setHidden(!export);
        for (i, button) in b.kinds.iter().enumerate() {
            let row = model.kinds.get(i).filter(|_| export);
            button.setHidden(row.is_none());
            if let Some(row) = row {
                if button.title().to_string() != row.title {
                    button.setTitle(&NSString::from_str(&row.title));
                }
                set_check(button, row.state);
                button.setEnabled(row.enabled);
            }
        }
        b.preview.enclosingScrollView().inspect(|s| s.setHidden(export));
        if !export {
            set_report(&b.preview, &model.preview);
        }
        b.conflict_label.setHidden(export);
        b.conflict.setHidden(export);
        if !export {
            set_popup(&b.conflict, &model.conflict, true);
        }
        for (i, button) in b.profiles.iter().enumerate() {
            let row = model.profiles.get(i).filter(|_| !export);
            button.setHidden(row.is_none());
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
        b.primary.setEnabled(if export { model.can_export } else { model.can_apply });
        let title = if export { files_text(T::ButtonExport, locale) } else { files_text(T::ButtonApplyBundle, locale) };
        if b.primary.title().to_string() != title {
            b.primary.setTitle(&NSString::from_str(title));
        }
        if !b.shown {
            self.files.bundle.shown = true;
            begin_sheet(&self.window, &self.files.bundle.sheet, "showing the bundle sheet");
        }
    }
}
