//! The Objective-C classes of the app-level panels (W3-UI-app): one target object for every
//! button, field, pop-up and table of the preferences, shortcut, history, preview and RD windows,
//! the key recorder and the preview's clickable image view. Like every class in this layer, a
//! callback only reduces its arguments to plain data and hands them to the [`HostApp`]; none of
//! them decides what an event means (see `classes.rs` for the invariants that make this sound:
//! main-thread-only classes, senders downcast and never assumed, bodies in `ffi_guard`).

use std::cell::{Cell, RefCell};
use std::rc::Weak;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSButton, NSControlStateValueOn, NSControlTextEditingDelegate, NSEvent, NSImageView, NSPopUpButton, NSTableColumn,
    NSTableView, NSTableViewDataSource, NSTableViewDelegate, NSTextField, NSView,
};
use objc2_foundation::{NSInteger, NSNotification, NSObject, NSPoint, NSRect, NSString};

use crate::app_menu::{AppEvent, binding_from_event};
use crate::app_view::{HistoryLine, ShortcutLine};
use crate::appkit::app::HostApp;
use crate::appkit::classes::ffi_guard;
use burnburn_presentation::PrefKey;

pub(crate) const HISTORY_TABLE: isize = 1;
pub(crate) const SHORTCUT_TABLE: isize = 2;
pub(crate) const RD_TABLE: isize = 3;

/// What the tables show; the host writes it before it reloads a table.
#[derive(Default)]
pub(crate) struct TableData {
    pub(crate) history: Vec<HistoryLine>,
    pub(crate) shortcuts: Vec<ShortcutLine>,
    pub(crate) rd: Vec<[String; 4]>,
}

pub(crate) struct TargetIvars {
    app: Weak<HostApp>,
    pub(crate) data: RefCell<TableData>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnAppPanelTarget"]
    #[ivars = TargetIvars]
    pub(crate) struct AppPanelTarget;

    impl AppPanelTarget {
        /// A number field of the preferences window (tag = index in `PrefKey::ALL`).
        #[unsafe(method(prefText:))]
        fn pref_text(&self, sender: Option<&AnyObject>) {
            ffi_guard("prefText:", (), || {
                let Some(field) = sender.and_then(|s| s.downcast_ref::<NSTextField>()) else { return };
                let Some(key) = usize::try_from(field.tag()).ok().and_then(|i| PrefKey::ALL.get(i)) else { return };
                let text = field.stringValue().to_string();
                self.send(AppEvent::PrefEdited { key: *key, text });
            });
        }

        /// A checkbox of the preferences window.
        #[unsafe(method(prefCheck:))]
        fn pref_check(&self, sender: Option<&AnyObject>) {
            ffi_guard("prefCheck:", (), || {
                let Some(b) = sender.and_then(|s| s.downcast_ref::<NSButton>()) else { return };
                let Some(key) = usize::try_from(b.tag()).ok().and_then(|i| PrefKey::ALL.get(i)) else { return };
                let on = b.state() == NSControlStateValueOn;
                self.send(AppEvent::PrefEdited { key: *key, text: if on { "on" } else { "off" }.to_owned() });
            });
        }

        /// A pop-up of the preferences window: the chosen item's stable id goes to presentation.
        #[unsafe(method(prefChoice:))]
        fn pref_choice(&self, sender: Option<&AnyObject>) {
            ffi_guard("prefChoice:", (), || {
                let Some(p) = sender.and_then(|s| s.downcast_ref::<NSPopUpButton>()) else { return };
                let Some(key) = usize::try_from(p.tag()).ok().and_then(|i| PrefKey::ALL.get(i)) else { return };
                let Some(id) = usize::try_from(p.indexOfSelectedItem())
                    .ok()
                    .and_then(|i| crate::app_view::pref_choice_id(*key, i))
                else {
                    return;
                };
                self.send(AppEvent::PrefEdited { key: *key, text: id.to_owned() });
            });
        }

        #[unsafe(method(prefsReset:))]
        fn prefs_reset(&self, _sender: Option<&AnyObject>) {
            ffi_guard("prefsReset:", (), || self.send(AppEvent::PrefsReset));
        }

        // ---- shortcut editor ----
        #[unsafe(method(shortcutFilter:))]
        fn shortcut_filter(&self, sender: Option<&AnyObject>) {
            ffi_guard("shortcutFilter:", (), || {
                let Some(field) = sender.and_then(|s| s.downcast_ref::<NSTextField>()) else { return };
                let text = field.stringValue().to_string();
                if let Some(app) = self.ivars().app.upgrade() {
                    app.shortcut_filter_changed(&text);
                }
            });
        }

        /// Record…, Clear, Default, Reset all and Replace (button tags 0…4).
        #[unsafe(method(shortcutButton:))]
        fn shortcut_button(&self, sender: Option<&AnyObject>) {
            ffi_guard("shortcutButton:", (), || {
                let Some(b) = sender.and_then(|s| s.downcast_ref::<NSButton>()) else { return };
                if let Some(app) = self.ivars().app.upgrade() {
                    app.shortcut_button(b.tag());
                }
            });
        }

        // ---- history ----
        #[unsafe(method(historyClicked:))]
        fn history_clicked(&self, sender: Option<&AnyObject>) {
            ffi_guard("historyClicked:", (), || {
                let Some(table) = sender.and_then(|s| s.downcast_ref::<NSTableView>()) else { return };
                let Ok(row) = usize::try_from(table.clickedRow()) else { return };
                if let Some(app) = self.ivars().app.upgrade() {
                    app.history_row_clicked(row);
                }
            });
        }

        // ---- preview ----
        /// Play/Pause, Previous move, Next move (tags 0, -1, 1) and the travel checkbox (tag 9).
        #[unsafe(method(previewButton:))]
        fn preview_button(&self, sender: Option<&AnyObject>) {
            ffi_guard("previewButton:", (), || {
                let Some(b) = sender.and_then(|s| s.downcast_ref::<NSButton>()) else { return };
                let on = b.state() == NSControlStateValueOn;
                if let Some(app) = self.ivars().app.upgrade() {
                    app.preview_button(b.tag(), on);
                }
            });
        }

        #[unsafe(method(previewSeek:))]
        fn preview_seek(&self, sender: Option<&AnyObject>) {
            ffi_guard("previewSeek:", (), || {
                let Some(slider) = sender.and_then(|s| s.downcast_ref::<objc2_app_kit::NSSlider>()) else { return };
                if let Some(app) = self.ivars().app.upgrade() {
                    app.preview_seek(slider.doubleValue());
                }
            });
        }

        #[unsafe(method(previewSpeed:))]
        fn preview_speed(&self, sender: Option<&AnyObject>) {
            ffi_guard("previewSpeed:", (), || {
                let Some(p) = sender.and_then(|s| s.downcast_ref::<NSPopUpButton>()) else { return };
                let Ok(index) = usize::try_from(p.indexOfSelectedItem()) else { return };
                if let Some(app) = self.ivars().app.upgrade() {
                    app.preview_speed(index);
                }
            });
        }

        // ---- RD inspector and support dialog ----
        #[unsafe(method(rdRefresh:))]
        fn rd_refresh(&self, _sender: Option<&AnyObject>) {
            ffi_guard("rdRefresh:", (), || {
                if let Some(app) = self.ivars().app.upgrade() {
                    app.rd_refresh();
                }
            });
        }

        #[unsafe(method(supportOption:))]
        fn support_option(&self, _sender: Option<&AnyObject>) {
            ffi_guard("supportOption:", (), || {
                if let Some(app) = self.ivars().app.upgrade() {
                    app.support_option_changed();
                }
            });
        }
    }

    unsafe impl NSObjectProtocol for AppPanelTarget {}

    unsafe impl NSTableViewDataSource for AppPanelTarget {
        #[unsafe(method(numberOfRowsInTableView:))]
        fn number_of_rows(&self, table_view: &NSTableView) -> NSInteger {
            ffi_guard("numberOfRowsInTableView:", 0, || {
                let Ok(data) = self.ivars().data.try_borrow() else { return 0 };
                (match table_view.tag() {
                    HISTORY_TABLE => data.history.len(),
                    SHORTCUT_TABLE => data.shortcuts.len(),
                    RD_TABLE => data.rd.len(),
                    _ => 0,
                }) as NSInteger
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
                let column = table_column?.identifier().to_string();
                let data = self.ivars().data.try_borrow().ok()?;
                let text = match table_view.tag() {
                    HISTORY_TABLE => data.history.get(row)?.text.clone(),
                    SHORTCUT_TABLE => {
                        let line = data.shortcuts.get(row)?;
                        match column.as_str() {
                            "key" => line.key_text.clone(),
                            "default" => line.default_text.clone(),
                            _ => line.name.clone(),
                        }
                    }
                    RD_TABLE => {
                        let cells = data.rd.get(row)?;
                        let index = ["offset", "bytes", "name", "decoded"].iter().position(|c| *c == column)?;
                        cells[index].clone()
                    }
                    _ => return None,
                };
                Some(Retained::into_super(Retained::into_super(NSString::from_str(&text))))
            })
        }
    }

    unsafe impl NSControlTextEditingDelegate for AppPanelTarget {}

    unsafe impl NSTableViewDelegate for AppPanelTarget {
        #[unsafe(method(tableViewSelectionDidChange:))]
        fn selection_changed(&self, notification: &NSNotification) {
            ffi_guard("tableViewSelectionDidChange:", (), || {
                let object = notification.object();
                let Some(table) = object.as_deref().and_then(|o| o.downcast_ref::<NSTableView>()) else { return };
                if table.tag() == SHORTCUT_TABLE
                    && let Some(app) = self.ivars().app.upgrade()
                {
                    app.shortcut_selection_changed();
                }
            });
        }
    }
);

impl AppPanelTarget {
    pub(crate) fn new(mtm: MainThreadMarker, app: Weak<HostApp>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(TargetIvars { app, data: RefCell::new(TableData::default()) });
        // SAFETY: NSObject's designated initialiser on a freshly allocated instance with ivars set.
        unsafe { msg_send![super(this), init] }
    }

    /// The rows the tables show (the host writes them, then reloads the table).
    pub(crate) fn ivars_data(&self) -> &RefCell<TableData> {
        &self.ivars().data
    }

    fn send(&self, event: AppEvent) {
        if let Some(app) = self.ivars().app.upgrade() {
            app.native(crate::adapter::NativeEvent::App(event));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The key recorder: a view that, once armed, turns the next key combination into a binding.
// ---------------------------------------------------------------------------------------------

pub(crate) struct RecorderIvars {
    app: Weak<HostApp>,
    armed: Cell<bool>,
}

define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnKeyRecorder"]
    #[ivars = RecorderIvars]
    pub(crate) struct KeyRecorder;

    impl KeyRecorder {
        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        /// While armed every combination is ours, so Cmd shortcuts reach the recorder before the
        /// menu bar does.
        #[unsafe(method(performKeyEquivalent:))]
        fn perform_key_equivalent(&self, event: &NSEvent) -> bool {
            ffi_guard("performKeyEquivalent:", false, || self.capture(event))
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            ffi_guard("keyDown:", (), || {
                if !self.capture(event) {
                    // SAFETY: NSResponder's own implementation (passes the key up the chain).
                    let () = unsafe { msg_send![super(self), keyDown: event] };
                }
            });
        }
    }

    unsafe impl NSObjectProtocol for KeyRecorder {}
);

impl KeyRecorder {
    pub(crate) fn new(mtm: MainThreadMarker, app: Weak<HostApp>, frame: NSRect) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(RecorderIvars { app, armed: Cell::new(false) });
        // SAFETY: NSView's designated initialiser on a freshly allocated instance with ivars set.
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }

    pub(crate) fn set_armed(&self, armed: bool) {
        self.ivars().armed.set(armed);
    }

    /// True when the event was taken (armed). Escape without modifiers disarms without a binding.
    fn capture(&self, event: &NSEvent) -> bool {
        if !self.ivars().armed.get() {
            return false;
        }
        let chars = event.charactersIgnoringModifiers().map(|s| s.to_string()).unwrap_or_default();
        let binding = binding_from_event(&chars, event.keyCode(), event.modifierFlags().0);
        if let Some(app) = self.ivars().app.upgrade() {
            app.shortcut_recorded(binding);
        }
        true
    }
}

// ---------------------------------------------------------------------------------------------
// The preview image: a click is a pick at that position.
// ---------------------------------------------------------------------------------------------

pub(crate) struct PreviewIvars {
    app: Weak<HostApp>,
}

define_class!(
    #[unsafe(super(NSImageView))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnPreviewView"]
    #[ivars = PreviewIvars]
    pub(crate) struct PreviewView;

    impl PreviewView {
        /// The first click on an inactive window already picks.
        #[unsafe(method(acceptsFirstMouse:))]
        fn accepts_first_mouse(&self, _event: Option<&NSEvent>) -> bool {
            true
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            ffi_guard("mouseDown:", (), || {
                let at: NSPoint = self.convertPoint_fromView(event.locationInWindow(), None);
                let height = self.bounds().size.height;
                // The image fills the view one to one; pixel rows count from the top.
                if let Some(app) = self.ivars().app.upgrade() {
                    app.preview_clicked([at.x, height - at.y]);
                }
            });
        }
    }

    unsafe impl NSObjectProtocol for PreviewView {}
);

impl PreviewView {
    pub(crate) fn new(mtm: MainThreadMarker, app: Weak<HostApp>, frame: NSRect) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(PreviewIvars { app });
        // SAFETY: NSImageView's designated initialiser on a freshly allocated instance with ivars set.
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }
}
