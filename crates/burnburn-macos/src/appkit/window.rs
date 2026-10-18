//! One document window: tool strip, split view with the object table, the canvas
//! view above the production strip (operations panel, bed camera) and the inspector (X/Y/B/H,
//! rotation, messages, plan preview with Compile/Export, machine). Builds the widgets and copies
//! the view model into them; it never decides anything.

use std::rc::Weak;

use burnburn_presentation::{
    AppViewModel, CameraViewModel, CommandKey, FieldKey, LAYER_SLOTS, MachineViewModel, MessageKey,
};
use burnburn_types::{DocumentId, OperationId};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly, sel};
use objc2_app_kit::{
    NSAccessibility, NSAlert, NSAutoresizingMaskOptions, NSBackingStoreType, NSButton, NSButtonCell, NSButtonType,
    NSColor, NSControlStateValueOff, NSControlStateValueOn, NSFloatingWindowLevel, NSFont, NSLineBreakMode,
    NSPopUpButton, NSSavePanel, NSScrollView, NSSplitView, NSSplitViewDividerStyle, NSTableColumn, NSTableView,
    NSTextField, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSIndexSet, NSMutableIndexSet, NSPoint, NSRect, NSSize, NSString};

use super::app::{HostApp, HostOptions};
use super::canvas::Canvas;
use super::classes::{CanvasView, ControllerParts, WindowController, table_delegate, table_source, window_delegate};
use super::toolstrip::{STRIP_HEIGHT as TOOL_STRIP_HEIGHT, ToolStrip};
use crate::adapter::{
    MenuItem, menu_tag, message_text, object_labels, selected_rows as vm_selected_rows, window_title,
};
use crate::l10n::{HostText, Locale, host_text, message_or_key};
use crate::panels::{
    CameraControl, INSPECTOR_FIELDS, LAYER_COLUMNS, LayerColumn, OPERATION_KINDS, camera_panel, cut_settings_panel,
    field_unit, import_panel, layer_label, machine_panel, operation_kind_name, operations_panel, palette_buttons,
    plan_panel,
};

const CONTENT_SIZE: NSSize = NSSize { width: 1100.0, height: 760.0 };
/// Machine controls offered at most at once (ConfirmStored, Start, QueryStatus, Cancel, Reconnect).
const MACHINE_SLOTS: usize = 6;

/// The import dialog: a sheet on the document window while presentation shows an import preview.
pub(crate) struct ImportSheet {
    pub(crate) sheet: Retained<NSWindow>,
    /// File, unit prompt, size, report.
    pub(crate) labels: [Retained<NSTextField>; 4],
    pub(crate) units: Retained<NSPopUpButton>,
    pub(crate) commit: Retained<NSButton>,
    pub(crate) cancel: Retained<NSButton>,
    pub(crate) shown: bool,
}
const SIDEBAR_WIDTH: f64 = 200.0;
const INSPECTOR_WIDTH: f64 = 260.0;
/// Height of the production strip under the canvas (palette, layers, bed camera).
const STRIP_HEIGHT: f64 = 250.0;
const CAMERA_WIDTH: f64 = 250.0;
/// Palette swatch width and pitch (points).
const SWATCH: f64 = 19.0;
const SWATCH_PITCH: f64 = 20.0;

/// The canvas theme's colour of layer `slot` (palette swatch, layer cell and canvas agree).
pub(crate) fn layer_colour(slot: u8) -> Retained<NSColor> {
    let colours = burnburn_render::Theme::light().process_layers;
    let c = colours[usize::from(slot) % colours.len()];
    NSColor::colorWithSRGBRed_green_blue_alpha(f64::from(c.r), f64::from(c.g), f64::from(c.b), 1.0)
}

/// Black or white, whichever reads better on the layer colour.
pub(crate) fn layer_text_colour(slot: u8) -> Retained<NSColor> {
    let colours = burnburn_render::Theme::light().process_layers;
    let c = colours[usize::from(slot) % colours.len()];
    let luminance = 0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b;
    if luminance > 0.5 { NSColor::blackColor() } else { NSColor::whiteColor() }
}

/// The operations panel: colour palette, layer table, add row and the message line.
pub(crate) struct OperationsSection {
    pub(crate) table: Retained<NSTableView>,
    /// Name, speed, power of a new operation.
    pub(crate) inputs: [Retained<NSTextField>; 3],
    pub(crate) kind: Retained<NSPopUpButton>,
    pub(crate) add: Retained<NSButton>,
    pub(crate) error: Retained<NSTextField>,
    /// One button per colour (tag = slot).
    pub(crate) palette: Vec<Retained<NSButton>>,
    /// Opens Cut Settings for the selected layer row.
    pub(crate) settings: Retained<NSButton>,
}

/// The Cut Settings sheet of one layer.
pub(crate) struct CutSettingsSheet {
    pub(crate) sheet: Retained<NSWindow>,
    pub(crate) title: Retained<NSTextField>,
    pub(crate) mode: Retained<NSPopUpButton>,
    /// Speed, min power, max power, passes, kerf.
    pub(crate) fields: [Retained<NSTextField>; 5],
    pub(crate) output: Retained<NSButton>,
    pub(crate) visible: Retained<NSButton>,
    pub(crate) error: Retained<NSTextField>,
    pub(crate) ok: Retained<NSButton>,
    pub(crate) cancel: Retained<NSButton>,
    /// The operation whose values are in the fields while the sheet is up.
    pub(crate) shown: Option<OperationId>,
}

/// The bed-camera section.
pub(crate) struct CameraSection {
    /// Status, frame, note.
    pub(crate) labels: [Retained<NSTextField>; 3],
    pub(crate) devices: Retained<NSPopUpButton>,
    /// Request access, start, stop (tags = [`CameraControl`] order).
    pub(crate) buttons: [Retained<NSButton>; 3],
}

/// Tag of a camera button.
pub(crate) fn camera_control(tag: isize) -> Option<CameraControl> {
    [CameraControl::RequestAccess, CameraControl::Start, CameraControl::Stop].get(usize::try_from(tag).ok()?).copied()
}

/// What [`DocWindow::open`] did with the canvas surface.
#[derive(Debug)]
pub enum CanvasStatus {
    /// Metal surface created; frames are presented once the window is on screen.
    Attached,
    /// No surface: the window works without a canvas picture.
    Unavailable(String),
}

pub(crate) struct DocWindow {
    pub(crate) window: Retained<NSWindow>,
    pub(crate) controller: Retained<WindowController>,
    pub(crate) canvas_view: Retained<CanvasView>,
    pub(crate) table: Retained<NSTableView>,
    pub(crate) strip: ToolStrip,
    pub(crate) fields: Vec<(FieldKey, Retained<NSTextField>)>,
    pub(crate) status_label: Retained<NSTextField>,
    /// Preview panel: title, plan id, estimate, state, findings.
    pub(crate) plan_labels: [Retained<NSTextField>; 5],
    pub(crate) compile_button: Retained<NSButton>,
    pub(crate) export_button: Retained<NSButton>,
    /// Machine section: status, message, progress, job.
    pub(crate) machine_labels: [Retained<NSTextField>; 4],
    /// Fixed slots for the machine controls (shown/hidden per view model).
    pub(crate) machine_buttons: Vec<Retained<NSButton>>,
    pub(crate) import: ImportSheet,
    pub(crate) operations: OperationsSection,
    pub(crate) cut_settings: CutSettingsSheet,
    pub(crate) camera: CameraSection,
    pub(crate) canvas_status: CanvasStatus,
    /// The sheet currently shown for this window (tests drive it like a user would).
    pub(crate) save_panel: Option<Retained<NSSavePanel>>,
    pub(crate) export_panel: Option<Retained<NSSavePanel>>,
    pub(crate) close_alert: Option<Retained<NSAlert>>,
    /// AppKit is already closing the window (we are inside `windowWillClose:`).
    pub(crate) closing: bool,
    locale: Locale,
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}

const TOP_PINNED: NSAutoresizingMaskOptions = NSAutoresizingMaskOptions(
    NSAutoresizingMaskOptions::ViewMinYMargin.0 | NSAutoresizingMaskOptions::ViewWidthSizable.0,
);

fn label(mtm: MainThreadMarker, text: &str, frame: NSRect) -> Retained<NSTextField> {
    let label = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    label.setFrame(frame);
    label.setAutoresizingMask(TOP_PINNED);
    label
}

fn field(mtm: MainThreadMarker, frame: NSRect) -> Retained<NSTextField> {
    let field = NSTextField::textFieldWithString(&NSString::from_str(""), mtm);
    field.setFrame(frame);
    field.setAutoresizingMask(TOP_PINNED);
    // Commit on Return *and* on end of editing (Tab, click elsewhere): orchestrator decision; the
    // adapter drops a commit whose text equals the view model's, so Return + Tab is one command.
    if let Some(cell) = field.cell() {
        cell.setSendsActionOnEndEditing(true);
    }
    field
}

/// Rows selected in `table`, ascending.
pub(crate) fn selected_rows(table: &NSTableView) -> Vec<usize> {
    let set = table.selectedRowIndexes();
    let mut rows = Vec::new();
    let mut i = set.firstIndex();
    while i != usize::MAX && i != isize::MAX as usize && rows.len() < set.count() {
        rows.push(i);
        i = set.indexGreaterThanIndex(i);
    }
    rows
}

/// The inspector column, laid out from the top.
struct Column<'a> {
    mtm: MainThreadMarker,
    view: &'a NSView,
    y: f64,
    width: f64,
}

impl Column<'_> {
    fn add_label(&mut self, text: &str, height: f64) -> Retained<NSTextField> {
        self.y -= height;
        let l = label(self.mtm, text, rect(12.0, self.y, self.width, height));
        self.view.addSubview(&l);
        self.y -= 4.0;
        l
    }

    fn add_field(&mut self) -> Retained<NSTextField> {
        self.y -= 24.0;
        let f = field(self.mtm, rect(12.0, self.y, self.width, 24.0));
        self.view.addSubview(&f);
        self.y -= 8.0;
        f
    }

    #[allow(unsafe_code)]
    fn add_button(&mut self, title: &str, target: &AnyObject, command: CommandKey) -> Retained<NSButton> {
        self.y -= 28.0;
        // SAFETY: `panelButton:` is a controller method taking the sender (`v@:@`); the target is
        // weak and DocWindow keeps the controller alive while the button exists (cleared in Drop).
        let b = unsafe {
            NSButton::buttonWithTitle_target_action(
                &NSString::from_str(title),
                Some(target),
                Some(sel!(panelButton:)),
                self.mtm,
            )
        };
        b.setFrame(rect(12.0, self.y, self.width, 28.0));
        b.setAutoresizingMask(TOP_PINNED);
        b.setTag(menu_tag(MenuItem::Command(command)));
        self.view.addSubview(&b);
        self.y -= 4.0;
        b
    }
}

#[allow(unsafe_code)]
fn button(
    mtm: MainThreadMarker,
    title: &str,
    target: &AnyObject,
    action: objc2::runtime::Sel,
    frame: NSRect,
) -> Retained<NSButton> {
    // SAFETY: `action` is a controller method taking the sender (`v@:@`); the target is weak and
    // DocWindow keeps the controller alive while the button exists (cleared in Drop).
    let b =
        unsafe { NSButton::buttonWithTitle_target_action(&NSString::from_str(title), Some(target), Some(action), mtm) };
    b.setFrame(frame);
    b.setAutoresizingMask(TOP_PINNED);
    b
}

/// The import dialog's sheet window and widgets (not shown until presentation has a preview).
#[allow(unsafe_code)]
fn import_sheet(mtm: MainThreadMarker, target: &AnyObject, locale: Locale) -> ImportSheet {
    let (w, h) = (440.0, 250.0);
    // SAFETY: a freshly allocated window with a valid rect, style and the buffered backing.
    let sheet = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            rect(0.0, 0.0, w, h),
            NSWindowStyleMask::Titled,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: ownership stays with this Retained.
    unsafe { sheet.setReleasedWhenClosed(false) };
    sheet.setTitle(&NSString::from_str(host_text(HostText::ImportTitle, locale)));
    let content = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, w, h));
    let mut col = Column { mtm, view: &content, y: h - 16.0, width: w - 24.0 };
    let file = col.add_label("", 20.0);
    file.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
    let prompt = col.add_label("", 20.0);
    col.add_label(host_text(HostText::ImportUnitLabel, locale), 18.0);
    col.y -= 26.0;
    let units =
        NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), rect(12.0, col.y, 160.0, 26.0), false);
    for unit in crate::panels::IMPORT_UNITS {
        units.addItemWithTitle(&NSString::from_str(crate::panels::import_unit_name(unit, locale)));
    }
    units.setAccessibilityLabel(Some(&NSString::from_str(host_text(HostText::ImportUnitLabel, locale))));
    // SAFETY: as for the buttons; `importUnitChosen:` takes the sender.
    unsafe { units.setTarget(Some(target)) };
    // SAFETY: as above.
    unsafe { units.setAction(Some(sel!(importUnitChosen:))) };
    content.addSubview(&units);
    col.y -= 8.0;
    let size = col.add_label("", 18.0);
    let report = col.add_label("", 36.0);
    report.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
    let cancel = button(
        mtm,
        host_text(HostText::ImportCancel, locale),
        target,
        sel!(panelButton:),
        rect(w - 232.0, 12.0, 104.0, 28.0),
    );
    cancel.setTag(menu_tag(MenuItem::Command(CommandKey::CancelImport)));
    let commit = button(
        mtm,
        host_text(HostText::ImportCommit, locale),
        target,
        sel!(panelButton:),
        rect(w - 116.0, 12.0, 104.0, 28.0),
    );
    commit.setTag(menu_tag(MenuItem::Command(CommandKey::CommitImport)));
    content.addSubview(&cancel);
    content.addSubview(&commit);
    sheet.setContentView(Some(&content));
    ImportSheet { sheet, labels: [file, prompt, size, report], units, commit, cancel, shown: false }
}

/// The Cut Settings sheet (targets are set once the controller exists).
#[allow(unsafe_code)]
fn cut_settings_sheet(mtm: MainThreadMarker, locale: Locale) -> CutSettingsSheet {
    let t = |text: HostText| host_text(text, locale);
    let (w, h) = (420.0, 360.0);
    // SAFETY: a freshly allocated window with a valid rect, style and the buffered backing.
    let sheet = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            rect(0.0, 0.0, w, h),
            NSWindowStyleMask::Titled,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: ownership stays with this Retained.
    unsafe { sheet.setReleasedWhenClosed(false) };
    sheet.setTitle(&NSString::from_str(t(HostText::CutSettingsTitle)));
    let content = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, w, h));
    let title = label(mtm, "", rect(16.0, h - 36.0, w - 32.0, 20.0));
    title.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
    content.addSubview(&title);
    let row_y = |i: usize| h - 72.0 - i as f64 * 32.0;
    let caption = |i: usize, text: HostText| {
        let l = label(mtm, t(text), rect(16.0, row_y(i), 190.0, 22.0));
        content.addSubview(&l);
    };
    caption(0, HostText::CutSettingsMode);
    let mode = NSPopUpButton::initWithFrame_pullsDown(
        NSPopUpButton::alloc(mtm),
        rect(210.0, row_y(0) - 2.0, 194.0, 26.0),
        false,
    );
    // Blocked modes stay listed but disabled (C05): enablement is ours, not AppKit's.
    mode.setAutoenablesItems(false);
    mode.setAccessibilityLabel(Some(&NSString::from_str(t(HostText::CutSettingsMode))));
    content.addSubview(&mode);
    let captions = [
        HostText::CutSettingsSpeed,
        HostText::CutSettingsPowerMin,
        HostText::CutSettingsPowerMax,
        HostText::CutSettingsPasses,
        HostText::CutSettingsKerf,
    ];
    let fields = captions.map(|text| {
        let i = 1 + captions.iter().position(|c| *c == text).unwrap_or(0);
        caption(i, text);
        let f = field(mtm, rect(210.0, row_y(i), 194.0, 24.0));
        f.setAccessibilityLabel(Some(&NSString::from_str(t(text))));
        content.addSubview(&f);
        f
    });
    let checkbox = |i: usize, text: HostText| {
        // SAFETY: no target and no action yet; set once the controller exists (not needed: read on OK).
        let b = unsafe { NSButton::checkboxWithTitle_target_action(&NSString::from_str(t(text)), None, None, mtm) };
        b.setFrame(rect(16.0, row_y(i), w - 32.0, 22.0));
        content.addSubview(&b);
        b
    };
    let output = checkbox(6, HostText::CutSettingsOutput);
    let visible = checkbox(7, HostText::CutSettingsVisible);
    let error = label(mtm, "", rect(16.0, 50.0, w - 32.0, 34.0));
    error.setTextColor(Some(&NSColor::systemRedColor()));
    error.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
    content.addSubview(&error);
    let cancel = push_button(mtm, t(HostText::CutSettingsCancel));
    cancel.setFrame(rect(w - 232.0, 12.0, 104.0, 28.0));
    cancel.setKeyEquivalent(&NSString::from_str("\u{1b}"));
    let ok = push_button(mtm, t(HostText::CutSettingsOk));
    ok.setFrame(rect(w - 116.0, 12.0, 104.0, 28.0));
    ok.setKeyEquivalent(&NSString::from_str("\r"));
    content.addSubview(&cancel);
    content.addSubview(&ok);
    sheet.setContentView(Some(&content));
    CutSettingsSheet { sheet, title, mode, fields, output, visible, error, ok, cancel, shown: None }
}

fn strip_label(mtm: MainThreadMarker, view: &NSView, text: &str, frame: NSRect, wrap: bool) -> Retained<NSTextField> {
    let l = NSTextField::labelWithString(&NSString::from_str(text), mtm);
    l.setFrame(frame);
    l.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
    if wrap {
        l.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
    }
    view.addSubview(&l);
    l
}

/// A push button whose target/action are set later (once the window controller exists).
#[allow(unsafe_code)]
fn push_button(mtm: MainThreadMarker, title: &str) -> Retained<NSButton> {
    // SAFETY: no target and no action yet: clicking does nothing until both are set.
    unsafe { NSButton::buttonWithTitle_target_action(&NSString::from_str(title), None, None, mtm) }
}

/// The production strip's widgets (targets/actions are set once the controller exists).
#[allow(unsafe_code)]
fn production_strip(
    mtm: MainThreadMarker,
    width: f64,
    locale: Locale,
) -> (Retained<NSView>, OperationsSection, CameraSection) {
    let t = |text: HostText| host_text(text, locale);
    let strip = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, width, STRIP_HEIGHT));
    let ops_width = width - CAMERA_WIDTH - 12.0;

    // Colour palette across the strip (LightBurn-like): a click puts the selection on that layer.
    let mut palette = Vec::new();
    for slot in 0..LAYER_SLOTS {
        let b = push_button(mtm, "\u{25A0}");
        b.setBordered(false);
        b.setFont(Some(&NSFont::systemFontOfSize(20.0)));
        b.setContentTintColor(Some(&layer_colour(slot)));
        b.setFrame(rect(12.0 + f64::from(slot) * SWATCH_PITCH, 214.0, SWATCH, 26.0));
        b.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
        b.setTag(isize::from(slot));
        b.setAccessibilityLabel(Some(&NSString::from_str(&layer_label(slot))));
        strip.addSubview(&b);
        palette.push(b);
    }
    strip.setAccessibilityLabel(Some(&NSString::from_str(t(HostText::PaletteLabel))));

    let title =
        strip_label(mtm, &strip, t(HostText::OperationsTitle), rect(12.0, 186.0, ops_width - 170.0, 18.0), false);
    title.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
    let settings = push_button(mtm, t(HostText::CutSettingsButton));
    settings.setFrame(rect(ops_width - 160.0, 182.0, 160.0, 26.0));
    settings.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
    strip.addSubview(&settings);
    let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(12.0, 84.0, ops_width - 12.0, 98.0));
    scroll.setHasVerticalScroller(true);
    scroll.setHasHorizontalScroller(true);
    scroll.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
    let table = NSTableView::initWithFrame(NSTableView::alloc(mtm), rect(0.0, 0.0, ops_width - 12.0, 98.0));
    for c in LAYER_COLUMNS {
        let column = NSTableColumn::initWithIdentifier(NSTableColumn::alloc(mtm), &NSString::from_str(c.id()));
        column.setTitle(&NSString::from_str(c.caption(locale)));
        column.setEditable(c.field().is_some() || c.is_toggle());
        column.setWidth(match c {
            LayerColumn::Name => 100.0,
            LayerColumn::Layer | LayerColumn::Output | LayerColumn::Visible | LayerColumn::Passes => 48.0,
            _ => 64.0,
        });
        if c.is_toggle() {
            let cell = NSButtonCell::initTextCell(NSButtonCell::alloc(mtm), &NSString::from_str(""));
            cell.setButtonType(NSButtonType::Switch);
            // SAFETY: an NSButtonCell is a valid data cell for a cell-based table column.
            unsafe { column.setDataCell(&cell) };
        }
        table.addTableColumn(&column);
    }
    table.setAllowsEmptySelection(true);
    table.setAccessibilityLabel(Some(&NSString::from_str(t(HostText::OperationsTitle))));
    scroll.setDocumentView(Some(&table));
    strip.addSubview(&scroll);

    let input = |x: f64, w: f64, caption: HostText| {
        let f = NSTextField::textFieldWithString(&NSString::from_str(""), mtm);
        f.setFrame(rect(x, 50.0, w, 24.0));
        f.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
        f.setPlaceholderString(Some(&NSString::from_str(t(caption))));
        f.setAccessibilityLabel(Some(&NSString::from_str(t(caption))));
        strip.addSubview(&f);
        f
    };
    let name = input(12.0, 110.0, HostText::OperationName);
    let kind = NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), rect(126.0, 48.0, 84.0, 26.0), false);
    for k in OPERATION_KINDS {
        kind.addItemWithTitle(&NSString::from_str(operation_kind_name(k, locale)));
    }
    kind.setAccessibilityLabel(Some(&NSString::from_str(t(HostText::OperationKind))));
    kind.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
    strip.addSubview(&kind);
    let speed = input(214.0, 74.0, HostText::OperationSpeed);
    let power = input(292.0, 64.0, HostText::OperationPower);
    let add = push_button(mtm, t(HostText::OperationAdd));
    add.setFrame(rect(360.0, 48.0, (ops_width - 360.0).max(120.0), 28.0));
    add.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
    strip.addSubview(&add);
    let error = strip_label(mtm, &strip, "", rect(12.0, 8.0, ops_width, 36.0), true);
    error.setTextColor(Some(&NSColor::systemRedColor()));

    let x = width - CAMERA_WIDTH;
    let cw = CAMERA_WIDTH - 12.0;
    let camera_title = strip_label(mtm, &strip, t(HostText::CameraTitle), rect(x, 186.0, cw, 18.0), false);
    camera_title.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
    let status = strip_label(mtm, &strip, "", rect(x, 150.0, cw, 32.0), true);
    let devices = NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), rect(x, 120.0, cw, 26.0), false);
    devices.setAccessibilityLabel(Some(&NSString::from_str(t(HostText::CameraTitle))));
    devices.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
    strip.addSubview(&devices);
    let bw = (cw - 8.0) / 3.0;
    let camera_button = |i: usize, text: HostText| {
        let b = push_button(mtm, t(text));
        b.setFrame(rect(x + i as f64 * (bw + 4.0), 86.0, bw, 28.0));
        b.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
        b.setTag(i as isize);
        b.setEnabled(false);
        strip.addSubview(&b);
        b
    };
    let buttons = [
        camera_button(0, HostText::CameraAccessButton),
        camera_button(1, HostText::CameraStartButton),
        camera_button(2, HostText::CameraStopButton),
    ];
    let frame = strip_label(mtm, &strip, "", rect(x, 62.0, cw, 18.0), false);
    let note = strip_label(mtm, &strip, "", rect(x, 8.0, cw, 50.0), true);
    note.setFont(Some(&NSFont::systemFontOfSize(10.0)));
    (
        strip,
        OperationsSection { table, inputs: [name, speed, power], kind, add, error, palette, settings },
        CameraSection { labels: [status, frame, note], devices, buttons },
    )
}

impl DocWindow {
    #[allow(unsafe_code)]
    pub(crate) fn open(
        mtm: MainThreadMarker,
        app: Weak<HostApp>,
        document: DocumentId,
        options: &HostOptions,
        locale: Locale,
    ) -> DocWindow {
        let style = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable
            | NSWindowStyleMask::Resizable;
        // SAFETY: a freshly allocated window with a valid rect, style and the buffered backing.
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                rect(0.0, 0.0, CONTENT_SIZE.width, CONTENT_SIZE.height + TOOL_STRIP_HEIGHT),
                style,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        // SAFETY: ownership stays with this Retained; AppKit must not also release on close.
        unsafe { window.setReleasedWhenClosed(false) };

        let (h, canvas_width) = (CONTENT_SIZE.height, CONTENT_SIZE.width - SIDEBAR_WIDTH - INSPECTOR_WIDTH);
        let split = NSSplitView::initWithFrame(NSSplitView::alloc(mtm), rect(0.0, 0.0, CONTENT_SIZE.width, h));
        split.setVertical(true);
        split.setDividerStyle(NSSplitViewDividerStyle::Thin);

        // Left: the object list (NSTableView, one column, fed from the view model).
        let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(0.0, 0.0, SIDEBAR_WIDTH, h));
        scroll.setHasVerticalScroller(true);
        let table = NSTableView::initWithFrame(NSTableView::alloc(mtm), rect(0.0, 0.0, SIDEBAR_WIDTH, h));
        let column = NSTableColumn::initWithIdentifier(NSTableColumn::alloc(mtm), &NSString::from_str("name"));
        column.setTitle(&NSString::from_str(host_text(HostText::ObjectsColumn, locale)));
        column.setWidth(SIDEBAR_WIDTH - 20.0);
        table.addTableColumn(&column);
        table.setAllowsMultipleSelection(true);
        table.setAllowsEmptySelection(true);
        table.setAccessibilityLabel(Some(&NSString::from_str(host_text(HostText::ObjectListLabel, locale))));
        scroll.setDocumentView(Some(&table));

        // Centre: the canvas view above the production strip; wgpu attaches its CAMetalLayer once
        // the view is in the window.
        let centre = NSSplitView::initWithFrame(NSSplitView::alloc(mtm), rect(0.0, 0.0, canvas_width, h));
        centre.setVertical(false);
        centre.setDividerStyle(NSSplitViewDividerStyle::Thin);
        let canvas_view = CanvasView::new(mtm, rect(0.0, 0.0, canvas_width, h - STRIP_HEIGHT), app.clone(), document);
        let (strip, operations, camera) = production_strip(mtm, canvas_width, locale);
        let cut_settings = cut_settings_sheet(mtm, locale);

        // Right: inspector fields, message line, plan preview.
        let inspector = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, INSPECTOR_WIDTH, h));
        inspector.setAccessibilityLabel(Some(&NSString::from_str(host_text(HostText::InspectorLabel, locale))));
        let mut col = Column { mtm, view: &inspector, y: h - 8.0, width: INSPECTOR_WIDTH - 24.0 };
        let mut fields = Vec::new();
        for key in INSPECTOR_FIELDS {
            let caption = format!("{} ({})", message_or_key(MessageKey::Field(key), locale), field_unit(key, locale));
            col.add_label(&caption, 18.0);
            let f = col.add_field();
            f.setAccessibilityLabel(Some(&NSString::from_str(&caption)));
            fields.push((key, f));
        }
        let status_label = col.add_label("", 32.0);
        status_label.setTextColor(Some(&NSColor::systemRedColor()));
        status_label.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
        col.y -= 12.0;
        let plan_title = col.add_label("", 20.0);
        plan_title.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
        let plan_id = col.add_label("", 18.0);
        plan_id.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
        plan_id.setSelectable(true);
        plan_id.setFont(Some(&NSFont::systemFontOfSize(10.0)));
        let estimate = col.add_label("", 18.0);
        let state = col.add_label("", 34.0);
        state.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
        let findings = col.add_label("", 18.0);

        let controller = WindowController::new(
            mtm,
            ControllerParts {
                app: app.clone(),
                document,
                fields: fields.clone(),
                operations_table: operations.table.clone(),
                operation_inputs: operations.inputs.clone(),
                operation_kind: operations.kind.clone(),
                cut_mode: cut_settings.mode.clone(),
                cut_fields: cut_settings.fields.clone(),
                cut_toggles: [cut_settings.output.clone(), cut_settings.visible.clone()],
                camera_devices: camera.devices.clone(),
            },
        );
        let target: &AnyObject = controller.as_ref();
        let compile_button = col.add_button(host_text(HostText::CompileButton, locale), target, CommandKey::Compile);
        let export_button = col.add_button(host_text(HostText::ExportButton, locale), target, CommandKey::ExportRd);

        // Machine section: status in words, message, progress, job, controls.
        col.y -= 12.0;
        let machine_title = col.add_label(host_text(HostText::MachineTitle, locale), 20.0);
        machine_title.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
        let machine_status = col.add_label("", 20.0);
        machine_status.setFont(Some(&NSFont::boldSystemFontOfSize(12.0)));
        let machine_message = col.add_label("", 34.0);
        machine_message.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
        let machine_progress = col.add_label("", 18.0);
        let machine_job = col.add_label("", 18.0);
        let half = (col.width - 8.0) / 2.0;
        let mut machine_buttons = Vec::new();
        for slot in 0..MACHINE_SLOTS {
            let (row, column) = (slot / 2, slot % 2);
            let y = col.y - 28.0 - row as f64 * 30.0;
            let b =
                button(mtm, "", target, sel!(machineButton:), rect(12.0 + column as f64 * (half + 8.0), y, half, 28.0));
            b.setTag(slot as isize);
            b.setHidden(true);
            inspector.addSubview(&b);
            machine_buttons.push(b);
        }
        let import = import_sheet(mtm, target, locale);

        centre.addSubview(&canvas_view);
        centre.addSubview(&strip);
        centre.adjustSubviews();
        centre.setPosition_ofDividerAtIndex(h - STRIP_HEIGHT, 0);
        split.addSubview(&scroll);
        split.addSubview(&centre);
        split.addSubview(&inspector);
        split.adjustSubviews();
        // The tool strip sits above the split view; the content view holds both.
        let strip = ToolStrip::new(mtm, app, document, target, locale, CONTENT_SIZE.width);
        strip.view().setFrame(rect(0.0, h, CONTENT_SIZE.width, TOOL_STRIP_HEIGHT));
        let container =
            NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, CONTENT_SIZE.width, h + TOOL_STRIP_HEIGHT));
        split.setFrame(rect(0.0, 0.0, CONTENT_SIZE.width, h));
        split.setAutoresizingMask(NSAutoresizingMaskOptions(
            NSAutoresizingMaskOptions::ViewWidthSizable.0 | NSAutoresizingMaskOptions::ViewHeightSizable.0,
        ));
        container.addSubview(&split);
        container.addSubview(strip.view());
        window.setContentView(Some(&container));

        for (_, f) in &fields {
            // SAFETY: the target is weak; DocWindow keeps the controller alive while the field
            // exists and clears the target in Drop.
            unsafe { f.setTarget(Some(target)) };
            // SAFETY: `fieldCommitted:` is a controller method taking the sender (`v@:@`).
            unsafe { f.setAction(Some(sel!(fieldCommitted:))) };
        }
        // SAFETY: as for the fields; the actions below take the sender (`v@:@`).
        unsafe { operations.add.setTarget(Some(target)) };
        // SAFETY: as above.
        unsafe { operations.add.setAction(Some(sel!(addOperation:))) };
        for b in &operations.palette {
            // SAFETY: as above.
            unsafe { b.setTarget(Some(target)) };
            // SAFETY: as above.
            unsafe { b.setAction(Some(sel!(paletteClicked:))) };
        }
        // SAFETY: as above; the Cut Settings button and a double-click on a row open the sheet.
        unsafe { operations.settings.setTarget(Some(target)) };
        // SAFETY: as above.
        unsafe { operations.settings.setAction(Some(sel!(layerOpened:))) };
        // SAFETY: as above.
        unsafe { operations.table.setTarget(Some(target)) };
        // SAFETY: as above.
        unsafe { operations.table.setDoubleAction(Some(sel!(layerOpened:))) };
        for (b, action) in [(&cut_settings.ok, sel!(cutSettingsOk:)), (&cut_settings.cancel, sel!(cutSettingsCancel:))]
        {
            // SAFETY: as above.
            unsafe { b.setTarget(Some(target)) };
            // SAFETY: as above.
            unsafe { b.setAction(Some(action)) };
        }
        // SAFETY: as above.
        unsafe { cut_settings.mode.setTarget(Some(target)) };
        // SAFETY: as above.
        unsafe { cut_settings.mode.setAction(Some(sel!(cutSettingsModeChosen:))) };
        for b in &camera.buttons {
            // SAFETY: as above.
            unsafe { b.setTarget(Some(target)) };
            // SAFETY: as above.
            unsafe { b.setAction(Some(sel!(cameraButton:))) };
        }
        // SAFETY: data source and delegate are weak; cleared in Drop before the controller goes.
        unsafe { operations.table.setDataSource(Some(table_source(&controller))) };
        // SAFETY: as above.
        unsafe { operations.table.setDelegate(Some(table_delegate(&controller))) };
        // SAFETY: data source and delegate are weak; cleared in Drop before the controller goes.
        unsafe { table.setDataSource(Some(table_source(&controller))) };
        // SAFETY: as above.
        unsafe { table.setDelegate(Some(table_delegate(&controller))) };
        window.setDelegate(Some(window_delegate(&controller)));

        window.center();
        if options.float_windows {
            // A test process cannot take focus from the user's frontmost app; float the window
            // above it so it is really on screen (Metal presents nothing for an occluded window).
            window.setLevel(NSFloatingWindowLevel);
            window.orderFrontRegardless();
        } else {
            window.makeKeyAndOrderFront(None);
        }

        let view: Retained<NSView> = Retained::into_super(canvas_view.clone());
        let canvas_status = match Canvas::attach(mtm, view) {
            Ok(canvas) => {
                controller.set_canvas(Some(canvas));
                CanvasStatus::Attached
            }
            Err(e) => {
                eprintln!("burnburn: {e}");
                CanvasStatus::Unavailable(e.to_string())
            }
        };
        DocWindow {
            window,
            controller,
            canvas_view,
            table,
            strip,
            fields,
            status_label,
            plan_labels: [plan_title, plan_id, estimate, state, findings],
            compile_button,
            export_button,
            machine_labels: [machine_status, machine_message, machine_progress, machine_job],
            machine_buttons,
            import,
            operations,
            cut_settings,
            camera,
            canvas_status,
            save_panel: None,
            export_panel: None,
            close_alert: None,
            closing: false,
            locale,
        }
    }

    /// Copy the view model into the widgets. Only changed values are written, so a field the user
    /// is typing in keeps its text until presentation reports a different value.
    pub(crate) fn refresh(
        &mut self,
        vm: &AppViewModel,
        document: DocumentId,
        machine: Option<&MachineViewModel>,
        camera: Option<&CameraViewModel>,
    ) {
        let Some(doc) = vm.document(document) else { return };
        let locale = self.locale;
        let title = window_title(vm, document);
        if self.window.title().to_string() != title {
            self.window.setTitle(&NSString::from_str(&title));
        }
        self.window.setDocumentEdited(doc.dirty);

        let mut message = String::new();
        for (key, field) in &self.fields {
            let model = doc.inspector.as_ref().and_then(|i| i.field(*key));
            let text = model.map(|f| f.text.clone()).unwrap_or_default();
            field.setEnabled(model.is_some_and(|f| f.editable));
            // A field with an error keeps what the user typed (so it can be corrected).
            let has_error = model.is_some_and(|f| f.error.is_some());
            if !has_error && field.stringValue().to_string() != text {
                field.setStringValue(&NSString::from_str(&text));
            }
            if let Some(error) = model.and_then(|f| f.error) {
                message = message_text(error, locale);
            }
        }
        if message.is_empty()
            && let Some(m) = doc.messages.last()
        {
            message = message_text(m.key, locale);
        }
        if message.is_empty()
            && let Some(m) = &doc.tool_measurement
        {
            message = crate::toolbar::measurement_text(m, locale);
        }
        set_text(&self.status_label, &message);

        let panel = plan_panel(vm, document, locale);
        for (label, text) in
            self.plan_labels.iter().zip([&panel.title, &panel.plan_id, &panel.estimate, &panel.state, &panel.findings])
        {
            set_text(label, text);
        }
        let enabled = |k: CommandKey| vm.active == Some(document) && vm.commands.get(&k).is_some_and(|c| c.enabled);
        self.compile_button.setEnabled(enabled(CommandKey::Compile));
        self.export_button.setEnabled(enabled(CommandKey::ExportRd));

        let m = machine_panel(machine, locale);
        for (label, text) in self.machine_labels.iter().zip([&m.status, &m.message, &m.progress, &m.job]) {
            set_text(label, text);
        }
        for (slot, b) in self.machine_buttons.iter().enumerate() {
            match m.buttons.get(slot) {
                Some((title, enabled)) => {
                    set_button_title(b, title);
                    b.setEnabled(*enabled);
                    b.setHidden(false);
                }
                None => b.setHidden(true),
            }
        }
        self.refresh_import(vm, document);
        self.refresh_operations(vm, document);
        self.refresh_cut_settings(vm, document);
        self.refresh_camera(camera);

        // The view model's toolbar is the active document's: another window keeps what it had.
        if vm.active == Some(document) {
            self.strip.refresh(&vm.toolbar);
        }

        if self.controller.set_labels(object_labels(vm, document)) {
            let table = self.table.clone();
            if let Err(e) = super::objc_try("reloading the object table", || table.reloadData()) {
                eprintln!("burnburn: {e}");
            }
        }
        let wanted = vm_selected_rows(vm, document);
        if selected_rows(&self.table) != wanted {
            let set = NSMutableIndexSet::new();
            for row in &wanted {
                set.addIndex(*row);
            }
            let set: &NSIndexSet = &set;
            self.controller.syncing(|| {
                // An index beyond the rows raises NSRangeException: never abort for a stale row.
                if let Err(e) = super::objc_try("selecting object rows", || {
                    self.table.selectRowIndexes_byExtendingSelection(set, false)
                }) {
                    eprintln!("burnburn: {e}");
                }
            });
        }
    }

    /// The operations table and its message line.
    fn refresh_operations(&self, vm: &AppViewModel, document: DocumentId) {
        let panel = operations_panel(vm, document, self.locale);
        let empty = panel.rows.is_empty();
        for (b, model) in self.operations.palette.iter().zip(palette_buttons(vm, document, self.locale)) {
            if b.toolTip().map(|t| t.to_string()).as_deref() != Some(&model.help) {
                let help = NSString::from_str(&model.help);
                b.setToolTip(Some(&help));
                b.setAccessibilityHelp(Some(&help));
            }
        }
        if self.controller.set_operation_rows(panel.rows.clone()) {
            let table = self.operations.table.clone();
            if let Err(e) = super::objc_try("reloading the operations table", || table.reloadData()) {
                eprintln!("burnburn: {e}");
            }
        }
        let message = if !panel.error.is_empty() || !empty { &panel.error } else { &panel.empty_text };
        set_text(&self.operations.error, message);
        let color = if panel.error.is_empty() { NSColor::secondaryLabelColor() } else { NSColor::systemRedColor() };
        self.operations.error.setTextColor(Some(&color));
    }

    /// Show, fill or end the Cut Settings sheet as presentation opens and closes it. The fields
    /// are filled once per opening, so a refused OK keeps what the user typed.
    fn refresh_cut_settings(&mut self, vm: &AppViewModel, document: DocumentId) {
        let c = &mut self.cut_settings;
        let Some(panel) = cut_settings_panel(vm, document, self.locale) else {
            if c.shown.take().is_some() {
                let (window, sheet) = (self.window.clone(), c.sheet.clone());
                if let Err(e) = super::objc_try("ending the Cut Settings sheet", || window.endSheet(&sheet)) {
                    eprintln!("burnburn: {e}");
                }
            }
            return;
        };
        let titles: Vec<String> = c.mode.itemTitles().iter().map(|t| t.to_string()).collect();
        let wanted: Vec<String> = panel.modes.iter().map(|m| m.title.clone()).collect();
        if titles != wanted {
            c.mode.removeAllItems();
            for title in &wanted {
                c.mode.addItemWithTitle(&NSString::from_str(title));
            }
        }
        for (i, m) in panel.modes.iter().enumerate() {
            if let Some(item) = c.mode.itemAtIndex(i as isize) {
                item.setEnabled(m.enabled);
            }
        }
        set_text(&c.error, &panel.error);
        if c.shown == Some(panel.operation) {
            return;
        }
        set_text(&c.title, &panel.title);
        c.mode.selectItemAtIndex(panel.mode_index.map_or(-1, |i| i as isize));
        for (f, text) in
            c.fields.iter().zip([&panel.speed, &panel.power_min, &panel.power_max, &panel.passes, &panel.kerf])
        {
            f.setStringValue(&NSString::from_str(text));
        }
        c.fields[4].setEnabled(panel.kerf_enabled);
        for (b, on) in [(&c.output, panel.output), (&c.visible, panel.visible)] {
            b.setState(if on { NSControlStateValueOn } else { NSControlStateValueOff });
        }
        let attached = c.shown.is_some();
        c.shown = Some(panel.operation);
        if !attached {
            let (window, sheet) = (self.window.clone(), c.sheet.clone());
            if let Err(e) =
                super::objc_try("showing the Cut Settings sheet", || window.beginSheet_completionHandler(&sheet, None))
            {
                eprintln!("burnburn: {e}");
                c.shown = None;
            }
        }
    }

    /// The bed-camera section (also refreshed by the host's camera tick while a camera runs).
    pub(crate) fn refresh_camera(&self, camera: Option<&CameraViewModel>) {
        let panel = camera_panel(camera, self.locale);
        for (label, text) in self.camera.labels.iter().zip([&panel.status, &panel.frame, &panel.note]) {
            set_text(label, text);
        }
        let popup = &self.camera.devices;
        let titles: Vec<String> = popup.itemTitles().iter().map(|t| t.to_string()).collect();
        if titles != panel.devices {
            popup.removeAllItems();
            for name in &panel.devices {
                popup.addItemWithTitle(&NSString::from_str(name));
            }
        }
        if let Some(i) = panel.selected
            && popup.indexOfSelectedItem() != i as isize
        {
            popup.selectItemAtIndex(i as isize);
        }
        popup.setEnabled(panel.start_enabled);
        for (b, on) in self.camera.buttons.iter().zip([panel.access_enabled, panel.start_enabled, panel.stop_enabled]) {
            b.setEnabled(on);
        }
    }

    /// Show, update or end the import sheet as presentation's import preview comes and goes.
    fn refresh_import(&mut self, vm: &AppViewModel, document: DocumentId) {
        let Some(panel) = import_panel(vm, document, self.locale) else {
            if self.import.shown {
                self.import.shown = false;
                let (window, sheet) = (self.window.clone(), self.import.sheet.clone());
                if let Err(e) = super::objc_try("ending the import sheet", || window.endSheet(&sheet)) {
                    eprintln!("burnburn: {e}");
                }
            }
            return;
        };
        for (label, text) in
            self.import.labels.iter().zip([&panel.file, &panel.unit_prompt, &panel.size, &panel.report])
        {
            set_text(label, text);
        }
        let selected = panel.selected_unit.map_or(-1, |i| i as isize);
        if self.import.units.indexOfSelectedItem() != selected {
            self.import.units.selectItemAtIndex(selected);
        }
        self.import.commit.setEnabled(panel.commit_enabled);
        if !self.import.shown {
            let (window, sheet) = (self.window.clone(), self.import.sheet.clone());
            match super::objc_try("showing the import sheet", || window.beginSheet_completionHandler(&sheet, None)) {
                Ok(()) => self.import.shown = true,
                Err(e) => eprintln!("burnburn: {e}"),
            }
        }
    }

    /// Close the window natively (presentation already agreed).
    pub(crate) fn close_now(&self) {
        self.controller.allow_close();
        let window = self.window.clone();
        if let Err(e) = super::objc_try("closing a window", || window.close()) {
            eprintln!("burnburn: {e}");
        }
    }
}

fn set_button_title(button: &NSButton, title: &str) {
    if button.title().to_string() != title {
        button.setTitle(&NSString::from_str(title));
    }
}

fn set_text(label: &NSTextField, text: &str) {
    if label.stringValue().to_string() != text {
        label.setStringValue(&NSString::from_str(text));
    }
}

impl Drop for DocWindow {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // Clearing weak references is always valid; afterwards nothing refers to the controller.
        self.strip.clear_targets();
        for (_, f) in &self.fields {
            // SAFETY: a nil target cannot dangle.
            unsafe { f.setTarget(None) };
        }
        // SAFETY: as above.
        unsafe { self.import.units.setTarget(None) };
        // SAFETY: as above.
        unsafe { self.cut_settings.mode.setTarget(None) };
        // SAFETY: as above.
        unsafe { self.operations.table.setTarget(None) };
        for b in [
            &self.compile_button,
            &self.export_button,
            &self.import.commit,
            &self.import.cancel,
            &self.operations.add,
            &self.operations.settings,
            &self.cut_settings.ok,
            &self.cut_settings.cancel,
        ]
        .into_iter()
        .chain(self.machine_buttons.iter())
        .chain(self.camera.buttons.iter())
        .chain(self.operations.palette.iter())
        {
            // SAFETY: as above.
            unsafe { b.setTarget(None) };
        }
        // SAFETY: nil data source / delegate cannot dangle.
        unsafe { self.table.setDataSource(None) };
        // SAFETY: as above.
        unsafe { self.table.setDelegate(None) };
        // SAFETY: as above.
        unsafe { self.operations.table.setDataSource(None) };
        // SAFETY: as above.
        unsafe { self.operations.table.setDelegate(None) };
        self.controller.set_canvas(None);
        self.window.setDelegate(None);
        if self.import.shown {
            self.window.endSheet(&self.import.sheet);
        }
        if self.cut_settings.shown.is_some() {
            self.window.endSheet(&self.cut_settings.sheet);
        }
        if !self.closing {
            self.controller.allow_close();
            self.window.close();
        }
    }
}
