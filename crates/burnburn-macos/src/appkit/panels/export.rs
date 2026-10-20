//! The export dialog (M026, P05): SVG and DXF options, and the list of everything the file
//! cannot carry, shown before anything is written. Sketch constraints and modifiers that would be
//! lost need the "Ik begrijp dat dit verloren gaat" check box before Exporteer… works.
//!
//! The controls call the intent builders in `crate::panels_files`; every option change sends the
//! whole form and presentation answers with the new report.

use std::rc::{Rc, Weak};

use burnburn_presentation::{AppViewModel, ExportField};
use burnburn_types::DocumentId;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSButton, NSColor, NSPopUpButton, NSTextField, NSTextView, NSView, NSWindow};
use objc2_foundation::NSString;

use super::files_ui::{
    FilesTarget, Link, checkbox, is_on, report_view, save_file, set_check, set_popup, set_report, unwire,
};
use super::image::{begin_sheet, caption, end_sheet, heading, input, mark_field, note, popup, sheet_window};
use super::widgets::{push_button, rect, set_text};
use crate::appkit::app::HostApp;
use crate::appkit::window::DocWindow;
use crate::l10n::Locale;
use crate::l10n_files::{FilesText as T, files_text};
use crate::panels_files::{
    CheckState, ExportForm, export_acknowledge_intent, export_close_intent, export_file_name, export_options_intent,
    export_panel, export_write_intent,
};

const SIZE: (f64, f64) = (540.0, 640.0);

pub(crate) struct ExportSheet {
    pub(crate) sheet: Retained<NSWindow>,
    pub(crate) shown: bool,
    pub(crate) title: Retained<NSTextField>,
    pub(crate) objects: Retained<NSTextField>,
    pub(crate) selection_only: Retained<NSButton>,
    pub(crate) unit_label: Retained<NSTextField>,
    pub(crate) unit: Retained<NSPopUpButton>,
    pub(crate) layers: Retained<NSPopUpButton>,
    pub(crate) curves_label: Retained<NSTextField>,
    pub(crate) curves: Retained<NSPopUpButton>,
    pub(crate) use_colors: Retained<NSButton>,
    pub(crate) stroke_label: Retained<NSTextField>,
    pub(crate) stroke: Retained<NSTextField>,
    pub(crate) decimals_label: Retained<NSTextField>,
    pub(crate) decimals: Retained<NSTextField>,
    pub(crate) tolerance_label: Retained<NSTextField>,
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
    let t = |text: T| files_text(text, locale);
    let (w, h) = SIZE;
    let (sheet, content) = sheet_window(mtm, SIZE, t(T::ExportSvgTitle));
    let inner = w - 32.0;
    let mut y = h - 16.0;
    let title = heading(mtm, &content, rect(16.0, y - 20.0, inner, 20.0));
    y -= 28.0;
    let objects = caption(mtm, &content, "", rect(16.0, y - 18.0, inner, 18.0));
    y -= 28.0;
    let selection_only = checkbox(mtm, t(T::CheckSelectionOnly), rect(16.0, y - 20.0, inner, 20.0));
    content.addSubview(&selection_only);
    y -= 30.0;
    let unit_label = caption(mtm, &content, t(T::LabelUnit), rect(16.0, y - 22.0, 110.0, 20.0));
    let unit = popup(mtm, &content, t(T::LabelUnit), &[], rect(130.0, y - 24.0, inner - 114.0, 26.0));
    y -= 32.0;
    caption(mtm, &content, t(T::LabelLayers), rect(16.0, y - 22.0, 110.0, 20.0));
    let layers = popup(mtm, &content, t(T::LabelLayers), &[], rect(130.0, y - 24.0, inner - 114.0, 26.0));
    y -= 32.0;
    let curves_label = caption(mtm, &content, t(T::LabelCurves), rect(16.0, y - 22.0, 110.0, 20.0));
    let curves = popup(mtm, &content, t(T::LabelCurves), &[], rect(130.0, y - 24.0, inner - 114.0, 26.0));
    y -= 32.0;
    let stroke_label = caption(mtm, &content, t(T::LabelStroke), rect(16.0, y - 22.0, 150.0, 20.0));
    let stroke = input(mtm, &content, t(T::LabelStroke), rect(170.0, y - 24.0, 90.0, 24.0));
    let decimals_label = caption(mtm, &content, t(T::LabelDecimals), rect(280.0, y - 22.0, 90.0, 20.0));
    let decimals = input(mtm, &content, t(T::LabelDecimals), rect(374.0, y - 24.0, 60.0, 24.0));
    decimals.setPlaceholderString(Some(&NSString::from_str(t(T::DecimalsHint))));
    let tolerance_label = caption(mtm, &content, t(T::LabelTolerance), rect(16.0, y - 22.0, 150.0, 20.0));
    let tolerance = input(mtm, &content, t(T::LabelTolerance), rect(170.0, y - 24.0, 90.0, 24.0));
    y -= 32.0;
    let use_colors = checkbox(mtm, t(T::CheckUseColors), rect(16.0, y - 20.0, inner, 20.0));
    content.addSubview(&use_colors);
    y -= 32.0;
    let losses = report_view(mtm, &content, rect(16.0, 112.0, inner, y - 112.0));
    let acknowledge = checkbox(mtm, t(T::ExportAcknowledge), rect(16.0, 84.0, inner, 20.0));
    content.addSubview(&acknowledge);
    let status = note(mtm, &content, rect(16.0, 48.0, inner, 32.0), &NSColor::secondaryLabelColor());
    let primary = push_button(mtm, t(T::ButtonExport));
    primary.setFrame(rect(w - 232.0, 12.0, 112.0, 28.0));
    primary.setKeyEquivalent(&NSString::from_str("\r"));
    content.addSubview(&primary);
    let close = push_button(mtm, t(T::ButtonClose));
    close.setFrame(rect(w - 112.0, 12.0, 96.0, 28.0));
    close.setKeyEquivalent(&NSString::from_str("\u{1b}"));
    content.addSubview(&close);
    sheet.setContentView(Some(&content));

    let this = ExportSheet {
        sheet,
        shown: false,
        title,
        objects,
        selection_only,
        unit_label,
        unit,
        layers,
        curves_label,
        curves,
        use_colors,
        stroke_label,
        stroke,
        decimals_label,
        decimals,
        tolerance_label,
        tolerance,
        losses,
        acknowledge,
        status,
        primary,
        close,
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
        let e = &self.files.export;
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
        for v in [&e.stroke_label as &NSView, &e.stroke, &e.decimals_label, &e.decimals] {
            v.setHidden(!model.show_stroke);
        }
        for v in [&e.curves_label as &NSView, &e.curves] {
            v.setHidden(!model.show_curves);
        }
        for v in [&e.tolerance_label as &NSView, &e.tolerance] {
            v.setHidden(!model.show_tolerance);
        }
        let _ = &e.unit_label;
        set_report(&e.losses, &model.losses);
        e.acknowledge.setHidden(!model.show_acknowledge);
        if e.acknowledge.title().to_string() != model.acknowledge_title {
            e.acknowledge.setTitle(&NSString::from_str(&model.acknowledge_title));
        }
        set_check(&e.acknowledge, if model.acknowledged { CheckState::On } else { CheckState::Off });
        set_text(&e.status, &model.status);
        e.primary.setEnabled(model.can_export);
        if !e.shown {
            self.files.export.shown = true;
            begin_sheet(&self.window, &self.files.export.sheet, "showing the export sheet");
        }
    }
}
