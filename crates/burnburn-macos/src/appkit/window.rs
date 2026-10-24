//! One document window: tool strip (icons per catalog tool), Schets toolbar item, split view with the object table, the canvas
//! view above the production strip (operations panel, bed camera) and the inspector (X/Y/B/H,
//! rotation, messages, plan preview with Compile/Export, machine). Builds the widgets and copies
//! the view model into them; it never decides anything.

use std::rc::Weak;

use burnburn_presentation::{AppViewModel, CameraViewModel, CommandKey, FieldKey, MachineViewModel};
use burnburn_types::DocumentId;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly, Message, sel};
use objc2_app_kit::{
    NSAccessibility, NSAlert, NSAutoresizingMaskOptions, NSBackingStoreType, NSButton, NSColor, NSFloatingWindowLevel,
    NSFont, NSLineBreakMode, NSSavePanel, NSScrollView, NSSegmentedControl, NSSplitView, NSSplitViewDividerStyle,
    NSTableColumn, NSTableView, NSTextField, NSToolbar, NSToolbarDisplayMode, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSIndexSet, NSMutableIndexSet, NSPoint, NSSize, NSString};

use super::app::{HostApp, HostOptions};
use super::canvas::Canvas;
use super::classes::{
    CanvasView, ControllerParts, WindowController, split_delegate, table_delegate, table_source, toolbar_delegate,
    window_delegate,
};
use super::panels::arrange::arrange_toolbar;
use super::panels::barcode::{BarcodeSection, barcode_section};
use super::panels::camera::CameraSection;
use super::panels::cut_settings::{CutSettingsSheet, cut_settings_sheet};
use super::panels::empty_state::{EmptyOverlay, ProgressWidgets};
use super::panels::g5::{G5Section, g5_section};
use super::panels::image::{ImageSheets, image_sheets};
use super::panels::import::{ImportSheet, import_sheet};
use super::panels::inspector::{InspectorControlsWidgets, transform_block};
use super::panels::laser::{LaserSection, laser_section};
use super::panels::operations::OperationsSection;
use super::panels::production::{STRIP_HEIGHT, production_strip};
use super::panels::schikken::{SchikkenSection, schikken_section};
use super::panels::shapes::{ShapesSection, shapes_section};
use super::panels::sketch::{SketchSection, sketch_section};
use super::panels::text::{TextSection, text_section};
use super::panels::variable::{VariableSection, variable_section};
use super::panels::widgets::{Column, button, rect, selected_rows, set_button_title, set_text};
use super::status_bar::{STATUS_HEIGHT, StatusBarWidgets, status_bar_widgets};
use super::toolstrip::ToolStrip;
use super::window_state::WindowState;
use crate::adapter::{message_text, object_labels, selected_rows as vm_selected_rows, window_title};
use crate::l10n::{HostText, Locale, host_text};
use crate::panels::{machine_panel, plan_panel};

/// Default content size: the layer table needs about 630 pt beside the camera section, so the
/// canvas column is 940 pt wide (the plan's 1100 pt left only 640).
const CONTENT_SIZE: NSSize = NSSize { width: 1400.0, height: 820.0 };
/// The window does not shrink below this content size.
const MIN_CONTENT: NSSize = NSSize { width: 960.0, height: 620.0 };
/// Pane widths the dividers allow (points): object list, inspector, and the canvas column.
pub(crate) const SIDEBAR_RANGE: (f64, f64) = (150.0, 360.0);
pub(crate) const INSPECTOR_RANGE: (f64, f64) = (240.0, 420.0);
pub(crate) const CANVAS_MIN: f64 = 420.0;
/// Machine controls offered at most at once (ConfirmStored, Start, QueryStatus, Cancel, Reconnect).
const MACHINE_SLOTS: usize = 6;
const SIDEBAR_WIDTH: f64 = 200.0;
const INSPECTOR_WIDTH: f64 = 260.0;

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
    /// The split view and its three panes: object list, canvas column, inspector column.
    pub(crate) split: Retained<NSSplitView>,
    pub(crate) sidebar_pane: Retained<NSView>,
    pub(crate) canvas_pane: Retained<NSView>,
    pub(crate) inspector_pane: Retained<NSView>,
    /// The status bar: pointer coordinate, zoom, unit.
    pub(crate) status_coordinate: Retained<NSTextField>,
    pub(crate) status_zoom: Retained<NSTextField>,
    pub(crate) status_unit: Retained<NSTextField>,
    /// "Kopieer id" at the plan.
    pub(crate) copy_id_button: Retained<NSButton>,
    /// Pointer in canvas points (None until the pointer was over the canvas) and the display unit
    /// the coordinate is shown in.
    pub(crate) pointer: Option<[f64; 2]>,
    pub(crate) length_unit: burnburn_host_models::format::LengthPreference,
    pub(crate) status_bar: StatusBarWidgets,
    /// The empty state over the object list and over the layer table, and the progress row of the
    /// running tasks in the status bar (LF17).
    pub(crate) objects_empty: EmptyOverlay,
    pub(crate) operations_empty: EmptyOverlay,
    pub(crate) progress: ProgressWidgets,
    pub(crate) table: Retained<NSTableView>,
    pub(crate) toolbar: Retained<NSToolbar>,
    /// The toolbar's Uitlijnen control (align and distribute segments).
    pub(crate) arrange_tools: Retained<NSSegmentedControl>,
    pub(crate) strip: ToolStrip,
    pub(crate) fields: Vec<(FieldKey, Retained<NSTextField>)>,
    /// Reference point, aspect lock and flip buttons beside the fields.
    pub(crate) controls: InspectorControlsWidgets,
    pub(crate) status_label: Retained<NSTextField>,
    /// Preview panel: title, plan id, estimate, state, findings.
    pub(crate) plan_labels: [Retained<NSTextField>; 5],
    /// The measured values the plan used, one line each (W09).
    pub(crate) plan_calibration: Retained<NSTextField>,
    /// What the GRBL backend refuses of the plan (pull-down under the findings; hidden on Ruida).
    pub(crate) grbl_refusals: Retained<objc2_app_kit::NSPopUpButton>,
    /// Kept alive for the pull-down (a control's target is not retained).
    #[allow(dead_code)]
    grbl_refusal_target: Retained<super::panels::grbl_window::RefusalTarget>,
    pub(crate) compile_button: Retained<NSButton>,
    pub(crate) export_button: Retained<NSButton>,
    /// Machine section: status, message, progress, job.
    pub(crate) machine_labels: [Retained<NSTextField>; 4],
    /// Fixed slots for the machine controls (shown/hidden per view model).
    pub(crate) machine_buttons: Vec<Retained<NSButton>>,
    pub(crate) import: ImportSheet,
    pub(crate) image: ImageSheets,
    pub(crate) operations: OperationsSection,
    pub(crate) cut_settings: CutSettingsSheet,
    pub(crate) camera: CameraSection,
    pub(crate) sketch: SketchSection,
    /// The shape parameters, tool options and the geometry dialog.
    pub(crate) shapes: ShapesSection,
    /// The Schikken panel: options, dialogs and panels of the arrange flows (W3-UI-schikken).
    pub(crate) schikken: SchikkenSection,
    /// The golf-5 panels: notes, offset and repair, centre-line decisions, structure (G5-native).
    pub(crate) g5: Vec<G5Section>,
    /// The text toolbar, the text-on-path sheet, the variable-text panel and the barcode sheet.
    pub(crate) text: TextSection,
    pub(crate) variable: VariableSection,
    pub(crate) barcode: BarcodeSection,
    /// The laser window (floating panel, shown from the Laser menu).
    pub(crate) laser: LaserSection,
    /// The Material and Art Library panels, the bundle sheet and the export dialog.
    pub(crate) files: super::panels::files::FilesSection,
    pub(crate) canvas_status: CanvasStatus,
    /// The sheet currently shown for this window (tests drive it like a user would).
    pub(crate) save_panel: Option<Retained<NSSavePanel>>,
    pub(crate) export_panel: Option<Retained<NSSavePanel>>,
    pub(crate) close_alert: Option<Retained<NSAlert>>,
    /// AppKit is already closing the window (we are inside `windowWillClose:`).
    pub(crate) closing: bool,
    pub(crate) locale: Locale,
}

impl DocWindow {
    #[allow(unsafe_code)]
    pub(crate) fn open(
        mtm: MainThreadMarker,
        app: Weak<HostApp>,
        document: DocumentId,
        options: &HostOptions,
        locale: Locale,
        store: &WindowState,
    ) -> DocWindow {
        let saved = store.load(mtm);
        let style = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable
            | NSWindowStyleMask::Resizable;
        // SAFETY: a freshly allocated window with a valid rect, style and the buffered backing.
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                rect(0.0, 0.0, CONTENT_SIZE.width, CONTENT_SIZE.height),
                style,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        // SAFETY: ownership stays with this Retained; AppKit must not also release on close.
        unsafe { window.setReleasedWhenClosed(false) };

        let (h, canvas_width) =
            (CONTENT_SIZE.height - STATUS_HEIGHT, CONTENT_SIZE.width - SIDEBAR_WIDTH - INSPECTOR_WIDTH);
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
        let sketch = sketch_section(mtm, locale);
        let shapes = shapes_section(mtm, app.clone(), document, locale);
        let schikken = schikken_section(mtm, app.clone(), document, locale);
        let mut g5: Vec<G5Section> =
            crate::panels::G5Panel::ALL.iter().map(|p| g5_section(mtm, app.clone(), document, *p)).collect();
        let text = text_section(mtm, app.clone(), document, locale);
        let variable = variable_section(mtm, app.clone(), document, locale);
        let barcode = barcode_section(mtm, app.clone(), document, locale);
        let laser = laser_section(mtm, app.clone(), document, locale);
        let arrange_tools = arrange_toolbar(mtm, locale);

        // Right: inspector fields, message line, plan preview.
        let inspector = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, INSPECTOR_WIDTH, h));
        inspector.setAccessibilityLabel(Some(&NSString::from_str(host_text(HostText::InspectorLabel, locale))));
        let mut col = Column { mtm, view: &inspector, y: h - 8.0, width: INSPECTOR_WIDTH - 24.0 };
        let (mut fields, controls) = transform_block(&mut col, locale);
        fields.push(super::panels::priority::priority_row(&mut col, locale));
        let status_label = col.add_label("", 32.0);
        status_label.setTextColor(Some(&NSColor::secondaryLabelColor()));
        status_label.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
        col.y -= 12.0;
        // Two lines: "Plan van <datum tijd> · <duur>" is wider than the column.
        let plan_title = col.add_label("", 36.0);
        plan_title.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
        plan_title.setFont(Some(&NSFont::boldSystemFontOfSize(13.0)));
        // The plan id row: the id itself is not shown (Q4); "Kopieer id" copies it, and the GRBL
        // refusals pull-down sits at the right end.
        let plan_id = col.add_label("", 26.0);
        plan_id.setHidden(true);
        let estimate = col.add_label("", 18.0);
        let state = col.add_label("", 34.0);
        state.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
        let findings = col.add_label("", 18.0);
        // The measured values the plan used and the proofs they come from (W09).
        // One short line (the full text is its tooltip): the column has little room left under the plan.
        let plan_calibration = col.add_label("", 14.0);
        plan_calibration.setFont(Some(&NSFont::systemFontOfSize(10.0)));
        plan_calibration.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
        // The GRBL refusals share the plan-identity row: that label truncates in the middle anyway.
        let popup_width = 104.0;
        let id_frame = plan_id.frame();
        let (grbl_refusals, grbl_refusal_target) = super::panels::grbl_window::refusal_popup(
            mtm,
            app.clone(),
            document,
            rect(
                id_frame.origin.x + id_frame.size.width - popup_width,
                id_frame.origin.y,
                popup_width,
                id_frame.size.height,
            ),
        );
        inspector.addSubview(&grbl_refusals);

        let status_bar = status_bar_widgets(mtm, CONTENT_SIZE.width, locale);
        let objects_empty = EmptyOverlay::new(mtm, app.clone(), &scroll, 150.0);
        let operations_empty = EmptyOverlay::new(mtm, app.clone(), &operations.scroll, 260.0);
        let progress = ProgressWidgets::new(mtm, app.clone(), &status_bar.view, 290.0, STATUS_HEIGHT);
        let (status_coordinate, status_zoom, status_unit) =
            (status_bar.coordinate.clone(), status_bar.zoom.clone(), status_bar.unit.clone());
        let sidebar_pane: Retained<NSView> = Retained::into_super(scroll.clone());
        let canvas_pane: Retained<NSView> = Retained::into_super(centre.clone());
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
                cut_options: cut_settings.options.iter().map(|w| (w.field, w.control.clone())).collect(),
                camera_devices: camera.devices.clone(),
                sketch_table: sketch.table.clone(),
                sketch_params_table: sketch.params_table.clone(),
                param_inputs: sketch.param_inputs.clone(),
                sketch_tools: sketch.toolbar.clone(),
                sketch_tools_label: host_text(HostText::MenuSketch, locale).to_owned(),
                arrange_tools: arrange_tools.clone(),
                arrange_tools_label: host_text(HostText::MenuAlign, locale).to_owned(),
            },
        );
        let target: &AnyObject = controller.as_ref();
        let copy_id_button = button(
            mtm,
            crate::panels::copy_plan_id_title(locale),
            target,
            sel!(copyPlanId:),
            rect(id_frame.origin.x, id_frame.origin.y, 100.0, id_frame.size.height - 2.0),
        );
        copy_id_button.setEnabled(false);
        inspector.addSubview(&copy_id_button);
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
        let files = super::panels::files::files_section(
            mtm,
            &window,
            app.clone(),
            document,
            locale,
            &import.pages,
            &import.layers,
        );
        let image = image_sheets(mtm, target, app.clone(), document, locale);

        centre.addSubview(&canvas_view);
        centre.addSubview(&strip);
        centre.adjustSubviews();
        centre.setPosition_ofDividerAtIndex(h - STRIP_HEIGHT, 0);
        split.addSubview(&scroll);
        split.addSubview(&centre);
        split.setDelegate(Some(split_delegate(&controller)));
        // The inspector column carries the stack of the selection at its foot (G9-editor, E06) and
        // scrolls when the window is lower than the column.
        let inspector_column = super::panels::g5::embed_stack(mtm, &inspector, &mut g5, INSPECTOR_WIDTH, h);
        let inspector_pane: Retained<NSView> = Retained::into_super(inspector_column.clone());
        split.addSubview(&inspector_column);
        split.adjustSubviews();
        // The tool groups are the items of the window's toolbar; the content view holds the split
        // view and the status bar.
        let tool_strip = ToolStrip::new(mtm, app, document, target, locale);
        controller.set_panel_items(
            tool_strip
                .panel_item_ids()
                .into_iter()
                .filter_map(|id| tool_strip.panel_item(&id).map(|i| (id, i)))
                .collect(),
        );
        let container =
            NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, CONTENT_SIZE.width, CONTENT_SIZE.height));
        split.setFrame(rect(0.0, STATUS_HEIGHT, CONTENT_SIZE.width, h));
        split.setAutoresizingMask(NSAutoresizingMaskOptions(
            NSAutoresizingMaskOptions::ViewWidthSizable.0 | NSAutoresizingMaskOptions::ViewHeightSizable.0,
        ));
        container.addSubview(&split);
        container.addSubview(&status_bar.view);
        window.setContentView(Some(&container));
        window.setContentMinSize(MIN_CONTENT);

        for (_, f) in &fields {
            // SAFETY: the target is weak; DocWindow keeps the controller alive while the field
            // exists and clears the target in Drop.
            unsafe { f.setTarget(Some(target)) };
            // SAFETY: `fieldCommitted:` is a controller method taking the sender (`v@:@`).
            unsafe { f.setAction(Some(sel!(fieldCommitted:))) };
        }
        for b in &controls.reference {
            // SAFETY: as for the fields; `referencePoint:` takes the sender, its tag is the point's index.
            unsafe { b.setTarget(Some(target)) };
            // SAFETY: as above.
            unsafe { b.setAction(Some(sel!(referencePoint:))) };
        }
        // SAFETY: as above; `aspectLock:` takes the sender.
        unsafe { controls.aspect_lock.setTarget(Some(target)) };
        // SAFETY: as above.
        unsafe { controls.aspect_lock.setAction(Some(sel!(aspectLock:))) };
        for b in &controls.flip {
            // SAFETY: as above; the flip buttons use `panelButton:` (tag = the command's menu tag).
            unsafe { b.setTarget(Some(target)) };
            // SAFETY: as above.
            unsafe { b.setAction(Some(sel!(panelButton:))) };
        }
        // SAFETY: as above; `arrangeToolbar:` takes the sender.
        unsafe { arrange_tools.setTarget(Some(target)) };
        // SAFETY: as above.
        unsafe { arrange_tools.setAction(Some(sel!(arrangeToolbar:))) };
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
        for w in &cut_settings.options {
            w.control.set_target(Some(target));
        }
        for b in camera.buttons.iter().chain(std::iter::once(&camera.window_button)) {
            // SAFETY: as above.
            unsafe { b.setTarget(Some(target)) };
            // SAFETY: as above.
            unsafe { b.setAction(Some(sel!(cameraButton:))) };
        }
        for (_, b) in &sketch.buttons {
            // SAFETY: as above; the sketch panel's command buttons use `panelButton:` (tag = menu tag).
            unsafe { b.setTarget(Some(target)) };
            // SAFETY: as above.
            unsafe { b.setAction(Some(sel!(panelButton:))) };
        }
        for (b, action) in [
            (&sketch.problem, sel!(sketchProblemClicked:)),
            (&sketch.role, sel!(sketchRoleClicked:)),
            (&sketch.restore, sel!(sketchRestoreClicked:)),
            (&sketch.param_delete, sel!(sketchParameterDelete:)),
            (&sketch.param_set, sel!(sketchParameterSet:)),
        ] {
            // SAFETY: as above.
            unsafe { b.setTarget(Some(target)) };
            // SAFETY: as above.
            unsafe { b.setAction(Some(action)) };
        }
        // SAFETY: as above; `sketchToolbar:` takes the sender.
        unsafe { sketch.toolbar.setTarget(Some(target)) };
        // SAFETY: as above.
        unsafe { sketch.toolbar.setAction(Some(sel!(sketchToolbar:))) };
        for t in [&sketch.table, &sketch.params_table] {
            // SAFETY: data source and delegate are weak; cleared in Drop before the controller goes.
            unsafe { t.setDataSource(Some(table_source(&controller))) };
            // SAFETY: as above.
            unsafe { t.setDelegate(Some(table_delegate(&controller))) };
        }
        // SAFETY: data source and delegate are weak; cleared in Drop before the controller goes.
        unsafe { operations.table.setDataSource(Some(table_source(&controller))) };
        // SAFETY: as above.
        unsafe {
            operations.table.setDelegate(Some(objc2::runtime::ProtocolObject::from_ref(&*operations.layer_delegate)))
        };
        operations.layer_delegate.attach(&controller);
        // SAFETY: data source and delegate are weak; cleared in Drop before the controller goes.
        unsafe { table.setDataSource(Some(table_source(&controller))) };
        // SAFETY: as above.
        unsafe { table.setDelegate(Some(table_delegate(&controller))) };
        window.setDelegate(Some(window_delegate(&controller)));

        let toolbar =
            NSToolbar::initWithIdentifier(NSToolbar::alloc(mtm), &NSString::from_str("app.burnburn.document"));
        toolbar.setDisplayMode(NSToolbarDisplayMode::IconAndLabel);
        toolbar.setAllowsUserCustomization(true);
        // The toolbar writes its customisation to the user's defaults: only the app does that.
        toolbar.setAutosavesConfiguration(store.is_standard());
        // The delegate is weak; cleared in Drop.
        toolbar.setDelegate(Some(toolbar_delegate(&controller)));
        window.setToolbar(Some(&toolbar));

        // LF15: Tab runs object list, canvas, inspector, then the operations of the production strip.
        {
            let table_view: Retained<NSView> = Retained::into_super(Retained::into_super(table.clone()));
            let canvas: Retained<NSView> = Retained::into_super(canvas_view.clone());
            let mut stops = vec![table_view];
            // The empty object list's button ("Teken een rechthoek", LF17); AppKit skips it while hidden.
            if let Some(button) = objects_empty.key_stop() {
                stops.push(Retained::into_super(Retained::into_super(button)));
            }
            stops.push(canvas.clone());
            stops.extend(reading_stops(&inspector_pane));
            stops.extend(reading_stops(&strip));
            link_key_views(&stops);
            window.setInitialFirstResponder(Some(&canvas));
        }

        match &saved {
            Some(s) => {
                window.setFrame_display(rect(s.x, s.y, s.width, s.height), false);
                split.adjustSubviews();
                split.setPosition_ofDividerAtIndex(s.sidebar, 0);
                let total = split.frame().size.width;
                split.setPosition_ofDividerAtIndex(total - s.inspector - split.dividerThickness(), 1);
            }
            None => window.center(),
        }
        if options.quiet_windows {
            // Quiet native tests: in the window list (views lay out, events can be sent), but
            // invisible, click-through and without taking the focus from the user's app.
            super::quiet_window(&window);
            window.orderFrontRegardless();
        } else if options.float_windows {
            // A test process cannot take focus from the user's frontmost app; float the window
            // above it so it is really on screen (Metal presents nothing for an occluded window).
            window.setLevel(NSFloatingWindowLevel);
            window.orderFrontRegardless();
        } else {
            window.makeKeyAndOrderFront(None);
        }

        // The window is laid out now (toolbar, content rect): the column starts at its top (the
        // clip is clamped to the end of the content).
        inspector.scrollPoint(NSPoint::new(0.0, 1.0e7));

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
            pointer: None,
            length_unit: burnburn_host_models::format::LengthPreference::Millimeters,
            status_bar,
            objects_empty,
            operations_empty,
            progress,
            split,
            sidebar_pane,
            canvas_pane,
            inspector_pane,
            status_coordinate,
            status_zoom,
            status_unit,
            copy_id_button,
            table,
            toolbar,
            arrange_tools,
            strip: tool_strip,
            fields,
            controls,
            status_label,
            plan_labels: [plan_title, plan_id, estimate, state, findings],
            plan_calibration,
            grbl_refusals,
            grbl_refusal_target,
            compile_button,
            export_button,
            machine_labels: [machine_status, machine_message, machine_progress, machine_job],
            machine_buttons,
            import,
            image,
            operations,
            cut_settings,
            camera,
            sketch,
            shapes,
            schikken,
            g5,
            text,
            variable,
            barcode,
            laser,
            files,
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
        let title = burnburn_host_models::l10n_names::display_name(&window_title(vm, document), locale);
        if self.window.title().to_string() != title {
            self.window.setTitle(&NSString::from_str(&title));
        }
        self.window.setDocumentEdited(doc.dirty);

        let mut message = String::new();
        let mut tone = burnburn_host_models::style::StatusTone::Neutral;
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
                tone = burnburn_host_models::style::StatusTone::Error;
            }
            // The caption names the unit the field is shown in (display-unit preference).
            if let (Some(unit), Some(caption)) = (model.map(|f| f.unit), caption_of(field)) {
                let text = format!(
                    "{} ({})",
                    message_text(burnburn_presentation::MessageKey::Field(*key), locale),
                    crate::panels::field_unit_for(*key, unit, locale)
                );
                if caption.stringValue().to_string() != text {
                    caption.setStringValue(&NSString::from_str(&text));
                }
            }
        }
        if message.is_empty()
            && let Some(m) = doc.messages.last()
        {
            message = message_text(m.key, locale);
            tone = burnburn_host_models::style::message_tone(m.key);
        }
        if message.is_empty()
            && let Some(m) = &doc.tool_measurement
        {
            message = crate::toolbar::measurement_text(m, locale);
        }
        // The colour is never the only carrier: the symbol of the tone leads the line.
        let line = if message.is_empty() { message } else { format!("{} {message}", tone.glyph()) };
        set_text(&self.status_label, &line);
        // Only a failure is red; ordinary messages and measurements are secondary text.
        self.status_label.setTextColor(Some(&*match tone {
            burnburn_host_models::style::StatusTone::Neutral => NSColor::secondaryLabelColor(),
            burnburn_host_models::style::StatusTone::Warning => NSColor::systemOrangeColor(),
            burnburn_host_models::style::StatusTone::Error => NSColor::systemRedColor(),
        }));

        let panel = plan_panel(vm, document, locale);
        // The plan hash is not shown (Q4): its row stays empty and the title carries it as a tooltip.
        let no_text = String::new();
        for (label, text) in
            self.plan_labels.iter().zip([&panel.title, &no_text, &panel.estimate, &panel.state, &panel.findings])
        {
            set_text(label, text);
        }
        self.plan_labels[0]
            .setToolTip((!panel.id_tooltip.is_empty()).then(|| NSString::from_str(&panel.id_tooltip)).as_deref());
        set_text(&self.plan_calibration, &panel.calibration.join("; "));
        self.plan_calibration.setToolTip(Some(&NSString::from_str(&panel.calibration.join("\n"))));
        super::panels::grbl_window::refresh_refusals(&self.grbl_refusals, &panel, locale);
        let enabled = |k: CommandKey| vm.active == Some(document) && vm.commands.get(&k).is_some_and(|c| c.enabled);
        set_button_title(&self.copy_id_button, &panel.copy_id_title);
        self.copy_id_button.setEnabled(!panel.plan_id.is_empty());
        self.copy_id_button
            .setToolTip((!panel.id_tooltip.is_empty()).then(|| NSString::from_str(&panel.id_tooltip)).as_deref());
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
        self.refresh_inspector_controls(vm, document);
        self.refresh_arrange_toolbar(vm);
        self.refresh_import(vm, document);
        self.refresh_image(vm, document);
        self.refresh_operations(vm, document);
        self.refresh_cut_settings(vm, document);
        self.refresh_camera(camera);
        self.refresh_sketch(vm, document);
        self.refresh_shapes(vm, document);
        self.refresh_schikken(vm, document);
        self.refresh_g5(vm, document);
        self.refresh_text(vm, document);
        self.refresh_variable(vm, document);
        self.refresh_barcode(vm, document);
        self.refresh_files(vm, document);

        self.refresh_status_bar(doc);
        self.progress.show(
            &burnburn_host_models::empty_state::progress_rows(doc, locale),
            burnburn_host_models::l10n_empty::empty_text(
                burnburn_host_models::l10n_empty::EmptyText::TaskCancel,
                locale,
            ),
        );

        // The view model's toolbar is the active document's: another window keeps what it had.
        if vm.active == Some(document) {
            self.strip.refresh(&vm.toolbar);
        }

        if self.controller.set_labels(
            object_labels(vm, document)
                .iter()
                .map(|l| burnburn_host_models::l10n_names::display_name(l, locale))
                .collect(),
        ) {
            let table = self.table.clone();
            if let Err(e) = super::objc_try("reloading the object table", || table.reloadData()) {
                eprintln!("burnburn: {e}");
            }
        }
        self.objects_empty.show(burnburn_host_models::empty_state::object_list_empty(vm, document, locale).as_ref());
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

    /// The status bar: the pointer's coordinate (once it was over the canvas), zoom and unit.
    pub(crate) fn refresh_status_bar(&self, doc: &burnburn_presentation::DocumentViewModel) {
        let bar = crate::panels::status_bar(self.pointer, &doc.camera, &doc.viewport, self.length_unit, self.locale);
        self.status_bar.show(&bar);
    }

    /// The window's frame and pane widths, for saving.
    pub(crate) fn saved_state(&self) -> burnburn_host_models::window_state::SavedWindow {
        let frame = self.window.frame();
        burnburn_host_models::window_state::SavedWindow {
            x: frame.origin.x,
            y: frame.origin.y,
            width: frame.size.width,
            height: frame.size.height,
            sidebar: self.sidebar_pane.frame().size.width,
            inspector: self.inspector_pane.frame().size.width,
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

impl Drop for DocWindow {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // Clearing weak references is always valid; afterwards nothing refers to the controller.
        self.strip.clear_targets();
        for (_, f) in &self.fields {
            // SAFETY: a nil target cannot dangle.
            unsafe { f.setTarget(None) };
        }
        for b in self.controls.reference.iter().chain(self.controls.flip.iter()) {
            // SAFETY: a nil target cannot dangle.
            unsafe { b.setTarget(None) };
        }
        // SAFETY: as above.
        unsafe { self.controls.aspect_lock.setTarget(None) };
        // SAFETY: as above.
        unsafe { self.arrange_tools.setTarget(None) };
        // SAFETY: as above.
        unsafe { self.import.units.setTarget(None) };
        // SAFETY: as above.
        unsafe { self.cut_settings.mode.setTarget(None) };
        for w in &self.cut_settings.options {
            w.control.set_target(None);
        }
        self.image.clear_targets();
        self.objects_empty.clear_target();
        self.operations_empty.clear_target();
        self.progress.clear_target();
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
        .chain(std::iter::once(&self.camera.window_button))
        .chain(self.operations.palette.iter())
        {
            // SAFETY: as above.
            unsafe { b.setTarget(None) };
        }
        self.toolbar.setDelegate(None);
        self.split.setDelegate(None);
        // SAFETY: a nil target cannot dangle.
        unsafe { self.copy_id_button.setTarget(None) };
        for (_, b) in &self.sketch.buttons {
            // SAFETY: as above.
            unsafe { b.setTarget(None) };
        }
        for b in [
            &self.sketch.problem,
            &self.sketch.role,
            &self.sketch.restore,
            &self.sketch.param_delete,
            &self.sketch.param_set,
        ] {
            // SAFETY: as above.
            unsafe { b.setTarget(None) };
        }
        // SAFETY: as above.
        unsafe { self.sketch.toolbar.setTarget(None) };
        for t in [&self.sketch.table, &self.sketch.params_table] {
            // SAFETY: nil data source / delegate cannot dangle.
            unsafe { t.setDataSource(None) };
            // SAFETY: as above.
            unsafe { t.setDelegate(None) };
        }
        if self.sketch.shown {
            self.sketch.panel.orderOut(None);
        }
        self.laser.clear();
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
        self.files.clear_targets();
        self.files.end_all(&self.window);
        self.shapes.clear_targets();
        self.shapes.hide(&self.window);
        self.schikken.clear_targets();
        self.schikken.hide();
        for section in &mut self.g5 {
            section.clear_targets();
            section.hide();
        }
        self.text.clear_targets();
        self.text.hide(&self.window);
        self.variable.clear_targets();
        self.variable.hide();
        self.barcode.clear_targets();
        self.barcode.hide(&self.window);
        self.image.end_all(&self.window);
        if !self.closing {
            self.controller.allow_close();
            self.window.close();
        }
    }
}

/// Whether Tab can land on `view` (hidden views count: they come and go, AppKit skips them while
/// they are hidden or disabled).
fn is_tab_stop(view: &NSView) -> bool {
    let any: &AnyObject = view.as_ref();
    if let Some(t) = any.downcast_ref::<NSTextField>() {
        return t.isEditable();
    }
    if let Some(t) = any.downcast_ref::<objc2_app_kit::NSTextView>() {
        return t.isEditable();
    }
    any.downcast_ref::<NSButton>().is_some()
        || any.downcast_ref::<objc2_app_kit::NSPopUpButton>().is_some()
        || any.downcast_ref::<NSSegmentedControl>().is_some()
        || any.downcast_ref::<NSTableView>().is_some()
}

/// The tab stops under `pane` in reading order: lines from the top, left to right within a line.
/// Positions are taken in the pane's own document (a scroll view's document view), so how far the
/// pane is scrolled does not matter.
#[allow(unsafe_code)]
fn reading_stops(pane: &NSView) -> Vec<Retained<NSView>> {
    fn collect(view: &NSView, out: &mut Vec<Retained<NSView>>) {
        for child in view.subviews().iter() {
            let any: &AnyObject = child.as_ref();
            // A field being edited holds its field editor; a text view of a memo is a stop itself
            // but the scroll views and clip views around it are not.
            if is_tab_stop(&child) {
                out.push(child.clone());
            }
            if any.downcast_ref::<NSTextField>().is_none() && any.downcast_ref::<NSTableView>().is_none() {
                collect(&child, out);
            }
        }
    }
    let mut views = Vec::new();
    collect(pane, &mut views);
    let space: Retained<NSView> =
        pane.downcast_ref::<NSScrollView>().and_then(|s| s.documentView()).unwrap_or_else(|| pane.retain());
    let mut placed: Vec<(f64, f64, Retained<NSView>)> = views
        .into_iter()
        .map(|v| {
            let r = v.convertRect_toView(v.bounds(), Some(&space));
            let mid = r.origin.y + r.size.height / 2.0;
            (if space.isFlipped() { mid } else { -mid }, r.origin.x, v)
        })
        .collect();
    placed.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut lines: Vec<Vec<(f64, f64, Retained<NSView>)>> = Vec::new();
    for p in placed {
        match lines.last_mut() {
            Some(line) if line.first().is_some_and(|f| (f.0 - p.0).abs() < 9.0) => line.push(p),
            _ => lines.push(vec![p]),
        }
    }
    for line in &mut lines {
        line.sort_by(|a, b| a.1.total_cmp(&b.1));
    }
    lines.into_iter().flatten().map(|(_, _, v)| v).collect()
}

/// Links `stops` into a loop with `nextKeyView`, the last back to the first.
#[allow(unsafe_code)]
fn link_key_views(stops: &[Retained<NSView>]) {
    let (Some(first), Some(last)) = (stops.first(), stops.last()) else { return };
    for pair in stops.windows(2) {
        // SAFETY: the links are unretained; every stop is a view of this window, which keeps its
        // views alive as long as the window exists.
        unsafe { pair[0].setNextKeyView(Some(&pair[1])) };
    }
    // SAFETY: as above.
    unsafe { last.setNextKeyView(Some(first)) };
}

/// The caption label above an inspector field (same container, tag `CAPTION_TAG_BASE + field tag`).
#[allow(unsafe_code)]
fn caption_of(field: &NSTextField) -> Option<Retained<NSTextField>> {
    // SAFETY: reading the superview of our own live field; nothing is mutated or retained beyond
    // the returned label.
    let container = unsafe { field.superview() }?;
    container
        .viewWithTag(super::panels::inspector::CAPTION_TAG_BASE + field.tag())
        .and_then(|v| v.downcast::<NSTextField>().ok())
}
