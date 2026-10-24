//! The Objective-C classes AppKit calls back into. Every callback reduces its arguments to a
//! [`NativeEvent`] (plain data) and hands it to the [`HostApp`]; no callback decides what an
//! event means or touches the document.
//!
//! Invariants (why the `unsafe` in this module is sound):
//! - All classes are `MainThreadOnly`: AppKit calls them on the main thread only, and objc2 only
//!   lets Rust create them with a `MainThreadMarker`.
//! - Every method's Rust signature matches its selector's type encoding (checked by objc2's
//!   `define_class!` against the declared protocol methods; free methods take `&AnyObject`/`&NSEvent`
//!   and return `()`/`bool` as AppKit sends them). Senders are downcast, never assumed.
//! - No callback lets a panic unwind into AppKit: each body runs in [`ffi_guard`] with a default
//!   return value. Objective-C exceptions raised by AppKit calls the host makes are caught at the
//!   call site with [`super::objc_try`].
//! - Ivars hold only a `Weak<HostApp>`, the document id and Rust state behind `RefCell`/`Cell`;
//!   callbacks re-borrow with `try_borrow*` so a re-entrant AppKit callback is dropped instead
//!   of aliasing. After the window closes the host drops its `DocWindow`; a late callback finds
//!   the session gone (`Weak` upgrade fails or the document is no longer in the view model) and does nothing.
//! - `msg_send![super(..)]` calls only pass through AppKit's own implementation with the same
//!   arguments, on an initialised instance.

use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::rc::{Rc, Weak};

use burnburn_presentation::{CommandKey, CutField, FieldKey};
use burnburn_types::DocumentId;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject, Sel};
use objc2::{AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAccessibility, NSAccessibilityElement, NSAccessibilityLayoutAreaRole, NSAccessibilityLayoutItemRole,
    NSApplication, NSApplicationDelegate, NSApplicationTerminateReply, NSButton, NSControlStateValueOff,
    NSControlStateValueOn, NSControlTextEditingDelegate, NSEvent, NSEventType, NSMenuItem, NSMenuItemValidation,
    NSPopUpButton, NSResponder, NSSegmentedControl, NSSplitView, NSSplitViewDelegate, NSTableColumn, NSTableView,
    NSTableViewDataSource, NSTableViewDelegate, NSTextField, NSTextInputClient, NSToolbar,
    NSToolbarDelegate, NSToolbarItem, NSTrackingArea, NSTrackingAreaOptions, NSView, NSWindow, NSWindowDelegate,
};
use objc2_foundation::{
    NSArray, NSAttributedString, NSAttributedStringKey, NSInteger, NSNotFound, NSNotification, NSNumber, NSPoint,
    NSRange, NSRangePointer, NSRect, NSSize, NSString, NSUInteger, NSURL,
};

use super::app::HostApp;
use super::canvas::Canvas;
use super::panels::cut_settings::OptionControl;
use super::toolstrip::PanelItem;
use crate::adapter::{
    CommitCause, MouseButton, MousePhase, NativeEvent, menu_checked, menu_enabled_with_recent, menu_item, menu_title,
};
use crate::l10n::{HostText, host_text};
use crate::panels::{LayerColumn, OperationLine, SketchColumn, SketchLine};
use crate::text_input::Composer;

/// Run a callback body; a panic stops here (logged) instead of unwinding through AppKit's frames.
/// The event is dropped, `default` returned, and the host is asked (on the main queue, after this
/// callback) to autosave what changed into recovery files (ADR 0002's recovery route).
pub(crate) fn ffi_guard<T>(what: &str, default: T, body: impl FnOnce() -> T) -> T {
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(v) => v,
        Err(_) => {
            eprintln!("burnburn: internal error in {what}; the event was dropped, unsaved work is autosaved");
            super::app::request_emergency_autosave();
            default
        }
    }
}

/// Identifier of the toolbar item with the Schets (sketch) commands.
pub(crate) const SKETCH_ITEM: &str = "app.burnburn.toolbar.sketch";
/// Identifier of the toolbar item with the Uitlijnen (align and distribute) commands.
pub(crate) const ARRANGE_ITEM: &str = "app.burnburn.toolbar.arrange";

pub(crate) struct AppIvars {
    app: RefCell<Weak<HostApp>>,
}

impl AppIvars {
    fn new() -> Self {
        AppIvars { app: RefCell::new(Weak::new()) }
    }

    fn app(&self) -> Option<Rc<HostApp>> {
        self.app.try_borrow().ok()?.upgrade()
    }

    fn set(&self, app: Weak<HostApp>) {
        if let Ok(mut slot) = self.app.try_borrow_mut() {
            *slot = app;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Menu target (one per app): every main-menu item sends `burnburnMenu:` here; its tag says which.
// ---------------------------------------------------------------------------------------------

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnMenuTarget"]
    #[ivars = AppIvars]
    pub(crate) struct MenuTarget;

    impl MenuTarget {
        #[unsafe(method(burnburnMenu:))]
        fn menu(&self, sender: Option<&AnyObject>) {
            ffi_guard("burnburnMenu:", (), || {
                let Some(item) = sender.and_then(|s| s.downcast_ref::<NSMenuItem>()).and_then(|i| menu_item(i.tag()))
                else {
                    return;
                };
                if let Some(app) = self.ivars().app() {
                    app.native(NativeEvent::Menu(item));
                }
            });
        }
    }

    unsafe impl NSObjectProtocol for MenuTarget {}

    unsafe impl NSMenuItemValidation for MenuTarget {
        #[unsafe(method(validateMenuItem:))]
        fn validate_menu_item(&self, menu_item_obj: &NSMenuItem) -> bool {
            ffi_guard("validateMenuItem:", false, || {
                let (Some(item), Some(app)) = (menu_item(menu_item_obj.tag()), self.ivars().app()) else {
                    return false;
                };
                let vm = app.view();
                // Recent-project entries keep the file name they were built with.
                let title = menu_title(item, &vm, app.locale());
                if !title.is_empty() {
                    menu_item_obj.setTitle(&NSString::from_str(&title));
                }
                menu_item_obj.setState(if menu_checked(item, &vm) || app.app_menu_checked(item) { NSControlStateValueOn } else { NSControlStateValueOff });
                menu_enabled_with_recent(item, &vm, app.recent_count())
            })
        }
    }
);

impl MenuTarget {
    pub(crate) fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(AppIvars::new());
        // SAFETY: NSObject's designated initialiser on a freshly allocated instance with ivars set.
        unsafe { msg_send![super(this), init] }
    }

    pub(crate) fn set_app(&self, app: Weak<HostApp>) {
        self.ivars().set(app);
    }
}

// ---------------------------------------------------------------------------------------------
// Application delegate: Finder/Dock opens, launch, reopen and quit with dirty documents.
// ---------------------------------------------------------------------------------------------

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnAppDelegate"]
    #[ivars = AppIvars]
    pub(crate) struct AppDelegate;

    unsafe impl NSObjectProtocol for AppDelegate {}

    unsafe impl NSApplicationDelegate for AppDelegate {
        /// Dirty documents: ask each (Later); nothing to lose: Now. A failure keeps the app (Cancel).
        #[unsafe(method(applicationShouldTerminate:))]
        fn application_should_terminate(&self, _sender: &NSApplication) -> NSApplicationTerminateReply {
            ffi_guard("applicationShouldTerminate:", NSApplicationTerminateReply::TerminateCancel, || {
                match self.ivars().app() {
                    Some(app) => app.should_terminate(),
                    None => NSApplicationTerminateReply::TerminateNow,
                }
            })
        }

        #[unsafe(method(application:openURLs:))]
        fn application_open_urls(&self, _application: &NSApplication, urls: &NSArray<NSURL>) {
            ffi_guard("application:openURLs:", (), || {
                let paths: Vec<PathBuf> = urls.iter().filter_map(|u| u.to_file_path()).collect();
                if let Some(app) = self.ivars().app() {
                    app.open_paths(paths);
                }
            });
        }

        #[unsafe(method(applicationDidFinishLaunching:))]
        fn application_did_finish_launching(&self, _notification: &NSNotification) {
            ffi_guard("applicationDidFinishLaunching:", (), || {
                if let Some(app) = self.ivars().app() {
                    app.did_finish_launching();
                }
            });
        }

        /// Dock click with no window open: a new untitled document, as document apps do.
        #[unsafe(method(applicationShouldHandleReopen:hasVisibleWindows:))]
        fn application_should_handle_reopen(&self, _sender: &NSApplication, has_visible_windows: bool) -> bool {
            ffi_guard("applicationShouldHandleReopen:hasVisibleWindows:", true, || {
                if !has_visible_windows && let Some(app) = self.ivars().app() {
                    app.dispatch(burnburn_presentation::UiIntent::NewDocument);
                }
                true
            })
        }
    }
);

impl AppDelegate {
    pub(crate) fn new(mtm: MainThreadMarker, app: Weak<HostApp>) -> Retained<Self> {
        let ivars = AppIvars::new();
        ivars.set(app);
        let this = Self::alloc(mtm).set_ivars(ivars);
        // SAFETY: NSObject's designated initialiser on a freshly allocated instance with ivars set.
        unsafe { msg_send![super(this), init] }
    }
}

/// The delegate object as the protocol `NSApplication.setDelegate:` expects.
pub(crate) fn app_delegate(delegate: &AppDelegate) -> &ProtocolObject<dyn NSApplicationDelegate> {
    ProtocolObject::from_ref(delegate)
}

// ---------------------------------------------------------------------------------------------
// Window controller (one per document window): window delegate, field/button/tool target, table
// source, toolbar delegate.
// ---------------------------------------------------------------------------------------------

/// The widgets a window controller reads when its targets fire.
pub(crate) struct ControllerParts {
    pub(crate) app: Weak<HostApp>,
    pub(crate) document: DocumentId,
    pub(crate) fields: Vec<(FieldKey, Retained<NSTextField>)>,
    pub(crate) operations_table: Retained<NSTableView>,
    /// Name, speed, power of a new operation.
    pub(crate) operation_inputs: [Retained<NSTextField>; 3],
    pub(crate) operation_kind: Retained<NSPopUpButton>,
    /// Cut Settings sheet: mode pop-up, the five fields, Output and Visible checkboxes.
    pub(crate) cut_mode: Retained<NSPopUpButton>,
    pub(crate) cut_fields: [Retained<NSTextField>; 5],
    pub(crate) cut_toggles: [Retained<NSButton>; 2],
    /// Cut Settings sheet: the option fields (tabs, leads, fill and image options) and their controls.
    pub(crate) cut_options: Vec<(CutField, OptionControl)>,
    pub(crate) camera_devices: Retained<NSPopUpButton>,
    /// Sketch panel: constraint table, parameter table and the new parameter's name and expression.
    pub(crate) sketch_table: Retained<NSTableView>,
    pub(crate) sketch_params_table: Retained<NSTableView>,
    pub(crate) param_inputs: [Retained<NSTextField>; 2],
    /// Toolbar segments of the Schets commands.
    pub(crate) sketch_tools: Retained<NSSegmentedControl>,
    pub(crate) sketch_tools_label: String,
    /// Toolbar segments of the Uitlijnen commands.
    pub(crate) arrange_tools: Retained<NSSegmentedControl>,
    pub(crate) arrange_tools_label: String,
}

pub(crate) struct ControllerIvars {
    app: Weak<HostApp>,
    document: DocumentId,
    fields: Vec<(FieldKey, Retained<NSTextField>)>,
    operations_table: Retained<NSTableView>,
    operation_inputs: [Retained<NSTextField>; 3],
    operation_kind: Retained<NSPopUpButton>,
    cut_mode: Retained<NSPopUpButton>,
    cut_fields: [Retained<NSTextField>; 5],
    cut_toggles: [Retained<NSButton>; 2],
    cut_options: Vec<(CutField, OptionControl)>,
    camera_devices: Retained<NSPopUpButton>,
    sketch_table: Retained<NSTableView>,
    sketch_params_table: Retained<NSTableView>,
    param_inputs: [Retained<NSTextField>; 2],
    sketch_tools: Retained<NSSegmentedControl>,
    sketch_tools_label: String,
    arrange_tools: Retained<NSSegmentedControl>,
    arrange_tools_label: String,
    /// The ribbon panels as toolbar items (identifier, content), set once the strip exists.
    panel_items: RefCell<Vec<(String, PanelItem)>>,
    /// Layer-table rows, copied from the view model.
    operation_rows: RefCell<Vec<OperationLine>>,
    /// Constraint-table and parameter-table rows, copied from the view model.
    sketch_rows: RefCell<Vec<SketchLine>>,
    sketch_params: RefCell<Vec<(String, String)>>,
    /// Object names shown by the table (copied from the view model on refresh).
    labels: RefCell<Vec<String>>,
    /// True while the host itself changes the table selection (no intent for that).
    syncing: Cell<bool>,
    /// The Metal canvas; dropped when the window closes (surface before view).
    canvas: RefCell<Option<Canvas>>,
    /// Set once presentation agreed to close: `windowShouldClose:` then says yes.
    may_close: Cell<bool>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnWindowController"]
    #[ivars = ControllerIvars]
    pub(crate) struct WindowController;

    impl WindowController {
        /// Target/action of the inspector fields: Return, and end of editing (Tab, click away).
        #[unsafe(method(fieldCommitted:))]
        fn field_committed(&self, sender: Option<&AnyObject>) {
            ffi_guard("fieldCommitted:", (), || {
                let Some(field) = sender.and_then(|s| s.downcast_ref::<NSTextField>()) else { return };
                let ivars = self.ivars();
                let Some(key) = ivars.fields.iter().find(|(_, f)| std::ptr::eq(field, &**f)).map(|(k, _)| *k) else {
                    return;
                };
                let cause = match MainThreadMarker::new().and_then(|m| NSApplication::sharedApplication(m).currentEvent()) {
                    Some(e) if e.r#type() == NSEventType::KeyDown && e.keyCode() == 36 => CommitCause::Return,
                    _ => CommitCause::EndEditing,
                };
                let text = field.stringValue().to_string();
                self.send(NativeEvent::FieldCommitted { document: ivars.document, field: key, text, cause });
            });
        }

        /// A tool button of the strip: its tag is the catalog index of the tool it shows.
        #[unsafe(method(toolButton:))]
        fn tool_button(&self, sender: Option<&AnyObject>) {
            ffi_guard("toolButton:", (), || {
                // A button of the ribbon, or an entry of a panel's overflow menu in the toolbar.
                let tag = match sender {
                    Some(s) if s.downcast_ref::<NSButton>().is_some() => s.downcast_ref::<NSButton>().map(|b| b.tag()),
                    Some(s) => s.downcast_ref::<NSMenuItem>().map(|i| i.tag()),
                    None => None,
                };
                if let Some(spec) = tag.and_then(crate::toolbar::spec_for_tag) {
                    self.send(NativeEvent::ToolButton { document: self.ivars().document, id: spec.id.to_owned() });
                }
            });
        }

        /// "Kopieer id" at the plan: the plan id goes on the pasteboard as text.
        #[unsafe(method(copyPlanId:))]
        fn copy_plan_id(&self, _sender: Option<&AnyObject>) {
            ffi_guard("copyPlanId:", (), || {
                if let Some(app) = self.ivars().app.upgrade() {
                    app.copy_plan_id(self.ivars().document);
                }
            });
        }

        /// The arrow next to a group's button: open the drop-down of its variants.
        #[unsafe(method(variantMenu:))]
        fn variant_menu(&self, sender: Option<&AnyObject>) {
            ffi_guard("variantMenu:", (), || {
                let Some(button) = sender.and_then(|s| s.downcast_ref::<NSButton>()) else { return };
                let Some(group) = crate::toolbar::spec_for_tag(button.tag()).and_then(|s| s.group) else { return };
                if let Some(app) = self.ivars().app.upgrade() {
                    app.show_variant_menu(self.ivars().document, group, button);
                }
            });
        }

        /// An entry of a variant drop-down: its tag is the catalog index of the variant.
        #[unsafe(method(variantChosen:))]
        fn variant_chosen(&self, sender: Option<&AnyObject>) {
            ffi_guard("variantChosen:", (), || {
                let Some(item) = sender.and_then(|s| s.downcast_ref::<NSMenuItem>()) else { return };
                let Some(spec) = crate::toolbar::spec_for_tag(item.tag()) else { return };
                if let Some(group) = spec.group {
                    self.send(NativeEvent::VariantChosen {
                        document: self.ivars().document,
                        group: group.to_owned(),
                        id: spec.id.to_owned(),
                    });
                }
            });
        }

        /// Panel buttons (Compileer, Exporteer RD…): the tag is the command's menu tag.
        #[unsafe(method(panelButton:))]
        fn panel_button(&self, sender: Option<&AnyObject>) {
            ffi_guard("panelButton:", (), || {
                let Some(button) = sender.and_then(|s| s.downcast_ref::<NSButton>()) else { return };
                if let Some(crate::adapter::MenuItem::Command(command)) = menu_item(button.tag()) {
                    self.send(NativeEvent::PanelCommand { document: self.ivars().document, command });
                }
            });
        }
    }

    impl WindowController {
        /// The import dialog's unit pop-up.
        #[unsafe(method(importUnitChosen:))]
        fn import_unit_chosen(&self, sender: Option<&AnyObject>) {
            ffi_guard("importUnitChosen:", (), || {
                let Some(popup) = sender.and_then(|s| s.downcast_ref::<objc2_app_kit::NSPopUpButton>()) else { return };
                self.send(NativeEvent::ImportUnitChosen { document: self.ivars().document, index: popup.indexOfSelectedItem() });
            });
        }

        /// The operations panel's Add button: the typed texts go to presentation unparsed.
        #[unsafe(method(addOperation:))]
        fn add_operation(&self, _sender: Option<&AnyObject>) {
            ffi_guard("addOperation:", (), || {
                let ivars = self.ivars();
                let [name, speed, power] = ivars.operation_inputs.each_ref().map(|f| f.stringValue().to_string());
                self.send(NativeEvent::AddOperationRequested {
                    document: ivars.document,
                    name,
                    kind_index: ivars.operation_kind.indexOfSelectedItem(),
                    speed,
                    power,
                });
            });
        }

        /// The sketch panel's conflict message: select the constraints it names.
        #[unsafe(method(sketchProblemClicked:))]
        fn sketch_problem_clicked(&self, _sender: Option<&AnyObject>) {
            ffi_guard("sketchProblemClicked:", (), || {
                self.send(NativeEvent::SketchProblemClicked { document: self.ivars().document });
            });
        }

        /// The sketch panel's Meten/Sturen button.
        #[unsafe(method(sketchRoleClicked:))]
        fn sketch_role_clicked(&self, _sender: Option<&AnyObject>) {
            ffi_guard("sketchRoleClicked:", (), || {
                self.send(NativeEvent::SketchRoleRequested { document: self.ivars().document });
            });
        }

        /// The sketch panel's "Herstel laatste geldige" button.
        #[unsafe(method(sketchRestoreClicked:))]
        fn sketch_restore_clicked(&self, _sender: Option<&AnyObject>) {
            ffi_guard("sketchRestoreClicked:", (), || {
                self.send(NativeEvent::SketchRestoreRequested { document: self.ivars().document });
            });
        }

        /// The sketch panel's "Verwijder parameter" button: the selected row of the parameter table.
        #[unsafe(method(sketchParameterDelete:))]
        fn sketch_parameter_delete(&self, _sender: Option<&AnyObject>) {
            ffi_guard("sketchParameterDelete:", (), || {
                let ivars = self.ivars();
                let rows = super::panels::widgets::selected_rows(&ivars.sketch_params_table);
                if let Some(row) = rows.first() {
                    self.send(NativeEvent::SketchParameterRemoveRequested { document: ivars.document, row: *row });
                }
            });
        }

        /// The sketch panel's Zet parameter button: name and expression go unparsed.
        #[unsafe(method(sketchParameterSet:))]
        fn sketch_parameter_set(&self, _sender: Option<&AnyObject>) {
            ffi_guard("sketchParameterSet:", (), || {
                let ivars = self.ivars();
                let [name, text] = ivars.param_inputs.each_ref().map(|f| f.stringValue().to_string());
                self.send(NativeEvent::SketchParameterCommitted { document: ivars.document, name, text });
            });
        }

        /// One of the inspector's reference-point buttons: its tag is the index in `RefPoint::ALL`.
        #[unsafe(method(referencePoint:))]
        fn reference_point(&self, sender: Option<&AnyObject>) {
            ffi_guard("referencePoint:", (), || {
                let Some(button) = sender.and_then(|s| s.downcast_ref::<NSButton>()) else { return };
                self.send(NativeEvent::ReferencePointChosen { document: self.ivars().document, index: button.tag() });
            });
        }

        /// The inspector's aspect-lock checkbox.
        #[unsafe(method(aspectLock:))]
        fn aspect_lock(&self, _sender: Option<&AnyObject>) {
            ffi_guard("aspectLock:", (), || {
                self.send(NativeEvent::AspectLockToggled { document: self.ivars().document });
            });
        }

        /// The toolbar's Uitlijnen control (momentary segments).
        #[unsafe(method(arrangeToolbar:))]
        fn arrange_toolbar(&self, sender: Option<&AnyObject>) {
            ffi_guard("arrangeToolbar:", (), || {
                let Some(control) = sender.and_then(|s| s.downcast_ref::<NSSegmentedControl>()) else { return };
                if let Some(command) = crate::panels::arrange_toolbar_command(control.selectedSegment()) {
                    self.send(NativeEvent::PanelCommand { document: self.ivars().document, command });
                }
            });
        }

        /// The toolbar's Schets control (momentary segments).
        #[unsafe(method(sketchToolbar:))]
        fn sketch_toolbar(&self, sender: Option<&AnyObject>) {
            ffi_guard("sketchToolbar:", (), || {
                let Some(control) = sender.and_then(|s| s.downcast_ref::<NSSegmentedControl>()) else { return };
                if let Some(command) = crate::panels::sketch_toolbar_command(control.selectedSegment()) {
                    self.send(NativeEvent::PanelCommand { document: self.ivars().document, command });
                }
            });
        }

        /// A palette colour: the tag is its slot; Shift (from the click event) adds a treatment.
        #[unsafe(method(paletteClicked:))]
        fn palette_clicked(&self, sender: Option<&AnyObject>) {
            ffi_guard("paletteClicked:", (), || {
                let Some(button) = sender.and_then(|s| s.downcast_ref::<NSButton>()) else { return };
                // Only the click's own mouse event decides Shift (never a leftover or unrelated event).
                let event = MainThreadMarker::new().and_then(|m| NSApplication::sharedApplication(m).currentEvent());
                let flags = crate::adapter::click_modifier_flags(
                    event.as_ref().map(|e| e.r#type().0),
                    event.as_ref().map_or(0, |e| e.modifierFlags().0),
                );
                self.send(NativeEvent::PaletteClicked { document: self.ivars().document, tag: button.tag(), flags });
            });
        }

        /// Double-click on a layer row (the clicked row) or Cut Settings… (the selected row).
        #[unsafe(method(layerOpened:))]
        fn layer_opened(&self, sender: Option<&AnyObject>) {
            ffi_guard("layerOpened:", (), || {
                let table = &self.ivars().operations_table;
                let clicked = sender.and_then(|s| s.downcast_ref::<NSTableView>()).map(|t| t.clickedRow());
                let row = clicked.unwrap_or_else(|| table.selectedRow());
                self.send(NativeEvent::LayerOpened { document: self.ivars().document, row });
            });
        }

        /// A text field of the layer table ended its edit: the typed text goes to presentation unparsed.
        #[unsafe(method(layerFieldEdited:))]
        fn layer_field_edited(&self, sender: Option<&AnyObject>) {
            ffi_guard("layerFieldEdited:", (), || {
                let Some(field) = sender.and_then(|s| s.downcast_ref::<NSTextField>()) else { return };
                let Some((row, column)) = self.layer_cell_of(field) else { return };
                self.commit_layer_field(row, column, field.stringValue().to_string());
            });
        }

        /// An Output or Show check box of the layer table was clicked.
        #[unsafe(method(layerToggled:))]
        fn layer_toggled(&self, sender: Option<&AnyObject>) {
            ffi_guard("layerToggled:", (), || {
                let Some(button) = sender.and_then(|s| s.downcast_ref::<NSButton>()) else { return };
                let Some((row, column)) = self.layer_cell_of(button) else { return };
                self.commit_layer_toggle(row, column, button.state() == NSControlStateValueOn);
            });
        }

        /// OK in the Cut Settings sheet: the typed texts go to presentation unparsed.
        #[unsafe(method(cutSettingsOk:))]
        fn cut_settings_ok(&self, _sender: Option<&AnyObject>) {
            ffi_guard("cutSettingsOk:", (), || {
                let ivars = self.ivars();
                let [speed, power_min, power_max, passes, kerf] =
                    ivars.cut_fields.each_ref().map(|f| f.stringValue().to_string());
                let [output, visible] = ivars.cut_toggles.each_ref().map(|b| b.state() == NSControlStateValueOn);
                self.send(NativeEvent::CutSettingsConfirmed {
                    document: ivars.document,
                    mode_index: ivars.cut_mode.indexOfSelectedItem(),
                    speed,
                    power_min,
                    power_max,
                    passes,
                    kerf,
                    output,
                    visible,
                    options: super::panels::cut_settings::read_options(&ivars.cut_options),
                });
            });
        }

        #[unsafe(method(cutSettingsCancel:))]
        fn cut_settings_cancel(&self, _sender: Option<&AnyObject>) {
            ffi_guard("cutSettingsCancel:", (), || {
                self.send(NativeEvent::CutSettingsCancelled { document: self.ivars().document });
            });
        }

        /// The sheet's mode pop-up: the kerf field applies to Line only (widget state, no intent).
        #[unsafe(method(cutSettingsModeChosen:))]
        fn cut_settings_mode_chosen(&self, _sender: Option<&AnyObject>) {
            ffi_guard("cutSettingsModeChosen:", (), || {
                let ivars = self.ivars();
                ivars.cut_fields[4].setEnabled(crate::panels::kerf_applies(ivars.cut_mode.indexOfSelectedItem()));
                // The mode decides which option rows exist.
                if let Some(app) = ivars.app.upgrade() {
                    app.cut_option_changed();
                }
            });
        }

        /// A switch, pop-up or mode of the Cut Settings sheet changed: the rows are laid out again
        /// from the values on the sheet (greyed rows, rows of the chosen mode). No intent.
        #[unsafe(method(cutSettingsOptionChanged:))]
        fn cut_settings_option_changed(&self, _sender: Option<&AnyObject>) {
            ffi_guard("cutSettingsOptionChanged:", (), || {
                if let Some(app) = self.ivars().app.upgrade() {
                    app.cut_option_changed();
                }
            });
        }

        /// A bed-camera button: the tag is its [`crate::panels::CameraControl`].
        #[unsafe(method(cameraButton:))]
        fn camera_button(&self, sender: Option<&AnyObject>) {
            ffi_guard("cameraButton:", (), || {
                let Some(button) = sender.and_then(|s| s.downcast_ref::<NSButton>()) else { return };
                let Some(control) = super::panels::camera::camera_control(button.tag()) else { return };
                if let Some(app) = self.ivars().app.upgrade() {
                    app.camera_button_pressed(control, self.ivars().camera_devices.indexOfSelectedItem());
                }
            });
        }

        /// A machine control: the tag is its slot in the machine view model's buttons.
        #[unsafe(method(machineButton:))]
        fn machine_button(&self, sender: Option<&AnyObject>) {
            ffi_guard("machineButton:", (), || {
                let Some(button) = sender.and_then(|s| s.downcast_ref::<NSButton>()) else { return };
                if let (Some(app), Ok(slot)) = (self.ivars().app.upgrade(), usize::try_from(button.tag())) {
                    app.machine_button_pressed(self.ivars().document, slot);
                }
            });
        }
    }

    unsafe impl NSObjectProtocol for WindowController {}

    unsafe impl NSWindowDelegate for WindowController {
        /// Closing goes through presentation: a dirty document first asks (sheet).
        #[unsafe(method(windowShouldClose:))]
        fn window_should_close(&self, _sender: &NSWindow) -> bool {
            ffi_guard("windowShouldClose:", false, || {
                if self.ivars().may_close.get() {
                    return true;
                }
                match self.ivars().app.upgrade() {
                    Some(app) => app.close_requested(self.ivars().document),
                    // The app is gone: nothing can be lost any more.
                    None => true,
                }
            })
        }

        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, _notification: &NSNotification) {
            ffi_guard("windowWillClose:", (), || {
                // The surface goes before its view (see `Canvas`).
                if let Ok(mut canvas) = self.ivars().canvas.try_borrow_mut() {
                    canvas.take();
                }
                if let Some(app) = self.ivars().app.upgrade() {
                    app.window_closed(self.ivars().document);
                }
            });
        }

        #[unsafe(method(windowDidBecomeKey:))]
        fn window_did_become_key(&self, _notification: &NSNotification) {
            ffi_guard("windowDidBecomeKey:", (), || {
                self.send(NativeEvent::WindowBecameKey { document: self.ivars().document });
            });
        }

        #[unsafe(method(windowDidResize:))]
        fn window_did_resize(&self, _notification: &NSNotification) {
            ffi_guard("windowDidResize:", (), || self.geometry_changed());
        }

        #[unsafe(method(windowDidMove:))]
        fn window_did_move(&self, _notification: &NSNotification) {
            ffi_guard("windowDidMove:", (), || self.geometry_changed());
        }

        #[unsafe(method(windowDidChangeOcclusionState:))]
        fn window_did_change_occlusion_state(&self, _notification: &NSNotification) {
            ffi_guard("windowDidChangeOcclusionState:", (), || self.redraw());
        }

        #[unsafe(method(windowDidChangeBackingProperties:))]
        fn window_did_change_backing_properties(&self, _notification: &NSNotification) {
            ffi_guard("windowDidChangeBackingProperties:", (), || {
                if let Some(app) = self.ivars().app.upgrade() {
                    app.canvas_resized(self.ivars().document);
                }
            });
        }
    }

    unsafe impl NSTableViewDataSource for WindowController {
        #[unsafe(method(numberOfRowsInTableView:))]
        fn number_of_rows(&self, table_view: &NSTableView) -> NSInteger {
            ffi_guard("numberOfRowsInTableView:", 0, || {
                if self.is_operations(table_view) {
                    return self.ivars().operation_rows.try_borrow().map_or(0, |r| r.len() as NSInteger);
                }
                if self.is_sketch(table_view) {
                    return self.ivars().sketch_rows.try_borrow().map_or(0, |r| r.len() as NSInteger);
                }
                if self.is_sketch_params(table_view) {
                    return self.ivars().sketch_params.try_borrow().map_or(0, |r| r.len() as NSInteger);
                }
                self.ivars().labels.try_borrow().map_or(0, |l| l.len() as NSInteger)
            })
        }

        #[unsafe(method_id(tableView:objectValueForTableColumn:row:))]
        fn object_value(
            &self,
            table_view: &NSTableView,
            table_column: Option<&NSTableColumn>,
            row: NSInteger,
        ) -> Option<Retained<AnyObject>> {
            ffi_guard("tableView:objectValueForTableColumn:row:", None, || {
                let row = usize::try_from(row).ok()?;
                let text = if self.is_operations(table_view) {
                    let column = layer_column(table_column?)?;
                    let rows = self.ivars().operation_rows.try_borrow().ok()?;
                    let line = rows.get(row)?;
                    if let Some(on) = line.checked(column) {
                        let number = NSNumber::numberWithBool(on);
                        return Some(Retained::into_super(Retained::into_super(Retained::into_super(number))));
                    }
                    line.text(column)
                } else if self.is_sketch(table_view) {
                    let column = SketchColumn::from_id(&table_column?.identifier().to_string())?;
                    self.ivars().sketch_rows.try_borrow().ok()?.get(row)?.text(column)
                } else if self.is_sketch_params(table_view) {
                    let id = table_column?.identifier().to_string();
                    let rows = self.ivars().sketch_params.try_borrow().ok()?;
                    let (name, value) = rows.get(row)?;
                    if id == SKETCH_PARAM_NAME { name.clone() } else { value.clone() }
                } else {
                    self.ivars().labels.try_borrow().ok()?.get(row)?.clone()
                };
                Some(Retained::into_super(Retained::into_super(NSString::from_str(&text))))
            })
        }

        /// An operations-table cell edit ended: its text goes to presentation unparsed.
        #[unsafe(method(tableView:setObjectValue:forTableColumn:row:))]
        fn set_object_value(
            &self,
            table_view: &NSTableView,
            object: Option<&AnyObject>,
            table_column: Option<&NSTableColumn>,
            row: NSInteger,
        ) {
            ffi_guard("tableView:setObjectValue:forTableColumn:row:", (), || {
                if self.is_sketch(table_view) || self.is_sketch_params(table_view) {
                    let (Some(column), Ok(row)) = (table_column, usize::try_from(row)) else { return };
                    let Some(text) = object.and_then(|o| o.downcast_ref::<NSString>()).map(|t| t.to_string()) else {
                        return;
                    };
                    let document = self.ivars().document;
                    let id = column.identifier().to_string();
                    if self.is_sketch(table_view) {
                        if SketchColumn::from_id(&id) == Some(SketchColumn::Value) {
                            self.send(NativeEvent::SketchValueCommitted { document, row, text });
                        }
                    } else if id == SKETCH_PARAM_VALUE {
                        let name = self.ivars().sketch_params.try_borrow().ok().and_then(|p| p.get(row).map(|r| r.0.clone()));
                        if let Some(name) = name {
                            self.send(NativeEvent::SketchParameterCommitted { document, name, text });
                        }
                    }
                    return;
                }
                if !self.is_operations(table_view) {
                    return;
                }
                let (Some(column), Ok(row)) = (table_column.and_then(layer_column), usize::try_from(row)) else { return };
                if column.is_toggle() {
                    let Some(on) = object.and_then(|o| o.downcast_ref::<NSNumber>()).map(|n| n.boolValue()) else { return };
                    self.commit_layer_toggle(row, column, on);
                    return;
                }
                let Some(text) = object.and_then(|o| o.downcast_ref::<NSString>()) else { return };
                self.commit_layer_field(row, column, text.to_string());
            });
        }
    }

    unsafe impl NSControlTextEditingDelegate for WindowController {}

    unsafe impl NSTableViewDelegate for WindowController {
        #[unsafe(method(tableViewSelectionDidChange:))]
        fn table_selection_did_change(&self, notification: &NSNotification) {
            ffi_guard("tableViewSelectionDidChange:", (), || {
                if self.ivars().syncing.get() {
                    return;
                }
                let Some(table) = notification.object().and_then(|o| o.downcast::<NSTableView>().ok()) else { return };
                if self.is_operations(&table) || self.is_sketch_params(&table) {
                    return;
                }
                if self.is_sketch(&table) {
                    let rows = super::panels::widgets::selected_rows(&table);
                    self.send(NativeEvent::SketchRowsSelected { document: self.ivars().document, rows });
                    return;
                }
                let rows = super::panels::widgets::selected_rows(&table);
                self.send(NativeEvent::TableSelectionChanged { document: self.ivars().document, rows });
            });
        }
    }

    unsafe impl NSToolbarDelegate for WindowController {
        #[unsafe(method_id(toolbar:itemForItemIdentifier:willBeInsertedIntoToolbar:))]
        fn toolbar_item(
            &self,
            _toolbar: &NSToolbar,
            identifier: &NSString,
            flag: bool,
        ) -> Option<Retained<NSToolbarItem>> {
            ffi_guard("toolbar:itemForItemIdentifier:willBeInsertedIntoToolbar:", None, || {
                let mtm = MainThreadMarker::new()?;
                // A ribbon panel: its tool buttons are the item's view (a palette copy has none),
                // its tools are the overflow menu.
                let wanted = identifier.to_string();
                let panel = self
                    .ivars()
                    .panel_items
                    .try_borrow()
                    .ok()
                    .and_then(|p| p.iter().find(|(id, _)| *id == wanted).map(|(_, i)| (i.title.clone(), i.view.clone(), i.overflow.clone())));
                if let Some((title, view, overflow)) = panel {
                    let item = NSToolbarItem::initWithItemIdentifier(NSToolbarItem::alloc(mtm), identifier);
                    item.setLabel(&NSString::from_str(&title));
                    item.setPaletteLabel(&NSString::from_str(&title));
                    if flag {
                        item.setView(Some(&view));
                        item.setMenuFormRepresentation(Some(&overflow));
                    }
                    return Some(item);
                }
                let (control, label) = match wanted.as_str() {
                    SKETCH_ITEM => (&self.ivars().sketch_tools, &self.ivars().sketch_tools_label),
                    ARRANGE_ITEM => (&self.ivars().arrange_tools, &self.ivars().arrange_tools_label),
                    _ => return None,
                };
                let item = NSToolbarItem::initWithItemIdentifier(NSToolbarItem::alloc(mtm), identifier);
                let label = NSString::from_str(label);
                item.setLabel(&label);
                item.setPaletteLabel(&label);
                item.setView(Some(control));
                Some(item)
            })
        }

        #[unsafe(method_id(toolbarDefaultItemIdentifiers:))]
        fn toolbar_default(&self, _toolbar: &NSToolbar) -> Retained<NSArray<NSString>> {
            self.toolbar_identifiers()
        }

        #[unsafe(method_id(toolbarAllowedItemIdentifiers:))]
        fn toolbar_allowed(&self, _toolbar: &NSToolbar) -> Retained<NSArray<NSString>> {
            self.toolbar_identifiers()
        }
    }

    /// The panes keep their width range: object list 150–360 pt, inspector 240–420 pt, and the
    /// canvas column never gets narrower than 420 pt (LF7).
    unsafe impl NSSplitViewDelegate for WindowController {
        #[unsafe(method(splitView:constrainMinCoordinate:ofSubviewAt:))]
        fn constrain_min(&self, split_view: &NSSplitView, proposed: f64, index: NSInteger) -> f64 {
            ffi_guard("splitView:constrainMinCoordinate:ofSubviewAt:", proposed, || {
                let (sidebar, inspector, width, thickness) = split_metrics(split_view);
                match index {
                    0 => super::window::SIDEBAR_RANGE.0,
                    _ => (sidebar + thickness + super::window::CANVAS_MIN)
                        .max(width - thickness - super::window::INSPECTOR_RANGE.1)
                        .min(width - thickness - inspector.min(super::window::INSPECTOR_RANGE.1)),
                }
            })
        }

        #[unsafe(method(splitView:constrainMaxCoordinate:ofSubviewAt:))]
        fn constrain_max(&self, split_view: &NSSplitView, proposed: f64, index: NSInteger) -> f64 {
            ffi_guard("splitView:constrainMaxCoordinate:ofSubviewAt:", proposed, || {
                let (_, inspector, width, thickness) = split_metrics(split_view);
                match index {
                    0 => super::window::SIDEBAR_RANGE
                        .1
                        .min(width - 2.0 * thickness - inspector - super::window::CANVAS_MIN)
                        .max(super::window::SIDEBAR_RANGE.0),
                    _ => width - thickness - super::window::INSPECTOR_RANGE.0,
                }
            })
        }

        /// Only the canvas column takes the extra room when the window grows (and gives it back
        /// when it shrinks): the object list and the inspector hold their width.
        #[unsafe(method(splitView:resizeSubviewsWithOldSize:))]
        fn resize_subviews(&self, split_view: &NSSplitView, _old_size: NSSize) {
            ffi_guard("splitView:resizeSubviewsWithOldSize:", (), || {
                let panes = split_view.subviews();
                if panes.count() != 3 {
                    // Not our layout: AppKit's own.
                    split_view.adjustSubviews();
                    return;
                }
                let (sidebar, inspector) = pane_widths(split_view.frame().size.width, split_view);
                let size = split_view.frame().size;
                let t = split_view.dividerThickness();
                let centre = (size.width - sidebar - inspector - 2.0 * t).max(0.0);
                let frame = |x: f64, w: f64| NSRect::new(NSPoint::new(x, 0.0), NSSize::new(w, size.height));
                panes.objectAtIndex(0).setFrame(frame(0.0, sidebar));
                panes.objectAtIndex(1).setFrame(frame(sidebar + t, centre));
                panes.objectAtIndex(2).setFrame(frame(sidebar + t + centre + t, inspector));
            });
        }

        #[unsafe(method(splitViewDidResizeSubviews:))]
        fn split_did_resize(&self, _notification: &NSNotification) {
            ffi_guard("splitViewDidResizeSubviews:", (), || self.geometry_changed());
        }
    }
);

// ---------------------------------------------------------------------------------------------
// Tool strip view: tells the host when its icons must be drawn again (light/dark, Retina).
// ---------------------------------------------------------------------------------------------

pub(crate) struct StripIvars {
    app: Weak<HostApp>,
    document: DocumentId,
}

define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnToolStripView"]
    #[ivars = StripIvars]
    pub(crate) struct ToolStripView;

    impl ToolStripView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            false
        }

        /// Light/dark switch (system setting or the window's own appearance).
        #[unsafe(method(viewDidChangeEffectiveAppearance))]
        fn view_did_change_effective_appearance(&self) {
            ffi_guard("viewDidChangeEffectiveAppearance", (), || {
                // SAFETY: NSView's own implementation, no arguments, initialised instance.
                let () = unsafe { msg_send![super(self), viewDidChangeEffectiveAppearance] };
                self.restyle();
            });
        }

        /// Moved to a screen with another backing scale.
        #[unsafe(method(viewDidChangeBackingProperties))]
        fn view_did_change_backing_properties(&self) {
            ffi_guard("viewDidChangeBackingProperties", (), || {
                // SAFETY: NSView's own implementation, no arguments, initialised instance.
                let () = unsafe { msg_send![super(self), viewDidChangeBackingProperties] };
                self.restyle();
            });
        }
    }

    unsafe impl NSObjectProtocol for ToolStripView {}
);

impl ToolStripView {
    pub(crate) fn new(
        mtm: MainThreadMarker,
        frame: NSRect,
        app: Weak<HostApp>,
        document: DocumentId,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(StripIvars { app, document });
        // SAFETY: NSView's designated initialiser on a freshly allocated instance with ivars set.
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }

    fn restyle(&self) {
        if let Some(app) = self.ivars().app.upgrade() {
            app.native(NativeEvent::ToolStripRestyle { document: self.ivars().document });
        }
    }
}

/// The widths the object list and the inspector keep for a split view `total` wide: what they
/// have, within their ranges, giving way (inspector first) only when the canvas column would fall
/// below its minimum.
fn pane_widths(total: f64, split: &NSSplitView) -> (f64, f64) {
    let (sidebar, inspector, _, t) = split_metrics(split);
    let (sidebar_range, inspector_range) = (super::window::SIDEBAR_RANGE, super::window::INSPECTOR_RANGE);
    let mut sidebar = sidebar.clamp(sidebar_range.0, sidebar_range.1);
    let mut inspector = inspector.clamp(inspector_range.0, inspector_range.1);
    let deficit = super::window::CANVAS_MIN - (total - sidebar - inspector - 2.0 * t);
    if deficit > 0.0 {
        let from_inspector = deficit.min(inspector - inspector_range.0);
        inspector -= from_inspector;
        sidebar -= (deficit - from_inspector).min(sidebar - sidebar_range.0);
    }
    // A split view narrower than its panes (never for a window at its minimum size): they share it.
    let room = (total - 2.0 * t).max(0.0);
    if sidebar + inspector > room {
        let ratio = room / (sidebar + inspector);
        sidebar *= ratio;
        inspector *= ratio;
    }
    (sidebar, inspector)
}

/// Width of the object list and of the inspector, of the split view and of its dividers.
fn split_metrics(split: &NSSplitView) -> (f64, f64, f64, f64) {
    let panes = split.subviews();
    let width_of = |i: usize| if i < panes.count() { panes.objectAtIndex(i).frame().size.width } else { 0.0 };
    (width_of(0), width_of(2), split.frame().size.width, split.dividerThickness())
}

impl WindowController {
    /// Panel items, then Uitlijnen and Schets (the toolbar's default and allowed items).
    fn toolbar_identifiers(&self) -> Retained<NSArray<NSString>> {
        let mut ids: Vec<Retained<NSString>> = self
            .ivars()
            .panel_items
            .try_borrow()
            .map(|p| p.iter().map(|(id, _)| NSString::from_str(id)).collect())
            .unwrap_or_default();
        ids.push(NSString::from_str(ARRANGE_ITEM));
        ids.push(NSString::from_str(SKETCH_ITEM));
        NSArray::from_retained_slice(&ids)
    }

    pub(crate) fn new(mtm: MainThreadMarker, parts: ControllerParts) -> Retained<Self> {
        let ControllerParts {
            app,
            document,
            fields,
            operations_table,
            operation_inputs,
            operation_kind,
            cut_mode,
            cut_fields,
            cut_toggles,
            cut_options,
            camera_devices,
            sketch_table,
            sketch_params_table,
            param_inputs,
            sketch_tools,
            sketch_tools_label,
            arrange_tools,
            arrange_tools_label,
        } = parts;
        let this = Self::alloc(mtm).set_ivars(ControllerIvars {
            app,
            document,
            fields,
            operations_table,
            operation_inputs,
            operation_kind,
            cut_mode,
            cut_fields,
            cut_toggles,
            cut_options,
            camera_devices,
            sketch_table,
            sketch_params_table,
            param_inputs,
            sketch_tools,
            sketch_tools_label,
            arrange_tools,
            arrange_tools_label,
            panel_items: RefCell::new(Vec::new()),
            operation_rows: RefCell::new(Vec::new()),
            sketch_rows: RefCell::new(Vec::new()),
            sketch_params: RefCell::new(Vec::new()),
            labels: RefCell::new(Vec::new()),
            syncing: Cell::new(false),
            canvas: RefCell::new(None),
            may_close: Cell::new(false),
        });
        // SAFETY: NSObject's designated initialiser on a freshly allocated instance with ivars set.
        unsafe { msg_send![super(this), init] }
    }

    fn send(&self, event: NativeEvent) {
        if let Some(app) = self.ivars().app.upgrade() {
            app.native(event);
        }
    }

    /// The ribbon panels the toolbar shows as its items.
    pub(crate) fn set_panel_items(&self, items: Vec<(String, PanelItem)>) {
        if let Ok(mut slot) = self.ivars().panel_items.try_borrow_mut() {
            *slot = items;
        }
    }

    /// Frame or pane widths changed: the host saves them.
    fn geometry_changed(&self) {
        if let Some(app) = self.ivars().app.upgrade() {
            app.window_geometry_changed(self.ivars().document);
        }
    }

    pub(crate) fn set_canvas(&self, canvas: Option<Canvas>) {
        if let Ok(mut slot) = self.ivars().canvas.try_borrow_mut() {
            *slot = canvas;
        }
    }

    pub(crate) fn with_canvas<T>(&self, f: impl FnOnce(&mut Canvas) -> T) -> Option<T> {
        let mut slot = self.ivars().canvas.try_borrow_mut().ok()?;
        slot.as_mut().map(f)
    }

    pub(crate) fn redraw(&self) {
        if let Some(app) = self.ivars().app.upgrade() {
            app.redraw(self.ivars().document);
        }
    }

    pub(crate) fn allow_close(&self) {
        self.ivars().may_close.set(true);
    }

    /// Replace the table's rows; true when they changed (the table must reload).
    pub(crate) fn set_labels(&self, labels: Vec<String>) -> bool {
        match self.ivars().labels.try_borrow_mut() {
            Ok(mut current) if *current != labels => {
                *current = labels;
                true
            }
            _ => false,
        }
    }

    fn is_operations(&self, table: &NSTableView) -> bool {
        std::ptr::eq(table, &*self.ivars().operations_table)
    }

    /// One row of the layer table, as the view model last said it.
    pub(crate) fn operation_line(&self, row: usize) -> Option<OperationLine> {
        self.ivars().operation_rows.try_borrow().ok()?.get(row).cloned()
    }

    /// The row and column of the layer-table cell `control` lies in.
    fn layer_cell_of(&self, control: &NSView) -> Option<(usize, LayerColumn)> {
        let table = &self.ivars().operations_table;
        let row = usize::try_from(table.rowForView(control)).ok()?;
        let column = usize::try_from(table.columnForView(control)).ok()?;
        layer_column(&table.tableColumns().objectAtIndex(column))
            .map(|c| (row, c))
    }

    /// The typed text of a layer-table cell goes to presentation unparsed.
    fn commit_layer_field(&self, row: usize, column: LayerColumn, text: String) {
        let Some(field) = column.field() else { return };
        self.send(NativeEvent::OperationFieldCommitted { document: self.ivars().document, row, field, text });
    }

    fn commit_layer_toggle(&self, row: usize, column: LayerColumn, on: bool) {
        self.send(NativeEvent::LayerToggled { document: self.ivars().document, row, column, on });
    }

    /// Replace the operations table's rows; true when they changed (the table must reload).
    pub(crate) fn set_operation_rows(&self, rows: Vec<OperationLine>) -> bool {
        match self.ivars().operation_rows.try_borrow_mut() {
            Ok(mut current) if *current != rows => {
                *current = rows;
                true
            }
            _ => false,
        }
    }

    fn is_sketch(&self, table: &NSTableView) -> bool {
        std::ptr::eq(table, &*self.ivars().sketch_table)
    }

    fn is_sketch_params(&self, table: &NSTableView) -> bool {
        std::ptr::eq(table, &*self.ivars().sketch_params_table)
    }

    /// Replace the constraint table's rows; true when they changed (the table must reload).
    pub(crate) fn set_sketch_rows(&self, rows: Vec<SketchLine>) -> bool {
        match self.ivars().sketch_rows.try_borrow_mut() {
            Ok(mut current) if *current != rows => {
                *current = rows;
                true
            }
            _ => false,
        }
    }

    /// Replace the parameter table's rows; true when they changed.
    pub(crate) fn set_sketch_params(&self, rows: Vec<(String, String)>) -> bool {
        match self.ivars().sketch_params.try_borrow_mut() {
            Ok(mut current) if *current != rows => {
                *current = rows;
                true
            }
            _ => false,
        }
    }

    /// Run `f` (a programmatic table change) without turning it into an intent.
    pub(crate) fn syncing(&self, f: impl FnOnce()) {
        let before = self.ivars().syncing.replace(true);
        f();
        self.ivars().syncing.set(before);
    }
}

// ---------------------------------------------------------------------------------------------
// Canvas view: a flipped NSView (origin top-left, y down, like `PointerEvent`) with the Metal
// layer; an accessibility layout area whose children are the document's objects; and an
// NSTextInputClient so input methods work on the canvas (composition stays here).
// ---------------------------------------------------------------------------------------------

pub(crate) struct CanvasIvars {
    app: Weak<HostApp>,
    document: DocumentId,
    composer: RefCell<Composer>,
}

define_class!(
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnCanvasView"]
    #[ivars = CanvasIvars]
    pub(crate) struct CanvasView;

    impl CanvasView {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        /// A click into a background window's canvas acts right away (like other drawing apps)
        /// instead of only activating the window.
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(setFrameSize:))]
        fn set_frame_size(&self, size: NSSize) {
            ffi_guard("setFrameSize:", (), || {
                // SAFETY: NSView's own implementation, same argument, initialised instance.
                let () = unsafe { msg_send![super(self), setFrameSize: size] };
                self.resized();
            });
        }

        /// Light/dark switch (system setting or the window's own appearance): the canvas theme follows.
        #[unsafe(method(viewDidChangeEffectiveAppearance))]
        fn view_did_change_effective_appearance(&self) {
            ffi_guard("viewDidChangeEffectiveAppearance", (), || {
                // SAFETY: NSView's own implementation, no arguments, initialised instance.
                let () = unsafe { msg_send![super(self), viewDidChangeEffectiveAppearance] };
                if let Some(app) = self.ivars().app.upgrade() {
                    app.redraw(self.ivars().document);
                }
            });
        }

        /// Moved to a screen with another backing scale (Retina ↔ non-Retina) or the scale changed.
        #[unsafe(method(viewDidChangeBackingProperties))]
        fn view_did_change_backing_properties(&self) {
            ffi_guard("viewDidChangeBackingProperties", (), || {
                // SAFETY: NSView's own implementation, no arguments, initialised instance.
                let () = unsafe { msg_send![super(self), viewDidChangeBackingProperties] };
                self.resized();
            });
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            ffi_guard("mouseDown:", (), || {
                if let Some(window) = self.window() {
                    window.makeFirstResponder(Some(self));
                }
                self.mouse(event, MousePhase::Down, MouseButton::Left);
            });
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            ffi_guard("mouseDragged:", (), || self.mouse(event, MousePhase::Dragged, MouseButton::Left));
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            ffi_guard("mouseUp:", (), || self.mouse(event, MousePhase::Up, MouseButton::Left));
        }

        /// Button-less moves (hover): the drawing tools' rubber band and snap highlight.
        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            ffi_guard("mouseMoved:", (), || self.mouse(event, MousePhase::Moved, MouseButton::Left));
        }

        /// In a window: mouse moves are delivered to the first responder (the canvas after a
        /// click), and the tracking area delivers them while the pointer is over the canvas.
        #[unsafe(method(viewDidMoveToWindow))]
        fn view_did_move_to_window(&self) {
            ffi_guard("viewDidMoveToWindow", (), || {
                if let Some(window) = self.window() {
                    window.setAcceptsMouseMovedEvents(true);
                }
            });
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &NSEvent) {
            ffi_guard("rightMouseDown:", (), || self.mouse(event, MousePhase::Down, MouseButton::Right));
        }

        #[unsafe(method(rightMouseUp:))]
        fn right_mouse_up(&self, event: &NSEvent) {
            ffi_guard("rightMouseUp:", (), || self.mouse(event, MousePhase::Up, MouseButton::Right));
        }

        #[unsafe(method(otherMouseDown:))]
        fn other_mouse_down(&self, event: &NSEvent) {
            ffi_guard("otherMouseDown:", (), || self.mouse(event, MousePhase::Down, MouseButton::Other));
        }

        #[unsafe(method(otherMouseUp:))]
        fn other_mouse_up(&self, event: &NSEvent) {
            ffi_guard("otherMouseUp:", (), || self.mouse(event, MousePhase::Up, MouseButton::Other));
        }

        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {
            ffi_guard("scrollWheel:", (), || {
                self.send(NativeEvent::Scroll {
                    document: self.ivars().document,
                    delta: [event.scrollingDeltaX(), event.scrollingDeltaY()],
                    precise: event.hasPreciseScrollingDeltas(),
                    flags: event.modifierFlags().0,
                    timestamp_s: event.timestamp(),
                });
            });
        }

        #[unsafe(method(magnifyWithEvent:))]
        fn magnify_with_event(&self, event: &NSEvent) {
            ffi_guard("magnifyWithEvent:", (), || {
                let at = self.convertPoint_fromView(event.locationInWindow(), None);
                self.send(NativeEvent::Magnify {
                    document: self.ivars().document,
                    magnification: event.magnification(),
                    location_pt: [at.x, at.y],
                    timestamp_s: event.timestamp(),
                });
            });
        }

        /// While composing, every key goes to the input method. Otherwise Esc/Delete are canvas
        /// keys, shortcuts go up the responder chain, and text goes through `interpretKeyEvents:`
        /// (which calls back `insertText:` / `setMarkedText:` / `doCommandBySelector:`).
        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            ffi_guard("keyDown:", (), || {
                let composing = self.ivars().composer.try_borrow().is_ok_and(|c| c.has_marked());
                if !composing {
                    let handled = self.ivars().app.upgrade().is_some_and(|app| {
                        app.canvas_key(self.ivars().document, event.keyCode(), event.modifierFlags().0)
                    });
                    if handled {
                        return;
                    }
                    // A tool or command shortcut from the shortcut register (plain letters are
                    // text while the Text tool is active: presentation says so).
                    let chars = event.charactersIgnoringModifiers().map(|s| s.to_string()).unwrap_or_default();
                    let shortcut = self.ivars().app.upgrade().is_some_and(|app| {
                        app.canvas_shortcut(self.ivars().document, &chars, event.keyCode(), event.modifierFlags().0)
                    });
                    if shortcut {
                        return;
                    }
                    let flags = crate::adapter::modifiers(event.modifierFlags().0);
                    if flags.command || flags.control {
                        // SAFETY: NSResponder's own implementation (passes the key up the chain).
                        let () = unsafe { msg_send![super(self), keyDown: event] };
                        return;
                    }
                }
                self.interpretKeyEvents(&NSArray::from_slice(&[event]));
            });
        }

        /// Edit › Select All goes through the responder chain (nil target) so a text field keeps
        /// its own Select All; on the canvas it selects every object.
        #[unsafe(method(selectAll:))]
        fn select_all(&self, _sender: Option<&AnyObject>) {
            ffi_guard("selectAll:", (), || {
                if let Some(app) = self.ivars().app.upgrade() {
                    app.select_all(self.ivars().document);
                }
            });
        }

        /// Edit ▸ Knip, Kopieer, Plak, Plak op plaats go through the responder chain (nil target),
        /// like Select All: a focused text field keeps normal text editing; on the canvas they
        /// act on the selection and on the pasteboard.
        #[unsafe(method(cut:))]
        fn cut(&self, _sender: Option<&AnyObject>) {
            ffi_guard("cut:", (), || self.edit_command(CommandKey::Cut));
        }

        #[unsafe(method(copy:))]
        fn copy(&self, _sender: Option<&AnyObject>) {
            ffi_guard("copy:", (), || self.edit_command(CommandKey::Copy));
        }

        #[unsafe(method(paste:))]
        fn paste(&self, _sender: Option<&AnyObject>) {
            ffi_guard("paste:", (), || self.edit_command(CommandKey::Paste));
        }

        #[unsafe(method(pasteInPlace:))]
        fn paste_in_place(&self, _sender: Option<&AnyObject>) {
            ffi_guard("pasteInPlace:", (), || self.edit_command(CommandKey::PasteInPlace));
        }

        // ---- NSAccessibility: the canvas is a layout area, objects are layout items ----

        #[unsafe(method(isAccessibilityElement))]
        fn is_accessibility_element(&self) -> bool {
            true
        }

        #[unsafe(method_id(accessibilityRole))]
        fn accessibility_role(&self) -> Option<Retained<NSString>> {
            // SAFETY: reading an immutable AppKit constant.
            Some(unsafe { NSAccessibilityLayoutAreaRole }.retain())
        }

        #[unsafe(method_id(accessibilityLabel))]
        fn accessibility_label(&self) -> Option<Retained<NSString>> {
            ffi_guard("accessibilityLabel", None, || {
                let app = self.ivars().app.upgrade()?;
                Some(NSString::from_str(host_text(HostText::CanvasLabel, app.locale())))
            })
        }

        #[unsafe(method_id(accessibilityChildren))]
        fn accessibility_children(&self) -> Option<Retained<NSArray>> {
            ffi_guard("accessibilityChildren", None, || Some(self.accessibility_elements()))
        }
    }

    unsafe impl NSObjectProtocol for CanvasView {}

    /// Enablement of the responder-chain Edit items, from the view model (and, for Paste, the
    /// pasteboard). Anything else that reaches the canvas stays enabled as AppKit would have it.
    unsafe impl NSMenuItemValidation for CanvasView {
        #[unsafe(method(validateMenuItem:))]
        fn validate_menu_item(&self, menu_item: &NSMenuItem) -> bool {
            ffi_guard("validateMenuItem:", false, || {
                let Some(app) = self.ivars().app.upgrade() else { return false };
                let Some(action) = menu_item.action() else { return true };
                let command = if action == sel!(cut:) {
                    CommandKey::Cut
                } else if action == sel!(copy:) {
                    CommandKey::Copy
                } else if action == sel!(paste:) {
                    CommandKey::Paste
                } else if action == sel!(pasteInPlace:) {
                    CommandKey::PasteInPlace
                } else if action == sel!(selectAll:) {
                    CommandKey::SelectAll
                } else {
                    return true;
                };
                app.edit_item_enabled(command)
            })
        }
    }

    unsafe impl NSTextInputClient for CanvasView {
        #[unsafe(method(insertText:replacementRange:))]
        fn insert_text(&self, string: &AnyObject, _replacement_range: NSRange) {
            ffi_guard("insertText:replacementRange:", (), || {
                let text = plain_text(string);
                let committed = self.ivars().composer.try_borrow_mut().ok().and_then(|mut c| c.insert(&text));
                self.committed(committed);
            });
        }

        /// Commands from the input route (arrows, cancelOperation: during composition …) are not
        /// canvas text; Esc/Delete reach the canvas through `keyDown:` before this route.
        #[unsafe(method(doCommandBySelector:))]
        fn do_command_by_selector(&self, _selector: Sel) {}

        #[unsafe(method(setMarkedText:selectedRange:replacementRange:))]
        fn set_marked_text(&self, string: &AnyObject, selected_range: NSRange, _replacement_range: NSRange) {
            ffi_guard("setMarkedText:selectedRange:replacementRange:", (), || {
                let text = plain_text(string);
                if let Ok(mut c) = self.ivars().composer.try_borrow_mut() {
                    c.set_marked(&text, utf16_range_to_chars(&text, selected_range));
                }
            });
        }

        #[unsafe(method(unmarkText))]
        fn unmark_text(&self) {
            ffi_guard("unmarkText", (), || {
                let committed = self.ivars().composer.try_borrow_mut().ok().and_then(|mut c| c.unmark());
                self.committed(committed);
            });
        }

        #[unsafe(method(selectedRange))]
        fn selected_range(&self) -> NSRange {
            ffi_guard("selectedRange", NSRange::new(NSNotFound as NSUInteger, 0), || {
                match self.ivars().composer.try_borrow() {
                    Ok(c) if c.has_marked() => {
                        let (location, length) = chars_to_utf16(c.marked_text(), c.selected_range());
                        NSRange::new(location, length)
                    }
                    _ => NSRange::new(NSNotFound as NSUInteger, 0),
                }
            })
        }

        #[unsafe(method(markedRange))]
        fn marked_range(&self) -> NSRange {
            ffi_guard("markedRange", NSRange::new(NSNotFound as NSUInteger, 0), || {
                match self.ivars().composer.try_borrow() {
                    Ok(c) if c.has_marked() => NSRange::new(0, c.marked_utf16_len()),
                    _ => NSRange::new(NSNotFound as NSUInteger, 0),
                }
            })
        }

        #[unsafe(method(hasMarkedText))]
        fn has_marked_text(&self) -> bool {
            ffi_guard("hasMarkedText", false, || self.ivars().composer.try_borrow().is_ok_and(|c| c.has_marked()))
        }

        #[unsafe(method_id(attributedSubstringForProposedRange:actualRange:))]
        fn attributed_substring(
            &self,
            _range: NSRange,
            _actual_range: NSRangePointer,
        ) -> Option<Retained<NSAttributedString>> {
            None
        }

        #[unsafe(method_id(validAttributesForMarkedText))]
        fn valid_attributes(&self) -> Retained<NSArray<NSAttributedStringKey>> {
            NSArray::new()
        }

        /// Where the input method puts its candidate window: just below the canvas's top-left
        /// corner, in screen coordinates.
        #[unsafe(method(firstRectForCharacterRange:actualRange:))]
        fn first_rect(&self, _range: NSRange, _actual_range: NSRangePointer) -> NSRect {
            ffi_guard("firstRectForCharacterRange:actualRange:", NSRect::ZERO, || {
                let local = NSRect::new(NSPoint::new(16.0, 16.0), NSSize::new(1.0, 20.0));
                let in_window = self.convertRect_toView(local, None);
                self.window().map_or(NSRect::ZERO, |w| w.convertRectToScreen(in_window))
            })
        }

        #[unsafe(method(characterIndexForPoint:))]
        fn character_index(&self, _point: NSPoint) -> NSUInteger {
            NSNotFound as NSUInteger
        }
    }
);

/// The string of an `NSString` or `NSAttributedString` argument (input methods send either).
fn plain_text(string: &AnyObject) -> String {
    if let Some(s) = string.downcast_ref::<NSString>() {
        return s.to_string();
    }
    if let Some(s) = string.downcast_ref::<NSAttributedString>() {
        return s.string().to_string();
    }
    String::new()
}

/// AppKit ranges count UTF-16 units; the composer counts chars.
fn utf16_range_to_chars(text: &str, range: NSRange) -> (usize, usize) {
    let to_chars = |units: usize| {
        let mut seen = 0;
        let mut chars = 0;
        for c in text.chars() {
            if seen >= units {
                break;
            }
            seen += c.len_utf16();
            chars += 1;
        }
        chars
    };
    let start = to_chars(range.location);
    let end = to_chars(range.location.saturating_add(range.length));
    (start, end.saturating_sub(start))
}

fn chars_to_utf16(text: &str, (location, length): (usize, usize)) -> (usize, usize) {
    let units = |n: usize| text.chars().take(n).map(char::len_utf16).sum::<usize>();
    let start = units(location);
    (start, units(location + length) - start)
}

impl CanvasView {
    pub(crate) fn new(
        mtm: MainThreadMarker,
        frame: NSRect,
        app: Weak<HostApp>,
        document: DocumentId,
    ) -> Retained<Self> {
        let this =
            Self::alloc(mtm).set_ivars(CanvasIvars { app, document, composer: RefCell::new(Composer::default()) });
        // SAFETY: NSView's designated initialiser on a freshly allocated instance with ivars set.
        let view: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        let options = NSTrackingAreaOptions::MouseMoved
            | NSTrackingAreaOptions::ActiveInKeyWindow
            | NSTrackingAreaOptions::InVisibleRect;
        // SAFETY: a tracking area owned by (and added to) this view; InVisibleRect makes AppKit
        // keep its rect equal to the visible rect, so no `updateTrackingAreas` is needed.
        let area = unsafe {
            NSTrackingArea::initWithRect_options_owner_userInfo(
                NSTrackingArea::alloc(),
                NSRect::ZERO,
                options,
                Some(&view),
                None,
            )
        };
        view.addTrackingArea(&area);
        view
    }

    fn send(&self, event: NativeEvent) {
        if let Some(app) = self.ivars().app.upgrade() {
            app.native(event);
        }
    }

    fn resized(&self) {
        if let Some(app) = self.ivars().app.upgrade() {
            app.canvas_resized(self.ivars().document);
        }
    }

    /// Cut, Copy and the two pastes of the canvas.
    fn edit_command(&self, command: CommandKey) {
        let Some(app) = self.ivars().app.upgrade() else { return };
        let document = self.ivars().document;
        match command {
            CommandKey::Paste => app.paste(document, false),
            CommandKey::PasteInPlace => app.paste(document, true),
            _ => app.clipboard_command(document, command),
        }
    }

    fn committed(&self, text: Option<String>) {
        if let Some(text) = text {
            self.send(NativeEvent::TextCommitted { document: self.ivars().document, text });
        }
    }

    fn mouse(&self, event: &NSEvent, phase: MousePhase, button: MouseButton) {
        let at = self.convertPoint_fromView(event.locationInWindow(), None);
        // `clickCount` raises for events that are not clicks (mouse moved).
        let click_count = if phase == MousePhase::Moved { 0 } else { event.clickCount() };
        self.send(NativeEvent::Mouse {
            document: self.ivars().document,
            phase,
            button,
            location_pt: [at.x, at.y],
            flags: event.modifierFlags().0,
            click_count,
            timestamp_s: event.timestamp(),
        });
    }

    /// One `NSAccessibilityElement` per document object (fresh each call: the scene may have
    /// changed), framed in this view's coordinates so AppKit keeps them right when the window moves.
    fn accessibility_elements(&self) -> Retained<NSArray> {
        let elements =
            self.ivars().app.upgrade().map(|app| app.canvas_accessibility(self.ivars().document)).unwrap_or_default();
        let parent: &AnyObject = self.as_ref();
        let objects: Vec<Retained<AnyObject>> = elements
            .iter()
            .map(|e| {
                let element = NSAccessibilityElement::new();
                // SAFETY: reading an immutable AppKit constant.
                element.setAccessibilityRole(Some(unsafe { NSAccessibilityLayoutItemRole }));
                element.setAccessibilityLabel(Some(&NSString::from_str(&e.label)));
                let value = NSString::from_str(&e.value);
                // SAFETY: the value is an NSString, as AXValue of a layout item expects.
                unsafe { element.setAccessibilityValue(Some(&value)) };
                element.setAccessibilitySelected(e.selected);
                element.setAccessibilityFrameInParentSpace(NSRect::new(
                    NSPoint::new(e.frame_pt[0], e.frame_pt[1]),
                    NSSize::new(e.frame_pt[2], e.frame_pt[3]),
                ));
                // SAFETY: the parent is this live view; AppKit holds it weakly.
                unsafe { element.setAccessibilityParent(Some(parent)) };
                Retained::into_super(Retained::into_super(element))
            })
            .collect();
        NSArray::from_retained_slice(&objects)
    }

    pub(crate) fn as_view(&self) -> &NSView {
        self
    }
}

/// The split view delegate object as the protocol AppKit expects.
pub(crate) fn split_delegate(controller: &WindowController) -> &ProtocolObject<dyn NSSplitViewDelegate> {
    ProtocolObject::from_ref(controller)
}

/// The window delegate object as the protocol AppKit expects.
pub(crate) fn window_delegate(controller: &WindowController) -> &ProtocolObject<dyn NSWindowDelegate> {
    ProtocolObject::from_ref(controller)
}

/// Column identifiers of the sketch panel's parameter table.
pub(crate) const SKETCH_PARAM_NAME: &str = "param-name";
pub(crate) const SKETCH_PARAM_VALUE: &str = "param-value";

/// The controller as table data source / delegate.
pub(crate) fn table_source(controller: &WindowController) -> &ProtocolObject<dyn NSTableViewDataSource> {
    ProtocolObject::from_ref(controller)
}

pub(crate) fn table_delegate(controller: &WindowController) -> &ProtocolObject<dyn NSTableViewDelegate> {
    ProtocolObject::from_ref(controller)
}

pub(crate) fn toolbar_delegate(controller: &WindowController) -> &ProtocolObject<dyn NSToolbarDelegate> {
    ProtocolObject::from_ref(controller)
}

/// The layer-table column of an `NSTableColumn` (by identifier).
fn layer_column(column: &NSTableColumn) -> Option<LayerColumn> {
    LayerColumn::from_id(&column.identifier().to_string())
}
