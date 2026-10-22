//! The app-level menu items and key handling on Windows (W4-L8b), without the `windows` crate.
//!
//! The macOS host has a module of the same name written around `NSEvent` flags and the Mac glyphs.
//! This one has the Windows spelling (`Ctrl+Shift+S`), virtual-key codes and the same presentation
//! intents. `app_view` (shared with the Mac) draws bindings with [`binding_glyphs`] from here.
//!
//! Ctrl is the shortcut modifier on Windows and is stored as the register's `Cmd` flag (the same
//! translation the canvas uses for pointer modifiers). The Mac `Ctrl` flag has no key on Windows:
//! such a binding is shown as `Win+…` and cannot be typed here. Nudges: arrow = normal step,
//! Shift+arrow = large, Alt+arrow = small (the Mac uses Ctrl for small; Ctrl+arrow is text
//! navigation on Windows and is left alone).

use burnburn_presentation::{AppIntent, AppViewModel, HelpInfo, KeyBinding, NudgeDirection, NudgeStep, UiIntent};
use burnburn_types::DocumentId;

use crate::app_text::{AppText, app_text};
use crate::host_panels::app::AppSheet;
use crate::host_panels::ids::menu_base;
use crate::host_panels::spec::KeyStroke;
use crate::host_panels::{Family, HostRequest, PanelReaction};
use crate::l10n::Locale;

/// Menu items that open an app-level panel or dialog (the host performs them; none is an intent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppAction {
    About,
    Preferences,
    Shortcuts,
    History,
    Preview,
    InspectRd,
    Manual,
    SupportData,
    DebugLog,
    /// Venster: Machine en productie (G5-machine-rest).
    MachineHub,
}

impl AppAction {
    pub const ALL: [AppAction; 10] = [
        AppAction::About,
        AppAction::Preferences,
        AppAction::Shortcuts,
        AppAction::History,
        AppAction::Preview,
        AppAction::InspectRd,
        AppAction::Manual,
        AppAction::SupportData,
        AppAction::DebugLog,
        AppAction::MachineHub,
    ];

    /// The `WM_COMMAND` id of this item (inside the app block of the lane's menu ids).
    pub fn id(self) -> u16 {
        let index = Self::ALL.iter().position(|a| *a == self).map_or(0, |i| i as u16);
        menu_base(Family::App) + index
    }

    pub fn from_id(id: u16) -> Option<AppAction> {
        let index = id.checked_sub(menu_base(Family::App))?;
        Self::ALL.get(usize::from(index)).copied()
    }

    pub fn title(self, locale: Locale) -> &'static str {
        let text = match self {
            AppAction::About => AppText::About,
            AppAction::Preferences => AppText::Preferences,
            AppAction::Shortcuts => AppText::Shortcuts,
            AppAction::History => AppText::UndoHistory,
            AppAction::Preview => AppText::Preview,
            AppAction::InspectRd => AppText::InspectRd,
            AppAction::Manual => AppText::Manual,
            AppAction::SupportData => AppText::SupportData,
            AppAction::DebugLog => AppText::DebugLog,
            AppAction::MachineHub => AppText::MachineHub,
        };
        app_text(text, locale)
    }

    /// History needs a window, Preview and RD a window with a plan, the manual a known address.
    pub fn enabled(self, vm: &AppViewModel, manual_known: bool) -> bool {
        let active = vm.active.and_then(|d| vm.document(d));
        match self {
            AppAction::History => active.is_some(),
            AppAction::Preview | AppAction::InspectRd => active.is_some_and(|d| d.plan.is_some()),
            AppAction::Manual => manual_known,
            AppAction::About
            | AppAction::Preferences
            | AppAction::Shortcuts
            | AppAction::SupportData
            | AppAction::DebugLog
            | AppAction::MachineHub => true,
        }
    }

    /// The register id of the host action this item stands for (`host.<name>`).
    pub fn shortcut_id(self) -> &'static str {
        match self {
            AppAction::About => "host.about",
            AppAction::Preferences => "host.preferences",
            AppAction::Shortcuts => "host.shortcuts",
            AppAction::History => "host.history",
            AppAction::Preview => "host.preview",
            AppAction::InspectRd => "host.inspect_rd",
            AppAction::Manual => "host.manual",
            AppAction::SupportData => "host.support_bundle",
            AppAction::DebugLog => "host.debug_log",
            AppAction::MachineHub => "host.machine_hub",
        }
    }
}

/// A binding as the menus and the shortcut editor draw it on Windows.
pub fn binding_glyphs(binding: &KeyBinding) -> String {
    let mut out = String::new();
    for (on, name) in
        [(binding.command, "Ctrl"), (binding.control, "Win"), (binding.option, "Alt"), (binding.shift, "Shift")]
    {
        if on {
            out.push_str(name);
            out.push('+');
        }
    }
    match binding.key.as_str() {
        k if k.chars().count() == 1 => out.push_str(&k.to_uppercase()),
        k => out.push_str(k),
    }
    out
}

/// The name of a virtual key in the register, if it has one.
fn key_name(vk: u16) -> Option<String> {
    let name = match vk {
        0x41..=0x5A => return char::from_u32(u32::from(vk)).map(|c| c.to_ascii_lowercase().to_string()),
        0x30..=0x39 => return char::from_u32(u32::from(vk)).map(|c| c.to_string()),
        0x0D => "Return",
        0x09 => "Tab",
        0x20 => "Space",
        0x08 | 0x2E => "Delete",
        0x1B => "Escape",
        0x25 => "Left",
        0x26 => "Up",
        0x27 => "Right",
        0x28 => "Down",
        0x70..=0x7B => return Some(format!("F{}", vk - 0x70 + 1)),
        _ => return None,
    };
    Some(name.to_owned())
}

/// The binding a key press stands for (`None`: a key without a name in the register).
pub fn binding_from_keystroke(stroke: &KeyStroke) -> Option<KeyBinding> {
    Some(KeyBinding {
        key: key_name(stroke.vk)?,
        command: stroke.ctrl,
        control: false,
        option: stroke.alt,
        shift: stroke.shift,
    })
}

/// An arrow key on the canvas as a nudge of the selection. Ctrl+arrow belongs to text editing.
pub fn nudge_intent(document: DocumentId, vm: &AppViewModel, stroke: &KeyStroke) -> Option<UiIntent> {
    vm.document(document)?;
    if stroke.ctrl {
        return None;
    }
    let direction = match stroke.vk {
        0x25 => NudgeDirection::Left,
        0x27 => NudgeDirection::Right,
        0x26 => NudgeDirection::Up,
        0x28 => NudgeDirection::Down,
        _ => return None,
    };
    let step = if stroke.alt {
        NudgeStep::Small
    } else if stroke.shift {
        NudgeStep::Large
    } else {
        NudgeStep::Normal
    };
    Some(UiIntent::App(AppIntent::Nudge { document, direction, step }))
}

/// What choosing the menu item does.
pub fn action_reaction(action: AppAction, vm: &AppViewModel, help: &HelpInfo, locale: Locale) -> PanelReaction {
    if !action.enabled(vm, help.manual_url.is_some()) {
        return PanelReaction::Nothing;
    }
    let sheet = |s: AppSheet| PanelReaction::Host(HostRequest::ShowSheet(Family::App, s.code()));
    match action {
        AppAction::About => {
            let t = |text| app_text(text, locale);
            PanelReaction::Host(HostRequest::MessageBox {
                title: format!("{} {}", t(AppText::About), help.app_name),
                body: format!(
                    "{} {}\n{}: {} {}\n{}",
                    help.app_name,
                    help.version,
                    t(AppText::AboutPlatform),
                    help.os,
                    help.arch,
                    t(AppText::AboutOffline)
                ),
            })
        }
        AppAction::Preferences => sheet(AppSheet::Prefs),
        AppAction::Shortcuts => sheet(AppSheet::Shortcuts),
        AppAction::History => sheet(AppSheet::History),
        AppAction::Preview => sheet(AppSheet::Preview),
        AppAction::InspectRd => sheet(AppSheet::Rd),
        AppAction::SupportData | AppAction::DebugLog => sheet(AppSheet::Support),
        AppAction::MachineHub => sheet(AppSheet::Hub),
        AppAction::Manual => help
            .manual_url
            .map_or(PanelReaction::Nothing, |url| PanelReaction::Host(HostRequest::OpenUrl(url.to_owned()))),
    }
}
