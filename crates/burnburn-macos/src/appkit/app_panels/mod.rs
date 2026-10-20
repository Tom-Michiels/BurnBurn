//! The app-level panels of the macOS host (W3-UI-app): preferences, shortcut editor, undo history,
//! preview and RD inspector. `target` holds the Objective-C classes, `windows` the widget layouts;
//! the host's behaviour is in `app/app_host.rs`.

use std::rc::Weak;
use std::time::Instant;

use burnburn_presentation::KeyBinding;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSButton, NSTextField, NSTextView};
use objc2_foundation::NSTimer;

use crate::app_view::PreviewFrame;
use crate::appkit::app::HostApp;

mod target;
pub(crate) mod windows;

pub(crate) use target::AppPanelTarget;
use windows::{HistoryWindow, PrefsWindow, PreviewWindow, RdWindow, ShortcutsWindow};

/// The checkboxes, report text and notice of the support dialog while it is on screen.
pub(crate) struct SupportDialog {
    pub(crate) paths: Retained<NSButton>,
    pub(crate) ids: Retained<NSButton>,
    pub(crate) report: Retained<NSTextView>,
    #[allow(dead_code)]
    pub(crate) note: Retained<NSTextField>,
}

/// Windows and the little UI state behind them. Windows are built the first time they are shown.
pub(crate) struct AppPanels {
    pub(crate) target: Retained<AppPanelTarget>,
    pub(crate) prefs: Option<PrefsWindow>,
    pub(crate) shortcuts: Option<ShortcutsWindow>,
    pub(crate) history: Option<HistoryWindow>,
    pub(crate) preview: Option<PreviewWindow>,
    pub(crate) rd: Option<RdWindow>,
    pub(crate) shortcut_filter: String,
    /// A recorded binding another entry holds: Replace takes it over.
    pub(crate) pending_conflict: Option<(String, KeyBinding)>,
    pub(crate) recording: bool,
    /// The map the preview bitmap was drawn with (a click needs the same one).
    pub(crate) preview_frame: Option<PreviewFrame>,
    pub(crate) preview_timer: Option<Retained<NSTimer>>,
    pub(crate) last_tick: Option<Instant>,
    pub(crate) support: Option<SupportDialog>,
}

impl AppPanels {
    pub(crate) fn new(mtm: MainThreadMarker, app: Weak<HostApp>) -> AppPanels {
        AppPanels {
            target: AppPanelTarget::new(mtm, app),
            prefs: None,
            shortcuts: None,
            history: None,
            preview: None,
            rd: None,
            shortcut_filter: String::new(),
            pending_conflict: None,
            recording: false,
            preview_frame: None,
            preview_timer: None,
            last_tick: None,
            support: None,
        }
    }
}

impl Drop for AppPanels {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        if let Some(timer) = self.preview_timer.take() {
            timer.invalidate();
        }
        // The tables' data source and delegate are weak: clear them before the target goes.
        let tables = [
            self.shortcuts.as_ref().map(|w| &w.table),
            self.history.as_ref().map(|w| &w.table),
            self.rd.as_ref().map(|w| &w.table),
        ];
        for table in tables.into_iter().flatten() {
            // SAFETY: clearing a weak reference to an object that is still alive.
            unsafe { table.setDataSource(None) };
            // SAFETY: as above.
            unsafe { table.setDelegate(None) };
        }
    }
}
