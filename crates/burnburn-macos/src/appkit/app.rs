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
    AppController, AppPorts, AppUpdate, AppViewModel, CameraPort, CloseChoice, CommandKey, HostEffect, UiIntent,
};
use burnburn_render::{Theme, build_packet};
use burnburn_types::DocumentId;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly, sel};
use objc2_app_kit::{
    NSAlert, NSApplication, NSApplicationTerminateReply, NSEventModifierFlags, NSMenu, NSMenuItem, NSModalResponse,
    NSOpenPanel, NSSavePanel,
};
use objc2_foundation::{NSArray, NSString, NSTimer};

use super::classes::{AppDelegate, MenuTarget, app_delegate};
use super::objc_try;
use super::window::{CanvasStatus, DocWindow};
use crate::accessibility::{AxElement, canvas_elements};
use crate::adapter::{
    CLOSE_ALERT_BUTTONS, MenuItem, NativeEvent, Reaction, key_code, menu_items, menu_tag, translate, window_title,
};
use crate::l10n::{HostText, Locale, close_alert_title, host_text};
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
            recovery_dir: support.map(|s| crate::lifecycle::recovery_dir(&s)),
            autosave_interval_s: Some(crate::lifecycle::AUTOSAVE_INTERVAL_S),
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
        let app = Rc::new_cyclic(|me: &Weak<HostApp>| {
            let target = MenuTarget::new(mtm);
            target.set_app(me.clone());
            let menu = main_menu(mtm, &target, locale);
            HostApp {
                mtm,
                me: me.clone(),
                controller: RefCell::new(AppController::new(ports)),
                windows: RefCell::new(BTreeMap::new()),
                inbox,
                _target: target,
                _delegate: AppDelegate::new(mtm, me.clone()),
                menu,
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
            }
        });
        if let Ok(mut controller) = app.controller.try_borrow_mut() {
            // Canvas text needs a face: the first readable system font (none: presentation says so).
            if let Some(font) =
                burnburn_runtime::fonts::first_readable_font(&burnburn_runtime::fonts::default_font_candidates())
            {
                controller.attach_fonts(font.source, font.font);
            }
            if let Some(dir) = &app.options.recovery_dir {
                if let Err(e) = std::fs::create_dir_all(dir) {
                    eprintln!("burnburn: recovery folder {}: {e}", dir.display());
                }
                controller.set_recovery_dir(dir.clone());
            }
        }
        if let Some(interval) = app.options.autosave_interval_s {
            app.start_timer(interval, HostApp::autosave_now);
        }
        CURRENT.with(|c| {
            if let Ok(mut c) = c.try_borrow_mut() {
                *c = Rc::downgrade(&app);
            }
        });
        let ns_app = NSApplication::sharedApplication(mtm);
        ns_app.setMainMenu(Some(&app.menu));
        ns_app.setDelegate(Some(app_delegate(&app._delegate)));
        app
    }

    pub fn view(&self) -> Arc<AppViewModel> {
        match self.controller.try_borrow() {
            Ok(c) => c.view(),
            Err(_) => Arc::new(AppViewModel { documents: Vec::new(), active: None, commands: BTreeMap::new() }),
        }
    }

    pub fn menu(&self) -> &NSMenu {
        &self.menu
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

    /// One native event: translated with the current view model, then performed.
    pub fn native(&self, event: NativeEvent) {
        if let NativeEvent::TextCommitted { text, .. } = &event
            && let Ok(mut commits) = self.text_commits.try_borrow_mut()
        {
            commits.push(text.clone());
        }
        match translate(event, &self.view()) {
            Reaction::Nothing => {}
            Reaction::Intent(intent) => {
                self.dispatch(intent);
            }
            Reaction::ShowOpenPanel => self.show_open_panel(),
            Reaction::ShowImportPanel => self.show_import_panel(),
        }
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
        let Some(flow) = QuitFlow::begin(&self.view()) else { return NSApplicationTerminateReply::TerminateNow };
        if let Ok(mut quit) = self.quit.try_borrow_mut() {
            *quit = Some(flow);
        }
        self.advance_quit();
        NSApplicationTerminateReply::TerminateLater
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

    /// A camera button with device pop-up item `device_index` chosen.
    pub(crate) fn camera_button_pressed(&self, control: CameraControl, device_index: isize) {
        let camera = self.controller.try_borrow().ok().and_then(|c| c.camera_view());
        if let Some(intent) = crate::panels::camera_intent(camera.as_ref(), control, device_index) {
            self.dispatch(intent);
            self.refresh();
        }
    }

    /// Copy the camera view model into every window's camera section (frames and permission
    /// answers arrive without an intent).
    fn refresh_camera(&self) {
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
        let (machine, camera) = match self.controller.try_borrow() {
            Ok(c) => (c.machine_view(), c.camera_view()),
            Err(_) => (None, None),
        };
        if let Ok(mut windows) = self.windows.try_borrow_mut() {
            for (document, w) in windows.iter_mut() {
                w.refresh(&vm, *document, machine.as_ref(), camera.as_ref());
            }
        }
    }

    /// Draw the document's scene into its canvas (no-op without a surface or scene).
    pub(crate) fn redraw(&self, document: DocumentId) {
        let Some(scene) = self.controller.try_borrow().ok().and_then(|c| c.scene(document)) else { return };
        let packet = build_packet(&scene, &Theme::light());
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

    pub fn with_window<T>(&self, document: DocumentId, f: impl FnOnce(&WindowParts<'_>) -> T) -> Option<T> {
        let windows = self.windows.try_borrow().ok()?;
        let w = windows.get(&document)?;
        Some(f(&WindowParts {
            window: &w.window,
            table: &w.table,
            fields: &w.fields,
            toolbar_tools: &w.toolbar_tools,
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
            camera_labels: [&w.camera.labels[0], &w.camera.labels[1], &w.camera.labels[2]],
            camera_devices: &w.camera.devices,
            camera_buttons: [&w.camera.buttons[0], &w.camera.buttons[1], &w.camera.buttons[2]],
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
    pub toolbar_tools: &'a objc2_app_kit::NSSegmentedControl,
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
    /// Bed camera: status, frame, note; device pop-up; request access, start, stop.
    pub camera_labels: [&'a objc2_app_kit::NSTextField; 3],
    pub camera_devices: &'a objc2_app_kit::NSPopUpButton,
    pub camera_buttons: [&'a objc2_app_kit::NSButton; 3],
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

/// App menu, File, Edit, Tools. Every item sends `burnburnMenu:` to `target` with its tag; titles
/// and enablement come from the view model in `validateMenuItem:`, the menu headings from the
/// catalogue in `locale`.
#[allow(unsafe_code)]
fn main_menu(mtm: MainThreadMarker, target: &MenuTarget, locale: Locale) -> Retained<NSMenu> {
    let target: &AnyObject = target.as_ref();
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
    let item = |menu_item: MenuItem, key: &str, shift: bool| {
        let title = crate::adapter::menu_title(
            menu_item,
            &AppViewModel { documents: Vec::new(), active: None, commands: BTreeMap::new() },
            locale,
        );
        let item = raw_item(&title, Some(sel!(burnburnMenu:)), key);
        item.setTag(menu_tag(menu_item));
        if shift {
            item.setKeyEquivalentModifierMask(NSEventModifierFlags::Command | NSEventModifierFlags::Shift);
        }
        // SAFETY: the target is weak; HostApp keeps it for the app's lifetime.
        unsafe { item.setTarget(Some(target)) };
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
    let main = NSMenu::new(mtm);
    main.addItem(&submenu(t(HostText::AppMenu), vec![raw_item(t(HostText::Quit), Some(sel!(terminate:)), "q")]));
    main.addItem(&submenu(
        t(HostText::MenuFile),
        vec![
            item(c(CommandKey::New), "n", false),
            item(MenuItem::Open, "o", false),
            item(MenuItem::Import, "i", true),
            sep(),
            item(c(CommandKey::Close), "w", false),
            item(c(CommandKey::Save), "s", false),
            item(c(CommandKey::SaveAs), "s", true),
        ],
    ));
    main.addItem(&submenu(
        t(HostText::MenuEdit),
        vec![
            item(c(CommandKey::Undo), "z", false),
            item(c(CommandKey::Redo), "z", true),
            sep(),
            item(c(CommandKey::Delete), "", false),
            // Responder chain: a text field keeps its own Select All (see CanvasView::selectAll:).
            raw_item(t(HostText::SelectAll), Some(sel!(selectAll:)), "a"),
        ],
    ));
    main.addItem(&submenu(
        t(HostText::MenuTools),
        vec![
            item(c(CommandKey::ToolSelect), "", false),
            item(c(CommandKey::ToolRectangle), "", false),
            item(c(CommandKey::ToolEllipse), "", false),
            item(c(CommandKey::ToolLine), "", false),
            item(c(CommandKey::ToolPen), "", false),
            item(c(CommandKey::ToolNode), "", false),
            sep(),
            item(c(CommandKey::Measure), "", false),
        ],
    ));
    main.addItem(&submenu(
        t(HostText::MenuProduction),
        vec![item(c(CommandKey::Compile), "b", false), item(c(CommandKey::ExportRd), "e", true)],
    ));
    debug_assert!(menu_items().iter().all(|i| menu_tag(*i) >= 1));
    main
}
