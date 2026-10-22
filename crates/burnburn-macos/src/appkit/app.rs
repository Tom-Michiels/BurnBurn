//! The host's application object: one `AppController`, its document windows, the native sheets
//! and the application delegate's decisions (open from Finder, quit with dirty documents).
//! Main thread only (`Rc`, `RefCell`).
//!
//! Re-entrancy rule: the controller is borrowed only for one `handle`/`task_finished` call; effects
//! are applied after the borrow ends, so AppKit callbacks triggered by an effect (closing a
//! window, selecting table rows) can dispatch again.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::sync::Arc;

use block2::RcBlock;
use burnburn_editor::Viewport;
use burnburn_io::ProjectUuid;
use burnburn_io::fs::FileSystem;
use burnburn_presentation::{
    AppController, AppPorts, AppUpdate, AppViewModel, CamDialog, CameraPort, CloseChoice, CommandKey, HostEffect,
    MessageKey, UiIntent,
};
use burnburn_types::DocumentId;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly, sel};
use objc2_app_kit::{
    NSAlert, NSApplication, NSApplicationTerminateReply, NSEventModifierFlags, NSMenu, NSMenuItem, NSModalResponse,
    NSOpenPanel, NSSavePanel,
};
use objc2_foundation::{NSArray, NSString, NSTimer};

mod app_host;
mod cam_host;
mod camera_host;
mod files_host;
pub use app_host::{MenuKeyReport, app_user_dir};
mod laser_host;
pub use cam_host::CamPanelParts;
mod schikken_host;
mod text_host;

use super::classes::{AppDelegate, MenuTarget, app_delegate};
use super::objc_try;
use super::window::{CanvasStatus, DocWindow};
use crate::accessibility::{AxElement, canvas_elements};
use crate::adapter::{
    CLOSE_ALERT_BUTTONS, MenuItem, NativeEvent, Reaction, key_code, menu_items, menu_tag, recent_open_reaction,
    translate, window_title,
};
use crate::l10n::{HostText, Locale, close_alert_title, host_text};
use crate::quit_guard::{GUARD_ALERT_BUTTONS, GuardChoice, guard_button, guard_choice, guard_message, quit_guard};
use crate::lifecycle::{QuitFlow, QuitStep, RECOVERY_ALERT_BUTTONS, open_requests, recovery_message};
use crate::panels::CameraControl;
use crate::tasks::{TaskInbox, task_channel};

/// Host behaviour that differs between the app and native tests.
#[derive(Debug, Clone, PartialEq)]
pub struct HostOptions {
    /// The app quits when its last window closes; tests keep the process.
    pub quit_when_last_window_closes: bool,
    /// Tests float windows above the user's frontmost app so they are really on screen.
    pub float_windows: bool,
    /// UI language; `None` follows the system's preferred languages.
    pub locale: Option<Locale>,
    /// The app really terminates when a quit is answered; tests only record the outcome.
    pub terminate_on_quit: bool,
    /// Where autosave writes recovery files (created when missing); `None`: no autosave and no
    /// recovery offer.
    pub recovery_dir: Option<PathBuf>,
    /// Autosave tick in seconds; `None`: only on request (`HostApp::autosave_now`, crash guard).
    pub autosave_interval_s: Option<f64>,
    /// Use a private, uniquely named pasteboard instead of the user's general one (tests must
    /// never touch the user's clipboard).
    pub private_pasteboard: bool,
    /// Where Archief ▸ Recente projecten is stored (read at start, written on every change);
    /// `None`: the list is neither read nor stored.
    pub recent_projects_file: Option<PathBuf>,
}

impl HostOptions {
    /// The app's options: quit with the last window, follow the system language, autosave into
    /// `~/Library/Application Support/BurnBurn/Recovery`.
    pub fn app() -> HostOptions {
        let support = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library").join("Application Support"));
        HostOptions {
            quit_when_last_window_closes: true,
            float_windows: false,
            locale: None,
            terminate_on_quit: true,
            recovery_dir: support.as_ref().map(|s| crate::lifecycle::recovery_dir(s)),
            autosave_interval_s: Some(crate::lifecycle::AUTOSAVE_INTERVAL_S),
            private_pasteboard: false,
            recent_projects_file: support.as_deref().map(crate::recent::recent_projects_file),
        }
    }
}

/// How an `applicationShouldTerminate:` with dirty documents ended (evidence for tests/logs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuitOutcome {
    Terminate,
    Cancelled,
}

thread_local! {
    /// The app the main-queue wake-up delivers task outcomes to.
    static CURRENT: RefCell<Weak<HostApp>> = const { RefCell::new(Weak::new()) };
}

pub struct HostApp {
    mtm: MainThreadMarker,
    me: Weak<HostApp>,
    controller: RefCell<AppController>,
    windows: RefCell<BTreeMap<DocumentId, DocWindow>>,
    inbox: TaskInbox,
    /// Keeps the menu items' (weak) target alive for the app's lifetime.
    _target: Retained<MenuTarget>,
    /// `NSApplication.delegate` is weak: the host keeps it.
    _delegate: Retained<AppDelegate>,
    menu: Retained<NSMenu>,
    recent_menu: Retained<NSMenu>,
    /// Archief ▸ Recente templates, rebuilt from the controller's list.
    recent_templates_menu: Retained<NSMenu>,
    pasteboard: Retained<objc2_app_kit::NSPasteboard>,
    open_panel: RefCell<Option<Retained<NSOpenPanel>>>,
    options: HostOptions,
    locale: Locale,
    /// A quit waiting for the user's answers (`NSTerminateLater`).
    quit: RefCell<Option<QuitFlow>>,
    advancing_quit: Cell<bool>,
    quit_outcomes: RefCell<Vec<QuitOutcome>>,
    /// A file open was requested before launch finished (no untitled window then).
    open_requested: Cell<bool>,
    /// Every intent sent to the controller, in order (evidence for tests and logs).
    dispatched: RefCell<Vec<UiIntent>>,
    /// Final canvas text from the input route (evidence for tests; it goes on as `InsertText`).
    text_commits: RefCell<Vec<String>>,
    /// Recovery files still to offer, the alert on screen and its position ("1 van 2").
    recovery_queue: RefCell<VecDeque<PathBuf>>,
    recovery_alert: RefCell<Option<Retained<NSAlert>>>,
    recovery_count: Cell<(usize, usize)>,
    /// Autosave tick and the camera tick (refreshes the camera section while a camera is attached).
    timers: RefCell<Vec<Retained<NSTimer>>>,
    /// The panels of the open CAM dialogs (Optimization, tests, Device Settings).
    cam_panels: RefCell<cam_host::CamPanels>,
    /// The camera window (W3-UI-camera), created on first use.
    camera_window: RefCell<Option<super::panels::camera_window::CameraWindow>>,
    /// The app-level panels (preferences, shortcuts, history, preview, RD) and their small state.
    app_panels: RefCell<super::app_panels::AppPanels>,
    /// The preferences the host last put into effect, the autosave tick that follows them, the
    /// canvas curve tolerance and what the menu's keys said about the shortcut register.
    applied_prefs: Cell<Option<burnburn_presentation::Preferences>>,
    autosave_timer: RefCell<Option<Retained<NSTimer>>>,
    display_tolerance_px: Cell<f64>,
    applied_shortcut_revision: Cell<u64>,
    menu_key_report: RefCell<app_host::MenuKeyReport>,
}

/// Ask the running host (on the main queue, after the current callback) to autosave: used when a
/// callback panicked, so unsaved work reaches a recovery file.
pub(crate) fn request_emergency_autosave() {
    dispatch2::DispatchQueue::main().exec_async(|| {
        CURRENT.with(|current| {
            let app = current.try_borrow().ok().and_then(|w| w.upgrade());
            if let Some(app) = app {
                app.autosave_now();
            }
        });
    });
}

fn random_uuid() -> ProjectUuid {
    use std::hash::{BuildHasher, Hasher};
    let mut bytes = [0u8; 16];
    for (i, chunk) in bytes.chunks_mut(8).enumerate() {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_usize(i);
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
        chunk.copy_from_slice(&h.finish().to_le_bytes()[..chunk.len()]);
    }
    ProjectUuid::from_bytes(bytes)
}

impl HostApp {
    /// Create the app object, its controller (with `fs` for save/open tasks), the main menu and
    /// the application delegate.
    pub fn new(mtm: MainThreadMarker, fs: Arc<dyn FileSystem>, options: HostOptions) -> Rc<HostApp> {
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {
            dispatch2::DispatchQueue::main().exec_async(|| {
                CURRENT.with(|current| {
                    let app = current.try_borrow().ok().and_then(|w| w.upgrade());
                    if let Some(app) = app {
                        app.drain_tasks();
                    }
                });
            });
        });
        let (port, inbox) = task_channel(wake);
        let ports = AppPorts { tasks: Arc::new(port), fs, new_project_uuid: Arc::new(random_uuid) };
        let locale = options.locale.unwrap_or_else(super::system_locale);
        let private_pasteboard = options.private_pasteboard;
        let app = Rc::new_cyclic(|me: &Weak<HostApp>| {
            let target = MenuTarget::new(mtm);
            target.set_app(me.clone());
            let bar = main_menu(mtm, &target, locale);
            HostApp {
                mtm,
                me: me.clone(),
                controller: RefCell::new(AppController::new(ports)),
                windows: RefCell::new(BTreeMap::new()),
                inbox,
                _target: target,
                _delegate: AppDelegate::new(mtm, me.clone()),
                menu: bar.main,
                recent_menu: bar.recent,
                recent_templates_menu: bar.recent_templates,
                pasteboard: if private_pasteboard {
                    super::clipboard::private_pasteboard()
                } else {
                    super::clipboard::general_pasteboard()
                },
                open_panel: RefCell::new(None),
                options,
                locale,
                quit: RefCell::new(None),
                advancing_quit: Cell::new(false),
                quit_outcomes: RefCell::new(Vec::new()),
                open_requested: Cell::new(false),
                dispatched: RefCell::new(Vec::new()),
                text_commits: RefCell::new(Vec::new()),
                recovery_queue: RefCell::new(VecDeque::new()),
                recovery_alert: RefCell::new(None),
                recovery_count: Cell::new((0, 0)),
                timers: RefCell::new(Vec::new()),
                cam_panels: RefCell::new(cam_host::CamPanels::new()),
                camera_window: RefCell::new(None),
                app_panels: RefCell::new(super::app_panels::AppPanels::new(mtm, me.clone())),
                applied_prefs: Cell::new(None),
                autosave_timer: RefCell::new(None),
                display_tolerance_px: Cell::new(burnburn_presentation::DisplayQuality::Smooth.tolerance_px()),
                applied_shortcut_revision: Cell::new(0),
                menu_key_report: RefCell::new(app_host::MenuKeyReport::default()),
            }
        });
        if let Ok(mut controller) = app.controller.try_borrow_mut() {
            // Text needs faces: the installed system fonts, listed by the family they declare
            // (the text toolbar's choice); none at all falls back to the first readable default.
            let catalog = burnburn_runtime::fonts::SystemFontCatalog::system();
            if let Some(font) = catalog.default_font() {
                controller.attach_fonts(catalog, font);
            } else if let Some(font) =
                burnburn_runtime::fonts::first_readable_font(&burnburn_runtime::fonts::default_font_candidates())
            {
                controller.attach_fonts(font.source, font.font);
            }
            controller.attach_clock(Arc::new(text_host::host_clock));
            if let Some(dir) = &app.options.recovery_dir {
                if let Err(e) = std::fs::create_dir_all(dir) {
                    eprintln!("burnburn: recovery folder {}: {e}", dir.display());
                }
                controller.set_recovery_dir(dir.clone());
            }
        }
        if let Some(file) = &app.options.recent_projects_file {
            // A missing or damaged file is an empty list.
            let paths = crate::recent::read_recent_projects(file);
            if let Ok(mut controller) = app.controller.try_borrow_mut() {
                controller.handle(UiIntent::SetRecentProjects { paths });
            }
            app.rebuild_recent_menu(&app.recent_paths());
        }
        app.init_files();
        if let Some(interval) = app.options.autosave_interval_s {
            app.set_autosave_interval(Some(interval));
        }
        CURRENT.with(|c| {
            if let Ok(mut c) = c.try_borrow_mut() {
                *c = Rc::downgrade(&app);
            }
        });
        let ns_app = NSApplication::sharedApplication(mtm);
        ns_app.setMainMenu(Some(&app.menu));
        ns_app.setDelegate(Some(app_delegate(&app._delegate)));
        // The register's bindings (and any the user changed) go into the menu; what the menu had
        // that the register does not know is kept for the test that guards it.
        let report = app.apply_menu_shortcuts();
        if let Ok(mut slot) = app.menu_key_report.try_borrow_mut() {
            *slot = report;
        }
        app
    }

    /// What applying the shortcut register to the menu found that the register does not cover
    /// (tests assert it is empty).
    pub fn menu_key_report(&self) -> MenuKeyReport {
        self.menu_key_report.try_borrow().map(|r| r.clone()).unwrap_or_default()
    }

    pub fn view(&self) -> Arc<AppViewModel> {
        match self.controller.try_borrow() {
            Ok(c) => c.view(),
            Err(_) => Arc::new(AppViewModel {
                documents: Vec::new(),
                active: None,
                commands: BTreeMap::new(),
                toolbar: Default::default(),
            }),
        }
    }

    pub fn menu(&self) -> &NSMenu {
        &self.menu
    }

    /// Archief ▸ Recente projecten.
    pub fn recent_menu(&self) -> &NSMenu {
        &self.recent_menu
    }

    /// How many recent projects the menu offers now (`validateMenuItem:`).
    pub(crate) fn recent_count(&self) -> usize {
        self.controller.try_borrow().map(|c| c.recent_projects().len()).unwrap_or(0)
    }

    /// The pasteboard Cut/Copy/Paste use.
    pub fn pasteboard(&self) -> &objc2_app_kit::NSPasteboard {
        &self.pasteboard
    }

    /// The UI language chosen at start-up.
    pub fn locale(&self) -> Locale {
        self.locale
    }

    /// Intents sent so far (and clears the record).
    pub fn take_dispatched(&self) -> Vec<UiIntent> {
        self.dispatched.try_borrow_mut().map(|mut d| std::mem::take(&mut *d)).unwrap_or_default()
    }

    /// Send one intent to the controller and perform what it asks.
    pub fn dispatch(&self, intent: UiIntent) -> AppUpdate {
        let update = self.dispatch_without_apply(intent);
        self.apply(&update);
        update
    }

    fn dispatch_without_apply(&self, intent: UiIntent) -> AppUpdate {
        if let Ok(mut d) = self.dispatched.try_borrow_mut() {
            d.push(intent.clone());
        }
        if let Ok(mut quit) = self.quit.try_borrow_mut()
            && let Some(flow) = quit.as_mut()
        {
            flow.observe_intent(&intent);
        }
        match self.controller.try_borrow_mut() {
            Ok(mut controller) => controller.handle(intent),
            Err(_) => {
                eprintln!("burnburn: re-entrant intent dropped: {intent:?}");
                AppUpdate::default()
            }
        }
    }

    fn recent_paths(&self) -> Vec<PathBuf> {
        self.controller.try_borrow().map(|c| c.recent_projects()).unwrap_or_default()
    }

    /// Rebuild Archief ▸ Recente projecten from `paths` (most recent first).
    fn rebuild_recent_menu(&self, paths: &[PathBuf]) {
        fill_recent_menu(self.mtm, &self._target, &self.recent_menu, paths, self.locale);
    }

    /// Cut or Copy of the canvas selection (Edit items through the responder chain).
    pub(crate) fn clipboard_command(&self, document: DocumentId, command: CommandKey) {
        self.dispatch(UiIntent::Invoke { document: Some(document), command });
    }

    /// Paste into `document`: BurnBurn's own type on the pasteboard wins over the controller's
    /// last copy (another window or process may have copied since).
    pub(crate) fn paste(&self, document: DocumentId, in_place: bool) {
        let bytes = super::clipboard::read_clipboard(&self.pasteboard);
        self.dispatch(crate::pasteboard::paste_intent(Some(document), in_place, bytes));
    }

    /// Enablement of an Edit item that reaches the canvas through the responder chain.
    pub(crate) fn edit_item_enabled(&self, command: CommandKey) -> bool {
        crate::pasteboard::edit_item_enabled(command, &self.view(), super::clipboard::has_clipboard(&self.pasteboard))
    }

    /// One native event: translated with the current view model, then performed.
    pub fn native(&self, event: NativeEvent) {
        if let NativeEvent::Menu(item @ MenuItem::Recent(_)) = &event {
            // The entry's path is in the stored list, which the view model does not carry.
            if let Reaction::Intent(intent) = recent_open_reaction(*item, &self.recent_paths()) {
                self.dispatch(intent);
            }
            return;
        }
        if let NativeEvent::Menu(MenuItem::RecentTemplate(n)) = &event {
            self.open_recent_template(*n);
            return;
        }
        if let NativeEvent::TextCommitted { text, .. } = &event
            && let Ok(mut commits) = self.text_commits.try_borrow_mut()
        {
            commits.push(text.clone());
        }
        if let NativeEvent::ToolStripRestyle { .. } = &event {
            // Light/dark or backing scale changed: the strip draws its icons again.
            self.refresh();
            return;
        }
        match translate(event, &self.view()) {
            Reaction::Nothing => {}
            Reaction::Intent(intent) => {
                self.dispatch(intent);
            }
            Reaction::ShowOpenPanel => self.show_open_panel(),
            Reaction::ShowImportPanel => self.show_import_panel(),
            Reaction::ShowReplaceImagePanel => self.show_image_file_panel(ImageFilePanel::Replace),
            Reaction::ShowBitmapSavePanel => self.show_bitmap_save_panel(),
            Reaction::ShowTracePanel => self.show_image_file_panel(ImageFilePanel::Trace),
            Reaction::ToggleLaserPanel => self.toggle_laser_panel(),
            Reaction::ToggleSchikkenPanel => self.toggle_schikken_panel(),
            Reaction::ShowDiagnosticsPanel => self.show_diagnostics_panel(),
            Reaction::App(action) => self.perform_app_action(action),
        }
    }

    /// The arrow of a variant group was clicked: show the drop-down below it. The menu is built
    /// while the windows are borrowed and shown after the borrow ended (its action re-enters).
    pub(crate) fn show_variant_menu(&self, document: DocumentId, group: &str, anchor: &objc2_app_kit::NSButton) {
        let menu = match self.windows.try_borrow() {
            Ok(windows) => windows.get(&document).and_then(|w| w.strip.menu(group)),
            Err(_) => None,
        };
        let Some(menu) = menu else { return };
        let below = objc2_foundation::NSPoint::new(0.0, 0.0);
        menu.popUpMenuPositioningItem_atLocation_inView(None, below, Some(anchor));
    }

    /// Deliver finished worker tasks (runs on the main queue after a worker's wake-up).
    pub fn drain_tasks(&self) {
        for outcome in self.inbox.drain() {
            let update = match self.controller.try_borrow_mut() {
                Ok(mut controller) => controller.task_finished(outcome),
                Err(_) => continue,
            };
            self.apply(&update);
        }
    }

    // ---- Application delegate ----------------------------------------------------------------

    /// `applicationShouldTerminate:`. Nothing to lose: now. Otherwise ask every dirty document
    /// in turn (the same sheet as its close button) and answer later.
    pub(crate) fn should_terminate(&self) -> NSApplicationTerminateReply {
        if self.quit.try_borrow().map(|q| q.is_some()).unwrap_or(true) {
            // A second Cmd-Q while the questions are up: keep waiting for them.
            return NSApplicationTerminateReply::TerminateLater;
        }
        // Update gate: a transfer, a stored or running job or live control needs a confirmation
        // that names the reason, before any document question.
        let busy = self.controller.try_borrow().ok().and_then(|c| quit_guard(&c.machine_activity()));
        if let Some(reason) = busy
            && !self.confirm_quit_while_busy(reason)
        {
            if let Ok(mut outcomes) = self.quit_outcomes.try_borrow_mut() {
                outcomes.push(QuitOutcome::Cancelled);
            }
            return NSApplicationTerminateReply::TerminateCancel;
        }
        let Some(flow) = QuitFlow::begin(&self.view()) else { return NSApplicationTerminateReply::TerminateNow };
        if let Ok(mut quit) = self.quit.try_borrow_mut() {
            *quit = Some(flow);
        }
        self.advance_quit();
        NSApplicationTerminateReply::TerminateLater
    }

    /// The update-gate alert (modal): true only when the user chose "Toch afsluiten".
    fn confirm_quit_while_busy(&self, reason: burnburn_runtime::update_gate::DeferReason) -> bool {
        let (title, info) = guard_message(reason, self.locale);
        let alert = NSAlert::new(self.mtm);
        alert.setMessageText(&NSString::from_str(title));
        alert.setInformativeText(&NSString::from_str(info));
        for choice in GUARD_ALERT_BUTTONS {
            alert.addButtonWithTitle(&NSString::from_str(guard_button(choice, self.locale)));
        }
        let response = match objc_try("showing the quit guard", || alert.runModal()) {
            Ok(response) => response,
            Err(e) => {
                eprintln!("burnburn: {e}");
                return false;
            }
        };
        guard_choice(response) == GuardChoice::QuitAnyway
    }

    fn advance_quit(&self) {
        if self.advancing_quit.replace(true) {
            return;
        }
        loop {
            let vm = self.view();
            let step = match self.quit.try_borrow_mut() {
                Ok(mut quit) => match quit.as_mut() {
                    Some(flow) => flow.step(&vm),
                    None => break,
                },
                Err(_) => break,
            };
            match step {
                QuitStep::Close(document) => {
                    self.dispatch(UiIntent::Close { document });
                }
                QuitStep::Wait => break,
                QuitStep::Terminate => {
                    self.finish_quit(QuitOutcome::Terminate);
                    break;
                }
                QuitStep::Abort => {
                    self.finish_quit(QuitOutcome::Cancelled);
                    break;
                }
            }
        }
        self.advancing_quit.set(false);
    }

    fn finish_quit(&self, outcome: QuitOutcome) {
        if let Ok(mut quit) = self.quit.try_borrow_mut() {
            *quit = None;
        }
        if let Ok(mut outcomes) = self.quit_outcomes.try_borrow_mut() {
            outcomes.push(outcome);
        }
        if self.options.terminate_on_quit {
            NSApplication::sharedApplication(self.mtm)
                .replyToApplicationShouldTerminate(outcome == QuitOutcome::Terminate);
        }
    }

    /// `application:openURLs:` (Finder double-click, Open With, drop on the Dock icon).
    pub(crate) fn open_paths(&self, paths: Vec<PathBuf>) {
        self.open_requested.set(true);
        for intent in open_requests(paths) {
            self.dispatch(intent);
        }
    }

    /// `applicationDidFinishLaunching:`: an untitled document unless a file is being opened.
    pub(crate) fn did_finish_launching(&self) {
        if !self.open_requested.get() && self.open_documents().is_empty() {
            self.dispatch(UiIntent::NewDocument);
        }
        NSApplication::sharedApplication(self.mtm).activate();
        self.check_recovery();
    }

    // ---- Autosave, recovery, camera ------------------------------------------------------------

    /// What the autosave tick does: presentation writes recovery files for documents changed
    /// since their last autosave (never the project file).
    pub fn autosave_now(&self) {
        self.dispatch(UiIntent::Autosave);
    }

    /// Look for recovery files (at start); each found file is offered in an alert.
    pub fn check_recovery(&self) {
        if self.options.recovery_dir.is_some() {
            self.dispatch(UiIntent::CheckRecovery);
        }
    }

    /// The recovery alert on screen, if any (tests answer it like a user).
    pub fn recovery_alert(&self) -> Option<Retained<NSAlert>> {
        self.recovery_alert.try_borrow().ok()?.clone()
    }

    /// Attach the bed camera. Nothing is asked or started: only the section's buttons do that.
    pub fn attach_camera(&self, camera: Arc<dyn CameraPort>) {
        if let Ok(mut controller) = self.controller.try_borrow_mut() {
            controller.attach_camera(camera);
        }
        self.start_timer(0.5, HostApp::refresh_camera);
        self.refresh();
    }

    /// A control of the Cut Settings sheet changed which option rows exist or are greyed.
    pub(crate) fn cut_option_changed(&self) {
        self.refresh();
    }

    /// A camera button with device pop-up item `device_index` chosen.
    pub(crate) fn camera_button_pressed(&self, control: CameraControl, device_index: isize) {
        if control == CameraControl::Window {
            self.toggle_camera_window();
            return;
        }
        let camera = self.controller.try_borrow().ok().and_then(|c| c.camera_view());
        if let Some(intent) = crate::panels::camera_intent(camera.as_ref(), control, device_index) {
            self.dispatch(intent);
            self.refresh();
        }
    }

    /// Copy the camera view model into every window's camera section (frames and permission
    /// answers arrive without an intent).
    fn refresh_camera(&self) {
        // Finished corner detections and lens fits arrive without an intent.
        let delivered = self.controller.try_borrow_mut().ok().map(|mut c| c.poll_camera_work());
        if delivered.is_some_and(|u| u.view_changed) {
            self.refresh_camera_window();
        }
        let camera = self.controller.try_borrow().ok().and_then(|c| c.camera_view());
        if let Ok(windows) = self.windows.try_borrow() {
            for w in windows.values() {
                w.refresh_camera(camera.as_ref());
            }
        }
    }

    /// A repeating main-thread timer calling `tick` on this app while it lives.
    #[allow(unsafe_code)]
    fn start_timer(&self, interval_s: f64, tick: fn(&HostApp)) {
        let me = self.me.clone();
        let block = RcBlock::new(move |_timer: std::ptr::NonNull<NSTimer>| {
            super::classes::ffi_guard("timer", (), || {
                if let Some(app) = me.upgrade() {
                    tick(&app);
                }
            });
        });
        // SAFETY: a block-based timer on the main run loop; the block only upgrades a `Weak` and
        // calls the host on the main thread; the timer is invalidated when the host drops.
        let timer = unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(interval_s, true, &block) };
        if let Ok(mut timers) = self.timers.try_borrow_mut() {
            timers.push(timer);
        }
    }

    /// Offer the next queued recovery file (one alert at a time, as a sheet on a window).
    fn show_next_recovery(&self) {
        if self.recovery_alert().is_some() {
            return;
        }
        let Some(path) = self.recovery_queue.try_borrow_mut().ok().and_then(|mut q| q.pop_front()) else { return };
        let (done, total) = self.recovery_count.get();
        self.recovery_count.set((done + 1, total));
        let (title, info) = recovery_message(&path, done + 1, total, self.locale);
        let alert = NSAlert::new(self.mtm);
        alert.setMessageText(&NSString::from_str(&title));
        alert.setInformativeText(&NSString::from_str(&info));
        for choice in RECOVERY_ALERT_BUTTONS {
            alert.addButtonWithTitle(&NSString::from_str(host_text(choice.text(), self.locale)));
        }
        let me = self.me.clone();
        let answered = path.clone();
        let handler = RcBlock::new(move |response: NSModalResponse| {
            super::classes::ffi_guard("recovery alert completion", (), || {
                if let Some(app) = me.upgrade() {
                    if let Ok(mut a) = app.recovery_alert.try_borrow_mut() {
                        *a = None;
                    }
                    app.native(NativeEvent::RecoveryAnswered { path: answered.clone(), response });
                    app.show_next_recovery();
                }
            });
        });
        let window = self.view().active.and_then(|d| self.window(d)).or_else(|| {
            let windows = self.windows.try_borrow().ok()?;
            windows.values().next().map(|w| w.window.clone())
        });
        let Some(window) = window else {
            // No window to attach a sheet to: ask modally.
            let response = alert.runModal();
            self.native(NativeEvent::RecoveryAnswered { path, response });
            self.show_next_recovery();
            return;
        };
        match objc_try("showing the recovery sheet", || {
            alert.beginSheetModalForWindow_completionHandler(&window, Some(&handler))
        }) {
            Ok(()) => {
                if let Ok(mut a) = self.recovery_alert.try_borrow_mut() {
                    *a = Some(alert);
                }
            }
            Err(e) => eprintln!("burnburn: {e}"),
        }
    }

    // ---- Window callbacks ----------------------------------------------------------------------

    /// `windowShouldClose:`: ask presentation. True only when it closes the window right away.
    pub(crate) fn close_requested(&self, document: DocumentId) -> bool {
        let Reaction::Intent(intent) = translate(NativeEvent::CloseRequested { document }, &self.view()) else {
            return true;
        };
        let mut update = self.dispatch_without_apply(intent);
        let closes = update.effects.contains(&HostEffect::CloseWindow { document });
        // AppKit closes the window itself when we return true; do not close it a second time.
        update.effects.retain(|e| *e != HostEffect::CloseWindow { document });
        if closes
            && let Ok(windows) = self.windows.try_borrow()
            && let Some(w) = windows.get(&document)
        {
            w.controller.allow_close();
        }
        self.apply(&update);
        closes
    }

    /// `windowWillClose:`. A window closed behind presentation's back (only the host itself does
    /// that, e.g. on drop) discards its session so the controller and the screen agree.
    pub(crate) fn window_closed(&self, document: DocumentId) {
        let mut removed = self.windows.try_borrow_mut().ok().and_then(|mut w| w.remove(&document));
        if let Some(w) = removed.as_mut() {
            w.closing = true;
        }
        if self.view().document(document).is_some() {
            self.dispatch(UiIntent::ConfirmClose { document, choice: CloseChoice::Discard });
        }
        // Drop the DocWindow after AppKit's close finished with it (AppKit retains the window
        // during the notification).
        drop(removed);
        let empty = self.windows.try_borrow().map(|w| w.is_empty()).unwrap_or(false);
        let quitting = self.quit.try_borrow().map(|q| q.is_some()).unwrap_or(true);
        // While quitting, the quit flow answers AppKit itself.
        if empty && self.options.quit_when_last_window_closes && !quitting {
            NSApplication::sharedApplication(self.mtm).terminate(None);
        }
    }

    /// The canvas view changed size or backing scale.
    pub(crate) fn canvas_resized(&self, document: DocumentId) {
        let Some((size, scale)) = self.windows.try_borrow().ok().and_then(|w| {
            let w = w.get(&document)?;
            Some((w.canvas_view.bounds().size, w.window.backingScaleFactor()))
        }) else {
            return;
        };
        self.native(NativeEvent::Resized {
            document,
            width_pt: size.width,
            height_pt: size.height,
            backing_scale: scale,
        });
    }

    /// Esc/Delete/Return on the canvas; false lets AppKit pass the key on.
    pub(crate) fn canvas_key(&self, document: DocumentId, code: u16, flags: usize) -> bool {
        // Arrow keys nudge the selection by the preference distances (not while composing text:
        // `keyDown:` does not ask the host then).
        if let Some(arrow) = self.arrow_for(code) {
            self.canvas_arrow(document, arrow, flags);
            return true;
        }
        // Return finishes pen/polyline paths; during a composition the input method owns it
        // (`keyDown:` does not ask the host then).
        if ![key_code::ESCAPE, key_code::DELETE, key_code::FORWARD_DELETE, key_code::RETURN].contains(&code) {
            return false;
        }
        self.native(NativeEvent::KeyDown { document, key_code: code, flags });
        true
    }

    pub(crate) fn select_all(&self, document: DocumentId) {
        self.dispatch(UiIntent::Invoke { document: Some(document), command: CommandKey::SelectAll });
    }

    /// A machine control in `document`'s window: the view model's button in that slot, sent
    /// with `action` and `seen` unchanged (a disabled one sends nothing).
    pub(crate) fn machine_button_pressed(&self, document: DocumentId, slot: usize) {
        let machine = self.controller.try_borrow().ok().and_then(|c| c.machine_view());
        let intent =
            machine.and_then(|m| m.buttons.get(slot).and_then(|b| crate::panels::machine_intent(Some(document), b)));
        if let Some(intent) = intent {
            self.dispatch(intent);
        }
    }

    /// Accessibility elements of the canvas (from the scene the GPU draws).
    pub(crate) fn canvas_accessibility(&self, document: DocumentId) -> Vec<AxElement> {
        let Some(scene) = self.controller.try_borrow().ok().and_then(|c| c.scene(document)) else { return Vec::new() };
        canvas_elements(&scene, &self.view(), document, self.locale)
    }

    fn apply(&self, update: &AppUpdate) {
        for effect in &update.effects {
            if let Ok(mut quit) = self.quit.try_borrow_mut()
                && let Some(flow) = quit.as_mut()
            {
                flow.observe_effect(effect);
            }
            self.apply_effect(effect);
        }
        if update.view_changed || !update.scenes_changed.is_empty() {
            self.refresh();
        }
        for document in &update.scenes_changed {
            self.redraw(*document);
        }
        // After the refresh, so the table already has the row to edit.
        for effect in &update.effects {
            if let HostEffect::EditSketchDimension { document, constraint, .. } = effect {
                self.begin_dimension_edit(*document, *constraint);
            }
        }
        if self.quit.try_borrow().map(|q| q.is_some()).unwrap_or(false) {
            self.advance_quit();
        }
    }

    fn apply_effect(&self, effect: &HostEffect) {
        match effect {
            HostEffect::OpenWindow { document } => self.open_window(*document),
            HostEffect::CloseWindow { document } => {
                let window = self.windows.try_borrow_mut().ok().and_then(|mut w| w.remove(document));
                if let Some(window) = window {
                    window.close_now();
                }
            }
            HostEffect::ShowSavePanel { document, suggested_name } => {
                self.show_save_panel(*document, suggested_name, SaveKind::Project)
            }
            HostEffect::ShowExportPanel { document, suggested_name } => {
                self.show_save_panel(*document, suggested_name, SaveKind::Rd)
            }
            HostEffect::ConfirmClose { document } => self.show_close_alert(*document),
            HostEffect::ShowError { document, message } => {
                let text = crate::adapter::message_text(*message, self.locale);
                eprintln!("burnburn: {text}");
                if let Some(document) = document
                    && let Ok(windows) = self.windows.try_borrow()
                    && let Some(w) = windows.get(document)
                {
                    w.status_label.setStringValue(&NSString::from_str(&text));
                }
            }
            HostEffect::Redraw { document } => self.redraw(*document),
            HostEffect::SetClipboard { bytes } => {
                if let Err(e) = super::clipboard::write_clipboard(&self.pasteboard, bytes) {
                    eprintln!("burnburn: clipboard: {e}");
                }
            }
            HostEffect::RecentProjectsChanged { paths } => {
                if let Some(file) = &self.options.recent_projects_file
                    && let Err(e) = crate::recent::write_recent_projects(file, paths)
                {
                    eprintln!("burnburn: recent projects {}: {e}", file.display());
                }
                self.rebuild_recent_menu(paths);
            }
            HostEffect::ShowTemplateSavePanel { document, suggested_name } => {
                self.show_template_save_panel(*document, suggested_name)
            }
            HostEffect::ShowTemplateOpenPanel => self.show_template_open_panel(),
            HostEffect::ShowBundleOpenPanel { document } => self.show_bundle_open_panel(*document),
            HostEffect::RecentTemplatesChanged { paths } => self.recent_templates_changed(paths),
            HostEffect::OfferRecovery { files } => {
                if let Ok(mut queue) = self.recovery_queue.try_borrow_mut() {
                    // A second check while alerts are up offers each file once.
                    let fresh: Vec<_> = files.iter().filter(|f| !queue.contains(f)).cloned().collect();
                    let (done, total) =
                        if self.recovery_alert().is_some() { self.recovery_count.get() } else { (0, 0) };
                    self.recovery_count.set((done, total + fresh.len()));
                    queue.extend(fresh);
                }
                self.show_next_recovery();
            }
            _ => {}
        }
    }

    /// Starts editing a dimension's value cell in the sketch panel.
    fn begin_dimension_edit(&self, document: DocumentId, constraint: burnburn_document::sketch::ConstraintId) {
        let row = crate::panels::sketch_row_of(&self.view(), document, constraint);
        if let (Some(row), Ok(windows)) = (row, self.windows.try_borrow())
            && let Some(w) = windows.get(&document)
        {
            w.begin_dimension_edit(row);
        }
    }

    fn open_window(&self, document: DocumentId) {
        if self.windows.try_borrow().map(|w| w.contains_key(&document)).unwrap_or(true) {
            return;
        }
        let window = DocWindow::open(self.mtm, self.me.clone(), document, &self.options, self.locale);
        if let Ok(mut windows) = self.windows.try_borrow_mut() {
            windows.insert(document, window);
        }
        self.canvas_resized(document);
        self.refresh();
        self.redraw(document);
    }

    fn refresh(&self) {
        let vm = self.view();
        let (machine, camera, laser) = match self.controller.try_borrow() {
            Ok(c) => {
                let laser: BTreeMap<DocumentId, Option<burnburn_presentation::LaserViewModel>> = vm
                    .documents
                    .iter()
                    .filter(|d| self.laser_shown_in(d.id))
                    .map(|d| (d.id, c.laser_view_for(Some(d.id))))
                    .collect();
                (c.machine_view(), c.camera_view(), laser)
            }
            Err(_) => (None, None, BTreeMap::new()),
        };
        let tool_shortcuts = self.tool_shortcuts();
        if let Ok(mut windows) = self.windows.try_borrow_mut() {
            for (document, w) in windows.iter_mut() {
                w.strip.set_shortcuts(tool_shortcuts.clone());
                w.refresh(&vm, *document, machine.as_ref(), camera.as_ref());
                if w.laser.shown {
                    w.refresh_laser(laser.get(document).and_then(Option::as_ref));
                }
            }
        }
        self.refresh_cam_panels();
        self.refresh_camera_window();
        self.refresh_app_panels();
    }

    /// Draw the document's scene into its canvas (no-op without a surface or scene).
    pub(crate) fn redraw(&self, document: DocumentId) {
        let Some(scene) = self.controller.try_borrow().ok().and_then(|c| c.scene(document)) else { return };
        let wireframe = self.view().document(document).is_some_and(|d| d.wireframe);
        let packet = super::canvas::canvas_packet_with(&scene, wireframe, self.display_tolerance_px.get());
        let Ok(windows) = self.windows.try_borrow() else { return };
        let Some(w) = windows.get(&document) else { return };
        if let Some(Err(e)) = w.controller.with_canvas(|canvas| canvas.draw(&packet)) {
            eprintln!("burnburn: canvas: {e}");
        }
    }

    fn show_save_panel(&self, document: DocumentId, suggested_name: &str, kind: SaveKind) {
        let Ok(mut windows) = self.windows.try_borrow_mut() else { return };
        let Some(w) = windows.get_mut(&document) else { return };
        let panel = NSSavePanel::savePanel(self.mtm);
        panel.setNameFieldStringValue(&NSString::from_str(suggested_name));
        set_file_type(&panel, kind.extension());
        let me = self.me.clone();
        let reader = panel.clone();
        let handler = RcBlock::new(move |response: NSModalResponse| {
            super::classes::ffi_guard("save panel completion", (), || {
                let path = (response == crate::adapter::response::OK)
                    .then(|| reader.URL())
                    .flatten()
                    .and_then(|u| u.to_file_path());
                if let Some(app) = me.upgrade() {
                    app.forget_sheets(document);
                    app.native(match kind {
                        SaveKind::Project => NativeEvent::SavePanelFinished { document, path },
                        SaveKind::Rd => NativeEvent::ExportPanelFinished { document, path },
                    });
                }
            });
        });
        let begun = objc_try("showing the save sheet", || {
            panel.beginSheetModalForWindow_completionHandler(&w.window, &handler)
        });
        match begun {
            Ok(()) => match kind {
                SaveKind::Project => w.save_panel = Some(panel),
                SaveKind::Rd => w.export_panel = Some(panel),
            },
            Err(e) => eprintln!("burnburn: {e}"),
        }
    }

    fn show_close_alert(&self, document: DocumentId) {
        let title = window_title(&self.view(), document);
        let locale = self.locale;
        let Ok(mut windows) = self.windows.try_borrow_mut() else { return };
        let Some(w) = windows.get_mut(&document) else { return };
        let alert = NSAlert::new(self.mtm);
        alert.setMessageText(&NSString::from_str(&close_alert_title(&title, locale)));
        alert.setInformativeText(&NSString::from_str(host_text(HostText::CloseAlertInfo, locale)));
        for choice in CLOSE_ALERT_BUTTONS {
            let text = match choice {
                CloseChoice::Save => HostText::CloseSave,
                CloseChoice::Discard => HostText::CloseDiscard,
                CloseChoice::Cancel => HostText::CloseCancel,
            };
            alert.addButtonWithTitle(&NSString::from_str(host_text(text, locale)));
        }
        let me = self.me.clone();
        let handler = RcBlock::new(move |response: NSModalResponse| {
            super::classes::ffi_guard("close alert completion", (), || {
                if let Some(app) = me.upgrade() {
                    app.forget_sheets(document);
                    app.native(NativeEvent::CloseAlertFinished { document, response });
                }
            });
        });
        match objc_try("showing the close sheet", || {
            alert.beginSheetModalForWindow_completionHandler(&w.window, Some(&handler))
        }) {
            Ok(()) => w.close_alert = Some(alert),
            Err(e) => eprintln!("burnburn: {e}"),
        }
    }

    fn forget_sheets(&self, document: DocumentId) {
        if let Ok(mut windows) = self.windows.try_borrow_mut()
            && let Some(w) = windows.get_mut(&document)
        {
            w.save_panel = None;
            w.export_panel = None;
            w.close_alert = None;
        }
    }

    /// Archief › Importeer…: a DXF/SVG for the active document, as a sheet on its window.
    fn show_import_panel(&self) {
        let Some(document) = self.view().active else { return };
        let Some(window) = self.window(document) else { return };
        let panel = NSOpenPanel::openPanel(self.mtm);
        panel.setCanChooseFiles(true);
        panel.setCanChooseDirectories(false);
        panel.setAllowsMultipleSelection(false);
        #[allow(deprecated)]
        panel.setAllowedFileTypes(Some(&NSArray::from_retained_slice(&[
            NSString::from_str("dxf"),
            NSString::from_str("svg"),
            NSString::from_str("plt"),
            NSString::from_str("hpgl"),
            NSString::from_str("hgl"),
            NSString::from_str("pdf"),
            NSString::from_str("ai"),
            NSString::from_str("lbrn2"),
            NSString::from_str("png"),
            NSString::from_str("jpg"),
            NSString::from_str("jpeg"),
            NSString::from_str("bmp"),
            NSString::from_str("tif"),
            NSString::from_str("tiff"),
        ])));
        let me = self.me.clone();
        let reader = panel.clone();
        let handler = RcBlock::new(move |response: NSModalResponse| {
            super::classes::ffi_guard("import panel completion", (), || {
                let path: Option<PathBuf> = (response == crate::adapter::response::OK)
                    .then(|| reader.URLs().firstObject())
                    .flatten()
                    .and_then(|u| u.to_file_path());
                if let Some(app) = me.upgrade() {
                    app.native(NativeEvent::ImportFileChosen { document, path });
                }
            });
        });
        if let Err(e) =
            objc_try("showing the import panel", || panel.beginSheetModalForWindow_completionHandler(&window, &handler))
        {
            eprintln!("burnburn: {e}");
        }
    }

    /// Beeld ▸ Beeld vervangen… / Beeld tracen…: one picture for the active document, as a sheet.
    fn show_image_file_panel(&self, kind: ImageFilePanel) {
        use super::panels::image::{BITMAP_TYPES, TRACE_TYPES, show_image_open_panel};
        let Some(document) = self.view().active else { return };
        let Some(window) = self.window(document) else { return };
        match kind {
            ImageFilePanel::Replace => show_image_open_panel(
                self.mtm,
                &window,
                self.me.clone(),
                &BITMAP_TYPES,
                "replace image panel completion",
                move |path| NativeEvent::ReplaceImageFileChosen { document, path },
            ),
            ImageFilePanel::Trace => show_image_open_panel(
                self.mtm,
                &window,
                self.me.clone(),
                &TRACE_TYPES,
                "trace panel completion",
                move |path| NativeEvent::TraceFileChosen { document, path },
            ),
        }
    }

    /// Beeld ▸ Verwerkte bitmap bewaren…: a PNG name for the active document.
    fn show_bitmap_save_panel(&self) {
        let Some(document) = self.view().active else { return };
        let Some(window) = self.window(document) else { return };
        let name = crate::panels_image::suggested_bitmap_name(&window_title(&self.view(), document));
        super::panels::image::show_bitmap_save_panel(self.mtm, &window, self.me.clone(), &name, move |path| {
            NativeEvent::BitmapSavePanelFinished { document, path }
        });
    }

    fn show_open_panel(&self) {
        let panel = NSOpenPanel::openPanel(self.mtm);
        panel.setCanChooseFiles(true);
        panel.setCanChooseDirectories(false);
        panel.setAllowsMultipleSelection(false);
        set_file_type(&panel, "burnburn");
        let me = self.me.clone();
        let reader = panel.clone();
        let handler = RcBlock::new(move |response: NSModalResponse| {
            super::classes::ffi_guard("open panel completion", (), || {
                let path: Option<PathBuf> = (response == crate::adapter::response::OK)
                    .then(|| reader.URLs().firstObject())
                    .flatten()
                    .and_then(|u| u.to_file_path());
                if let Some(app) = me.upgrade() {
                    if let Ok(mut p) = app.open_panel.try_borrow_mut() {
                        *p = None;
                    }
                    app.native(NativeEvent::OpenPanelFinished { path });
                }
            });
        });
        if let Err(e) = objc_try("showing the open panel", || panel.beginWithCompletionHandler(&handler)) {
            eprintln!("burnburn: {e}");
            return;
        }
        if let Ok(mut p) = self.open_panel.try_borrow_mut() {
            *p = Some(panel);
        }
    }

    // ---- Test and smoke-run accessors (read native state; never bypass presentation) ----------

    /// How quits with dirty documents ended, in order.
    pub fn quit_outcomes(&self) -> Vec<QuitOutcome> {
        self.quit_outcomes.try_borrow().map(|o| o.clone()).unwrap_or_default()
    }

    /// Final texts the canvas's input route produced (and clears the record).
    pub fn take_text_commits(&self) -> Vec<String> {
        self.text_commits.try_borrow_mut().map(|mut t| std::mem::take(&mut *t)).unwrap_or_default()
    }

    /// Content of every text object of `document`, in id order.
    pub fn text_contents(&self, document: DocumentId) -> Vec<String> {
        self.controller.try_borrow().map(|c| c.text_contents(document)).unwrap_or_default()
    }

    /// Documents that have a native window.
    pub fn open_documents(&self) -> Vec<DocumentId> {
        self.windows.try_borrow().map(|w| w.keys().copied().collect()).unwrap_or_default()
    }

    /// The scene the window draws (tests and screenshots).
    pub fn scene(&self, document: DocumentId) -> Option<Arc<burnburn_editor::SceneSnapshot>> {
        self.controller.try_borrow().ok()?.scene(document)
    }

    pub fn with_window<T>(&self, document: DocumentId, f: impl FnOnce(&WindowParts<'_>) -> T) -> Option<T> {
        let windows = self.windows.try_borrow().ok()?;
        let w = windows.get(&document)?;
        Some(f(&WindowParts {
            window: &w.window,
            table: &w.table,
            fields: &w.fields,
            reference_buttons: &w.controls.reference,
            aspect_lock: &w.controls.aspect_lock,
            flip_buttons: &w.controls.flip,
            strip: &w.strip,
            status_label: &w.status_label,
            plan_labels: [
                &w.plan_labels[0],
                &w.plan_labels[1],
                &w.plan_labels[2],
                &w.plan_labels[3],
                &w.plan_labels[4],
            ],
            compile_button: &w.compile_button,
            export_button: &w.export_button,
            machine_labels: [&w.machine_labels[0], &w.machine_labels[1], &w.machine_labels[2], &w.machine_labels[3]],
            machine_buttons: &w.machine_buttons,
            import_sheet: w.import.shown.then_some(&*w.import.sheet),
            import_labels: [&w.import.labels[0], &w.import.labels[1], &w.import.labels[2], &w.import.labels[3]],
            import_units: &w.import.units,
            import_commit: &w.import.commit,
            image_import_sheet: w.image.import.shown.then_some(&*w.image.import.sheet),
            image_import_labels: w.image.import.labels.each_ref().map(|l| &**l),
            image_dpi: &w.image.import.dpi,
            image_commit: &w.image.import.commit,
            adjust_sheet: w.image.adjust.shown.map(|_| &*w.image.adjust.sheet),
            adjust_labels: w.image.adjust.labels.each_ref().map(|l| &**l),
            adjust_fields: w.image.adjust.fields.each_ref().map(|f| &**f),
            adjust_gray: &w.image.adjust.gray,
            adjust_preview: &w.image.adjust.preview,
            adjust_apply: &w.image.adjust.apply,
            tracer_sheet: w.image.tracer.shown.then_some(&*w.image.tracer.sheet),
            tracer_labels: w.image.tracer.labels.each_ref().map(|l| &**l),
            tracer_fields: w.image.tracer.fields.each_ref().map(|f| &**f),
            tracer_mode: &w.image.tracer.mode,
            tracer_recipe: &w.image.tracer.recipe,
            tracer_preview: &w.image.tracer.preview,
            tracer_apply: &w.image.tracer.apply,
            tracer_close: &w.image.tracer.close,
            operations_table: &w.operations.table,
            operation_inputs: [&w.operations.inputs[0], &w.operations.inputs[1], &w.operations.inputs[2]],
            operation_kind: &w.operations.kind,
            operation_add: &w.operations.add,
            operation_error: &w.operations.error,
            palette: &w.operations.palette,
            cut_settings_button: &w.operations.settings,
            cut_sheet: w.cut_settings.shown.map(|_| &*w.cut_settings.sheet),
            cut_title: &w.cut_settings.title,
            cut_mode: &w.cut_settings.mode,
            cut_fields: w.cut_settings.fields.each_ref().map(|f| &**f),
            cut_output: &w.cut_settings.output,
            cut_visible: &w.cut_settings.visible,
            cut_error: &w.cut_settings.error,
            cut_ok: &w.cut_settings.ok,
            cut_cancel: &w.cut_settings.cancel,
            cut_options: w.cut_settings.options.iter().map(|o| o.part()).collect(),
            camera_labels: [&w.camera.labels[0], &w.camera.labels[1], &w.camera.labels[2]],
            camera_devices: &w.camera.devices,
            camera_buttons: [&w.camera.buttons[0], &w.camera.buttons[1], &w.camera.buttons[2]],
            camera_window_button: &w.camera.window_button,
            sketch_panel: &w.sketch.panel,
            sketch_panel_shown: w.sketch.shown,
            sketch_table: &w.sketch.table,
            sketch_freedom: &w.sketch.freedom,
            sketch_problem: &w.sketch.problem,
            sketch_role: &w.sketch.role,
            sketch_params_table: &w.sketch.params_table,
            sketch_param_inputs: [&w.sketch.param_inputs[0], &w.sketch.param_inputs[1]],
            sketch_param_set: &w.sketch.param_set,
            sketch_buttons: &w.sketch.buttons,
            sketch_toolbar: &w.sketch.toolbar,
            shapes_panel_shown: w.shapes.panel_visible(),
            shapes_rows: (0..2 * crate::panels::PARAM_SLOTS).filter_map(|i| w.shapes.param_widgets(i)).collect(),
            geometry_sheet: w.shapes.dialog_visible().then(|| w.shapes.sheet()),
            geometry_field: w.shapes.dialog_widgets().0,
            geometry_ok: w.shapes.dialog_widgets().1,
            geometry_cancel: w.shapes.dialog_widgets().2,
            arrange_toolbar: &w.arrange_tools,
            files: w.files_parts(),
            import_pages: &w.import.pages,
            import_report: &w.import.report,
            canvas_view: w.canvas_view.as_view(),
            save_panel: w.save_panel.as_deref(),
            export_panel: w.export_panel.as_deref(),
            close_alert: w.close_alert.as_deref(),
            canvas_status: &w.canvas_status,
            canvas_frames: w.controller.with_canvas(|c| c.frames()).unwrap_or(0),
            canvas_surfaces: w.controller.with_canvas(|c| c.surfaces_created()).unwrap_or(0),
        }))
    }

    /// The document's window, retained: use it for actions that re-enter the host (sending events,
    /// `performClose:`) instead of acting inside [`Self::with_window`]'s borrow.
    pub fn window(&self, document: DocumentId) -> Option<Retained<objc2_app_kit::NSWindow>> {
        self.windows.try_borrow().ok()?.get(&document).map(|w| w.window.clone())
    }

    pub fn open_panel(&self) -> Option<Retained<NSOpenPanel>> {
        self.open_panel.try_borrow().ok()?.clone()
    }

    /// Take the canvas surface away as the OS would on surface loss, then redraw.
    pub fn simulate_surface_loss(&self, document: DocumentId) {
        if let Ok(windows) = self.windows.try_borrow()
            && let Some(w) = windows.get(&document)
        {
            w.controller.with_canvas(|c| c.simulate_surface_loss());
        }
        self.redraw(document);
    }

    /// The viewport the controller currently uses for `document`.
    pub fn viewport(&self, document: DocumentId) -> Option<Viewport> {
        self.view().document(document).map(|d| d.viewport)
    }
}

impl Drop for HostApp {
    fn drop(&mut self) {
        if let Ok(timers) = self.timers.try_borrow() {
            for t in timers.iter() {
                t.invalidate();
            }
        }
    }
}

/// The picture file panels of the Beeld menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImageFilePanel {
    Replace,
    Trace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SaveKind {
    Project,
    Rd,
}

impl SaveKind {
    fn extension(self) -> &'static str {
        match self {
            SaveKind::Project => "burnburn",
            SaveKind::Rd => "rd",
        }
    }
}

/// Borrowed native parts of one window, for tests.
pub struct WindowParts<'a> {
    pub window: &'a objc2_app_kit::NSWindow,
    pub table: &'a objc2_app_kit::NSTableView,
    pub fields: &'a [(burnburn_presentation::FieldKey, Retained<objc2_app_kit::NSTextField>)],
    /// The 3×3 reference-point buttons (row by row), the aspect-lock checkbox and the two flip buttons.
    pub reference_buttons: &'a [Retained<objc2_app_kit::NSButton>],
    pub aspect_lock: &'a objc2_app_kit::NSButton,
    pub flip_buttons: &'a [Retained<objc2_app_kit::NSButton>],
    pub strip: &'a super::toolstrip::ToolStrip,
    pub status_label: &'a objc2_app_kit::NSTextField,
    /// Preview panel: title, plan id, estimate, state, findings.
    pub plan_labels: [&'a objc2_app_kit::NSTextField; 5],
    pub compile_button: &'a objc2_app_kit::NSButton,
    pub export_button: &'a objc2_app_kit::NSButton,
    /// Machine section: status, message, progress, job.
    pub machine_labels: [&'a objc2_app_kit::NSTextField; 4],
    pub machine_buttons: &'a [Retained<objc2_app_kit::NSButton>],
    /// Import dialog (a sheet) while presentation shows an import preview.
    pub import_sheet: Option<&'a objc2_app_kit::NSWindow>,
    /// Import texts: file, unit prompt, size, report.
    pub import_labels: [&'a objc2_app_kit::NSTextField; 4],
    pub import_units: &'a objc2_app_kit::NSPopUpButton,
    pub import_commit: &'a objc2_app_kit::NSButton,
    /// Bitmap import sheet (W2-L1-beeld) while presentation shows an image import.
    pub image_import_sheet: Option<&'a objc2_app_kit::NSWindow>,
    /// File, pixels, DPI caption, proposal, size, issue.
    pub image_import_labels: [&'a objc2_app_kit::NSTextField; 6],
    pub image_dpi: &'a objc2_app_kit::NSTextField,
    pub image_commit: &'a objc2_app_kit::NSButton,
    /// Adjust Image sheet: title, source size, issue, computing; the nine fields; gray pop-up,
    /// preview and Pas toe.
    pub adjust_sheet: Option<&'a objc2_app_kit::NSWindow>,
    pub adjust_labels: [&'a objc2_app_kit::NSTextField; 4],
    pub adjust_fields: [&'a objc2_app_kit::NSTextField; 9],
    pub adjust_gray: &'a objc2_app_kit::NSPopUpButton,
    pub adjust_preview: &'a objc2_app_kit::NSImageView,
    pub adjust_apply: &'a objc2_app_kit::NSButton,
    /// Tracer sheet: source, summary, computing, scale hint, issue; the four fields; mode and
    /// recipe pop-ups, preview, Pas trace toe and Sluit.
    pub tracer_sheet: Option<&'a objc2_app_kit::NSWindow>,
    pub tracer_labels: [&'a objc2_app_kit::NSTextField; 5],
    pub tracer_fields: [&'a objc2_app_kit::NSTextField; 4],
    pub tracer_mode: &'a objc2_app_kit::NSPopUpButton,
    pub tracer_recipe: &'a objc2_app_kit::NSPopUpButton,
    pub tracer_preview: &'a objc2_app_kit::NSImageView,
    pub tracer_apply: &'a objc2_app_kit::NSButton,
    pub tracer_close: &'a objc2_app_kit::NSButton,
    /// Operations panel: table, new-operation inputs (name, speed, power), kind, Add, message.
    pub operations_table: &'a objc2_app_kit::NSTableView,
    pub operation_inputs: [&'a objc2_app_kit::NSTextField; 3],
    pub operation_kind: &'a objc2_app_kit::NSPopUpButton,
    pub operation_add: &'a objc2_app_kit::NSButton,
    pub operation_error: &'a objc2_app_kit::NSTextField,
    /// Layer palette: one button per colour (tag = slot), then Cut Settings… for the selected row.
    pub palette: &'a [Retained<objc2_app_kit::NSButton>],
    pub cut_settings_button: &'a objc2_app_kit::NSButton,
    /// The Cut Settings sheet while presentation has it open.
    pub cut_sheet: Option<&'a objc2_app_kit::NSWindow>,
    pub cut_title: &'a objc2_app_kit::NSTextField,
    pub cut_mode: &'a objc2_app_kit::NSPopUpButton,
    /// Speed, min power, max power, passes, kerf.
    pub cut_fields: [&'a objc2_app_kit::NSTextField; 5],
    pub cut_output: &'a objc2_app_kit::NSButton,
    pub cut_visible: &'a objc2_app_kit::NSButton,
    pub cut_error: &'a objc2_app_kit::NSTextField,
    pub cut_ok: &'a objc2_app_kit::NSButton,
    pub cut_cancel: &'a objc2_app_kit::NSButton,
    /// The option controls of the sheet (tabs, leads, fill and image options), in sheet order.
    pub cut_options: Vec<super::panels::cut_settings::CutOptionPart<'a>>,
    /// Bed camera: status, frame, note; device pop-up; request access, start, stop.
    pub camera_labels: [&'a objc2_app_kit::NSTextField; 3],
    pub camera_devices: &'a objc2_app_kit::NSPopUpButton,
    pub camera_buttons: [&'a objc2_app_kit::NSButton; 3],
    /// Opens the camera window (W3-UI-camera).
    pub camera_window_button: &'a objc2_app_kit::NSButton,
    /// Sketch panel (shown while the window is in sketch mode): panel, constraint table, freedom
    /// label, conflict button, Meten/Sturen, parameter table and fields, command buttons.
    pub sketch_panel: &'a objc2_app_kit::NSPanel,
    pub sketch_panel_shown: bool,
    pub sketch_table: &'a objc2_app_kit::NSTableView,
    pub sketch_freedom: &'a objc2_app_kit::NSTextField,
    pub sketch_problem: &'a objc2_app_kit::NSButton,
    pub sketch_role: &'a objc2_app_kit::NSButton,
    pub sketch_params_table: &'a objc2_app_kit::NSTableView,
    pub sketch_param_inputs: [&'a objc2_app_kit::NSTextField; 2],
    pub sketch_param_set: &'a objc2_app_kit::NSButton,
    pub sketch_buttons: &'a [(burnburn_presentation::CommandKey, Retained<objc2_app_kit::NSButton>)],
    pub sketch_toolbar: &'a objc2_app_kit::NSSegmentedControl,
    /// The shape panel (parameters of the selected basic shape, then the tool's options) is up.
    pub shapes_panel_shown: bool,
    /// Its rows, shape section first then tool section: caption, text field, checkbox.
    pub shapes_rows: Vec<(&'a objc2_app_kit::NSTextField, &'a objc2_app_kit::NSTextField, &'a objc2_app_kit::NSButton)>,
    /// The geometry dialog (join, round corners, optimize) while presentation has it open.
    pub geometry_sheet: Option<&'a objc2_app_kit::NSWindow>,
    pub geometry_field: &'a objc2_app_kit::NSTextField,
    pub geometry_ok: &'a objc2_app_kit::NSButton,
    pub geometry_cancel: &'a objc2_app_kit::NSButton,
    /// The toolbar's Uitlijnen control.
    pub arrange_toolbar: &'a objc2_app_kit::NSSegmentedControl,
    /// The Material and Art Library panels, the bundle sheet and the export dialog.
    pub files: super::panels::files::FilesParts<'a>,
    /// The import sheet's page pop-up (PDF/AI) and its full report.
    pub import_pages: &'a objc2_app_kit::NSPopUpButton,
    pub import_report: &'a objc2_app_kit::NSTextView,
    pub canvas_view: &'a objc2_app_kit::NSView,
    pub save_panel: Option<&'a NSSavePanel>,
    pub export_panel: Option<&'a NSSavePanel>,
    pub close_alert: Option<&'a NSAlert>,
    pub canvas_status: &'a CanvasStatus,
    pub canvas_frames: u64,
    pub canvas_surfaces: u64,
}

impl WindowParts<'_> {
    /// The inspector field for `key`.
    pub fn field(&self, key: burnburn_presentation::FieldKey) -> Option<&objc2_app_kit::NSTextField> {
        self.fields.iter().find(|(k, _)| *k == key).map(|(_, f)| &**f)
    }
}

#[allow(deprecated)]
fn set_file_type(panel: &NSSavePanel, extension: &str) {
    // `allowedContentTypes` needs UniformTypeIdentifiers bindings; the file-type list is enough
    // for one extension and still honoured by AppKit.
    let types = NSArray::from_retained_slice(&[NSString::from_str(extension)]);
    panel.setAllowedFileTypes(Some(&types));
}

/// The menu bar and the one menu that changes while the app runs.
struct MenuBar {
    main: Retained<NSMenu>,
    /// Archief ▸ Recente projecten, rebuilt from the controller's list.
    recent: Retained<NSMenu>,
    /// Archief ▸ Recente templates, rebuilt from the controller's list.
    recent_templates: Retained<NSMenu>,
}

/// Modifier masks of the key equivalents.
const CMD: NSEventModifierFlags = NSEventModifierFlags::Command;
const CMD_SHIFT: NSEventModifierFlags =
    NSEventModifierFlags(NSEventModifierFlags::Command.0 | NSEventModifierFlags::Shift.0);
const CMD_OPT: NSEventModifierFlags =
    NSEventModifierFlags(NSEventModifierFlags::Command.0 | NSEventModifierFlags::Option.0);

/// A menu item that sends `burnburnMenu:` to `target` with the tag of `menu_item`.
#[allow(unsafe_code)]
fn command_item(mtm: MainThreadMarker, target: &MenuTarget, menu_item: MenuItem, title: &str) -> Retained<NSMenuItem> {
    let target: &AnyObject = target.as_ref();
    // SAFETY: a freshly allocated item; the action is `burnburnMenu:` (`v@:@`) on our target.
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            Some(sel!(burnburnMenu:)),
            &NSString::from_str(""),
        )
    };
    item.setTag(menu_tag(menu_item));
    // SAFETY: the target is weak; HostApp keeps it for the app's lifetime.
    unsafe { item.setTarget(Some(target)) };
    item
}

/// Fill Archief ▸ Recente projecten: one entry per stored project (most recent first), a
/// separator and Wis menu. Titles are file names; the tooltip is the full path.
fn fill_recent_menu(mtm: MainThreadMarker, target: &MenuTarget, menu: &NSMenu, paths: &[PathBuf], locale: Locale) {
    menu.removeAllItems();
    for (index, path) in paths.iter().enumerate() {
        let item = command_item(mtm, target, MenuItem::Recent(index), &crate::recent::recent_title(path));
        item.setToolTip(Some(&NSString::from_str(&path.to_string_lossy())));
        menu.addItem(&item);
    }
    if !paths.is_empty() {
        menu.addItem(&NSMenuItem::separatorItem(mtm));
    }
    menu.addItem(&command_item(mtm, target, MenuItem::ClearRecent, host_text(HostText::ClearRecent, locale)));
}

/// Fill Archief ▸ Recente templates like the recent projects: file names, tooltip with the path.
fn fill_recent_templates_menu(
    mtm: MainThreadMarker,
    target: &MenuTarget,
    menu: &NSMenu,
    paths: &[PathBuf],
    locale: Locale,
) {
    menu.removeAllItems();
    for (index, path) in paths.iter().enumerate() {
        let item = command_item(mtm, target, MenuItem::RecentTemplate(index), &crate::recent::recent_title(path));
        item.setToolTip(Some(&NSString::from_str(&path.to_string_lossy())));
        menu.addItem(&item);
    }
    if !paths.is_empty() {
        menu.addItem(&NSMenuItem::separatorItem(mtm));
    }
    menu.addItem(&command_item(mtm, target, MenuItem::ClearRecentTemplates, host_text(HostText::ClearRecent, locale)));
}

/// App menu, File, Edit, Arrange, Tools, View, Sketch, Production. Every item sends
/// `burnburnMenu:` to `target` with its tag, except Cut / Copy / Paste / Paste in Place / Select
/// All: those go through the responder chain (nil target), so a focused text field keeps normal
/// text editing and the canvas handles them otherwise. Titles, enablement and check marks come
/// from the view model in `validateMenuItem:`, the menu headings from the catalogue in `locale`.
#[allow(unsafe_code)]
fn main_menu(mtm: MainThreadMarker, target: &MenuTarget, locale: Locale) -> MenuBar {
    let target_object: &AnyObject = target.as_ref();
    let t = |text: HostText| host_text(text, locale);
    let raw_item = |title: &str, action: Option<objc2::runtime::Sel>, key: &str| {
        // SAFETY: a freshly allocated item; the action is `burnburnMenu:` (`v@:@`) on our target,
        // or an AppKit action sent through the responder chain (nil target).
        unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str(title),
                action,
                &NSString::from_str(key),
            )
        }
    };
    let empty_vm =
        AppViewModel { documents: Vec::new(), active: None, commands: BTreeMap::new(), toolbar: Default::default() };
    let keyed = |menu_item: MenuItem, key: &str, mask: NSEventModifierFlags| {
        let title = crate::adapter::menu_title(menu_item, &empty_vm, locale);
        let item = raw_item(&title, Some(sel!(burnburnMenu:)), key);
        item.setTag(menu_tag(menu_item));
        item.setKeyEquivalentModifierMask(mask);
        // SAFETY: the target is weak; HostApp keeps it for the app's lifetime.
        unsafe { item.setTarget(Some(target_object)) };
        item
    };
    let item = |menu_item: MenuItem, key: &str, shift: bool| keyed(menu_item, key, if shift { CMD_SHIFT } else { CMD });
    // An Edit item through the responder chain: no target, so the first responder that handles
    // the action (a field editor, or the canvas) gets it.
    let chain = |command: CommandKey, action: objc2::runtime::Sel, key: &str, mask: NSEventModifierFlags| {
        let item = raw_item(&crate::adapter::message_text(MessageKey::Command(command), locale), Some(action), key);
        item.setKeyEquivalentModifierMask(mask);
        item
    };
    let submenu = |title: &str, items: Vec<Retained<NSMenuItem>>| {
        let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(title));
        for i in &items {
            menu.addItem(i);
        }
        let holder = raw_item(title, None, "");
        holder.setSubmenu(Some(&menu));
        holder
    };
    let sep = || NSMenuItem::separatorItem(mtm);
    let c = MenuItem::Command;
    let plain = |key: CommandKey| item(c(key), "", false);
    let main = NSMenu::new(mtm);
    main.addItem(&app_host::application_menu(mtm, target, locale));

    let recent = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(t(HostText::RecentProjects)));
    fill_recent_menu(mtm, target, &recent, &[], locale);
    let recent_holder = raw_item(t(HostText::RecentProjects), None, "");
    recent_holder.setSubmenu(Some(&recent));
    let files_t = |text: crate::l10n_files::FilesText| crate::l10n_files::files_text(text, locale);
    let recent_templates = NSMenu::initWithTitle(
        NSMenu::alloc(mtm),
        &NSString::from_str(files_t(crate::l10n_files::FilesText::MenuNewFromRecentTemplate)),
    );
    fill_recent_templates_menu(mtm, target, &recent_templates, &[], locale);
    let recent_templates_holder = raw_item(files_t(crate::l10n_files::FilesText::MenuNewFromRecentTemplate), None, "");
    recent_templates_holder.setSubmenu(Some(&recent_templates));
    main.addItem(&submenu(
        t(HostText::MenuFile),
        vec![
            item(c(CommandKey::New), "n", false),
            plain(CommandKey::NewFromTemplate),
            recent_templates_holder,
            item(MenuItem::Open, "o", false),
            recent_holder,
            item(MenuItem::Import, "i", true),
            submenu(
                files_t(crate::l10n_files::FilesText::MenuExportSubmenu),
                vec![plain(CommandKey::ExportSvg), plain(CommandKey::ExportDxf)],
            ),
            submenu(
                files_t(crate::l10n_files::FilesText::MenuBundleSubmenu),
                vec![plain(CommandKey::ExportBundle), plain(CommandKey::ImportBundle)],
            ),
            sep(),
            item(c(CommandKey::Close), "w", false),
            item(c(CommandKey::Save), "s", false),
            item(c(CommandKey::SaveAs), "s", true),
            plain(CommandKey::SaveAsTemplate),
        ],
    ));
    main.addItem(&submenu(
        t(HostText::MenuEdit),
        vec![
            item(c(CommandKey::Undo), "z", false),
            item(c(CommandKey::Redo), "z", true),
            sep(),
            chain(CommandKey::Cut, sel!(cut:), "x", CMD),
            chain(CommandKey::Copy, sel!(copy:), "c", CMD),
            chain(CommandKey::Paste, sel!(paste:), "v", CMD),
            chain(CommandKey::PasteInPlace, sel!(pasteInPlace:), "v", CMD_SHIFT),
            item(c(CommandKey::Duplicate), "d", false),
            plain(CommandKey::Delete),
            sep(),
            // Responder chain: a text field keeps its own Select All (see CanvasView::selectAll:).
            chain(CommandKey::SelectAll, sel!(selectAll:), "a", CMD),
            keyed(c(CommandKey::InvertSelection), "i", CMD_OPT),
            plain(CommandKey::SelectOpenShapes),
            plain(CommandKey::SelectInLayer),
            plain(CommandKey::SelectContained),
            plain(CommandKey::SelectSmaller),
            plain(CommandKey::SelectCircles),
            plain(CommandKey::SelectSameType),
            sep(),
            plain(CommandKey::DeleteDuplicates),
            sep(),
            item(MenuItem::CamDialog(CamDialog::Optimization), "", false),
            item(MenuItem::CamDialog(CamDialog::Device), "", false),
        ],
    ));
    main.addItem(&submenu(
        t(HostText::MenuArrange),
        vec![
            item(c(CommandKey::Group), "g", false),
            item(c(CommandKey::Ungroup), "g", true),
            plain(CommandKey::AutoGroup),
            plain(CommandKey::BreakApart),
            sep(),
            plain(CommandKey::FlipHorizontal),
            plain(CommandKey::FlipVertical),
            plain(CommandKey::Rotate90Cw),
            plain(CommandKey::Rotate90Ccw),
            sep(),
            submenu(
                t(HostText::MenuAlign),
                [
                    CommandKey::AlignLeft,
                    CommandKey::AlignCenterX,
                    CommandKey::AlignRight,
                    CommandKey::AlignTop,
                    CommandKey::AlignCenterY,
                    CommandKey::AlignBottom,
                ]
                .into_iter()
                .map(plain)
                .collect(),
            ),
            submenu(
                t(HostText::MenuDistribute),
                [CommandKey::DistributeHorizontal, CommandKey::DistributeVertical].into_iter().map(plain).collect(),
            ),
            submenu(
                &crate::panels::schikken_text(crate::panels::SchikkenText::MenuDock, locale),
                [CommandKey::DockLeft, CommandKey::DockRight, CommandKey::DockUp, CommandKey::DockDown]
                    .into_iter()
                    .map(plain)
                    .collect(),
            ),
            submenu(
                &crate::panels::schikken_text(crate::panels::SchikkenText::MenuMoveTogether, locale),
                [CommandKey::MoveTogetherHorizontal, CommandKey::MoveTogetherVertical].into_iter().map(plain).collect(),
            ),
            sep(),
            plain(CommandKey::QuickNest),
            plain(CommandKey::NestSelected),
            item(MenuItem::SchikkenPanel, "", false),
            sep(),
            item(c(CommandKey::PushForward), "]", false),
            item(c(CommandKey::PushBackward), "[", false),
            item(c(CommandKey::PushToFront), "]", true),
            item(c(CommandKey::PushToBack), "[", true),
            sep(),
            plain(CommandKey::LockSelected),
            plain(CommandKey::UnlockSelected),
            plain(CommandKey::UnlockAll),
            plain(CommandKey::HideSelected),
            plain(CommandKey::IsolateSelection),
            plain(CommandKey::ShowAll),
        ],
    ));
    main.addItem(&submenu(
        t(HostText::MenuTools),
        vec![
            plain(CommandKey::ToolSelect),
            plain(CommandKey::ToolRectangle),
            plain(CommandKey::ToolEllipse),
            plain(CommandKey::ToolLine),
            plain(CommandKey::ToolPen),
            plain(CommandKey::ToolNode),
            plain(CommandKey::ToolTrim),
            plain(CommandKey::ToolWarp),
            plain(CommandKey::ToolDeform),
            plain(CommandKey::ResizeSlots),
            sep(),
            plain(CommandKey::BooleanUnion),
            plain(CommandKey::BooleanDifference),
            plain(CommandKey::BooleanIntersection),
            sep(),
            plain(CommandKey::SnapToObjects),
            plain(CommandKey::SnapToGrid),
            sep(),
            plain(CommandKey::Measure),
        ],
    ));
    main.addItem(&submenu(
        t(HostText::MenuView),
        vec![
            item(c(CommandKey::ZoomPage), "0", false),
            item(c(CommandKey::ZoomSelection), "0", true),
            item(c(CommandKey::ZoomIn), "=", false),
            item(c(CommandKey::ZoomOut), "-", false),
            sep(),
            plain(CommandKey::ViewWireframe),
        ],
    ));
    main.addItem(&submenu(
        t(HostText::MenuShapes),
        vec![
            item(c(CommandKey::ShapesJoin), "", false),
            item(c(CommandKey::ShapesCut), "", false),
            item(c(CommandKey::ShapesRoundCorners), "", false),
            item(c(CommandKey::ShapesOptimize), "", false),
            sep(),
            item(c(CommandKey::ShapesReverse), "", false),
            item(c(CommandKey::ShapesClockwise), "", false),
            item(c(CommandKey::ShapesCounterClockwise), "", false),
            sep(),
            item(c(CommandKey::ShapesConvertToPath), "", false),
            sep(),
            item(c(CommandKey::NodeDelete), "", false),
            item(c(CommandKey::NodeBreak), "", false),
            item(c(CommandKey::NodeSmooth), "", false),
            item(c(CommandKey::NodeSymmetric), "", false),
            item(c(CommandKey::NodeCorner), "", false),
            item(c(CommandKey::NodeToLine), "", false),
            item(c(CommandKey::NodeToCurve), "", false),
            item(c(CommandKey::NodeClosePath), "", false),
            item(c(CommandKey::NodeJoinEnds), "", false),
            item(c(CommandKey::NodeStartPoint), "", false),
        ],
    ));
    main.addItem(&submenu(
        crate::panels::text_text(crate::panels::TextText::MenuText, locale),
        vec![
            plain(CommandKey::TextOnPath),
            plain(CommandKey::ShapesConvertToPath),
            sep(),
            plain(CommandKey::BarcodeNew),
        ],
    ));
    main.addItem(&submenu(
        t(HostText::MenuSketch),
        vec![
            plain(CommandKey::SketchNew),
            plain(CommandKey::SketchFromSelection),
            plain(CommandKey::SketchEdit),
            plain(CommandKey::SketchFinish),
            sep(),
            plain(CommandKey::SketchCoincident),
            plain(CommandKey::SketchHorizontal),
            plain(CommandKey::SketchVertical),
            plain(CommandKey::SketchParallel),
            plain(CommandKey::SketchPerpendicular),
            plain(CommandKey::SketchEqual),
            plain(CommandKey::SketchTangent),
            plain(CommandKey::SketchFix),
            sep(),
            plain(CommandKey::SketchDimension),
            plain(CommandKey::SketchRemoveConstraint),
        ],
    ));
    main.addItem(&submenu(
        t(HostText::ImageMenu),
        vec![
            item(MenuItem::ImportImage, "", false),
            item(c(CommandKey::AdjustImage), "", false),
            item(c(CommandKey::MaskImage), "", false),
            sep(),
            item(c(CommandKey::ReplaceImage), "", false),
            item(c(CommandKey::RefreshImage), "", false),
            item(c(CommandKey::SaveProcessedBitmap), "", false),
            sep(),
            item(MenuItem::TraceImage, "", false),
            item(c(CommandKey::TraceEdit), "", false),
        ],
    ));
    main.addItem(&submenu(
        t(HostText::MenuProduction),
        vec![item(c(CommandKey::Compile), "b", false), item(c(CommandKey::ExportRd), "e", true)],
    ));
    main.addItem(&submenu(
        crate::panels::laser_text(crate::panels::LaserText::MenuLaser, locale),
        vec![
            item(MenuItem::LaserWindow, "l", true),
            sep(),
            item(MenuItem::CamDialog(CamDialog::MaterialTest), "", false),
            item(MenuItem::CamDialog(CamDialog::IntervalTest), "", false),
            item(MenuItem::CamDialog(CamDialog::FocusTest), "", false),
            plain(CommandKey::PrintAndCut),
            plain(CommandKey::CenterFinder),
        ],
    ));
    main.addItem(&app_host::window_menu(
        mtm,
        target,
        locale,
        vec![plain(CommandKey::ShowMaterialLibrary), plain(CommandKey::ShowArtLibrary)],
    ));
    main.addItem(&app_host::help_menu(mtm, target, locale));
    debug_assert!(menu_items().iter().all(|i| menu_tag(*i) >= 1));
    MenuBar { main, recent, recent_templates }
}
