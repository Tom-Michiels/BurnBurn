//! The file features on Windows (W4-L8b): open/recent, import report (SVG, DXF, HP-GL, PDF with
//! page choice, LightBurn project report), export SVG/DXF with options and the loss report (and a
//! confirmation before anything is dropped), templates, Material Library, Art Library and bundles
//! (with the diff and explicit choices).
//!
//! Every sheet is a [`PanelSpec`] built from the plain-data models shared with the Mac host
//! (`crate::panels_files`, `crate::panels::import_panel`); the events become the same `UiIntent`s.
//! Sheets are separate windows: [`Sheet`] says which, and [`sheet_open`] whether presentation has
//! it open for a document. Numbers go on as typed (DR-05).

use std::path::PathBuf;

use burnburn_presentation::{
    AppViewModel, CommandKey, ExportFormat, FilesIntent, HostEffect, MessageKey, TemplateScope, UiIntent,
};
use burnburn_types::DocumentId;

use super::filters::import_filters;
use super::ids::FILES_CONTROLS;
use super::spec::notify;
use super::{
    Chosen, ControlSpec, DialogRequest, FileFilter, FilePurpose, PanelEvent, PanelReaction, PanelSpec, Row, command_on,
};
use crate::l10n::{HostText, Locale, host_text, message_or_key};
use crate::l10n_files::{FilesText, files_text};
use crate::panels::{IMPORT_UNITS, import_panel, import_unit, import_unit_name};
use crate::panels_files::*;

/// The windows of the file family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Sheet {
    /// Open, recent projects, import, export, templates and the library/bundle commands.
    Start,
    Import,
    Export,
    Material,
    Art,
    Bundle,
}

impl Sheet {
    pub const ALL: [Sheet; 6] =
        [Sheet::Start, Sheet::Import, Sheet::Export, Sheet::Material, Sheet::Art, Sheet::Bundle];

    /// A small number that names the sheet in a window key.
    pub fn code(self) -> u8 {
        Sheet::ALL.iter().position(|s| *s == self).map_or(0, |i| i as u8)
    }

    pub fn from_code(code: u8) -> Option<Sheet> {
        Sheet::ALL.get(usize::from(code)).copied()
    }
}

/// Control ids of the file family (`7400..7600`), one block per sheet.
pub mod id {
    use super::FILES_CONTROLS as B;

    /// Start sheet: `B + 0 ..`.
    pub mod start {
        use super::B;
        pub const HEAD_PROJECTS: u16 = B;
        pub const OPEN: u16 = B + 1;
        pub const RECENT: u16 = B + 2;
        pub const OPEN_RECENT: u16 = B + 3;
        pub const CLEAR_RECENT: u16 = B + 4;
        pub const HEAD_IMPORT: u16 = B + 5;
        pub const IMPORT: u16 = B + 6;
        pub const IMPORT_IMAGE: u16 = B + 7;
        pub const HEAD_EXPORT: u16 = B + 8;
        pub const EXPORT_SVG: u16 = B + 9;
        pub const EXPORT_DXF: u16 = B + 10;
        pub const EXPORT_RD: u16 = B + 11;
        pub const HEAD_TEMPLATES: u16 = B + 12;
        pub const TEMPLATE_SCOPE: u16 = B + 13;
        pub const SAVE_TEMPLATE: u16 = B + 14;
        pub const NEW_TEMPLATE: u16 = B + 15;
        pub const RECENT_TEMPLATES: u16 = B + 16;
        pub const OPEN_RECENT_TEMPLATE: u16 = B + 17;
        pub const CLEAR_TEMPLATES: u16 = B + 18;
        pub const HEAD_LIBRARIES: u16 = B + 19;
        pub const MATERIAL: u16 = B + 20;
        pub const ART: u16 = B + 21;
        pub const BUNDLE_EXPORT: u16 = B + 22;
        pub const BUNDLE_IMPORT: u16 = B + 23;
    }

    /// Import report sheet: `B + 30 ..`.
    pub mod import {
        use super::B;
        pub const HEADING: u16 = B + 30;
        pub const FILE: u16 = B + 31;
        pub const UNIT_PROMPT: u16 = B + 32;
        pub const UNIT_CAPTION: u16 = B + 33;
        pub const UNIT: u16 = B + 34;
        pub const PAGES_CAPTION: u16 = B + 35;
        pub const PAGES: u16 = B + 36;
        pub const SIZE: u16 = B + 37;
        pub const REPORT: u16 = B + 38;
        pub const COMMIT: u16 = B + 39;
        pub const CANCEL: u16 = B + 40;
        /// The check that accepts the proposed layers (G7-trace, P04).
        pub const LAYERS: u16 = B + 41;
    }

    /// Export dialog: `B + 50 ..`.
    pub mod export {
        use super::B;
        pub const TITLE: u16 = B + 50;
        pub const OBJECTS: u16 = B + 51;
        pub const SELECTION_ONLY: u16 = B + 52;
        pub const UNIT_CAPTION: u16 = B + 53;
        pub const UNIT: u16 = B + 54;
        pub const LAYERS_CAPTION: u16 = B + 55;
        pub const LAYERS: u16 = B + 56;
        pub const CURVES_CAPTION: u16 = B + 57;
        pub const CURVES: u16 = B + 58;
        pub const USE_COLORS: u16 = B + 59;
        pub const STROKE_CAPTION: u16 = B + 60;
        pub const STROKE: u16 = B + 61;
        pub const DECIMALS_CAPTION: u16 = B + 62;
        pub const DECIMALS: u16 = B + 63;
        pub const TOLERANCE_CAPTION: u16 = B + 64;
        pub const TOLERANCE: u16 = B + 65;
        pub const LOSSES: u16 = B + 66;
        pub const ACKNOWLEDGE: u16 = B + 67;
        pub const STATUS: u16 = B + 68;
        pub const EXPORT: u16 = B + 69;
        pub const CLOSE: u16 = B + 70;
    }

    /// Material Library: `B + 90 ..`.
    pub mod material {
        use super::B;
        pub const STATUS: u16 = B + 90;
        pub const MATERIAL_CAPTION: u16 = B + 91;
        pub const MATERIAL: u16 = B + 92;
        pub const THICKNESS_CAPTION: u16 = B + 93;
        pub const THICKNESS: u16 = B + 94;
        pub const MACHINE_CAPTION: u16 = B + 95;
        pub const MACHINE: u16 = B + 96;
        pub const ENTRIES_CAPTION: u16 = B + 97;
        pub const ENTRIES: u16 = B + 98;
        pub const DETAIL: u16 = B + 99;
        pub const LAYER_CAPTION: u16 = B + 100;
        pub const LAYER: u16 = B + 101;
        pub const APPLY: u16 = B + 102;
        pub const UPDATE: u16 = B + 103;
        pub const UPDATE_APPLY: u16 = B + 104;
        pub const NAME_CAPTION: u16 = B + 105;
        pub const NAME: u16 = B + 106;
        pub const NEW_MATERIAL_CAPTION: u16 = B + 107;
        pub const NEW_MATERIAL: u16 = B + 108;
        pub const STORE_THICKNESS_CAPTION: u16 = B + 109;
        pub const STORE_THICKNESS: u16 = B + 110;
        pub const SAVE: u16 = B + 111;
        pub const OVERWRITE: u16 = B + 112;
        pub const REMOVE: u16 = B + 113;
        pub const CLOSE: u16 = B + 114;
    }

    /// Art Library: `B + 130 ..`.
    pub mod art {
        use super::B;
        pub const STATUS: u16 = B + 130;
        pub const GROUP_CAPTION: u16 = B + 131;
        pub const GROUP: u16 = B + 132;
        pub const SEARCH_CAPTION: u16 = B + 133;
        pub const SEARCH: u16 = B + 134;
        pub const ITEMS: u16 = B + 135;
        pub const THUMBNAIL: u16 = B + 136;
        pub const DETAIL: u16 = B + 137;
        pub const PLACE: u16 = B + 138;
        pub const NAME_CAPTION: u16 = B + 139;
        pub const NAME: u16 = B + 140;
        pub const ADD_FILE: u16 = B + 141;
        pub const ADD_SELECTION: u16 = B + 142;
        pub const SCALE_PROMPT: u16 = B + 143;
        pub const UNIT: u16 = B + 144;
        pub const RETRY: u16 = B + 145;
        pub const RENAME_ITEM: u16 = B + 146;
        pub const MOVE_ITEM: u16 = B + 147;
        pub const REMOVE_ITEM: u16 = B + 148;
        pub const NEW_GROUP: u16 = B + 149;
        pub const RENAME_GROUP: u16 = B + 150;
        pub const REMOVE_GROUP: u16 = B + 151;
        pub const CLOSE: u16 = B + 152;
    }

    /// Bundle dialog: `B + 170 ..`.
    pub mod bundle {
        use super::B;
        pub const TITLE: u16 = B + 170;
        pub const STATUS: u16 = B + 171;
        /// Four kind check boxes (materials, art, profiles, templates): `+0..+3`.
        pub const KIND: u16 = B + 172;
        pub const PREVIEW: u16 = B + 176;
        pub const CONFLICT_CAPTION: u16 = B + 177;
        pub const CONFLICT: u16 = B + 178;
        /// Replace-profile check boxes: `+0..+3`.
        pub const PROFILE: u16 = B + 179;
        pub const REPORT: u16 = B + 183;
        pub const PRIMARY: u16 = B + 184;
        pub const CLOSE: u16 = B + 185;
        pub const PROFILES: u16 = 4;
    }
}

/// What a sheet is built from.
pub struct FilesInput<'a> {
    pub document: DocumentId,
    pub vm: &'a AppViewModel,
    pub locale: Locale,
    pub recent_projects: &'a [PathBuf],
    pub recent_templates: &'a [PathBuf],
    /// The item of the template-scope choice (host state of the Start sheet).
    pub template_scope: isize,
    /// The row selected in the recent-projects list (host state).
    pub recent_row: Option<usize>,
    /// The row selected in the recent-templates list (host state).
    pub template_row: Option<usize>,
}

/// Whether presentation has this sheet open for `document` (the Start sheet is the user's choice).
pub fn sheet_open(sheet: Sheet, vm: &AppViewModel, document: DocumentId) -> bool {
    let Some(doc) = vm.document(document) else { return false };
    match sheet {
        Sheet::Start => false,
        Sheet::Import => doc.import.is_some(),
        Sheet::Export => doc.files.export.is_some(),
        Sheet::Material => doc.files.material.is_some(),
        Sheet::Art => doc.files.art.is_some(),
        Sheet::Bundle => doc.files.bundle.is_some(),
    }
}

fn ft(text: FilesText, locale: Locale) -> &'static str {
    files_text(text, locale)
}

fn cmd_name(command: CommandKey, locale: Locale) -> String {
    message_or_key(MessageKey::Command(command), locale)
}

fn cmd_button(input: &FilesInput<'_>, cid: u16, command: CommandKey) -> ControlSpec {
    ControlSpec::button(cid, cmd_name(command, input.locale), command_on(input.vm, input.document, command))
}

fn caption(cid: u16, text: &str) -> ControlSpec {
    ControlSpec::label(cid, text).weighted(2)
}

fn pair(caption_id: u16, caption_text: &str, control: ControlSpec) -> Row {
    Row::of(vec![caption(caption_id, caption_text), control])
}

fn combo_of(cid: u16, popup: &Popup, enabled: bool) -> ControlSpec {
    ControlSpec::combo(cid, popup.items.clone(), popup.selected, enabled)
}

fn file_stem(path: &std::path::Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

pub fn files_spec(sheet: Sheet, input: &FilesInput<'_>) -> PanelSpec {
    match sheet {
        Sheet::Start => start_spec(input),
        Sheet::Import => import_spec(input),
        Sheet::Export => export_spec(input),
        Sheet::Material => material_spec(input),
        Sheet::Art => art_spec(input),
        Sheet::Bundle => bundle_spec(input),
    }
}

fn start_spec(input: &FilesInput<'_>) -> PanelSpec {
    use id::start as s;
    let locale = input.locale;
    let open = input.vm.document(input.document).is_some();
    let recent: Vec<String> = input.recent_projects.iter().map(|p| file_stem(p)).collect();
    let templates: Vec<String> = input.recent_templates.iter().map(|p| file_stem(p)).collect();
    let rows = vec![
        Row::one(ControlSpec::heading(s::HEAD_PROJECTS, host_text(HostText::RecentProjects, locale))),
        Row::of(vec![
            ControlSpec::button(s::OPEN, host_text(HostText::Open, locale), true),
            ControlSpec::button(s::CLEAR_RECENT, host_text(HostText::ClearRecent, locale), !recent.is_empty()),
        ]),
        Row::one(ControlSpec::list(s::RECENT, 5, recent.clone(), input.recent_row.filter(|r| *r < recent.len()), true)),
        Row::one(ControlSpec::button(
            s::OPEN_RECENT,
            host_text(HostText::Open, locale),
            input.recent_row.is_some_and(|r| r < recent.len()),
        )),
        Row::one(ControlSpec::heading(s::HEAD_IMPORT, host_text(HostText::ImportTitle, locale))),
        Row::of(vec![
            ControlSpec::button(s::IMPORT, host_text(HostText::Import, locale), open),
            ControlSpec::button(s::IMPORT_IMAGE, host_text(HostText::ImageMenuImport, locale), open),
        ]),
        Row::one(ControlSpec::heading(s::HEAD_EXPORT, ft(FilesText::MenuExportSubmenu, locale))),
        Row::of(vec![
            cmd_button(input, s::EXPORT_SVG, CommandKey::ExportSvg),
            cmd_button(input, s::EXPORT_DXF, CommandKey::ExportDxf),
            cmd_button(input, s::EXPORT_RD, CommandKey::ExportRd),
        ]),
        Row::one(ControlSpec::heading(s::HEAD_TEMPLATES, ft(FilesText::PanelTemplateContent, locale))),
        Row::one(ControlSpec::combo(
            s::TEMPLATE_SCOPE,
            template_scope_titles(locale),
            usize::try_from(input.template_scope).ok(),
            true,
        )),
        Row::of(vec![
            cmd_button(input, s::SAVE_TEMPLATE, CommandKey::SaveAsTemplate),
            cmd_button(input, s::NEW_TEMPLATE, CommandKey::NewFromTemplate),
        ]),
        Row::one(ControlSpec::list(
            s::RECENT_TEMPLATES,
            4,
            templates.clone(),
            input.template_row.filter(|r| *r < templates.len()),
            true,
        )),
        Row::of(vec![
            ControlSpec::button(
                s::OPEN_RECENT_TEMPLATE,
                ft(FilesText::MenuNewFromRecentTemplate, locale),
                input.template_row.is_some_and(|r| r < templates.len()),
            ),
            ControlSpec::button(s::CLEAR_TEMPLATES, host_text(HostText::ClearRecent, locale), !templates.is_empty()),
        ]),
        Row::one(ControlSpec::heading(s::HEAD_LIBRARIES, ft(FilesText::MaterialTitle, locale))),
        Row::of(vec![
            cmd_button(input, s::MATERIAL, CommandKey::ShowMaterialLibrary),
            cmd_button(input, s::ART, CommandKey::ShowArtLibrary),
        ]),
        Row::of(vec![
            cmd_button(input, s::BUNDLE_EXPORT, CommandKey::ExportBundle),
            cmd_button(input, s::BUNDLE_IMPORT, CommandKey::ImportBundle),
        ]),
    ];
    PanelSpec { title: host_text(HostText::MenuFile, locale).to_owned(), rows }
}

fn import_spec(input: &FilesInput<'_>) -> PanelSpec {
    use id::import as i;
    let locale = input.locale;
    let title = host_text(HostText::ImportTitle, locale).to_owned();
    let Some(p) = import_panel(input.vm, input.document, locale) else { return PanelSpec { title, rows: Vec::new() } };
    let rows = vec![
        Row::one(ControlSpec::heading(i::HEADING, title.clone())),
        Row::one(ControlSpec::label(i::FILE, p.file.clone())),
        Row::one(ControlSpec::label(i::UNIT_PROMPT, p.unit_prompt.clone())),
        Row::of(vec![
            caption(i::UNIT_CAPTION, host_text(HostText::ImportUnitLabel, locale)).hidden_if(!p.show_units),
            ControlSpec::combo(
                i::UNIT,
                IMPORT_UNITS.iter().map(|u| import_unit_name(*u, locale).to_owned()).collect(),
                p.selected_unit,
                true,
            )
            .hidden_if(!p.show_units),
        ]),
        Row::of(vec![
            caption(i::PAGES_CAPTION, ft(FilesText::ImportPageLabel, locale)).hidden_if(p.pages.items.is_empty()),
            combo_of(i::PAGES, &p.pages, true).hidden_if(p.pages.items.is_empty()),
        ]),
        Row::one(
            ControlSpec::check(
                i::LAYERS,
                crate::panels_files::import_layers_check_text(locale),
                p.layers.unwrap_or(false),
                true,
            )
            .hidden_if(p.layers.is_none()),
        ),
        Row::one(ControlSpec::label(i::SIZE, p.size.clone())),
        Row::one(ControlSpec::report(i::REPORT, 10, p.report.clone())),
        Row::of(vec![
            ControlSpec::button(i::COMMIT, host_text(HostText::ImportCommit, locale), p.commit_enabled),
            ControlSpec::button(
                i::CANCEL,
                host_text(HostText::ImportCancel, locale),
                command_on(input.vm, input.document, CommandKey::CancelImport),
            ),
        ]),
    ];
    PanelSpec { title, rows }
}

fn export_spec(input: &FilesInput<'_>) -> PanelSpec {
    use id::export as e;
    let locale = input.locale;
    let Some(p) = export_panel(input.vm, input.document, locale) else { return PanelSpec::default() };
    let rows = vec![
        Row::one(ControlSpec::heading(e::TITLE, p.title.clone())),
        Row::one(ControlSpec::label(e::OBJECTS, p.objects.clone())),
        Row::one(ControlSpec::check(
            e::SELECTION_ONLY,
            ft(FilesText::CheckSelectionOnly, locale),
            p.selection_only,
            true,
        )),
        pair(e::UNIT_CAPTION, ft(FilesText::LabelUnit, locale), combo_of(e::UNIT, &p.units, true)),
        pair(e::LAYERS_CAPTION, ft(FilesText::LabelLayers, locale), combo_of(e::LAYERS, &p.layers, true)),
        Row::of(vec![
            caption(e::CURVES_CAPTION, ft(FilesText::LabelCurves, locale)).hidden_if(!p.show_curves),
            combo_of(e::CURVES, &p.curves, true).hidden_if(!p.show_curves),
        ]),
        Row::one(ControlSpec::check(e::USE_COLORS, ft(FilesText::CheckUseColors, locale), p.use_colors, true)),
        Row::of(vec![
            caption(e::STROKE_CAPTION, ft(FilesText::LabelStroke, locale)).hidden_if(!p.show_stroke),
            ControlSpec::edit(e::STROKE, p.stroke.clone(), true).hidden_if(!p.show_stroke),
        ]),
        Row::of(vec![
            caption(e::DECIMALS_CAPTION, ft(FilesText::LabelDecimals, locale)).hidden_if(!p.show_decimals),
            ControlSpec::edit(e::DECIMALS, p.decimals.clone(), true).hidden_if(!p.show_decimals),
        ]),
        Row::of(vec![
            caption(e::TOLERANCE_CAPTION, ft(FilesText::LabelTolerance, locale)).hidden_if(!p.show_tolerance),
            ControlSpec::edit(e::TOLERANCE, p.tolerance.clone(), true).hidden_if(!p.show_tolerance),
        ]),
        Row::one(ControlSpec::report(e::LOSSES, 8, p.losses.clone())),
        Row::one(
            ControlSpec::check(e::ACKNOWLEDGE, p.acknowledge_title.clone(), p.acknowledged, true)
                .hidden_if(!p.show_acknowledge),
        ),
        Row::one(ControlSpec::label(e::STATUS, p.status.clone())),
        Row::of(vec![
            ControlSpec::button(e::EXPORT, ft(FilesText::ButtonExport, locale), p.can_export),
            ControlSpec::button(e::CLOSE, ft(FilesText::ButtonClose, locale), true),
        ]),
    ];
    PanelSpec { title: p.title, rows }
}

fn material_spec(input: &FilesInput<'_>) -> PanelSpec {
    use id::material as m;
    let locale = input.locale;
    let title = ft(FilesText::MaterialTitle, locale).to_owned();
    let Some(p) = material_panel(input.vm, input.document, locale) else {
        return PanelSpec { title, rows: Vec::new() };
    };
    // The layer the buttons act on is host state; the first layer is the default, as on the Mac.
    let actions = material_actions(input.vm, input.document, 0);
    let rows = vec![
        Row::one(ControlSpec::label(m::STATUS, p.status.clone())),
        pair(m::MATERIAL_CAPTION, ft(FilesText::LabelMaterial, locale), combo_of(m::MATERIAL, &p.materials, true)),
        pair(
            m::THICKNESS_CAPTION,
            ft(FilesText::LabelThickness, locale),
            ControlSpec::edit(m::THICKNESS, p.thickness.clone(), true),
        ),
        pair(m::MACHINE_CAPTION, ft(FilesText::LabelMachine, locale), combo_of(m::MACHINE, &p.machines, true)),
        Row::one(ControlSpec::label(m::ENTRIES_CAPTION, ft(FilesText::LabelEntries, locale))),
        Row::one(ControlSpec::list(m::ENTRIES, 5, p.entries.items.clone(), p.entries.selected, true)),
        Row::one(ControlSpec::report(m::DETAIL, 3, p.detail.clone())),
        pair(m::LAYER_CAPTION, ft(FilesText::LabelLayer, locale), combo_of(m::LAYER, &p.operations, true)),
        Row::of(vec![
            ControlSpec::button(m::APPLY, ft(FilesText::ButtonApply, locale), actions.apply),
            ControlSpec::button(m::UPDATE, ft(FilesText::ButtonUpdate, locale), actions.update),
            ControlSpec::button(m::UPDATE_APPLY, ft(FilesText::ButtonUpdateApply, locale), actions.update),
        ]),
        pair(m::NAME_CAPTION, ft(FilesText::LabelName, locale), ControlSpec::edit(m::NAME, "", true)),
        pair(
            m::NEW_MATERIAL_CAPTION,
            ft(FilesText::LabelNewMaterial, locale),
            ControlSpec::edit(m::NEW_MATERIAL, "", true),
        ),
        pair(
            m::STORE_THICKNESS_CAPTION,
            ft(FilesText::LabelThickness, locale),
            ControlSpec::edit(m::STORE_THICKNESS, "", true),
        ),
        Row::of(vec![
            ControlSpec::button(m::SAVE, ft(FilesText::ButtonSave, locale), actions.save),
            ControlSpec::button(m::OVERWRITE, ft(FilesText::ButtonOverwrite, locale), actions.overwrite),
            ControlSpec::button(m::REMOVE, ft(FilesText::ButtonRemove, locale), actions.remove),
        ]),
        Row::one(ControlSpec::button(m::CLOSE, ft(FilesText::ButtonClose, locale), true)),
    ];
    PanelSpec { title, rows }
}

fn art_spec(input: &FilesInput<'_>) -> PanelSpec {
    use id::art as a;
    let locale = input.locale;
    let title = ft(FilesText::ArtTitle, locale).to_owned();
    let Some(p) = art_panel(input.vm, input.document, locale) else { return PanelSpec { title, rows: Vec::new() } };
    let prompt = p.scale_prompt.clone().unwrap_or_default();
    let rows = vec![
        Row::one(ControlSpec::label(a::STATUS, p.status.clone())),
        pair(a::GROUP_CAPTION, ft(FilesText::LabelGroup, locale), combo_of(a::GROUP, &p.groups, true)),
        pair(
            a::SEARCH_CAPTION,
            ft(FilesText::LabelSearch, locale),
            ControlSpec::edit(a::SEARCH, p.search.clone(), true),
        ),
        Row::one(ControlSpec::list(a::ITEMS, 6, p.items.items.clone(), p.items.selected, true)),
        Row::one(ControlSpec::bitmap(a::THUMBNAIL, 70, p.thumbnail.clone())),
        Row::one(ControlSpec::label(a::DETAIL, p.detail.clone())),
        Row::of(vec![
            ControlSpec::button(a::PLACE, ft(FilesText::ButtonPlace, locale), p.can_place),
            ControlSpec::button(a::REMOVE_ITEM, ft(FilesText::ButtonRemoveItem, locale), p.can_edit_item),
            ControlSpec::button(a::MOVE_ITEM, ft(FilesText::ButtonMoveItem, locale), p.can_edit_item),
        ]),
        pair(a::NAME_CAPTION, ft(FilesText::ArtNameHint, locale), ControlSpec::edit(a::NAME, "", true)),
        Row::of(vec![
            ControlSpec::button(a::ADD_FILE, ft(FilesText::ButtonAddFile, locale), true),
            ControlSpec::button(a::ADD_SELECTION, ft(FilesText::ButtonAddSelection, locale), true),
            ControlSpec::button(a::RENAME_ITEM, ft(FilesText::ButtonRenameItem, locale), p.can_edit_item),
        ]),
        Row::one(ControlSpec::label(a::SCALE_PROMPT, prompt).hidden_if(p.scale_prompt.is_none())),
        Row::of(vec![
            ControlSpec::combo(a::UNIT, art_unit_titles(), Some(0), true).hidden_if(p.scale_prompt.is_none()),
            ControlSpec::button(a::RETRY, ft(FilesText::ButtonAddFile, locale), true)
                .hidden_if(p.scale_prompt.is_none()),
        ]),
        Row::of(vec![
            ControlSpec::button(a::NEW_GROUP, ft(FilesText::ButtonNewGroup, locale), true),
            ControlSpec::button(a::RENAME_GROUP, ft(FilesText::ButtonRenameGroup, locale), p.can_edit_group),
            ControlSpec::button(a::REMOVE_GROUP, ft(FilesText::ButtonRemoveGroup, locale), p.can_edit_group),
        ]),
        Row::one(ControlSpec::button(a::CLOSE, ft(FilesText::ButtonClose, locale), true)),
    ];
    PanelSpec { title, rows }
}

fn bundle_spec(input: &FilesInput<'_>) -> PanelSpec {
    use id::bundle as b;
    let locale = input.locale;
    let Some(p) = bundle_panel(input.vm, input.document, locale) else { return PanelSpec::default() };
    let mut rows = vec![
        Row::one(ControlSpec::heading(b::TITLE, p.title.clone())),
        Row::one(ControlSpec::label(b::STATUS, p.status.clone())),
    ];
    for slot in 0..4u16 {
        let row = p.kinds.get(usize::from(slot));
        rows.push(Row::one(
            ControlSpec::check(
                b::KIND + slot,
                row.map_or_else(String::new, |r| r.title.clone()),
                row.is_some_and(|r| r.state == CheckState::On),
                row.is_some_and(|r| r.enabled),
            )
            .hidden_if(row.is_none()),
        ));
    }
    rows.push(Row::one(ControlSpec::report(b::PREVIEW, 10, p.preview.clone()).hidden_if(!p.import)));
    rows.push(Row::of(vec![
        caption(b::CONFLICT_CAPTION, ft(FilesText::LabelConflict, locale)).hidden_if(!p.import),
        combo_of(b::CONFLICT, &p.conflict, !p.busy).hidden_if(!p.import),
    ]));
    for slot in 0..b::PROFILES {
        let profile = p.profiles.get(usize::from(slot));
        rows.push(Row::one(
            ControlSpec::check(
                b::PROFILE + slot,
                profile.map_or_else(String::new, |x| x.title.clone()),
                profile.is_some_and(|x| x.replace),
                profile.is_some_and(|x| x.enabled),
            )
            .hidden_if(profile.is_none()),
        ));
    }
    rows.push(Row::one(ControlSpec::report(b::REPORT, 3, p.report.clone()).hidden_if(p.report.is_empty())));
    let (primary_text, primary_on) = if p.import {
        (ft(FilesText::ButtonApplyBundle, locale), p.can_apply)
    } else {
        (ft(FilesText::ButtonExport, locale), p.can_export)
    };
    rows.push(Row::of(vec![
        ControlSpec::button(b::PRIMARY, primary_text, primary_on),
        ControlSpec::button(b::CLOSE, ft(FilesText::ButtonClose, locale), true),
    ]));
    PanelSpec { title: p.title, rows }
}

fn intent(i: Option<UiIntent>) -> PanelReaction {
    i.map_or(PanelReaction::Nothing, PanelReaction::Intent)
}

fn invoke(input: &FilesInput<'_>, command: CommandKey) -> PanelReaction {
    if command_on(input.vm, input.document, command) {
        PanelReaction::Intent(UiIntent::Invoke { document: Some(input.document), command })
    } else {
        PanelReaction::Nothing
    }
}

fn open_dialog(purpose: FilePurpose, title: &str, filters: Vec<FileFilter>) -> PanelReaction {
    PanelReaction::Dialog(DialogRequest::Open { purpose, title: title.to_owned(), filters })
}

fn save_dialog(purpose: FilePurpose, title: &str, suggested_name: String, filters: Vec<FileFilter>) -> PanelReaction {
    PanelReaction::Dialog(DialogRequest::Save { purpose, title: title.to_owned(), suggested_name, filters })
}

fn ext_filter(name: &str, extension: &'static str) -> FileFilter {
    FileFilter { name: name.to_owned(), extensions: vec![extension] }
}

pub fn on_files_event(sheet: Sheet, input: &FilesInput<'_>, event: &PanelEvent) -> PanelReaction {
    if input.vm.document(input.document).is_none() {
        return PanelReaction::Nothing;
    }
    match sheet {
        Sheet::Start => on_start(input, event),
        Sheet::Import => on_import(input, event),
        Sheet::Export => on_export(input, event),
        Sheet::Material => on_material(input, event),
        Sheet::Art => on_art(input, event),
        Sheet::Bundle => on_bundle(input, event),
    }
}

fn on_start(input: &FilesInput<'_>, event: &PanelEvent) -> PanelReaction {
    use id::start as s;
    let (doc, locale) = (input.document, input.locale);
    if event.code != notify::CLICKED {
        return PanelReaction::Nothing;
    }
    match event.id {
        s::OPEN => open_dialog(
            FilePurpose::OpenProject,
            host_text(HostText::Open, locale),
            vec![ext_filter("BurnBurn", "burnburn")],
        ),
        s::OPEN_RECENT => {
            let row = event.values.selection.get(&s::RECENT).copied().flatten();
            intent(row.and_then(|r| input.recent_projects.get(r)).map(|path| UiIntent::Open { path: path.clone() }))
        }
        s::CLEAR_RECENT => PanelReaction::Intent(UiIntent::ClearRecentProjects),
        s::IMPORT => open_dialog(
            FilePurpose::ImportDrawing { document: doc },
            host_text(HostText::Import, locale),
            import_filters(false),
        ),
        s::IMPORT_IMAGE => open_dialog(
            FilePurpose::ImportDrawing { document: doc },
            host_text(HostText::ImageMenuImport, locale),
            import_filters(true),
        ),
        s::EXPORT_SVG => invoke(input, CommandKey::ExportSvg),
        s::EXPORT_DXF => invoke(input, CommandKey::ExportDxf),
        s::EXPORT_RD => invoke(input, CommandKey::ExportRd),
        s::SAVE_TEMPLATE => invoke(input, CommandKey::SaveAsTemplate),
        s::NEW_TEMPLATE => invoke(input, CommandKey::NewFromTemplate),
        s::MATERIAL => invoke(input, CommandKey::ShowMaterialLibrary),
        s::ART => invoke(input, CommandKey::ShowArtLibrary),
        s::BUNDLE_EXPORT => invoke(input, CommandKey::ExportBundle),
        s::BUNDLE_IMPORT => invoke(input, CommandKey::ImportBundle),
        s::OPEN_RECENT_TEMPLATE => {
            let row = event.values.selection.get(&s::RECENT_TEMPLATES).copied().flatten();
            intent(row.and_then(|r| recent_template_intent(r, input.recent_templates)))
        }
        s::CLEAR_TEMPLATES => PanelReaction::Intent(UiIntent::Files(FilesIntent::ClearRecentTemplates)),
        _ => PanelReaction::Nothing,
    }
}

fn on_import(input: &FilesInput<'_>, event: &PanelEvent) -> PanelReaction {
    use id::import as i;
    let doc = input.document;
    if event.code == notify::CLOSED {
        return invoke(input, CommandKey::CancelImport);
    }
    match (event.id, event.code) {
        (i::COMMIT, notify::CLICKED) => {
            let ready = import_panel(input.vm, doc, input.locale).is_some_and(|p| p.commit_enabled);
            if ready { invoke(input, CommandKey::CommitImport) } else { PanelReaction::Nothing }
        }
        (i::CANCEL, notify::CLICKED) => invoke(input, CommandKey::CancelImport),
        (i::UNIT, notify::CHANGED) => intent(
            import_unit(event.values.selected(i::UNIT)).map(|unit| UiIntent::SetImportUnit { document: doc, unit }),
        ),
        (i::PAGES, notify::CHANGED) => intent(import_pages_intent(doc, input.vm, event.values.selected(i::PAGES))),
        (i::LAYERS, notify::CLICKED) => {
            intent(crate::panels_files::import_layers_intent(doc, input.vm, event.values.checked(i::LAYERS)))
        }
        _ => PanelReaction::Nothing,
    }
}

fn on_export(input: &FilesInput<'_>, event: &PanelEvent) -> PanelReaction {
    use id::export as e;
    let (doc, locale) = (input.document, input.locale);
    if event.code == notify::CLOSED || (event.id == e::CLOSE && event.code == notify::CLICKED) {
        return PanelReaction::Intent(export_close_intent(doc));
    }
    let Some(dialog) = input.vm.document(doc).and_then(|d| d.files.export.as_ref()) else {
        return PanelReaction::Nothing;
    };
    let v = &event.values;
    let form_event = match event.id {
        e::UNIT | e::LAYERS | e::CURVES => event.code == notify::CHANGED,
        e::SELECTION_ONLY | e::USE_COLORS => event.code == notify::CLICKED,
        e::STROKE | e::DECIMALS | e::TOLERANCE => matches!(event.code, notify::ENTER | notify::EDIT_DONE),
        _ => false,
    };
    if form_event {
        let form = ExportForm {
            selection_only: v.checked(e::SELECTION_ONLY),
            unit: v.selected(e::UNIT),
            layers: v.selected(e::LAYERS),
            use_colors: v.checked(e::USE_COLORS),
            stroke: v.text(e::STROKE).to_owned(),
            decimals: v.text(e::DECIMALS).to_owned(),
            curves: v.selected(e::CURVES),
            tolerance: v.text(e::TOLERANCE).to_owned(),
        };
        return intent(export_options_intent(doc, input.vm, &form));
    }
    match (event.id, event.code) {
        (e::ACKNOWLEDGE, notify::CLICKED) => {
            PanelReaction::Intent(export_acknowledge_intent(doc, v.checked(e::ACKNOWLEDGE)))
        }
        (e::EXPORT, notify::CLICKED) => {
            let ready = export_panel(input.vm, doc, locale).is_some_and(|p| p.can_export);
            let title = input.vm.document(doc).map(|d| d.title.clone()).unwrap_or_default();
            if !ready {
                return PanelReaction::Nothing;
            }
            let (name, ext) = match dialog.format {
                ExportFormat::Svg => ("SVG", "svg"),
                _ => ("DXF", "dxf"),
            };
            save_dialog(
                FilePurpose::ExportDrawing { document: doc },
                ft(FilesText::PanelExportSave, locale),
                export_file_name(&title, dialog.format),
                vec![ext_filter(name, ext)],
            )
        }
        _ => PanelReaction::Nothing,
    }
}

fn on_material(input: &FilesInput<'_>, event: &PanelEvent) -> PanelReaction {
    use id::material as m;
    let doc = input.document;
    let (vm, v) = (input.vm, &event.values);
    let layer = v.selected(m::LAYER);
    if event.code == notify::CLOSED || (event.id == m::CLOSE && event.code == notify::CLICKED) {
        return PanelReaction::Intent(UiIntent::Invoke {
            document: Some(doc),
            command: CommandKey::ShowMaterialLibrary,
        });
    }
    let search = matches!(event.id, m::MATERIAL | m::MACHINE) && event.code == notify::CHANGED
        || event.id == m::THICKNESS && matches!(event.code, notify::ENTER | notify::EDIT_DONE);
    if search {
        return intent(material_filter_intent(
            doc,
            vm,
            v.selected(m::MATERIAL),
            v.text(m::THICKNESS),
            v.selected(m::MACHINE),
        ));
    }
    if event.id == m::ENTRIES && event.code == notify::CHANGED {
        return intent(material_select_intent(doc, vm, v.selected(m::ENTRIES)));
    }
    if event.code != notify::CLICKED {
        return PanelReaction::Nothing;
    }
    match event.id {
        m::APPLY => intent(material_apply_intent(doc, vm, layer)),
        m::OVERWRITE => intent(material_overwrite_intent(doc, vm, layer)),
        m::UPDATE => intent(material_update_intent(doc, vm, layer, false)),
        m::UPDATE_APPLY => intent(material_update_intent(doc, vm, layer, true)),
        m::SAVE => intent(material_save_intent(
            doc,
            vm,
            layer,
            v.text(m::NAME),
            v.text(m::NEW_MATERIAL),
            v.text(m::STORE_THICKNESS),
        )),
        m::REMOVE => intent(material_remove_intent(doc, vm)),
        _ => PanelReaction::Nothing,
    }
}

fn on_art(input: &FilesInput<'_>, event: &PanelEvent) -> PanelReaction {
    use id::art as a;
    let (doc, vm, locale, v) = (input.document, input.vm, input.locale, &event.values);
    if event.code == notify::CLOSED || (event.id == a::CLOSE && event.code == notify::CLICKED) {
        return PanelReaction::Intent(UiIntent::Invoke { document: Some(doc), command: CommandKey::ShowArtLibrary });
    }
    match (event.id, event.code) {
        (a::GROUP, notify::CHANGED) => intent(art_group_intent(doc, vm, v.selected(a::GROUP))),
        (a::SEARCH, notify::ENTER | notify::EDIT_DONE) => intent(art_search_intent(doc, vm, v.text(a::SEARCH))),
        (a::ITEMS, notify::CHANGED) => intent(art_item_intent(doc, vm, v.selected(a::ITEMS))),
        (a::PLACE, notify::CLICKED) => intent(art_place_intent(doc, vm)),
        (a::REMOVE_ITEM, notify::CLICKED) => intent(art_remove_item_intent(doc, vm)),
        (a::RENAME_ITEM, notify::CLICKED) => intent(art_rename_item_intent(doc, vm, v.text(a::NAME))),
        (a::MOVE_ITEM, notify::CLICKED) => intent(art_move_item_intent(doc, vm, v.selected(a::GROUP))),
        (a::NEW_GROUP, notify::CLICKED) => PanelReaction::Intent(art_add_group_intent(doc, v.text(a::NAME))),
        (a::RENAME_GROUP, notify::CLICKED) => intent(art_rename_group_intent(doc, vm, v.text(a::NAME))),
        (a::REMOVE_GROUP, notify::CLICKED) => intent(art_remove_group_intent(doc, vm)),
        (a::ADD_SELECTION, notify::CLICKED) => {
            PanelReaction::Intents(art_add_selection_intents(doc, vm, v.text(a::NAME), locale))
        }
        (a::ADD_FILE, notify::CLICKED) => open_dialog(
            FilePurpose::ArtAddFile { document: doc, name: v.text(a::NAME).to_owned(), unit_item: v.selected(a::UNIT) },
            ft(FilesText::PanelArtOpen, locale),
            vec![FileFilter { name: "Tekeningen / Drawings".to_owned(), extensions: ART_FILE_TYPES.to_vec() }],
        ),
        (a::RETRY, notify::CLICKED) => {
            let intents = art_retry_intents(doc, vm, v.text(a::NAME), v.selected(a::UNIT), locale);
            if intents.is_empty() { PanelReaction::Nothing } else { PanelReaction::Intents(intents) }
        }
        _ => PanelReaction::Nothing,
    }
}

fn on_bundle(input: &FilesInput<'_>, event: &PanelEvent) -> PanelReaction {
    use id::bundle as b;
    let (doc, vm, locale, v) = (input.document, input.vm, input.locale, &event.values);
    if event.code == notify::CLOSED || (event.id == b::CLOSE && event.code == notify::CLICKED) {
        return PanelReaction::Intent(bundle_close_intent(doc));
    }
    let Some(panel) = bundle_panel(vm, doc, locale) else { return PanelReaction::Nothing };
    match (event.id, event.code) {
        (cid, notify::CLICKED) if (b::KIND..b::KIND + 4).contains(&cid) => {
            intent(bundle_kind_toggle_intent(doc, vm, isize::from((cid - b::KIND) as i16)))
        }
        (b::CONFLICT, notify::CHANGED) => intent(bundle_conflict_intent(doc, v.selected(b::CONFLICT))),
        (cid, notify::CLICKED) if (b::PROFILE..b::PROFILE + b::PROFILES).contains(&cid) => {
            let slot = usize::from(cid - b::PROFILE);
            intent(bundle_profile_intent(doc, vm, slot, v.checked(cid)))
        }
        (b::PRIMARY, notify::CLICKED) if panel.import => {
            if panel.can_apply {
                PanelReaction::Intent(bundle_apply_intent(doc))
            } else {
                PanelReaction::Nothing
            }
        }
        (b::PRIMARY, notify::CLICKED) => {
            if panel.can_export {
                save_dialog(
                    FilePurpose::BundleWrite { document: doc },
                    ft(FilesText::PanelBundleSave, locale),
                    "BurnBurn.burnburnbundle".to_owned(),
                    vec![ext_filter("BurnBurn bundel / bundle", "burnburnbundle")],
                )
            } else {
                PanelReaction::Nothing
            }
        }
        _ => PanelReaction::Nothing,
    }
}

/// A file dialog of this family closed (cancel, or a window that is gone, sends nothing).
pub fn on_files_file_chosen(vm: &AppViewModel, purpose: &FilePurpose, path: Chosen) -> PanelReaction {
    let Some(path) = path else { return PanelReaction::Nothing };
    let open = |d: &DocumentId| vm.document(*d).is_some();
    match purpose {
        FilePurpose::OpenProject => PanelReaction::Intent(UiIntent::Open { path }),
        FilePurpose::ExportRd { document } if open(document) => {
            PanelReaction::Intent(UiIntent::ExportRd { document: *document, path })
        }
        FilePurpose::ExportGcode { document } if open(document) => {
            PanelReaction::Intent(UiIntent::ExportGcode { document: *document, path })
        }
        FilePurpose::ExportDrawing { document } if open(document) => {
            PanelReaction::Intent(export_write_intent(*document, path))
        }
        FilePurpose::TemplateSave { document, scope } if open(document) => {
            PanelReaction::Intent(UiIntent::Files(FilesIntent::SaveAsTemplate {
                document: *document,
                path,
                scope: *scope,
            }))
        }
        FilePurpose::TemplateOpen => PanelReaction::Intent(template_open_intent(path)),
        FilePurpose::MachineProfileOpen => PanelReaction::Intent(machine_profile_open_intent(path)),
        FilePurpose::BundleOpen { document } if open(document) => {
            PanelReaction::Intent(bundle_open_intent(*document, path))
        }
        FilePurpose::BundleWrite { document } if open(document) => {
            PanelReaction::Intent(bundle_write_intent(*document, path))
        }
        FilePurpose::ArtAddFile { document, name, unit_item } if open(document) => {
            // The language only decides the name of a group created on the way.
            PanelReaction::Intents(art_add_file_intents(*document, vm, &path, name, *unit_item, Locale::En))
        }
        _ => PanelReaction::Nothing,
    }
}

/// The dialog a host effect of this family asks for (the RD export, templates, bundles).
pub fn effect_dialog(effect: &HostEffect, input: &FilesInput<'_>) -> Option<DialogRequest> {
    let locale = input.locale;
    let scope = |item: isize| usize::try_from(item).ok().and_then(|i| TEMPLATE_SCOPES.get(i)).copied();
    match effect {
        HostEffect::ShowExportPanel { document, suggested_name } => Some(DialogRequest::Save {
            purpose: FilePurpose::ExportRd { document: *document },
            title: cmd_name(CommandKey::ExportRd, locale),
            suggested_name: suggested_name.clone(),
            filters: vec![ext_filter("Ruida RD", "rd")],
        }),
        HostEffect::ShowGcodeExportPanel { document, suggested_name } => Some(DialogRequest::Save {
            purpose: FilePurpose::ExportGcode { document: *document },
            title: cmd_name(CommandKey::ExportGcode, locale),
            suggested_name: suggested_name.clone(),
            filters: vec![ext_filter("G-code", "gcode")],
        }),
        HostEffect::ShowTemplateSavePanel { document, suggested_name } => Some(DialogRequest::Save {
            purpose: FilePurpose::TemplateSave {
                document: *document,
                scope: scope(input.template_scope).unwrap_or(TemplateScope::Everything),
            },
            title: ft(FilesText::PanelTemplateSave, locale).to_owned(),
            suggested_name: suggested_name.clone(),
            filters: vec![ext_filter("BurnBurn template", "burnburntemplate")],
        }),
        HostEffect::ShowTemplateOpenPanel => Some(DialogRequest::Open {
            purpose: FilePurpose::TemplateOpen,
            title: ft(FilesText::PanelTemplateOpen, locale).to_owned(),
            filters: vec![ext_filter("BurnBurn template", "burnburntemplate")],
        }),
        HostEffect::ShowMachineProfileOpenPanel => Some(DialogRequest::Open {
            purpose: FilePurpose::MachineProfileOpen,
            title: ft(FilesText::PanelMachineProfileOpen, locale).to_owned(),
            filters: vec![ext_filter("Machineprofiel / machine profile (JSON)", "json")],
        }),
        HostEffect::ShowBundleOpenPanel { document } => Some(DialogRequest::Open {
            purpose: FilePurpose::BundleOpen { document: *document },
            title: ft(FilesText::PanelBundleOpen, locale).to_owned(),
            filters: vec![ext_filter("BurnBurn bundel / bundle", "burnburnbundle")],
        }),
        _ => None,
    }
}
