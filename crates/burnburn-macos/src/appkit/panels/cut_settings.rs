//! The Cut Settings sheet of one layer.

use crate::appkit::panels::widgets::{field, label, push_button, rect, set_text};
use crate::appkit::window::DocWindow;

use burnburn_presentation::AppViewModel;
use burnburn_types::{DocumentId, OperationId};
use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAccessibility, NSBackingStoreType, NSButton, NSColor, NSControlStateValueOff, NSControlStateValueOn, NSFont,
    NSLineBreakMode, NSPopUpButton, NSTextField, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::NSString;

use crate::l10n::{HostText, Locale, host_text};
use crate::panels::cut_settings_panel;

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

/// The Cut Settings sheet (targets are set once the controller exists).
#[allow(unsafe_code)]
pub(crate) fn cut_settings_sheet(mtm: MainThreadMarker, locale: Locale) -> CutSettingsSheet {
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

impl DocWindow {
    /// Show, fill or end the Cut Settings sheet as presentation opens and closes it. The fields
    /// are filled once per opening, so a refused OK keeps what the user typed.
    pub(crate) fn refresh_cut_settings(&mut self, vm: &AppViewModel, document: DocumentId) {
        let c = &mut self.cut_settings;
        let Some(panel) = cut_settings_panel(vm, document, self.locale) else {
            if c.shown.take().is_some() {
                let (window, sheet) = (self.window.clone(), c.sheet.clone());
                if let Err(e) = crate::appkit::objc_try("ending the Cut Settings sheet", || window.endSheet(&sheet)) {
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
            if let Err(e) = crate::appkit::objc_try("showing the Cut Settings sheet", || {
                window.beginSheet_completionHandler(&sheet, None)
            }) {
                eprintln!("burnburn: {e}");
                c.shown = None;
            }
        }
    }
}
