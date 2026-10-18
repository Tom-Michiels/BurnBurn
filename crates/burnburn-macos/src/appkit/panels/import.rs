//! The import sheet: a sheet on the document window while presentation shows an import preview.

use crate::appkit::panels::widgets::{Column, button, rect, set_text};
use crate::appkit::window::DocWindow;

use burnburn_presentation::{AppViewModel, CommandKey};
use burnburn_types::DocumentId;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly, sel};
use objc2_app_kit::{
    NSAccessibility, NSBackingStoreType, NSButton, NSFont, NSLineBreakMode, NSPopUpButton, NSTextField, NSView,
    NSWindow, NSWindowStyleMask,
};
use objc2_foundation::NSString;

use crate::adapter::{MenuItem, menu_tag};
use crate::l10n::{HostText, Locale, host_text};
use crate::panels::import_panel;

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

/// The import dialog's sheet window and widgets (not shown until presentation has a preview).
#[allow(unsafe_code)]
pub(crate) fn import_sheet(mtm: MainThreadMarker, target: &AnyObject, locale: Locale) -> ImportSheet {
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

impl DocWindow {
    /// Show, update or end the import sheet as presentation's import preview comes and goes.
    pub(crate) fn refresh_import(&mut self, vm: &AppViewModel, document: DocumentId) {
        let Some(panel) = import_panel(vm, document, self.locale) else {
            if self.import.shown {
                self.import.shown = false;
                let (window, sheet) = (self.window.clone(), self.import.sheet.clone());
                if let Err(e) = crate::appkit::objc_try("ending the import sheet", || window.endSheet(&sheet)) {
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
            match crate::appkit::objc_try("showing the import sheet", || {
                window.beginSheet_completionHandler(&sheet, None)
            }) {
                Ok(()) => self.import.shown = true,
                Err(e) => eprintln!("burnburn: {e}"),
            }
        }
    }
}
