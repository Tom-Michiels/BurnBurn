//! The export dialog (M026, P05): SVG and DXF options, and the list of everything the file
//! cannot carry, shown before anything is written. Sketch constraints and modifiers that would be
//! lost need the "Ik begrijp dat dit verloren gaat" check box before Exporteer… works.
//!
//! The controls call the intent builders in `crate::panels_files`; every option change sends the
//! whole form and presentation answers with the new report. The sheet is a form built from
//! `files_form::export_form` (LF6b); the options a format does not have are hidden rows.

use std::rc::{Rc, Weak};

use burnburn_host_models::form::ButtonRole;
use burnburn_host_models::panels::files_form::{export as id, export_form};
use burnburn_presentation::{AppViewModel, ExportField};
use burnburn_types::DocumentId;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSButton, NSPopUpButton, NSTextField, NSTextView, NSWindow};
use objc2_foundation::NSString;

use super::files_ui::{FilesTarget, Link, is_on, save_file, set_check, set_popup, set_report, unwire};
use super::form_surface::{FormSurface, form_sheet};
use super::image::{begin_sheet, end_sheet, mark_field};
use super::widgets::set_text;
use crate::appkit::app::HostApp;
use crate::appkit::window::DocWindow;
use crate::l10n::Locale;
use crate::l10n_files::{FilesText as T, files_text};
use crate::panels_files::{
    CheckState, ExportForm, export_acknowledge_intent, export_close_intent, export_file_name, export_options_intent,
    export_panel, export_write_intent,
};

const WIDTH: f64 = 540.0;

pub(crate) struct ExportSheet {
    pub(crate) sheet: Retained<NSWindow>,
    surface: FormSurface,
    pub(crate) shown: bool,
    pub(crate) title: Retained<NSTextField>,
    pub(crate) objects: Retained<NSTextField>,
    pub(crate) selection_only: Retained<NSButton>,
    pub(crate) unit: Retained<NSPopUpButton>,
    pub(crate) layers: Retained<NSPopUpButton>,
    pub(crate) curves: Retained<NSPopUpButton>,
    pub(crate) use_colors: Retained<NSButton>,
    pub(crate) stroke: Retained<NSTextField>,
    pub(crate) decimals: Retained<NSTextField>,
    pub(crate) tolerance: Retained<NSTextField>,
    pub(crate) losses: Retained<NSTextView>,
    pub(crate) acknowledge: Retained<NSButton>,
    pub(crate) status: Retained<NSTextField>,
    pub(crate) primary: Retained<NSButton>,
    pub(crate) close: Retained<NSButton>,
}

/// The form as the controls hold it now.
struct Controls {
    selection_only: Retained<NSButton>,
    unit: Retained<NSPopUpButton>,
    layers: Retained<NSPopUpButton>,
    use_colors: Retained<NSButton>,
    stroke: Retained<NSTextField>,
    decimals: Retained<NSTextField>,
    curves: Retained<NSPopUpButton>,
    tolerance: Retained<NSTextField>,
}

impl Controls {
    fn form(&self) -> ExportForm {
        ExportForm {
            selection_only: is_on(&self.selection_only),
            unit: self.unit.indexOfSelectedItem(),
            layers: self.layers.indexOfSelectedItem(),
            use_colors: is_on(&self.use_colors),
            stroke: self.stroke.stringValue().to_string(),
            decimals: self.decimals.stringValue().to_string(),
            curves: self.curves.indexOfSelectedItem(),
            tolerance: self.tolerance.stringValue().to_string(),
        }
    }
}

pub(crate) fn export_sheet(
    mtm: MainThreadMarker,
    target: &FilesTarget,
    app: Weak<HostApp>,
    document: DocumentId,
    locale: Locale,
) -> ExportSheet {
    let spec = export_form(locale);
    let surface = FormSurface::new(mtm, &[(&spec, WIDTH)]);
    let sheet = form_sheet(mtm, &surface, files_text(T::ExportSvgTitle, locale));
    let this = ExportSheet {
        sheet,
        shown: false,
        title: surface.text(id::TITLE),
        objects: surface.text(id::OBJECTS),
        selection_only: surface.button(id::SELECTION_ONLY),
        unit: surface.popup(id::UNIT),
        layers: surface.popup(id::LAYERS),
        curves: surface.popup(id::CURVES),
        use_colors: surface.button(id::USE_COLORS),
        stroke: surface.text(id::STROKE),
        decimals: surface.text(id::DECIMALS),
        tolerance: surface.text(id::TOLERANCE),
        losses: surface.report(id::LOSSES),
        acknowledge: surface.button(id::ACKNOWLEDGE),
        status: surface.text(id::STATUS),
        primary: surface.dialog_button(ButtonRole::Primary),
        close: surface.dialog_button(ButtonRole::Cancel),
        surface,
    };
    let link = Rc::new(Link { app, document });
    let controls = Rc::new(Controls {
        selection_only: this.selection_only.clone(),
        unit: this.unit.clone(),
        layers: this.layers.clone(),
        use_colors: this.use_colors.clone(),
        stroke: this.stroke.clone(),
        decimals: this.decimals.clone(),
        curves: this.curves.clone(),
        tolerance: this.tolerance.clone(),
    });
    let option = |c: &objc2_app_kit::NSControl| {
        let (link, controls) = (link.clone(), controls.clone());
        target.wire(c, move || {
            link.run(|app, doc, vm| {
                if let Some(i) = export_options_intent(doc, vm, &controls.form()) {
                    app.dispatch(i);
                }
            });
        });
    };
    for c in [
        &*this.selection_only as &objc2_app_kit::NSControl,
        &this.unit,
        &this.layers,
        &this.curves,
        &this.use_colors,
        &this.stroke,
        &this.decimals,
        &this.tolerance,
    ] {
        option(c);
    }
    {
        let (link, ack) = (link.clone(), this.acknowledge.clone());
        target.wire(&this.acknowledge, move || {
            link.run(|app, doc, _| {
                app.dispatch(export_acknowledge_intent(doc, is_on(&ack)));
            });
        });
    }
    {
        let link = link.clone();
        target.wire(&this.primary, move || {
            link.run(|_, doc, vm| {
                let Some(d) = vm.document(doc).and_then(|d| d.files.export.as_ref()) else { return };
                let (format, name) = (
                    d.format,
                    export_file_name(&vm.document(doc).map(|d| d.title.clone()).unwrap_or_default(), d.format),
                );
                let extension = if format == burnburn_presentation::ExportFormat::Svg { "svg" } else { "dxf" };
                let weak = link.app.clone();
                save_file(
                    mtm,
                    None,
                    files_text(T::PanelExportSave, locale),
                    &name,
                    extension,
                    None,
                    "export save panel completion",
                    move |path, _| {
                        if let (Some(path), Some(app)) = (path, weak.upgrade()) {
                            app.dispatch(export_write_intent(doc, path));
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
                app.dispatch(export_close_intent(doc));
            });
        });
    }
    this
}

impl ExportSheet {
    pub(crate) fn clear_targets(&self) {
        let controls: [&objc2_app_kit::NSControl; 11] = [
            &self.selection_only,
            &self.unit,
            &self.layers,
            &self.curves,
            &self.use_colors,
            &self.stroke,
            &self.decimals,
            &self.tolerance,
            &self.acknowledge,
            &self.primary,
            &self.close,
        ];
        for c in controls {
            unwire(c);
        }
    }

    pub(crate) fn end(&self, window: &NSWindow) {
        if self.shown {
            end_sheet(window, &self.sheet, "ending the export sheet");
        }
    }
}

impl DocWindow {
    /// Show, update or end the export dialog as presentation's dialog comes and goes.
    pub(crate) fn refresh_export(&mut self, vm: &AppViewModel, document: DocumentId) {
        let locale = self.locale;
        let Some(model) = export_panel(vm, document, locale) else {
            if self.files.export.shown {
                self.files.export.shown = false;
                end_sheet(&self.window, &self.files.export.sheet, "ending the export sheet");
            }
            return;
        };
        let e = &mut self.files.export;
        e.sheet.setTitle(&NSString::from_str(&model.title));
        set_text(&e.title, &model.title);
        set_text(&e.objects, &model.objects);
        set_check(&e.selection_only, if model.selection_only { CheckState::On } else { CheckState::Off });
        set_check(&e.use_colors, if model.use_colors { CheckState::On } else { CheckState::Off });
        set_popup(&e.unit, &model.units, true);
        set_popup(&e.layers, &model.layers, true);
        set_popup(&e.curves, &model.curves, true);
        // A field in error keeps what the user typed so it can be corrected.
        for (field, text, error) in [
            (&e.stroke, &model.stroke, ExportField::Stroke),
            (&e.decimals, &model.decimals, ExportField::Decimals),
            (&e.tolerance, &model.tolerance, ExportField::Tolerance),
        ] {
            let in_error = model.field_error == Some(error);
            mark_field(field, in_error);
            if !in_error && field.stringValue().to_string() != *text {
                field.setStringValue(&NSString::from_str(text));
            }
        }
        e.surface.set_hidden(id::STROKE, !model.show_stroke);
        e.surface.set_hidden(id::DECIMALS, !model.show_stroke);
        e.surface.set_hidden(id::CURVES, !model.show_curves);
        e.surface.set_hidden(id::TOLERANCE, !model.show_tolerance);
        set_report(&e.losses, &model.losses);
        e.surface.set_hidden(id::ACKNOWLEDGE, !model.show_acknowledge);
        if e.acknowledge.title().to_string() != model.acknowledge_title {
            e.acknowledge.setTitle(&NSString::from_str(&model.acknowledge_title));
        }
        set_check(&e.acknowledge, if model.acknowledged { CheckState::On } else { CheckState::Off });
        set_text(&e.status, &model.status);
        e.surface.set_hidden(id::STATUS, model.status.is_empty());
        e.primary.setEnabled(model.can_export);
        e.surface.fit_window(&e.sheet);
        if !e.shown {
            e.shown = true;
            begin_sheet(&self.window, &e.sheet, "showing the export sheet");
        }
    }
}
