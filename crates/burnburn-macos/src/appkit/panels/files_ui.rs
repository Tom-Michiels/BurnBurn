//! Building blocks of the file panels (W3-UI-bestanden): one action target for every control of
//! the Material and Art Library panels, the bundle sheet and the export dialog; pop-up, check box
//! and report-text helpers; and the open/save panels with their completion handlers.
//!
//! Controls carry no logic. Each is wired to a closure that reads the widget values it needs and
//! calls one of the headless intent builders in `crate::panels_files`; nothing is parsed here
//! (DR-05) and nothing talks to the document except through `HostApp::dispatch`.
#![allow(unsafe_code)] // Objective-C class definition and control wiring (SAFETY below)

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::{Rc, Weak};

use block2::RcBlock;
use burnburn_presentation::AppViewModel;
use burnburn_types::DocumentId;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSButton, NSControl, NSControlStateValueMixed, NSControlStateValueOff, NSControlStateValueOn, NSModalResponse,
    NSOpenPanel, NSPopUpButton, NSSavePanel, NSScrollView, NSTextView, NSView, NSWindow,
};
use objc2_foundation::{NSArray, NSRect, NSString};

use crate::appkit::app::HostApp;
use crate::appkit::classes::ffi_guard;
use crate::panels_files::{CheckState, Popup};

type Action = Rc<dyn Fn()>;

pub(crate) struct ActionTable {
    actions: RefCell<Vec<Action>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BurnBurnFilesTarget"]
    #[ivars = ActionTable]
    pub(crate) struct FilesTarget;

    impl FilesTarget {
        /// Every control of the file panels: its tag is the index of its closure.
        #[unsafe(method(filesAction:))]
        fn files_action(&self, sender: Option<&AnyObject>) {
            ffi_guard("filesAction:", (), || {
                let Some(control) = sender.and_then(|s| s.downcast_ref::<NSControl>()) else { return };
                let Ok(index) = usize::try_from(control.tag()) else { return };
                // Clone the closure out of the table: it dispatches an intent, which refreshes the
                // panels, which may wire new controls.
                let action = self.ivars().actions.borrow().get(index).cloned();
                if let Some(action) = action {
                    action();
                }
            });
        }
    }

    unsafe impl NSObjectProtocol for FilesTarget {}
);

impl FilesTarget {
    pub(crate) fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ActionTable { actions: RefCell::new(Vec::new()) });
        // SAFETY: NSObject's designated initialiser on a freshly allocated instance with ivars set.
        unsafe { msg_send![super(this), init] }
    }

    /// Run `action` when `control` is used (clicked, or its text committed).
    pub(crate) fn wire(&self, control: &NSControl, action: impl Fn() + 'static) {
        let mut actions = self.ivars().actions.borrow_mut();
        let Ok(tag) = isize::try_from(actions.len()) else { return };
        actions.push(Rc::new(action));
        control.setTag(tag);
        // SAFETY: the target is weak; the window's `FilesSection` keeps it alive while the control
        // exists and clears the target in `clear_targets`. `filesAction:` takes the sender (`v@:@`).
        unsafe { control.setTarget(Some(self.as_ref())) };
        // SAFETY: as above.
        unsafe { control.setAction(Some(sel!(filesAction:))) };
    }
}

/// Point a control at nothing (before the target goes).
pub(crate) fn unwire(control: &NSControl) {
    // SAFETY: a nil target cannot dangle.
    unsafe { control.setTarget(None) };
}

/// What a closure needs to send an intent: the app, this window's document, the current view.
pub(crate) struct Link {
    pub(crate) app: Weak<HostApp>,
    pub(crate) document: DocumentId,
}

impl Link {
    pub(crate) fn run(&self, f: impl FnOnce(&HostApp, DocumentId, &AppViewModel)) {
        if let Some(app) = self.app.upgrade() {
            let vm = app.view();
            f(&app, self.document, &vm);
        }
    }
}

/// Show `model` in `popup`: the items when they changed, the selection when asked to (a pop-up
/// whose choice only the widget knows keeps what the user picked).
pub(crate) fn set_popup(popup: &NSPopUpButton, model: &Popup, select: bool) {
    let current: Vec<String> = popup.itemTitles().iter().map(|t| t.to_string()).collect();
    let changed = current != model.items;
    // Rebuilding the items drops the selection; a pop-up the user drives keeps their pick when a
    // title merely changed (a layer whose origin text changes after Bijwerken).
    let picked = popup.indexOfSelectedItem();
    if changed {
        popup.removeAllItems();
        for title in &model.items {
            popup.addItemWithTitle(&NSString::from_str(title));
        }
    }
    let kept = (!select && changed && picked >= 0 && usize::try_from(picked).is_ok_and(|i| i < model.items.len()))
        .then_some(picked);
    let wanted = kept.unwrap_or_else(|| model.selected.and_then(|i| isize::try_from(i).ok()).unwrap_or(-1));
    if (select || changed) && popup.indexOfSelectedItem() != wanted {
        // An index beyond the items raises NSRangeException: the model never holds one, but a
        // stale value must not abort the app.
        let _ = crate::appkit::objc_try("selecting a pop-up item", || popup.selectItemAtIndex(wanted));
    }
}

/// A two-state check box.
pub(crate) fn checkbox(mtm: MainThreadMarker, title: &str, frame: NSRect) -> Retained<NSButton> {
    // SAFETY: no target and no action yet: clicking does nothing until `FilesTarget::wire`.
    let b = unsafe { NSButton::checkboxWithTitle_target_action(&NSString::from_str(title), None, None, mtm) };
    b.setFrame(frame);
    b
}

pub(crate) fn set_check(button: &NSButton, state: CheckState) {
    let wanted = match state {
        CheckState::Off => NSControlStateValueOff,
        CheckState::Mixed => NSControlStateValueMixed,
        CheckState::On => NSControlStateValueOn,
    };
    if button.state() != wanted {
        button.setState(wanted);
    }
}

/// A check box that can show a mixed state (some of a kind chosen). A click never decides the
/// state: the intent follows the view model, and the next refresh shows the result.
pub(crate) fn tri_state_checkbox(mtm: MainThreadMarker, frame: NSRect) -> Retained<NSButton> {
    let b = checkbox(mtm, "", frame);
    b.setAllowsMixedState(true);
    b
}

pub(crate) fn is_on(button: &NSButton) -> bool {
    button.state() == NSControlStateValueOn
}

/// A read-only multi-line text with a scroll bar, for reports.
pub(crate) fn report_view(mtm: MainThreadMarker, content: &NSView, frame: NSRect) -> Retained<NSTextView> {
    let scroll: Retained<NSScrollView> = NSTextView::scrollableTextView(mtm);
    scroll.setFrame(frame);
    scroll.setHasVerticalScroller(true);
    let text = scroll
        .documentView()
        .and_then(|v| Retained::downcast::<NSTextView>(v).ok())
        .unwrap_or_else(|| NSTextView::initWithFrame(NSTextView::alloc(mtm), frame));
    text.setEditable(false);
    text.setSelectable(true);
    content.addSubview(&scroll);
    text
}

pub(crate) fn set_report(view: &NSTextView, text: &str) {
    if view.string().to_string() != text {
        view.setString(&NSString::from_str(text));
    }
}

// ---------------------------------------------------------------------------------------------
// Open and save panels.
// ---------------------------------------------------------------------------------------------

fn run_panel(what: &str, begin: impl FnOnce()) {
    if let Err(e) = crate::appkit::objc_try(what, begin) {
        eprintln!("burnburn: {e}");
    }
}

/// Ask for one file of `extensions`; `done` gets the path on OK. As a sheet on `parent` when given.
pub(crate) fn open_file(
    mtm: MainThreadMarker,
    parent: Option<&NSWindow>,
    title: &str,
    extensions: &[&str],
    what: &'static str,
    done: impl Fn(Option<PathBuf>) + 'static,
) {
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseFiles(true);
    panel.setCanChooseDirectories(false);
    panel.setAllowsMultipleSelection(false);
    panel.setMessage(Some(&NSString::from_str(title)));
    #[allow(deprecated)]
    panel.setAllowedFileTypes(Some(&NSArray::from_retained_slice(
        &extensions.iter().map(|e| NSString::from_str(e)).collect::<Vec<_>>(),
    )));
    let reader = panel.clone();
    let handler = RcBlock::new(move |response: NSModalResponse| {
        ffi_guard(what, (), || {
            let path = (response == crate::adapter::response::OK)
                .then(|| reader.URLs().firstObject())
                .flatten()
                .and_then(|u| u.to_file_path());
            done(path);
        });
    });
    match parent {
        Some(window) => run_panel(what, || panel.beginSheetModalForWindow_completionHandler(window, &handler)),
        None => run_panel(what, || panel.beginWithCompletionHandler(&handler)),
    }
}

/// Ask where to write a file with `extension`; `done` gets the path and the accessory pop-up's item
/// (−1 without an accessory) on OK.
pub(crate) fn save_file(
    mtm: MainThreadMarker,
    parent: Option<&NSWindow>,
    title: &str,
    suggested: &str,
    extension: &str,
    accessory: Option<(&str, &[String])>,
    what: &'static str,
    done: impl Fn(Option<PathBuf>, isize) + 'static,
) {
    let panel = NSSavePanel::savePanel(mtm);
    panel.setMessage(Some(&NSString::from_str(title)));
    panel.setNameFieldStringValue(&NSString::from_str(suggested));
    #[allow(deprecated)]
    panel.setAllowedFileTypes(Some(&NSArray::from_retained_slice(&[NSString::from_str(extension)])));
    let popup = accessory.map(|(label, items)| {
        let width = 360.0;
        let view =
            NSView::initWithFrame(NSView::alloc(mtm), crate::appkit::panels::widgets::rect(0.0, 0.0, width, 36.0));
        let caption = crate::appkit::panels::widgets::label(
            mtm,
            label,
            crate::appkit::panels::widgets::rect(8.0, 8.0, 70.0, 20.0),
        );
        view.addSubview(&caption);
        let popup = NSPopUpButton::initWithFrame_pullsDown(
            NSPopUpButton::alloc(mtm),
            crate::appkit::panels::widgets::rect(82.0, 4.0, width - 90.0, 26.0),
            false,
        );
        for item in items {
            popup.addItemWithTitle(&NSString::from_str(item));
        }
        view.addSubview(&popup);
        panel.setAccessoryView(Some(&view));
        popup
    });
    let reader = panel.clone();
    let handler = RcBlock::new(move |response: NSModalResponse| {
        ffi_guard(what, (), || {
            let path = (response == crate::adapter::response::OK)
                .then(|| reader.URL())
                .flatten()
                .and_then(|u| u.to_file_path());
            done(path, popup.as_ref().map_or(-1, |p| p.indexOfSelectedItem()));
        });
    });
    match parent {
        Some(window) => run_panel(what, || panel.beginSheetModalForWindow_completionHandler(window, &handler)),
        None => run_panel(what, || panel.beginWithCompletionHandler(&handler)),
    }
}
