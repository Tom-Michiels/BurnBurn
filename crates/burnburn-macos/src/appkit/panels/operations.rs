//! The operations section of the production strip: colour palette, layer table, add row.

use crate::appkit::panels::layer_cells::{self, LayerTableDelegate};
use crate::appkit::panels::production::LIFT;
use crate::appkit::panels::widgets::{push_button, rect, selected_rows, set_error_text, set_text, strip_label};
use crate::appkit::toolstrip::symbol_image;
use crate::appkit::window::DocWindow;

use burnburn_presentation::{AppViewModel, PALETTE_SLOTS};
use burnburn_types::DocumentId;
use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAccessibility, NSAutoresizingMaskOptions, NSButton, NSCellImagePosition, NSColor, NSFont, NSImageScaling,
    NSPopUpButton, NSScrollView, NSTableColumn, NSTableView, NSTableViewColumnAutoresizingStyle, NSTextField, NSView,
};
use objc2_foundation::{NSMutableIndexSet, NSSize, NSString};

use crate::l10n::{HostText, Locale, host_text};
use crate::panels::{
    LAYER_COLUMNS, LayerColumn, OPERATION_KINDS, layer_label, operation_kind_name, operations_panel, palette_buttons,
};

/// Room around a column header's text.
const HEADER_PAD: f64 = 6.0;
/// Palette swatch width and pitch (points).
pub(crate) const SWATCH: f64 = 19.0;
pub(crate) const SWATCH_PITCH: f64 = 19.0;

/// The canvas theme's colour of layer `slot` (palette swatch, layer cell and canvas agree).
pub(crate) fn layer_colour(slot: u8) -> Retained<NSColor> {
    let colours = burnburn_render::Theme::light().process_layers;
    let c = colours[usize::from(slot) % colours.len()];
    NSColor::colorWithSRGBRed_green_blue_alpha(f64::from(c.r), f64::from(c.g), f64::from(c.b), 1.0)
}

/// The operations panel: colour palette, layer table, add row and the message line.
pub(crate) struct OperationsSection {
    pub(crate) table: Retained<NSTableView>,
    /// The scroll view round the table: the empty state is laid over it (LF17).
    pub(crate) scroll: Retained<NSScrollView>,
    /// Name, speed, power of a new operation.
    pub(crate) inputs: [Retained<NSTextField>; 3],
    pub(crate) kind: Retained<NSPopUpButton>,
    pub(crate) add: Retained<NSButton>,
    pub(crate) error: Retained<NSTextField>,
    /// Makes the cells of the layer table (the table keeps its delegate weakly).
    pub(crate) layer_delegate: Retained<LayerTableDelegate>,
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
    for slot in 0..PALETTE_SLOTS {
        // The swatch is the SF Symbol "square.fill" in the layer colour (was the glyph U+25A0).
        let b = push_button(mtm, "");
        b.setBordered(false);
        b.setImage(symbol_image("square.fill", &layer_label(slot)).as_deref());
        b.setImagePosition(NSCellImagePosition::ImageOnly);
        b.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
        b.setContentTintColor(Some(&layer_colour(slot)));
        b.setFrame(rect(12.0 + f64::from(slot) * SWATCH_PITCH, 214.0 + LIFT, SWATCH, 26.0));
        b.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
        b.setTag(isize::from(slot));
        // A word before the code ("Laag C01"): VoiceOver reads a bare "C01" letter by letter.
        b.setAccessibilityLabel(Some(&NSString::from_str(&format!(
            "{} {}",
            t(HostText::LayerColumn),
            layer_label(slot)
        ))));
        strip.addSubview(&b);
        palette.push(b);
    }
    strip.setAccessibilityLabel(Some(&NSString::from_str(t(HostText::PaletteLabel))));

    let title =
        strip_label(mtm, strip, t(HostText::OperationsTitle), rect(12.0, 186.0 + LIFT, ops_width - 184.0, 18.0), false);
    title.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
    let settings = push_button(mtm, t(HostText::CutSettingsButton));
    settings.setFrame(rect(ops_width - 164.0, 182.0 + LIFT, 160.0, 26.0));
    settings.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
    strip.addSubview(&settings);
    let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(12.0, 84.0 + LIFT, ops_width - 12.0, 98.0));
    scroll.setHasVerticalScroller(true);
    scroll.setHasHorizontalScroller(true);
    scroll.setAutoresizingMask(NSAutoresizingMaskOptions(
        NSAutoresizingMaskOptions::ViewWidthSizable.0 | NSAutoresizingMaskOptions::ViewMinYMargin.0,
    ));
    let table = NSTableView::initWithFrame(NSTableView::alloc(mtm), rect(0.0, 0.0, ops_width - 12.0, 98.0));
    // Set before the columns are added: a table that squeezes its columns while they are added
    // keeps them squeezed.
    table.setColumnAutoresizingStyle(NSTableViewColumnAutoresizingStyle::NoColumnAutoresizing);
    for c in LAYER_COLUMNS {
        let column = NSTableColumn::initWithIdentifier(NSTableColumn::alloc(mtm), &NSString::from_str(c.id()));
        column.setTitle(&NSString::from_str(c.caption(locale)));
        column.setEditable(c.field().is_some() || c.is_toggle());
        // Wide enough for the header in this language (never cut off), and for what the cells
        // show: the swatch with its layer label, a name, a checkbox.
        let floor: f64 = match c {
            LayerColumn::Name => 96.0,
            LayerColumn::Layer => 48.0,
            LayerColumn::Kind => 56.0,
            LayerColumn::Output | LayerColumn::Visible => 40.0,
            _ => 48.0,
        };
        let width = floor.max((column.headerCell().cellSize().width + HEADER_PAD).ceil());
        column.setWidth(width);
        // Never narrower than that, whatever the table does when the window changes size.
        column.setMinWidth(width);
        table.addTableColumn(&column);
    }
    table.setAllowsEmptySelection(true);
    table.setRowHeight(layer_cells::ROW_HEIGHT);
    // The system's 17 pt between columns would not leave room for the columns at the default width.
    table.setIntercellSpacing(NSSize::new(4.0, 2.0));
    // The columns keep the width they were given (a squeezed header is a cut-off header); a
    // narrow window scrolls the table instead.
    table.setColumnAutoresizingStyle(NSTableViewColumnAutoresizingStyle::NoColumnAutoresizing);
    table.setAccessibilityLabel(Some(&NSString::from_str(t(HostText::OperationsTitle))));
    scroll.setDocumentView(Some(&table));
    strip.addSubview(&scroll);
    let layer_delegate = LayerTableDelegate::new(mtm);

    let input = |x: f64, w: f64, caption: HostText| {
        let f = NSTextField::textFieldWithString(&NSString::from_str(""), mtm);
        f.setFrame(rect(x, 50.0 + LIFT, w, 24.0));
        f.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
        f.setPlaceholderString(Some(&NSString::from_str(t(caption))));
        f.setAccessibilityLabel(Some(&NSString::from_str(t(caption))));
        strip.addSubview(&f);
        f
    };
    let name = input(12.0, 110.0, HostText::OperationName);
    let kind =
        NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), rect(126.0, 48.0 + LIFT, 84.0, 26.0), false);
    for k in OPERATION_KINDS {
        kind.addItemWithTitle(&NSString::from_str(operation_kind_name(k, locale)));
    }
    kind.setAccessibilityLabel(Some(&NSString::from_str(t(HostText::OperationKind))));
    kind.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
    strip.addSubview(&kind);
    let speed = input(214.0, 74.0, HostText::OperationSpeed);
    let power = input(292.0, 64.0, HostText::OperationPower);
    let add = push_button(mtm, t(HostText::OperationAdd));
    add.setFrame(rect(12.0, 44.0, 172.0, 28.0));
    add.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
    strip.addSubview(&add);
    let error = strip_label(mtm, strip, "", rect(12.0, 6.0, ops_width, 34.0), true);
    error.setTextColor(Some(&NSColor::systemRedColor()));
    OperationsSection {
        table,
        scroll,
        inputs: [name, speed, power],
        kind,
        add,
        error,
        layer_delegate,
        palette,
        settings,
    }
}

impl DocWindow {
    /// The operations table and its message line.
    pub(crate) fn refresh_operations(&self, vm: &AppViewModel, document: DocumentId) {
        let panel = operations_panel(vm, document, self.locale);

        for (b, model) in self.operations.palette.iter().zip(palette_buttons(vm, document, self.locale)) {
            if b.toolTip().map(|t| t.to_string()).as_deref() != Some(&model.help) {
                let help = NSString::from_str(&model.help);
                b.setToolTip(Some(&help));
                b.setAccessibilityHelp(Some(&help));
            }
        }
        if self.controller.set_operation_rows(panel.rows.clone()) {
            let table = self.operations.table.clone();
            // A view-based table drops its selection on reload: the rows that are still there stay selected.
            let selected = selected_rows(&table);
            if let Err(e) = crate::appkit::objc_try("reloading the operations table", || {
                table.reloadData();
                let rows = usize::try_from(table.numberOfRows()).unwrap_or(0);
                let keep = NSMutableIndexSet::new();
                for row in selected.iter().filter(|r| **r < rows) {
                    keep.addIndex(*row);
                }
                if keep.count() > 0 {
                    table.selectRowIndexes_byExtendingSelection(&keep, false);
                }
            }) {
                eprintln!("burnburn: {e}");
            }
        }
        // The empty table says so with the empty state laid over it (LF17); the message line keeps
        // only errors, with their tone glyph (LF15: colour is never the only carrier).
        self.operations_empty
            .show(burnburn_host_models::empty_state::operations_empty(vm, document, self.locale).as_ref());
        let message = &panel.error;
        if message.is_empty() {
            set_text(&self.operations.error, message);
        } else {
            set_error_text(&self.operations.error, message);
        }
        let color = if panel.error.is_empty() { NSColor::secondaryLabelColor() } else { NSColor::systemRedColor() };
        self.operations.error.setTextColor(Some(&color));
    }
}
