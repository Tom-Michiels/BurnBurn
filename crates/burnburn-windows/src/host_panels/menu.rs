//! The menu entries of this lane (W4-L8b), as data: which `WM_COMMAND` id opens which panel or
//! dialog, their titles and enablement, and the reaction. The Win32 layer appends them to the
//! menus (Laser, Beeld, Archief, Venster, Help) and routes the ids here; ids are in `6000..6500`.

use std::path::PathBuf;

use burnburn_presentation::{AppViewModel, CommandKey, DiagnosticOptions, FilesIntent, HelpInfo, MessageKey, UiIntent};
use burnburn_types::DocumentId;

use super::filters::{import_filters, trace_filter};
use super::ids::menu_base;
use super::{DialogRequest, Family, FileFilter, FilePurpose, HostRequest, PanelReaction, command_on};
use crate::app_menu::{AppAction, action_reaction};
use crate::l10n::{HostText, Locale, host_text, message_or_key};
use crate::panels::{CameraText, LaserText, camera_text, laser_text};

/// How many recent projects / templates get a menu id.
pub const RECENT_SLOTS: usize = 20;

/// One entry of this lane's menus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Item {
    LaserWindow,
    ExportDiagnostics,
    ImportImage,
    TraceImage,
    ImagePanel,
    FilesPanel,
    Import,
    Material,
    Art,
    ExportSvg,
    ExportDxf,
    SaveTemplate,
    NewTemplate,
    BundleExport,
    BundleImport,
    ClearRecentTemplates,
    ClearRecentProjects,
    RecentProject(usize),
    RecentTemplate(usize),
    CameraWindow,
    App(AppAction),
}

/// What the entries are decided from.
pub struct MenuContext<'a> {
    pub vm: &'a AppViewModel,
    pub locale: Locale,
    pub manual_known: bool,
    pub recent_projects: &'a [PathBuf],
    pub recent_templates: &'a [PathBuf],
}

/// Every fixed entry (the recent lists are added by the host from the stored lists).
pub fn items() -> Vec<Item> {
    let mut v = vec![
        Item::LaserWindow,
        Item::ExportDiagnostics,
        Item::ImportImage,
        Item::TraceImage,
        Item::ImagePanel,
        Item::FilesPanel,
        Item::Import,
        Item::Material,
        Item::Art,
        Item::ExportSvg,
        Item::ExportDxf,
        Item::SaveTemplate,
        Item::NewTemplate,
        Item::BundleExport,
        Item::BundleImport,
        Item::ClearRecentTemplates,
        Item::ClearRecentProjects,
        Item::CameraWindow,
    ];
    v.extend(AppAction::ALL.iter().map(|a| Item::App(*a)));
    v
}

/// Offsets inside the Files block of `menu_base`.
const RECENT_PROJECT_AT: u16 = 20;
const RECENT_TEMPLATE_AT: u16 = 40;

/// The fixed entries of the Files block, in id order.
const FILES_FIXED: [Item; 12] = [
    Item::FilesPanel,
    Item::Import,
    Item::Material,
    Item::Art,
    Item::ExportSvg,
    Item::ExportDxf,
    Item::SaveTemplate,
    Item::NewTemplate,
    Item::BundleExport,
    Item::BundleImport,
    Item::ClearRecentTemplates,
    Item::ClearRecentProjects,
];

const SLOTS: u16 = RECENT_SLOTS as u16;

impl Item {
    pub fn id(self) -> u16 {
        let files = menu_base(Family::Files);
        match self {
            Item::LaserWindow => menu_base(Family::Laser),
            Item::ExportDiagnostics => menu_base(Family::Laser) + 1,
            Item::ImportImage => menu_base(Family::Image),
            Item::TraceImage => menu_base(Family::Image) + 1,
            Item::ImagePanel => menu_base(Family::Image) + 2,
            Item::CameraWindow => menu_base(Family::Camera),
            Item::App(action) => action.id(),
            Item::RecentProject(n) => files + RECENT_PROJECT_AT + (n as u16).min(SLOTS - 1),
            Item::RecentTemplate(n) => files + RECENT_TEMPLATE_AT + (n as u16).min(SLOTS - 1),
            fixed => files + FILES_FIXED.iter().position(|i| *i == fixed).map_or(0, |p| p as u16),
        }
    }

    pub fn from_id(id: u16) -> Option<Item> {
        let files = menu_base(Family::Files);
        if let Some(at) = id.checked_sub(files).filter(|a| *a < super::ids::MENU_BLOCK) {
            return match at {
                a if usize::from(a) < FILES_FIXED.len() => FILES_FIXED.get(usize::from(a)).copied(),
                a if (RECENT_PROJECT_AT..RECENT_PROJECT_AT + SLOTS).contains(&a) => {
                    Some(Item::RecentProject(usize::from(a - RECENT_PROJECT_AT)))
                }
                a if (RECENT_TEMPLATE_AT..RECENT_TEMPLATE_AT + SLOTS).contains(&a) => {
                    Some(Item::RecentTemplate(usize::from(a - RECENT_TEMPLATE_AT)))
                }
                _ => None,
            };
        }
        [
            Item::LaserWindow,
            Item::ExportDiagnostics,
            Item::ImportImage,
            Item::TraceImage,
            Item::ImagePanel,
            Item::CameraWindow,
        ]
        .into_iter()
        .find(|i| i.id() == id)
        .or_else(|| AppAction::from_id(id).map(Item::App))
    }

    /// The command a command entry invokes.
    pub fn command(self) -> Option<CommandKey> {
        Some(match self {
            Item::Material => CommandKey::ShowMaterialLibrary,
            Item::Art => CommandKey::ShowArtLibrary,
            Item::ExportSvg => CommandKey::ExportSvg,
            Item::ExportDxf => CommandKey::ExportDxf,
            Item::SaveTemplate => CommandKey::SaveAsTemplate,
            Item::NewTemplate => CommandKey::NewFromTemplate,
            Item::BundleExport => CommandKey::ExportBundle,
            Item::BundleImport => CommandKey::ImportBundle,
            _ => return None,
        })
    }

    pub fn title(self, ctx: &MenuContext<'_>) -> String {
        let locale = ctx.locale;
        let file = |paths: &[PathBuf], n: usize| {
            paths.get(n).and_then(|p| p.file_name()).map(|f| f.to_string_lossy().into_owned()).unwrap_or_default()
        };
        if let Some(command) = self.command() {
            return message_or_key(MessageKey::Command(command), locale);
        }
        match self {
            Item::LaserWindow => laser_text(LaserText::MenuWindowItem, locale).to_owned(),
            Item::ExportDiagnostics => laser_text(LaserText::DiagnosticsItem, locale).to_owned(),
            Item::ImportImage => host_text(HostText::ImageMenuImport, locale).to_owned(),
            Item::TraceImage => host_text(HostText::ImageMenuTrace, locale).to_owned(),
            Item::ImagePanel => host_text(HostText::ImageMenu, locale).to_owned(),
            Item::FilesPanel => host_text(HostText::MenuFile, locale).to_owned(),
            Item::Import => host_text(HostText::Import, locale).to_owned(),
            Item::ClearRecentTemplates | Item::ClearRecentProjects => {
                host_text(HostText::ClearRecent, locale).to_owned()
            }
            Item::CameraWindow => camera_text(CameraText::WindowTitle, locale).to_owned(),
            Item::RecentProject(n) => file(ctx.recent_projects, n),
            Item::RecentTemplate(n) => file(ctx.recent_templates, n),
            Item::App(action) => action.title(locale).to_owned(),
            _ => String::new(),
        }
    }

    fn active(ctx: &MenuContext<'_>) -> Option<DocumentId> {
        ctx.vm.active.filter(|d| ctx.vm.document(*d).is_some())
    }

    pub fn enabled(self, ctx: &MenuContext<'_>) -> bool {
        let window = Self::active(ctx);
        if let Some(command) = self.command() {
            return window.is_some_and(|doc| command_on(ctx.vm, doc, command));
        }
        match self {
            Item::ExportDiagnostics => true,
            Item::App(action) => action.enabled(ctx.vm, ctx.manual_known),
            Item::RecentProject(n) => n < ctx.recent_projects.len(),
            Item::RecentTemplate(n) => n < ctx.recent_templates.len(),
            Item::ClearRecentTemplates => !ctx.recent_templates.is_empty(),
            Item::ClearRecentProjects => !ctx.recent_projects.is_empty(),
            Item::LaserWindow
            | Item::ImportImage
            | Item::TraceImage
            | Item::ImagePanel
            | Item::FilesPanel
            | Item::Import
            | Item::CameraWindow => window.is_some(),
            _ => false,
        }
    }
}

fn open(purpose: FilePurpose, title: String, filters: Vec<FileFilter>) -> PanelReaction {
    PanelReaction::Dialog(DialogRequest::Open { purpose, title, filters })
}

fn diagnostics(title: String) -> PanelReaction {
    PanelReaction::Dialog(DialogRequest::Save {
        purpose: FilePurpose::Diagnostics { options: DiagnosticOptions::default() },
        title,
        suggested_name: "burnburn-diagnose.txt".to_owned(),
        filters: vec![FileFilter { name: "Tekst / Text".to_owned(), extensions: vec!["txt"] }],
    })
}

fn recent(paths: &[PathBuf], n: usize, make: impl Fn(PathBuf) -> UiIntent) -> PanelReaction {
    paths.get(n).map_or(PanelReaction::Nothing, |path| PanelReaction::Intent(make(path.clone())))
}

/// What choosing the entry does.
pub fn reaction(item: Item, ctx: &MenuContext<'_>) -> PanelReaction {
    if !item.enabled(ctx) {
        return PanelReaction::Nothing;
    }
    let title = item.title(ctx);
    let show = |family| PanelReaction::Host(HostRequest::ShowSheet(family, 0));
    let document = Item::active(ctx);
    if let (Some(command), Some(document)) = (item.command(), document) {
        return PanelReaction::Intent(UiIntent::Invoke { document: Some(document), command });
    }
    match (item, document) {
        (Item::ExportDiagnostics, _) => diagnostics(title),
        (Item::App(action), _) => action_reaction(action, ctx.vm, &help_info(), ctx.locale),
        (Item::RecentProject(n), _) => recent(ctx.recent_projects, n, |path| UiIntent::Open { path }),
        (Item::RecentTemplate(n), _) => {
            recent(ctx.recent_templates, n, |path| UiIntent::Files(FilesIntent::NewFromTemplate { path }))
        }
        (Item::ClearRecentTemplates, _) => PanelReaction::Intent(UiIntent::Files(FilesIntent::ClearRecentTemplates)),
        (Item::ClearRecentProjects, _) => PanelReaction::Intent(UiIntent::ClearRecentProjects),
        (Item::LaserWindow, Some(_)) => show(Family::Laser),
        (Item::CameraWindow, Some(_)) => show(Family::Camera),
        (Item::ImagePanel, Some(_)) => show(Family::Image),
        (Item::FilesPanel, Some(_)) => show(Family::Files),
        (Item::ImportImage, Some(document)) => {
            open(FilePurpose::ImportDrawing { document }, title, import_filters(true))
        }
        (Item::Import, Some(document)) => open(FilePurpose::ImportDrawing { document }, title, import_filters(false)),
        (Item::TraceImage, Some(document)) => open(FilePurpose::TraceImage { document }, title, vec![trace_filter()]),
        _ => PanelReaction::Nothing,
    }
}

fn help_info() -> HelpInfo {
    burnburn_presentation::AppController::help_info()
}
