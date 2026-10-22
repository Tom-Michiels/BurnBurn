//! Native AppKit events, reduced to plain data, to `burnburn-presentation` intents — without AppKit.
//!
//! The AppKit layer (`appkit`) only reads widget state into a [`NativeEvent`] and performs the
//! [`Reaction`]. Everything that decides *what* an event means lives here and is tested headlessly
//! on every platform. The host never parses field text (DR-05): raw text goes to presentation.

use std::path::PathBuf;
use std::time::Duration;

use burnburn_editor::{CanvasKey, Modifiers, PointerButton, PointerId, PointerPhase, Viewport};
use burnburn_presentation::{
    AppViewModel, CameraIntent, CloseChoice, CommandKey, CutField, CutSettingsInput, CutValue, DialogKind, FieldKey,
    MessageKey, OperationField, SelectMode, UiIntent, ViewPointerEvent,
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
    /// Beeld ▸ Beeld importeren…: the same file panel as Import, with the bitmap types first.
    ImportImage,
    /// Beeld ▸ Beeld tracen…: first an image from NSOpenPanel, then presentation's tracer.
    TraceImage,
    /// Laser ▸ Laservenster: shows or hides the laser panel of the active window.
    LaserWindow,
    /// Help ▸ Diagnose exporteren…: options and a save panel first, then presentation's export.
    ExportDiagnostics,
    /// Archief ▸ Recente projecten ▸ the `n`-th entry (0 = most recent). Dynamic: not in [`menu_items`].
    Recent(usize),
    /// Archief ▸ Recente projecten ▸ Wis menu.
    ClearRecent,
    /// Bewerken / Laser ▸ one of the CAM dialogs (Optimalisatie-instellingen, Materiaal-, Interval-,
    /// Focustest, Apparaatinstellingen): presentation opens it for the active window.
    CamDialog(burnburn_presentation::CamDialog),
    /// Schikken ▸ Schikkenpaneel: shows or hides the options panel of the active window.
    SchikkenPanel,
    /// Venster ▸ Offset en reparatie: shows or hides that panel of the active window (G5-native).
    OffsetPanel,
    /// Venster ▸ Structuur: shows or hides the structure panel of the active window (G5-native).
    StructurePanel,
    /// Beeld ▸ Beeld voorbereiden: shows or hides the image preparation panel (G6-werk).
    TracePrepPanel,
    /// Venster ▸ Operatorweergave: shows or hides the compact operator view (G6-werk).
    OperatorPanel,
    /// Archief ▸ Recente templates ▸ the `n`-th entry (0 = most recent). Dynamic: not in [`menu_items`].
    RecentTemplate(usize),
    /// Archief ▸ Recente templates ▸ Wis menu.
    ClearRecentTemplates,
    /// An app-level panel or dialog (preferences, shortcuts, history, preview, help).
    App(crate::app_menu::AppAction),
}

/// Every menu item the host offers: Open and every presentation command.
pub fn menu_items() -> Vec<MenuItem> {
    [
        MenuItem::Open,
        MenuItem::Import,
        MenuItem::ImportImage,
        MenuItem::TraceImage,
        MenuItem::LaserWindow,
        MenuItem::ExportDiagnostics,
        MenuItem::ClearRecent,
        MenuItem::SchikkenPanel,
        MenuItem::OffsetPanel,
        MenuItem::StructurePanel,
        MenuItem::TracePrepPanel,
        MenuItem::OperatorPanel,
        MenuItem::ClearRecentTemplates,
    ]
    .into_iter()
    .chain(burnburn_presentation::CamDialog::ALL.iter().map(|d| MenuItem::CamDialog(*d)))
    .chain(CommandKey::ALL.iter().map(|k| MenuItem::Command(*k)))
    .chain(crate::app_menu::AppAction::ALL.into_iter().map(MenuItem::App))
    .collect()
}

/// Tag of Open; commands use 1 + their index in `CommandKey::ALL` (0 is AppKit's default tag).
const OPEN_TAG: isize = 1000;
const IMPORT_TAG: isize = 1001;
const IMPORT_IMAGE_TAG: isize = 1002;
const TRACE_IMAGE_TAG: isize = 1003;
const LASER_WINDOW_TAG: isize = 1004;
const DIAGNOSTICS_TAG: isize = 1005;
const CLEAR_RECENT_TAG: isize = 1006;
const SCHIKKEN_PANEL_TAG: isize = 1007;
/// Recent entry `n` has tag `RECENT_TAG_BASE + n`; the menu never shows more than `RECENT_TAG_SLOTS`.
const RECENT_TAG_BASE: isize = 2000;
const RECENT_TAG_SLOTS: isize = 64;
/// CAM dialog `n` (`CamDialog::ALL` order) has tag `CAM_TAG_BASE + n`. Tag ranges in use: 1000–1008
/// host items, 2000+ recent projects, 2100+ recent templates, 3000+ app items (`AppAction`), 4000+
/// CAM dialogs; 5000+ is taken by the inspector's caption views (`panels::inspector::CAPTION_TAG_BASE`),
/// which share a container with buttons carrying menu tags.
const CAM_TAG_BASE: isize = 4000;
const CLEAR_RECENT_TEMPLATES_TAG: isize = 1008;
/// The golf-5 panels (G5-native); far from 1009+ so parallel lanes keep their own tags.
const OFFSET_PANEL_TAG: isize = 1030;
const STRUCTURE_PANEL_TAG: isize = 1031;
/// The golf-6 workshop panels (G6-werk).
const TRACE_PREP_PANEL_TAG: isize = 1032;
const OPERATOR_PANEL_TAG: isize = 1033;
/// Recent template `n` has tag `RECENT_TEMPLATE_TAG_BASE + n`.
const RECENT_TEMPLATE_TAG_BASE: isize = 2100;

/// The `NSMenuItem.tag` of `item`.
pub fn menu_tag(item: MenuItem) -> isize {
    match item {
        MenuItem::Open => OPEN_TAG,
        MenuItem::Import => IMPORT_TAG,
        MenuItem::ImportImage => IMPORT_IMAGE_TAG,
        MenuItem::TraceImage => TRACE_IMAGE_TAG,
        MenuItem::LaserWindow => LASER_WINDOW_TAG,
        MenuItem::ExportDiagnostics => DIAGNOSTICS_TAG,
        MenuItem::ClearRecent => CLEAR_RECENT_TAG,
        MenuItem::SchikkenPanel => SCHIKKEN_PANEL_TAG,
        MenuItem::OffsetPanel => OFFSET_PANEL_TAG,
        MenuItem::StructurePanel => STRUCTURE_PANEL_TAG,
        MenuItem::TracePrepPanel => TRACE_PREP_PANEL_TAG,
        MenuItem::OperatorPanel => OPERATOR_PANEL_TAG,
        MenuItem::ClearRecentTemplates => CLEAR_RECENT_TEMPLATES_TAG,
        MenuItem::RecentTemplate(n) => {
            isize::try_from(n).ok().filter(|n| *n < RECENT_TAG_SLOTS).map_or(-1, |n| RECENT_TEMPLATE_TAG_BASE + n)
        }
        MenuItem::App(action) => action.tag(),
        MenuItem::Recent(n) => {
            isize::try_from(n).ok().filter(|n| *n < RECENT_TAG_SLOTS).map_or(-1, |n| RECENT_TAG_BASE + n)
        }
        MenuItem::CamDialog(dialog) => burnburn_presentation::CamDialog::ALL
            .iter()
            .position(|d| *d == dialog)
            .map_or(-1, |i| CAM_TAG_BASE + i as isize),
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
    if tag == IMPORT_IMAGE_TAG {
        return Some(MenuItem::ImportImage);
    }
    if tag == TRACE_IMAGE_TAG {
        return Some(MenuItem::TraceImage);
    }
    if tag == LASER_WINDOW_TAG {
        return Some(MenuItem::LaserWindow);
    }
    if tag == DIAGNOSTICS_TAG {
        return Some(MenuItem::ExportDiagnostics);
    }
    if tag == CLEAR_RECENT_TAG {
        return Some(MenuItem::ClearRecent);
    }
    if let Some(dialog) = tag
        .checked_sub(CAM_TAG_BASE)
        .and_then(|i| usize::try_from(i).ok())
        .and_then(|i| burnburn_presentation::CamDialog::ALL.get(i))
    {
        return Some(MenuItem::CamDialog(*dialog));
    }
    if tag == SCHIKKEN_PANEL_TAG {
        return Some(MenuItem::SchikkenPanel);
    }
    if tag == OFFSET_PANEL_TAG {
        return Some(MenuItem::OffsetPanel);
    }
    if tag == STRUCTURE_PANEL_TAG {
        return Some(MenuItem::StructurePanel);
    }
    if tag == TRACE_PREP_PANEL_TAG {
        return Some(MenuItem::TracePrepPanel);
    }
    if tag == OPERATOR_PANEL_TAG {
        return Some(MenuItem::OperatorPanel);
    }
    if tag == CLEAR_RECENT_TEMPLATES_TAG {
        return Some(MenuItem::ClearRecentTemplates);
    }
    if (RECENT_TEMPLATE_TAG_BASE..RECENT_TEMPLATE_TAG_BASE + RECENT_TAG_SLOTS).contains(&tag) {
        return usize::try_from(tag - RECENT_TEMPLATE_TAG_BASE).ok().map(MenuItem::RecentTemplate);
    }
    if let Some(action) = crate::app_menu::AppAction::from_tag(tag) {
        return Some(MenuItem::App(action));
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
    /// One of the inspector's nine reference-point buttons (`index` in `RefPoint::ALL` order).
    ReferencePointChosen {
        document: DocumentId,
        index: isize,
    },
    /// The inspector's aspect-lock checkbox was clicked: the lock flips.
    AspectLockToggled {
        document: DocumentId,
    },
    /// The RD export save sheet closed: a path on OK, `None` on cancel.
    ExportPanelFinished {
        document: DocumentId,
        path: Option<PathBuf>,
    },
    /// The G-code export save sheet closed: a path on OK, `None` on cancel.
    GcodePanelFinished {
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
        /// The option fields the sheet shows (tabs, leads, fill and image options), raw.
        options: Vec<(CutField, CutValue)>,
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
    /// The sketch panel's "Herstel laatste geldige" button.
    SketchRestoreRequested {
        document: DocumentId,
    },
    /// The sketch panel's "Verwijder parameter" button with the selected parameter row.
    SketchParameterRemoveRequested {
        document: DocumentId,
        row: usize,
    },
    /// A parameter set in the sketch panel: name and expression as typed.
    SketchParameterCommitted {
        document: DocumentId,
        name: String,
        text: String,
    },
    /// The Replace Image file panel closed: a path on OK, `None` on cancel.
    ReplaceImageFileChosen {
        document: DocumentId,
        path: Option<PathBuf>,
    },
    /// The Replace to Fit file panel closed: a path on OK, `None` on cancel (G7-trace, T09).
    ReplaceToFitFileChosen {
        document: DocumentId,
        path: Option<PathBuf>,
    },
    /// The Save Processed Bitmap sheet closed: a path on OK, `None` on cancel.
    BitmapSavePanelFinished {
        document: DocumentId,
        path: Option<PathBuf>,
    },
    /// The Trace Image file panel closed: a path on OK, `None` on cancel.
    TraceFileChosen {
        document: DocumentId,
        path: Option<PathBuf>,
    },
    /// The DPI field of the bitmap import sheet committed (Return or end of editing), raw text.
    ImageDpiCommitted {
        document: DocumentId,
        text: String,
    },
    /// A field or the gray pop-up of the Adjust Image sheet committed: every field's raw text in
    /// [`crate::panels_image::ADJUST_FIELDS`] order and the gray pop-up item.
    ImageAdjustEdited {
        document: DocumentId,
        fields: [String; 9],
        gray_index: isize,
    },
    /// A field, pop-up or checkbox of the tracer panel changed: the four fields in
    /// [`crate::panels_image::TRACE_FIELDS`] order, both pop-ups' items and both checkboxes.
    TraceFormEdited {
        document: DocumentId,
        fields: [String; 4],
        mode_index: isize,
        threshold_index: isize,
        invert: bool,
        curves: bool,
    },
    /// The tracer's recipe pop-up: item `index` of `TracePreset::ALL`.
    TracePresetChosen {
        document: DocumentId,
        index: isize,
    },
    /// The tracer's "Gebruik voorstel" button.
    TraceSuggestionClicked {
        document: DocumentId,
    },
    /// The recovery alert closed with this `NSModalResponse` (buttons in
    /// [`crate::lifecycle::RECOVERY_ALERT_BUTTONS`] order).
    RecoveryAnswered {
        path: PathBuf,
        response: isize,
    },
    /// OK in the geometry dialog (join, round corners, optimize): the typed value, unparsed.
    /// A control of the text toolbar, the text-on-path sheet, the variable-text panel or the
    /// barcode dialog (W3-UI-tekst): the slot it is and the value it reports.
    TextPanel {
        document: DocumentId,
        slot: crate::panels::Slot,
        value: crate::panels::SlotValue,
    },
    GeometryDialogConfirmed {
        document: DocumentId,
        text: String,
    },
    GeometryDialogCancelled {
        document: DocumentId,
    },
    /// A control of the Schikken panel was used. `values` are all of the panel's values at that
    /// moment (fields as typed, a choice as its index, a check as `1`/`0`).
    SchikkenControl {
        document: DocumentId,
        control: crate::panels::SchikkenControl,
        values: Vec<(crate::panels::SchikkenControl, String)>,
    },
    /// A control of one of the golf-5 panels (notes, offset and repair, centre-line decisions,
    /// structure) was used. `values` are all of that panel's values at that moment.
    G5Control {
        document: DocumentId,
        panel: crate::panels::G5Panel,
        control: crate::panels::G5Control,
        values: Vec<(crate::panels::G5Control, String)>,
    },
    /// A shape or tool option row of the inspector edited (Return or end of editing), raw text;
    /// a checkbox sends `1` or `0`.
    ParamCommitted {
        document: DocumentId,
        section: crate::panels::ParamSection,
        slot: usize,
        text: String,
    },
    /// The diagnostic export's save panel closed: where to write (`None`: cancelled) and which
    /// private data the user ticked.
    DiagnosticsPanelFinished {
        options: burnburn_presentation::DiagnosticOptions,
        path: Option<PathBuf>,
    },
    /// An event of an app-level panel (preferences, shortcuts, history, preview, support).
    App(crate::app_menu::AppEvent),
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
    /// Show NSOpenPanel for the picture of Replace Image; the result comes back as
    /// [`NativeEvent::ReplaceImageFileChosen`].
    ShowReplaceImagePanel,
    /// Show NSOpenPanel for the picture of Replace to Fit; the result comes back as
    /// [`NativeEvent::ReplaceToFitFileChosen`].
    ShowReplaceToFitPanel,
    /// Show NSSavePanel for Save Processed Bitmap; the result comes back as
    /// [`NativeEvent::BitmapSavePanelFinished`].
    ShowBitmapSavePanel,
    /// Show NSOpenPanel for an image to trace; the result comes back as
    /// [`NativeEvent::TraceFileChosen`].
    ShowTracePanel,
    /// Show or hide the laser panel of the active window.
    ToggleLaserPanel,
    /// Show or hide the Schikken options panel of the active window.
    ToggleSchikkenPanel,
    /// Show or hide one of the golf-5 panels (offset and repair, structure) of the active window.
    ToggleG5Panel(crate::panels::G5Panel),
    /// Ask which private data may go into the diagnostic report, then show a save panel; the answer
    /// comes back as [`NativeEvent::DiagnosticsPanelFinished`].
    ShowDiagnosticsPanel,
    /// Open or perform an app-level panel or dialog (see [`crate::app_menu::AppAction`]).
    App(crate::app_menu::AppAction),
}

/// `NSEventModifierFlags` bits (stable AppKit ABI values).
pub mod flags {
    pub const SHIFT: usize = 1 << 17;
    pub const CONTROL: usize = 1 << 18;
    pub const OPTION: usize = 1 << 19;
    pub const COMMAND: usize = 1 << 20;
}

/// AppKit `NSModalResponse` values the host sees from sheets.
/// The quit guard's choice for an `NSAlert` response (the guard itself is shared with Windows);
/// anything unexpected keeps the app open.
pub fn guard_choice(response: isize) -> crate::quit_guard::GuardChoice {
    match response.checked_sub(response::FIRST_BUTTON).and_then(|i| usize::try_from(i).ok()) {
        Some(1) => crate::quit_guard::GuardChoice::QuitAnyway,
        _ => crate::quit_guard::GuardChoice::Stay,
    }
}

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
    pub const TAB: u16 = 48;
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
        E::Menu(MenuItem::Import | MenuItem::ImportImage) => return Reaction::ShowImportPanel,
        E::Menu(MenuItem::TraceImage) => return Reaction::ShowTracePanel,
        E::Menu(MenuItem::Command(CommandKey::ReplaceImage)) => return Reaction::ShowReplaceImagePanel,
        E::Menu(MenuItem::Command(CommandKey::ReplaceImageToFit)) => return Reaction::ShowReplaceToFitPanel,
        E::Menu(MenuItem::Command(CommandKey::SaveProcessedBitmap)) => return Reaction::ShowBitmapSavePanel,
        E::Menu(MenuItem::LaserWindow) => return Reaction::ToggleLaserPanel,
        E::Menu(MenuItem::SchikkenPanel) => return Reaction::ToggleSchikkenPanel,
        E::Menu(MenuItem::OffsetPanel) => return Reaction::ToggleG5Panel(crate::panels::G5Panel::Offset),
        E::Menu(MenuItem::StructurePanel) => return Reaction::ToggleG5Panel(crate::panels::G5Panel::Structure),
        E::Menu(MenuItem::TracePrepPanel) => return Reaction::ToggleG5Panel(crate::panels::G5Panel::TracePrep),
        E::Menu(MenuItem::OperatorPanel) => return Reaction::ToggleG5Panel(crate::panels::G5Panel::Operator),
        E::Menu(MenuItem::ExportDiagnostics) => return Reaction::ShowDiagnosticsPanel,
        E::Menu(MenuItem::App(action)) => return Reaction::App(action),
        E::App(event) => return crate::app_menu::translate_app(event, vm),
        E::DiagnosticsPanelFinished { options, path: Some(path) } => UiIntent::ExportDiagnostics { options, path },
        E::DiagnosticsPanelFinished { path: None, .. } => return Reaction::Nothing,
        E::Menu(MenuItem::ClearRecent) => UiIntent::ClearRecentProjects,
        // The CAM dialogs belong to the window in front.
        E::Menu(MenuItem::CamDialog(dialog)) => {
            let Some(document) = vm.active else { return Reaction::Nothing };
            UiIntent::OpenCamDialog { document, dialog }
        }
        E::Menu(MenuItem::ClearRecentTemplates) => {
            UiIntent::Files(burnburn_presentation::FilesIntent::ClearRecentTemplates)
        }
        // A recent template needs the stored list: see `crate::panels_files::recent_template_intent`.
        E::Menu(MenuItem::RecentTemplate(_)) => return Reaction::Nothing,
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
                key_code::TAB => CanvasKey::Tab,
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
        E::ReferencePointChosen { document, index } if open(document) => {
            match crate::panels::reference_point_intent(document, index) {
                Some(intent) => intent,
                None => return Reaction::Nothing,
            }
        }
        E::AspectLockToggled { document } => match crate::panels::aspect_lock_intent(vm, document) {
            Some(intent) => intent,
            None => return Reaction::Nothing,
        },
        E::ExportPanelFinished { document, path: Some(path) } if open(document) => {
            UiIntent::ExportRd { document, path }
        }
        // Cancelling the export changes nothing (there is no export dialog state in presentation).
        E::ExportPanelFinished { .. } => return Reaction::Nothing,
        E::GcodePanelFinished { document, path: Some(path) } if open(document) => {
            UiIntent::ExportGcode { document, path }
        }
        E::GcodePanelFinished { .. } => return Reaction::Nothing,
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
            options,
        } => {
            let Some(sheet) = vm.document(document).and_then(|d| d.cut_settings.as_ref()) else {
                return Reaction::Nothing;
            };
            let mode = crate::panels::sheet_mode_of(sheet, mode_index);
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
                    extras: crate::panels::confirmed_extras(options),
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
        E::SketchRestoreRequested { document } => match crate::panels::sketch_restore_intent(vm, document) {
            Some(i) => i,
            None => return Reaction::Nothing,
        },
        E::SketchParameterRemoveRequested { document, row } => {
            match crate::panels::sketch_parameter_remove_intent(vm, document, row) {
                Some(i) => i,
                None => return Reaction::Nothing,
            }
        }
        E::SketchParameterCommitted { document, name, text } => {
            match crate::panels::sketch_parameter_intent(vm, document, &name, &text) {
                Some(i) => i,
                None => return Reaction::Nothing,
            }
        }
        E::TextPanel { document, slot, value } => match crate::panels::text_panel_intent(vm, document, slot, value) {
            Some(intent) => intent,
            None => return Reaction::Nothing,
        },
        E::GeometryDialogConfirmed { document, text } => {
            match crate::panels::geometry_dialog_confirm_intent(vm, document, &text) {
                Some(i) => i,
                None => return Reaction::Nothing,
            }
        }
        E::ReplaceImageFileChosen { document, path: Some(path) } if open(document) => {
            UiIntent::ReplaceImage { document, path }
        }
        E::ReplaceToFitFileChosen { document, path: Some(path) } if open(document) => {
            UiIntent::ReplaceImageToFit { document, path }
        }
        E::BitmapSavePanelFinished { document, path: Some(path) } if open(document) => {
            UiIntent::SaveProcessedBitmap { document, path }
        }
        E::TraceFileChosen { document, path: Some(path) } if open(document) => {
            UiIntent::OpenTraceSource { document, path }
        }
        E::ImageDpiCommitted { document, text } => {
            let Some(shown) = vm.document(document).and_then(|d| d.image.import.as_ref()) else {
                return Reaction::Nothing;
            };
            match crate::panels_image::image_dpi_intent(document, shown, text) {
                Some(i) => i,
                None => return Reaction::Nothing,
            }
        }
        E::GeometryDialogCancelled { document } => match crate::panels::geometry_dialog_cancel_intent(vm, document) {
            Some(i) => i,
            None => return Reaction::Nothing,
        },
        E::SchikkenControl { document, control, values } => {
            match crate::panels::schikken_intent(vm, document, control, &values) {
                Some(i) => i,
                None => return Reaction::Nothing,
            }
        }
        E::G5Control { document, panel, control, values } => {
            match crate::panels::g5_intent(vm, document, panel, control, &values) {
                Some(i) => i,
                None => return Reaction::Nothing,
            }
        }
        E::ParamCommitted { document, section, slot, text } => {
            match crate::panels::param_intent(vm, document, section, slot, &text) {
                Some(i) => i,
                None => return Reaction::Nothing,
            }
        }
        E::ImageAdjustEdited { document, fields, gray_index } => {
            let Some(shown) = vm.document(document).and_then(|d| d.image.adjust.as_ref()) else {
                return Reaction::Nothing;
            };
            match crate::panels_image::image_adjust_intent(document, shown, fields, gray_index) {
                Some(i) => i,
                None => return Reaction::Nothing,
            }
        }
        E::TraceFormEdited { document, fields, mode_index, threshold_index, invert, curves } => {
            let Some(shown) = vm.document(document).and_then(|d| d.tracer.as_ref()) else {
                return Reaction::Nothing;
            };
            match crate::panels_image::trace_form_intent(
                document,
                &shown.form,
                fields,
                mode_index,
                threshold_index,
                invert,
                curves,
            ) {
                Some(i) => i,
                None => return Reaction::Nothing,
            }
        }
        E::TracePresetChosen { document, index } if vm.document(document).is_some_and(|d| d.tracer.is_some()) => {
            match crate::panels_image::trace_preset_intent(document, index) {
                Some(i) => i,
                None => return Reaction::Nothing,
            }
        }
        E::TraceSuggestionClicked { document } => {
            let Some(shown) = vm.document(document).and_then(|d| d.tracer.as_ref()) else {
                return Reaction::Nothing;
            };
            match crate::panels_image::trace_suggestion_intent(document, &shown.form) {
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
        MenuItem::Import | MenuItem::ImportImage | MenuItem::TraceImage => vm.active.is_some(),
        // The laser panel belongs to a window; the diagnostic export reads the journal only.
        MenuItem::LaserWindow | MenuItem::SchikkenPanel | MenuItem::OffsetPanel | MenuItem::StructurePanel => {
            vm.active.is_some()
        }
        // The preparation panel has something to show while a tracer is open in the window in front.
        MenuItem::TracePrepPanel => vm.active.and_then(|d| vm.document(d)).is_some_and(|d| d.trace_prep.is_some()),
        MenuItem::OperatorPanel => vm.active.and_then(|d| vm.document(d)).is_some_and(|d| d.operator.is_some()),
        MenuItem::ExportDiagnostics => true,
        // A dialog opens for a window; its own greying (no machine, no Z axis) says why in words.
        MenuItem::CamDialog(_) => vm.active.is_some(),
        MenuItem::Recent(_) | MenuItem::ClearRecent => false,
        // The recent-templates menu is rebuilt with the list, so what it shows can be chosen.
        MenuItem::RecentTemplate(_) | MenuItem::ClearRecentTemplates => true,
        MenuItem::App(action) => action.enabled(vm),
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
        MenuItem::Open
        | MenuItem::Import
        | MenuItem::ImportImage
        | MenuItem::TraceImage
        | MenuItem::LaserWindow
        | MenuItem::SchikkenPanel
        | MenuItem::OffsetPanel
        | MenuItem::StructurePanel
        | MenuItem::TracePrepPanel
        | MenuItem::OperatorPanel
        | MenuItem::ExportDiagnostics
        | MenuItem::CamDialog(_)
        | MenuItem::Recent(_)
        | MenuItem::ClearRecent
        | MenuItem::RecentTemplate(_)
        | MenuItem::ClearRecentTemplates
        | MenuItem::App(_) => false,
        MenuItem::Command(key) => vm.commands.get(&key).is_some_and(|c| c.checked),
    }
}

/// Menu title in `locale`, including the undo/redo action name from the view model.
pub fn menu_title(item: MenuItem, vm: &AppViewModel, locale: Locale) -> String {
    match item {
        MenuItem::Open => host_text(HostText::Open, locale).to_owned(),
        MenuItem::Import => host_text(HostText::Import, locale).to_owned(),
        MenuItem::ImportImage => host_text(HostText::ImageMenuImport, locale).to_owned(),
        MenuItem::TraceImage => host_text(HostText::ImageMenuTrace, locale).to_owned(),
        MenuItem::LaserWindow => crate::panels::laser_text(crate::panels::LaserText::MenuWindowItem, locale).to_owned(),
        MenuItem::ExportDiagnostics => {
            crate::panels::laser_text(crate::panels::LaserText::DiagnosticsItem, locale).to_owned()
        }
        MenuItem::CamDialog(dialog) => crate::panels::cam_menu_title(dialog, locale),
        MenuItem::SchikkenPanel => crate::panels::schikken_text(crate::panels::SchikkenText::MenuPanel, locale),
        MenuItem::OffsetPanel => crate::l10n_g5::g5_text(crate::l10n_g5::G5Text::OffsetMenu, locale).to_owned(),
        MenuItem::StructurePanel => crate::l10n_g5::g5_text(crate::l10n_g5::G5Text::StructureMenu, locale).to_owned(),
        MenuItem::TracePrepPanel => {
            crate::l10n_g6_werk::g6_text(crate::l10n_g6_werk::G6Text::TracePrepMenu, locale).to_owned()
        }
        MenuItem::OperatorPanel => {
            crate::l10n_g6_werk::g6_text(crate::l10n_g6_werk::G6Text::OperatorMenu, locale).to_owned()
        }
        MenuItem::ClearRecent | MenuItem::ClearRecentTemplates => host_text(HostText::ClearRecent, locale).to_owned(),
        MenuItem::App(action) => action.title(locale).to_owned(),
        // The entries are titled with their file name when the menu is built (`recent::recent_title`).
        MenuItem::Recent(_) | MenuItem::RecentTemplate(_) => String::new(),
        MenuItem::Command(key) => match vm.commands.get(&key).map(|c| c.label) {
            Some(label) => message_text(label, locale),
            None => message_text(MessageKey::Command(key), locale),
        },
    }
}

/// Object table rows for `document`: the user-visible names, top to bottom.
pub fn object_labels(vm: &AppViewModel, document: DocumentId) -> Vec<String> {
    vm.document(document).map(|d| d.objects.iter().map(linked_label).collect()).unwrap_or_default()
}

/// The name of an object, with how many layers use it when more than one (C02).
fn linked_label(o: &burnburn_presentation::ObjectRow) -> String {
    if o.linked > 1 { format!("{} (\u{d7}{})", o.label, o.linked) } else { o.label.clone() }
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
