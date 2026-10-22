//! Glue between the controller, the headless panel families and the Win32 panel windows (W4-L8b).
//!
//! The host (`win32/mod.rs`) implements [`Bridge`] (it owns the controller and the dispatching),
//! calls [`install`] once, forwards the menu ids of this lane to [`handle_menu`], the host effects
//! to [`effect`] and calls [`refresh`] after every update. Everything that decides something lives
//! in `host_panels` and is tested headlessly; this file only moves data and performs requests.
//!
//! **Unconfirmed**: type-checked from macOS, never run on Windows.
//!
//! Re-entrancy: nothing here holds a borrow of the host state while it calls the bridge (which
//! dispatches intents and may call [`refresh`] again).

#![allow(unsafe_code)] // ShellExecuteW and menu calls (SAFETY below)

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use burnburn_image::SourceImage;
use burnburn_presentation::{
    AppController, AppIntent, AppViewModel, CameraViewModel, CameraWorkspaceView, DebugLogViewModel, DiagnosticOptions,
    HistoryViewModel, HostEffect, KeyBinding, LaserViewModel, MachineViewModel, PrefsViewModel, PreviewViewModel,
    RdInspectViewModel, ShortcutsViewModel, UiIntent,
};
use burnburn_types::{DocumentId, StableId};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    DeleteMenu, EnableMenuItem, HMENU, InsertMenuW, MF_BYCOMMAND, MF_BYPOSITION, MF_ENABLED, MF_GRAYED, MF_STRING,
    SW_SHOWNORMAL,
};
use windows::core::{HSTRING, w};

use super::{PanelKey, dialogs};
use crate::host_panels::app::{self, AppInput, AppSheet};
use crate::host_panels::camera::{self, CameraInput};
use crate::host_panels::files::{self, FilesInput, Sheet};
use crate::host_panels::image::{self, ImageInput};
use crate::host_panels::laser::{self, LaserInput};
use crate::host_panels::menu::{self, Item, MenuContext, RECENT_SLOTS};
use crate::host_panels::{
    ConfirmPurpose, DialogRequest, Family, HostRequest, PanelEvent, PanelReaction, PanelSpec, PanelValues, notify,
    on_confirmed, on_file_chosen,
};
use crate::l10n::Locale;
use crate::panels::{next_unplaced_mark, photo_with_marks};
use crate::panels_image::BitmapImage;

/// Everything the panels are built from, read from the controller at one moment.
pub struct Snapshot {
    pub vm: Arc<AppViewModel>,
    pub laser: Option<LaserViewModel>,
    pub machine: Option<MachineViewModel>,
    pub camera: Option<CameraViewModel>,
    pub workspace: Option<CameraWorkspaceView>,
    /// The alignment photo as RGBA, with its size.
    pub photo: Option<(u32, u32, Vec<u8>)>,
    pub prefs: PrefsViewModel,
    pub shortcuts: ShortcutsViewModel,
    pub history: Option<HistoryViewModel>,
    pub preview: Option<PreviewViewModel>,
    pub rd: Option<RdInspectViewModel>,
    pub debug: DebugLogViewModel,
    pub recent_projects: Vec<PathBuf>,
    pub recent_templates: Vec<PathBuf>,
}

/// What the host provides: the controller's views and the dispatching of intents.
pub trait Bridge {
    /// The views for the panels of `document` (the app-wide ones ignore it).
    fn snapshot(&self, document: Option<DocumentId>) -> Option<Snapshot>;
    /// Sends the intent and performs its effects (this may call [`refresh`] and [`effect`]).
    fn dispatch(&self, intent: UiIntent);
    /// The text a support bundle would hold for these options.
    fn support_report(&self, options: &DiagnosticOptions) -> String;
    fn locale(&self) -> Locale;
}

/// RGBA bytes of a camera frame, whatever its stored format.
pub fn rgba_of(image: &SourceImage) -> Vec<u8> {
    if image.format() == burnburn_image::PixelFormat::Rgba8 {
        return image.samples().to_vec();
    }
    let mut out = Vec::with_capacity(image.width() as usize * image.height() as usize * 4);
    for y in 0..image.height() {
        for x in 0..image.width() {
            out.extend(image.rgba_at(x, y).map(|v| v.round().clamp(0.0, 255.0) as u8));
        }
    }
    out
}

/// What the panels remember between events (window state, never document data).
#[derive(Default)]
struct HostState {
    laser_file_row: BTreeMap<u64, usize>,
    recent_row: Option<usize>,
    template_row: Option<usize>,
    template_scope: isize,
    shortcut_filter: String,
    shortcut_row: Option<usize>,
    recording: Option<String>,
    pending_replace: Option<(String, KeyBinding)>,
    support_options: DiagnosticOptions,
    camera_mark: Option<usize>,
    recent_menus: Vec<RecentMenus>,
}

thread_local! {
    static BRIDGE: RefCell<Option<Rc<dyn Bridge>>> = const { RefCell::new(None) };
    static HOST: RefCell<HostState> = RefCell::new(HostState::default());
}

fn bridge() -> Option<Rc<dyn Bridge>> {
    BRIDGE.with(|b| b.try_borrow().ok().and_then(|b| b.clone()))
}

fn host<T>(f: impl FnOnce(&mut HostState) -> T) -> Option<T> {
    HOST.with(|h| h.try_borrow_mut().ok().map(|mut h| f(&mut h)))
}

/// Connects the panels to the host and starts listening to their windows.
pub fn install(bridge: Rc<dyn Bridge>) {
    BRIDGE.with(|b| *b.borrow_mut() = Some(bridge));
    super::set_handler(Rc::new(on_event));
}

fn key_for(family: Family, sheet: u8, document: Option<DocumentId>) -> PanelKey {
    // Only the laser, image and file panels are about one window; camera and app are app-wide.
    let document = match family {
        Family::Laser | Family::Image | Family::Files => document.map_or(0, |d| d.raw()),
        Family::Camera | Family::App => 0,
    };
    PanelKey { family, sheet, document }
}

fn document_of(key: PanelKey) -> Option<DocumentId> {
    (key.document != 0).then(|| DocumentId::from_raw(key.document))
}

// ---------------------------------------------------------------------------------------------
// Specs.
// ---------------------------------------------------------------------------------------------

fn photo_bitmap(snap: &Snapshot) -> Option<BitmapImage> {
    let (w, h, rgba) = snap.photo.as_ref()?;
    let alignment = snap.workspace.as_ref()?.alignment.as_ref()?;
    let clicked: Vec<([f64; 2], burnburn_image::calib::MarkRole)> =
        alignment.marks.iter().filter_map(|m| Some((m.pixel?, m.role))).collect();
    Some(BitmapImage { width: *w, height: *h, rgba: photo_with_marks(rgba, *w, *h, &clicked) })
}

fn spec_for(key: PanelKey, snap: &Snapshot, locale: Locale, bridge: &dyn Bridge) -> Option<PanelSpec> {
    let state = host(|h| {
        (
            h.laser_file_row.get(&key.document).copied(),
            h.recent_row,
            h.template_row,
            h.template_scope,
            h.shortcut_filter.clone(),
            h.shortcut_row,
            h.recording.clone(),
            h.pending_replace.clone(),
            h.support_options,
            h.camera_mark,
        )
    })?;
    let document = document_of(key);
    Some(match key.family {
        Family::Laser => laser::laser_spec(&LaserInput {
            document: document?,
            vm: &snap.vm,
            laser: snap.laser.as_ref(),
            machine: snap.machine.as_ref(),
            locale,
            file_row: state.0,
        }),
        Family::Image => image::image_spec(&ImageInput { document: document?, vm: &snap.vm, locale }),
        Family::Files => files::files_spec(
            Sheet::from_code(key.sheet)?,
            &FilesInput {
                document: document?,
                vm: &snap.vm,
                locale,
                recent_projects: &snap.recent_projects,
                recent_templates: &snap.recent_templates,
                template_scope: state.3,
                recent_row: state.1,
                template_row: state.2,
            },
        ),
        Family::Camera => camera::camera_spec(&CameraInput {
            locale,
            workspace: snap.workspace.as_ref(),
            primary: snap.camera.as_ref(),
            photo: photo_bitmap(snap),
            mark: state.9,
        }),
        Family::App => {
            let support = bridge.support_report(&state.8);
            let pending = state.7.as_ref().map(|(id, b)| (id.as_str(), b));
            app::app_spec(
                AppSheet::from_code(key.sheet)?,
                &AppInput {
                    locale,
                    vm: &snap.vm,
                    document: snap.vm.active,
                    prefs: &snap.prefs,
                    shortcuts: &snap.shortcuts,
                    history: snap.history.as_ref(),
                    preview: snap.preview.as_ref(),
                    rd: snap.rd.as_ref(),
                    help: AppController::help_info(),
                    debug: &snap.debug,
                    support_text: &support,
                    shortcut_filter: &state.4,
                    shortcut_row: state.5,
                    recording: state.6.as_deref(),
                    pending_replace: pending,
                },
            )
        }
    })
}

/// Whether the panel should exist at all (a sheet closes when presentation closed it).
fn should_exist(key: PanelKey, snap: &Snapshot) -> bool {
    match key.family {
        Family::Files => match (Sheet::from_code(key.sheet), document_of(key)) {
            (Some(Sheet::Start), Some(d)) => snap.vm.document(d).is_some(),
            (Some(sheet), Some(d)) => files::sheet_open(sheet, &snap.vm, d),
            _ => false,
        },
        Family::Laser | Family::Image => document_of(key).is_some_and(|d| snap.vm.document(d).is_some()),
        Family::Camera | Family::App => true,
    }
}

/// Brings every open panel in line with the controller (call after every update).
pub fn refresh() {
    let Some(bridge) = bridge() else { return };
    let locale = bridge.locale();
    for key in super::open_keys() {
        let Some(snap) = bridge.snapshot(document_of(key)) else { continue };
        if !should_exist(key, &snap) {
            super::close(key);
            continue;
        }
        if let Some(spec) = spec_for(key, &snap, locale, bridge.as_ref()) {
            super::update(key, &spec);
        }
    }
    // Sheets that presentation opened (export dialog, libraries, bundle, import preview) appear.
    if let Some(snap) = bridge.snapshot(None)
        && let Some(doc) = snap.vm.active
    {
        for sheet in Sheet::ALL {
            if sheet != Sheet::Start && files::sheet_open(sheet, &snap.vm, doc) {
                show_sheet(Family::Files, sheet.code(), Some(doc));
            }
        }
    }
}

/// Opens (or raises) the panel; `document` is the window it is about.
pub fn show_sheet(family: Family, code: u8, document: Option<DocumentId>) {
    let Some(bridge) = bridge() else { return };
    let key = key_for(family, code, document);
    let Some(snap) = bridge.snapshot(document_of(key)) else { return };
    let Some(spec) = spec_for(key, &snap, bridge.locale(), bridge.as_ref()) else { return };
    // Files sheets that presentation has not opened would be empty windows.
    if !should_exist(key, &snap) {
        return;
    }
    let fresh = !super::is_open(key);
    super::show(key, &spec);
    if fresh && family == Family::Files && code != Sheet::Start.code() {
        // A sheet opened by presentation must not steal the user's typing every refresh.
    }
}

// ---------------------------------------------------------------------------------------------
// Events.
// ---------------------------------------------------------------------------------------------

fn note_state(key: PanelKey, ev: &PanelEvent) {
    let v: &PanelValues = &ev.values;
    host(|h| match key.family {
        Family::Laser => {
            if let Some(row) = v.selection.get(&laser::id::FILES) {
                match row {
                    Some(r) => {
                        h.laser_file_row.insert(key.document, *r);
                    }
                    None => {
                        h.laser_file_row.remove(&key.document);
                    }
                }
            }
        }
        Family::Files => {
            use files::id::start as s;
            if let Some(row) = v.selection.get(&s::RECENT) {
                h.recent_row = *row;
            }
            if let Some(row) = v.selection.get(&s::RECENT_TEMPLATES) {
                h.template_row = *row;
            }
            if let Some(Some(scope)) = v.selection.get(&s::TEMPLATE_SCOPE) {
                h.template_scope = isize::try_from(*scope).unwrap_or(0);
            }
        }
        Family::Camera => {
            if let Some(Some(mark)) = v.selection.get(&camera::id::MARK) {
                h.camera_mark = Some(*mark);
            }
        }
        Family::App => {
            use app::id as a;
            if let Some(t) = v.texts.get(&a::SC_SEARCH) {
                h.shortcut_filter.clone_from(t);
            }
            if let Some(row) = v.selection.get(&a::SC_LIST) {
                h.shortcut_row = *row;
            }
            h.support_options = DiagnosticOptions {
                include_project_paths: v.checked(a::SU_PATHS),
                include_machine_ids: v.checked(a::SU_IDS),
            };
        }
        Family::Image => {}
    });
}

fn reaction_for(key: PanelKey, ev: &PanelEvent, snap: &Snapshot, locale: Locale, bridge: &dyn Bridge) -> PanelReaction {
    let state = host(|h| {
        (
            h.laser_file_row.get(&key.document).copied(),
            h.recent_row,
            h.template_row,
            h.template_scope,
            h.shortcut_filter.clone(),
            h.shortcut_row,
            h.recording.clone(),
            h.pending_replace.clone(),
            h.support_options,
            h.camera_mark,
        )
    });
    let Some(state) = state else { return PanelReaction::Nothing };
    let document = document_of(key);
    match key.family {
        Family::Laser => {
            let Some(document) = document else { return PanelReaction::Nothing };
            laser::on_laser_event(
                &LaserInput {
                    document,
                    vm: &snap.vm,
                    laser: snap.laser.as_ref(),
                    machine: snap.machine.as_ref(),
                    locale,
                    file_row: state.0,
                },
                ev,
            )
        }
        Family::Image => {
            let Some(document) = document else { return PanelReaction::Nothing };
            image::on_image_event(&ImageInput { document, vm: &snap.vm, locale }, ev)
        }
        Family::Files => {
            let (Some(document), Some(sheet)) = (document, Sheet::from_code(key.sheet)) else {
                return PanelReaction::Nothing;
            };
            files::on_files_event(
                sheet,
                &FilesInput {
                    document,
                    vm: &snap.vm,
                    locale,
                    recent_projects: &snap.recent_projects,
                    recent_templates: &snap.recent_templates,
                    template_scope: state.3,
                    recent_row: state.1,
                    template_row: state.2,
                },
                ev,
            )
        }
        Family::Camera => {
            let input = CameraInput {
                locale,
                workspace: snap.workspace.as_ref(),
                primary: snap.camera.as_ref(),
                photo: None,
                mark: state.9,
            };
            let reaction = camera::on_camera_event(&input, ev);
            // After a mark was placed, the list moves on to the next mark without a position.
            if ev.id == camera::id::PHOTO
                && ev.code == notify::BITMAP_CLICK
                && matches!(reaction, PanelReaction::Intent(_))
                && let Ok(placed) = usize::try_from(ev.values.selected(camera::id::MARK))
                && let Some(alignment) = snap.workspace.as_ref().and_then(|w| w.alignment.as_ref())
            {
                let next = next_unplaced_mark(alignment, placed);
                host(|h| h.camera_mark = next.or(Some(placed)));
            }
            reaction
        }
        Family::App => {
            let Some(sheet) = AppSheet::from_code(key.sheet) else { return PanelReaction::Nothing };
            let support = bridge.support_report(&state.8);
            let pending = state.7.as_ref().map(|(id, b)| (id.as_str(), b));
            let input = AppInput {
                locale,
                vm: &snap.vm,
                document: snap.vm.active,
                prefs: &snap.prefs,
                shortcuts: &snap.shortcuts,
                history: snap.history.as_ref(),
                preview: snap.preview.as_ref(),
                rd: snap.rd.as_ref(),
                help: AppController::help_info(),
                debug: &snap.debug,
                support_text: &support,
                shortcut_filter: &state.4,
                shortcut_row: state.5,
                recording: state.6.as_deref(),
                pending_replace: pending,
            };
            // The Record button only starts waiting for a key (window state, no intent).
            if sheet == AppSheet::Shortcuts && ev.id == app::id::SC_RECORD && ev.code == notify::CLICKED {
                if let Some(id) = app::shortcut_row_id(&input, &ev.values) {
                    host(|h| {
                        h.recording = Some(id);
                        h.pending_replace = None;
                    });
                    super::set_capture_keys(true);
                }
                return PanelReaction::Nothing;
            }
            let reaction = app::on_app_event(sheet, &input, ev);
            if sheet == AppSheet::Shortcuts && ev.code == notify::KEY {
                // One key ends the waiting, whatever came of it (Esc just stops).
                host(|h| h.recording = None);
                super::set_capture_keys(false);
            }
            reaction
        }
    }
}

fn on_event(key: PanelKey, ev: PanelEvent) {
    let Some(bridge) = bridge() else { return };
    note_state(key, &ev);
    let Some(snap) = bridge.snapshot(document_of(key)) else { return };
    let reaction = reaction_for(key, &ev, &snap, bridge.locale(), bridge.as_ref());
    let owner = super::hwnd_of(key);
    perform(reaction, owner);
    // A shortcut that was refused as a conflict may be taken over with one more click.
    after_shortcut(&ev);
    refresh();
}

/// Remembers a conflicting key so that "Vervang" can take it over, forgets it after anything else.
fn after_shortcut(ev: &PanelEvent) {
    let Some(bridge) = bridge() else { return };
    let Some(stroke) = ev.key else {
        if ev.id == app::id::SC_REPLACE {
            host(|h| h.pending_replace = None);
        }
        return;
    };
    let Some(snap) = bridge.snapshot(None) else { return };
    let recording = host(|h| h.pending_replace.clone());
    let _ = recording;
    if let Some(error) = &snap.shortcuts.error
        && matches!(error.problem, burnburn_presentation::ShortcutProblem::Conflict { .. })
        && let Some(binding) = crate::app_menu::binding_from_keystroke(&stroke)
    {
        host(|h| h.pending_replace = Some((error.id.clone(), binding)));
    } else {
        host(|h| h.pending_replace = None);
    }
}

// ---------------------------------------------------------------------------------------------
// Performing reactions.
// ---------------------------------------------------------------------------------------------

/// Sends intents, shows dialogs (and acts on their answers) and performs host requests.
pub fn perform(reaction: PanelReaction, owner: Option<HWND>) {
    let Some(bridge) = bridge() else { return };
    match reaction {
        PanelReaction::Nothing => {}
        PanelReaction::Intent(intent) => bridge.dispatch(intent),
        PanelReaction::Intents(intents) => {
            for intent in intents {
                bridge.dispatch(intent);
            }
        }
        PanelReaction::Dialog(request) => run_dialog(request, owner),
        PanelReaction::Host(request) => perform_host(request, owner),
    }
}

fn run_dialog(request: DialogRequest, owner: Option<HWND>) {
    let Some(bridge) = bridge() else { return };
    match request {
        DialogRequest::Open { purpose, title, filters } => {
            let path = dialogs::pick_open(owner, &title, &filters);
            if let Some(snap) = bridge.snapshot(None) {
                perform(on_file_chosen(&snap.vm, &purpose, path), owner);
            }
        }
        DialogRequest::Save { purpose, title, suggested_name, filters } => {
            let path = dialogs::pick_save(owner, &title, &suggested_name, &filters);
            if let Some(snap) = bridge.snapshot(None) {
                perform(on_file_chosen(&snap.vm, &purpose, path), owner);
            }
        }
        DialogRequest::Confirm { purpose, title, body, confirm, cancel: _ } => {
            let ConfirmPurpose::DeleteControllerFile { .. } = &purpose;
            let yes = dialogs::confirm(owner, &title, &body, &confirm);
            perform(on_confirmed(&purpose, yes), owner);
        }
    }
}

fn perform_host(request: HostRequest, owner: Option<HWND>) {
    let Some(bridge) = bridge() else { return };
    match request {
        HostRequest::ShowSheet(family, code) => {
            let active = bridge.snapshot(None).and_then(|s| s.vm.active);
            show_sheet(family, code, active);
        }
        HostRequest::HideSheet(family, code) => {
            let active = bridge.snapshot(None).and_then(|s| s.vm.active);
            super::close(key_for(family, code, active));
        }
        HostRequest::OpenUrl(url) => {
            let url = HSTRING::from(url);
            // SAFETY: opens the address with the default handler; strings outlive the call.
            let _ = unsafe { ShellExecuteW(None, w!("open"), &url, None, None, SW_SHOWNORMAL) };
        }
        HostRequest::MessageBox { title, body } => dialogs::message(owner, &title, &body),
    }
}

/// A host effect of the files family (RD export, templates, bundles) shows its dialog. `true`: ours.
pub fn effect(effect: &HostEffect) -> bool {
    let Some(bridge) = bridge() else { return false };
    let Some(snap) = bridge.snapshot(None) else { return false };
    let Some(document) = snap.vm.active else { return false };
    let scope = host(|h| h.template_scope).unwrap_or(0);
    let input = FilesInput {
        document,
        vm: &snap.vm,
        locale: bridge.locale(),
        recent_projects: &snap.recent_projects,
        recent_templates: &snap.recent_templates,
        template_scope: scope,
        recent_row: None,
        template_row: None,
    };
    if let Some(request) = files::effect_dialog(effect, &input) {
        run_dialog(request, None);
        return true;
    }
    if matches!(effect, HostEffect::RecentProjectsChanged { .. } | HostEffect::RecentTemplatesChanged { .. }) {
        refill_recent_menus();
    }
    false
}

// ---------------------------------------------------------------------------------------------
// Menus.
// ---------------------------------------------------------------------------------------------

fn menu_context<'a>(snap: &'a Snapshot, locale: Locale) -> MenuContext<'a> {
    MenuContext {
        vm: &snap.vm,
        locale,
        manual_known: AppController::help_info().manual_url.is_some(),
        recent_projects: &snap.recent_projects,
        recent_templates: &snap.recent_templates,
    }
}

/// A menu id of this lane was chosen. `false`: not ours.
pub fn handle_menu(id: u16) -> bool {
    let Some(item) = Item::from_id(id) else { return false };
    let Some(bridge) = bridge() else { return true };
    let Some(snap) = bridge.snapshot(None) else { return true };
    let reaction = menu::reaction(item, &menu_context(&snap, bridge.locale()));
    perform(reaction, None);
    refresh();
    true
}

/// The recent-list popups of one document window's menu bar.
#[derive(Clone, Copy)]
struct RecentMenus {
    frame: isize,
    projects: Option<isize>,
    templates: Option<isize>,
}

/// A window got its menu bar: its two recent-list popups are filled now and on every change.
pub fn register_recent_menus(frame: HWND, projects: Option<HMENU>, templates: Option<HMENU>) {
    host(|h| {
        h.recent_menus.retain(|m| m.frame != frame.0 as isize);
        h.recent_menus.push(RecentMenus {
            frame: frame.0 as isize,
            projects: projects.map(|m| m.0 as isize),
            templates: templates.map(|m| m.0 as isize),
        });
    });
    refill_recent_menus();
}

/// A window is gone (its menus are destroyed with it).
pub fn forget_recent_menus(frame: HWND) {
    host(|h| h.recent_menus.retain(|m| m.frame != frame.0 as isize));
}

/// Rebuilds the two recent lists of every open window from the controller's lists.
pub fn refill_recent_menus() {
    let Some(bridge) = bridge() else { return };
    let Some(snap) = bridge.snapshot(None) else { return };
    let ctx = menu_context(&snap, bridge.locale());
    let windows = host(|h| h.recent_menus.clone()).unwrap_or_default();
    for window in windows {
        for (menu, entries) in [
            (window.projects, (0..RECENT_SLOTS).map(Item::RecentProject).collect::<Vec<_>>()),
            (window.templates, (0..RECENT_SLOTS).map(Item::RecentTemplate).collect::<Vec<_>>()),
        ] {
            let Some(raw) = menu else { continue };
            let menu = HMENU(raw as *mut _);
            for item in &entries {
                // SAFETY: removing an item of our own menu by id (absent ids are ignored).
                let _ = unsafe { DeleteMenu(menu, u32::from(item.id()), MF_BYCOMMAND) };
            }
            // The entries go in front of the separator and the "clear" item that stay in the popup.
            for (position, item) in entries.into_iter().filter(|i| i.enabled(&ctx)).enumerate() {
                let text = HSTRING::from(crate::menus::mnemonics::numbered(position, &item.title(&ctx)));
                // SAFETY: inserting into our own menu at a position; the string outlives the call.
                let _ = unsafe {
                    InsertMenuW(menu, position as u32, MF_BYPOSITION | MF_STRING, usize::from(item.id()), &text)
                };
            }
        }
    }
}

/// Enables or greys the lane's items of `menu` (call on `WM_INITMENUPOPUP`).
pub fn refresh_menu_state(menu: HMENU) {
    let Some(bridge) = bridge() else { return };
    let Some(snap) = bridge.snapshot(None) else { return };
    let ctx = menu_context(&snap, bridge.locale());
    for item in menu::items() {
        let flag = if item.enabled(&ctx) { MF_ENABLED } else { MF_GRAYED };
        // SAFETY: the popup handle comes with the message; ids are ours (absent ids are ignored).
        let _ = unsafe { EnableMenuItem(menu, u32::from(item.id()), flag) };
    }
}

/// An app-level `AppIntent` the host sends itself (kept here so that the type is used in one place).
pub fn is_app_intent(intent: &UiIntent) -> bool {
    matches!(intent, UiIntent::App(AppIntent::SetPreference { .. }))
}
