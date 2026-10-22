//! The app-level menu items and panel events (W3-UI-app), without AppKit: which items exist,
//! when they are enabled, and which intent each panel event is. Tested headlessly; the AppKit
//! panels (`appkit::app_panels`) only read widgets into [`AppEvent`]s.

use std::path::PathBuf;

use burnburn_presentation::{
    AppController, AppIntent, AppViewModel, DiagnosticOptions, KeyBinding, NudgeDirection, NudgeStep, PlaybackSpeed,
    PrefKey, PreviewIntent, UiIntent,
};
use burnburn_types::DocumentId;

use crate::adapter::{Reaction, modifiers};
use crate::app_text::{AppText, app_text};
use crate::l10n::Locale;

/// Menu items that open an app-level panel or dialog (the host performs them; none is an intent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppAction {
    About,
    Preferences,
    Shortcuts,
    History,
    /// Venster ▸ Machine en productie…: the machine hub (G5-machine-rest).
    MachineHub,
    Preview,
    InspectRd,
    Manual,
    SupportData,
    DebugLog,
}

/// Tags of the app items: a range of their own (commands use 1.., the fixed items 1000.., the
/// recent projects 2000..).
const TAG_BASE: isize = 3000;

impl AppAction {
    pub const ALL: [AppAction; 10] = [
        AppAction::About,
        AppAction::Preferences,
        AppAction::Shortcuts,
        AppAction::History,
        AppAction::MachineHub,
        AppAction::Preview,
        AppAction::InspectRd,
        AppAction::Manual,
        AppAction::SupportData,
        AppAction::DebugLog,
    ];

    pub fn tag(self) -> isize {
        TAG_BASE + Self::ALL.iter().position(|a| *a == self).map_or(-TAG_BASE - 1, |i| i as isize)
    }

    pub fn from_tag(tag: isize) -> Option<AppAction> {
        Self::ALL.get(usize::try_from(tag.checked_sub(TAG_BASE)?).ok()?).copied()
    }

    pub fn title(self, locale: Locale) -> &'static str {
        let text = match self {
            AppAction::About => AppText::About,
            AppAction::Preferences => AppText::Preferences,
            AppAction::Shortcuts => AppText::Shortcuts,
            AppAction::History => AppText::UndoHistory,
            AppAction::MachineHub => AppText::MachineHub,
            AppAction::Preview => AppText::Preview,
            AppAction::InspectRd => AppText::InspectRd,
            AppAction::Manual => AppText::Manual,
            AppAction::SupportData => AppText::SupportData,
            AppAction::DebugLog => AppText::DebugLog,
        };
        app_text(text, locale)
    }

    /// History needs a window, Preview and RD a window with a plan, the manual a known address.
    pub fn enabled(self, vm: &AppViewModel) -> bool {
        let active = vm.active.and_then(|d| vm.document(d));
        match self {
            AppAction::History => active.is_some(),
            AppAction::Preview | AppAction::InspectRd => active.is_some_and(|d| d.plan.is_some()),
            AppAction::Manual => AppController::help_info().manual_url.is_some(),
            AppAction::About
            | AppAction::Preferences
            | AppAction::Shortcuts
            | AppAction::MachineHub
            | AppAction::SupportData
            | AppAction::DebugLog => true,
        }
    }

    /// The register id of the host action this item stands for (`host.<name>`).
    pub fn shortcut_id(self) -> &'static str {
        match self {
            AppAction::About => "host.about",
            AppAction::Preferences => "host.preferences",
            AppAction::Shortcuts => "host.shortcuts",
            AppAction::History => "host.history",
            AppAction::MachineHub => "host.machine_hub",
            AppAction::Preview => "host.preview",
            AppAction::InspectRd => "host.inspect_rd",
            AppAction::Manual => "host.manual",
            AppAction::SupportData => "host.support_bundle",
            AppAction::DebugLog => "host.debug_log",
        }
    }
}

/// Arrow keys of the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowKey {
    Left,
    Right,
    Up,
    Down,
}

/// The arrow a virtual key code is (`kVK_LeftArrow` …).
pub fn arrow_key(code: u16) -> Option<ArrowKey> {
    match code {
        123 => Some(ArrowKey::Left),
        124 => Some(ArrowKey::Right),
        125 => Some(ArrowKey::Down),
        126 => Some(ArrowKey::Up),
        _ => None,
    }
}

/// What the app panels' widgets report, as plain data.
#[derive(Debug, Clone, PartialEq)]
pub enum AppEvent {
    PrefEdited {
        key: PrefKey,
        text: String,
    },
    PrefsReset,
    ShortcutRecorded {
        id: String,
        binding: KeyBinding,
        replace: bool,
    },
    ShortcutCleared {
        id: String,
    },
    ShortcutReset {
        id: String,
    },
    ShortcutsResetAll,
    /// Row 0 of the history table is the start; row `n` the state after `n` steps.
    HistoryRowClicked {
        document: DocumentId,
        row: usize,
    },
    PreviewSeeked {
        document: DocumentId,
        seconds: f64,
    },
    PreviewPlayToggled {
        document: DocumentId,
        playing: bool,
    },
    PreviewSpeedChosen {
        document: DocumentId,
        index: usize,
    },
    PreviewTravelToggled {
        document: DocumentId,
        show: bool,
    },
    PreviewClicked {
        document: DocumentId,
        at_mm: [f64; 2],
        tolerance_mm: f64,
    },
    PreviewMoveChosen {
        document: DocumentId,
        index: usize,
    },
    RdInspectRequested {
        document: DocumentId,
    },
    SupportDialogFinished {
        options: DiagnosticOptions,
        path: Option<PathBuf>,
    },
    DebugLogSwitched {
        enabled: bool,
    },
    /// An arrow key on the canvas with the event's raw modifier flags.
    ArrowKey {
        document: DocumentId,
        key: ArrowKey,
        flags: usize,
    },
}

/// The reaction to one panel event given the view model the window shows. An event of a window
/// that is gone reaches nothing.
pub fn translate_app(event: AppEvent, vm: &AppViewModel) -> Reaction {
    let open = |d: DocumentId| vm.document(d).is_some();
    let app = |intent| Reaction::Intent(UiIntent::App(intent));
    let preview = |intent| app(AppIntent::Preview(intent));
    match event {
        AppEvent::PrefEdited { key, text } => app(AppIntent::SetPreference { key, text }),
        AppEvent::PrefsReset => app(AppIntent::ResetPreferences),
        AppEvent::ShortcutRecorded { id, binding, replace } => app(AppIntent::SetShortcut { id, binding, replace }),
        AppEvent::ShortcutCleared { id } => app(AppIntent::ClearShortcut { id }),
        AppEvent::ShortcutReset { id } => app(AppIntent::ResetShortcut { id }),
        AppEvent::ShortcutsResetAll => app(AppIntent::ResetAllShortcuts),
        AppEvent::HistoryRowClicked { document, row } if open(document) => {
            app(AppIntent::HistoryJump { document, position: row })
        }
        AppEvent::PreviewSeeked { document, seconds } if open(document) => {
            preview(PreviewIntent::Seek { document, seconds })
        }
        AppEvent::PreviewPlayToggled { document, playing } if open(document) => {
            preview(PreviewIntent::Play { document, playing })
        }
        AppEvent::PreviewSpeedChosen { document, index } if open(document) => match PlaybackSpeed::ALL.get(index) {
            Some(speed) => preview(PreviewIntent::SetSpeed { document, speed: *speed }),
            None => Reaction::Nothing,
        },
        AppEvent::PreviewTravelToggled { document, show } if open(document) => {
            preview(PreviewIntent::ShowTravel { document, show })
        }
        AppEvent::PreviewClicked { document, at_mm, tolerance_mm } if open(document) => {
            preview(PreviewIntent::Pick { document, at_mm, tolerance_mm })
        }
        AppEvent::PreviewMoveChosen { document, index } if open(document) => {
            preview(PreviewIntent::SelectMove { document, index })
        }
        AppEvent::RdInspectRequested { document } if open(document) => preview(PreviewIntent::InspectRd { document }),
        AppEvent::SupportDialogFinished { options, path: Some(path) } => {
            app(AppIntent::ExportSupportBundle { options, path })
        }
        AppEvent::DebugLogSwitched { enabled } => app(AppIntent::SetDebugLog { enabled }),
        AppEvent::ArrowKey { document, key, flags } if open(document) => {
            let m = modifiers(flags);
            // Cmd+arrow belongs to text and AppKit.
            if m.command {
                return Reaction::Nothing;
            }
            let direction = match key {
                ArrowKey::Left => NudgeDirection::Left,
                ArrowKey::Right => NudgeDirection::Right,
                ArrowKey::Up => NudgeDirection::Up,
                ArrowKey::Down => NudgeDirection::Down,
            };
            let step = if m.control {
                NudgeStep::Small
            } else if m.shift {
                NudgeStep::Large
            } else {
                NudgeStep::Normal
            };
            app(AppIntent::Nudge { document, direction, step })
        }
        _ => Reaction::Nothing,
    }
}

/// Virtual key codes of the keys that have a name instead of a character.
const NAMED_CODES: &[(u16, &str)] = &[
    (36, "Return"),
    (48, "Tab"),
    (49, "Space"),
    (51, "Delete"),
    (53, "Escape"),
    (117, "Delete"),
    (123, "Left"),
    (124, "Right"),
    (125, "Down"),
    (126, "Up"),
    (122, "F1"),
    (120, "F2"),
    (99, "F3"),
    (118, "F4"),
    (96, "F5"),
    (97, "F6"),
    (98, "F7"),
    (100, "F8"),
    (101, "F9"),
    (109, "F10"),
    (103, "F11"),
    (111, "F12"),
];

/// The binding a key event stands for: `chars` are the characters ignoring modifiers, `key_code`
/// the virtual key code and `flags` the raw modifier flags. Named keys go by key code; otherwise
/// the one typed character is the key (a shifted letter is the same key).
pub fn binding_from_event(chars: &str, key_code: u16, flags: usize) -> Option<KeyBinding> {
    let m = modifiers(flags);
    let key = match NAMED_CODES.iter().find(|(code, _)| *code == key_code) {
        Some((_, name)) => (*name).to_owned(),
        None => {
            let mut it = chars.chars();
            match (it.next(), it.next()) {
                (Some(c), None) if !c.is_control() && !c.is_whitespace() => c.to_lowercase().collect(),
                _ => return None,
            }
        }
    };
    KeyBinding::parse(&format!(
        "{}{}{}{}{}",
        if m.command { "Cmd+" } else { "" },
        if m.control { "Ctrl+" } else { "" },
        if m.alt { "Opt+" } else { "" },
        if m.shift { "Shift+" } else { "" },
        key
    ))
}

/// A binding as the menus draw it: `⌃⌥⇧⌘` then the key.
pub fn binding_glyphs(binding: &KeyBinding) -> String {
    let mut out = String::new();
    for (on, glyph) in [(binding.control, '⌃'), (binding.option, '⌥'), (binding.shift, '⇧'), (binding.command, '⌘')]
    {
        if on {
            out.push(glyph);
        }
    }
    let key = match binding.key.as_str() {
        "Tab" => "⇥".to_owned(),
        "Return" => "↩".to_owned(),
        "Left" => "←".to_owned(),
        "Right" => "→".to_owned(),
        "Up" => "↑".to_owned(),
        "Down" => "↓".to_owned(),
        "Delete" => "⌫".to_owned(),
        "Escape" => "⎋".to_owned(),
        k if k.chars().count() == 1 => k.to_uppercase(),
        k => k.to_owned(),
    };
    out.push_str(&key);
    out
}
