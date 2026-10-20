//! Native AppKit events, reduced to plain data, to `burnburn-presentation` intents — without AppKit.
//!
//! The AppKit layer (`appkit`) only reads widget state into a [`NativeEvent`] and performs the
//! [`Reaction`]. Everything that decides *what* an event means lives here and is tested headlessly
//! on every platform. The host never parses field text (DR-05): raw text goes to presentation.

use std::path::PathBuf;
use std::time::Duration;

use burnburn_editor::{CanvasKey, Modifiers, PointerButton, PointerId, PointerPhase, Viewport};
use burnburn_presentation::{
    AppViewModel, CameraIntent, CloseChoice, CommandKey, CutSettingsInput, DialogKind, FieldKey, MessageKey,
    OperationField, SelectMode, UiIntent, ViewPointerEvent,
};
use burnburn_types::{DocumentId, MonotonicTime};

use crate::l10n::{HostText, Locale, host_text, message_or_key};

/// A main-menu item. `Open` is not a presentation command: it first needs a path from NSOpenPanel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuItem {
    Command(CommandKey),
    Open,
    /// Archief ▸ Importeer…: first a file from NSOpenPanel, then presentation's import preview.
    Import,
    /// Archief ▸ Recente projecten ▸ the `n`-th entry (0 = most recent). Dynamic: not in [`menu_items`].
    Recent(usize),
    /// Archief ▸ Recente projecten ▸ Wis menu.
    ClearRecent,
}

/// Every menu item the host offers: Open and every presentation command.
pub fn menu_items() -> Vec<MenuItem> {
    [MenuItem::Open, MenuItem::Import, MenuItem::ClearRecent]
        .into_iter()
        .chain(CommandKey::ALL.iter().map(|k| MenuItem::Command(*k)))
        .collect()
}

/// Tag of Open; commands use 1 + their index in `CommandKey::ALL` (0 is AppKit's default tag).
const OPEN_TAG: isize = 1000;
const IMPORT_TAG: isize = 1001;
const CLEAR_RECENT_TAG: isize = 1002;
/// Recent entry `n` has tag `RECENT_TAG_BASE + n`; the menu never shows more than `RECENT_TAG_SLOTS`.
const RECENT_TAG_BASE: isize = 2000;
const RECENT_TAG_SLOTS: isize = 64;

/// The `NSMenuItem.tag` of `item`.
pub fn menu_tag(item: MenuItem) -> isize {
    match item {
        MenuItem::Open => OPEN_TAG,
        MenuItem::Import => IMPORT_TAG,
        MenuItem::ClearRecent => CLEAR_RECENT_TAG,
        MenuItem::Recent(n) => {
            isize::try_from(n).ok().filter(|n| *n < RECENT_TAG_SLOTS).map_or(-1, |n| RECENT_TAG_BASE + n)
        }
        MenuItem::Command(key) => CommandKey::ALL.iter().position(|k| *k == key).map_or(-1, |i| i as isize + 1),
    }
}

/// The item a tag stands for; unknown tags are no item.
pub fn menu_item(tag: isize) -> Option<MenuItem> {
    if tag == OPEN_TAG {
        return Some(MenuItem::Open);
    }
    if tag == IMPORT_TAG {
        return Some(MenuItem::Import);
    }
    if tag == CLEAR_RECENT_TAG {
        return Some(MenuItem::ClearRecent);
    }
    if (RECENT_TAG_BASE..RECENT_TAG_BASE + RECENT_TAG_SLOTS).contains(&tag) {
        return usize::try_from(tag - RECENT_TAG_BASE).ok().map(MenuItem::Recent);
    }
    let index = usize::try_from(tag.checked_sub(1)?).ok()?;
    CommandKey::ALL.get(index).map(|k| MenuItem::Command(*k))
}

/// Mouse phases AppKit reports to the canvas view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MousePhase {
    Down,
    Dragged,
    Up,
    /// Mouse moved without a button (hover).
    Moved,
}

/// Which physical button an AppKit mouse event came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Other,
}

/// How a text field ended: Return (action) or end of editing (Tab, click elsewhere).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitCause {
    Return,
    EndEditing,
}

/// One AppKit event as plain data. Positions are logical canvas points, origin top-left, y down
/// (the canvas view is flipped); `flags` are raw `NSEventModifierFlags` bits.
#[derive(Debug, Clone, PartialEq)]
pub enum NativeEvent {
    FieldCommitted {
        document: DocumentId,
        field: FieldKey,
        text: String,
        cause: CommitCause,
    },
    Menu(MenuItem),
    /// NSOpenPanel closed: a path on OK, `None` on cancel.
    OpenPanelFinished {
        path: Option<PathBuf>,
    },
    /// NSSavePanel sheet closed: a path on OK, `None` on cancel.
    SavePanelFinished {
        document: DocumentId,
        path: Option<PathBuf>,
    },
    /// Close button, Cmd+W or `performClose:` (`windowShouldClose:`).
    CloseRequested {
        document: DocumentId,
    },
    /// The "save changes?" sheet closed with this `NSModalResponse`.
    CloseAlertFinished {
        document: DocumentId,
        response: isize,
    },
    /// The object table's selected rows changed.
    TableSelectionChanged {
        document: DocumentId,
        rows: Vec<usize>,
    },
    WindowBecameKey {
        document: DocumentId,
    },
    Mouse {
        document: DocumentId,
        phase: MousePhase,
        button: MouseButton,
        location_pt: [f64; 2],
        flags: usize,
        click_count: isize,
        timestamp_s: f64,
    },
    Scroll {
        document: DocumentId,
        delta: [f64; 2],
        precise: bool,
        flags: usize,
        timestamp_s: f64,
    },
    Magnify {
        document: DocumentId,
        magnification: f64,
        location_pt: [f64; 2],
        timestamp_s: f64,
    },
    Resized {
        document: DocumentId,
        width_pt: f64,
        height_pt: f64,
        backing_scale: f64,
    },
    KeyDown {
        document: DocumentId,
        key_code: u16,
        flags: usize,
    },
    /// A tool button of the strip, by catalog id.
    ToolButton {
        document: DocumentId,
        id: String,
    },
    /// An entry of a variant drop-down.
    VariantChosen {
        document: DocumentId,
        group: String,
        id: String,
    },
    /// The strip's icons must be drawn again (light/dark or backing scale changed). No intent.
    ToolStripRestyle {
        document: DocumentId,
    },
    /// A toolbar/panel button that runs a command for this window's document.
    PanelCommand {
        document: DocumentId,
        command: CommandKey,
    },
    /// The RD export save sheet closed: a path on OK, `None` on cancel.
    ExportPanelFinished {
        document: DocumentId,
        path: Option<PathBuf>,
    },
    /// Final text from the canvas's input route (`insertText:` or accepted composition).
    TextCommitted {
        document: DocumentId,
        text: String,
    },
    /// The import file panel closed: a path on OK, `None` on cancel.
    ImportFileChosen {
        document: DocumentId,
        path: Option<PathBuf>,
    },
    /// The import preview's unit pop-up: item `index` of [`crate::panels::IMPORT_UNITS`].
    ImportUnitChosen {
        document: DocumentId,
        index: isize,
    },
    /// The operations panel's Add button with the typed name, kind pop-up item (of
    /// [`crate::panels::OPERATION_KINDS`]), speed and power text.
    AddOperationRequested {
        document: DocumentId,
        name: String,
        kind_index: isize,
        speed: String,
        power: String,
    },
    /// An operations-table cell edited (Return or end of editing), raw text.
    OperationFieldCommitted {
        document: DocumentId,
        row: usize,
        field: OperationField,
        text: String,
    },
    /// A colour of the layer palette (`tag` = slot) clicked, with the event's modifier flags
    /// (Shift adds the colour as a second treatment).
    PaletteClicked {
        document: DocumentId,
        tag: isize,
        flags: usize,
    },
    /// A checkbox of the layer table (Output or Visible) clicked.
    LayerToggled {
        document: DocumentId,
        row: usize,
        column: crate::panels::LayerColumn,
        on: bool,
    },
    /// A layer row double-clicked, or Cut Settings… with this row selected (−1 = none).
    LayerOpened {
        document: DocumentId,
        row: isize,
    },
    /// OK in the Cut Settings sheet: mode pop-up item (of [`crate::panels::SHEET_MODES`]), the
    /// fields' raw text and both checkboxes.
    CutSettingsConfirmed {
        document: DocumentId,
        mode_index: isize,
        speed: String,
        power_min: String,
        power_max: String,
        passes: String,
        kerf: String,
        output: bool,
        visible: bool,
    },
    CutSettingsCancelled {
        document: DocumentId,
    },
    /// The sketch panel's constraint table: these rows are selected now.
    SketchRowsSelected {
        document: DocumentId,
        rows: Vec<usize>,
    },
    /// A value cell of the sketch panel edited (Return or end of editing), raw text.
    SketchValueCommitted {
        document: DocumentId,
        row: usize,
        text: String,
    },
    /// The sketch panel's Meten/Sturen button.
    SketchRoleRequested {
        document: DocumentId,
    },
    /// The conflict message of the sketch panel was clicked.
    SketchProblemClicked {
        document: DocumentId,
    },
    /// A parameter set in the sketch panel: name and expression as typed.
    SketchParameterCommitted {
        document: DocumentId,
        name: String,
        text: String,
    },
    /// The recovery alert closed with this `NSModalResponse` (buttons in
    /// [`crate::lifecycle::RECOVERY_ALERT_BUTTONS`] order).
    RecoveryAnswered {
        path: PathBuf,
        response: isize,
    },
}

/// What the AppKit layer must do with one event.
#[derive(Debug, Clone, PartialEq)]
pub enum Reaction {
    Nothing,
    Intent(UiIntent),
    /// Show NSOpenPanel; its result comes back as [`NativeEvent::OpenPanelFinished`].
    ShowOpenPanel,
    /// Show NSOpenPanel for a DXF/SVG; its result comes back as [`NativeEvent::ImportFileChosen`].
    ShowImportPanel,
}

/// `NSEventModifierFlags` bits (stable AppKit ABI values).
pub mod flags {
    pub const SHIFT: usize = 1 << 17;
    pub const CONTROL: usize = 1 << 18;
    pub const OPTION: usize = 1 << 19;
    pub const COMMAND: usize = 1 << 20;
}

/// AppKit `NSModalResponse` values the host sees from sheets.
pub mod response {
    pub const OK: isize = 1;
    pub const CANCEL: isize = 0;
    /// First, second and third alert button (`NSAlertFirstButtonReturn` …).
    pub const FIRST_BUTTON: isize = 1000;
    pub const SECOND_BUTTON: isize = 1001;
    pub const THIRD_BUTTON: isize = 1002;
}

/// Virtual key codes (`kVK_*`, US layout independent for these keys).
pub mod key_code {
    pub const ESCAPE: u16 = 53;
    pub const DELETE: u16 = 51;
    pub const FORWARD_DELETE: u16 = 117;
    pub const RETURN: u16 = 36;
}

/// Points per line for non-precise (mouse wheel) scroll deltas.
pub const LINE_SCROLL_PT: f64 = 10.0;

/// Buttons of the close sheet, in the order the host adds them (macOS convention).
pub const CLOSE_ALERT_BUTTONS: [CloseChoice; 3] = [CloseChoice::Save, CloseChoice::Cancel, CloseChoice::Discard];

/// Normalised modifiers: Cmd is the shortcut modifier on macOS.
pub fn modifiers(raw: usize) -> Modifiers {
    Modifiers {
        shift: raw & flags::SHIFT != 0,
        alt: raw & flags::OPTION != 0,
        command: raw & flags::COMMAND != 0,
        control: raw & flags::CONTROL != 0,
    }
}

fn time(seconds: f64) -> MonotonicTime {
    MonotonicTime::from_offset(Duration::try_from_secs_f64(seconds).unwrap_or(Duration::ZERO))
}

fn finite2(p: [f64; 2]) -> bool {
    p[0].is_finite() && p[1].is_finite()
}

/// Translates one native event, given the view model the window currently shows.
pub fn translate(event: NativeEvent, vm: &AppViewModel) -> Reaction {
    use NativeEvent as E;
    // Late callbacks of a closed window reach nothing.
    let open = |d: DocumentId| vm.document(d).is_some();
    let intent = match event {
        E::Menu(MenuItem::Open) => return Reaction::ShowOpenPanel,
        E::Menu(MenuItem::Import) => return Reaction::ShowImportPanel,
        E::Menu(MenuItem::ClearRecent) => UiIntent::ClearRecentProjects,
        // A recent entry needs the stored list: see `recent_open_reaction`.
        E::Menu(MenuItem::Recent(_)) => return Reaction::Nothing,
        E::Menu(MenuItem::Command(command)) => UiIntent::Invoke { document: None, command },
        E::OpenPanelFinished { path: Some(path) } => UiIntent::Open { path },
        E::OpenPanelFinished { path: None } => return Reaction::Nothing,
        E::FieldCommitted { document, field, text, .. } => {
            let Some(doc) = vm.document(document) else { return Reaction::Nothing };
            let shown = doc.inspector.as_ref().and_then(|i| i.field(field));
            // Return and the following end of editing both send the action: only changed text
            // is a commit. A field in error keeps its text, so a retry of the same text still goes.
            if shown.is_some_and(|f| f.error.is_none() && f.text == text) {
                return Reaction::Nothing;
            }
            UiIntent::CommitField { document, field, text }
        }
        E::SavePanelFinished { document, path } if open(document) => match path {
            Some(path) => UiIntent::Save { document, path: Some(path) },
            None => UiIntent::DialogCancelled { document, dialog: DialogKind::SavePanel },
        },
        E::CloseRequested { document } if open(document) => UiIntent::Close { document },
        E::CloseAlertFinished { document, response } if open(document) => {
            UiIntent::ConfirmClose { document, choice: close_choice(response) }
        }
        E::TableSelectionChanged { document, rows } => {
            let Some(doc) = vm.document(document) else { return Reaction::Nothing };
            let ids: Vec<_> = rows.iter().filter_map(|r| doc.objects.get(*r).map(|o| o.id)).collect();
            if ids == doc.selection {
                return Reaction::Nothing;
            }
            UiIntent::Select { document, ids, mode: SelectMode::Replace }
        }
        E::WindowBecameKey { document } if open(document) => UiIntent::Focus { document },
        E::Mouse { document, phase, button, location_pt, flags: raw, click_count, timestamp_s } if open(document) => {
            if !finite2(location_pt) {
                return Reaction::Nothing;
            }
            let m = modifiers(raw);
            let button = match (phase, button) {
                (MousePhase::Moved, _) => None,
                // Ctrl-click is the Mac secondary click.
                (_, MouseButton::Left) if m.control => Some(PointerButton::Secondary),
                (_, MouseButton::Left) => Some(PointerButton::Primary),
                (_, MouseButton::Right) => Some(PointerButton::Secondary),
                (_, MouseButton::Other) => Some(PointerButton::Middle),
            };
            let phase = match phase {
                MousePhase::Down => PointerPhase::Down,
                MousePhase::Dragged | MousePhase::Moved => PointerPhase::Move,
                MousePhase::Up => PointerPhase::Up,
            };
            let event = ViewPointerEvent {
                pointer: PointerId(0),
                phase,
                button,
                position_pt: location_pt,
                modifiers: m,
                click_count: u8::try_from(click_count.clamp(0, 255)).unwrap_or(u8::MAX),
                time: time(timestamp_s),
            };
            UiIntent::Pointer { document, event }
        }
        E::Scroll { document, delta, precise, flags: raw, .. } if open(document) => {
            let delta_pt = if precise { delta } else { [delta[0] * LINE_SCROLL_PT, delta[1] * LINE_SCROLL_PT] };
            if !finite2(delta_pt) {
                return Reaction::Nothing;
            }
            if modifiers(raw).command {
                // Cmd + wheel zooms around the view centre's pointer-less anchor: the host passes
                // no location with a scroll, so zoom about the canvas centre.
                let Some(doc) = vm.document(document) else { return Reaction::Nothing };
                let anchor_pt = [doc.viewport.width_pt() / 2.0, doc.viewport.height_pt() / 2.0];
                UiIntent::Camera {
                    document,
                    intent: CameraIntent::Magnify { magnification: delta_pt[1] / 100.0, anchor_pt },
                }
            } else {
                UiIntent::Camera { document, intent: CameraIntent::Scroll { delta_pt } }
            }
        }
        E::Magnify { document, magnification, location_pt, .. } if open(document) => {
            if !(magnification.is_finite() && finite2(location_pt)) {
                return Reaction::Nothing;
            }
            UiIntent::Camera { document, intent: CameraIntent::Magnify { magnification, anchor_pt: location_pt } }
        }
        E::Resized { document, width_pt, height_pt, backing_scale } if open(document) => {
            let Some(viewport) = viewport(width_pt, height_pt, backing_scale) else { return Reaction::Nothing };
            UiIntent::Camera { document, intent: CameraIntent::Resized { viewport } }
        }
        E::KeyDown { document, key_code: code, flags: raw } if open(document) => {
            let key = match code {
                key_code::ESCAPE => CanvasKey::Escape,
                key_code::DELETE => CanvasKey::Backspace,
                key_code::FORWARD_DELETE => CanvasKey::Delete,
                key_code::RETURN => CanvasKey::Return,
                _ => return Reaction::Nothing,
            };
            UiIntent::Key { document, key, modifiers: modifiers(raw) }
        }
        E::ToolButton { document, id } if open(document) => UiIntent::SelectTool { document: Some(document), id },
        E::VariantChosen { document, group, id } if open(document) => {
            UiIntent::SelectVariant { document: Some(document), group, id }
        }
        E::PanelCommand { document, command } if open(document) => {
            UiIntent::Invoke { document: Some(document), command }
        }
        E::ExportPanelFinished { document, path: Some(path) } if open(document) => {
            UiIntent::ExportRd { document, path }
        }
        // Cancelling the export changes nothing (there is no export dialog state in presentation).
        E::ExportPanelFinished { .. } => return Reaction::Nothing,
        // Final text only (composition stays in the host): one commit is one text command.
        E::TextCommitted { document, text } if open(document) && !text.is_empty() => {
            UiIntent::InsertText { document, text }
        }
        E::ImportFileChosen { document, path: Some(path) } if open(document) => UiIntent::Import { document, path },
        E::ImportUnitChosen { document, index } if open(document) => match crate::panels::import_unit(index) {
            Some(unit) => UiIntent::SetImportUnit { document, unit },
            None => return Reaction::Nothing,
        },
        E::AddOperationRequested { document, name, kind_index, speed, power } if open(document) => {
            match crate::panels::operation_kind(kind_index) {
                Some(kind) => UiIntent::AddOperationText { document, name, kind, speed, power },
                None => return Reaction::Nothing,
            }
        }
        E::OperationFieldCommitted { document, row, field, text } => {
            let Some(doc) = vm.document(document) else { return Reaction::Nothing };
            let Some(op) = doc.operations.get(row) else { return Reaction::Nothing };
            let shown = match field {
                OperationField::Name => &op.name,
                OperationField::Speed => &op.speed,
                OperationField::Power => &op.power,
                OperationField::PowerMin => &op.power_min,
                OperationField::PowerMax => &op.power,
                OperationField::Passes => &op.passes,
                OperationField::Kerf => &op.kerf,
                _ => return Reaction::Nothing,
            };
            // As for inspector fields: Return plus end of editing is one commit; a retry after
            // an error still goes.
            if doc.operation_error.is_none() && *shown == text {
                return Reaction::Nothing;
            }
            UiIntent::SetOperationField { document, operation: op.id, field, text }
        }
        E::PaletteClicked { document, tag, flags } if open(document) => match crate::panels::palette_slot(tag) {
            Some(slot) => UiIntent::AssignLayer { document, slot, add: modifiers(flags).shift },
            None => return Reaction::Nothing,
        },
        E::LayerToggled { document, row, column, on } => {
            let Some(op) = vm.document(document).and_then(|d| d.operations.get(row)) else { return Reaction::Nothing };
            match column {
                crate::panels::LayerColumn::Output if op.output_enabled != on => {
                    UiIntent::SetOperationOutput { document, operation: op.id, enabled: on }
                }
                crate::panels::LayerColumn::Visible if op.visible != on => {
                    UiIntent::SetOperationVisible { document, operation: op.id, visible: on }
                }
                _ => return Reaction::Nothing,
            }
        }
        E::LayerOpened { document, row } => {
            let row = usize::try_from(row).ok();
            let Some(op) = row.and_then(|r| vm.document(document)?.operations.get(r)) else { return Reaction::Nothing };
            UiIntent::OpenCutSettings { document, operation: op.id }
        }
        E::CutSettingsConfirmed {
            document,
            mode_index,
            speed,
            power_min,
            power_max,
            passes,
            kerf,
            output,
            visible,
        } => {
            let Some(sheet) = vm.document(document).and_then(|d| d.cut_settings.as_ref()) else {
                return Reaction::Nothing;
            };
            let mode = crate::panels::sheet_mode(mode_index).unwrap_or(sheet.mode);
            UiIntent::ApplyCutSettings {
                document,
                operation: sheet.operation,
                settings: CutSettingsInput {
                    mode,
                    speed,
                    power_min,
                    power_max,
                    passes,
                    kerf,
                    output_enabled: output,
                    visible,
                },
            }
        }
        E::CutSettingsCancelled { document } if vm.document(document).is_some_and(|d| d.cut_settings.is_some()) => {
            UiIntent::CancelCutSettings { document }
        }
        E::SketchRowsSelected { document, rows } => match crate::panels::sketch_rows_intent(vm, document, &rows) {
            Some(i) => i,
            None => return Reaction::Nothing,
        },
        E::SketchValueCommitted { document, row, text } => {
            match crate::panels::sketch_value_intent(vm, document, row, &text) {
                Some(i) => i,
                None => return Reaction::Nothing,
            }
        }
        E::SketchRoleRequested { document } => match crate::panels::sketch_role_intent(vm, document) {
            Some(i) => i,
            None => return Reaction::Nothing,
        },
        E::SketchProblemClicked { document } => match crate::panels::sketch_problem_intent(vm, document) {
            Some(i) => i,
            None => return Reaction::Nothing,
        },
        E::SketchParameterCommitted { document, name, text } => {
            match crate::panels::sketch_parameter_intent(vm, document, &name, &text) {
                Some(i) => i,
                None => return Reaction::Nothing,
            }
        }
        E::RecoveryAnswered { path, response } => match crate::lifecycle::recovery_choice(response) {
            Some(crate::lifecycle::RecoveryChoice::Restore) => UiIntent::RestoreRecovery { path },
            Some(crate::lifecycle::RecoveryChoice::Discard) => UiIntent::DiscardRecovery { path },
            Some(crate::lifecycle::RecoveryChoice::Later) | None => return Reaction::Nothing,
        },
        _ => return Reaction::Nothing,
    };
    Reaction::Intent(intent)
}

/// The close-sheet answer for an alert response; anything unexpected is the safe `Cancel`.
pub fn close_choice(response: isize) -> CloseChoice {
    let index = response.checked_sub(response::FIRST_BUTTON).and_then(|i| usize::try_from(i).ok());
    index.and_then(|i| CLOSE_ALERT_BUTTONS.get(i).copied()).unwrap_or(CloseChoice::Cancel)
}

/// `validateMenuItem:` — enablement comes from the view model (one source for menu and toolbar).
pub fn menu_enabled(item: MenuItem, vm: &AppViewModel) -> bool {
    match item {
        MenuItem::Open => true,
        // Importing needs a document window to import into.
        MenuItem::Import => vm.active.is_some(),
        MenuItem::Recent(_) | MenuItem::ClearRecent => false,
        MenuItem::Command(key) => vm.commands.get(&key).is_some_and(|c| c.enabled),
    }
}

/// As [`menu_enabled`], knowing how many recent projects the menu offers.
pub fn menu_enabled_with_recent(item: MenuItem, vm: &AppViewModel, recent: usize) -> bool {
    match item {
        MenuItem::Recent(n) => n < recent,
        MenuItem::ClearRecent => recent > 0,
        _ => menu_enabled(item, vm),
    }
}

/// What choosing a recent-projects entry does: open that path. `item` must be `Recent(n)` with `n`
/// inside `recent`; anything else (a stale index) does nothing.
pub fn recent_open_reaction(item: MenuItem, recent: &[PathBuf]) -> Reaction {
    match item {
        MenuItem::Recent(n) => match recent.get(n) {
            Some(path) => Reaction::Intent(UiIntent::Open { path: path.clone() }),
            None => Reaction::Nothing,
        },
        _ => Reaction::Nothing,
    }
}

/// Checkmark state (active tool, snapping toggles).
pub fn menu_checked(item: MenuItem, vm: &AppViewModel) -> bool {
    match item {
        MenuItem::Open | MenuItem::Import | MenuItem::Recent(_) | MenuItem::ClearRecent => false,
        MenuItem::Command(key) => vm.commands.get(&key).is_some_and(|c| c.checked),
    }
}

/// Menu title in `locale`, including the undo/redo action name from the view model.
pub fn menu_title(item: MenuItem, vm: &AppViewModel, locale: Locale) -> String {
    match item {
        MenuItem::Open => host_text(HostText::Open, locale).to_owned(),
        MenuItem::Import => host_text(HostText::Import, locale).to_owned(),
        MenuItem::ClearRecent => host_text(HostText::ClearRecent, locale).to_owned(),
        // The entries are titled with their file name when the menu is built (`recent::recent_title`).
        MenuItem::Recent(_) => String::new(),
        MenuItem::Command(key) => match vm.commands.get(&key).map(|c| c.label) {
            Some(label) => message_text(label, locale),
            None => message_text(MessageKey::Command(key), locale),
        },
    }
}

/// Object table rows for `document`: the user-visible names, top to bottom.
pub fn object_labels(vm: &AppViewModel, document: DocumentId) -> Vec<String> {
    vm.document(document).map(|d| d.objects.iter().map(|o| o.label.clone()).collect()).unwrap_or_default()
}

/// Table rows that the view model marks selected.
pub fn selected_rows(vm: &AppViewModel, document: DocumentId) -> Vec<usize> {
    vm.document(document)
        .map(|d| d.objects.iter().enumerate().filter(|(_, o)| o.selected).map(|(i, _)| i).collect())
        .unwrap_or_default()
}

/// User-visible text for a view-model message, from the one catalogue ([`crate::l10n`]).
pub fn message_text(key: MessageKey, locale: Locale) -> String {
    message_or_key(key, locale)
}

/// Window title for `document` ("Naamloos", file name).
pub fn window_title(vm: &AppViewModel, document: DocumentId) -> String {
    vm.document(document).map(|d| d.title.clone()).unwrap_or_default()
}

/// The viewport for a canvas view of `width_pt × height_pt` at `backing_scale`; `None` while the
/// view has no usable size (minimised, not yet laid out).
pub fn viewport(width_pt: f64, height_pt: f64, backing_scale: f64) -> Option<Viewport> {
    Viewport::new(width_pt, height_pt, backing_scale).ok()
}
