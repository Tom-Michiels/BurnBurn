//! The Windows panels that the Mac already builds from a [`FormSpec`] (LF13): CAM dialogs, Cut
//! Settings, the files family (export, bundle, material, art, import, image, tracer), the golf-5
//! panels, Schikken, the preferences and the machine hub. Each function takes the same headless
//! model the Mac builder takes and returns the shared form plus a route from every form id to what
//! the Windows host already acts on (a [`ControlId`] of [`super::react`], a preference, a hub
//! event). The old per-panel line lists (`forms::{cam, g5, ...}`) and the `PanelSpec` rows of
//! `host_panels` stay as the event handlers' input until the Win32 layer builds from here only.

use burnburn_host_models::app_view::PrefsPanel;
use burnburn_host_models::form::{Control, FormId, FormSpec, Platform};
use burnburn_host_models::hub_form::hub_form;
use burnburn_host_models::hub_panel::HubPanel;
use burnburn_host_models::l10n::Locale;
use burnburn_host_models::panels::files_form::{
    art_form, bundle_form, export_form, image_adjust_forms, image_import_form, import_form, material_form, tracer_forms,
};
use burnburn_host_models::panels::{
    CamTarget, CutTarget, CutTop, G5PanelModel, SchikkenPanel, cam_form, cut_form, g5_form, schikken_form,
};
use burnburn_host_models::prefs_form::{RESET_ID, prefs_form};
use burnburn_presentation::{CutSettingsViewModel, DialogViewModel, OperationKind, PrefKey};

use super::ControlId;
use super::spec_layout::{DluBase, SpecLayout, TextMeter, layout_spec};

/// What a control of a spec panel stands for.
#[derive(Debug, Clone, PartialEq)]
pub enum SpecRoute {
    /// A control of the editor panels: [`super::react`] knows what it means.
    Control(ControlId),
    /// A preference row of the settings window.
    Pref(PrefKey),
    /// "Restore defaults" of the settings window.
    PrefReset,
    /// A control of the machine hub (the id of `hub_form`).
    Hub(u32),
    /// A control of a sheet of the files family (the id of `files_form`).
    Files(FilesSheet, u32),
}

/// The sheets of the files family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilesSheet {
    Export,
    Bundle,
    Material,
    Art,
    Import,
    ImageImport,
    ImageAdjustValues,
    ImageAdjustPreview,
    TracerSettings,
    TracerPreview,
}

impl FilesSheet {
    pub const ALL: [FilesSheet; 10] = [
        FilesSheet::Export,
        FilesSheet::Bundle,
        FilesSheet::Material,
        FilesSheet::Art,
        FilesSheet::Import,
        FilesSheet::ImageImport,
        FilesSheet::ImageAdjustValues,
        FilesSheet::ImageAdjustPreview,
        FilesSheet::TracerSettings,
        FilesSheet::TracerPreview,
    ];
}

/// A shared form and the routes of its controls.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SpecPanel {
    pub spec: FormSpec,
    pub routes: Vec<(FormId, SpecRoute)>,
}

impl SpecPanel {
    /// What control `id` stands for.
    pub fn route(&self, id: FormId) -> Option<&SpecRoute> {
        self.routes.iter().find(|(i, _)| *i == id).map(|(_, r)| r)
    }

    /// The editor-panel control `id` stands for, for [`super::react`].
    pub fn control_id(&self, id: FormId) -> Option<&ControlId> {
        match self.route(id)? {
            SpecRoute::Control(c) => Some(c),
            _ => None,
        }
    }

    /// The rectangles of the panel at the DPI of `base`.
    pub fn layout(&self, base: DluBase, meter: &dyn TextMeter) -> SpecLayout {
        layout_spec(&self.spec, base, meter)
    }
}

/// Every id a form answers to: rows, tools, the buttons of an `Actions` row and the dialog buttons.
fn all_ids(spec: &FormSpec) -> Vec<FormId> {
    let mut ids = Vec::new();
    for row in spec.sections.iter().flat_map(|s| &s.rows) {
        match &row.control {
            Control::Actions { titles } => {
                ids.extend((0..titles.len()).map(|n| FormId(row.id.0 + u32::try_from(n).unwrap_or(0))));
            }
            Control::Pad { cells, .. } => {
                ids.extend((0..cells.len()).map(|n| FormId(row.id.0 + u32::try_from(n).unwrap_or(0))));
            }
            _ => ids.push(row.id),
        }
        ids.extend(row.tools.iter().map(|t| t.id));
    }
    ids.extend(spec.buttons.order(Platform::Windows).into_iter().map(|(_, b)| b.id));
    ids
}

/// A CAM dialog.
pub fn cam(model: &DialogViewModel, locale: Locale) -> SpecPanel {
    let form = cam_form(model, locale);
    let routes = form
        .targets
        .iter()
        .filter_map(|(id, target)| {
            let control = match *target {
                CamTarget::Field(f) => ControlId::Cam(f),
                CamTarget::Action(a) => ControlId::CamAction(a),
                CamTarget::Move(key, up) => ControlId::CamMove(key, up),
                CamTarget::Notes | CamTarget::Result | CamTarget::Capabilities => return None,
            };
            Some((*id, SpecRoute::Control(control)))
        })
        .collect();
    SpecPanel { spec: form.spec, routes }
}

/// The Cut Settings sheet.
pub fn cut_settings(
    sheet: Option<&CutSettingsViewModel>,
    top: &CutTop,
    kind: OperationKind,
    locale: Locale,
) -> SpecPanel {
    let extras = sheet.map(|s| s.extras.clone()).unwrap_or_default();
    let form = cut_form(sheet, top, &extras, kind, locale);
    let routes = form
        .targets
        .iter()
        .filter_map(|(id, target)| {
            let control = match *target {
                CutTarget::Mode => ControlId::CutMode,
                CutTarget::Speed => ControlId::CutSpeed,
                CutTarget::PowerMin => ControlId::CutPowerMin,
                CutTarget::PowerMax => ControlId::CutPowerMax,
                CutTarget::Passes => ControlId::CutPasses,
                CutTarget::Kerf => ControlId::CutKerf,
                CutTarget::Output => ControlId::CutOutput,
                CutTarget::Visible => ControlId::CutVisible,
                CutTarget::Option(field) => ControlId::CutOption(field),
                CutTarget::Ok => ControlId::CutOk,
                CutTarget::Cancel => ControlId::CutCancel,
                CutTarget::Error => return None,
            };
            Some((*id, SpecRoute::Control(control)))
        })
        .collect();
    SpecPanel { spec: form.spec, routes }
}

/// A golf-5 panel.
pub fn g5(model: &G5PanelModel) -> SpecPanel {
    let form = g5_form(&model.title, &model.lines);
    let routes = form.controls.iter().map(|(id, c)| (*id, SpecRoute::Control(ControlId::G5(*c)))).collect();
    SpecPanel { spec: form.spec, routes }
}

/// The Schikken panel.
pub fn schikken(panel: &SchikkenPanel) -> SpecPanel {
    let form = schikken_form(&panel.title, &panel.lines);
    let routes = form.controls.iter().map(|(id, c)| (*id, SpecRoute::Control(ControlId::Schikken(*c)))).collect();
    SpecPanel { spec: form.spec, routes }
}

/// The settings window.
pub fn prefs(panel: &PrefsPanel, locale: Locale) -> SpecPanel {
    let form = prefs_form(panel, locale);
    let mut routes: Vec<(FormId, SpecRoute)> = form.keys.iter().map(|(id, k)| (*id, SpecRoute::Pref(*k))).collect();
    routes.push((RESET_ID, SpecRoute::PrefReset));
    SpecPanel { spec: form.spec, routes }
}

/// The machine hub.
pub fn hub(panel: &HubPanel, locale: Locale) -> SpecPanel {
    let spec = hub_form(panel, locale);
    let routes = all_ids(&spec).into_iter().map(|id| (id, SpecRoute::Hub(id.0))).collect();
    SpecPanel { spec, routes }
}

/// A sheet of the files family.
pub fn files(sheet: FilesSheet, locale: Locale) -> SpecPanel {
    let spec = match sheet {
        FilesSheet::Export => export_form(locale),
        FilesSheet::Bundle => bundle_form(locale),
        FilesSheet::Material => material_form(locale),
        FilesSheet::Art => art_form(locale),
        FilesSheet::Import => import_form(locale),
        FilesSheet::ImageImport => image_import_form(locale),
        FilesSheet::ImageAdjustValues => image_adjust_forms(locale).0,
        FilesSheet::ImageAdjustPreview => image_adjust_forms(locale).1,
        FilesSheet::TracerSettings => tracer_forms(locale).0,
        FilesSheet::TracerPreview => tracer_forms(locale).1,
    };
    let routes = all_ids(&spec).into_iter().map(|id| (id, SpecRoute::Files(sheet, id.0))).collect();
    SpecPanel { spec, routes }
}
