//! One document window: tool strip (icons per catalog tool), Schets toolbar item, split view with the object table, the canvas
//! view above the production strip (operations panel, bed camera) and the inspector (X/Y/B/H,
//! rotation, messages, plan preview with Compile/Export, machine). Builds the widgets and copies
//! the view model into them; it never decides anything.

use std::rc::Weak;

use burnburn_presentation::{AppViewModel, CameraViewModel, CommandKey, FieldKey, MachineViewModel};
use burnburn_types::DocumentId;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly, sel};
use objc2_app_kit::{
    NSAccessibility, NSAlert, NSAutoresizingMaskOptions, NSBackingStoreType, NSButton, NSColor, NSFloatingWindowLevel,
    NSFont, NSLineBreakMode, NSSavePanel, NSScrollView, NSSegmentedControl, NSSplitView, NSSplitViewDividerStyle,
    NSTableColumn, NSTableView, NSTextField, NSToolbar, NSToolbarDisplayMode, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSIndexSet, NSMutableIndexSet, NSSize, NSString};

use super::app::{HostApp, HostOptions};
use super::canvas::Canvas;
use super::classes::{
    CanvasView, ControllerParts, WindowController, table_delegate, table_source, toolbar_delegate, window_delegate,
};
use super::panels::arrange::arrange_toolbar;
use super::panels::barcode::{BarcodeSection, barcode_section};
use super::panels::camera::CameraSection;
use super::panels::cut_settings::{CutSettingsSheet, cut_settings_sheet};
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
use super::toolstrip::{STRIP_HEIGHT as TOOL_STRIP_HEIGHT, ToolStrip};
use crate::adapter::{message_text, object_labels, selected_rows as vm_selected_rows, window_title};
use crate::l10n::{HostText, Locale, host_text};
use crate::panels::{machine_panel, plan_panel};

const CONTENT_SIZE: NSSize = NSSize { width: 1100.0, height: 800.0 };
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
        let sketch = sketch_section(mtm, locale);
        let shapes = shapes_section(mtm, app.clone(), document, locale);
        let schikken = schikken_section(mtm, app.clone(), document, locale);
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
        let files = super::panels::files::files_section(mtm, &window, app.clone(), document, locale, &import.pages);
        let image = image_sheets(mtm, target, app.clone(), document, locale);

        centre.addSubview(&canvas_view);
        centre.addSubview(&strip);
        centre.adjustSubviews();
        centre.setPosition_ofDividerAtIndex(h - STRIP_HEIGHT, 0);
        split.addSubview(&scroll);
        split.addSubview(&centre);
        split.addSubview(&inspector);
        split.adjustSubviews();
        // The tool strip sits above the split view; the content view holds both.
        let tool_strip = ToolStrip::new(mtm, app, document, target, locale, CONTENT_SIZE.width);
        tool_strip.view().setFrame(rect(0.0, h, CONTENT_SIZE.width, TOOL_STRIP_HEIGHT));
        let container =
            NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, CONTENT_SIZE.width, h + TOOL_STRIP_HEIGHT));
        split.setFrame(rect(0.0, 0.0, CONTENT_SIZE.width, h));
        split.setAutoresizingMask(NSAutoresizingMaskOptions(
            NSAutoresizingMaskOptions::ViewWidthSizable.0 | NSAutoresizingMaskOptions::ViewHeightSizable.0,
        ));
        container.addSubview(&split);
        container.addSubview(tool_strip.view());
        window.setContentView(Some(&container));

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
        unsafe { operations.table.setDelegate(Some(table_delegate(&controller))) };
        // SAFETY: data source and delegate are weak; cleared in Drop before the controller goes.
        unsafe { table.setDataSource(Some(table_source(&controller))) };
        // SAFETY: as above.
        unsafe { table.setDelegate(Some(table_delegate(&controller))) };
        window.setDelegate(Some(window_delegate(&controller)));

        let toolbar =
            NSToolbar::initWithIdentifier(NSToolbar::alloc(mtm), &NSString::from_str("app.burnburn.document"));
        toolbar.setDisplayMode(NSToolbarDisplayMode::IconAndLabel);
        // The delegate is weak; cleared in Drop.
        toolbar.setDelegate(Some(toolbar_delegate(&controller)));
        window.setToolbar(Some(&toolbar));

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
            toolbar,
            arrange_tools,
            strip: tool_strip,
            fields,
            controls,
            status_label,
            plan_labels: [plan_title, plan_id, estimate, state, findings],
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
        self.refresh_text(vm, document);
        self.refresh_variable(vm, document);
        self.refresh_barcode(vm, document);
        self.refresh_files(vm, document);

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
