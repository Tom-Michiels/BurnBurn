//! The sketch panel (B8): a floating utility panel with the constraint table (value column
//! editable), the degrees of freedom or the conflict in words, the relation buttons, the
//! parameter table and the parameter fields. It is shown while the active window is in sketch
//! mode and only copies the view model into widgets; edits become `NativeEvent`s.

use burnburn_presentation::{AppViewModel, CommandKey};
use burnburn_types::DocumentId;
use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAccessibility, NSBackingStoreType, NSButton, NSColor, NSFont, NSPanel, NSScrollView, NSSegmentSwitchTracking,
    NSSegmentedControl, NSTableColumn, NSTableView, NSTextField, NSView, NSWindowStyleMask,
};
use objc2_foundation::{NSIndexSet, NSMutableIndexSet, NSString};

use crate::adapter::{MenuItem, menu_tag};
use crate::appkit::classes::{SKETCH_PARAM_NAME, SKETCH_PARAM_VALUE};
use crate::appkit::panels::widgets::{field, label, push_button, rect, selected_rows, set_text};
use crate::appkit::window::DocWindow;
use crate::l10n::{HostText, Locale, host_text, message_or_key};
use crate::panels::{SKETCH_BUTTONS, SKETCH_COLUMNS, SKETCH_TOOLBAR, sketch_panel, sketch_toolbar_enabled};
use burnburn_presentation::MessageKey;

const WIDTH: f64 = 540.0;
const HEIGHT: f64 = 560.0;

pub(crate) struct SketchSection {
    pub(crate) panel: Retained<NSPanel>,
    pub(crate) freedom: Retained<NSTextField>,
    /// The conflict message: a borderless button that selects what it names.
    pub(crate) problem: Retained<NSButton>,
    pub(crate) refused: Retained<NSTextField>,
    pub(crate) table: Retained<NSTableView>,
    /// The relation, dimension and remove buttons, in [`SKETCH_BUTTONS`] order, then Sluit schets.
    pub(crate) buttons: Vec<(CommandKey, Retained<NSButton>)>,
    pub(crate) role: Retained<NSButton>,
    /// "Herstel laatste geldige" (S04) and "Verwijder parameter" (S02).
    pub(crate) restore: Retained<NSButton>,
    pub(crate) param_delete: Retained<NSButton>,
    pub(crate) params_table: Retained<NSTableView>,
    /// New parameter: name and expression.
    pub(crate) param_inputs: [Retained<NSTextField>; 2],
    pub(crate) param_set: Retained<NSButton>,
    /// The toolbar's Schets control (lives in the toolbar, kept here with its siblings).
    pub(crate) toolbar: Retained<NSSegmentedControl>,
    pub(crate) shown: bool,
}

fn table_in(
    mtm: MainThreadMarker,
    content: &NSView,
    frame: objc2_foundation::NSRect,
    columns: &[(&str, &str, f64, bool)],
    accessibility: &str,
) -> Retained<NSTableView> {
    let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), frame);
    scroll.setHasVerticalScroller(true);
    let table =
        NSTableView::initWithFrame(NSTableView::alloc(mtm), rect(0.0, 0.0, frame.size.width, frame.size.height));
    for (id, title, width, editable) in columns {
        let column = NSTableColumn::initWithIdentifier(NSTableColumn::alloc(mtm), &NSString::from_str(id));
        column.setTitle(&NSString::from_str(title));
        column.setWidth(*width);
        column.setEditable(*editable);
        table.addTableColumn(&column);
    }
    table.setAllowsMultipleSelection(true);
    table.setAllowsEmptySelection(true);
    table.setAccessibilityLabel(Some(&NSString::from_str(accessibility)));
    scroll.setDocumentView(Some(&table));
    content.addSubview(&scroll);
    table
}

/// The panel and its widgets (targets, data sources and delegates are set once the controller exists).
#[allow(unsafe_code)]
pub(crate) fn sketch_section(mtm: MainThreadMarker, locale: Locale) -> SketchSection {
    let t = |text: HostText| host_text(text, locale);
    let style = NSWindowStyleMask::Titled | NSWindowStyleMask::UtilityWindow;
    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(mtm),
        rect(0.0, 0.0, WIDTH, HEIGHT),
        style,
        NSBackingStoreType::Buffered,
        false,
    );
    // SAFETY: ownership stays with this Retained.
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setFloatingPanel(true);
    panel.setHidesOnDeactivate(true);
    panel.setTitle(&NSString::from_str(t(HostText::SketchPanelTitle)));
    let content = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, WIDTH, HEIGHT));
    let inner = WIDTH - 24.0;

    let freedom = label(mtm, "", rect(12.0, HEIGHT - 34.0, inner - 190.0, 20.0));
    freedom.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
    content.addSubview(&freedom);
    let restore = push_button(mtm, t(HostText::SketchRestoreLast));
    restore.setFrame(rect(12.0 + inner - 186.0, HEIGHT - 38.0, 186.0, 26.0));
    content.addSubview(&restore);
    let problem = push_button(mtm, "");
    problem.setBordered(false);
    problem.setAlignment(objc2_app_kit::NSTextAlignment::Left);
    problem.setFrame(rect(12.0, HEIGHT - 60.0, inner, 22.0));
    problem.setContentTintColor(Some(&NSColor::systemRedColor()));
    content.addSubview(&problem);
    let refused = label(mtm, "", rect(12.0, HEIGHT - 82.0, inner, 18.0));
    refused.setTextColor(Some(&NSColor::systemRedColor()));
    content.addSubview(&refused);

    let columns: Vec<(&str, &str, f64, bool)> = SKETCH_COLUMNS
        .iter()
        .map(|c| {
            (
                c.id(),
                c.caption(locale),
                if *c == crate::panels::SketchColumn::Value { 150.0 } else { 100.0 },
                c.is_editable(),
            )
        })
        .collect();
    let table = table_in(mtm, &content, rect(12.0, 280.0, inner, 196.0), &columns, t(HostText::SketchColumnKind));

    // Relation, dimension and remove buttons: four per row.
    let mut buttons = Vec::new();
    let bw = (inner - 3.0 * 6.0) / 4.0;
    for (i, key) in SKETCH_BUTTONS.iter().chain([&CommandKey::SketchFinish]).enumerate() {
        let (row, col) = (i / 4, i % 4);
        let b = push_button(mtm, &message_or_key(MessageKey::Command(*key), locale));
        b.setFrame(rect(12.0 + col as f64 * (bw + 6.0), 244.0 - row as f64 * 32.0, bw, 28.0));
        b.setTag(menu_tag(MenuItem::Command(*key)));
        content.addSubview(&b);
        buttons.push((*key, b));
    }
    let role = push_button(mtm, t(HostText::SketchToggleRole));
    role.setFrame(rect(12.0 + 3.0 * (bw + 6.0), 180.0, bw, 28.0));
    content.addSubview(&role);

    let params_title = label(mtm, t(HostText::SketchParametersTitle), rect(12.0, 150.0, inner - 164.0, 18.0));
    params_title.setFont(Some(&NSFont::boldSystemFontOfSize(12.0)));
    content.addSubview(&params_title);
    let param_delete = push_button(mtm, t(HostText::SketchParameterDelete));
    param_delete.setFrame(rect(12.0 + inner - 160.0, 147.0, 160.0, 24.0));
    content.addSubview(&param_delete);
    let params_table = table_in(
        mtm,
        &content,
        rect(12.0, 62.0, inner, 84.0),
        &[
            (SKETCH_PARAM_NAME, t(HostText::SketchParameterName), 120.0, false),
            (SKETCH_PARAM_VALUE, t(HostText::SketchColumnValue), 300.0, true),
        ],
        t(HostText::SketchParametersTitle),
    );
    let name = field(mtm, rect(12.0, 30.0, 110.0, 24.0));
    name.setPlaceholderString(Some(&NSString::from_str(t(HostText::SketchParameterName))));
    name.setAccessibilityLabel(Some(&NSString::from_str(t(HostText::SketchParameterName))));
    let expr = field(mtm, rect(128.0, 30.0, inner - 128.0 - 124.0, 24.0));
    expr.setPlaceholderString(Some(&NSString::from_str(t(HostText::SketchParameterValue))));
    expr.setAccessibilityLabel(Some(&NSString::from_str(t(HostText::SketchParameterValue))));
    let param_set = push_button(mtm, t(HostText::SketchParameterSet));
    param_set.setFrame(rect(12.0 + inner - 116.0, 28.0, 116.0, 28.0));
    for v in [&name, &expr] {
        content.addSubview(v);
    }
    content.addSubview(&param_set);
    let hint = label(mtm, t(HostText::SketchHint), rect(12.0, 6.0, inner, 18.0));
    hint.setFont(Some(&NSFont::systemFontOfSize(10.0)));
    content.addSubview(&hint);
    panel.setContentView(Some(&content));

    let toolbar = NSSegmentedControl::new(mtm);
    toolbar.setSegmentCount(SKETCH_TOOLBAR.len() as isize);
    toolbar.setTrackingMode(NSSegmentSwitchTracking::Momentary);
    for (i, command) in SKETCH_TOOLBAR.iter().enumerate() {
        toolbar.setLabel_forSegment(
            &NSString::from_str(&message_or_key(MessageKey::Command(*command), locale)),
            i as isize,
        );
    }
    toolbar.sizeToFit();
    toolbar.setAccessibilityLabel(Some(&NSString::from_str(t(HostText::MenuSketch))));

    SketchSection {
        panel,
        freedom,
        problem,
        refused,
        table,
        buttons,
        role,
        restore,
        param_delete,
        params_table,
        param_inputs: [name, expr],
        param_set,
        toolbar,
        shown: false,
    }
}

impl DocWindow {
    /// Show, fill or hide the sketch panel as the window enters and leaves sketch mode.
    pub(crate) fn refresh_sketch(&mut self, vm: &AppViewModel, document: DocumentId) {
        for (i, on) in sketch_toolbar_enabled(vm).into_iter().enumerate() {
            self.sketch.toolbar.setEnabled_forSegment(on, i as isize);
        }
        let model = sketch_panel(vm, document, self.locale).filter(|_| vm.active == Some(document));
        let Some(p) = model else {
            if self.sketch.shown {
                self.sketch.shown = false;
                let panel = self.sketch.panel.clone();
                if let Err(e) = crate::appkit::objc_try("hiding the sketch panel", || panel.orderOut(None)) {
                    eprintln!("burnburn: {e}");
                }
            }
            return;
        };
        let sk = &self.sketch;
        sk.panel.setTitle(&NSString::from_str(&p.title));
        set_text(&sk.freedom, &p.freedom);
        let problem = if p.problem.is_empty() { String::new() } else { format!("{} {}", p.problem, p.problem_hint) };
        if sk.problem.title().to_string() != problem {
            sk.problem.setTitle(&NSString::from_str(&problem));
        }
        sk.problem.setHidden(problem.is_empty());
        sk.problem.setToolTip(Some(&NSString::from_str(&p.problem_hint)));
        set_text(&sk.refused, &p.refused);
        for (key, b) in &sk.buttons {
            let enabled = vm.commands.get(key).is_some_and(|c| c.enabled);
            b.setEnabled(enabled);
        }
        sk.role.setEnabled(p.role_enabled);
        sk.restore.setEnabled(p.restore_enabled);
        sk.param_delete.setEnabled(!p.parameters.is_empty());
        if self.controller.set_sketch_rows(p.rows.clone()) {
            let table = sk.table.clone();
            if let Err(e) = crate::appkit::objc_try("reloading the constraint table", || table.reloadData()) {
                eprintln!("burnburn: {e}");
            }
        }
        if self.controller.set_sketch_params(p.parameters.clone()) {
            let table = sk.params_table.clone();
            if let Err(e) = crate::appkit::objc_try("reloading the parameter table", || table.reloadData()) {
                eprintln!("burnburn: {e}");
            }
        }
        let wanted: Vec<usize> = p.rows.iter().enumerate().filter(|(_, r)| r.selected).map(|(i, _)| i).collect();
        if selected_rows(&sk.table) != wanted {
            let set = NSMutableIndexSet::new();
            for row in &wanted {
                set.addIndex(*row);
            }
            let set: &NSIndexSet = &set;
            let table = sk.table.clone();
            self.controller.syncing(|| {
                if let Err(e) = crate::appkit::objc_try("selecting constraint rows", || {
                    table.selectRowIndexes_byExtendingSelection(set, false)
                }) {
                    eprintln!("burnburn: {e}");
                }
            });
        }
        if !self.sketch.shown {
            self.sketch.shown = true;
            let panel = self.sketch.panel.clone();
            let window_frame = self.window.frame();
            if let Err(e) = crate::appkit::objc_try("showing the sketch panel", || {
                panel.setFrameTopLeftPoint(objc2_foundation::NSPoint::new(
                    window_frame.origin.x + window_frame.size.width - WIDTH - 12.0,
                    window_frame.origin.y + window_frame.size.height - 60.0,
                ));
                panel.orderFront(None);
            }) {
                eprintln!("burnburn: {e}");
            }
        }
    }

    /// Start editing the value cell of constraint-table row `row` (after "Maat", or a double-click
    /// on a dimension label).
    pub(crate) fn begin_dimension_edit(&self, row: usize) {
        let value_column = SKETCH_COLUMNS.iter().position(|c| c.is_editable()).unwrap_or(2);
        let (panel, table) = (self.sketch.panel.clone(), self.sketch.table.clone());
        let result = crate::appkit::objc_try("editing a dimension", || {
            panel.makeKeyAndOrderFront(None);
            table.editColumn_row_withEvent_select(value_column as isize, row as isize, None, true);
        });
        if let Err(e) = result {
            eprintln!("burnburn: {e}");
        }
    }
}
