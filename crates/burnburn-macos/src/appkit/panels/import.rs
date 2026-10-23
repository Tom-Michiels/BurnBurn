//! The import sheet: a sheet on the document window while presentation shows an import preview.

use crate::appkit::panels::files_ui::{set_popup, set_report};
use crate::appkit::panels::form_surface::{FormSurface, form_sheet, point_at};
use crate::appkit::panels::widgets::set_text;
use crate::appkit::window::DocWindow;

use burnburn_host_models::form::ButtonRole;
use burnburn_host_models::panels::files_form::{import as id, import_form};
use burnburn_presentation::{AppViewModel, CommandKey};
use burnburn_types::DocumentId;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::sel;
use objc2_app_kit::{NSButton, NSPopUpButton, NSTextField, NSTextView, NSWindow};

use crate::adapter::{MenuItem, menu_tag};
use crate::l10n::{HostText, Locale, host_text};
use crate::panels::import_panel;

const WIDTH: f64 = 520.0;

/// The import dialog: a sheet on the document window while presentation shows an import preview.
/// Its widgets come from a form (`files_form::import_form`, LF6b); the unit question, the pages
/// and the layers check are rows the file type does or does not have.
pub(crate) struct ImportSheet {
    pub(crate) sheet: Retained<NSWindow>,
    surface: FormSurface,
    /// File, unit prompt, size, report.
    pub(crate) labels: [Retained<NSTextField>; 4],
    /// The unit question (SVG and DXF only).
    pub(crate) units: Retained<NSPopUpButton>,
    /// PDF/AI: the pages to import.
    pub(crate) pages: Retained<NSPopUpButton>,
    /// PDF/AI and plotter files: accept the proposed layers (G7-trace, P04).
    pub(crate) layers: Retained<NSButton>,
    /// The full report: layers, values that were not taken over, warnings.
    pub(crate) report: Retained<NSTextView>,
    pub(crate) commit: Retained<NSButton>,
    pub(crate) cancel: Retained<NSButton>,
    pub(crate) shown: bool,
}

/// The import dialog's sheet window and widgets (not shown until presentation has a preview).
pub(crate) fn import_sheet(mtm: MainThreadMarker, target: &AnyObject, locale: Locale) -> ImportSheet {
    let spec = import_form(locale);
    let surface = FormSurface::new(mtm, &[(&spec, WIDTH)]);
    let sheet = form_sheet(mtm, &surface, host_text(HostText::ImportTitle, locale));
    let units = surface.popup(id::UNITS);
    point_at(&units, target, sel!(importUnitChosen:));
    let cancel = surface.dialog_button(ButtonRole::Cancel);
    point_at(&cancel, target, sel!(panelButton:));
    cancel.setTag(menu_tag(MenuItem::Command(CommandKey::CancelImport)));
    let commit = surface.dialog_button(ButtonRole::Primary);
    point_at(&commit, target, sel!(panelButton:));
    commit.setTag(menu_tag(MenuItem::Command(CommandKey::CommitImport)));
    ImportSheet {
        sheet,
        labels: [surface.text(id::FILE), surface.text(id::PROMPT), surface.text(id::SIZE), surface.text(id::SUMMARY)],
        units,
        pages: surface.popup(id::PAGES),
        layers: surface.button(id::LAYERS),
        report: surface.report(id::REPORT),
        commit,
        cancel,
        shown: false,
        surface,
    }
}

impl DocWindow {
    /// Show, update or end the import sheet as presentation's import preview comes and goes.
    pub(crate) fn refresh_import(&mut self, vm: &AppViewModel, document: DocumentId) {
        let Some(panel) = import_panel(vm, document, self.locale) else {
            if self.import.shown {
                self.import.shown = false;
                let (window, sheet) = (self.window.clone(), self.import.sheet.clone());
                if let Err(e) = crate::appkit::objc_try("ending the import sheet", || window.endSheet(&sheet)) {
                    eprintln!("burnburn: {e}");
                }
            }
            return;
        };
        let summary = panel.report.lines().next().unwrap_or_default();
        for (label, text) in self.import.labels.iter().zip([&panel.file, &panel.unit_prompt, &panel.size, summary]) {
            set_text(label, text);
        }
        set_report(&self.import.report, &panel.report);
        let has_pages = !panel.pages.items.is_empty();
        let surface = &mut self.import.surface;
        surface.set_hidden(id::PROMPT, panel.unit_prompt.is_empty());
        surface.set_hidden(id::UNITS, !panel.show_units);
        surface.set_hidden(id::PAGES, !has_pages);
        surface.set_hidden(id::LAYERS, panel.layers.is_none());
        set_popup(&self.import.pages, &panel.pages, true);
        self.import.layers.setState(if panel.layers == Some(true) {
            objc2_app_kit::NSControlStateValueOn
        } else {
            objc2_app_kit::NSControlStateValueOff
        });
        let selected = panel.selected_unit.map_or(-1, |i| i as isize);
        if self.import.units.indexOfSelectedItem() != selected {
            self.import.units.selectItemAtIndex(selected);
        }
        self.import.commit.setEnabled(panel.commit_enabled);
        self.import.surface.fit_window(&self.import.sheet);
        if !self.import.shown {
            let (window, sheet) = (self.window.clone(), self.import.sheet.clone());
            match crate::appkit::objc_try("showing the import sheet", || {
                window.beginSheet_completionHandler(&sheet, None)
            }) {
                Ok(()) => self.import.shown = true,
                Err(e) => eprintln!("burnburn: {e}"),
            }
        }
    }
}
