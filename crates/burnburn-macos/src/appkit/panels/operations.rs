//! The operations section of the production strip: colour palette, layer table, add row.

use crate::appkit::panels::widgets::{push_button, rect, set_text, strip_label};
use crate::appkit::window::DocWindow;

use burnburn_presentation::{AppViewModel, LAYER_SLOTS};
use burnburn_types::DocumentId;
use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAccessibility, NSAutoresizingMaskOptions, NSButton, NSButtonCell, NSButtonType, NSColor, NSFont, NSPopUpButton,
    NSScrollView, NSTableColumn, NSTableView, NSTextField, NSView,
};
use objc2_foundation::NSString;

use crate::l10n::{HostText, Locale, host_text};
use crate::panels::{
    LAYER_COLUMNS, LayerColumn, OPERATION_KINDS, layer_label, operation_kind_name, operations_panel, palette_buttons,
};

/// Palette swatch width and pitch (points).
pub(crate) const SWATCH: f64 = 19.0;
pub(crate) const SWATCH_PITCH: f64 = 20.0;

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

/// The operations part of the production strip (targets/actions are set once the controller exists).
#[allow(unsafe_code)]
pub(crate) fn build(mtm: MainThreadMarker, strip: &NSView, ops_width: f64, locale: Locale) -> OperationsSection {
    let t = |text: HostText| host_text(text, locale);
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
        strip_label(mtm, strip, t(HostText::OperationsTitle), rect(12.0, 186.0, ops_width - 170.0, 18.0), false);
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
    let error = strip_label(mtm, strip, "", rect(12.0, 8.0, ops_width, 36.0), true);
    error.setTextColor(Some(&NSColor::systemRedColor()));
    OperationsSection { table, inputs: [name, speed, power], kind, add, error, palette, settings }
}

impl DocWindow {
    /// The operations table and its message line.
    pub(crate) fn refresh_operations(&self, vm: &AppViewModel, document: DocumentId) {
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
            if let Err(e) = crate::appkit::objc_try("reloading the operations table", || table.reloadData()) {
                eprintln!("burnburn: {e}");
            }
        }
        let message = if !panel.error.is_empty() || !empty { &panel.error } else { &panel.empty_text };
        set_text(&self.operations.error, message);
        let color = if panel.error.is_empty() { NSColor::secondaryLabelColor() } else { NSColor::systemRedColor() };
        self.operations.error.setTextColor(Some(&color));
    }
}
