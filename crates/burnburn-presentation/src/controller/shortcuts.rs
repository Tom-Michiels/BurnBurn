//! Shortcut editor model (X02): one register of everything that can have a key — every command,
//! every catalogue tool and the host's own actions — with defaults, the user's overrides,
//! conflict detection and reset. The menu, the tool tooltips and the canvas key route all read the
//! effective bindings from here; a binding is never kept in a second place.

use std::collections::BTreeMap;

use burnburn_editor::ToolKind;
use burnburn_types::DocumentId;

use crate::catalog::{Action, ToolCatalog, ToolState, enablement};
use crate::production::Role;
use crate::{CommandKey, UiIntent};

use super::{AppController, AppUpdate};

/// A key with modifiers. `key` is one character (lower case) or a name: `Tab`, `Return`,
/// `Space`, `Left`, `Right`, `Up`, `Down`, `Delete`, `Escape`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyBinding {
    pub key: String,
    pub command: bool,
    pub control: bool,
    pub option: bool,
    pub shift: bool,
}

/// Key names beyond a single character, in their canonical spelling.
const NAMED_KEYS: &[&str] = &[
    "Tab", "Return", "Space", "Left", "Right", "Up", "Down", "Delete", "Escape", "F1", "F2", "F3", "F4", "F5", "F6",
    "F7", "F8", "F9", "F10", "F11", "F12",
];

/// Other spellings people type.
const KEY_ALIASES: &[(&str, &str)] = &[("enter", "Return"), ("esc", "Escape"), ("backspace", "Delete")];

impl KeyBinding {
    /// `Cmd+Shift+Z`, `Opt+P`, `Shift+F`, `V`. Modifiers in any order and case; `Alt` is `Opt`.
    pub fn parse(text: &str) -> Option<KeyBinding> {
        let text = text.trim();
        // "Cmd++" and "+" name the plus key itself.
        let (modifiers, key) = if let Some(head) = text.strip_suffix("++") {
            (head, "+")
        } else if text == "+" {
            ("", "+")
        } else {
            match text.rsplit_once('+') {
                Some((head, key)) => (head, key),
                None => ("", text),
            }
        };
        let key = Self::canonical_key(key)?;
        let mut binding = KeyBinding { key, command: false, control: false, option: false, shift: false };
        if !modifiers.is_empty() {
            for token in modifiers.split('+') {
                let flag = match token.trim().to_ascii_lowercase().as_str() {
                    "cmd" | "command" => &mut binding.command,
                    "ctrl" | "control" => &mut binding.control,
                    "opt" | "option" | "alt" => &mut binding.option,
                    "shift" => &mut binding.shift,
                    _ => return None,
                };
                if std::mem::replace(flag, true) {
                    return None;
                }
            }
        }
        Some(binding)
    }

    fn canonical_key(token: &str) -> Option<String> {
        let token = token.trim();
        let mut chars = token.chars();
        match (chars.next(), chars.next()) {
            (None, _) => None,
            (Some(c), None) if !c.is_control() && !c.is_whitespace() => Some(c.to_lowercase().collect()),
            _ => {
                let lower = token.to_ascii_lowercase();
                let lower = KEY_ALIASES.iter().find(|(a, _)| *a == lower).map_or(lower.as_str(), |(_, n)| n);
                NAMED_KEYS.iter().find(|n| n.eq_ignore_ascii_case(lower)).map(|n| (*n).to_owned())
            }
        }
    }

    /// The canonical text: modifiers `Cmd`, `Ctrl`, `Opt`, `Shift` in that order, then the key.
    pub fn display(&self) -> String {
        let mut out = String::new();
        for (on, name) in [(self.command, "Cmd"), (self.control, "Ctrl"), (self.option, "Opt"), (self.shift, "Shift")] {
            if on {
                out.push_str(name);
                out.push('+');
            }
        }
        if self.key.chars().count() == 1 {
            out.extend(self.key.chars().flat_map(char::to_uppercase));
        } else {
            out.push_str(&self.key);
        }
        out
    }

    /// Cmd, Ctrl or Opt is held (a plain or Shift-only key may be typed text).
    pub fn has_command_modifier(&self) -> bool {
        self.command || self.control || self.option
    }
}

/// What a register entry does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortcutTarget {
    Command(CommandKey),
    /// A catalogue tool by id.
    Tool(&'static str),
    /// A host action (open, import, preferences, …) by stable id.
    Host(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortcutProblem {
    /// The text is not a key combination.
    Unparsable,
    /// The system or the canvas owns this key (quit, hide, Esc, Return, …).
    Reserved,
    /// A command needs Cmd, Ctrl or Opt; only tools may use a plain key.
    NeedsModifier,
    /// Another entry has it; assigning with `replace` takes it over.
    Conflict {
        with: String,
    },
    UnknownId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutError {
    pub id: String,
    pub problem: ShortcutProblem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutRow {
    /// Stable id: `cmd.<CommandKey>`, `tool.<catalogue id>` or `host.<name>`.
    pub id: String,
    pub target: ShortcutTarget,
    pub binding: Option<KeyBinding>,
    pub default: Option<KeyBinding>,
    /// The user changed it (including "no shortcut").
    pub customized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ShortcutsViewModel {
    pub rows: Vec<ShortcutRow>,
    pub error: Option<ShortcutError>,
}

impl ShortcutsViewModel {
    pub fn row(&self, id: &str) -> Option<&ShortcutRow> {
        self.rows.iter().find(|r| r.id == id)
    }
}

/// The value of an override that means "no shortcut".
pub(super) const NO_SHORTCUT: &str = "none";

/// Defaults of the menu commands. Hosts apply the effective bindings to their menus; an item whose
/// command is not listed here keeps whatever key the host gave it and is not editable.
const COMMAND_DEFAULTS: &[(CommandKey, &str)] = &[
    (CommandKey::New, "Cmd+N"),
    (CommandKey::Close, "Cmd+W"),
    (CommandKey::Save, "Cmd+S"),
    (CommandKey::SaveAs, "Cmd+Shift+S"),
    (CommandKey::Undo, "Cmd+Z"),
    (CommandKey::Redo, "Cmd+Shift+Z"),
    (CommandKey::Cut, "Cmd+X"),
    (CommandKey::Copy, "Cmd+C"),
    (CommandKey::Paste, "Cmd+V"),
    (CommandKey::PasteInPlace, "Cmd+Shift+V"),
    (CommandKey::Duplicate, "Cmd+D"),
    (CommandKey::SelectAll, "Cmd+A"),
    (CommandKey::InvertSelection, "Cmd+Opt+I"),
    (CommandKey::Group, "Cmd+G"),
    (CommandKey::Ungroup, "Cmd+Shift+G"),
    (CommandKey::PushForward, "Cmd+]"),
    (CommandKey::PushBackward, "Cmd+["),
    (CommandKey::PushToFront, "Cmd+Shift+]"),
    (CommandKey::PushToBack, "Cmd+Shift+["),
    (CommandKey::ZoomPage, "Cmd+0"),
    (CommandKey::ZoomSelection, "Cmd+Shift+0"),
    (CommandKey::ZoomIn, "Cmd+="),
    (CommandKey::ZoomOut, "Cmd+-"),
    (CommandKey::Compile, "Cmd+B"),
    (CommandKey::ExportRd, "Cmd+Shift+E"),
];

/// Host actions (menu items that are not presentation commands): stable ids and defaults.
pub const HOST_ACTIONS: &[(&str, Option<&str>)] = &[
    ("open", Some("Cmd+O")),
    ("import", Some("Cmd+Shift+I")),
    ("import_image", None),
    ("trace_image", None),
    ("laser_window", Some("Cmd+Shift+L")),
    ("export_diagnostics", None),
    ("preferences", Some("Cmd+,")),
    ("shortcuts", None),
    ("history", Some("Cmd+Opt+Y")),
    ("preview", Some("Cmd+Opt+P")),
    ("inspect_rd", None),
    ("about", None),
    ("manual", None),
    ("support_bundle", None),
    ("debug_log", None),
];

/// Keys the system or the canvas own: never assignable.
fn is_reserved(b: &KeyBinding) -> bool {
    let plain_reserved = ["Escape", "Return", "Delete", "Tab", "Space"];
    if !b.has_command_modifier() && !b.shift && plain_reserved.contains(&b.key.as_str()) {
        return true;
    }
    // The system's: quit, hide, minimise, app switcher, Spotlight, log out.
    let cmd_only = b.command && !b.control && !b.option && !b.shift;
    (cmd_only && ["q", "h", "m", "Tab", "Space"].contains(&b.key.as_str()))
        || (b.command && b.option && !b.control && !b.shift && b.key == "h")
        || (b.command && b.shift && !b.control && !b.option && b.key == "q")
        || (b.command && b.control && !b.option && !b.shift && b.key == "q")
}

struct Entry {
    id: String,
    target: ShortcutTarget,
    default: Option<KeyBinding>,
}

fn entries() -> Vec<Entry> {
    let mut out = Vec::new();
    for key in CommandKey::ALL {
        let default = COMMAND_DEFAULTS.iter().find(|(k, _)| k == key).and_then(|(_, text)| KeyBinding::parse(text));
        out.push(Entry { id: format!("cmd.{key:?}"), target: ShortcutTarget::Command(*key), default });
    }
    // A catalogue entry that is a command is listed as that command, once.
    for spec in ToolCatalog::standard().specs() {
        if matches!(spec.action, Action::Command(_)) {
            continue;
        }
        let default = spec.shortcut.and_then(KeyBinding::parse);
        out.push(Entry { id: format!("tool.{}", spec.id), target: ShortcutTarget::Tool(spec.id), default });
    }
    for (name, default) in HOST_ACTIONS {
        out.push(Entry {
            id: format!("host.{name}"),
            target: ShortcutTarget::Host(name),
            default: default.and_then(KeyBinding::parse),
        });
    }
    out
}

/// The binding in force for an entry: the user's choice, else the default.
fn effective(overrides: &BTreeMap<String, String>, entry: &Entry) -> Option<KeyBinding> {
    match overrides.get(&entry.id).map(String::as_str) {
        None => entry.default.clone(),
        Some(NO_SHORTCUT) => None,
        Some(text) => KeyBinding::parse(text).or_else(|| entry.default.clone()),
    }
}

impl AppController {
    pub fn shortcuts_view(&self) -> ShortcutsViewModel {
        let overrides = &self.app.settings.shortcuts;
        let rows = entries()
            .into_iter()
            .map(|e| ShortcutRow {
                binding: effective(overrides, &e),
                customized: overrides.contains_key(&e.id),
                id: e.id,
                target: e.target,
                default: e.default,
            })
            .collect();
        ShortcutsViewModel { rows, error: self.app.shortcut_error.clone() }
    }

    /// What pressing `binding` in `document`'s canvas does: the intent to dispatch, or `None`
    /// when the key is nothing of ours (it then goes on to text input). A plain key is text while
    /// the Text tool is active; Cmd, Ctrl and Opt combinations always count.
    pub fn shortcut_intent(&self, document: DocumentId, binding: &KeyBinding) -> Option<UiIntent> {
        let s = self.sessions.get(&document)?;
        let overrides = &self.app.settings.shortcuts;
        let entry = entries().into_iter().find(|e| effective(overrides, e).as_ref() == Some(binding))?;
        match entry.target {
            ShortcutTarget::Command(command) => Some(UiIntent::Invoke { document: Some(document), command }),
            ShortcutTarget::Tool(id) => {
                if !binding.has_command_modifier() && s.editor.tool() == ToolKind::Text {
                    return None;
                }
                let spec = ToolCatalog::standard().get(id)?;
                let state = ToolState {
                    document: true,
                    operator: self.role == Role::Operator,
                    in_sketch: s.sketch.active.is_some(),
                };
                enablement(spec, state)
                    .is_enabled()
                    .then(|| UiIntent::SelectTool { document: Some(document), id: id.to_owned() })
            }
            ShortcutTarget::Host(_) => None,
        }
    }

    fn shortcut_failed(&mut self, id: &str, problem: ShortcutProblem) -> AppUpdate {
        self.app.shortcut_error = Some(ShortcutError { id: id.to_owned(), problem });
        AppUpdate { view_changed: true, ..AppUpdate::default() }
    }

    /// Stores (or removes) the override of `id` and writes the user file.
    fn shortcut_changed(&mut self) -> AppUpdate {
        self.app.shortcut_error = None;
        self.app.load_problem = false;
        self.persist_settings()
    }

    pub(super) fn set_shortcut(&mut self, id: &str, binding: KeyBinding, replace: bool) -> AppUpdate {
        let all = entries();
        let Some(entry) = all.iter().find(|e| e.id == id) else {
            return self.shortcut_failed(id, ShortcutProblem::UnknownId);
        };
        if is_reserved(&binding) {
            return self.shortcut_failed(id, ShortcutProblem::Reserved);
        }
        if !binding.has_command_modifier() && !matches!(entry.target, ShortcutTarget::Tool(_)) {
            return self.shortcut_failed(id, ShortcutProblem::NeedsModifier);
        }
        let overrides = &self.app.settings.shortcuts;
        if effective(overrides, entry).as_ref() == Some(&binding) {
            self.app.shortcut_error = None;
            return AppUpdate { view_changed: true, ..AppUpdate::default() };
        }
        let owner = all.iter().find(|e| e.id != id && effective(overrides, e).as_ref() == Some(&binding));
        if let Some(owner) = owner {
            if !replace {
                let with = owner.id.clone();
                return self.shortcut_failed(id, ShortcutProblem::Conflict { with });
            }
            let owner_id = owner.id.clone();
            self.store_override(&all, &owner_id, None);
        }
        self.store_override(&all, id, Some(binding));
        self.shortcut_changed()
    }

    /// `None` = no shortcut. An override equal to the default is not stored.
    fn store_override(&mut self, all: &[Entry], id: &str, binding: Option<KeyBinding>) {
        let default = all.iter().find(|e| e.id == id).and_then(|e| e.default.clone());
        let map = &mut self.app.settings.shortcuts;
        if binding == default {
            map.remove(id);
        } else {
            map.insert(id.to_owned(), binding.map_or_else(|| NO_SHORTCUT.to_owned(), |b| b.display()));
        }
    }

    pub(super) fn clear_shortcut(&mut self, id: &str) -> AppUpdate {
        let all = entries();
        if !all.iter().any(|e| e.id == id) {
            return self.shortcut_failed(id, ShortcutProblem::UnknownId);
        }
        self.store_override(&all, id, None);
        self.shortcut_changed()
    }

    pub(super) fn reset_shortcut(&mut self, id: &str) -> AppUpdate {
        let all = entries();
        let Some(entry) = all.iter().find(|e| e.id == id) else {
            return self.shortcut_failed(id, ShortcutProblem::UnknownId);
        };
        let overrides = &self.app.settings.shortcuts;
        if let Some(default) = &entry.default
            && let Some(owner) = all.iter().find(|e| e.id != id && effective(overrides, e).as_ref() == Some(default))
        {
            let with = owner.id.clone();
            return self.shortcut_failed(id, ShortcutProblem::Conflict { with });
        }
        self.app.settings.shortcuts.remove(id);
        self.shortcut_changed()
    }

    pub(super) fn reset_all_shortcuts(&mut self) -> AppUpdate {
        // Overrides of ids this version does not know belong to a newer build: they stay.
        let known: Vec<String> = entries().into_iter().map(|e| e.id).collect();
        self.app.settings.shortcuts.retain(|id, _| !known.contains(id));
        self.shortcut_changed()
    }
}

/// Whether the user file's text of a shortcut override is readable.
pub(super) fn override_is_valid(text: &str) -> bool {
    text == NO_SHORTCUT || KeyBinding::parse(text).is_some()
}
