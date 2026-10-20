//! App-level panels (W3-UI-app): preferences, shortcuts, undo history, preview window, RD
//! inspector, help and support. One intent enum, one state struct on the controller.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use burnburn_document::CommandPayload;
use burnburn_geometry::{Affine, Vector};
use burnburn_types::{CancellationSource, DocumentId, RequestId, RevisionId, TaskId};

use crate::task::{TaskInput, TaskOutcome, TaskRequest, revision_key};

use super::prefs::{PrefError, PrefKey, Preferences, PrefsViewModel, UserSettings};
use crate::production::DiagnosticOptions;

use super::preview::PreviewIntent;
use super::shortcuts::KeyBinding;
use crate::view_model::MessageKey;

use super::{AppController, AppUpdate, apply_edit, error};

/// File name of the user's settings inside the user folder.
pub const USER_FILE: &str = "preferences.conf";
/// Larger user files are not read.
const MAX_USER_FILE_BYTES: u64 = 256 * 1024;

/// Everything the app panels ask of the controller. Window state and user settings only: none of
/// these is a document command, and none ever reaches a machine.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum AppIntent {
    /// Raw text (or a choice id such as `mm`, `on`, `coarse`) typed in the preferences window.
    SetPreference {
        key: PrefKey,
        text: String,
    },
    ResetPreferences,
    /// An arrow key on the canvas: move the selection by the preference distance of `step`.
    Nudge {
        document: DocumentId,
        direction: NudgeDirection,
        step: NudgeStep,
    },
    /// A click on a row of the undo history: go to the state after `position` applied steps
    /// (0 = as opened) by undoing or redoing step by step.
    HistoryJump {
        document: DocumentId,
        position: usize,
    },
    /// Shortcut editor: bind `id` to `binding`. A key another entry has is a conflict unless
    /// `replace` takes it over (the other entry is then left without a shortcut).
    SetShortcut {
        id: String,
        binding: KeyBinding,
        replace: bool,
    },
    /// Shortcut editor: `id` gets no shortcut.
    ClearShortcut {
        id: String,
    },
    /// Shortcut editor: back to the default of `id`.
    ResetShortcut {
        id: String,
    },
    ResetAllShortcuts,
    /// The preview window (play, scrub, pick) and the RD inspector.
    Preview(PreviewIntent),
    /// Help ▸ Support data: write the report the dialog showed to `path` (a save panel's choice).
    ExportSupportBundle {
        options: DiagnosticOptions,
        path: std::path::PathBuf,
    },
    /// Help ▸ Enable debug log.
    SetDebugLog {
        enabled: bool,
    },
    ClearDebugLog,
}

/// Which way an arrow key points on screen (world y is up).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NudgeDirection {
    Left,
    Right,
    Up,
    Down,
}

/// Which preference distance a nudge uses: Ctrl+arrow small, arrow normal, Shift+arrow large.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NudgeStep {
    Small,
    Normal,
    Large,
}

/// State behind the app panels.
#[derive(Debug, Default)]
pub(super) struct AppPanels {
    pub(super) prefs: Preferences,
    /// Where the user file lives; `None`: nothing is read or written (changes last for this run).
    user_dir: Option<PathBuf>,
    /// What else the user file holds (shortcut overrides, keys of a newer version).
    pub(super) settings: UserSettings,
    error: Option<PrefError>,
    pub(super) shortcut_error: Option<super::shortcuts::ShortcutError>,
    pub(super) shortcuts_revision: u64,
    pub(super) load_problem: bool,
    save_failed: bool,
    /// The write in flight, and whether the settings changed again while it ran: writes are
    /// serialized so an older text can never land after a newer one.
    write_in_flight: Option<TaskId>,
    write_again: bool,
    pub(super) debug_log: super::support::DebugLog,
    /// Preview window state and RD listings per window (W3-UI-app).
    pub(super) previews: std::collections::BTreeMap<DocumentId, super::preview::PreviewState>,
    pub(super) rd_listings: std::collections::BTreeMap<DocumentId, super::preview::RdListing>,
}

impl AppController {
    /// Where the user's settings live. Reads them through the injected filesystem; a missing
    /// file is the defaults, a damaged one is the defaults plus a visible problem and the file is
    /// left alone until the user changes something. Call once at start, before the first window.
    pub fn set_user_dir(&mut self, dir: PathBuf) -> AppUpdate {
        let file = dir.join(USER_FILE);
        let _ = self.ports.fs.create_dir_all(&dir);
        let loaded = match self.ports.fs.read(&file, MAX_USER_FILE_BYTES) {
            Ok(bytes) => Some(UserSettings::parse(&bytes)),
            Err(e) if e.kind == burnburn_io::fs::FsErrorKind::NotFound => None,
            Err(_) => Some(super::prefs::Loaded { settings: UserSettings::default(), damaged: true }),
        };
        let app = &mut self.app;
        app.user_dir = Some(dir);
        if let Some(loaded) = loaded {
            app.load_problem = loaded.damaged;
            app.prefs = loaded.settings.prefs;
            app.settings = loaded.settings;
            app.shortcuts_revision += 1;
        }
        AppUpdate { view_changed: true, ..AppUpdate::default() }
    }

    /// The preferences stored in `dir`, read without a controller (defaults for a missing or
    /// damaged file). The host needs the language before it builds its first menu.
    pub fn read_preferences(fs: &dyn burnburn_io::fs::FileSystem, dir: &Path) -> Preferences {
        match fs.read(&dir.join(USER_FILE), MAX_USER_FILE_BYTES) {
            Ok(bytes) => UserSettings::parse(&bytes).settings.prefs,
            Err(_) => Preferences::default(),
        }
    }

    pub fn preferences(&self) -> &Preferences {
        &self.app.prefs
    }

    pub fn prefs_view(&self) -> PrefsViewModel {
        PrefsViewModel {
            rows: PrefKey::ALL.iter().map(|k| self.app.prefs.row(*k)).collect(),
            unit: self.app.prefs.units,
            error: self.app.error,
            load_problem: self.app.load_problem,
            save_failed: self.app.save_failed,
        }
    }

    pub(super) fn app_intent(&mut self, intent: AppIntent) -> AppUpdate {
        match intent {
            AppIntent::SetPreference { key, text } => match self.app.prefs.with(key, &text) {
                Ok(next) => {
                    self.app.error = None;
                    if next == self.app.prefs {
                        return AppUpdate { view_changed: true, ..AppUpdate::default() };
                    }
                    self.app.prefs = next;
                    self.app.load_problem = false;
                    self.persist_settings()
                }
                Err(problem) => {
                    self.app.error = Some(PrefError { key, problem });
                    AppUpdate { view_changed: true, ..AppUpdate::default() }
                }
            },
            AppIntent::ResetPreferences => {
                self.app.error = None;
                self.app.load_problem = false;
                self.app.prefs = Preferences::default();
                self.persist_settings()
            }
            AppIntent::Nudge { document, direction, step } => self.nudge(document, direction, step),
            AppIntent::HistoryJump { document, position } => self.history_jump(document, position),
            AppIntent::SetShortcut { id, binding, replace } => self.set_shortcut(&id, binding, replace),
            AppIntent::ClearShortcut { id } => self.clear_shortcut(&id),
            AppIntent::ResetShortcut { id } => self.reset_shortcut(&id),
            AppIntent::ResetAllShortcuts => self.reset_all_shortcuts(),
            AppIntent::Preview(intent) => self.preview_intent(intent),
            AppIntent::ExportSupportBundle { options, path } => self.export_support_bundle(options, path),
            AppIntent::SetDebugLog { enabled } => self.set_debug_log(enabled),
            AppIntent::ClearDebugLog => self.clear_debug_log(),
        }
    }

    /// Arrow keys: the selection moves by the preference distance, as one undo entry.
    fn nudge(&mut self, document: DocumentId, direction: NudgeDirection, step: NudgeStep) -> AppUpdate {
        let prefs = self.app.prefs;
        let mm = match step {
            NudgeStep::Small => prefs.nudge_small_mm,
            NudgeStep::Normal => prefs.nudge_normal_mm,
            NudgeStep::Large => prefs.nudge_large_mm,
        };
        let (dx, dy) = match direction {
            NudgeDirection::Left => (-mm, 0.0),
            NudgeDirection::Right => (mm, 0.0),
            NudgeDirection::Up => (0.0, mm),
            NudgeDirection::Down => (0.0, -mm),
        };
        let request = self.next_id::<RequestId>();
        self.with_session(document, |s| {
            if s.selection.is_empty() {
                return AppUpdate::default();
            }
            let Ok(transform) = Affine::translate(Vector::new(dx, dy)) else {
                return error(Some(document), MessageKey::CommandRejected);
            };
            let payload = CommandPayload::Transform { targets: s.selection.ids().to_vec(), transform };
            apply_edit(s, document, payload, request)
        })
    }

    /// The text of the user file as it would be written now.
    fn user_text(&mut self) -> String {
        self.app.settings.prefs = self.app.prefs;
        self.app.settings.to_text()
    }

    /// Writes the user file in the background: one write at a time, the latest text last.
    pub(super) fn persist_settings(&mut self) -> AppUpdate {
        let changed = AppUpdate { view_changed: true, ..AppUpdate::default() };
        let Some(dir) = self.app.user_dir.clone() else { return changed };
        if self.app.write_in_flight.is_some() {
            self.app.write_again = true;
            return changed;
        }
        let text = self.user_text();
        match self.submit_user_write(&dir, text) {
            Some(id) => self.app.write_in_flight = Some(id),
            None => self.app.save_failed = true,
        }
        changed
    }

    fn submit_user_write(&mut self, dir: &Path, text: String) -> Option<TaskId> {
        let (document, id) = (self.next_id()?, self.next_id::<TaskId>()?);
        let input =
            TaskInput::WriteText { text: Arc::from(text), path: dir.join(USER_FILE), fs: Arc::clone(&self.ports.fs) };
        let kind = input.kind();
        let source = RevisionId::INITIAL;
        let request = TaskRequest {
            id,
            document,
            source,
            key: revision_key(kind, document, source),
            cancel: CancellationSource::new().token(),
            input,
        };
        self.ports.tasks.submit(request).ok().map(|()| id)
    }

    /// A finished user-file write; `None` when the task is not ours.
    pub(super) fn app_panel_task_finished(&mut self, outcome: &TaskOutcome) -> Option<AppUpdate> {
        if self.app.write_in_flight != Some(outcome.id) {
            return None;
        }
        self.app.write_in_flight = None;
        self.app.save_failed = outcome.result.is_err();
        if std::mem::take(&mut self.app.write_again) {
            let again = self.persist_settings();
            return Some(again);
        }
        Some(AppUpdate { view_changed: true, ..AppUpdate::default() })
    }
}
